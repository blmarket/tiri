use crate::environ::environ_create;
use crate::format::bytes::format_message_with;
use crate::log::{log_cstr, log_debug, log_hex};
use crate::names::default_window_name_cstring;
use crate::options::options_get_string;
use crate::options::OptionsScope;
use crate::server_client::Client as _;
use crate::window::Window as _;
use libc::{mkstemp, unlink};

use crate::session::Session;
use crate::shared::client::ClientRef;
use crate::shared::spawn::spawn_context;
use crate::shared::window::WindowRef;

use crate::window_pane::WindowPane;
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

use crate::shared::pane::window_pane;
use crate::shared::spawn::SPAWN_KILL;
use crate::shared::spawn::{spawn_editor_state, spawn_finish_edit_cb};
use crate::shared::window::winlink;

use std::fs::File;
use std::io::Write as _;
use std::os::fd::{FromRawFd, OwnedFd};

impl spawn_editor_state {
    pub(crate) fn new(path: CString, cb: spawn_finish_edit_cb) -> Box<Self> {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT_EDITOR_ID: AtomicU64 = AtomicU64::new(1);
        let id = NEXT_EDITOR_ID
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .expect("editor identity exhausted");
        Box::new(spawn_editor_state {
            id: crate::shared::spawn::EditorId(id),
            path,
            pid: 0,
            cb,
        })
    }
}

pub use ::libc::IUTF8;

pub const _PATH_DEFPATH: &std::ffi::CStr = c"/usr/bin:/bin";

pub const _PATH_TMP: &std::ffi::CStr = c"/tmp/";

pub(crate) unsafe fn spawn_log(from: &CStr, sc: &spawn_context) {
    let session_owner = sc.s.upgrade().expect("spawn context session");
    let mut wl: refbox::Weak<winlink> = sc.winlink_handle();
    let wp0_owner = sc.wp0.upgrade();
    let name = sc.name.as_deref().unwrap_or(c"none");
    log_debug(format_args!(
        "{}: name={}, flags={}",
        log_cstr(from),
        log_cstr(name),
        log_hex(((sc.flags) as u32) as u64)
    ));
    let tmp = format_message_with(|out| match (wl.is_alive(), wp0_owner.as_ref()) {
        (true, Some(wp0)) => write!(out, "wl={} wp0=%{}", { wl.get_unchecked().idx }, wp0.id()),
        (true, None) => write!(out, "wl={} wp0=none", { wl.get_unchecked().idx }),
        (false, Some(wp0)) => write!(out, "wl=none wp0=%{}", wp0.id()),
        (false, None) => out.write_all(b"wl=none wp0=none"),
    });
    log_debug(format_args!(
        "{}: s=${} {} idx={}",
        log_cstr(from),
        session_owner.id(),
        log_cstr(&tmp),
        { sc.idx }
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
    sc: &mut spawn_context,
    cause: &mut Option<CString>,
) -> bool {
    let window = sc
        .winlink_handle()
        .get_unchecked()
        .window_handle()
        .cloned()
        .expect("live respawn window");
    let result = (|| {
        if !sc.flags & SPAWN_KILL != 0
            && window
                .pane_snapshot()
                .iter()
                .filter_map(std::rc::Weak::upgrade)
                .any(|pane| pane.has_tty())
        {
            set_spawn_cause(
                Some(&mut *cause),
                &[
                    b"window ",
                    sc.s.upgrade().expect("spawn session").name().as_bytes(),
                    b":",
                    (sc.winlink_handle())
                        .get_unchecked()
                        .idx
                        .to_string()
                        .as_bytes(),
                    b" still active",
                ],
            );
            return false;
        }
        let source_pane_owner = window
            .step_pane(None, false)
            .upgrade()
            .expect("respawn window has a pane");
        sc.wp0 = std::rc::Rc::downgrade(&source_pane_owner);
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
pub(crate) unsafe fn initialize_spawned_window(sc: &spawn_context, window: &WindowRef) {
    if let Some(name) = sc.name.as_ref() {
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
pub unsafe fn spawn_editor_finish(wp_owner: &std::rc::Rc<std::cell::RefCell<window_pane>>) {
    wp_owner.finish_editing();
}

/// Write the existing bytes before editor pane creation can dispatch events.
/// Empty input fails, as tmux's single-item fwrite of zero bytes does.
pub(crate) fn spawn_editor_write(stream: &mut File, bytes: &[u8]) -> bool {
    !bytes.is_empty() && stream.write_all(bytes).is_ok()
}

pub(crate) unsafe fn spawn_editor(
    client_owner: &ClientRef,
    write: impl FnOnce(&mut File) -> bool,
    mut cb: spawn_finish_edit_cb,
) -> Option<crate::shared::spawn::EditorHandle> {
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
        let mut path: [u8; 19] = *b"/tmp/tmux.XXXXXXXX\0";
        let mut fd: ::core::ffi::c_int = 0;
        let editor = OptionsScope::GlobalServer.with_local(|oo| options_get_string(oo, c"editor"));
        fd = mkstemp(path.as_mut_ptr().cast());
        if fd == -(1 as ::core::ffi::c_int) {
            return None;
        }
        let mut stream = File::from(OwnedFd::from_raw_fd(fd));
        if !write(&mut stream) {
            drop(stream);
            unlink(path.as_ptr().cast());
            return None;
        }
        drop(stream);
        let path = CStr::from_bytes_with_nul(&path).expect("temporary path is terminated");
        let mut owner = spawn_editor_state::new(path.to_owned(), cb);
        es = &raw mut *owner;
        let source = original_window
            .active_pane()
            .upgrade()
            .expect("editor source pane");
        let cmd = CString::new([editor.as_bytes(), b" ", path.to_bytes()].concat())
            .expect("editor command components contain no NUL");
        sc.s = std::rc::Rc::downgrade(&session_owner);
        sc.set_wl(wl.clone());
        sc.tc = std::rc::Rc::downgrade(client_owner);
        sc.wp0 = std::rc::Rc::downgrade(&source);
        sc.argv = vec![cmd];
        sc.environ = Some(environ_create());
        sc.idx = -(1 as ::core::ffi::c_int);
        sc.cwd = Some(c"/tmp/".to_owned());
        let pane = <std::rc::Rc<std::cell::RefCell<window_pane>> as WindowPane>::spawn_process(
            &mut sc, &mut None,
        )?;
        Some(pane.install_editor(owner))
    })();
    drop(original_window);
    result
}
