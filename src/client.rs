use crate::cmd::parse::cmd_parse_from_argv;
use crate::cmd::{cmd_list_any_have, cmd_pack_argv};
use crate::compat::imsg::imsg;
use crate::compat::imsg::msg_command;
use crate::compat::imsg::*;
use crate::compat::imsg::{IMSG_HEADER_SIZE, MAX_IMSGSIZE, PROTOCOL_VERSION};
#[cfg(target_os = "linux")]
use crate::compat::systemd::systemd_activated;
use crate::control::control_wait_exit;
use crate::environ::process_environment;
use crate::file::{
    file_read_cancel, file_read_open, file_write_close, file_write_data, file_write_left,
    file_write_open,
};
use crate::log::{fatal, fatalx, log_cstr, log_debug, log_hex};
use crate::proc::{
    bytes_of, imsg_string, proc_add_peer, proc_clear_signals, proc_exit, proc_loop, proc_send,
    proc_set_signals, proc_start, read_plain,
};
use crate::server::server_start;
use crate::shared::abi::*;
use crate::shared::abi::{ssize_t, uint32_t};
pub use crate::shared::client::client_files;
use crate::shared::client::*;
use crate::shared::client::{
    CLIENT_CONTROL, CLIENT_CONTROLCONTROL, CLIENT_CONTROL_WAITEXIT, CLIENT_LOGIN,
    CLIENT_NOSTARTSERVER, CLIENT_STARTSERVER, CLIENT_WRITE_ACK,
};
use crate::shared::command::cmd_parse_result;
use crate::shared::command::*;
use crate::shared::errno::ErrorText;
use crate::shared::posix_io::{STDERR_FILENO, WAIT_ANY, WNOHANG};
use crate::shared::posix_terminal::{ICRNL, ONLCR, OPOST, TCSANOW, VMIN, VTIME};
use crate::shared::process::{tmuxpeer, tmuxproc};
pub use crate::shared::signal::{ProcessSignal, SA_RESTART, SIGHUP, SIGTSTP, SIG_DFL, SIG_IGN};
use crate::tmux::{
    env_cstring, find_cwd, find_home_cstr, setblocking, shell_argv0_cstring, shell_command,
    socket_path_cstr,
};
use crate::tty::tty_term_read_list;
use libc::{EAGAIN, ECHILD, ENAMETOOLONG, ENOENT};
use std::cell::{Cell, RefCell};
use std::ffi::{CStr, CString, OsStr};
use std::io::{self, Write as _};
use std::os::fd::{AsFd, AsRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use tiri_sys::{
    copy_speeds, exec, make_raw, send_signal, set_signal_disposition, signal_description, system,
    wait_child, Program, SignalDisposition,
};
use tiri_rt::Runtime as _;

pub use ::libc::{CREAD, CS8, ECONNREFUSED, HUPCL, IXANY, LOCK_EX, LOCK_NB, TCSAFLUSH};

thread_local! {
    static CLIENT_PROC: RefCell<refbox::Weak<tmuxproc>> = const { RefCell::new(refbox::Weak::new()) };
    static CLIENT_PEER: RefCell<refbox::Weak<tmuxpeer>> = const { RefCell::new(refbox::Weak::new()) };
    static CLIENT_FLAGS: Cell<uint64_t> = const { Cell::new(0) };
    static CLIENT_SUSPENDED: Cell<::core::ffi::c_int> = const { Cell::new(0) };
    static CLIENT_EXITREASON: Cell<client_exit_reason> = const { Cell::new(CLIENT_EXIT_NONE) };
    static CLIENT_EXITFLAG: Cell<::core::ffi::c_int> = const { Cell::new(0) };
    static CLIENT_EXITVAL: Cell<::core::ffi::c_int> = const { Cell::new(0) };
    static CLIENT_EXITTYPE: Cell<msgtype> = const { Cell::new(0) };
    static CLIENT_EXITSESSION: RefCell<Option<CString>> = const { RefCell::new(None) };
    static CLIENT_EXITMESSAGE: RefCell<Option<Vec<u8>>> = const { RefCell::new(None) };
    static CLIENT_EXEC_PAYLOAD: RefCell<Option<(CString, CString)>> = const { RefCell::new(None) };
    static CLIENT_ATTACHED: Cell<::core::ffi::c_int> = const { Cell::new(0) };
    static CLIENT_FILES: RefCell<client_files> = const { RefCell::new(client_files::new()) };
}
fn client_proc() -> refbox::Weak<tmuxproc> {
    CLIENT_PROC.with_borrow(Clone::clone)
}
fn client_peer() -> refbox::Weak<tmuxpeer> {
    CLIENT_PEER.with_borrow(Clone::clone)
}

pub(crate) fn client_find_file(
    stream: i32,
) -> Option<std::rc::Rc<std::cell::RefCell<crate::file::client_file>>> {
    CLIENT_FILES.with_borrow(|files| crate::file::client_files_find_stream(files, stream))
}

pub(crate) fn client_register_file(
    file: &std::rc::Rc<std::cell::RefCell<crate::file::client_file>>,
) {
    CLIENT_FILES.with_borrow_mut(|files| crate::file::client_files_insert(files, file.clone()));
}

pub(crate) fn client_remove_file(
    stream: i32,
    identity: &std::rc::Rc<std::cell::RefCell<crate::file::client_file>>,
) {
    let removed = CLIENT_FILES.with_borrow_mut(|files| {
        crate::file::client_files_remove_identity(files, stream, identity)
    });
    drop(removed);
}
enum ClientLock {
    Acquired(OwnedFd),
    Retry,
    Unavailable,
}

fn client_get_lock(lockfile: &CStr) -> ClientLock {
    log_debug(format_args!("lock file is {}", log_cstr(lockfile)));
    let lock = match tiri_sys::open(lockfile, libc::O_WRONLY | libc::O_CREAT, 0o600) {
        Ok(file) => file,
        Err(error) => {
            log_debug(format_args!("open failed: {error}"));
            return ClientLock::Unavailable;
        }
    };
    if let Err(error) = tiri_sys::lock(lock.as_fd(), LOCK_EX | LOCK_NB) {
        log_debug(format_args!("flock failed: {}", ErrorText(&error)));
        if error.raw_os_error() != Some(EAGAIN) {
            return ClientLock::Acquired(lock);
        }
        while tiri_sys::lock(lock.as_fd(), LOCK_EX)
            .is_err_and(|error| error.kind() == io::ErrorKind::Interrupted)
        {}
        return ClientLock::Retry;
    }
    log_debug(format_args!("flock succeeded"));
    ClientLock::Acquired(lock)
}

fn client_connect(path: &CStr, flags: uint64_t) -> io::Result<OwnedFd> {
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
        let error = match tiri_sys::connect(std::path::Path::new(connect_path)) {
            Ok(socket) => {
                drop(lockfd);
                setblocking(socket.as_fd(), 0);
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
            if lockfd.is_some() {
                if let Err(error) = std::fs::remove_file(std::path::Path::new(connect_path)) {
                    if error.kind() != io::ErrorKind::NotFound {
                        return Err(error);
                    }
                }
            }
            let fd = server_start(flags, lockfd, lockfile);
            setblocking(fd.as_fd(), 0);
            return Ok(fd);
        }
    }
}
fn client_exit_message() -> CString {
    let detached = |prefix: &[u8], plain: &CStr| {
        CLIENT_EXITSESSION.with_borrow(|session| match session {
            Some(session) => CString::new([prefix, session.as_bytes(), b")"].concat())
                .expect("session name contains no NUL"),
            None => plain.to_owned(),
        })
    };
    match CLIENT_EXITREASON.get() as ::core::ffi::c_uint {
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
        8 => CLIENT_EXITMESSAGE.with_borrow(|message| {
            message
                .as_ref()
                .and_then(|message| CStr::from_bytes_until_nul(message).ok())
                .expect("exit message is NUL-terminated")
                .to_owned()
        }),
        _ => c"unknown reason".to_owned(),
    }
}
fn client_exit() {
    if CLIENT_FILES.with_borrow(file_write_left) == 0 {
        proc_exit(&client_proc());
    }
}
pub fn client_main(
    argv: &[CString],
    mut flags: uint64_t,
    mut feat: ::core::ffi::c_int,
) -> ::core::ffi::c_int {
    let mut pr: cmd_parse_result = cmd_parse_result::empty();
    let mut i: ::core::ffi::c_int = 0;
    let mut ppid: pid_t = 0;
    let mut msg: msgtype = 0 as msgtype;
    let mut tio = tiri_sys::empty_terminal_attributes();
    let mut saved_tio = tiri_sys::empty_terminal_attributes();
    let mut size: size_t = 0;
    let mut caps = Vec::new();
    if shell_command().is_some() {
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
            if cmd_list_any_have(pr.cmdlist.as_ref().expect("parsed command list")) != 0 {
                flags |= CLIENT_STARTSERVER as uint64_t;
            }
            drop(pr.cmdlist.take());
        }
    }
    let mut process_owner = proc_start(c"client");
    CLIENT_PROC.set(process_owner.downgrade());
    let result = (|| {
        CLIENT_FLAGS.set((flags as ::core::ffi::c_ulonglong | CLIENT_WRITE_ACK) as uint64_t);
        log_debug(format_args!(
            "flags are {}",
            log_hex(CLIENT_FLAGS.get() as ::core::ffi::c_ulonglong)
        ));
        #[cfg(target_os = "linux")]
        let activated = systemd_activated();
        #[cfg(not(target_os = "linux"))]
        let activated = false;
        let connection = if activated {
            Ok(server_start(flags, None, None))
        } else {
            client_connect(&socket_path_cstr(), CLIENT_FLAGS.get())
        };
        let fd = match connection {
            Ok(fd) => fd,
            Err(error) => {
                if error.raw_os_error() == Some(ECONNREFUSED) {
                    let _ = io::stderr().write_all(
                        &[
                            b"no server running on ",
                            socket_path_cstr().to_bytes(),
                            b"\n",
                        ]
                        .concat(),
                    );
                } else {
                    let reason = ErrorText(&error).to_string();
                    let _ = io::stderr().write_all(
                        &[
                            b"error connecting to ",
                            socket_path_cstr().to_bytes(),
                            b" (",
                            reason.as_bytes(),
                            b")\n",
                        ]
                        .concat(),
                    );
                }
                return 1;
            }
        };
        let runtime = tiri_rt::mio::Runtime::new().expect("tiri-rt initialization");
        proc_set_signals(&client_proc(), Some(Box::new(client_signal)));
        client_signal(ProcessSignal::Child);
        let peer = proc_add_peer(&client_proc(), fd, Box::new(client_dispatch));
        CLIENT_PEER.set(peer);
        let cwd = find_cwd().unwrap_or_else(|| find_home_cstr().unwrap_or(c"/").to_owned());
        let ttynam = tiri_sys::terminal_name(io::stdin().as_fd()).unwrap_or_default();
        let termname = env_cstring("TERM").unwrap_or_default();
        let termname = termname.as_c_str();
        if tiri_sys::terminal_attributes(io::stdin().as_fd()).is_ok() && !termname.is_empty() {
            match tty_term_read_list(termname) {
                Ok(read_caps) => caps = read_caps,
                Err(cause) => {
                    let _ = io::stderr().write_all(&[cause.as_bytes(), b"\n"].concat());
                    return 1 as ::core::ffi::c_int;
                }
            }
        }
        if CLIENT_FLAGS.get() & CLIENT_CONTROLCONTROL as uint64_t != 0 {
            saved_tio = match crate::shared::terminal::read_attributes(io::stdin().as_fd()) {
                Ok(attributes) => attributes,
                Err(error) => {
                    eprintln!("tcgetattr failed: {}", ErrorText(&error));
                    return 1 as ::core::ffi::c_int;
                }
            };
            make_raw(&mut tio);
            tio.c_iflag = (ICRNL | IXANY) as tcflag_t;
            tio.c_oflag = (OPOST | ONLCR) as tcflag_t;
            #[cfg(target_vendor = "apple")]
            {
                tio.c_lflag = ::libc::NOKERNINFO;
            }
            tio.c_cflag = (CREAD | CS8 | HUPCL) as tcflag_t;
            tio.c_cc[VMIN] = 1 as cc_t;
            tio.c_cc[VTIME] = 0 as cc_t;
            let _ = copy_speeds(&saved_tio, &mut tio);
            let _ = crate::shared::terminal::set_attributes(io::stdin().as_fd(), TCSANOW, &tio);
        }
        client_send_identify(&ttynam, termname, &caps, &cwd, feat);
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
                eprintln!("command too long");
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
            if cmd_pack_argv(argv, &mut data[header_size..]) != 0 as ::core::ffi::c_int {
                eprintln!("command too long");
                return 1 as ::core::ffi::c_int;
            }
            if proc_send(&client_peer(), msg, None, &data) != 0 as ::core::ffi::c_int {
                eprintln!("failed to send command");
                return 1 as ::core::ffi::c_int;
            }
        } else if msg as ::core::ffi::c_uint
            == MSG_SHELL as ::core::ffi::c_int as ::core::ffi::c_uint
        {
            proc_send(&client_peer(), msg, None, &[]);
        }
        proc_loop(&client_proc(), runtime, None);
        if CLIENT_EXITTYPE.get() as ::core::ffi::c_uint
            == MSG_EXEC as ::core::ffi::c_int as ::core::ffi::c_uint
        {
            if CLIENT_FLAGS.get() & CLIENT_CONTROLCONTROL as uint64_t != 0 {
                let _ = crate::shared::terminal::set_attributes(
                    io::stdout().as_fd(),
                    TCSAFLUSH,
                    &saved_tio,
                );
            }
            let (shell, command) = CLIENT_EXEC_PAYLOAD
                .take()
                .expect("MSG_EXEC requires a stored shell and command");
            client_exec(&shell, &command);
        }
        if CLIENT_ATTACHED.get() != 0 {
            if CLIENT_EXITREASON.get() as ::core::ffi::c_uint
                != CLIENT_EXIT_NONE as ::core::ffi::c_int as ::core::ffi::c_uint
            {
                let _ = io::stdout()
                    .write_all(&[b"[", client_exit_message().as_bytes(), b"]\n"].concat());
            }
            ppid = std::os::unix::process::parent_id() as pid_t;
            if CLIENT_EXITTYPE.get() as ::core::ffi::c_uint
                == MSG_DETACHKILL as ::core::ffi::c_int as ::core::ffi::c_uint
                && ppid > 1 as ::core::ffi::c_int
            {
                let _ = send_signal(ppid, SIGHUP);
            }
        } else if CLIENT_FLAGS.get() & CLIENT_CONTROL as uint64_t != 0 {
            if CLIENT_EXITREASON.get() as ::core::ffi::c_uint
                != CLIENT_EXIT_NONE as ::core::ffi::c_int as ::core::ffi::c_uint
            {
                let _ = io::stdout()
                    .write_all(&[b"%exit ", client_exit_message().as_bytes(), b"\n"].concat());
            } else {
                let _ = io::stdout().write_all(b"%exit\n");
            }
            let _ = io::stdout().flush();
            if CLIENT_FLAGS.get() as ::core::ffi::c_ulonglong & CLIENT_CONTROL_WAITEXIT != 0 {
                control_wait_exit();
            }
            if CLIENT_FLAGS.get() & CLIENT_CONTROLCONTROL as uint64_t != 0 {
                let _ = io::stdout().write_all(b"\x1b\\");
                let _ = io::stdout().flush();
                let _ = crate::shared::terminal::set_attributes(
                    io::stdout().as_fd(),
                    TCSAFLUSH,
                    &saved_tio,
                );
            }
        } else if CLIENT_EXITREASON.get() as ::core::ffi::c_uint
            != CLIENT_EXIT_NONE as ::core::ffi::c_int as ::core::ffi::c_uint
        {
            let _ = io::stderr().write_all(&[client_exit_message().as_bytes(), b"\n"].concat());
        }
        setblocking(io::stdin().as_fd(), 1 as ::core::ffi::c_int);
        setblocking(io::stdout().as_fd(), 1 as ::core::ffi::c_int);
        setblocking(io::stderr().as_fd(), 1 as ::core::ffi::c_int);
        CLIENT_EXITMESSAGE.take();
        CLIENT_EXITSESSION.take();
        CLIENT_EXITVAL.get()
    })();
    drop(process_owner);
    CLIENT_PEER.set(refbox::Weak::new());
    CLIENT_PROC.set(refbox::Weak::new());
    result
}
fn client_send_identify(
    ttynam: &CStr,
    termname: &CStr,
    caps: &[CString],
    cwd: &CStr,
    mut feat: ::core::ffi::c_int,
) {
    let mut flags: uint64_t = CLIENT_FLAGS.get();
    let mut pid: pid_t = 0;
    proc_send(
        &client_peer(),
        MSG_IDENTIFY_LONGFLAGS,
        None,
        &bytes_of(&flags),
    );
    proc_send(
        &client_peer(),
        MSG_IDENTIFY_LONGFLAGS,
        None,
        &bytes_of(&CLIENT_FLAGS.get()),
    );
    proc_send(
        &client_peer(),
        MSG_IDENTIFY_TERM,
        None,
        termname.to_bytes_with_nul(),
    );
    proc_send(
        &client_peer(),
        MSG_IDENTIFY_FEATURES,
        None,
        &bytes_of(&feat),
    );
    proc_send(
        &client_peer(),
        MSG_IDENTIFY_TTYNAME,
        None,
        ttynam.to_bytes_with_nul(),
    );
    proc_send(
        &client_peer(),
        MSG_IDENTIFY_CWD,
        None,
        cwd.to_bytes_with_nul(),
    );
    for cap in caps {
        proc_send(
            &client_peer(),
            MSG_IDENTIFY_TERMINFO,
            None,
            cap.as_bytes_with_nul(),
        );
    }
    let fd = io::stdin()
        .as_fd()
        .try_clone_to_owned()
        .unwrap_or_else(|error| fatal(&error, |out| out.write_all(b"dup failed")));
    proc_send(&client_peer(), MSG_IDENTIFY_STDIN, Some(fd), &[]);
    let fd = io::stdout()
        .as_fd()
        .try_clone_to_owned()
        .unwrap_or_else(|error| fatal(&error, |out| out.write_all(b"dup failed")));
    proc_send(&client_peer(), MSG_IDENTIFY_STDOUT, Some(fd), &[]);
    pid = std::process::id() as pid_t;
    proc_send(
        &client_peer(),
        MSG_IDENTIFY_CLIENTPID,
        None,
        &bytes_of(&pid),
    );
    for var in process_environment() {
        let var = var.as_bytes_with_nul();
        if var.len() <= (MAX_IMSGSIZE as usize).wrapping_sub(IMSG_HEADER_SIZE) {
            proc_send(&client_peer(), MSG_IDENTIFY_ENVIRON, None, var);
        }
    }
    proc_send(&client_peer(), MSG_IDENTIFY_DONE, None, &[]);
}
fn client_exec(shell: &CStr, shellcmd: &CStr) -> ! {
    log_debug(format_args!(
        "shell {}, command {}",
        log_cstr(shell),
        log_cstr(shellcmd)
    ));
    let argv0 = shell_argv0_cstring(shell, CLIENT_FLAGS.get() & CLIENT_LOGIN as uint64_t != 0);
    std::env::set_var("SHELL", OsStr::from_bytes(shell.to_bytes()));
    proc_clear_signals(&client_proc(), 1 as ::core::ffi::c_int);
    setblocking(io::stdin().as_fd(), 1 as ::core::ffi::c_int);
    setblocking(io::stdout().as_fd(), 1 as ::core::ffi::c_int);
    setblocking(io::stderr().as_fd(), 1 as ::core::ffi::c_int);
    tiri_sys::close_on_exec_from(STDERR_FILENO + 1 as ::core::ffi::c_int);
    let error = exec(Program::Path(shell, &[&argv0, c"-c", shellcmd]));
    fatal(&error, |out| out.write_all(b"execl failed"));
}
fn client_signal(sig: ProcessSignal) {
    log_debug(format_args!(
        "{}: {}",
        "client_signal",
        log_cstr(&signal_description(sig.as_raw()))
    ));
    if sig == ProcessSignal::Child {
        loop {
            match wait_child(WAIT_ANY, WNOHANG) {
                Ok(None) => break,
                Ok(Some(_)) => {}
                Err(error) if error.raw_os_error() == Some(ECHILD) => break,
                Err(error) => log_debug(format_args!("waitpid failed: {}", ErrorText(&error))),
            }
        }
    } else if CLIENT_ATTACHED.get() == 0 {
        if sig == ProcessSignal::Terminate || sig == ProcessSignal::Hangup {
            proc_exit(&client_proc());
        }
    } else {
        match sig {
            ProcessSignal::Hangup => {
                CLIENT_EXITREASON.set(CLIENT_EXIT_LOST_TTY);
                CLIENT_EXITVAL.set(1 as ::core::ffi::c_int);
                proc_send(&client_peer(), MSG_EXITING, None, &[]);
            }
            ProcessSignal::Terminate => {
                if CLIENT_SUSPENDED.get() == 0 {
                    CLIENT_EXITREASON.set(CLIENT_EXIT_TERMINATED);
                }
                CLIENT_EXITVAL.set(1 as ::core::ffi::c_int);
                proc_send(&client_peer(), MSG_EXITING, None, &[]);
            }
            ProcessSignal::WindowChange => {
                proc_send(&client_peer(), MSG_RESIZE, None, &[]);
            }
            ProcessSignal::Continue => {
                if let Err(error) = set_signal_disposition(SIGTSTP, SignalDisposition::Ignore) {
                    fatal(&error, |out| out.write_all(b"sigaction failed"));
                }
                proc_send(&client_peer(), MSG_WAKEUP, None, &[]);
                CLIENT_SUSPENDED.set(0 as ::core::ffi::c_int);
            }
            _ => {}
        }
    };
}
fn client_file_check_cb() {
    if CLIENT_EXITFLAG.get() != 0 {
        client_exit();
    }
}
fn client_dispatch(message: crate::shared::process::PeerMessage<'_>) {
    let imsg = match message {
        crate::shared::process::PeerMessage::Disconnected => {
            if CLIENT_EXITFLAG.get() == 0 {
                CLIENT_EXITREASON.set(CLIENT_EXIT_LOST_SERVER);
                CLIENT_EXITVAL.set(1 as ::core::ffi::c_int);
            }
            proc_exit(&client_proc());
            return;
        }
        crate::shared::process::PeerMessage::Message(imsg) => imsg,
    };
    if CLIENT_ATTACHED.get() != 0 {
        client_dispatch_attached(imsg);
    } else {
        client_dispatch_wait(imsg);
    };
}
fn client_dispatch_exit_message(data: &[u8]) {
    let header = ::core::mem::size_of::<::core::ffi::c_int>();
    if data.len() < header && !data.is_empty() {
        fatalx(|out| out.write_all(b"bad MSG_EXIT size"));
    }
    if let Some(retval) = read_plain::<::core::ffi::c_int>(data) {
        CLIENT_EXITVAL.set(retval);
    }
    if data.len() > header {
        let mut message = data[header..].to_vec();
        *message.last_mut().unwrap() = 0;
        CLIENT_EXITMESSAGE.set(Some(message));
        CLIENT_EXITREASON.set(CLIENT_EXIT_MESSAGE_PROVIDED);
    }
}
fn client_dispatch_wait(imsg: &mut imsg) {
    let datalen = imsg.data.len() as ssize_t;
    match imsg.hdr.type_0 {
        MSG_EXIT | MSG_SHUTDOWN => {
            client_dispatch_exit_message(&imsg.data);
            CLIENT_EXITFLAG.set(1 as ::core::ffi::c_int);
            client_exit();
        }
        MSG_READY => {
            if datalen != 0 as ssize_t {
                fatalx(|out| out.write_all(b"bad MSG_READY size"));
            }
            CLIENT_ATTACHED.set(1 as ::core::ffi::c_int);
            proc_send(&client_peer(), MSG_RESIZE, None, &[]);
        }
        MSG_VERSION => {
            if datalen != 0 as ssize_t {
                fatalx(|out| out.write_all(b"bad MSG_VERSION size"));
            }
            eprintln!(
                "protocol version mismatch (client {}, server {})",
                PROTOCOL_VERSION,
                imsg.hdr.peerid & 0xff as uint32_t,
            );
            CLIENT_EXITVAL.set(1 as ::core::ffi::c_int);
            proc_exit(&client_proc());
        }
        MSG_FLAGS => {
            if datalen as usize != ::core::mem::size_of::<uint64_t>() as usize {
                fatalx(|out| out.write_all(b"bad MSG_FLAGS string"));
            }
            CLIENT_FLAGS.set(read_plain(&imsg.data).expect("checked MSG_FLAGS size"));
            log_debug(format_args!(
                "new flags are {}",
                log_hex(CLIENT_FLAGS.get() as ::core::ffi::c_ulonglong)
            ));
        }
        MSG_SHELL => {
            let Some(shell) = imsg_string(imsg) else {
                fatalx(|out| out.write_all(b"bad MSG_SHELL string"));
            };
            let Some(command) = shell_command() else {
                fatalx(|out| out.write_all(b"unexpected MSG_SHELL"));
            };
            client_exec(shell, &command);
        }
        MSG_DETACH | MSG_DETACHKILL => {
            proc_send(&client_peer(), MSG_EXITING, None, &[]);
        }
        MSG_EXITED => {
            proc_exit(&client_proc());
        }
        MSG_READ_OPEN => {
            file_read_open(
                &client_peer(),
                imsg,
                (CLIENT_FLAGS.get() & CLIENT_CONTROL as uint64_t == 0) as ::core::ffi::c_int,
                Some(Box::new(|_| client_file_check_cb())),
            );
        }
        MSG_READ_CANCEL => {
            file_read_cancel(imsg);
        }
        MSG_WRITE_OPEN => {
            file_write_open(
                &client_peer(),
                imsg,
                (CLIENT_FLAGS.get() & CLIENT_CONTROL as uint64_t == 0) as ::core::ffi::c_int,
                Some(Box::new(|_| client_file_check_cb())),
            );
        }
        MSG_WRITE => {
            file_write_data(imsg);
        }
        MSG_WRITE_CLOSE => {
            file_write_close(imsg);
        }
        MSG_STDERR | MSG_STDIN | MSG_STDOUT => {
            eprintln!("server version is too old for client");
            proc_exit(&client_proc());
        }
        _ => {
            log_debug(format_args!("unknown message type {}", { imsg.hdr.type_0 }));
        }
    };
}
fn client_dispatch_attached(imsg: &mut imsg) {
    let datalen = imsg.data.len() as ssize_t;
    match imsg.hdr.type_0 {
        MSG_FLAGS => {
            if datalen as usize != ::core::mem::size_of::<uint64_t>() as usize {
                fatalx(|out| out.write_all(b"bad MSG_FLAGS string"));
            }
            CLIENT_FLAGS.set(read_plain(&imsg.data).expect("checked MSG_FLAGS size"));
            log_debug(format_args!(
                "new flags are {}",
                log_hex(CLIENT_FLAGS.get() as ::core::ffi::c_ulonglong)
            ));
        }
        MSG_DETACH | MSG_DETACHKILL => {
            let Some(session) = imsg_string(imsg) else {
                fatalx(|out| out.write_all(b"bad MSG_DETACH string"));
            };
            CLIENT_EXITSESSION.set(Some(session.to_owned()));
            CLIENT_EXITTYPE.set(imsg.hdr.type_0);
            if imsg.hdr.type_0 == MSG_DETACHKILL {
                CLIENT_EXITREASON.set(CLIENT_EXIT_DETACHED_HUP);
            } else {
                CLIENT_EXITREASON.set(CLIENT_EXIT_DETACHED);
            }
            proc_send(&client_peer(), MSG_EXITING, None, &[]);
        }
        MSG_EXEC => {
            let Some(command) = imsg_string(imsg)
                .filter(|command| command.to_bytes_with_nul().len() != imsg.data.len())
            else {
                fatalx(|out| out.write_all(b"bad MSG_EXEC string"));
            };
            let shell = CStr::from_bytes_until_nul(&imsg.data[command.to_bytes_with_nul().len()..])
                .expect("the payload ends in a NUL");
            CLIENT_EXEC_PAYLOAD.set(Some((shell.to_owned(), command.to_owned())));
            CLIENT_EXITTYPE.set(imsg.hdr.type_0);
            proc_send(&client_peer(), MSG_EXITING, None, &[]);
        }
        MSG_EXIT => {
            client_dispatch_exit_message(&imsg.data);
            if CLIENT_EXITREASON.get() as ::core::ffi::c_uint
                == CLIENT_EXIT_NONE as ::core::ffi::c_int as ::core::ffi::c_uint
            {
                CLIENT_EXITREASON.set(CLIENT_EXIT_EXITED);
            }
            proc_send(&client_peer(), MSG_EXITING, None, &[]);
        }
        MSG_EXITED => {
            if datalen != 0 as ssize_t {
                fatalx(|out| out.write_all(b"bad MSG_EXITED size"));
            }
            proc_exit(&client_proc());
        }
        MSG_SHUTDOWN => {
            if datalen != 0 as ssize_t {
                fatalx(|out| out.write_all(b"bad MSG_SHUTDOWN size"));
            }
            proc_send(&client_peer(), MSG_EXITING, None, &[]);
            CLIENT_EXITREASON.set(CLIENT_EXIT_SERVER_EXITED);
            CLIENT_EXITVAL.set(1 as ::core::ffi::c_int);
        }
        MSG_SUSPEND => {
            if datalen != 0 as ssize_t {
                fatalx(|out| out.write_all(b"bad MSG_SUSPEND size"));
            }
            if let Err(error) = set_signal_disposition(SIGTSTP, SignalDisposition::Default) {
                fatal(&error, |out| out.write_all(b"sigaction failed"));
            }
            CLIENT_SUSPENDED.set(1 as ::core::ffi::c_int);
            let _ = send_signal(std::process::id() as pid_t, SIGTSTP);
        }
        MSG_LOCK => {
            let Some(command) = imsg_string(imsg) else {
                fatalx(|out| out.write_all(b"bad MSG_LOCK string"));
            };
            system(command);
            proc_send(&client_peer(), MSG_UNLOCK, None, &[]);
        }
        _ => {
            log_debug(format_args!("unknown message type {}", { imsg.hdr.type_0 }));
        }
    };
}
