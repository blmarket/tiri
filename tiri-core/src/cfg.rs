use crate::actions::queue::{
    callback, client as context_client, enqueue, new_state, print as context_print, program_task,
    wait as context_wait,
};
use crate::actions::sources::compile_file;
use crate::actions::{
    Completion, ContextRef, ContextSource, PARSE_ONLY, PARSE_QUIET, TaskOutcome,
};
use crate::control::control_notify_write;
use crate::format::bytes::{format_message_with, write_cstr};
use crate::log::{log_cstr, log_debug};
use crate::prompt_history::prompt_load_history;
use crate::server_client::Client as _;
use crate::server_client::ClientRegistry;
use crate::session::Session as _;
use crate::session::sessions;
use crate::shared::abi::uint64_t;
use crate::shared::client::{CLIENT_CONTROL, ClientRef, ClientWeak};
use crate::shared::errno::ErrorText;
use crate::shared::session::SessionRef;
use crate::window::Window as _;
use crate::window_copy::{window_copy_add, window_view_mode};
use crate::window_pane::WindowPane as _;
use crate::winlink::winlink_window;
use std::collections::VecDeque;
use std::ffi::{CStr, CString, OsStr};
use std::os::unix::ffi::OsStrExt;
use std::sync::{Mutex, OnceLock};

thread_local! {
    static CFG_CLIENT: std::cell::Cell<ClientWeak> = const { std::cell::Cell::new(std::rc::Weak::new()) };
    static CFG_FINISHED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static CFG_QUIET: std::cell::Cell<bool> = const { std::cell::Cell::new(true) };
    static CFG_WAITER: std::cell::RefCell<Option<Completion>> = const { std::cell::RefCell::new(None) };
}
static CFG_CAUSES: Mutex<VecDeque<CString>> = Mutex::new(VecDeque::new());
static CFG_FILES: OnceLock<Vec<CString>> = OnceLock::new();

pub fn cfg_set_files(files: Vec<CString>) {
    assert!(
        CFG_FILES.set(files).is_ok(),
        "configuration paths initialized twice"
    );
}

pub fn cfg_files() -> &'static [CString] {
    CFG_FILES.get().map_or(&[], Vec::as_slice)
}

pub fn cfg_report_missing() {
    CFG_QUIET.set(false);
}

pub fn cfg_finished() -> bool {
    CFG_FINISHED.get()
}

pub fn cfg_client() -> Option<ClientRef> {
    let observer = CFG_CLIENT.take();
    let client = observer.upgrade();
    CFG_CLIENT.set(observer);
    client
}

pub fn cfg_forget_client(client: &ClientRef) {
    let observer = CFG_CLIENT.take();
    if !observer.ptr_eq(&std::rc::Rc::downgrade(client)) {
        CFG_CLIENT.set(observer);
    }
}

fn cfg_client_done(item: &ContextRef) -> TaskOutcome {
    if !cfg_finished() {
        let (waiter, retval) = context_wait(item);
        drop(CFG_WAITER.replace(Some(waiter)));
        return retval;
    }
    TaskOutcome::Normal
}

fn cfg_done() -> TaskOutcome {
    if cfg_finished() {
        return TaskOutcome::Normal;
    }
    CFG_FINISHED.set(true);
    cfg_show_causes(None);
    let waiter = CFG_WAITER.take();
    if let Some(waiter) = waiter {
        waiter.resume();
    }
    prompt_load_history();
    TaskOutcome::Normal
}

pub fn start_cfg() {
    let registry_c_owner = ClientRegistry::first();
    CFG_CLIENT.set(
        registry_c_owner
            .as_ref()
            .map_or_else(std::rc::Weak::new, std::rc::Rc::downgrade),
    );
    if registry_c_owner.is_some() {
        enqueue(
            registry_c_owner.as_ref(),
            callback(c"cfg_client_done", Some(Box::new(cfg_client_done))),
        );
    }
    let flags = if CFG_QUIET.get() { PARSE_QUIET } else { 0 };
    for path in cfg_files() {
        load_cfg(path, registry_c_owner.as_ref(), flags);
    }
    enqueue(None, callback(c"cfg_done", Some(Box::new(|_| cfg_done()))));
}

pub fn load_cfg(
    path: &CStr,
    c: Option<&ClientRef>,
    flags: ::core::ffi::c_int,
) -> ::core::ffi::c_int {
    log_debug(format_args!("loading {}", log_cstr(path)));
    let mut bytes = Vec::new();
    match std::fs::File::open(OsStr::from_bytes(path.to_bytes())) {
        Ok(mut file) => {
            let _ = std::io::Read::read_to_end(&mut file, &mut bytes);
        }
        Err(error) => {
            if error.kind() == std::io::ErrorKind::NotFound && flags & PARSE_QUIET != 0 {
                return 0;
            }
            cfg_add_cause(|out| {
                write_cstr(out, Some(&*path))?;
                out.write_all(b": ")?;
                write!(out, "{}", ErrorText(&error))
            });
            return -1;
        }
    }
    let commands = match compile_file(&bytes, path, c, flags) {
        Ok(commands) => commands,
        Err(error) => {
            cfg_add_cause(|out| write_cstr(out, error.as_c_str()));
            return -1;
        }
    };
    if flags & PARSE_ONLY != 0 {
        drop(commands);
        return 0;
    }
    let state = new_state(None, None, 0);
    state.add_format(c"current_file", path);
    let task = program_task(&commands, Some(&state));
    enqueue(None, task);
    drop(commands);
    0
}

pub fn cfg_add_cause(write: impl FnOnce(&mut dyn std::io::Write) -> std::io::Result<()>) {
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

pub fn cfg_print_causes(item: &dyn ContextSource) {
    let c = context_client(Some(&*item.borrow()));
    cfg_drain_causes(|cause| {
        if let Some(client) = c
            .as_ref()
            .filter(|client| client.flags() & CLIENT_CONTROL as uint64_t != 0)
        {
            control_notify_write(client, |out| {
                out.write_all(b"%config-error ")?;
                write_cstr(out, &*cause)
            });
        } else {
            context_print(item, |out| write_cstr(out, &*cause));
        }
    });
}

pub fn cfg_show_causes(s_owner: Option<&SessionRef>) {
    let mut s = s_owner.cloned();
    let c = ClientRegistry::first();
    if CFG_CAUSES.lock().unwrap().is_empty() {
        return;
    }
    if let Some(client) = c
        .as_ref()
        .filter(|client| client.flags() & CLIENT_CONTROL as uint64_t != 0)
    {
        cfg_drain_causes(|cause| {
            control_notify_write(client, |out| {
                out.write_all(b"%config-error ")?;
                write_cstr(out, &*cause)
            });
        });
    } else {
        if s.is_none() {
            s = c
                .as_ref()
                .and_then(|c| c.attached_session().upgrade())
                .or_else(sessions::first);
        }
        let Some(session) = s.filter(|session| session.is_attached()) else {
            return;
        };
        let pane_owner = winlink_window(&session.current_winlink())
            .expect("live window")
            .active_pane()
            .upgrade()
            .expect("view-mode pane");
        if !pane_owner.is_mode(&window_view_mode) {
            pane_owner.set_mode(None, &window_view_mode, None, None, None);
        }
        cfg_drain_causes(|cause| {
            window_copy_add(&pane_owner, 0, |out| write_cstr(out, &*cause));
        });
    }
}

#[cfg(any(test, feature = "test-support"))]
pub mod cfg_wait_tests {
    use super::{
        CFG_FINISHED, CFG_WAITER, ClientRef, Completion, TaskOutcome, callback, cfg_client_done,
        cfg_done, cfg_finished, enqueue,
    };
    use crate::actions::ContextWeak;
    use crate::actions::queue::drive;
    use crate::options::{OptionsScope, options_table};
    use crate::server_client::Client as _;
    use crate::server_client::queue_test_clients;
    use crate::shared::client::CLIENT_DEAD;
    use std::cell::Cell;
    use std::rc::Rc;

    struct Options;
    impl Options {
        fn new() -> Self {
            let entry = options_table
                .iter()
                .find(|entry| entry.name == Some(c"history-file"))
                .unwrap();
            OptionsScope::GlobalServer.reset_default(entry);
            CFG_FINISHED.set(false);
            Self
        }
    }
    impl Drop for Options {
        fn drop(&mut self) {
            drop(CFG_WAITER.take());
        }
    }

    struct SetOnDrop(Rc<Cell<bool>>);
    impl Drop for SetOnDrop {
        fn drop(&mut self) {
            self.0.set(true);
        }
    }

    fn startup_queue(
        owner: &ClientRef,
        ran: &Rc<Cell<bool>>,
        released: &Rc<Cell<bool>>,
    ) -> ContextWeak {
        let head = enqueue(
            Some(owner),
            callback(c"cfg_client_done", Some(Box::new(cfg_client_done))),
        );
        let (ran, capture) = (ran.clone(), SetOnDrop(released.clone()));
        enqueue(
            Some(owner),
            callback(
                c"after config",
                Some(Box::new(move |_| {
                    let _capture = &capture;
                    ran.set(true);
                    TaskOutcome::Normal
                })),
            ),
        );
        head
    }

    pub fn cfg_done_resumes_the_waiting_startup_client() {
        let _options = Options::new();
        let owner = queue_test_clients::with_queue();
        let (ran, released) = (Rc::new(Cell::new(false)), Rc::new(Cell::new(false)));
        let head = startup_queue(&owner, &ran, &released);
        assert_eq!(drive(Some(&owner)), 0);
        assert!(
            CFG_WAITER.with_borrow(|waiter| waiter.as_ref().is_some_and(|w| !w.is_cancelled()))
        );
        assert!(!ran.get());

        assert!(matches!(cfg_done(), TaskOutcome::Normal));
        assert!(cfg_finished());
        assert!(CFG_WAITER.with_borrow(Option::is_none));
        assert!(!ran.get());
        assert_eq!(drive(Some(&owner)), 2);
        assert!(head.upgrade().is_none());
        assert!(ran.get());
    }

    pub fn the_startup_client_does_not_wait_for_a_finished_config() {
        let _options = Options::new();
        CFG_FINISHED.set(true);
        let owner = queue_test_clients::with_queue();
        let (ran, released) = (Rc::new(Cell::new(false)), Rc::new(Cell::new(false)));
        startup_queue(&owner, &ran, &released);
        assert_eq!(drive(Some(&owner)), 2);
        assert!(ran.get());
        assert!(CFG_WAITER.with_borrow(Option::is_none));
    }

    pub fn a_freed_startup_client_is_not_retained_and_cfg_done_skips_it() {
        let _options = Options::new();
        let owner = queue_test_clients::with_queue();
        let (ran, released) = (Rc::new(Cell::new(false)), Rc::new(Cell::new(false)));
        let head = startup_queue(&owner, &ran, &released);
        assert_eq!(drive(Some(&owner)), 0);
        owner.update_flags(CLIENT_DEAD as u64, 0);
        assert_eq!(drive(Some(&owner)), 0);

        let client = Rc::downgrade(&owner);
        drop(owner);
        assert!(client.upgrade().is_none());
        assert!(head.upgrade().is_none());
        assert!(released.get());
        assert!(
            CFG_WAITER.with_borrow(|waiter| waiter.as_ref().is_some_and(Completion::is_cancelled))
        );

        let _ = cfg_done();
        assert!(CFG_WAITER.with_borrow(Option::is_none));
        assert!(!ran.get());
    }
}
