use crate::src::cmd::parse::cmd_parse_from_argv;
use crate::src::cmd::{cmd_list_any_have, cmd_pack_argv};
use crate::src::compat::imsg::imsg;
use crate::src::compat::imsg::msg_command;
use crate::src::compat::imsg::*;
use crate::src::compat::imsg::{IMSG_HEADER_SIZE, MAX_IMSGSIZE, PROTOCOL_VERSION};
#[cfg(target_os = "linux")]
use crate::src::compat::systemd::systemd_activated;
use crate::src::control::control_wait_exit;
use crate::src::ffi::libc::{__errno_location, environ, stderr, stdout};
use crate::src::file::{
    file_read_cancel, file_read_open, file_write_close, file_write_data, file_write_left,
    file_write_open,
};
use crate::src::log::{fatal, fatalx, log_cstr, log_debug, log_hex};
use crate::src::proc::{
    bytes_of, proc_add_peer, proc_clear_signals, proc_exit, proc_loop, proc_send, proc_set_signals,
    proc_start,
};
use crate::src::server::server_start;
use crate::src::shared::abi::*;
use crate::src::shared::abi::{ssize_t, uint32_t};
use crate::src::shared::client::*;
pub use crate::src::shared::client::{client, client_files, overlay_mode_cb};
use crate::src::shared::client::{
    CLIENT_CONTROL, CLIENT_CONTROLCONTROL, CLIENT_CONTROL_WAITEXIT, CLIENT_LOGIN,
    CLIENT_NOSTARTSERVER, CLIENT_STARTSERVER, CLIENT_WRITE_ACK,
};
use crate::src::shared::command::cmd_parse_result;
use crate::src::shared::command::*;
use crate::src::shared::errno::{EAGAIN, ECHILD, ENAMETOOLONG, ENOENT};
use crate::src::shared::posix_io::{STDERR_FILENO, STDIN_FILENO, STDOUT_FILENO, WAIT_ANY, WNOHANG};
use crate::src::shared::posix_terminal::{ICRNL, ONLCR, OPOST, TCSANOW, VMIN, VTIME};
use crate::src::shared::process::{tmuxpeer, tmuxproc};
pub use crate::src::shared::signal::{
    ProcessSignal, SA_RESTART, SIGCHLD, SIGCONT, SIGHUP, SIGTERM, SIGTSTP, SIGWINCH, SIG_DFL,
    SIG_IGN,
};
use crate::src::shared::terminal::*;
use crate::src::tmux::{
    find_cwd, find_home_cstr, global_environ, ptm_fd, setblocking, shell_argv0_cstring,
    shell_command, socket_path_cstr,
};
use crate::src::tty_term::tty_term_read_list;
use libc::{
    cfgetispeed, cfgetospeed, cfmakeraw, cfsetispeed, cfsetospeed, execl, fflush, fprintf, getenv,
    getpid, getppid, kill, memset, printf, setenv, sigaction, sigemptyset, strerror, strlen,
    strsignal, system, unlink, waitpid,
};
use std::ffi::{CStr, CString, OsStr};
use std::io;
use std::os::fd::{AsFd, AsRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use tiri_rt::Runtime as _;

pub use ::libc::{CREAD, CS8, ECONNREFUSED, HUPCL, IXANY, LOCK_EX, LOCK_NB, TCSAFLUSH};

static mut client_proc: *mut tmuxproc = ::core::ptr::null::<tmuxproc>() as *mut tmuxproc;
static mut client_peer: *mut tmuxpeer = ::core::ptr::null::<tmuxpeer>() as *mut tmuxpeer;
static mut client_flags: uint64_t = 0;
static mut client_suspended: ::core::ffi::c_int = 0;
static mut client_exitreason: client_exit_reason = CLIENT_EXIT_NONE;
static mut client_exitflag: ::core::ffi::c_int = 0;
static mut client_exitval: ::core::ffi::c_int = 0;
static mut client_exittype: msgtype = 0 as msgtype;
static mut client_exitsession: Option<CString> = None;
static mut client_exitmessage: Option<Vec<u8>> = None;
static mut client_exec_payload: Option<(CString, CString)> = None;
static mut client_attached: ::core::ffi::c_int = 0;
static mut client_files: client_files = client_files::new();

pub(crate) unsafe fn client_find_file(
    stream: i32,
) -> Option<std::rc::Rc<std::cell::UnsafeCell<crate::src::file::client_file>>> {
    crate::src::file::client_files_find_stream(&client_files, stream)
}

pub(crate) unsafe fn client_register_file(
    file: &std::rc::Rc<std::cell::UnsafeCell<crate::src::file::client_file>>,
) {
    crate::src::file::client_files_insert(&mut client_files, file.clone());
}

pub(crate) unsafe fn client_remove_file(
    stream: i32,
    identity: *const crate::src::file::client_file,
) {
    let removed =
        crate::src::file::client_files_remove_identity(&mut client_files, stream, identity);
    drop(removed);
}
enum ClientLock {
    Acquired(OwnedFd),
    Retry,
    Unavailable,
}

unsafe fn client_get_lock(lockfile: &CStr) -> ClientLock {
    log_debug(format_args!("lock file is {}", log_cstr(lockfile)));
    let lock = match tiri_rt::unix::open(lockfile, libc::O_WRONLY | libc::O_CREAT, 0o600) {
        Ok(file) => file,
        Err(error) => {
            log_debug(format_args!("open failed: {error}"));
            return ClientLock::Unavailable;
        }
    };
    if let Err(error) = tiri_rt::unix::lock(lock.as_fd(), LOCK_EX | LOCK_NB) {
        log_debug(format_args!(
            "flock failed: {}",
            log_cstr(CStr::from_ptr(strerror(*__errno_location())))
        ));
        if error.raw_os_error() != Some(EAGAIN) {
            return ClientLock::Acquired(lock);
        }
        while tiri_rt::unix::lock(lock.as_fd(), LOCK_EX)
            .is_err_and(|error| error.kind() == io::ErrorKind::Interrupted)
        {}
        return ClientLock::Retry;
    }
    log_debug(format_args!("flock succeeded"));
    ClientLock::Acquired(lock)
}

unsafe fn client_connect(path: &CStr, flags: uint64_t) -> io::Result<OwnedFd> {
    let connect_path = OsStr::from_bytes(path.to_bytes());
    if connect_path.as_bytes().len() >= 108 {
        return Err(io::Error::from_raw_os_error(ENAMETOOLONG));
    }
    let mut lockfd = None;
    let mut locked = false;
    let mut lockfile = None;
    log_debug(format_args!("socket is {}", log_cstr(path)));
    loop {
        log_debug(format_args!("trying connect"));
        let error = match tiri_rt::unix::connect(std::path::Path::new(connect_path)) {
            Ok(socket) => {
                drop(lockfd);
                setblocking(socket.as_raw_fd(), 0);
                return Ok(socket);
            }
            Err(error) => error,
        };
        log_debug(format_args!("connect failed: {error}"));
        if !matches!(error.raw_os_error(), Some(ECONNREFUSED | ENOENT))
            || flags & CLIENT_NOSTARTSERVER as uint64_t != 0
            || flags & CLIENT_STARTSERVER as uint64_t == 0
        {
            return Err(error);
        }
        if !locked {
            let mut name = path.to_bytes().to_vec();
            name.extend_from_slice(b".lock");
            lockfile = Some(CString::new(name).expect("socket path has no interior NUL"));
            match client_get_lock(lockfile.as_ref().unwrap()) {
                ClientLock::Acquired(fd) => lockfd = Some(fd),
                ClientLock::Retry => {
                    lockfile = None;
                    continue;
                }
                ClientLock::Unavailable => lockfile = None,
            }
            log_debug(format_args!(
                "got lock ({})",
                lockfd.as_ref().map_or(-1, AsRawFd::as_raw_fd)
            ));
            locked = true;
        } else {
            if lockfd.is_some() && unlink(path.as_ptr()) != 0 && *__errno_location() != ENOENT {
                return Err(io::Error::last_os_error());
            }
            // After fork each process releases its own copy of the lock.
            let fd = server_start(flags, &mut lockfd, &mut lockfile);
            drop(lockfd);
            setblocking(fd.as_raw_fd(), 0);
            return Ok(fd);
        }
    }
}
unsafe fn client_exit_message() -> CString {
    let detached = |prefix: &[u8], plain: &CStr| match client_exitsession.as_ref() {
        Some(session) => CString::new([prefix, session.as_bytes(), b")"].concat())
            .expect("session name contains no NUL"),
        None => plain.to_owned(),
    };
    match client_exitreason as ::core::ffi::c_uint {
        1 => detached(b"detached (from session ", c"detached"),
        2 => detached(
            b"detached and SIGHUP (from session ",
            c"detached and SIGHUP",
        ),
        3 => c"lost tty".to_owned(),
        4 => c"terminated".to_owned(),
        5 => c"server exited unexpectedly".to_owned(),
        6 => c"exited".to_owned(),
        7 => c"server exited".to_owned(),
        8 => client_exitmessage
            .as_ref()
            .and_then(|message| CStr::from_bytes_until_nul(message).ok())
            .expect("exit message is NUL-terminated")
            .to_owned(),
        _ => c"unknown reason".to_owned(),
    }
}
unsafe fn client_exit() {
    if file_write_left(&client_files) == 0 {
        proc_exit(client_proc);
    }
}
pub unsafe fn client_main(
    argv: &[CString],
    mut flags: uint64_t,
    mut feat: ::core::ffi::c_int,
) -> ::core::ffi::c_int {
    let mut pr: cmd_parse_result = cmd_parse_result::empty();
    let mut i: ::core::ffi::c_int = 0;
    let mut termname: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    let mut cwd: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    let mut cwd_owner: Option<CString> = None;
    let mut ppid: pid_t = 0;
    let mut msg: msgtype = 0 as msgtype;
    let mut tio: termios = unsafe { ::core::mem::zeroed() };
    let mut saved_tio: termios = unsafe { ::core::mem::zeroed() };
    let mut size: size_t = 0;
    let mut caps = Vec::new();
    if shell_command.is_some() {
        msg = MSG_SHELL;
        flags |= CLIENT_STARTSERVER as uint64_t;
    } else if argv.is_empty() {
        msg = MSG_COMMAND;
        flags |= CLIENT_STARTSERVER as uint64_t;
    } else {
        msg = MSG_COMMAND;
        pr = cmd_parse_from_argv(argv);
        if pr.status as ::core::ffi::c_uint
            == CMD_PARSE_SUCCESS as ::core::ffi::c_int as ::core::ffi::c_uint
        {
            if cmd_list_any_have(&pr.cmdlist.as_ref().expect("parsed command list").borrow()) != 0 {
                flags |= CLIENT_STARTSERVER as uint64_t;
            }
            drop(pr.cmdlist.take());
        }
    }
    let mut process_owner = proc_start(c"client");
    client_proc = &raw mut *process_owner;
    // All ordinary returns, including startup failures, share explicit cleanup.
    let result = (|| {
        client_flags = (flags as ::core::ffi::c_ulonglong | CLIENT_WRITE_ACK) as uint64_t;
        log_debug(format_args!(
            "flags are {}",
            log_hex(client_flags as ::core::ffi::c_ulonglong)
        ));
        // Socket activation is only available through systemd.
        #[cfg(target_os = "linux")]
        let activated = systemd_activated() != 0;
        #[cfg(not(target_os = "linux"))]
        let activated = false;
        let connection = if activated {
            Ok(server_start(flags, &mut None, &mut None))
        } else {
            client_connect(socket_path_cstr(), client_flags)
        };
        let fd = match connection {
            Ok(fd) => fd,
            Err(error) => {
                if error.raw_os_error() == Some(ECONNREFUSED) {
                    fprintf(
                        stderr,
                        c"no server running on %s\n".as_ptr(),
                        socket_path_cstr().as_ptr(),
                    );
                } else {
                    fprintf(
                        stderr,
                        c"error connecting to %s (%s)\n".as_ptr(),
                        socket_path_cstr().as_ptr(),
                        strerror(error.raw_os_error().unwrap_or(::libc::EINVAL)),
                    );
                }
                return 1;
            }
        };
        // Connecting may daemonize a new server. Initialize each process's
        // runtime only after that fork.
        let runtime = tiri_rt::mio::Runtime::new().expect("tiri-rt initialization");
        proc_set_signals(
            client_proc,
            Some(Box::new(|sig| unsafe { client_signal(sig) })),
        );
        // The daemonization child may have exited before SIGCHLD was installed.
        client_signal(ProcessSignal::Child);
        client_peer = proc_add_peer(
            client_proc,
            fd,
            Box::new(|message| unsafe { client_dispatch(message) }),
        );
        cwd = cwd_owner
            .insert(find_cwd().unwrap_or_else(|| find_home_cstr().unwrap_or(c"/").to_owned()))
            .as_ptr();
        let ttynam =
            tiri_rt::unix::terminal_name(std::os::fd::BorrowedFd::borrow_raw(STDIN_FILENO))
                .unwrap_or_default();
        termname = getenv(c"TERM".as_ptr());
        if termname.is_null() {
            termname = c"".as_ptr();
        }
        if 0 as ::core::ffi::c_int != 0 as ::core::ffi::c_int {
            fatal(|out| out.write_all(b"pledge failed"));
        }
        if tiri_rt::unix::terminal_attributes(std::os::fd::BorrowedFd::borrow_raw(STDIN_FILENO))
            .is_ok()
            && *termname as ::core::ffi::c_int != '\0' as i32
        {
            match tty_term_read_list(CStr::from_ptr(termname)) {
                Ok(read_caps) => caps = read_caps,
                Err(cause) => {
                    fprintf(stderr, c"%s\n".as_ptr(), cause.as_ptr());
                    return 1 as ::core::ffi::c_int;
                }
            }
        }
        if ptm_fd != -(1 as ::core::ffi::c_int) {
            let _ = tiri_rt::unix::close(ptm_fd);
        }
        crate::src::tmux::free_global_options();
        drop(global_environ.take());
        if client_flags & CLIENT_CONTROLCONTROL as uint64_t != 0 {
            if crate::src::shared::terminal::read_attributes(STDIN_FILENO, &mut saved_tio)
                != 0 as ::core::ffi::c_int
            {
                fprintf(
                    stderr,
                    c"tcgetattr failed: %s\n".as_ptr(),
                    strerror(*__errno_location()),
                );
                return 1 as ::core::ffi::c_int;
            }
            cfmakeraw(&mut tio);
            tio.c_iflag = (ICRNL | IXANY) as tcflag_t;
            tio.c_oflag = (OPOST | ONLCR) as tcflag_t;
            tio.c_cflag = (CREAD | CS8 | HUPCL) as tcflag_t;
            tio.c_cc[VMIN] = 1 as cc_t;
            tio.c_cc[VTIME] = 0 as cc_t;
            cfsetispeed(&mut tio, cfgetispeed(&saved_tio));
            cfsetospeed(&mut tio, cfgetospeed(&saved_tio));
            crate::src::shared::terminal::set_attributes(STDIN_FILENO, TCSANOW, &tio);
        }
        client_send_identify(
            &ttynam,
            CStr::from_ptr(termname),
            &caps,
            CStr::from_ptr(cwd),
            feat,
        );
        if msg as ::core::ffi::c_uint == MSG_COMMAND as ::core::ffi::c_int as ::core::ffi::c_uint {
            size = 0 as size_t;
            i = 0 as ::core::ffi::c_int;
            while (i as usize) < argv.len() {
                size = size.wrapping_add(argv[i as usize].as_bytes_with_nul().len() as size_t);
                i += 1;
            }
            if size
                > (MAX_IMSGSIZE as usize)
                    .wrapping_sub(::core::mem::size_of::<msg_command>() as usize)
            {
                fprintf(stderr, c"command too long\n".as_ptr());
                return 1 as ::core::ffi::c_int;
            }
            const _: () = assert!(
                ::core::mem::size_of::<msg_command>()
                    == ::core::mem::size_of::<::core::ffi::c_int>()
            );
            let header_size = ::core::mem::size_of::<msg_command>();
            let mut data = vec![0u8; header_size + size];
            let argc = ::core::ffi::c_int::try_from(argv.len()).expect("argv length exceeds c_int");
            data[..header_size].copy_from_slice(&argc.to_ne_bytes());
            if cmd_pack_argv(
                argv,
                data[header_size..].as_mut_ptr() as *mut ::core::ffi::c_char,
                size,
            ) != 0 as ::core::ffi::c_int
            {
                fprintf(stderr, c"command too long\n".as_ptr());
                return 1 as ::core::ffi::c_int;
            }
            if proc_send(client_peer, msg, None, &data) != 0 as ::core::ffi::c_int {
                fprintf(stderr, c"failed to send command\n".as_ptr());
                return 1 as ::core::ffi::c_int;
            }
        } else if msg as ::core::ffi::c_uint
            == MSG_SHELL as ::core::ffi::c_int as ::core::ffi::c_uint
        {
            proc_send(client_peer, msg, None, &[]);
        }
        proc_loop(client_proc, runtime, None);
        if client_exittype as ::core::ffi::c_uint
            == MSG_EXEC as ::core::ffi::c_int as ::core::ffi::c_uint
        {
            if client_flags & CLIENT_CONTROLCONTROL as uint64_t != 0 {
                crate::src::shared::terminal::set_attributes(STDOUT_FILENO, TCSAFLUSH, &saved_tio);
            }
            let (shell, command) = client_exec_payload
                .as_ref()
                .expect("MSG_EXEC requires a stored shell and command");
            client_exec(shell, Some(command));
        }
        if client_attached != 0 {
            if client_exitreason as ::core::ffi::c_uint
                != CLIENT_EXIT_NONE as ::core::ffi::c_int as ::core::ffi::c_uint
            {
                printf(c"[%s]\n".as_ptr(), client_exit_message().as_ptr());
            }
            ppid = getppid() as pid_t;
            if client_exittype as ::core::ffi::c_uint
                == MSG_DETACHKILL as ::core::ffi::c_int as ::core::ffi::c_uint
                && ppid > 1 as ::core::ffi::c_int
            {
                kill(ppid as __pid_t, SIGHUP);
            }
        } else if client_flags & CLIENT_CONTROL as uint64_t != 0 {
            if client_exitreason as ::core::ffi::c_uint
                != CLIENT_EXIT_NONE as ::core::ffi::c_int as ::core::ffi::c_uint
            {
                printf(c"%%exit %s\n".as_ptr(), client_exit_message().as_ptr());
            } else {
                printf(c"%%exit\n".as_ptr());
            }
            fflush(stdout);
            if client_flags as ::core::ffi::c_ulonglong & CLIENT_CONTROL_WAITEXIT != 0 {
                control_wait_exit();
            }
            if client_flags & CLIENT_CONTROLCONTROL as uint64_t != 0 {
                printf(c"\x1B\\".as_ptr());
                fflush(stdout);
                crate::src::shared::terminal::set_attributes(STDOUT_FILENO, TCSAFLUSH, &saved_tio);
            }
        } else if client_exitreason as ::core::ffi::c_uint
            != CLIENT_EXIT_NONE as ::core::ffi::c_int as ::core::ffi::c_uint
        {
            fprintf(stderr, c"%s\n".as_ptr(), client_exit_message().as_ptr());
        }
        setblocking(STDIN_FILENO, 1 as ::core::ffi::c_int);
        setblocking(STDOUT_FILENO, 1 as ::core::ffi::c_int);
        setblocking(STDERR_FILENO, 1 as ::core::ffi::c_int);
        client_exitmessage.take();
        client_exitsession.take();
        client_exitval
    })();
    drop(process_owner);
    client_peer = std::ptr::null_mut();
    client_proc = std::ptr::null_mut();
    result
}
unsafe fn client_send_identify(
    ttynam: &CStr,
    termname: &CStr,
    caps: &[CString],
    cwd: &CStr,
    mut feat: ::core::ffi::c_int,
) {
    let mut ss: *mut *mut ::core::ffi::c_char = ::core::ptr::null_mut::<*mut ::core::ffi::c_char>();
    let mut sslen: size_t = 0;
    let mut flags: uint64_t = client_flags;
    let mut pid: pid_t = 0;
    proc_send(client_peer, MSG_IDENTIFY_LONGFLAGS, None, bytes_of(&flags));
    proc_send(
        client_peer,
        MSG_IDENTIFY_LONGFLAGS,
        None,
        bytes_of(&client_flags),
    );
    proc_send(
        client_peer,
        MSG_IDENTIFY_TERM,
        None,
        termname.to_bytes_with_nul(),
    );
    proc_send(client_peer, MSG_IDENTIFY_FEATURES, None, bytes_of(&feat));
    proc_send(
        client_peer,
        MSG_IDENTIFY_TTYNAME,
        None,
        ttynam.to_bytes_with_nul(),
    );
    proc_send(client_peer, MSG_IDENTIFY_CWD, None, cwd.to_bytes_with_nul());
    for cap in caps {
        proc_send(
            client_peer,
            MSG_IDENTIFY_TERMINFO,
            None,
            cap.as_bytes_with_nul(),
        );
    }
    let fd = io::stdin()
        .as_fd()
        .try_clone_to_owned()
        .unwrap_or_else(|_| fatal(|out| out.write_all(b"dup failed")));
    proc_send(client_peer, MSG_IDENTIFY_STDIN, Some(fd), &[]);
    let fd = io::stdout()
        .as_fd()
        .try_clone_to_owned()
        .unwrap_or_else(|_| fatal(|out| out.write_all(b"dup failed")));
    proc_send(client_peer, MSG_IDENTIFY_STDOUT, Some(fd), &[]);
    pid = getpid() as pid_t;
    proc_send(client_peer, MSG_IDENTIFY_CLIENTPID, None, bytes_of(&pid));
    ss = environ;
    while !(*ss).is_null() {
        sslen = strlen(*ss).wrapping_add(1 as size_t);
        if !(sslen > (MAX_IMSGSIZE as usize).wrapping_sub(IMSG_HEADER_SIZE)) {
            proc_send(
                client_peer,
                MSG_IDENTIFY_ENVIRON,
                None,
                CStr::from_ptr(*ss).to_bytes_with_nul(),
            );
        }
        ss = ss.offset(1);
    }
    proc_send(client_peer, MSG_IDENTIFY_DONE, None, &[]);
}
unsafe fn client_exec(shell: &CStr, shellcmd: Option<&CStr>) -> ! {
    log_debug(format_args!(
        "shell {}, command {}",
        log_cstr(shell),
        log_cstr(CStr::from_ptr(
            shellcmd.map_or(std::ptr::null(), std::ffi::CStr::as_ptr)
        ))
    ));
    let argv0 = shell_argv0_cstring(shell, client_flags & CLIENT_LOGIN as uint64_t != 0);
    setenv(c"SHELL".as_ptr(), shell.as_ptr(), 1 as ::core::ffi::c_int);
    proc_clear_signals(client_proc, 1 as ::core::ffi::c_int);
    setblocking(STDIN_FILENO, 1 as ::core::ffi::c_int);
    setblocking(STDOUT_FILENO, 1 as ::core::ffi::c_int);
    setblocking(STDERR_FILENO, 1 as ::core::ffi::c_int);
    tiri_rt::unix::close_from(STDERR_FILENO + 1 as ::core::ffi::c_int);
    execl(
        shell.as_ptr(),
        argv0.as_ptr(),
        c"-c".as_ptr(),
        shellcmd.map_or(std::ptr::null(), std::ffi::CStr::as_ptr),
        NULL as *mut ::core::ffi::c_char,
    );
    fatal(|out| out.write_all(b"execl failed"));
}
unsafe fn client_signal(sig: ProcessSignal) {
    let mut sigact: sigaction = unsafe { ::core::mem::zeroed() };
    let mut status: ::core::ffi::c_int = 0;
    let mut pid: pid_t = 0;
    log_debug(format_args!(
        "{}: {}",
        "client_signal",
        log_cstr(CStr::from_ptr(strsignal(sig.as_raw())))
    ));
    if sig == ProcessSignal::Child {
        loop {
            pid = waitpid(WAIT_ANY, &raw mut status, WNOHANG) as pid_t;
            if pid == 0 as ::core::ffi::c_int {
                break;
            }
            if !(pid == -(1 as ::core::ffi::c_int)) {
                continue;
            }
            if *__errno_location() == ECHILD {
                break;
            }
            log_debug(format_args!(
                "waitpid failed: {}",
                log_cstr(CStr::from_ptr(strerror(*__errno_location())))
            ));
        }
    } else if client_attached == 0 {
        if sig == ProcessSignal::Terminate || sig == ProcessSignal::Hangup {
            proc_exit(client_proc);
        }
    } else {
        match sig {
            ProcessSignal::Hangup => {
                client_exitreason = CLIENT_EXIT_LOST_TTY;
                client_exitval = 1 as ::core::ffi::c_int;
                proc_send(client_peer, MSG_EXITING, None, &[]);
            }
            ProcessSignal::Terminate => {
                if client_suspended == 0 {
                    client_exitreason = CLIENT_EXIT_TERMINATED;
                }
                client_exitval = 1 as ::core::ffi::c_int;
                proc_send(client_peer, MSG_EXITING, None, &[]);
            }
            ProcessSignal::WindowChange => {
                proc_send(client_peer, MSG_RESIZE, None, &[]);
            }
            ProcessSignal::Continue => {
                memset(
                    &raw mut sigact as *mut ::core::ffi::c_void,
                    0 as ::core::ffi::c_int,
                    ::core::mem::size_of::<sigaction>() as size_t,
                );
                sigemptyset(&raw mut sigact.sa_mask);
                sigact.sa_flags = SA_RESTART;
                sigact.sa_sigaction = SIG_IGN;
                if sigaction(
                    SIGTSTP,
                    &raw mut sigact,
                    ::core::ptr::null_mut::<sigaction>(),
                ) != 0 as ::core::ffi::c_int
                {
                    fatal(|out| out.write_all(b"sigaction failed"));
                }
                proc_send(client_peer, MSG_WAKEUP, None, &[]);
                client_suspended = 0 as ::core::ffi::c_int;
            }
            _ => {}
        }
    };
}
unsafe fn client_file_check_cb() {
    if client_exitflag != 0 {
        client_exit();
    }
}
unsafe fn client_dispatch(message: crate::src::shared::process::PeerMessage<'_>) {
    let imsg = match message {
        crate::src::shared::process::PeerMessage::Disconnected => {
            if client_exitflag == 0 {
                client_exitreason = CLIENT_EXIT_LOST_SERVER;
                client_exitval = 1 as ::core::ffi::c_int;
            }
            proc_exit(client_proc);
            return;
        }
        crate::src::shared::process::PeerMessage::Message(imsg) => imsg,
    };
    if client_attached != 0 {
        client_dispatch_attached(imsg);
    } else {
        client_dispatch_wait(imsg);
    };
}
unsafe fn client_dispatch_exit_message(mut data: *mut ::core::ffi::c_char, mut datalen: size_t) {
    let mut retval: ::core::ffi::c_int = 0;
    if datalen < ::core::mem::size_of::<::core::ffi::c_int>() as usize && datalen != 0 as size_t {
        fatalx(|out| out.write_all(b"bad MSG_EXIT size"));
    }
    if datalen >= ::core::mem::size_of::<::core::ffi::c_int>() as usize {
        retval = std::ptr::read_unaligned(data.cast::<::core::ffi::c_int>());
        client_exitval = retval;
    }
    if datalen > ::core::mem::size_of::<::core::ffi::c_int>() as usize {
        datalen = (datalen as ::core::ffi::c_ulong)
            .wrapping_sub(
                ::core::mem::size_of::<::core::ffi::c_int>() as usize as ::core::ffi::c_ulong
            ) as size_t as size_t;
        data = data.add(::core::mem::size_of::<::core::ffi::c_int>() as usize);
        let mut message = ::core::slice::from_raw_parts(data.cast::<u8>(), datalen).to_vec();
        *message.last_mut().unwrap() = 0;
        client_exitmessage = Some(message);
        client_exitreason = CLIENT_EXIT_MESSAGE_PROVIDED;
    }
}
unsafe fn client_dispatch_wait(imsg: &mut imsg) {
    let mut data: *mut ::core::ffi::c_char = ::core::ptr::null_mut::<::core::ffi::c_char>();
    let mut datalen: ssize_t = 0;
    static mut pledge_applied: ::core::ffi::c_int = 0;
    if pledge_applied == 0 {
        if 0 as ::core::ffi::c_int != 0 as ::core::ffi::c_int {
            fatal(|out| out.write_all(b"pledge failed"));
        }
        pledge_applied = 1 as ::core::ffi::c_int;
    }
    data = imsg.data.as_mut_ptr().cast::<::core::ffi::c_char>();
    datalen = imsg.data.len() as ssize_t;
    match imsg.hdr.type_0 {
        MSG_EXIT | MSG_SHUTDOWN => {
            client_dispatch_exit_message(data, datalen as size_t);
            client_exitflag = 1 as ::core::ffi::c_int;
            client_exit();
        }
        MSG_READY => {
            if datalen != 0 as ssize_t {
                fatalx(|out| out.write_all(b"bad MSG_READY size"));
            }
            client_attached = 1 as ::core::ffi::c_int;
            proc_send(client_peer, MSG_RESIZE, None, &[]);
        }
        MSG_VERSION => {
            if datalen != 0 as ssize_t {
                fatalx(|out| out.write_all(b"bad MSG_VERSION size"));
            }
            fprintf(
                stderr,
                c"protocol version mismatch (client %d, server %u)\n".as_ptr(),
                PROTOCOL_VERSION,
                imsg.hdr.peerid & 0xff as uint32_t,
            );
            client_exitval = 1 as ::core::ffi::c_int;
            proc_exit(client_proc);
        }
        MSG_FLAGS => {
            if datalen as usize != ::core::mem::size_of::<uint64_t>() as usize {
                fatalx(|out| out.write_all(b"bad MSG_FLAGS string"));
            }
            client_flags = std::ptr::read_unaligned(data.cast::<uint64_t>());
            log_debug(format_args!(
                "new flags are {}",
                log_hex(client_flags as ::core::ffi::c_ulonglong)
            ));
        }
        MSG_SHELL => {
            if datalen == 0 as ssize_t
                || *data.offset((datalen - 1 as ssize_t) as isize) as ::core::ffi::c_int
                    != '\0' as i32
            {
                fatalx(|out| out.write_all(b"bad MSG_SHELL string"));
            }
            client_exec(CStr::from_ptr(data), shell_command.as_deref());
        }
        MSG_DETACH | MSG_DETACHKILL => {
            proc_send(client_peer, MSG_EXITING, None, &[]);
        }
        MSG_EXITED => {
            proc_exit(client_proc);
        }
        MSG_READ_OPEN => {
            file_read_open(
                client_peer,
                imsg,
                (client_flags & CLIENT_CONTROL as uint64_t == 0) as ::core::ffi::c_int,
                Some(Box::new(|_| unsafe { client_file_check_cb() })),
            );
        }
        MSG_READ_CANCEL => {
            file_read_cancel(imsg);
        }
        MSG_WRITE_OPEN => {
            file_write_open(
                client_peer,
                imsg,
                (client_flags & CLIENT_CONTROL as uint64_t == 0) as ::core::ffi::c_int,
                Some(Box::new(|_| unsafe { client_file_check_cb() })),
            );
        }
        MSG_WRITE => {
            file_write_data(imsg);
        }
        MSG_WRITE_CLOSE => {
            file_write_close(imsg);
        }
        MSG_STDERR | MSG_STDIN | MSG_STDOUT => {
            fprintf(stderr, c"server version is too old for client\n".as_ptr());
            proc_exit(client_proc);
        }
        _ => {
            log_debug(format_args!("unknown message type {}", { imsg.hdr.type_0 }));
        }
    };
}
unsafe fn client_dispatch_attached(imsg: &mut imsg) {
    let mut sigact: sigaction = unsafe { ::core::mem::zeroed() };
    let mut data: *mut ::core::ffi::c_char = ::core::ptr::null_mut::<::core::ffi::c_char>();
    let mut datalen: ssize_t = 0;
    data = imsg.data.as_mut_ptr().cast::<::core::ffi::c_char>();
    datalen = imsg.data.len() as ssize_t;
    match imsg.hdr.type_0 {
        MSG_FLAGS => {
            if datalen as usize != ::core::mem::size_of::<uint64_t>() as usize {
                fatalx(|out| out.write_all(b"bad MSG_FLAGS string"));
            }
            client_flags = std::ptr::read_unaligned(data.cast::<uint64_t>());
            log_debug(format_args!(
                "new flags are {}",
                log_hex(client_flags as ::core::ffi::c_ulonglong)
            ));
        }
        MSG_DETACH | MSG_DETACHKILL => {
            if datalen == 0 as ssize_t
                || *data.offset((datalen - 1 as ssize_t) as isize) as ::core::ffi::c_int
                    != '\0' as i32
            {
                fatalx(|out| out.write_all(b"bad MSG_DETACH string"));
            }
            client_exitsession = Some(std::ffi::CStr::from_ptr(data).to_owned());
            client_exittype = imsg.hdr.type_0;
            if imsg.hdr.type_0 == MSG_DETACHKILL {
                client_exitreason = CLIENT_EXIT_DETACHED_HUP;
            } else {
                client_exitreason = CLIENT_EXIT_DETACHED;
            }
            proc_send(client_peer, MSG_EXITING, None, &[]);
        }
        MSG_EXEC => {
            if datalen == 0 as ssize_t
                || *data.offset((datalen - 1 as ssize_t) as isize) as ::core::ffi::c_int
                    != '\0' as i32
                || strlen(data).wrapping_add(1 as size_t) == datalen as size_t
            {
                fatalx(|out| out.write_all(b"bad MSG_EXEC string"));
            }
            let command = std::ffi::CStr::from_ptr(data).to_owned();
            let shell = std::ffi::CStr::from_ptr(data.add(strlen(data) + 1)).to_owned();
            client_exec_payload = Some((shell, command));
            client_exittype = imsg.hdr.type_0;
            proc_send(client_peer, MSG_EXITING, None, &[]);
        }
        MSG_EXIT => {
            client_dispatch_exit_message(data, datalen as size_t);
            if client_exitreason as ::core::ffi::c_uint
                == CLIENT_EXIT_NONE as ::core::ffi::c_int as ::core::ffi::c_uint
            {
                client_exitreason = CLIENT_EXIT_EXITED;
            }
            proc_send(client_peer, MSG_EXITING, None, &[]);
        }
        MSG_EXITED => {
            if datalen != 0 as ssize_t {
                fatalx(|out| out.write_all(b"bad MSG_EXITED size"));
            }
            proc_exit(client_proc);
        }
        MSG_SHUTDOWN => {
            if datalen != 0 as ssize_t {
                fatalx(|out| out.write_all(b"bad MSG_SHUTDOWN size"));
            }
            proc_send(client_peer, MSG_EXITING, None, &[]);
            client_exitreason = CLIENT_EXIT_SERVER_EXITED;
            client_exitval = 1 as ::core::ffi::c_int;
        }
        MSG_SUSPEND => {
            if datalen != 0 as ssize_t {
                fatalx(|out| out.write_all(b"bad MSG_SUSPEND size"));
            }
            memset(
                &raw mut sigact as *mut ::core::ffi::c_void,
                0 as ::core::ffi::c_int,
                ::core::mem::size_of::<sigaction>() as size_t,
            );
            sigemptyset(&raw mut sigact.sa_mask);
            sigact.sa_flags = SA_RESTART;
            sigact.sa_sigaction = SIG_DFL;
            if sigaction(
                SIGTSTP,
                &raw mut sigact,
                ::core::ptr::null_mut::<sigaction>(),
            ) != 0 as ::core::ffi::c_int
            {
                fatal(|out| out.write_all(b"sigaction failed"));
            }
            client_suspended = 1 as ::core::ffi::c_int;
            kill(getpid(), SIGTSTP);
        }
        MSG_LOCK => {
            if datalen == 0 as ssize_t
                || *data.offset((datalen - 1 as ssize_t) as isize) as ::core::ffi::c_int
                    != '\0' as i32
            {
                fatalx(|out| out.write_all(b"bad MSG_LOCK string"));
            }
            system(data);
            proc_send(client_peer, MSG_UNLOCK, None, &[]);
        }
        _ => {
            log_debug(format_args!("unknown message type {}", { imsg.hdr.type_0 }));
        }
    };
}
