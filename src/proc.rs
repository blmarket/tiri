use crate::src::compat::getpeereid::getpeereid;
use crate::src::compat::imsg::imsg;
use crate::src::compat::imsg::PROTOCOL_VERSION;
use crate::src::compat::imsg::*;
use crate::src::compat::imsg::{
    imsg_compose, imsgbuf_clear, imsgbuf_get, imsgbuf_init, imsgbuf_output, imsgbuf_queuelen,
    imsgbuf_receive, imsgbuf_written,
};
use crate::src::compat::setproctitle::setproctitle;
use crate::src::ffi::utf8proc::utf8proc_version;
use crate::src::format::bytes::write_cstr;
use crate::src::log::{fatal, log_cstr, log_debug, log_open, log_pointer, log_toggle};
use crate::src::reactor;
use crate::src::shared::abi::*;
use crate::src::shared::abi::{gid_t, uid_t, uint32_t};
use crate::src::shared::process::PeerMessage;
use crate::src::shared::process::{tmuxpeer, tmuxproc};
use crate::src::shared::signal::ProcessSignal;
pub use crate::src::shared::signal::{
    SA_RESTART, SIGCHLD, SIGCONT, SIGHUP, SIGINT, SIGTERM, SIGTSTP, SIGTTIN, SIGTTOU, SIGUSR1,
    SIGUSR2, SIGWINCH, SIG_DFL, SIG_IGN,
};
use crate::src::tmux::{getversion, socket_path_cstr};
use libc::utsname;
use libc::{daemon, fork, getpid, memset, sigaction, sigemptyset, uname};
use std::ffi::CStr;
use std::os::fd::{AsFd, AsRawFd, OwnedFd};
use tiri_rt::{AsyncRead as _, AsyncWrite as _, Runtime as _, Signals as _};

pub use ::libc::{SIGPIPE, SIGQUIT};

pub const NCURSES_VERSION_PATCH: ::core::ffi::c_int = 20251230 as ::core::ffi::c_int;
pub const NCURSES_VERSION: &std::ffi::CStr = c"6.6";

pub const PEER_BAD: ::core::ffi::c_int = 0x1 as ::core::ffi::c_int;
unsafe fn proc_dispatch(peer: *mut tmuxpeer, message: PeerMessage<'_>) -> bool {
    let Some(mut dispatchcb) = (*peer).dispatchcb.take() else {
        return false;
    };
    let tp = (*peer).parent;
    dispatchcb(message);
    if (*tp)
        .peers
        .iter()
        .any(|owned_peer| std::ptr::eq(&**owned_peer, peer))
    {
        (*peer).dispatchcb = Some(dispatchcb);
        true
    } else {
        false
    }
}
unsafe fn proc_disconnect(peer: *mut tmuxpeer) {
    (*peer).flags |= PEER_BAD;
    drop((*peer).io_task.take());
    imsgbuf_clear(&mut (*peer).ibuf);
    proc_dispatch(peer, PeerMessage::Disconnected);
}

unsafe fn proc_receive(peer: *mut tmuxpeer, bytes: &[u8], received: tiri_rt::Received) -> bool {
    if received.bytes == 0
        || imsgbuf_receive(&mut (*peer).ibuf, &bytes[..received.bytes], received.fd).is_err()
    {
        proc_disconnect(peer);
        return false;
    }
    while let Some(mut message) = imsgbuf_get(&mut (*peer).ibuf) {
        log_debug(format_args!(
            "peer {} message {}",
            log_pointer(peer),
            message.hdr.type_0
        ));
        if peer_check_version(peer, &message) != 0 {
            break;
        }
        if !proc_dispatch(peer, PeerMessage::Message(&mut message)) {
            return false;
        }
    }
    true
}
unsafe fn proc_signal_cb(signo: ::core::ffi::c_int, tp: *mut tmuxproc) {
    (*tp)
        .signalcb
        .as_mut()
        .expect("signal callback is installed")(ProcessSignal::from_raw(signo));
}
unsafe fn peer_check_version(peer: *mut tmuxpeer, imsg: &imsg) -> ::core::ffi::c_int {
    let mut version: ::core::ffi::c_int = 0;
    version = (imsg.hdr.peerid & 0xff as uint32_t) as ::core::ffi::c_int;
    if imsg.hdr.type_0 != MSG_VERSION && version != PROTOCOL_VERSION {
        log_debug(format_args!(
            "peer {} bad version {}",
            log_pointer(peer),
            (version) as i32
        ));
        proc_send(peer, MSG_VERSION, None, &[]);
        (*peer).flags |= PEER_BAD;
        return -(1 as ::core::ffi::c_int);
    }
    0 as ::core::ffi::c_int
}
unsafe fn proc_update_io(peer: *mut tmuxpeer) {
    let writable = imsgbuf_queuelen(&(*peer).ibuf) > 0;
    if (*peer).ibuf.fd.is_none() || ((*peer).io_task.is_some() && (*peer).io_writable == writable) {
        return;
    }
    (*peer).io_writable = writable;
    let fd = (*peer).ibuf.fd.as_ref().expect("peer socket").as_fd();
    reactor::task_start(&mut (*peer).io_task, move || {
        let source = reactor::io(fd)?;
        Ok(async move {
            use std::future::{poll_fn, Future};
            use std::pin::pin;
            use std::task::Poll;
            enum Completion {
                Read(std::io::Result<tiri_rt::Received>),
                Write(std::io::Result<usize>),
            }
            let mut prefer_write = false;
            loop {
                let reading = unsafe { (*peer).flags & PEER_BAD == 0 };
                let (output, raw_fd) = unsafe { imsgbuf_output(&(*peer).ibuf) };
                if !reading && output.is_empty() {
                    unsafe { proc_disconnect(peer) };
                    return;
                }
                let mut input = vec![0; 65536];
                let buffers = [std::io::IoSlice::new(&output)];
                let completion = {
                    let fd = raw_fd.map(|raw| unsafe { std::os::fd::BorrowedFd::borrow_raw(raw) });
                    let mut reader = pin!(source.read(&mut input));
                    let mut writer = pin!(source.write(&buffers, fd));
                    poll_fn(|cx| {
                        for writing in [prefer_write, !prefer_write] {
                            if writing && !output.is_empty() {
                                if let Poll::Ready(result) = writer.as_mut().poll(cx) {
                                    return Poll::Ready(Completion::Write(result));
                                }
                            } else if !writing && reading {
                                if let Poll::Ready(result) = reader.as_mut().poll(cx) {
                                    return Poll::Ready(Completion::Read(result));
                                }
                            }
                        }
                        Poll::Pending
                    })
                    .await
                };
                unsafe {
                    match completion {
                        Completion::Read(Ok(received)) => {
                            prefer_write = true;
                            if !proc_receive(peer, &input, received) {
                                return;
                            }
                        }
                        Completion::Write(Ok(n)) if n > 0 => {
                            prefer_write = false;
                            imsgbuf_written(&mut (*peer).ibuf, n);
                        }
                        Completion::Write(Err(error))
                            if error.raw_os_error() == Some(libc::ENOBUFS) =>
                        {
                            tiri_rt::mio::Sleep::new(
                                std::time::Instant::now() + std::time::Duration::from_millis(1),
                            )
                            .await
                            .ok();
                        }
                        _ => {
                            proc_disconnect(peer);
                            return;
                        }
                    }
                    if (*peer).flags & PEER_BAD != 0 && imsgbuf_queuelen(&(*peer).ibuf) == 0 {
                        proc_disconnect(peer);
                        return;
                    }
                    proc_update_io(peer);
                }
                reactor::yield_now().await;
            }
        })
    })
    .expect("start peer I/O");
}
/// The bytes of a plain message struct, as sent on the wire.
pub fn bytes_of<T: Copy>(value: &T) -> &[u8] {
    unsafe { std::slice::from_raw_parts((value as *const T).cast::<u8>(), size_of::<T>()) }
}
pub unsafe fn proc_send(
    mut peer: *mut tmuxpeer,
    mut type_0: msgtype,
    fd: Option<OwnedFd>,
    data: &[u8],
) -> ::core::ffi::c_int {
    let imsgbuf = &mut (*peer).ibuf;
    if (*peer).flags & PEER_BAD != 0 {
        return -(1 as ::core::ffi::c_int);
    }
    log_debug(format_args!(
        "sending message {} to peer {} ({} bytes)",
        (type_0 as ::core::ffi::c_uint) as i32,
        log_pointer(peer),
        data.len()
    ));
    if imsg_compose(
        imsgbuf,
        type_0,
        PROTOCOL_VERSION as uint32_t,
        -(1 as pid_t),
        fd,
        data,
    )
    .is_err()
    {
        return -(1 as ::core::ffi::c_int);
    }
    proc_update_io(peer);
    0 as ::core::ffi::c_int
}
pub unsafe fn proc_start(name: &CStr) -> Box<tmuxproc> {
    let mut u: utsname = unsafe { ::core::mem::zeroed() };
    log_open(name);
    setproctitle(|out| {
        write_cstr(out, &*name)?;
        out.write_all(b" (")?;
        out.write_all(socket_path_cstr().to_bytes())?;
        out.write_all(b")")
    });
    if uname(&raw mut u) < 0 as ::core::ffi::c_int {
        memset(
            &raw mut u as *mut ::core::ffi::c_void,
            0 as ::core::ffi::c_int,
            ::core::mem::size_of::<utsname>() as size_t,
        );
    }
    log_debug(format_args!(
        "{} started ({}): version {}, socket {}, protocol {}",
        log_cstr(name),
        getpid() as ::core::ffi::c_long,
        log_cstr(getversion()),
        log_cstr(socket_path_cstr()),
        { PROTOCOL_VERSION }
    ));
    log_debug(format_args!(
        "on {} {} {}",
        log_cstr(CStr::from_ptr(
            &raw mut u.sysname as *mut ::core::ffi::c_char
        )),
        log_cstr(CStr::from_ptr(
            &raw mut u.release as *mut ::core::ffi::c_char
        )),
        log_cstr(CStr::from_ptr(
            &raw mut u.version as *mut ::core::ffi::c_char
        ))
    ));
    log_debug(format_args!("using runtime tiri-rt mio"));
    log_debug(format_args!(
        "using utf8proc {}",
        log_cstr(CStr::from_ptr(utf8proc_version()))
    ));
    log_debug(format_args!(
        "using ncurses {} {:06}",
        log_cstr(NCURSES_VERSION),
        (NCURSES_VERSION_PATCH) as u32
    ));
    Box::new(tmuxproc {
        name: name.to_owned(),
        exit: 0,
        signalcb: None,
        signal_task: Default::default(),
        peers: Vec::new(),
    })
}
/// Stop observers and release peers before releasing the process allocation.
/// Drop only after dispatch has returned and no further process use is possible.
impl Drop for tmuxproc {
    fn drop(&mut self) {
        unsafe {
            proc_clear_signals(self, 0);
            while let Some(peer) = self.peers.pop() {
                proc_free_peer(peer);
            }
        }
    }
}

/// Drive the process until exit, then release streams and the runtime owner.
pub unsafe fn proc_loop(
    mut tp: *mut tmuxproc,
    mut runtime: tiri_rt::mio::Runtime,
    mut loopcb: Option<&mut dyn FnMut() -> bool>,
) {
    log_debug(format_args!(
        "{} loop enter",
        crate::src::log::log_bytes((*tp).name.as_bytes())
    ));
    loop {
        runtime.poll(None).expect("tiri-rt poll");
        let exiting = (*tp).exit != 0;
        let drained = (*tp)
            .peers
            .iter()
            .all(|peer| imsgbuf_queuelen(&peer.ibuf) == 0);
        if (exiting && drained) || loopcb.as_mut().is_some_and(|callback| !callback()) {
            break;
        }
    }
    log_debug(format_args!(
        "{} loop exit",
        crate::src::log::log_bytes((*tp).name.as_bytes())
    ));
    reactor::shutdown_runtime(runtime);
}
pub unsafe fn proc_exit(tp: *mut tmuxproc) {
    (*tp).exit = 1;
}
pub unsafe fn proc_set_signals(
    mut tp: *mut tmuxproc,
    mut signalcb: Option<Box<dyn FnMut(ProcessSignal)>>,
) {
    let mut sa: sigaction = unsafe { ::core::mem::zeroed() };
    (*tp).signalcb = signalcb;
    memset(
        &raw mut sa as *mut ::core::ffi::c_void,
        0 as ::core::ffi::c_int,
        ::core::mem::size_of::<sigaction>() as size_t,
    );
    sigemptyset(&raw mut sa.sa_mask);
    sa.sa_flags = SA_RESTART;
    sa.sa_sigaction = SIG_IGN;
    sigaction(SIGPIPE, &raw mut sa, ::core::ptr::null_mut::<sigaction>());
    sigaction(SIGTSTP, &raw mut sa, ::core::ptr::null_mut::<sigaction>());
    sigaction(SIGTTIN, &raw mut sa, ::core::ptr::null_mut::<sigaction>());
    sigaction(SIGTTOU, &raw mut sa, ::core::ptr::null_mut::<sigaction>());
    sigaction(SIGQUIT, &raw mut sa, ::core::ptr::null_mut::<sigaction>());
    crate::src::reactor::task_start(&mut (*tp).signal_task, move || {
        let mut signals = tiri_rt::mio::Signals::new(&[
            SIGINT, SIGHUP, SIGCHLD, SIGCONT, SIGTERM, SIGUSR1, SIGUSR2, SIGWINCH,
        ])?;
        Ok(async move {
            loop {
                let signo = signals.recv().await.expect("process signal wait");
                unsafe { proc_signal_cb(signo, tp) };
                reactor::yield_now().await;
            }
        })
    })
    .expect("start process signals");
}
pub unsafe fn proc_clear_signals(mut tp: *mut tmuxproc, mut defaults: ::core::ffi::c_int) {
    let mut sa: sigaction = unsafe { ::core::mem::zeroed() };
    memset(
        &raw mut sa as *mut ::core::ffi::c_void,
        0 as ::core::ffi::c_int,
        ::core::mem::size_of::<sigaction>() as size_t,
    );
    sigemptyset(&raw mut sa.sa_mask);
    sa.sa_flags = SA_RESTART;
    sa.sa_sigaction = SIG_DFL;
    sigaction(SIGPIPE, &raw mut sa, ::core::ptr::null_mut::<sigaction>());
    sigaction(SIGTSTP, &raw mut sa, ::core::ptr::null_mut::<sigaction>());
    drop((*tp).signal_task.take());
    if defaults != 0 {
        sigaction(SIGINT, &raw mut sa, ::core::ptr::null_mut::<sigaction>());
        sigaction(SIGQUIT, &raw mut sa, ::core::ptr::null_mut::<sigaction>());
        sigaction(SIGHUP, &raw mut sa, ::core::ptr::null_mut::<sigaction>());
        sigaction(SIGCHLD, &raw mut sa, ::core::ptr::null_mut::<sigaction>());
        sigaction(SIGCONT, &raw mut sa, ::core::ptr::null_mut::<sigaction>());
        sigaction(SIGTERM, &raw mut sa, ::core::ptr::null_mut::<sigaction>());
        sigaction(SIGUSR1, &raw mut sa, ::core::ptr::null_mut::<sigaction>());
        sigaction(SIGUSR2, &raw mut sa, ::core::ptr::null_mut::<sigaction>());
        sigaction(SIGWINCH, &raw mut sa, ::core::ptr::null_mut::<sigaction>());
    }
}
pub unsafe fn proc_add_peer(
    mut tp: *mut tmuxproc,
    socket: OwnedFd,
    mut dispatchcb: Box<dyn for<'a> FnMut(PeerMessage<'a>)>,
) -> *mut tmuxpeer {
    let mut owned_peer = Box::new(tmuxpeer::default());
    let peer: *mut tmuxpeer = &mut *owned_peer;
    (*peer).parent = tp;
    (*peer).dispatchcb = Some(dispatchcb);
    let fd = socket.as_raw_fd();
    imsgbuf_init(&mut (*peer).ibuf, socket);
    if getpeereid(fd, &raw mut (*peer).uid, &raw mut (*peer).gid) != 0 as ::core::ffi::c_int {
        (*peer).uid = -(1 as ::core::ffi::c_int) as uid_t;
        (*peer).gid = -(1 as ::core::ffi::c_int) as gid_t;
    }
    log_debug(format_args!("add peer {}: {}", log_pointer(peer), { fd }));
    (*tp).peers.push(owned_peer);
    proc_update_io(peer);
    peer
}
pub unsafe fn proc_remove_peer(peer: *mut tmuxpeer) {
    let peers = &mut (*(*peer).parent).peers;
    let peer_index = peers
        .iter()
        .position(|owned_peer| std::ptr::eq(&**owned_peer, peer))
        .expect("peer must be owned by its parent process");
    let owned_peer = peers.remove(peer_index);
    proc_free_peer(owned_peer);
}

unsafe fn proc_free_peer(mut owned_peer: Box<tmuxpeer>) {
    let peer = &raw mut *owned_peer;
    log_debug(format_args!("remove peer {}", log_pointer(peer)));
    drop((*peer).io_task.take());
    imsgbuf_clear(&mut (*peer).ibuf);
    drop(owned_peer);
}
pub unsafe fn proc_kill_peer(mut peer: *mut tmuxpeer) {
    (*peer).flags |= PEER_BAD;
}
pub unsafe fn proc_toggle_log(mut tp: *mut tmuxproc) {
    log_toggle(&((*tp).name));
}
pub unsafe fn proc_fork_and_daemon() -> (pid_t, OwnedFd) {
    assert!(
        !tiri_rt::mio::Runtime::is_initialized(),
        "daemonize before initializing the runtime"
    );
    let (parent, child) = tiri_rt::unix::socket_pair()
        .unwrap_or_else(|_| fatal(|out| out.write_all(b"socketpair failed")));
    let pid = fork() as pid_t;
    match pid {
        -1 => {
            fatal(|out| out.write_all(b"fork failed"));
        }
        0 => {
            drop(parent);
            if daemon(1 as ::core::ffi::c_int, 0 as ::core::ffi::c_int) != 0 as ::core::ffi::c_int {
                fatal(|out| out.write_all(b"daemon failed"));
            }
            (0, child)
        }
        _ => {
            drop(child);
            (pid, parent)
        }
    }
}
pub unsafe fn proc_get_peer_uid(mut peer: *mut tmuxpeer) -> uid_t {
    (*peer).uid
}
pub unsafe fn proc_get_peer_gid(mut peer: *mut tmuxpeer) -> gid_t {
    (*peer).gid
}

#[cfg(test)]
mod ownership_tests {
    use super::*;
    use std::os::unix::net::UnixStream;

    #[test]
    fn loop_exit_releases_streams_and_runtime_for_both_exit_paths() {
        use crate::src::reactor::{BufferEvent as _, StreamHandle};
        use tiri_rt::Handle as _;

        for callback_exit in [false, true] {
            let runtime = tiri_rt::mio::Runtime::new().unwrap();
            let handle = runtime.handle();
            let mut owner = Box::new(tmuxproc {
                name: c"loop-ownership-test".to_owned(),
                exit: 0,
                signalcb: None,
                signal_task: None,
                peers: Vec::new(),
            });
            let tp = &raw mut *owner;
            let (socket, _remote) = UnixStream::pair().unwrap();
            unsafe {
                let fd = socket.as_raw_fd();
                let flags = libc::fcntl(fd, libc::F_GETFL);
                let stream = reactor::bufferevent_new(fd, None, None, None);
                assert!(!stream.is_null());
                let observer = StreamHandle::from_ptr(stream);
                let task = handle
                    .spawn(async move {
                        if !callback_exit {
                            proc_exit(tp);
                        }
                    })
                    .unwrap();
                let mut callback = || {
                    assert!(tiri_rt::mio::Runtime::is_initialized());
                    false
                };
                let loopcb: Option<&mut dyn FnMut() -> bool> = if callback_exit {
                    Some(&mut callback)
                } else {
                    None
                };
                proc_loop(tp, runtime, loopcb);

                assert!(!observer.is_alive());
                assert_eq!(libc::fcntl(fd, libc::F_GETFL), flags);
                assert!(!tiri_rt::mio::Runtime::is_initialized());
                assert!(matches!(
                    handle.spawn(async {}),
                    Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe
                ));
                drop(task);
                drop(owner);
            }
        }
    }

    #[test]
    fn rejected_messages_close_the_transferred_descriptor() {
        use std::io::Read;
        for (flags, len) in [(PEER_BAD, 0), (0, MAX_IMSGSIZE as usize)] {
            let mut peer = tmuxpeer {
                flags,
                ..Default::default()
            };
            let (socket, mut remote) = UnixStream::pair().unwrap();
            remote.set_nonblocking(true).unwrap();
            assert_eq!(
                unsafe {
                    proc_send(
                        &raw mut peer,
                        MSG_COMMAND,
                        Some(socket.into()),
                        &vec![0; len],
                    )
                },
                -1
            );
            assert_eq!(remote.read(&mut [0]).unwrap(), 0);
        }
    }
}
