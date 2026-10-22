#[cfg(test)]
mod completion_tests;
mod model;
mod stream;
use model::FileRegistration;
pub use model::{client_file, client_file_cb, client_file_event, client_files};

use crate::compat::imsg::imsg;
use crate::compat::imsg::*;
use crate::compat::imsg::{IMSG_HEADER_SIZE, MAX_IMSGSIZE};
use crate::log::{fatalx, log_cstr, log_debug};
use crate::proc::{bytes_of, proc_send};
use crate::reactor::BufferEvent;
use crate::reactor::{
    bufferevent_enable, bufferevent_get_input, bufferevent_new, bufferevent_write, evbuffer_add,
    evbuffer_add_formatted, evbuffer_drain, evbuffer_get_length, evbuffer_pullup,
};

use crate::server_client::Client as _;
use crate::shared::abi::ssize_t;
use crate::shared::abi::*;
use crate::shared::client::ClientRef;
use crate::shared::client::{CLIENT_ATTACHED, CLIENT_CONTROL, CLIENT_DEAD, CLIENT_WRITE_ACK};
use crate::shared::event::*;
use crate::shared::event::{EV_READ, EV_WRITE};
use crate::shared::posix_io::{O_APPEND, O_NONBLOCK, STDERR_FILENO, STDIN_FILENO, STDOUT_FILENO};
use crate::shared::process::tmuxpeer;
use crate::tmux::find_home_cstr;
use libc::{E2BIG, EINVAL, ENOMEM};
use std::cell::RefCell;
use std::ffi::{CStr, CString, OsStr};
use std::io::{self, Write as _};
use std::os::fd::{AsFd, IntoRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt as _;
use std::rc::{Rc, Weak};

#[derive(Copy, Clone)]
#[repr(C)]
pub struct msg_read_open {
    pub stream: ::core::ffi::c_int,
    pub fd: ::core::ffi::c_int,
}
#[derive(Copy, Clone)]
#[repr(C)]
pub struct msg_read_data {
    pub stream: ::core::ffi::c_int,
}
#[derive(Copy, Clone)]
#[repr(C)]
pub struct msg_read_done {
    pub stream: ::core::ffi::c_int,
    pub error: ::core::ffi::c_int,
}
#[derive(Copy, Clone)]
#[repr(C)]
pub struct msg_read_cancel {
    pub stream: ::core::ffi::c_int,
}
#[derive(Copy, Clone)]
#[repr(C)]
pub struct msg_write_open {
    pub stream: ::core::ffi::c_int,
    pub fd: ::core::ffi::c_int,
    pub flags: ::core::ffi::c_int,
}
#[derive(Copy, Clone)]
#[repr(C)]
pub struct msg_write_data {
    pub stream: ::core::ffi::c_int,
}
#[derive(Copy, Clone)]
#[repr(C)]
pub struct msg_write_ready {
    pub stream: ::core::ffi::c_int,
    pub error: ::core::ffi::c_int,
}
#[derive(Copy, Clone)]
#[repr(C)]
pub struct msg_write_close {
    pub stream: ::core::ffi::c_int,
}
#[derive(Copy, Clone)]
#[repr(C)]
pub struct msg_write_done {
    pub stream: ::core::ffi::c_int,
    pub error: ::core::ffi::c_int,
}

pub use ::libc::{EBADF, EIO};

unsafe fn read_imsg_payload<T: Copy>(imsg: &imsg) -> Option<T> {
    if imsg.data.len() < ::core::mem::size_of::<T>() {
        return None;
    }
    Some(::core::ptr::read_unaligned(imsg.data.as_ptr().cast::<T>()))
}

pub const BEV_EVENT_ERROR: ::core::ffi::c_int = 0x20 as ::core::ffi::c_int;
pub const EVBUFFER_ERROR: ::core::ffi::c_int = BEV_EVENT_ERROR;

thread_local! {
    static FILE_NEXT_STREAM: std::cell::Cell<::core::ffi::c_int> = const { std::cell::Cell::new(3) };
}

fn file_set_path(cf: &mut client_file, path: CString) {
    cf.path = Default::default();
    cf.path = Some(path);
}

/// The local path of a file opened by the server itself.
fn file_os_path(cf: &client_file) -> &OsStr {
    OsStr::from_bytes(cf.path.as_deref().expect("file path").to_bytes())
}

/// A transfer's terminal event, as reported to its waiting command.
pub(crate) struct FileOutcome {
    pub(crate) path: Option<CString>,
    pub(crate) error: i32,
    pub(crate) buffer: tiri_buffer::SegmentedBuf,
}

/// A command's wait for a transfer's terminal event. It resolves to `None` when
/// the file's client is dead by then, as tmux never reports to a dead client.
/// Dropping it cancels the wait; the transfer still runs to its terminal
/// completion unobserved.
pub(crate) struct FileWait {
    done: tiri_rt::oneshot::Receiver<FileOutcome>,
    file: Weak<RefCell<client_file>>,
    /// A local transfer's deferred completion, which retains the file.
    local: Option<tiri_rt::mio::Task>,
}

impl std::future::Future for FileWait {
    type Output = Option<FileOutcome>;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<FileOutcome>> {
        std::pin::Pin::new(&mut self.done).poll(cx)
    }
}

impl Drop for FileWait {
    fn drop(&mut self) {
        if let Some(file) = self.file.upgrade() {
            let (waiter, listener) = {
                let mut file = file.borrow_mut();
                (file.wait.take(), file.cb.take())
            };
            drop(waiter);
            drop(listener);
        }
        drop(self.local.take());
    }
}

/// Wait for `file`, whose transfer has started with `done` as its waiter.
fn file_wait(
    file: Rc<RefCell<client_file>>,
    done: tiri_rt::oneshot::Receiver<FileOutcome>,
) -> FileWait {
    let observer = Rc::downgrade(&file);
    let unlinked = matches!(file.borrow().registration, FileRegistration::Unlinked);
    let local = unlinked.then(|| {
        assert!(file.borrow().completed, "local transfers complete at once");
        let mut task = None;
        crate::reactor::task_start(&mut task, move || {
            Ok(async move { file_fire_done_cb(&file) })
        })
        .expect("schedule local file completion");
        task.expect("started task")
    });
    FileWait {
        done,
        file: observer,
        local,
    }
}

fn file_get_path(c: Option<&ClientRef>, file: &CStr) -> CString {
    let file = file.to_bytes();
    let path = if file.starts_with(b"~/") {
        let home = find_home_cstr().map_or(&[][..], CStr::to_bytes);
        [home, &file[1..]].concat()
    } else {
        file.to_vec()
    };
    let full_path = if path.first() == Some(&b'/') {
        path
    } else {
        let cwd = ClientRef::working_directory(c, None).expect("client working directory");
        [cwd.as_bytes(), b"/", path.as_slice()].concat()
    };
    CString::new(full_path).expect("C string path fragments contain no NUL")
}
/// Client-backed transfers resolve protocol delivery through the model. A
/// standalone peer transfer retains its existing explicit peer lifetime.
fn file_send(file: &client_file, kind: msgtype, fd: Option<OwnedFd>, data: &[u8]) -> i32 {
    match file.c.as_ref() {
        Some(client) => client.send_message(kind, fd, data),
        None => proc_send(&file.peer, kind, fd, data),
    }
}

unsafe fn file_create_with_peer(
    peer: &refbox::Weak<tmuxpeer>,
    mut stream: ::core::ffi::c_int,
    mut cb: client_file_cb,
) -> Rc<RefCell<client_file>> {
    let owner = client_file::new();
    {
        let mut cf = owner.borrow_mut();
        cf.c = None;
        cf.stream = stream;
        cf.cb = cb;
        cf.peer = peer.clone();
    }
    crate::client::client_register_file(&owner);
    owner
}
fn file_create_with_client(
    client_owner: Option<&ClientRef>,
    stream: ::core::ffi::c_int,
    cb: client_file_cb,
) -> Rc<RefCell<client_file>> {
    let client_owner =
        client_owner.filter(|owner| owner.flags() & CLIENT_ATTACHED as uint64_t == 0);
    let owner = client_file::new();
    {
        let mut cf = owner.borrow_mut();
        cf.c = client_owner.cloned();
        cf.stream = stream;
        cf.cb = cb;
    }
    if let Some(client_owner) = client_owner {
        client_owner.register_file(&owner);
    }
    owner
}
fn file_fire_done_cb(owner: &Rc<RefCell<client_file>>) {
    let (dead, mut callback, waiter) = {
        let mut file = owner.borrow_mut();
        let dead = file.c.as_ref().is_some_and(|owner| owner.is_dead());
        (dead, file.cb.take(), file.wait.take())
    };
    if !dead && waiter.is_none() {
        if let Some(callback) = callback.as_mut() {
            let (path, error, mut buffer) = {
                let mut file = owner.borrow_mut();
                stream::collect_for_callback(&mut file);
                (
                    file.path.clone(),
                    file.error,
                    std::mem::take(&mut file.buffer),
                )
            };
            callback(client_file_event {
                path: path.as_deref(),
                error,
                closed: true,
                buffer: Some(&mut buffer),
            });
            owner.borrow_mut().buffer = buffer;
        }
    }
    drop(callback);
    client_files_remove(&mut owner.borrow_mut());
    if let Some(waiter) = waiter.filter(|_| !dead) {
        let outcome = {
            let mut file = owner.borrow_mut();
            stream::collect_for_callback(&mut file);
            FileOutcome {
                path: file.path.clone(),
                error: file.error,
                buffer: std::mem::take(&mut file.buffer),
            }
        };
        let _ = waiter.send(outcome);
    }
}
fn file_fire_done(owner: &Rc<RefCell<client_file>>) {
    let wake = {
        let mut file = owner.borrow_mut();
        if file.completed {
            return;
        }
        file.completed = true;
        drop(file.push_task.take());
        stream::finish(&mut file)
    };
    if let Some(wake) = wake {
        wake.wake();
    }
    if matches!(owner.borrow().registration, FileRegistration::Unlinked) {
        return;
    }
    let file = Rc::downgrade(owner);
    crate::reactor::task_start(&mut owner.borrow_mut().done_task, move || {
        Ok(async move {
            if let Some(file) = file.upgrade() {
                let task = file.borrow_mut().done_task.take();
                drop(task);
                {
                    file_fire_done_cb(&file);
                }
            }
        })
    })
    .expect("schedule file completion");
}
fn file_fire_read(file_owner: &Rc<RefCell<client_file>>) {
    let (mut callback, mut buffer, path, error, was_waiting) = {
        let mut cf = file_owner.borrow_mut();
        let dead = cf.c.as_ref().is_some_and(|owner| owner.is_dead());
        if dead || cf.cb.is_none() {
            return;
        }
        stream::collect_for_callback(&mut cf);
        (
            cf.cb.take().unwrap(),
            std::mem::take(&mut cf.buffer),
            cf.path.clone(),
            cf.error,
            cf.wait.is_some(),
        )
    };
    callback(client_file_event {
        path: path.as_deref(),
        error,
        closed: false,
        buffer: Some(&mut buffer),
    });
    let mut cf = file_owner.borrow_mut();
    cf.buffer = buffer;
    if !was_waiting || cf.wait.is_some() {
        cf.cb = Some(callback);
    }
}

pub fn file_can_print(c: Option<&ClientRef>) -> ::core::ffi::c_int {
    c.is_some_and(|c| c.flags() & (CLIENT_ATTACHED | CLIENT_DEAD | CLIENT_CONTROL) as uint64_t == 0)
        as ::core::ffi::c_int
}
pub fn file_print(
    client_owner: Option<&ClientRef>,
    write: impl FnOnce(&mut dyn std::io::Write) -> std::io::Result<()>,
) {
    let Some(client_owner) = client_owner else {
        return;
    };
    let mut find: client_file = client_file::empty();
    let mut msg: msg_write_open = msg_write_open {
        stream: 0,
        fd: 0,
        flags: 0,
    };
    if file_can_print(Some(client_owner)) == 0 {
        return;
    }
    find.stream = 1 as ::core::ffi::c_int;
    let file_owner = client_owner.find_file(find.stream).upgrade();
    match file_owner {
        None => {
            let transfer_owner =
                file_create_with_client(Some(client_owner), 1 as ::core::ffi::c_int, None);
            let mut cf = transfer_owner.borrow_mut();
            file_set_path(&mut cf, CString::new("-").unwrap());
            evbuffer_add_formatted(&mut cf.buffer, write);
            msg.stream = 1 as ::core::ffi::c_int;
            msg.fd = STDOUT_FILENO;
            msg.flags = 0 as ::core::ffi::c_int;
            client_owner.send_message(MSG_WRITE_OPEN, None, bytes_of(&msg));
        }
        Some(file_owner) => {
            evbuffer_add_formatted(&mut file_owner.borrow_mut().buffer, write);
            file_push(&file_owner);
        }
    }
}
pub fn file_print_buffer(client_owner: Option<&ClientRef>, data: &[u8]) {
    let Some(client_owner) = client_owner else {
        return;
    };
    let mut find: client_file = client_file::empty();
    let mut msg: msg_write_open = msg_write_open {
        stream: 0,
        fd: 0,
        flags: 0,
    };
    if file_can_print(Some(client_owner)) == 0 {
        return;
    }
    find.stream = 1 as ::core::ffi::c_int;
    let file_owner = client_owner.find_file(find.stream).upgrade();
    match file_owner {
        None => {
            let transfer_owner =
                file_create_with_client(Some(client_owner), 1 as ::core::ffi::c_int, None);
            let mut cf = transfer_owner.borrow_mut();
            file_set_path(&mut cf, CString::new("-").unwrap());
            evbuffer_add(&mut cf.buffer, data);
            msg.stream = 1 as ::core::ffi::c_int;
            msg.fd = STDOUT_FILENO;
            msg.flags = 0 as ::core::ffi::c_int;
            client_owner.send_message(MSG_WRITE_OPEN, None, bytes_of(&msg));
        }
        Some(file_owner) => {
            evbuffer_add(&mut file_owner.borrow_mut().buffer, data);
            file_push(&file_owner);
        }
    }
}
pub fn file_error(
    client_owner: Option<&ClientRef>,
    write: impl FnOnce(&mut dyn std::io::Write) -> std::io::Result<()>,
) {
    let Some(client_owner) = client_owner else {
        return;
    };
    let mut find: client_file = client_file::empty();
    let mut msg: msg_write_open = msg_write_open {
        stream: 0,
        fd: 0,
        flags: 0,
    };
    if file_can_print(Some(client_owner)) == 0 {
        return;
    }
    find.stream = 2 as ::core::ffi::c_int;
    let file_owner = client_owner.find_file(find.stream).upgrade();
    match file_owner {
        None => {
            let transfer_owner =
                file_create_with_client(Some(client_owner), 2 as ::core::ffi::c_int, None);
            let mut cf = transfer_owner.borrow_mut();
            file_set_path(&mut cf, CString::new("-").unwrap());
            evbuffer_add_formatted(&mut cf.buffer, write);
            msg.stream = 2 as ::core::ffi::c_int;
            msg.fd = STDERR_FILENO;
            msg.flags = 0 as ::core::ffi::c_int;
            client_owner.send_message(MSG_WRITE_OPEN, None, bytes_of(&msg));
        }
        Some(file_owner) => {
            evbuffer_add_formatted(&mut file_owner.borrow_mut().buffer, write);
            file_push(&file_owner);
        }
    }
}

/// Write `bdata` to `path` for a waiting command.
pub(crate) fn file_write_wait(
    client_owner: Option<&ClientRef>,
    path: &CStr,
    flags: ::core::ffi::c_int,
    bdata: &[u8],
) -> FileWait {
    let (waiter, done) = tiri_rt::oneshot::channel();
    let file = file_write_impl(client_owner, path, flags, bdata, waiter);
    file_wait(file, done)
}

fn file_write_impl(
    client_owner: Option<&ClientRef>,
    path: &CStr,
    mut flags: ::core::ffi::c_int,
    bdata: &[u8],
    waiter: tiri_rt::oneshot::Sender<FileOutcome>,
) -> Rc<RefCell<client_file>> {
    let mut current_block: u64;
    let mut msglen: size_t = 0;
    let mut fd: ::core::ffi::c_int = -(1 as ::core::ffi::c_int);
    let fresh0 = FILE_NEXT_STREAM.get();
    FILE_NEXT_STREAM.set(fresh0 + 1);
    let mut stream: u_int = fresh0 as u_int;
    let transfer_owner = file_create_with_client(client_owner, stream as ::core::ffi::c_int, None);
    let mut guard = transfer_owner.borrow_mut();
    let cf = &mut *guard;
    cf.wait = Some(waiter);
    if path == c"-" {
        file_set_path(cf, CString::new("-").unwrap());
        fd = STDOUT_FILENO;
        if client_owner
            .is_none_or(|owner| owner.flags() & (CLIENT_ATTACHED | CLIENT_CONTROL) as uint64_t != 0)
        {
            cf.error = EBADF;
            current_block = 4636144702248558238;
        } else {
            current_block = 8821498768635335055;
        }
    } else {
        file_set_path(cf, file_get_path(client_owner, path));
        if client_owner.is_none_or(|owner| owner.flags() & CLIENT_ATTACHED as uint64_t != 0) {
            let append = flags & O_APPEND != 0;
            match std::fs::OpenOptions::new()
                .create(true)
                .append(append)
                .write(true)
                .truncate(!append)
                .open(file_os_path(cf))
            {
                Err(error) => cf.error = error.raw_os_error().unwrap_or(EIO),
                Ok(file) => {
                    // As tmux's `fwrite` into a stdio buffer of the file's
                    // block size: only a write that overflows it reaches the
                    // file now, and the error of the final flush, which
                    // tmux's ignored `fclose` does, is lost on drop.
                    let capacity = match file.metadata().map(|meta| meta.blksize()) {
                        Ok(size) if size > 0 => size as usize,
                        _ => libc::BUFSIZ as usize,
                    };
                    let mut file = io::BufWriter::with_capacity(capacity, file);
                    if file.write_all(bdata).is_err() {
                        cf.error = EIO;
                    }
                }
            }
            current_block = 4636144702248558238;
        } else {
            current_block = 8821498768635335055;
        }
    }
    if current_block == 8821498768635335055 {
        evbuffer_add(&mut cf.buffer, bdata);
        msglen = cf
            .path
            .as_deref()
            .expect("file path")
            .to_bytes_with_nul()
            .len()
            + ::core::mem::size_of::<msg_write_open>();
        if msglen > (MAX_IMSGSIZE as usize).wrapping_sub(IMSG_HEADER_SIZE) {
            cf.error = E2BIG;
        } else {
            let mut msg = vec![0_u8; msglen];
            let header = msg_write_open {
                stream: cf.stream,
                fd,
                flags,
            };
            let (head, path) = msg.split_at_mut(::core::mem::size_of::<msg_write_open>());
            head.copy_from_slice(bytes_of(&header));
            path.copy_from_slice(cf.path.as_ref().expect("file path").to_bytes_with_nul());
            if file_send(cf, MSG_WRITE_OPEN, None, &msg[..msglen]) != 0 as ::core::ffi::c_int {
                cf.error = EINVAL;
            } else {
                drop(guard);
                return transfer_owner;
            }
        }
    }
    drop(guard);
    file_fire_done(&transfer_owner);
    transfer_owner
}

/// Read `path` for a waiting command. `progress` builds the listener for data
/// as it arrives, after the file is allocated and before any event.
pub(crate) fn file_read_wait(
    client_owner: Option<&ClientRef>,
    path: &CStr,
    progress: impl FnOnce(Weak<RefCell<client_file>>) -> client_file_cb,
) -> FileWait {
    let (waiter, done) = tiri_rt::oneshot::channel();
    let file = file_read_impl(client_owner, path, |file| {
        file.borrow_mut().wait = Some(waiter);
        progress(Rc::downgrade(file))
    });
    file_wait(file, done)
}

/// Start a read whose waiter and listener `init` installs on the new file.
fn file_read_impl(
    client_owner: Option<&ClientRef>,
    path: &CStr,
    init: impl FnOnce(&Rc<RefCell<client_file>>) -> client_file_cb,
) -> Rc<RefCell<client_file>> {
    let mut current_block: u64;
    let mut fd: ::core::ffi::c_int = -(1 as ::core::ffi::c_int);
    let fresh1 = FILE_NEXT_STREAM.get();
    FILE_NEXT_STREAM.set(fresh1 + 1);
    let mut stream: u_int = fresh1 as u_int;
    let transfer_owner = file_create_with_client(client_owner, stream as ::core::ffi::c_int, None);
    transfer_owner.borrow_mut().read.active = true;
    let cb = init(&transfer_owner);
    let mut guard = transfer_owner.borrow_mut();
    let cf = &mut *guard;
    cf.cb = cb;
    if path == c"-" {
        file_set_path(cf, CString::new("-").unwrap());
        fd = STDIN_FILENO;
        if client_owner
            .is_none_or(|owner| owner.flags() & (CLIENT_ATTACHED | CLIENT_CONTROL) as uint64_t != 0)
        {
            cf.error = EBADF;
            current_block = 17369485759464587280;
        } else {
            current_block = 17710118112003399050;
        }
    } else {
        file_set_path(cf, file_get_path(client_owner, path));
        if client_owner.is_none_or(|owner| owner.flags() & CLIENT_ATTACHED as uint64_t != 0) {
            match std::fs::read(file_os_path(cf)) {
                Err(error) => cf.error = error.raw_os_error().unwrap_or(EIO),
                Ok(bytes) => {
                    evbuffer_add(&mut cf.read.input, &bytes);
                }
            }
            current_block = 17369485759464587280;
        } else {
            current_block = 17710118112003399050;
        }
    }
    if current_block == 17710118112003399050 {
        let path = cf.path.as_deref().expect("file path").to_bytes_with_nul();
        let header_len = ::core::mem::size_of::<msg_read_open>();
        let msglen = header_len + path.len();
        if msglen > (MAX_IMSGSIZE as usize).wrapping_sub(IMSG_HEADER_SIZE) {
            cf.error = E2BIG;
        } else {
            let header = msg_read_open {
                stream: cf.stream,
                fd,
            };
            let mut msg = vec![0; msglen];
            msg[..header_len].copy_from_slice(bytes_of(&header));
            msg[header_len..].copy_from_slice(path);
            if file_send(cf, MSG_READ_OPEN, None, &msg[..msglen]) != 0 as ::core::ffi::c_int {
                cf.error = EINVAL;
            } else {
                drop(guard);
                return transfer_owner;
            }
        }
    }
    drop(guard);
    file_fire_done(&transfer_owner);
    transfer_owner
}
pub fn file_cancel(cf: &mut client_file) {
    let mut msg: msg_read_cancel = msg_read_cancel { stream: 0 };
    log_debug(format_args!("read cancel file {}", { cf.stream }));
    if cf.closed != 0 {
        return;
    }
    cf.closed = 1 as ::core::ffi::c_int;
    msg.stream = cf.stream;
    file_send(cf, MSG_READ_CANCEL, None, bytes_of(&msg));
}
fn file_push_cb(owner: &Rc<RefCell<client_file>>) {
    let alive = owner
        .borrow()
        .c
        .as_ref()
        .is_none_or(|owner| owner.flags() & CLIENT_DEAD as uint64_t == 0);
    if alive {
        file_push(owner);
    }
}
fn file_push(file_owner: &Rc<RefCell<client_file>>) {
    let mut guard = file_owner.borrow_mut();
    let cf = &mut *guard;
    drop(cf.push_task.take());
    let mut msg = Vec::<u8>::new();
    let header_len = ::core::mem::size_of::<msg_write_data>();
    let mut sent: size_t = 0;
    let mut left: size_t = 0;
    let mut close_0: msg_write_close = msg_write_close { stream: 0 };
    left = evbuffer_get_length(&cf.buffer);
    while left != 0 as size_t {
        sent = left;
        if sent
            > (MAX_IMSGSIZE as usize)
                .wrapping_sub(IMSG_HEADER_SIZE)
                .wrapping_sub(::core::mem::size_of::<msg_write_data>() as usize)
        {
            sent = (MAX_IMSGSIZE as usize)
                .wrapping_sub(IMSG_HEADER_SIZE)
                .wrapping_sub(::core::mem::size_of::<msg_write_data>() as usize)
                as size_t;
        }
        let msglen = header_len + sent;
        msg.resize(msglen, 0);
        let header = msg_write_data { stream: cf.stream };
        msg[..header_len].copy_from_slice(bytes_of(&header));
        msg[header_len..].copy_from_slice(
            evbuffer_pullup(&mut cf.buffer, sent as ssize_t)
                .map_or(&[][..], |bytes| &bytes[..sent]),
        );
        if file_send(cf, MSG_WRITE, None, &msg[..msglen]) != 0 as ::core::ffi::c_int {
            break;
        }
        evbuffer_drain(&mut cf.buffer, sent);
        left = evbuffer_get_length(&cf.buffer);
        log_debug(format_args!(
            "file {} sent {}, left {}",
            { cf.stream },
            (sent) as usize,
            (left) as usize
        ));
    }
    let mut done = false;
    if left != 0 as size_t {
        let observer = Rc::downgrade(file_owner);
        crate::reactor::task_start(&mut cf.push_task, move || {
            Ok(async move {
                if let Some(owner) = observer.upgrade() {
                    file_push_cb(&owner);
                }
            })
        })
        .expect("retry file output");
    } else if cf.stream > 2 as ::core::ffi::c_int {
        close_0.stream = cf.stream;
        file_send(cf, MSG_WRITE_CLOSE, None, bytes_of(&close_0));
        done =
            cf.c.as_ref()
                .is_none_or(|owner| owner.flags() & CLIENT_WRITE_ACK as uint64_t == 0);
    }
    drop(guard);
    if done {
        file_fire_done(file_owner);
    }
}
pub fn file_write_left(files: &client_files) -> ::core::ffi::c_int {
    let mut left: size_t = 0;
    let mut waiting: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    for file in client_files_iter(files).filter_map(|file| file.upgrade()) {
        let cf = file.borrow();
        if let Some(remaining) = cf
            .event
            .with_stream(|stream| evbuffer_get_length(&stream.output))
        {
            left = remaining;
            if left != 0 as size_t {
                waiting += 1;
                log_debug(format_args!(
                    "file {} {} bytes left",
                    (cf.stream) as u32,
                    (left) as usize
                ));
            }
        }
    }
    (waiting != 0 as ::core::ffi::c_int) as ::core::ffi::c_int
}
/// The listener runs unborrowed, since it may traverse the file index, and
/// stays installed afterwards.
fn file_notify_closed(owner: &Rc<RefCell<client_file>>) {
    let mut callback = owner.borrow_mut().cb.take();
    if let Some(callback) = callback.as_mut() {
        callback(client_file_event {
            path: None,
            error: 0,
            closed: true,
            buffer: None,
        });
    }
    owner.borrow_mut().cb = callback;
}
unsafe fn file_write_finished(owner: &Rc<RefCell<client_file>>) {
    {
        let mut guard = owner.borrow_mut();
        let cf = &mut *guard;
        let mut msg: msg_write_done = msg_write_done {
            stream: 0,
            error: 0,
        };
        std::mem::take(&mut cf.event).free();
        if let Some(fd) = cf.fd.take() {
            if let Err(error) = tiri_rt::unix::close(fd.into_raw_fd()) {
                if cf.error == 0 {
                    cf.error = error.raw_os_error().unwrap_or(libc::EIO);
                }
            }
        }
        msg.stream = cf.stream;
        msg.error = cf.error;
        file_send(cf, MSG_WRITE_DONE, None, bytes_of(&msg));
    }
    file_notify_closed(owner);
    client_files_remove(&mut owner.borrow_mut());
}
unsafe fn file_write_error_callback(
    stream_error: Option<&io::Error>,
    owner: &Rc<RefCell<client_file>>,
) {
    let closed = {
        let mut guard = owner.borrow_mut();
        let cf = &mut *guard;
        let error = stream_error
            .and_then(io::Error::raw_os_error)
            .filter(|&error| error != 0)
            .unwrap_or(EIO);
        log_debug(format_args!("write error file {}", { cf.stream }));
        cf.error = error;
        std::mem::take(&mut cf.event).free();
        drop(cf.fd.take());
        cf.closed != 0
    };
    if closed {
        file_write_finished(owner);
    } else {
        file_notify_closed(owner);
    }
}
unsafe fn file_write_callback(owner: &Rc<RefCell<client_file>>) {
    let finished = {
        let cf = owner.borrow();
        log_debug(format_args!("write check file {}", { cf.stream }));
        let remaining = cf
            .event
            .with_stream(|stream| evbuffer_get_length(&stream.output))
            .unwrap_or(0);
        cf.closed != 0 && remaining == 0 as size_t
    };
    if finished {
        file_write_finished(owner);
    } else {
        file_notify_closed(owner);
    }
}
pub unsafe fn file_write_open(
    peer: &refbox::Weak<tmuxpeer>,
    imsg: &imsg,
    mut close_received: ::core::ffi::c_int,
    mut cb: client_file_cb,
) {
    let transfer_owner;
    let msglen = imsg.data.len();
    if msglen < ::core::mem::size_of::<msg_write_open>() {
        fatalx(|out| out.write_all(b"bad MSG_WRITE_OPEN size"));
    }
    let msg = read_imsg_payload::<msg_write_open>(imsg).unwrap();
    let mut reply: msg_write_ready = msg_write_ready {
        stream: 0,
        error: 0,
    };
    let mut find: client_file = client_file::empty();
    let mut error: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    let path = if msglen == ::core::mem::size_of::<msg_write_open>() {
        c"-"
    } else {
        CStr::from_bytes_until_nul(&imsg.data[::core::mem::size_of::<msg_write_open>()..])
            .unwrap_or_else(|_| fatalx(|out| out.write_all(b"bad MSG_WRITE_OPEN path")))
    };
    log_debug(format_args!(
        "open write file {} {}",
        { msg.stream },
        log_cstr(path)
    ));
    find.stream = msg.stream;
    if crate::client::client_find_file(find.stream).is_some() {
        error = EBADF;
    } else {
        transfer_owner = file_create_with_peer(peer, msg.stream, cb);
        let mut guard = transfer_owner.borrow_mut();
        let cf = &mut *guard;
        if cf.closed != 0 {
            error = EBADF;
        } else {
            let fd = if msg.fd == -1 {
                tiri_rt::unix::open(
                    path,
                    libc::O_WRONLY | libc::O_CREAT | msg.flags | O_NONBLOCK,
                    0o644,
                )
            } else {
                let duplicate = match msg.fd {
                    STDOUT_FILENO => io::stdout().as_fd().try_clone_to_owned(),
                    STDERR_FILENO => io::stderr().as_fd().try_clone_to_owned(),
                    _ => Err(io::Error::from_raw_os_error(EBADF)),
                };
                if close_received != 0 && (msg.fd == STDOUT_FILENO || msg.fd == STDERR_FILENO) {
                    let _ = tiri_rt::unix::close(msg.fd);
                }
                duplicate
            };
            match fd {
                Err(open_error) => error = open_error.raw_os_error().unwrap_or(::libc::EIO),
                Ok(fd) => {
                    cf.fd = Some(fd);
                    let data_observer = Rc::downgrade(&transfer_owner);
                    let error_observer = data_observer.clone();
                    let stream = bufferevent_new(
                        cf.fd.as_ref().map(AsFd::as_fd),
                        None,
                        bufferevent_data_callback(move || unsafe {
                            if let Some(owner) = data_observer.upgrade() {
                                file_write_callback(&owner);
                            }
                        }),
                        bufferevent_event_callback(move |_, stream_error| unsafe {
                            if let Some(owner) = error_observer.upgrade() {
                                file_write_error_callback(stream_error, &owner);
                            }
                        }),
                    );
                    match stream {
                        Err(stream_error) => {
                            error = stream_error.raw_os_error().unwrap_or(EIO);
                            drop(cf.fd.take());
                            client_files_remove(cf);
                        }
                        Ok(stream) => {
                            cf.event = stream.clone();
                            let _ = stream.with_stream(|stream| {
                                bufferevent_enable(stream, EV_WRITE as ::core::ffi::c_short)
                            });
                        }
                    }
                }
            }
        }
    }
    reply.stream = msg.stream;
    reply.error = error;
    proc_send(peer, MSG_WRITE_READY, None, bytes_of(&reply));
}
pub unsafe fn file_write_data(imsg: &imsg) {
    let msglen = imsg.data.len();
    if msglen < ::core::mem::size_of::<msg_write_data>() {
        fatalx(|out| out.write_all(b"bad MSG_WRITE size"));
    }
    let msg = read_imsg_payload::<msg_write_data>(imsg).unwrap();
    let mut find: client_file = client_file::empty();
    let size = msglen - ::core::mem::size_of::<msg_write_data>();
    find.stream = msg.stream;
    let Some(file_owner) = crate::client::client_find_file(find.stream) else {
        fatalx(|out| out.write_all(b"unknown stream number"));
    };
    let cf = file_owner.borrow();
    log_debug(format_args!("write {} to file {}", (size) as usize, {
        cf.stream
    }));
    let _ = cf.event.with_stream(|stream| {
        bufferevent_write(
            stream,
            &imsg.data[::core::mem::size_of::<msg_write_data>()..][..size],
        )
    });
}
pub unsafe fn file_write_close(imsg: &imsg) {
    let msglen = imsg.data.len();
    if msglen != ::core::mem::size_of::<msg_write_close>() {
        fatalx(|out| out.write_all(b"bad MSG_WRITE_CLOSE size"));
    }
    let msg = read_imsg_payload::<msg_write_close>(imsg).unwrap();
    let mut find: client_file = client_file::empty();
    find.stream = msg.stream;
    let Some(file_owner) = crate::client::client_find_file(find.stream) else {
        fatalx(|out| out.write_all(b"unknown stream number"));
    };
    let remaining = {
        let mut cf = file_owner.borrow_mut();
        log_debug(format_args!("close file {}", { cf.stream }));
        cf.closed = 1 as ::core::ffi::c_int;
        cf.event
            .with_stream(|stream| evbuffer_get_length(&stream.output))
            .unwrap_or(0)
    };
    if remaining == 0 as size_t {
        file_write_finished(&file_owner);
    }
}
fn file_read_error_callback(mut what: ::core::ffi::c_short, owner: &Rc<RefCell<client_file>>) {
    let mut guard = owner.borrow_mut();
    let cf = &mut *guard;
    let mut msg: msg_read_done = msg_read_done {
        stream: 0,
        error: 0,
    };
    log_debug(format_args!("read error file {}", { cf.stream }));
    msg.stream = cf.stream;
    msg.error = if what as ::core::ffi::c_int & EVBUFFER_ERROR != 0 {
        EIO
    } else {
        0 as ::core::ffi::c_int
    };
    file_send(cf, MSG_READ_DONE, None, bytes_of(&msg));
    std::mem::take(&mut cf.event).free();
    drop(cf.fd.take());
    client_files_remove(cf);
}
fn file_read_callback(owner: &Rc<RefCell<client_file>>) {
    let cf = owner.borrow();
    let mut msg = Vec::<u8>::new();
    let header_len = ::core::mem::size_of::<msg_read_data>();
    while let Some(chunk) = cf.event.with_stream(|stream| {
        let input = &mut *stream.input;
        let bsize = evbuffer_get_length(input).min(
            (MAX_IMSGSIZE as usize)
                .wrapping_sub(IMSG_HEADER_SIZE)
                .wrapping_sub(header_len),
        );
        evbuffer_pullup(input, bsize as ssize_t)
            .map_or_else(Vec::new, |bytes| bytes[..bsize].to_vec())
    }) {
        let bsize = chunk.len();
        if bsize == 0 {
            break;
        }
        log_debug(format_args!("read {} from file {}", { bsize }, {
            cf.stream
        }));
        let msglen = header_len + bsize;
        msg.resize(msglen, 0);
        let header = msg_read_data { stream: cf.stream };
        msg[..header_len].copy_from_slice(bytes_of(&header));
        msg[header_len..].copy_from_slice(&chunk);
        file_send(&cf, MSG_READ, None, &msg[..msglen]);
        let _ = cf
            .event
            .with_stream(|stream| evbuffer_drain(bufferevent_get_input(&mut *stream), bsize));
    }
}
pub unsafe fn file_read_open(
    peer: &refbox::Weak<tmuxpeer>,
    imsg: &imsg,
    mut close_received: ::core::ffi::c_int,
    mut cb: client_file_cb,
) {
    let transfer_owner;
    let msglen = imsg.data.len();
    if msglen < ::core::mem::size_of::<msg_read_open>() {
        fatalx(|out| out.write_all(b"bad MSG_READ_OPEN size"));
    }
    let msg = read_imsg_payload::<msg_read_open>(imsg).unwrap();
    let mut reply: msg_read_done = msg_read_done {
        stream: 0,
        error: 0,
    };
    let mut find: client_file = client_file::empty();
    let mut error: ::core::ffi::c_int = 0;
    let path = if msglen == ::core::mem::size_of::<msg_read_open>() {
        c"-"
    } else {
        CStr::from_bytes_until_nul(&imsg.data[::core::mem::size_of::<msg_read_open>()..])
            .unwrap_or_else(|_| fatalx(|out| out.write_all(b"bad MSG_READ_OPEN path")))
    };
    log_debug(format_args!(
        "open read file {} {}",
        { msg.stream },
        log_cstr(path)
    ));
    find.stream = msg.stream;
    if crate::client::client_find_file(find.stream).is_some() {
        error = EBADF;
    } else {
        transfer_owner = file_create_with_peer(peer, msg.stream, cb);
        let mut guard = transfer_owner.borrow_mut();
        let cf = &mut *guard;
        if cf.closed != 0 {
            error = EBADF;
        } else {
            let fd = if msg.fd == -1 {
                tiri_rt::unix::open(path, libc::O_RDONLY | O_NONBLOCK, 0)
            } else {
                let duplicate = match msg.fd {
                    STDIN_FILENO => io::stdin().as_fd().try_clone_to_owned(),
                    _ => Err(io::Error::from_raw_os_error(EBADF)),
                };
                if close_received != 0 && (msg.fd == STDIN_FILENO) {
                    let _ = tiri_rt::unix::close(msg.fd);
                }
                duplicate
            };
            match fd {
                Err(open_error) => error = open_error.raw_os_error().unwrap_or(::libc::EIO),
                Ok(fd) => {
                    cf.fd = Some(fd);
                    let data_observer = Rc::downgrade(&transfer_owner);
                    let error_observer = data_observer.clone();
                    let stream = bufferevent_new(
                        cf.fd.as_ref().map(AsFd::as_fd),
                        bufferevent_data_callback(move || {
                            if let Some(owner) = data_observer.upgrade() {
                                file_read_callback(&owner);
                            }
                        }),
                        None,
                        bufferevent_event_callback(move |flags, _| {
                            if let Some(owner) = error_observer.upgrade() {
                                file_read_error_callback(flags, &owner);
                            }
                        }),
                    );
                    match stream {
                        Err(stream_error) => {
                            error = stream_error.raw_os_error().unwrap_or(EIO);
                            drop(cf.fd.take());
                            client_files_remove(cf);
                        }
                        Ok(stream) => {
                            cf.event = stream.clone();
                            let _ = stream.with_stream(|stream| {
                                bufferevent_enable(stream, EV_READ as ::core::ffi::c_short)
                            });
                            return;
                        }
                    }
                }
            }
        }
    }
    reply.stream = msg.stream;
    reply.error = error;
    proc_send(peer, MSG_READ_DONE, None, bytes_of(&reply));
}
pub unsafe fn file_read_cancel(imsg: &imsg) {
    let msglen = imsg.data.len();
    if msglen != ::core::mem::size_of::<msg_read_cancel>() {
        fatalx(|out| out.write_all(b"bad MSG_READ_CANCEL size"));
    }
    let msg = read_imsg_payload::<msg_read_cancel>(imsg).unwrap();
    let mut find: client_file = client_file::empty();
    find.stream = msg.stream;
    let Some(file_owner) = crate::client::client_find_file(find.stream) else {
        fatalx(|out| out.write_all(b"unknown stream number"));
    };
    log_debug(format_args!("cancel file {}", {
        file_owner.borrow().stream
    }));
    file_read_error_callback(0, &file_owner);
}
pub unsafe fn file_write_ready(client: &ClientRef, imsg: &imsg) -> ::core::ffi::c_int {
    let msglen = imsg.data.len();
    if msglen != ::core::mem::size_of::<msg_write_ready>() {
        return -1;
    }
    let msg = read_imsg_payload::<msg_write_ready>(imsg).unwrap();
    let mut find: client_file = client_file::empty();
    find.stream = msg.stream;
    let Some(file_owner) = client.find_file(find.stream).upgrade() else {
        return 0 as ::core::ffi::c_int;
    };
    if msg.error != 0 as ::core::ffi::c_int {
        file_owner.borrow_mut().error = msg.error;
        file_fire_done(&file_owner);
    } else {
        file_push(&file_owner);
    }
    0 as ::core::ffi::c_int
}
pub unsafe fn file_write_done(client: &ClientRef, imsg: &imsg) -> ::core::ffi::c_int {
    let msglen = imsg.data.len();
    if msglen != ::core::mem::size_of::<msg_write_done>() {
        return -1;
    }
    let msg = read_imsg_payload::<msg_write_done>(imsg).unwrap();
    let mut find: client_file = client_file::empty();
    find.stream = msg.stream;
    let Some(file_owner) = client.find_file(find.stream).upgrade() else {
        return 0 as ::core::ffi::c_int;
    };
    {
        let mut cf = file_owner.borrow_mut();
        if cf
            .c
            .as_ref()
            .is_none_or(|owner| owner.flags() & CLIENT_WRITE_ACK as uint64_t == 0)
        {
            return 0 as ::core::ffi::c_int;
        }
        log_debug(format_args!("file {} write done", { cf.stream }));
        cf.error = msg.error;
    }
    file_fire_done(&file_owner);
    0 as ::core::ffi::c_int
}
pub unsafe fn file_read_data(client: &ClientRef, imsg: &imsg) -> ::core::ffi::c_int {
    let msglen = imsg.data.len();
    if msglen < ::core::mem::size_of::<msg_read_data>() {
        return -1;
    }
    let msg = read_imsg_payload::<msg_read_data>(imsg).unwrap();
    let mut find: client_file = client_file::empty();
    let bdata = &imsg.data[::core::mem::size_of::<msg_read_data>()..msglen];
    let bsize = bdata.len();
    find.stream = msg.stream;
    let Some(file_owner) = client.find_file(find.stream).upgrade() else {
        return 0 as ::core::ffi::c_int;
    };
    let mut cf = file_owner.borrow_mut();
    log_debug(format_args!(
        "file {} read {} bytes",
        { cf.stream },
        (bsize) as usize
    ));
    if cf.error == 0 as ::core::ffi::c_int && cf.closed == 0 {
        if evbuffer_add(&mut cf.read.input, bdata) != 0 as ::core::ffi::c_int {
            cf.error = ENOMEM;
            drop(cf);
            file_fire_done(&file_owner);
        } else {
            let wake = stream::take_wake(&mut cf.read);
            drop(cf);
            if let Some(wake) = wake {
                wake.wake();
            }
            file_fire_read(&file_owner);
        }
    }
    0 as ::core::ffi::c_int
}
pub unsafe fn file_read_done(client: &ClientRef, imsg: &imsg) -> ::core::ffi::c_int {
    let msglen = imsg.data.len();
    if msglen != ::core::mem::size_of::<msg_read_done>() {
        return -1;
    }
    let msg = read_imsg_payload::<msg_read_done>(imsg).unwrap();
    let mut find: client_file = client_file::empty();
    find.stream = msg.stream;
    let Some(file_owner) = client.find_file(find.stream).upgrade() else {
        return 0 as ::core::ffi::c_int;
    };
    {
        let mut cf = file_owner.borrow_mut();
        log_debug(format_args!("file {} read done", { cf.stream }));
        cf.error = msg.error;
    }
    file_fire_done(&file_owner);
    0 as ::core::ffi::c_int
}

fn client_files_key(elm: &client_file) -> i32 {
    elm.stream
}
/// Lookups clone the indexed Rc to keep a file alive during an operation.
pub(crate) fn client_files_find_stream(
    head: &client_files,
    stream: i32,
) -> Option<Rc<RefCell<client_file>>> {
    head.get(&stream).cloned()
}

pub(crate) fn client_files_insert(
    head: &mut client_files,
    file: Rc<RefCell<client_file>>,
) -> Option<Rc<RefCell<client_file>>> {
    let key = client_files_key(&file.borrow());
    if let Some(existing) = head.get(&key).cloned() {
        return Some(existing);
    }
    {
        let mut cf = file.borrow_mut();
        cf.registration = if cf.c.is_some() {
            FileRegistration::Client
        } else {
            FileRegistration::Peer
        };
    }
    head.insert(key, file);
    None
}

/// Retire a stream through its existing holder, before releasing file resources.
fn client_files_remove(elm: &mut client_file) {
    let registration = std::mem::take(&mut elm.registration);
    let stream = elm.stream;
    let identity = std::ptr::from_ref(elm);
    unsafe {
        match registration {
            FileRegistration::Unlinked => {}
            FileRegistration::Client => elm
                .c
                .as_ref()
                .expect("registered client file retains its client")
                .unregister_file(stream, identity),
            FileRegistration::Peer => crate::client::client_remove_file(stream, identity),
        }
    }
}

/// Identity prevents a delayed completion from removing a reused stream number.
pub(crate) fn client_files_remove_identity(
    files: &mut client_files,
    stream: i32,
    identity: *const client_file,
) -> Option<Rc<RefCell<client_file>>> {
    if files
        .get(&stream)
        .is_some_and(|file| std::ptr::eq(file.as_ptr(), identity))
    {
        files.remove(&stream)
    } else {
        None
    }
}

pub fn client_files_is_empty(files: &client_files) -> bool {
    files.is_empty()
}

pub(crate) fn client_files_has_pending_data(files: &client_files) -> bool {
    client_files_iter(files).any(|owner| {
        let owner = owner.upgrade().expect("indexed file is live");
        let file = owner.borrow();
        evbuffer_get_length(&file.buffer) != 0 || evbuffer_get_length(&file.read.input) != 0
    })
}

pub(crate) fn client_files_interrupt(
    files: impl Iterator<Item = Weak<RefCell<client_file>>>,
    error: i32,
) {
    for file in files {
        let file = file.upgrade().expect("indexed file is live");
        file.borrow_mut().error = error;
        file_fire_done(&file);
    }
}

/// Snapshot stream order without borrowing an index across callbacks. Weak file
/// identities do not retain transfers; each yield checks the holder's current
/// index, so removing or replacing a stream also removes it from traversal.
pub fn client_files_iter(
    files: &client_files,
) -> impl std::iter::FusedIterator<Item = Weak<RefCell<client_file>>> {
    let entries: Vec<_> = files.values().map(Rc::downgrade).collect();
    entries.into_iter().filter(|entry| {
        let Some(owner) = entry.upgrade() else {
            return false;
        };
        let file = owner.borrow();
        unsafe {
            match file.registration {
                FileRegistration::Unlinked => false,
                FileRegistration::Client => file
                    .c
                    .as_ref()
                    .is_some_and(|client| client.find_file(file.stream).ptr_eq(entry)),
                FileRegistration::Peer => crate::client::client_find_file(file.stream)
                    .is_some_and(|indexed| Rc::ptr_eq(&indexed, &owner)),
            }
        }
    })
}

impl Drop for client_file {
    fn drop(&mut self) {
        client_files_remove(self);
    }
}
