use crate::src::compat::stdio::CFile;
use crate::src::environ::environ_create;
use crate::src::ffi::libc::{__errno_location, fdopen, fwrite, mkstemp, unlink};
use crate::src::format::bytes::xformat;
use crate::src::log::{log_cstr, log_debug, log_hex};
use crate::src::names::default_window_name_cstring;
use crate::src::options::options_get_string;
use crate::src::server_client::Client as _;
use crate::src::window::Window as _;

use crate::src::session::Session;
use crate::src::shared::client::ClientRef;
use crate::src::shared::spawn::spawn_context;
use crate::src::shared::window::WindowRef;
use crate::src::tmux::global_options;

use crate::src::window_pane::WindowPane;
use std::ffi::{CStr, CString};

pub(crate) fn set_spawn_cause(cause: Option<&mut Option<CString>>, parts: &[&[u8]]) {
    let Some(cause) = cause else {
        return;
    };
    let mut bytes = Vec::with_capacity(parts.iter().map(|part| part.len()).sum());
    for part in parts {
        bytes.extend_from_slice(part);
    }
    *cause = Some(CString::new(bytes).expect("spawn diagnostic contains no NUL"));
}

use crate::src::shared::abi::__off_t;
use crate::src::shared::abi::*;
use crate::src::shared::pane::window_pane;
use crate::src::shared::spawn::SPAWN_KILL;
use crate::src::shared::spawn::{spawn_editor_state, spawn_finish_edit_cb};
use crate::src::shared::stdio::FILE;
use crate::src::shared::window::winlink;

use std::os::fd::{AsRawFd, FromRawFd, IntoRawFd, OwnedFd};

pub type off_t = __off_t;

pub type uintmax_t = ::libc::uintmax_t;

impl spawn_editor_state {
    pub(crate) fn new(path: CString, cb: spawn_finish_edit_cb) -> Box<Self> {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT_EDITOR_ID: AtomicU64 = AtomicU64::new(1);
        let id = NEXT_EDITOR_ID
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .expect("editor identity exhausted");
        Box::new(spawn_editor_state {
            id: crate::src::shared::spawn::EditorId(id),
            path,
            pid: 0,
            cb,
        })
    }
}

pub use ::libc::{IUTF8, SEEK_END, SEEK_SET};

pub const _PATH_DEFPATH: &std::ffi::CStr = c"/usr/bin:/bin";

pub const _PATH_TMP: &std::ffi::CStr = c"/tmp/";

pub(crate) unsafe fn spawn_log(from: &CStr, mut sc: *mut spawn_context) {
    let session_owner = (*sc).s.upgrade().expect("spawn context session");
    let mut wl: refbox::Weak<winlink> = (*sc).winlink_handle();
    let wp0_owner = (*sc).wp0.upgrade();
    let name = (*sc).name.as_deref().unwrap_or(c"none");
    let mut tmp: [u8; 128] = [0; 128];
    log_debug(format_args!(
        "{}: name={}, flags={}",
        log_cstr(from),
        log_cstr(name),
        log_hex((((*sc).flags) as u32) as u64)
    ));
    match (wl.is_alive(), wp0_owner.as_ref()) {
        (true, Some(wp0)) => {
            xformat(
                &mut tmp,
                format_args!("wl={} wp0=%{}", { wl.get_unchecked().idx }, wp0.id()),
            );
        }
        (true, None) => {
            xformat(
                &mut tmp,
                format_args!("wl={} wp0=none", { wl.get_unchecked().idx }),
            );
        }
        (false, Some(wp0)) => {
            xformat(&mut tmp, format_args!("wl=none wp0=%{}", wp0.id()));
        }
        (false, None) => {
            xformat(&mut tmp, format_args!("wl=none wp0=none"));
        }
    }
    log_debug(format_args!(
        "{}: s=${} {} idx={}",
        log_cstr(from),
        session_owner.id(),
        log_cstr(CStr::from_bytes_until_nul(&tmp).expect("xformat output is terminated")),
        { (*sc).idx }
    ));
}

pub unsafe fn spawn_window(
    sc: &mut spawn_context,
    cause: &mut Option<CString>,
) -> refbox::Weak<winlink> {
    let session = sc.s.upgrade().expect("spawn context session");
    match session.spawn_window(sc) {
        Ok(link) => link,
        Err(error) => {
            *cause = Some(error);
            refbox::Weak::new()
        }
    }
}
/// Reset an existing window's panes before its process is respawned.
/// Session link mutation remains in the Session-owned spawn transaction.
pub(crate) unsafe fn prepare_respawn_window(
    sc: *mut spawn_context,
    cause: *mut Option<CString>,
) -> bool {
    let window = (*sc)
        .winlink_handle()
        .get_unchecked()
        .window_handle()
        .cloned()
        .expect("live respawn window");
    let result = (|| {
        if !(*sc).flags & SPAWN_KILL != 0 {
            if window
                .pane_snapshot()
                .iter()
                .filter_map(std::rc::Weak::upgrade)
                .any(|pane| pane.has_tty())
            {
                set_spawn_cause(
                    cause.as_mut(),
                    &[
                        b"window ",
                        (*sc).s.upgrade().expect("spawn session").name().as_bytes(),
                        b":",
                        ((*sc).winlink_handle())
                            .get_unchecked()
                            .idx
                            .to_string()
                            .as_bytes(),
                        b" still active",
                    ],
                );
                return false;
            }
        }
        let source_pane_owner = window
            .step_pane(None, false)
            .upgrade()
            .expect("respawn window has a pane");
        (*sc).wp0 = std::rc::Rc::downgrade(&source_pane_owner);
        // The window keeps only the source pane, and selects it.
        let rest = window
            .pane_snapshot()
            .iter()
            .filter_map(std::rc::Weak::upgrade)
            .filter(|pane| !std::rc::Rc::ptr_eq(pane, &source_pane_owner))
            .collect::<Vec<_>>();
        window
            .rearrange_panes(std::slice::from_ref(&source_pane_owner))
            .expect("a removal is never refused");
        for pane in rest {
            pane.destroy();
        }
        window.select_pane(&source_pane_owner, false);
        true
    })();
    drop(window);
    result
}
/// Initialize the newly-created window's name and fill cells without emitting
/// a rename notification (the window-created notification follows spawning).
pub(crate) unsafe fn initialize_spawned_window(sc: *mut spawn_context, window: &WindowRef) {
    if let Some(name) = (*sc).name.as_ref() {
        window.initialize_name(name.clone(), true);
    } else {
        window.initialize_name(default_window_name_cstring(window), false);
    }
    window.refresh_fill_cells();
}
impl Drop for spawn_editor_state {
    fn drop(&mut self) {
        unsafe {
            unlink(self.path.as_ptr());
        }
    }
}
pub unsafe fn spawn_cancel_editor(mut es: *mut spawn_editor_state) {
    if es.is_null() {
        return;
    }
    (*es).cb = None;
}
pub unsafe fn spawn_get_editor_pid(mut es: *mut spawn_editor_state) -> pid_t {
    if es.is_null() {
        return -(1 as pid_t);
    }
    (*es).pid
}
pub unsafe fn spawn_editor_finish(wp_owner: &std::rc::Rc<std::cell::UnsafeCell<window_pane>>) {
    wp_owner.finish_editing();
}

/// Open the editor's temporary descriptor as a C `FILE` while keeping its
/// ownership explicit at the Rust/stdio boundary.
///
/// `fd_owner` owns the live descriptor returned by `mkstemp`. On `fdopen`
/// failure, `FILE` never owns the descriptor and `OwnedFd::drop` closes it
/// exactly once. On success, `into_raw_fd` relinquishes the Rust owner before
/// the caller can `fclose` the stream, making `FILE` the sole closer.
pub(crate) unsafe fn spawn_editor_fdopen(fd_owner: OwnedFd) -> *mut FILE {
    let mode: &CStr = c"w";
    let file = fdopen(fd_owner.as_raw_fd(), mode.as_ptr());
    if file.is_null() {
        // Keep the fdopen errno available to the caller while the Rust owner
        // closes the descriptor on this failure path.
        let fdopen_errno = *__errno_location();
        drop(fd_owner);
        *__errno_location() = fdopen_errno;
        return ::core::ptr::null_mut::<FILE>();
    }
    let _fd_owned_by_file = fd_owner.into_raw_fd();
    file
}

/// Write the existing bytes before editor pane creation can dispatch events.
/// Keep the fwrite item-count behavior for empty input and short writes.
pub(crate) fn spawn_editor_write(stream: &CFile, bytes: &[u8]) -> bool {
    unsafe { fwrite(bytes.as_ptr().cast(), bytes.len(), 1, stream.as_ptr()) == 1 }
}

pub(crate) unsafe fn spawn_editor(
    client_owner: &ClientRef,
    write: impl FnOnce(&CFile) -> bool,
    mut cb: spawn_finish_edit_cb,
) -> Option<crate::src::shared::spawn::EditorHandle> {
    let session_owner = client_owner.attached_session().upgrade()?;
    let mut es: *mut spawn_editor_state = ::core::ptr::null_mut::<spawn_editor_state>();
    let mut sc: spawn_context = spawn_context {
        item: std::rc::Weak::new(),
        s: std::rc::Weak::new(),
        wl: refbox::Weak::new(),
        tc: std::rc::Weak::new(),
        wp0: std::rc::Weak::new(),
        name: None,
        argv: Vec::new(),
        environ: None,
        idx: 0,
        cwd: None,
        flags: 0,
    };
    let mut wl: refbox::Weak<winlink> = session_owner.current_winlink();
    let original_window = wl
        .get_unchecked()
        .window_handle()
        .cloned()
        .expect("editor window");
    let result = (|| {
        let mut f: *mut FILE = ::core::ptr::null_mut::<FILE>();
        let mut path: [::core::ffi::c_char; 19] =
            ::core::mem::transmute::<[u8; 19], [::core::ffi::c_char; 19]>(*b"/tmp/tmux.XXXXXXXX\0");
        let mut editor: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
        let mut fd: ::core::ffi::c_int = 0;
        let editor_value = options_get_string(global_options, c"editor");
        editor = editor_value.as_ptr();
        fd = mkstemp(&raw mut path as *mut ::core::ffi::c_char);
        if fd == -(1 as ::core::ffi::c_int) {
            return None;
        }
        let fd_owner = OwnedFd::from_raw_fd(fd);
        f = spawn_editor_fdopen(fd_owner);
        if f.is_null() {
            unlink(&raw mut path as *mut ::core::ffi::c_char);
            return None;
        }
        let stream = CFile::from_raw(f).expect("fdopen returned a non-null stream");
        if !write(&stream) {
            drop(stream);
            unlink(&raw mut path as *mut ::core::ffi::c_char);
            return None;
        }
        drop(stream);
        let mut owner = spawn_editor_state::new(CStr::from_ptr(path.as_ptr()).to_owned(), cb);
        es = &raw mut *owner;
        // The editor runs in a temporary pane beside the active one; it
        // closes when the editor exits and selection returns to that pane.
        let source = original_window
            .active_pane()
            .upgrade()
            .expect("editor source pane");
        let cmd = CString::new(
            [
                CStr::from_ptr(editor).to_bytes(),
                b" ",
                CStr::from_ptr(path.as_ptr()).to_bytes(),
            ]
            .concat(),
        )
        .expect("editor command components contain no NUL");
        sc.s = std::rc::Rc::downgrade(&session_owner);
        sc.set_wl(wl.clone());
        sc.tc = std::rc::Rc::downgrade(client_owner);
        sc.wp0 = std::rc::Rc::downgrade(&source);
        sc.argv = vec![cmd];
        sc.environ = Some(environ_create());
        sc.idx = -(1 as ::core::ffi::c_int);
        sc.cwd = Some(c"/tmp/".to_owned());
        // A full strip or a failed spawn drops the owner, removing the file.
        let pane = <std::rc::Rc<std::cell::UnsafeCell<window_pane>> as WindowPane>::spawn_process(
            &mut sc, &mut None,
        )?;
        Some(pane.install_editor(owner))
    })();
    drop(original_window);
    result
}
