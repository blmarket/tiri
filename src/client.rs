use crate::cli::find_cwd;
use std::cell::{Cell, RefCell};
use std::ffi::{CStr, CString, OsStr};
use std::io::{self, Write as _};
use std::os::fd::{AsFd, AsRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use tiri_cmds::argv_starts_server;
use tiri_core::actions::pack_argv;
use tiri_core::compat::imsg::{
    IMSG_HEADER_SIZE, MAX_IMSGSIZE, MSG_COMMAND, MSG_DETACH, MSG_DETACHKILL, MSG_EXEC, MSG_EXIT,
    MSG_EXITED, MSG_EXITING, MSG_FLAGS, MSG_IDENTIFY_CLIENTPID, MSG_IDENTIFY_CWD,
    MSG_IDENTIFY_DONE, MSG_IDENTIFY_ENVIRON, MSG_IDENTIFY_FEATURES, MSG_IDENTIFY_LONGFLAGS,
    MSG_IDENTIFY_STDIN, MSG_IDENTIFY_STDOUT, MSG_IDENTIFY_TERM, MSG_IDENTIFY_TERMINFO,
    MSG_IDENTIFY_TTYNAME, MSG_LOCK, MSG_READY, MSG_READ_CANCEL, MSG_READ_OPEN, MSG_RESIZE,
    MSG_SHELL, MSG_SHUTDOWN, MSG_STDERR, MSG_STDIN, MSG_STDOUT, MSG_SUSPEND, MSG_UNLOCK,
    MSG_VERSION, MSG_WAKEUP, MSG_WRITE, MSG_WRITE_CLOSE, MSG_WRITE_OPEN, PROTOCOL_VERSION, imsg,
    msg_command, msgtype,
};
#[cfg(target_os = "linux")]
use tiri_core::compat::systemd::systemd_activated;
use tiri_core::control::control_wait_exit;
use tiri_core::environ::process_environment;
use tiri_core::file::{
    file_read_cancel, file_read_open, file_write_close, file_write_data, file_write_left,
    file_write_open,
};
use tiri_core::libc::{
    self, CREAD, CS8, EAGAIN, ECHILD, ECONNREFUSED, ENAMETOOLONG, ENOENT, HUPCL, IXANY, LOCK_EX,
    LOCK_NB, TCSAFLUSH,
};
use tiri_core::log::{fatal, fatalx, log_cstr, log_debug, log_hex};
use tiri_core::proc::{
    bytes_of, imsg_string, proc_add_peer, proc_clear_signals, proc_exit, proc_loop, proc_send,
    proc_set_signals, proc_start, read_plain,
};
use tiri_core::refbox;
use tiri_core::server::server_start;
use tiri_core::shared::abi::{cc_t, pid_t, size_t, ssize_t, tcflag_t, uint32_t, uint64_t};
use tiri_core::shared::client::{
    CLIENT_CONTROL, CLIENT_CONTROL_WAITEXIT, CLIENT_CONTROLCONTROL, CLIENT_EXIT_DETACHED,
    CLIENT_EXIT_DETACHED_HUP, CLIENT_EXIT_EXITED, CLIENT_EXIT_LOST_SERVER, CLIENT_EXIT_LOST_TTY,
    CLIENT_EXIT_MESSAGE_PROVIDED, CLIENT_EXIT_NONE, CLIENT_EXIT_SERVER_EXITED,
    CLIENT_EXIT_TERMINATED, CLIENT_LOGIN, CLIENT_NOSTARTSERVER, CLIENT_STARTSERVER,
    CLIENT_WRITE_ACK, client_exit_reason,
};
use tiri_core::shared::errno::ErrorText;
use tiri_core::shared::posix_io::{STDERR_FILENO, WAIT_ANY, WNOHANG};
use tiri_core::shared::posix_terminal::{ICRNL, ONLCR, OPOST, TCSANOW, VMIN, VTIME};
use tiri_core::shared::process::{PeerMessage, tmuxpeer, tmuxproc};
use tiri_core::shared::signal::{ProcessSignal, SIGHUP, SIGTSTP};
use tiri_core::tiri_rt;
use tiri_core::tiri_sys::{
    self, Program, SignalDisposition, copy_speeds, exec, make_raw, send_signal,
    set_signal_disposition, set_terminal_attributes, signal_description, system,
    terminal_attributes, wait_child,
};
use tiri_core::tmux::{
    env_cstring, find_home_cstr, setblocking, shell_argv0_cstring, socket_path_cstr,
};
use tiri_core::tty::tty_term_read_list;

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
    static CLIENT_SHELL_COMMAND: RefCell<Option<CString>> = const { RefCell::new(None) };
    static CLIENT_ATTACHED: Cell<::core::ffi::c_int> = const { Cell::new(0) };
}

fn client_proc() -> refbox::Weak<tmuxproc> {
    CLIENT_PROC.with_borrow(Clone::clone)
}

fn client_peer() -> refbox::Weak<tmuxpeer> {
    CLIENT_PEER.with_borrow(Clone::clone)
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
    if file_write_left() == 0 {
        proc_exit(&client_proc());
    }
}

pub(crate) fn client_main(
    argv: &[CString],
    shell_command: Option<CString>,
    mut flags: uint64_t,
    feat: ::core::ffi::c_int,
) -> ::core::ffi::c_int {
    let mut tio = tiri_sys::empty_terminal_attributes();
    let mut saved_tio = tiri_sys::empty_terminal_attributes();
    let mut caps = Vec::new();
    let msg: msgtype = if shell_command.is_some() {
        flags |= CLIENT_STARTSERVER as uint64_t;
        MSG_SHELL
    } else if argv.is_empty() {
        flags |= CLIENT_STARTSERVER as uint64_t;
        MSG_COMMAND
    } else {
        if argv_starts_server(argv) {
            flags |= CLIENT_STARTSERVER as uint64_t;
        }
        MSG_COMMAND
    };
    CLIENT_SHELL_COMMAND.set(shell_command);
    let process_owner = proc_start(c"client");
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
        let runtime = tiri_rt::Runtime::new().expect("tiri-rt initialization");
        proc_set_signals(&client_proc(), Some(Box::new(client_signal)));
        client_signal(ProcessSignal::Child);
        let peer = proc_add_peer(&client_proc(), fd, Box::new(client_dispatch));
        CLIENT_PEER.set(peer);
        let cwd = find_cwd().unwrap_or_else(|| find_home_cstr().unwrap_or(c"/").to_owned());
        let ttynam = tiri_sys::terminal_name(io::stdin().as_fd()).unwrap_or_default();
        let termname = env_cstring("TERM").unwrap_or_default();
        let termname = termname.as_c_str();
        if terminal_attributes(io::stdin().as_fd()).is_ok() && !termname.is_empty() {
            match tty_term_read_list(termname) {
                Ok(read_caps) => caps = read_caps,
                Err(cause) => {
                    let _ = io::stderr().write_all(&[cause.as_bytes(), b"\n"].concat());
                    return 1;
                }
            }
        }
        if CLIENT_FLAGS.get() & CLIENT_CONTROLCONTROL as uint64_t != 0 {
            saved_tio = match terminal_attributes(io::stdin().as_fd()) {
                Ok(attributes) => attributes,
                Err(error) => {
                    eprintln!("tcgetattr failed: {}", ErrorText(&error));
                    return 1;
                }
            };
            make_raw(&mut tio);
            tio.c_iflag = (ICRNL | IXANY) as tcflag_t;
            tio.c_oflag = (OPOST | ONLCR) as tcflag_t;
            #[cfg(target_vendor = "apple")]
            {
                tio.c_lflag = libc::NOKERNINFO;
            }
            tio.c_cflag = (CREAD | CS8 | HUPCL) as tcflag_t;
            tio.c_cc[VMIN] = 1 as cc_t;
            tio.c_cc[VTIME] = 0 as cc_t;
            let _ = copy_speeds(&saved_tio, &mut tio);
            let _ = set_terminal_attributes(io::stdin().as_fd(), TCSANOW, &tio);
        }
        client_send_identify(&ttynam, termname, &caps, &cwd, feat);
        if msg == MSG_COMMAND {
            let size: size_t = argv
                .iter()
                .map(|arg| arg.as_bytes_with_nul().len())
                .sum();
            if size
                > (MAX_IMSGSIZE as usize)
                    .wrapping_sub(::core::mem::size_of::<msg_command>() as usize)
            {
                eprintln!("command too long");
                return 1;
            }
            const _: () = assert!(
                ::core::mem::size_of::<msg_command>()
                    == ::core::mem::size_of::<::core::ffi::c_int>()
            );
            let header_size = ::core::mem::size_of::<msg_command>();
            let mut data = vec![0u8; header_size + size];
            let argc = ::core::ffi::c_int::try_from(argv.len()).expect("argv length exceeds c_int");
            data[..header_size].copy_from_slice(&argc.to_ne_bytes());
            if pack_argv(argv, &mut data[header_size..]) != 0 {
                eprintln!("command too long");
                return 1;
            }
            if proc_send(&client_peer(), msg, None, &data) != 0 {
                eprintln!("failed to send command");
                return 1;
            }
        } else if msg == MSG_SHELL {
            proc_send(&client_peer(), msg, None, &[]);
        }
        proc_loop(&client_proc(), runtime, None);
        if CLIENT_EXITTYPE.get() == MSG_EXEC {
            if CLIENT_FLAGS.get() & CLIENT_CONTROLCONTROL as uint64_t != 0 {
                let _ = set_terminal_attributes(io::stdout().as_fd(), TCSAFLUSH, &saved_tio);
            }
            let (shell, command) = CLIENT_EXEC_PAYLOAD
                .take()
                .expect("MSG_EXEC requires a stored shell and command");
            client_exec(&shell, &command);
        }
        if CLIENT_ATTACHED.get() != 0 {
            if CLIENT_EXITREASON.get() != CLIENT_EXIT_NONE {
                let _ = io::stdout()
                    .write_all(&[b"[", client_exit_message().as_bytes(), b"]\n"].concat());
            }
            let ppid = std::os::unix::process::parent_id() as pid_t;
            if CLIENT_EXITTYPE.get() == MSG_DETACHKILL && ppid > 1 {
                let _ = send_signal(ppid, SIGHUP);
            }
        } else if CLIENT_FLAGS.get() & CLIENT_CONTROL as uint64_t != 0 {
            if CLIENT_EXITREASON.get() != CLIENT_EXIT_NONE {
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
                let _ = set_terminal_attributes(io::stdout().as_fd(), TCSAFLUSH, &saved_tio);
            }
        } else if CLIENT_EXITREASON.get() != CLIENT_EXIT_NONE {
            let _ = io::stderr().write_all(&[client_exit_message().as_bytes(), b"\n"].concat());
        }
        setblocking(io::stdin().as_fd(), 1);
        setblocking(io::stdout().as_fd(), 1);
        setblocking(io::stderr().as_fd(), 1);
        CLIENT_EXITMESSAGE.take();
        CLIENT_EXITSESSION.take();
        CLIENT_SHELL_COMMAND.take();
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
    feat: ::core::ffi::c_int,
) {
    let flags: uint64_t = CLIENT_FLAGS.get();
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
    let pid = std::process::id() as pid_t;
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
    proc_clear_signals(&client_proc(), 1);
    setblocking(io::stdin().as_fd(), 1);
    setblocking(io::stdout().as_fd(), 1);
    setblocking(io::stderr().as_fd(), 1);
    tiri_sys::close_on_exec_from(STDERR_FILENO + 1);
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
                CLIENT_EXITVAL.set(1);
                proc_send(&client_peer(), MSG_EXITING, None, &[]);
            }
            ProcessSignal::Terminate => {
                if CLIENT_SUSPENDED.get() == 0 {
                    CLIENT_EXITREASON.set(CLIENT_EXIT_TERMINATED);
                }
                CLIENT_EXITVAL.set(1);
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
                CLIENT_SUSPENDED.set(0);
            }
            _ => {}
        }
    }
}

fn client_file_check_cb() {
    if CLIENT_EXITFLAG.get() != 0 {
        client_exit();
    }
}

fn client_dispatch(message: PeerMessage<'_>) {
    let imsg = match message {
        PeerMessage::Disconnected => {
            if CLIENT_EXITFLAG.get() == 0 {
                CLIENT_EXITREASON.set(CLIENT_EXIT_LOST_SERVER);
                CLIENT_EXITVAL.set(1);
            }
            proc_exit(&client_proc());
            return;
        }
        PeerMessage::Message(imsg) => imsg,
    };
    if CLIENT_ATTACHED.get() != 0 {
        client_dispatch_attached(imsg);
    } else {
        client_dispatch_wait(imsg);
    }
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
            CLIENT_EXITFLAG.set(1);
            client_exit();
        }
        MSG_READY => {
            if datalen != 0 {
                fatalx(|out| out.write_all(b"bad MSG_READY size"));
            }
            CLIENT_ATTACHED.set(1);
            proc_send(&client_peer(), MSG_RESIZE, None, &[]);
        }
        MSG_VERSION => {
            if datalen != 0 {
                fatalx(|out| out.write_all(b"bad MSG_VERSION size"));
            }
            eprintln!(
                "protocol version mismatch (client {}, server {})",
                PROTOCOL_VERSION,
                imsg.hdr.peerid & 0xff as uint32_t,
            );
            CLIENT_EXITVAL.set(1);
            proc_exit(&client_proc());
        }
        MSG_FLAGS => {
            if datalen as usize != ::core::mem::size_of::<uint64_t>() {
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
            let Some(command) = CLIENT_SHELL_COMMAND.take() else {
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
    }
}

fn client_dispatch_attached(imsg: &mut imsg) {
    let datalen = imsg.data.len() as ssize_t;
    match imsg.hdr.type_0 {
        MSG_FLAGS => {
            if datalen as usize != ::core::mem::size_of::<uint64_t>() {
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
            if CLIENT_EXITREASON.get() == CLIENT_EXIT_NONE {
                CLIENT_EXITREASON.set(CLIENT_EXIT_EXITED);
            }
            proc_send(&client_peer(), MSG_EXITING, None, &[]);
        }
        MSG_EXITED => {
            if datalen != 0 {
                fatalx(|out| out.write_all(b"bad MSG_EXITED size"));
            }
            proc_exit(&client_proc());
        }
        MSG_SHUTDOWN => {
            if datalen != 0 {
                fatalx(|out| out.write_all(b"bad MSG_SHUTDOWN size"));
            }
            proc_send(&client_peer(), MSG_EXITING, None, &[]);
            CLIENT_EXITREASON.set(CLIENT_EXIT_SERVER_EXITED);
            CLIENT_EXITVAL.set(1);
        }
        MSG_SUSPEND => {
            if datalen != 0 {
                fatalx(|out| out.write_all(b"bad MSG_SUSPEND size"));
            }
            if let Err(error) = set_signal_disposition(SIGTSTP, SignalDisposition::Default) {
                fatal(&error, |out| out.write_all(b"sigaction failed"));
            }
            CLIENT_SUSPENDED.set(1);
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
    }
}
