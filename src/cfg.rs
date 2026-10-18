use crate::src::cmd::parse::{cmd_parse_from_buffer, cmd_parse_from_file};
use crate::src::cmd::queue::{
    cmdq_add_format, cmdq_append, cmdq_copy_state, cmdq_get_callback_owned, cmdq_get_client,
    cmdq_get_command, cmdq_get_state, cmdq_insert_after, cmdq_new_state, cmdq_print, cmdq_wait,
    CmdqWaiter,
};
use crate::src::compat::stdio::CFile;
use crate::src::control::control_notify_write;
use crate::src::ffi::libc::__errno_location;
use crate::src::format::bytes::format_message_with;
use crate::src::format::bytes::nullable_cstr;
use crate::src::format::bytes::write_cstr;
use crate::src::log::{log_cstr, log_debug};
use crate::src::prompt_history::prompt_load_history;
use crate::src::server_client::Client as _;
use crate::src::server_client::ClientRegistry;
use crate::src::session::sessions;
use libc::{fopen, strerror};

use crate::src::session::Session as _;
use crate::src::shared::abi::*;
use crate::src::shared::client::CLIENT_CONTROL;
use crate::src::shared::client::{ClientRef, ClientWeak};
use crate::src::shared::command::*;
use crate::src::shared::command::{cmd_find_state, cmdq_item};
use crate::src::shared::command::{cmd_parse_input, cmd_parse_result};
use crate::src::shared::command::{CMD_PARSE_PARSEONLY, CMD_PARSE_QUIET};
use crate::src::shared::errno::ENOENT;
use crate::src::shared::key::key_event;
use crate::src::shared::session::SessionRef;
use crate::src::shared::stdio::FILE;
use crate::src::window::Window as _;
use crate::src::window_copy::{window_copy_add, window_view_mode};
use crate::src::window_pane::WindowPane as _;
use std::collections::VecDeque;
use std::ffi::{CStr, CString};
use std::sync::{Mutex, OnceLock};
pub static mut cfg_client: ClientWeak = std::rc::Weak::new();
pub static mut cfg_finished: ::core::ffi::c_int = 0;
static CFG_CAUSES: Mutex<VecDeque<CString>> = Mutex::new(VecDeque::new());
/// Resumes the startup client's `cfg_client_done` once the configuration is loaded.
static mut CFG_WAITER: Option<CmdqWaiter> = None;
pub static mut cfg_quiet: ::core::ffi::c_int = 1 as ::core::ffi::c_int;
static CFG_FILES: OnceLock<Vec<CString>> = OnceLock::new();

pub(crate) fn cfg_set_files(files: Vec<CString>) {
    assert!(
        CFG_FILES.set(files).is_ok(),
        "configuration paths initialized twice"
    );
}

pub(crate) fn cfg_files() -> &'static [CString] {
    CFG_FILES.get().map_or(&[], Vec::as_slice)
}
unsafe fn cfg_client_done(item: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>) -> CmdOutcome {
    if cfg_finished == 0 {
        let (waiter, retval) = cmdq_wait(item);
        drop(CFG_WAITER.replace(waiter));
        return retval;
    }
    CmdOutcome::Normal
}
unsafe fn cfg_done() -> CmdOutcome {
    if cfg_finished != 0 {
        return CmdOutcome::Normal;
    }
    cfg_finished = 1 as ::core::ffi::c_int;
    cfg_show_causes(None);
    if let Some(waiter) = CFG_WAITER.take() {
        waiter.resume();
    }
    prompt_load_history();
    CmdOutcome::Normal
}
pub unsafe fn start_cfg() {
    let mut c: Option<ClientRef> = None;
    let mut flags: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    let mut registry_c_owner = ClientRegistry::first();
    c = registry_c_owner.clone();
    cfg_client = registry_c_owner
        .as_ref()
        .map_or_else(std::rc::Weak::new, std::rc::Rc::downgrade);
    if !c.is_none() {
        cmdq_append(
            registry_c_owner.as_ref(),
            cmdq_get_callback_owned(
                c"cfg_client_done",
                Some(Box::new(|item| unsafe { cfg_client_done(item) })),
            ),
        );
    }
    if cfg_quiet != 0 {
        flags = CMD_PARSE_QUIET;
    }
    for path in cfg_files() {
        load_cfg(path, registry_c_owner.as_ref(), flags);
    }
    cmdq_append(
        None,
        cmdq_get_callback_owned(c"cfg_done", Some(Box::new(|_| unsafe { cfg_done() }))),
    );
}
pub unsafe fn load_cfg(
    path: &CStr,
    c: Option<&ClientRef>,
    mut flags: ::core::ffi::c_int,
) -> ::core::ffi::c_int {
    let mut f: *mut FILE = ::core::ptr::null_mut::<FILE>();
    let mut pi: cmd_parse_input = cmd_parse_input {
        flags: 0,
        file: None,
        line: 0,
        item: std::rc::Weak::new(),
        c: Default::default(),
        fs: cmd_find_state {
            flags: 0,
            s: Default::default(),
            wl: Default::default(),
            w: Default::default(),
            wp: Default::default(),
            idx: 0,
        },
    };
    let mut pr: cmd_parse_result = cmd_parse_result::empty();
    let state;

    log_debug(format_args!("loading {}", log_cstr(path)));
    f = fopen(path.as_ptr(), c"rb".as_ptr());
    if f.is_null() {
        if *__errno_location() == ENOENT && flags & CMD_PARSE_QUIET != 0 {
            return 0 as ::core::ffi::c_int;
        }
        cfg_add_cause(|out| {
            write_cstr(out, nullable_cstr(path.as_ptr()))?;
            out.write_all(b": ")?;
            write_cstr(out, nullable_cstr(strerror(*__errno_location())))
        });
        return -(1 as ::core::ffi::c_int);
    }
    let stream = CFile::from_raw(f).expect("fopen returned a non-null stream");
    pi.flags = flags;
    pi.file = Some(path.to_owned());
    pi.line = 1 as u_int;
    pi.set_item(None);
    pi.c = c.map_or_else(std::rc::Weak::new, std::rc::Rc::downgrade);
    pr = cmd_parse_from_file(stream.as_ptr(), &raw mut pi);
    drop(stream);
    if pr.status as ::core::ffi::c_uint
        == CMD_PARSE_ERROR as ::core::ffi::c_int as ::core::ffi::c_uint
    {
        cfg_add_cause(|out| write_cstr(out, pr.error.as_deref()));
        return -(1 as ::core::ffi::c_int);
    }
    if flags & CMD_PARSE_PARSEONLY != 0 {
        drop(pr.cmdlist.take());
        return 0 as ::core::ffi::c_int;
    }

    state = cmdq_new_state(
        ::core::ptr::null_mut::<cmd_find_state>(),
        ::core::ptr::null_mut::<key_event>(),
        0 as ::core::ffi::c_int,
    );

    cmdq_add_format(
        &state,
        c"current_file",
        pi.file.as_deref().unwrap_or(c"(null)"),
    );
    let new_item0_allocation = cmdq_get_command(
        pr.cmdlist.as_ref().expect("successful command parse"),
        Some(&state),
    );

    cmdq_append(None, new_item0_allocation);

    drop(pr.cmdlist.take());

    0 as ::core::ffi::c_int
}
pub unsafe fn load_cfg_from_buffer(
    buf: &[u8],
    path: &CStr,
    c: Option<&ClientRef>,
    item_handle: Option<&std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>>,
    mut current: *mut cmd_find_state,
    mut flags: ::core::ffi::c_int,
    mut new_item: Option<&mut std::rc::Weak<std::cell::UnsafeCell<cmdq_item>>>,
) -> ::core::ffi::c_int {
    let item = item_handle.map_or(std::ptr::null_mut(), |item| item.get());
    let mut pi: cmd_parse_input = cmd_parse_input {
        flags: 0,
        file: None,
        line: 0,
        item: std::rc::Weak::new(),
        c: Default::default(),
        fs: cmd_find_state {
            flags: 0,
            s: Default::default(),
            wl: Default::default(),
            w: Default::default(),
            wp: Default::default(),
            idx: 0,
        },
    };
    let mut pr: cmd_parse_result = cmd_parse_result::empty();

    if let Some(new_item) = new_item.as_deref_mut() {
        *new_item = std::rc::Weak::new();
    }
    log_debug(format_args!("loading {}", log_cstr(path)));
    pi.flags = flags;
    pi.file = Some(path.to_owned());
    pi.line = 1 as u_int;
    pi.set_item(item_handle);
    pi.c = c.map_or_else(std::rc::Weak::new, std::rc::Rc::downgrade);
    pr = cmd_parse_from_buffer(buf, &raw mut pi);
    if pr.status as ::core::ffi::c_uint
        == CMD_PARSE_ERROR as ::core::ffi::c_int as ::core::ffi::c_uint
    {
        cfg_add_cause(|out| write_cstr(out, pr.error.as_deref()));
        return -(1 as ::core::ffi::c_int);
    }
    if flags & CMD_PARSE_PARSEONLY != 0 {
        drop(pr.cmdlist.take());
        return 0 as ::core::ffi::c_int;
    }
    let state = if !item.is_null() {
        cmdq_copy_state(
            cmdq_get_state(&*item).expect("command queue state"),
            current,
        )
    } else {
        cmdq_new_state(
            ::core::ptr::null_mut::<cmd_find_state>(),
            ::core::ptr::null_mut::<key_event>(),
            0 as ::core::ffi::c_int,
        )
    };
    cmdq_add_format(
        &state,
        c"current_file",
        pi.file.as_deref().unwrap_or(c"(null)"),
    );
    let new_item0_allocation = cmdq_get_command(
        pr.cmdlist.as_ref().expect("successful command parse"),
        Some(&state),
    );
    let last = if let Some(item) = item_handle {
        cmdq_insert_after(item, new_item0_allocation)
    } else {
        cmdq_append(None, new_item0_allocation)
    };
    drop(pr.cmdlist.take());

    if let Some(new_item) = new_item {
        *new_item = last;
    }
    0 as ::core::ffi::c_int
}
pub unsafe fn cfg_add_cause(write: impl FnOnce(&mut dyn std::io::Write) -> std::io::Result<()>) {
    let msg = format_message_with(write);
    CFG_CAUSES.lock().unwrap().push_back(msg);
}

fn cfg_drain_causes(mut deliver: impl FnMut(&CStr)) {
    loop {
        let cause = CFG_CAUSES.lock().unwrap().pop_front();
        let Some(cause) = cause else { break };
        deliver(&cause);
    }
}

pub unsafe fn cfg_print_causes(item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>) {
    let item = item_handle.get();
    let c_owner = cmdq_get_client((item).as_ref());
    let mut c: Option<ClientRef> = c_owner.clone();
    cfg_drain_causes(|cause| {
        if !c.is_none()
            && c.as_ref().expect("live client").flags() & CLIENT_CONTROL as uint64_t != 0
        {
            control_notify_write(&c.clone().expect("live client"), |out| {
                out.write_all(b"%config-error ")?;
                write_cstr(out, &*cause)
            });
        } else {
            cmdq_print(item_handle, |out| write_cstr(out, &*cause));
        }
    });
}
pub unsafe fn cfg_show_causes(s_owner: Option<&SessionRef>) {
    let mut s = s_owner.cloned();
    let mut registry_c_owner = ClientRegistry::first();
    let mut c: Option<ClientRef> = registry_c_owner.clone();
    if CFG_CAUSES.lock().unwrap().is_empty() {
        return;
    }
    if !c.is_none() && c.as_ref().expect("live client").flags() & CLIENT_CONTROL as uint64_t != 0 {
        cfg_drain_causes(|cause| {
            control_notify_write(&c.clone().expect("live client"), |out| {
                out.write_all(b"%config-error ")?;
                write_cstr(out, &*cause)
            });
        });
    } else {
        if s.is_none() {
            s = c
                .as_ref()
                .and_then(|c| c.attached_session().upgrade())
                .or_else(|| sessions::first());
        }
        if s.is_none() || !s.as_ref().expect("live session").is_attached() {
            return;
        }
        let pane_owner = s
            .as_ref()
            .expect("live session")
            .current_winlink()
            .get_unchecked()
            .window_handle()
            .expect("live window")
            .active_pane()
            .upgrade()
            .expect("view-mode pane");
        if !pane_owner.is_mode(&window_view_mode) {
            pane_owner.set_mode(None, &window_view_mode, None, None, None);
        }
        cfg_drain_causes(|cause| {
            window_copy_add(&pane_owner, 0 as ::core::ffi::c_int, |out| {
                write_cstr(out, &*cause)
            });
        });
    }
}

#[cfg(test)]
mod cfg_wait_tests {
    use super::*;
    use crate::src::cmd::queue::{cmdq_len, cmdq_next};
    use crate::src::options::options_default;
    use crate::src::options::options_table;
    use crate::src::server_client::queue_test_clients as clients;
    use crate::src::shared::client::CLIENT_DEAD;
    use crate::src::tmux::with_global_options;
    use std::cell::{Cell, UnsafeCell};
    use std::rc::{Rc, Weak};

    /// An unfinished config, and server options with no history file for
    /// `cfg_done`.
    struct Options;
    impl Options {
        unsafe fn new() -> Self {
            let entry = options_table
                .iter()
                .find(|entry| entry.name == Some(c"history-file"))
                .unwrap();
            with_global_options(|oo| {
                options_default(oo, entry);
            });
            cfg_finished = 0;
            Self
        }
    }
    impl Drop for Options {
        fn drop(&mut self) {
            unsafe {
                drop(CFG_WAITER.take());
            }
        }
    }

    struct SetOnDrop(Rc<Cell<bool>>);
    impl Drop for SetOnDrop {
        fn drop(&mut self) {
            self.0.set(true);
        }
    }

    /// Queue `cfg_client_done` on `owner`, then an item recording whether it
    /// ran or was released.
    unsafe fn startup_queue(
        owner: &ClientRef,
        ran: &Rc<Cell<bool>>,
        released: &Rc<Cell<bool>>,
    ) -> Weak<UnsafeCell<cmdq_item>> {
        let head = cmdq_append(
            Some(owner),
            cmdq_get_callback_owned(
                c"cfg_client_done",
                Some(Box::new(|item| unsafe { cfg_client_done(item) })),
            ),
        );
        let (ran, capture) = (ran.clone(), SetOnDrop(released.clone()));
        cmdq_append(
            Some(owner),
            cmdq_get_callback_owned(
                c"after config",
                Some(Box::new(move |_| {
                    let _capture = &capture;
                    ran.set(true);
                    CmdOutcome::Normal
                })),
            ),
        );
        head
    }

    #[test]
    fn cfg_done_resumes_the_waiting_startup_client() {
        unsafe {
            let _options = Options::new();
            let owner = clients::with_queue();
            let (ran, released) = (Rc::new(Cell::new(false)), Rc::new(Cell::new(false)));
            let head = startup_queue(&owner, &ran, &released);
            assert_eq!(cmdq_next(Some(&owner)), 0);
            assert!(CFG_WAITER.as_ref().is_some_and(|w| !w.is_cancelled()));
            assert!(!ran.get());

            assert!(matches!(cfg_done(), CmdOutcome::Normal));
            assert_eq!(cfg_finished, 1);
            assert!(CFG_WAITER.is_none());
            assert!(!ran.get());
            assert_eq!(cmdq_next(Some(&owner)), 1);
            assert!(head.upgrade().is_none());
            assert!(ran.get());
        }
    }

    #[test]
    fn the_startup_client_does_not_wait_for_a_finished_config() {
        unsafe {
            let _options = Options::new();
            cfg_finished = 1;
            let owner = clients::with_queue();
            let (ran, released) = (Rc::new(Cell::new(false)), Rc::new(Cell::new(false)));
            startup_queue(&owner, &ran, &released);
            assert_eq!(cmdq_next(Some(&owner)), 2);
            assert!(ran.get());
            assert!(CFG_WAITER.is_none());
        }
    }

    #[test]
    fn a_replaced_waiter_is_dropped_without_resuming() {
        unsafe {
            let _options = Options::new();
            let (first, second) = (clients::with_queue(), clients::with_queue());
            let first_ran = Rc::new(Cell::new(false));
            let first_head = startup_queue(&first, &first_ran, &Rc::new(Cell::new(false)));
            assert_eq!(cmdq_next(Some(&first)), 0);
            let second_ran = Rc::new(Cell::new(false));
            startup_queue(&second, &second_ran, &Rc::new(Cell::new(false)));
            assert_eq!(cmdq_next(Some(&second)), 0);

            let _ = cfg_done();
            assert_eq!(cmdq_next(Some(&second)), 1);
            assert!(second_ran.get());
            assert_eq!(cmdq_next(Some(&first)), 0);
            assert!(first_head.upgrade().is_some());
            assert_eq!(cmdq_len(Some(&first)), 2);
            assert!(!first_ran.get());
        }
    }

    #[test]
    fn a_freed_startup_client_is_not_retained_and_cfg_done_skips_it() {
        unsafe {
            let _options = Options::new();
            let owner = clients::with_queue();
            let (ran, released) = (Rc::new(Cell::new(false)), Rc::new(Cell::new(false)));
            let head = startup_queue(&owner, &ran, &released);
            assert_eq!(cmdq_next(Some(&owner)), 0);
            owner.update_flags(CLIENT_DEAD as u64, 0);
            assert_eq!(cmdq_next(Some(&owner)), 0);

            let client = Rc::downgrade(&owner);
            drop(owner);
            assert!(client.upgrade().is_none());
            assert!(head.upgrade().is_none());
            assert!(released.get());
            assert!(CFG_WAITER.as_ref().is_some_and(CmdqWaiter::is_cancelled));

            let _ = cfg_done();
            assert!(CFG_WAITER.is_none());
            assert!(!ran.get());
        }
    }
}
