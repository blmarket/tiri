use crate::compat::imsg::imsg;
use crate::compat::imsg::PROTOCOL_VERSION;
use crate::compat::imsg::*;
use crate::compat::imsg::{
    imsg_compose, imsgbuf_clear, imsgbuf_get, imsgbuf_init, imsgbuf_output, imsgbuf_queuelen,
    imsgbuf_receive, imsgbuf_written,
};
use crate::compat::setproctitle::setproctitle;
use crate::format::bytes::write_cstr;
use crate::log::{fatal, log_cstr, log_debug, log_open, log_pointer, log_toggle};
use crate::reactor;
use crate::shared::abi::*;
use crate::shared::abi::{gid_t, uid_t, uint32_t};
use crate::shared::process::PeerMessage;
use crate::shared::process::{tmuxpeer, tmuxproc};
use crate::shared::signal::ProcessSignal;
pub use crate::shared::signal::{
    SA_RESTART, SIGCHLD, SIGCONT, SIGHUP, SIGINT, SIGTERM, SIGTSTP, SIGTTIN, SIGTTOU, SIGUSR1,
    SIGUSR2, SIGWINCH, SIG_DFL, SIG_IGN,
};
use crate::tmux::{getversion, socket_path_cstr};
use refbox::RefBox;
use std::ffi::{CStr, CString};
use std::os::fd::{AsFd, AsRawFd, OwnedFd};
use tiri_sys::{set_signal_disposition, SignalDisposition};
use tiri_rt::{AsyncRead as _, AsyncWrite as _, Runtime as _, Signals as _};

pub use ::libc::{SIGPIPE, SIGQUIT};

pub const NCURSES_VERSION_PATCH: ::core::ffi::c_int = 20251230 as ::core::ffi::c_int;
pub const NCURSES_VERSION: &std::ffi::CStr = c"6.6";

pub const PEER_BAD: ::core::ffi::c_int = 0x1 as ::core::ffi::c_int;
/// Borrow the peer for one step. A removed peer yields `None`; peer borrows
/// never span a callback, so a nested borrow is a logic error.
fn with_peer<R>(peer: &refbox::Weak<tmuxpeer>, step: impl FnOnce(&mut tmuxpeer) -> R) -> Option<R> {
    match peer.try_borrow_mut() {
        Ok(mut peer) => Some(step(&mut peer)),
        Err(refbox::BorrowError::Dropped) => None,
        Err(refbox::BorrowError::Borrowed) => panic!("peer already borrowed"),
    }
}
/// The callback runs unborrowed and may remove the peer, which drops it.
fn proc_dispatch(peer: &refbox::Weak<tmuxpeer>, message: PeerMessage<'_>) -> bool {
    let Some(mut dispatchcb) = with_peer(peer, |peer| peer.dispatchcb.take()).flatten() else {
        return false;
    };
    dispatchcb(message);
    with_peer(peer, |peer| peer.dispatchcb = Some(dispatchcb)).is_some()
}
fn proc_disconnect(peer: &refbox::Weak<tmuxpeer>) {
    let io_task = with_peer(peer, |peer| {
        peer.flags |= PEER_BAD;
        imsgbuf_clear(&mut peer.ibuf);
        peer.io_task.take()
    });
    drop(io_task);
    proc_dispatch(peer, PeerMessage::Disconnected);
}

fn proc_receive(peer: &refbox::Weak<tmuxpeer>, bytes: &[u8], received: tiri_rt::Received) -> bool {
    if received.bytes == 0
        || with_peer(peer, |peer| {
            imsgbuf_receive(&mut peer.ibuf, &bytes[..received.bytes], received.fd).is_ok()
        }) != Some(true)
    {
        proc_disconnect(peer);
        return false;
    }
    while let Some(mut message) = with_peer(peer, |peer| imsgbuf_get(&mut peer.ibuf)).flatten() {
        log_debug(format_args!(
            "peer {} message {}",
            log_pointer(peer.as_ptr()),
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

fn proc_signal_cb(signo: ::core::ffi::c_int, tp: &refbox::Weak<tmuxproc>) {
    let mut signalcb = tp
        .try_borrow_mut()
        .expect("process exist")
        .signalcb
        .take()
        .expect("signal callback is installed");
    signalcb(ProcessSignal::from_raw(signo));
    if let Ok(mut tp) = tp.try_borrow_mut() {
        tp.signalcb.get_or_insert(signalcb);
    }
}

fn peer_check_version(peer: &refbox::Weak<tmuxpeer>, imsg: &imsg) -> ::core::ffi::c_int {
    let mut version: ::core::ffi::c_int = 0;
    version = (imsg.hdr.peerid & 0xff as uint32_t) as ::core::ffi::c_int;
    if imsg.hdr.type_0 != MSG_VERSION && version != PROTOCOL_VERSION {
        log_debug(format_args!(
            "peer {} bad version {}",
            log_pointer(peer.as_ptr()),
            (version) as i32
        ));
        proc_send(peer, MSG_VERSION, None, &[]);
        with_peer(peer, |peer| peer.flags |= PEER_BAD);
        return -(1 as ::core::ffi::c_int);
    }
    0 as ::core::ffi::c_int
}
fn proc_update_io(peer: &refbox::Weak<tmuxpeer>) {
    with_peer(peer, |state| {
        let writable = imsgbuf_queuelen(&state.ibuf) > 0;
        if state.ibuf.fd.is_none() || (state.io_task.is_some() && state.io_writable == writable) {
            return;
        }
        state.io_writable = writable;
        let fd = state.ibuf.fd.as_ref().expect("peer socket").as_fd();
        let peer = peer.clone();
        drop(state.io_task.take());
        state.io_task = Some(
            reactor::task_start(move || {
                let source = reactor::io(fd)?;
                Ok(proc_peer_io(peer, source))
            })
            .expect("start peer I/O"),
        );
    });
}
/// Owned by the peer's `io_task`, so it is cancelled when the peer is removed.
async fn proc_peer_io(peer: refbox::Weak<tmuxpeer>, source: tiri_rt::mio::Io) {
    use std::future::{poll_fn, Future};
    use std::pin::pin;
    use std::task::Poll;
    enum Completion {
        Read(std::io::Result<tiri_rt::Received>),
        Write(std::io::Result<usize>),
    }
    let mut prefer_write = false;
    loop {
        let Some((reading, output)) = with_peer(&peer, |peer| {
            (peer.flags & PEER_BAD == 0, imsgbuf_output(&peer.ibuf))
        }) else {
            return;
        };
        let Ok((output, fd)) = output else {
            proc_disconnect(&peer);
            return;
        };
        if !reading && output.is_empty() {
            proc_disconnect(&peer);
            return;
        }
        let mut input = vec![0; 65536];
        let buffers = [std::io::IoSlice::new(&output)];
        let completion = {
            let fd = fd.as_ref().map(AsFd::as_fd);
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
        match completion {
            Completion::Read(Ok(received)) => {
                prefer_write = true;
                if !proc_receive(&peer, &input, received) {
                    return;
                }
            }
            Completion::Write(Ok(n)) if n > 0 => {
                prefer_write = false;
                with_peer(&peer, |peer| imsgbuf_written(&mut peer.ibuf, n));
            }
            Completion::Write(Err(error)) if error.raw_os_error() == Some(libc::ENOBUFS) => {
                tiri_rt::mio::Sleep::new(
                    std::time::Instant::now() + std::time::Duration::from_millis(1),
                )
                .await
                .ok();
            }
            _ => {
                proc_disconnect(&peer);
                return;
            }
        }
        let Some(drained) = with_peer(&peer, |peer| {
            peer.flags & PEER_BAD != 0 && imsgbuf_queuelen(&peer.ibuf) == 0
        }) else {
            return;
        };
        if drained {
            proc_disconnect(&peer);
            return;
        }
        proc_update_io(&peer);
        reactor::yield_now().await;
    }
}
/// The string payload of a message, which must end in a NUL.
pub fn imsg_string(imsg: &imsg) -> Option<&CStr> {
    if imsg.data.last() != Some(&0) {
        return None;
    }
    CStr::from_bytes_until_nul(&imsg.data).ok()
}
/// A value sent on the wire as its in-memory layout: its fields' native-endian
/// bytes in order, `SIZE` bytes in all.
pub trait PlainMessage: Sized {
    const SIZE: usize;
    fn write_to(&self, out: &mut [u8]);
    fn read_from(data: &[u8]) -> Self;
}
impl PlainMessage for i32 {
    const SIZE: usize = size_of::<i32>();
    fn write_to(&self, out: &mut [u8]) {
        out.copy_from_slice(&self.to_ne_bytes());
    }
    fn read_from(data: &[u8]) -> Self {
        Self::from_ne_bytes(data.try_into().expect("message field size"))
    }
}
impl PlainMessage for u64 {
    const SIZE: usize = size_of::<u64>();
    fn write_to(&self, out: &mut [u8]) {
        out.copy_from_slice(&self.to_ne_bytes());
    }
    fn read_from(data: &[u8]) -> Self {
        Self::from_ne_bytes(data.try_into().expect("message field size"))
    }
}
/// Implement [`PlainMessage`] for `repr(C)` structs of `c_int` fields, whose
/// layout has no padding.
macro_rules! int_message {
    ($($message:ident { $($field:ident),+ })+) => {$(
        impl $crate::proc::PlainMessage for $message {
            const SIZE: usize = [$(stringify!($field)),+].len() * size_of::<::core::ffi::c_int>();
            fn write_to(&self, out: &mut [u8]) {
                let mut fields = out.chunks_exact_mut(size_of::<::core::ffi::c_int>());
                $($crate::proc::PlainMessage::write_to(&self.$field, fields.next().expect("message field"));)+
            }
            fn read_from(data: &[u8]) -> Self {
                let mut fields = data.chunks_exact(size_of::<::core::ffi::c_int>());
                Self {
                    $($field: $crate::proc::PlainMessage::read_from(fields.next().expect("message field")),)+
                }
            }
        }
        const _: () = assert!(<$message as $crate::proc::PlainMessage>::SIZE == size_of::<$message>());
    )+};
}
pub(crate) use int_message;
/// The bytes of a plain message, as sent on the wire.
pub fn bytes_of<T: PlainMessage>(value: &T) -> Vec<u8> {
    let mut bytes = vec![0; T::SIZE];
    value.write_to(&mut bytes);
    bytes
}
/// A plain message from the start of `data`, if it is long enough.
pub fn read_plain<T: PlainMessage>(data: &[u8]) -> Option<T> {
    data.get(..T::SIZE).map(T::read_from)
}
pub fn proc_send(
    peer: &refbox::Weak<tmuxpeer>,
    mut type_0: msgtype,
    fd: Option<OwnedFd>,
    data: &[u8],
) -> ::core::ffi::c_int {
    // A lost client's peer is removed before the client is freed.
    let composed = with_peer(peer, |peer| {
        if peer.flags & PEER_BAD != 0 {
            return false;
        }
        log_debug(format_args!(
            "sending message {} to peer {} ({} bytes)",
            (type_0 as ::core::ffi::c_uint) as i32,
            log_pointer(&*peer),
            data.len()
        ));
        imsg_compose(
            &mut peer.ibuf,
            type_0,
            PROTOCOL_VERSION as uint32_t,
            -(1 as pid_t),
            fd,
            data,
        )
        .is_ok()
    });
    if composed != Some(true) {
        return -(1 as ::core::ffi::c_int);
    }
    proc_update_io(peer);
    0 as ::core::ffi::c_int
}
pub fn proc_start(name: &CStr) -> RefBox<tmuxproc> {
    log_open(name);
    setproctitle(|out| {
        write_cstr(out, &*name)?;
        out.write_all(b" (")?;
        out.write_all(socket_path_cstr().to_bytes())?;
        out.write_all(b")")
    });
    log_debug(format_args!(
        "{} started ({}): version {}, socket {}, protocol {}",
        log_cstr(name),
        std::process::id(),
        log_cstr(getversion()),
        log_cstr(&socket_path_cstr()),
        { PROTOCOL_VERSION }
    ));
    let field = |field: &[::core::ffi::c_char]| {
        let bytes: Vec<u8> = field
            .iter()
            .map(|&byte| byte as u8)
            .take_while(|&byte| byte != 0)
            .collect();
        CString::new(bytes).expect("bytes before NUL")
    };
    let (sysname, release, version) = match tiri_sys::uname() {
        Ok(u) => (field(&u.sysname), field(&u.release), field(&u.version)),
        Err(_) => Default::default(),
    };
    log_debug(format_args!(
        "on {} {} {}",
        log_cstr(&sysname),
        log_cstr(&release),
        log_cstr(&version)
    ));
    log_debug(format_args!("using runtime tiri-rt mio"));
    log_debug(format_args!(
        "using utf8proc {}",
        log_cstr(crate::ffi::utf8proc::version())
    ));
    log_debug(format_args!(
        "using ncurses {} {:06}",
        log_cstr(NCURSES_VERSION),
        (NCURSES_VERSION_PATCH) as u32
    ));
    RefBox::new(tmuxproc {
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
        clear_signals(self, 0);
        while let Some(peer) = self.peers.pop() {
            proc_free_peer(peer);
        }
    }
}

/// Drive the process until exit, then release streams and the runtime owner.
pub fn proc_loop(
    tp: &refbox::Weak<tmuxproc>,
    mut runtime: tiri_rt::mio::Runtime,
    mut loopcb: Option<&mut dyn FnMut() -> bool>,
) {
    log_debug(format_args!(
        "{} loop enter",
        crate::log::log_bytes(tp.try_borrow().expect("process exist").name.as_bytes())
    ));
    loop {
        runtime.poll(None).expect("tiri-rt poll");
        let exiting = tp.try_borrow().expect("process exist").exit != 0;
        let drained = tp
            .try_borrow()
            .expect("process exist")
            .peers
            .iter()
            .all(|peer| {
                peer.try_access_mut(|peer| imsgbuf_queuelen(&peer.ibuf) == 0)
                    .expect("peers are unborrowed between polls")
            });
        if (exiting && drained) || loopcb.as_mut().is_some_and(|callback| !callback()) {
            break;
        }
    }
    log_debug(format_args!(
        "{} loop exit",
        crate::log::log_bytes(tp.try_borrow().expect("process exist").name.as_bytes())
    ));
    reactor::shutdown_runtime(runtime);
}

pub fn proc_exit(tp: &refbox::Weak<tmuxproc>) {
    tp.try_borrow_mut().expect("process exist").exit = 1;
}

pub fn proc_set_signals(
    tp: &refbox::Weak<tmuxproc>,
    signalcb: Option<Box<dyn FnMut(ProcessSignal)>>,
) {
    let mut process = tp.try_borrow_mut().expect("process exist");
    process.signalcb = signalcb;
    for signal in [SIGPIPE, SIGTSTP, SIGTTIN, SIGTTOU, SIGQUIT] {
        let _ = set_signal_disposition(signal, SignalDisposition::Ignore);
    }
    drop(process.signal_task.take());
    let tp = tp.clone();
    process.signal_task = Some(
        crate::reactor::task_start(move || {
            let mut signals = tiri_rt::mio::Signals::new(&[
                SIGINT, SIGHUP, SIGCHLD, SIGCONT, SIGTERM, SIGUSR1, SIGUSR2, SIGWINCH,
            ])?;
            Ok(async move {
                loop {
                    let signo = signals.recv().await.expect("process signal wait");
                    proc_signal_cb(signo, &tp);
                    reactor::yield_now().await;
                }
            })
        })
        .expect("start process signals"),
    );
}
pub fn proc_clear_signals(tp: &refbox::Weak<tmuxproc>, defaults: ::core::ffi::c_int) {
    clear_signals(&mut tp.try_borrow_mut().expect("process exist"), defaults);
}
fn clear_signals(tp: &mut tmuxproc, defaults: ::core::ffi::c_int) {
    for signal in [SIGPIPE, SIGTSTP] {
        let _ = set_signal_disposition(signal, SignalDisposition::Default);
    }
    drop(tp.signal_task.take());
    if defaults != 0 {
        for signal in [
            SIGINT, SIGQUIT, SIGHUP, SIGCHLD, SIGCONT, SIGTERM, SIGUSR1, SIGUSR2, SIGWINCH,
        ] {
            let _ = set_signal_disposition(signal, SignalDisposition::Default);
        }
    }
}
pub fn proc_add_peer(
    tp: &refbox::Weak<tmuxproc>,
    socket: OwnedFd,
    mut dispatchcb: Box<dyn for<'a> FnMut(PeerMessage<'a>)>,
) -> refbox::Weak<tmuxpeer> {
    let fd = socket.as_raw_fd();
    let (uid, gid) = tiri_sys::peer_credentials(socket.as_fd()).unwrap_or((
        -(1 as ::core::ffi::c_int) as uid_t,
        -(1 as ::core::ffi::c_int) as gid_t,
    ));
    let mut state = tmuxpeer {
        parent: tp.clone(),
        uid,
        gid,
        dispatchcb: Some(dispatchcb),
        ..Default::default()
    };
    imsgbuf_init(&mut state.ibuf, socket);
    let owned_peer = refbox::RefBox::new(state);
    let peer = owned_peer.downgrade();
    log_debug(format_args!(
        "add peer {}: {}",
        log_pointer(owned_peer.as_ptr()),
        { fd }
    ));
    tp.try_borrow_mut()
        .expect("process exist")
        .peers
        .push(owned_peer);
    proc_update_io(&peer);
    peer
}
pub fn proc_remove_peer(peer: &refbox::Weak<tmuxpeer>) {
    let parent = with_peer(peer, |peer| peer.parent.clone()).expect("removed peer is live");
    let owned_peer = {
        let mut parent = parent.try_borrow_mut().expect("process exist");
        let peer_index = parent
            .peers
            .iter()
            .position(|owned_peer| peer.is(owned_peer))
            .expect("peer must be owned by its parent process");
        parent.peers.remove(peer_index)
    };
    proc_free_peer(owned_peer);
}

// Consumes the peer's owner so the free stays explicit at the call site.
fn proc_free_peer(owned_peer: refbox::RefBox<tmuxpeer>) {
    log_debug(format_args!(
        "remove peer {}",
        log_pointer(owned_peer.as_ptr())
    ));
}
pub fn proc_kill_peer(peer: &refbox::Weak<tmuxpeer>) {
    with_peer(peer, |peer| peer.flags |= PEER_BAD).expect("killed peer is live");
}
pub fn proc_toggle_log(tp: &refbox::Weak<tmuxproc>) {
    log_toggle(&tp.try_borrow().expect("process exist").name);
}
/// Fork the server and detach it; the server child gets pid 0.
pub fn proc_fork_and_daemon() -> (pid_t, OwnedFd) {
    assert!(
        !tiri_rt::mio::Runtime::is_initialized(),
        "daemonize before initializing the runtime"
    );
    let (parent, child) = tiri_sys::socket_pair()
        .unwrap_or_else(|error| fatal(&error, |out| out.write_all(b"socketpair failed")));
    match tiri_sys::fork() {
        Err(error) => fatal(&error, |out| out.write_all(b"fork failed")),
        Ok(None) => {
            drop(parent);
            if let Err(error) = tiri_sys::daemonize() {
                fatal(&error, |out| out.write_all(b"daemon failed"));
            }
            (0, child)
        }
        Ok(Some(pid)) => {
            drop(child);
            (pid, parent)
        }
    }
}
pub fn proc_get_peer_uid(peer: &refbox::Weak<tmuxpeer>) -> uid_t {
    with_peer(peer, |peer| peer.uid).expect("queried peer is live")
}
pub fn proc_get_peer_gid(peer: &refbox::Weak<tmuxpeer>) -> gid_t {
    with_peer(peer, |peer| peer.gid).expect("queried peer is live")
}

#[cfg(test)]
mod ownership_tests {
    use super::*;
    use std::os::unix::net::UnixStream;

    #[test]
    fn rejected_messages_close_the_transferred_descriptor() {
        use std::io::Read;
        for (flags, len) in [(PEER_BAD, 0), (0, MAX_IMSGSIZE as usize)] {
            let owned_peer = refbox::RefBox::new(tmuxpeer {
                flags,
                ..Default::default()
            });
            let (socket, mut remote) = UnixStream::pair().unwrap();
            remote.set_nonblocking(true).unwrap();
            assert_eq!(
                proc_send(
                    &owned_peer.downgrade(),
                    MSG_COMMAND,
                    Some(socket.into()),
                    &vec![0; len],
                ),
                -1
            );
            assert_eq!(remote.read(&mut [0]).unwrap(), 0);
        }
    }
}
