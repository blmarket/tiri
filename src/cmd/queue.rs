use crate::src::arguments::{
    args_count, args_flag_values, args_flags, args_get, args_print_cstring, args_string,
};
use crate::src::cfg::{cfg_add_cause, cfg_finished};
use crate::src::cmd::find::cmd_find_client;
use crate::src::cmd::{
    cmd_get_args_mut, cmd_get_entry, cmd_get_group, cmd_get_source, cmd_print_cstring,
};
use crate::src::control::{control_write, control_write_guard};
use crate::src::events::events_fire;
use crate::src::events_payload::{
    event_payload_create, event_payload_set_identity, event_payload_set_string,
    event_payload_set_target,
};
use crate::src::file::{file_cancel_cmdq_wait, file_error};
use crate::src::format::bytes::format_message_with;
use crate::src::format::bytes::nullable_cstr;
use crate::src::format::bytes::write_cstr;
use crate::src::format::bytes::{xformat, xformat_with};
use crate::src::format::{
    format_add, format_add_cstr, format_create_owned, format_merge, format_owner_ptr,
};
use crate::src::key_string::key_string_format;
use crate::src::log::{log_cstr, log_debug, log_get_level};
use crate::src::reactor::{evbuffer_add_formatted, evbuffer_new};
use crate::src::server::server_add_message;
use crate::src::server_client::Client as _;
use crate::src::shared::abi::*;
use crate::src::shared::abi::{__uid_t, uid_t};
use crate::src::shared::account::passwd;
use crate::src::shared::arguments::args;
use crate::src::shared::client::client_file;
use crate::src::shared::client::{ClientRef, ClientWeak};
use crate::src::shared::client::{CLIENT_CONTROL, CLIENT_UTF8};
use crate::src::shared::command::*;
use crate::src::shared::command::{
    cmd, cmd_entry_flag, cmd_find_state, cmd_list, cmdq_cb, cmdq_state,
};
use crate::src::shared::command::{
    CMDQ_STATE_CONTROL, CMDQ_STATE_NOHOOKS, CMD_AFTERHOOK, CMD_CLIENT_CANFAIL, CMD_CLIENT_CFLAG,
    CMD_CLIENT_TFLAG,
};
use crate::src::shared::format::format_tree;
use crate::src::shared::format::FORMAT_NONE;
use crate::src::shared::key::key_event;
use crate::src::shared::key::*;
use crate::src::shared::session::SessionRef;
use crate::src::status::status_message_set;
use crate::src::text::utf8::utf8_sanitize_cstring;
use libc::{getpwuid, getuid, time};
use std::ffi::{CStr, CString};
use tiri_buffer::SegmentedBuf;

/// Queue-created command state. Only this module may construct or remove it.
///
/// ```compile_fail
/// use tiri::src::shared::command::cmdq_item;
/// let item = cmdq_item::empty();
/// ```
pub struct cmdq_item {
    pub name: Option<std::ffi::CString>,
    queue: Option<QueueTarget>,
    /// Own the remaining detached chain until enqueue consumes it.
    next: Option<std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>>,
    /// Nonowning execution context, which can temporarily differ from the queue owner.
    pub client: ClientWeak,
    /// `client` holds a substitute lent to a clientless command for its dispatch.
    client_lent: bool,
    /// Nonowning command target; upgrade before accessing the client.
    pub target_client: ClientWeak,
    pub group: u_int,
    pub number: u_int,
    pub time: time_t,
    pub state: Option<std::rc::Rc<cmdq_state>>,
    pub source: cmd_find_state,
    pub target: cmd_find_state,
    cmdlist: Option<std::rc::Rc<std::cell::RefCell<cmd_list>>>,
    pub cmd: refbox::Weak<cmd>,
    work: Work,
    file_task: Option<tiri_rt::mio::Task>,
    wait_file: std::rc::Weak<std::cell::UnsafeCell<client_file>>,
}

/// Dropping an item cancels its pending work, like dropping a future.
impl Drop for cmdq_item {
    fn drop(&mut self) {
        // The item can no longer be upgraded, so the cancel skips clearing it.
        if let Some(file) = self.wait_file.upgrade() {
            unsafe { file_cancel_cmdq_wait(&file) };
        }
        drop(self.cmdlist.take());
        drop(self.state.take());
        drop(std::mem::replace(&mut self.work, Work::Fired));
    }
}

/// What the queue does with an item when it reaches the head.
enum Work {
    /// Dispatch the command's entry.
    Command,
    Callback(Box<dyn FnOnce(&std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>) -> cmd_retval>),
    /// Inside its command or callback. `resumed` records a `cmdq_continue`
    /// that arrived before the command returned.
    Executing {
        resumed: bool,
    },
    /// Returned `CMD_RETURN_WAIT`; blocks its queue until `cmdq_continue`.
    Waiting,
    /// Installed by `cmdq_suspend`; blocks its queue. Each wake of `waker`
    /// polls `future` at once, and dropping it cancels the wait.
    Suspended {
        future: std::pin::Pin<Box<dyn std::future::Future<Output = ()>>>,
        waker: std::task::LocalWaker,
    },
    /// The future is moved out while it is polled. `rewoken` records a wake
    /// that arrived during the poll.
    Polling {
        rewoken: bool,
    },
    /// Finished; the next `cmdq_next` pass removes it.
    Fired,
}

impl Work {
    fn label(&self) -> &'static str {
        match self {
            Self::Command => "command",
            Self::Callback(_) => "callback",
            Self::Executing { .. } => "executing",
            Self::Waiting => "waiting",
            Self::Suspended { .. } => "suspended",
            Self::Polling { .. } => "polling",
            Self::Fired => "fired",
        }
    }
}

/// Returned only through `cmdq_suspend`. It equals `CMD_RETURN_WAIT` while the
/// legacy wait remains.
const SUSPENDED: cmd_retval = CMD_RETURN_WAIT;

/// Polls a suspended item on wake. It never touches the runtime, so a wait may
/// end after runtime shutdown.
struct ItemWake(std::rc::Weak<std::cell::UnsafeCell<cmdq_item>>);

impl std::task::LocalWake for ItemWake {
    fn wake(self: std::rc::Rc<Self>) {
        self.wake_by_ref();
    }
    fn wake_by_ref(self: &std::rc::Rc<Self>) {
        let Some(item_owner) = self.0.upgrade() else {
            return;
        };
        unsafe {
            match (*item_owner.get()).work {
                Work::Suspended { .. } => cmdq_poll(&item_owner),
                // The poller polls again after Pending.
                Work::Polling { ref mut rewoken } => *rewoken = true,
                // A wake during exec is covered by the first poll.
                _ => {}
            }
        }
    }
}

impl cmdq_item {
    /// Observe the command retained by this queue item's command list.
    pub fn command_handle(&self) -> refbox::Weak<cmd> {
        self.cmd.clone()
    }

    fn empty() -> Self {
        Self {
            name: Default::default(),
            queue: Default::default(),
            next: Default::default(),
            client: Default::default(),
            client_lent: false,
            target_client: Default::default(),
            group: Default::default(),
            number: Default::default(),
            time: Default::default(),
            state: Default::default(),
            source: Default::default(),
            target: Default::default(),
            cmdlist: Default::default(),
            cmd: Default::default(),
            work: Work::Command,
            file_task: None,
            wait_file: Default::default(),
        }
    }
}

#[repr(C)]
/// Box-owned by a client until client destruction; the lazy global queue lives for
/// the process. The private deque owns stable command item allocations.
///
/// ```compile_fail
/// use tiri::src::cmd::queue::cmdq_new;
/// let mut queue = cmdq_new();
/// queue.list.pop_front();
/// ```
pub struct cmdq_list {
    /// Current execution position, observed without retaining the item.
    item: std::rc::Weak<std::cell::UnsafeCell<cmdq_item>>,
    list: std::collections::VecDeque<std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>>,
    /// Detached commands belong to the server's global queue, not a client.
    background: std::collections::BTreeMap<u64, crate::src::reactor::Timer>,
    next_background_id: u64,
}

impl cmdq_list {
    fn first(&self) -> Option<std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>> {
        self.list.front().cloned()
    }

    fn position(&self, item: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>) -> usize {
        self.list
            .iter()
            .position(|owner| std::rc::Rc::ptr_eq(owner, item))
            .expect("command item belongs to queue")
    }
}

unsafe fn cmdq_new_named_item(
    label: &'static CStr,
) -> std::rc::Rc<std::cell::UnsafeCell<cmdq_item>> {
    let owner = std::rc::Rc::new(std::cell::UnsafeCell::new(cmdq_item::empty()));
    let item = owner.get();
    (*item).name = Some(format_message_with(|out| {
        out.write_all(b"[")?;
        out.write_all(label.to_bytes())?;
        write!(out, "/{item:p}]")
    }));

    owner
}

pub(crate) unsafe fn cmdq_background(
    delay: std::time::Duration,
    callback: impl FnOnce() + 'static,
) -> std::io::Result<()> {
    let id = QueueTarget::Global.with_queue(|queue| {
        let id = queue.next_background_id;
        queue.next_background_id = id.checked_add(1).expect("background command ID overflow");
        id
    });
    let timer = crate::src::reactor::Timer::new(delay, move || unsafe {
        // Release ownership before dispatch, outside the queue borrow: the
        // callback can schedule or cancel other background commands.
        let timer = QueueTarget::Global.with_queue(|queue| queue.background.remove(&id));
        drop(timer);
        callback();
    })?;
    QueueTarget::Global.with_queue(|queue| queue.background.insert(id, timer));
    Ok(())
}

/// Release detached command captures outside the queue borrow during shutdown.
pub(crate) unsafe fn cmdq_cancel_background() {
    let background = QueueTarget::Global.with_queue(|queue| std::mem::take(&mut queue.background));
    drop(background);
}

/// Remote files are owned by their stream index. Local completion work is
/// retained by file_task while this command waits.
pub(crate) fn cmdq_set_wait_file(
    item: &mut cmdq_item,
    file: &std::rc::Rc<std::cell::UnsafeCell<client_file>>,
) {
    assert!(
        std::rc::Weak::ptr_eq(&item.wait_file, &std::rc::Weak::new()),
        "queue item already has a file wait"
    );
    item.wait_file = std::rc::Rc::downgrade(file);
}

pub(crate) fn cmdq_set_file_task(item: &mut cmdq_item, task: tiri_rt::mio::Task) {
    assert!(
        item.file_task.is_none(),
        "queue item already has a file task"
    );
    item.file_task = Some(task);
}

pub(crate) fn cmdq_clear_wait_file(
    item: &mut cmdq_item,
    file: &std::rc::Weak<std::cell::UnsafeCell<client_file>>,
) {
    if item.wait_file.ptr_eq(file) {
        item.wait_file = std::rc::Weak::new();
        drop(item.file_task.take());
    }
}

/// The execution context of an item may change during dispatch; its queue
/// identity does not. A weak owner avoids adding a second client lifetime duty.
#[derive(Clone)]
enum QueueTarget {
    Global,
    Client(ClientWeak),
}

impl QueueTarget {
    fn for_client(owner: Option<&ClientRef>) -> Self {
        owner.map_or(Self::Global, |owner| {
            Self::Client(std::rc::Rc::downgrade(owner))
        })
    }

    /// Only immediate deque operations belong in this scope. In particular,
    /// dispatch and explicit item removal (which can invoke cancellation) must
    /// happen after it returns. Results must own their data.
    unsafe fn with_queue<R>(&self, operation: impl FnOnce(&mut cmdq_list) -> R) -> R {
        static mut GLOBAL_QUEUE: Option<Box<cmdq_list>> = None;
        match self {
            Self::Global => operation(GLOBAL_QUEUE.get_or_insert_with(cmdq_new)),
            Self::Client(observer) => {
                let owner = observer.upgrade().expect("live command queue owner");
                let mut queue = owner.borrow_queue_mut();
                operation(queue)
            }
        }
    }
}

unsafe fn cmdq_name(c: Option<&ClientRef>) -> CString {
    let Some(c) = c else {
        return c"<global>".to_owned();
    };
    let name = c.name();
    format_message_with(|out| {
        if let Some(name) = name.as_ref() {
            out.write_all(b"<")?;
            out.write_all(name.as_bytes())?;
            out.write_all(b">")
        } else {
            write!(out, "<{:p}>", std::rc::Rc::as_ptr(c))
        }
    })
}

pub fn cmdq_new() -> Box<cmdq_list> {
    Box::new(cmdq_list {
        item: std::rc::Weak::new(),
        list: std::collections::VecDeque::new(),
        background: std::collections::BTreeMap::new(),
        next_background_id: 0,
    })
}

pub fn cmdq_get_name(item: &cmdq_item) -> Option<&std::ffi::CStr> {
    item.name.as_deref()
}
pub fn cmdq_get_cmd(item: &cmdq_item) -> refbox::Weak<cmd> {
    item.command_handle()
}
pub fn cmdq_get_client(item_handle: Option<&cmdq_item>) -> Option<ClientRef> {
    item_handle.and_then(|item| item.client.upgrade())
}
pub fn cmdq_get_target_client(item_handle: Option<&cmdq_item>) -> Option<ClientRef> {
    item_handle.and_then(|item| item.target_client.upgrade())
}
pub fn cmdq_get_state(item: &cmdq_item) -> Option<&std::rc::Rc<cmdq_state>> {
    item.state.as_ref()
}
pub fn cmdq_get_target(item: &cmdq_item) -> &cmd_find_state {
    &item.target
}
pub fn cmdq_get_target_mut(item: &mut cmdq_item) -> &mut cmd_find_state {
    &mut item.target
}
pub fn cmdq_get_source(item: &cmdq_item) -> &cmd_find_state {
    &item.source
}
pub fn cmdq_get_source_mut(item: &mut cmdq_item) -> &mut cmd_find_state {
    &mut item.source
}
/// Event metadata is read from a snapshot, never through a pointer into shared state.
pub fn cmdq_get_event(item: &cmdq_item) -> key_event {
    cmdq_get_state(item)
        .expect("command queue state")
        .event
        .metadata_snapshot()
}
/// Retain the state; callers borrow its current target only for each operation.
pub fn cmdq_get_state_owned(item: &cmdq_item) -> std::rc::Rc<cmdq_state> {
    cmdq_get_state(item).expect("command queue state").clone()
}
pub fn cmdq_get_flags(item: &cmdq_item) -> ::core::ffi::c_int {
    cmdq_get_state(item).expect("command queue state").flags
}
pub unsafe fn cmdq_new_state(
    mut current: *mut cmd_find_state,
    mut event: *mut key_event,
    mut flags: ::core::ffi::c_int,
) -> std::rc::Rc<cmdq_state> {
    let snapshot = if event.is_null() {
        key_event {
            client: std::rc::Weak::new(),
            key: KEYC_NONE as key_code,
            m: Default::default(),
            bytes: None,
        }
    } else {
        (*event).metadata_snapshot()
    };
    let mut target = cmd_find_state::default();
    if !current.is_null() && (*current).is_valid() {
        target.copy_from(&*current);
    } else {
        target = cmd_find_state::clear_state(0);
    }
    std::rc::Rc::new(cmdq_state {
        flags,
        formats: Default::default(),
        event: snapshot,
        current: std::cell::RefCell::new(target),
    })
}
pub unsafe fn cmdq_copy_state(
    state: &cmdq_state,
    current: *mut cmd_find_state,
) -> std::rc::Rc<cmdq_state> {
    let mut event = state.event.metadata_snapshot();
    if !current.is_null() {
        return cmdq_new_state(current, &mut event, state.flags);
    }
    let mut current = state.current_snapshot();
    cmdq_new_state(&mut current, &mut event, state.flags)
}
/// Copy a literal key and value into the command queue state's formats.
pub unsafe fn cmdq_add_format(state: &cmdq_state, key: &CStr, value: &CStr) {
    let mut formats = state.formats.borrow_mut();
    if formats.is_none() {
        *formats = Some(format_create_owned(
            None,
            None,
            FORMAT_NONE,
            0 as ::core::ffi::c_int,
        ));
    }
    format_add_cstr(format_owner_ptr(&mut formats), key, value);
}
pub unsafe fn cmdq_add_formats(state: &cmdq_state, mut ft: *mut format_tree) {
    let mut formats = state.formats.borrow_mut();
    if formats.is_none() {
        *formats = Some(format_create_owned(
            None,
            None,
            FORMAT_NONE,
            0 as ::core::ffi::c_int,
        ));
    }
    format_merge(format_owner_ptr(&mut formats), ft);
}
pub unsafe fn cmdq_merge_formats(
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
    mut ft: *mut format_tree,
) {
    let item = item_handle.get();
    if (*item).command_handle().is_alive() {
        let entry = cmd_get_entry(((*item).command_handle()).get_unchecked());
        format_add(ft, c"command", |out| write_cstr(out, &*entry.name));
    }
    let state = cmdq_get_state(&*item).expect("command queue state").clone();
    let mut formats = state.formats.borrow_mut();
    if formats.is_some() {
        format_merge(ft, format_owner_ptr(&mut formats));
    }
}
/// Consume a detached chain, moving its owners into the queue in order.
pub unsafe fn cmdq_append(
    owner: Option<&ClientRef>,
    item: std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
) -> std::rc::Weak<std::cell::UnsafeCell<cmdq_item>> {
    let queue = QueueTarget::for_client(owner);
    let mut remaining = Some(item);
    let mut last = std::rc::Weak::new();
    while let Some(owner_item) = remaining {
        let item = owner_item.get();
        assert!((*item).queue.is_none(), "detached command item");
        remaining = (*item).next.take();
        (*item).client = owner.map(std::rc::Rc::downgrade).unwrap_or_default();
        (*item).queue = Some(queue.clone());
        last = std::rc::Rc::downgrade(&owner_item);
        queue.with_queue(|queue| queue.list.push_back(owner_item));
        log_debug(format_args!(
            "{} {}: {}",
            "cmdq_append",
            crate::src::log::log_bytes(cmdq_name(owner).as_bytes()),
            log_cstr((*item).name.as_deref().unwrap_or(c"(null)"))
        ));
    }
    last
}

/// Consume a detached chain and return the final queued insertion anchor.
pub unsafe fn cmdq_insert_after(
    after: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
    item: std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
) -> std::rc::Weak<std::cell::UnsafeCell<cmdq_item>> {
    assert!((*after.get()).queue.is_some(), "queued insertion anchor");
    let c_owner = (*after.get()).client.upgrade();
    let queue = (*after.get())
        .queue
        .clone()
        .expect("queued insertion anchor");
    let mut position = queue.with_queue(|queue| queue.position(after) + 1);
    let mut previous = after.get();
    let mut remaining = Some(item);
    let mut last = std::rc::Weak::new();
    while let Some(owner_item) = remaining {
        let item = owner_item.get();
        assert!((*item).queue.is_none(), "detached command item");
        remaining = (*item).next.take();
        (*item).client = c_owner
            .as_ref()
            .map(std::rc::Rc::downgrade)
            .unwrap_or_default();
        (*item).queue = Some(queue.clone());
        last = std::rc::Rc::downgrade(&owner_item);
        queue.with_queue(|queue| queue.list.insert(position, owner_item));
        position += 1;
        log_debug(format_args!(
            "{} {}: {} after {}",
            "cmdq_insert_after",
            crate::src::log::log_bytes(cmdq_name(c_owner.as_ref()).as_bytes()),
            log_cstr((*item).name.as_deref().unwrap_or(c"(null)")),
            log_cstr((*previous).name.as_deref().unwrap_or(c"(null)"))
        ));
        previous = item;
    }
    last
}
pub unsafe fn cmdq_insert_hook(
    _s_owner: Option<&SessionRef>,
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
    mut current: *mut cmd_find_state,
    write: impl FnOnce(&mut dyn std::io::Write) -> std::io::Result<()>,
) {
    let item = item_handle.get();

    let mut cmd: refbox::Weak<cmd> = (*item).command_handle();
    let mut args_0: *mut args =
        cmd_get_args_mut(cmd.get_mut_unchecked()).map_or(std::ptr::null_mut(), |args| args);
    let mut tmp: [u8; 32] = [0; 32];
    let mut i: u_int = 0;
    let mut value: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    if cmdq_get_state(&*item).expect("command queue state").flags & CMDQ_STATE_NOHOOKS != 0 {
        return;
    }
    let name = format_message_with(write);
    let mut ep = event_payload_create();
    if !current.is_null() {
        event_payload_set_target(&mut ep, &*current);
    }
    event_payload_set_identity(
        &mut ep,
        c"_cmdq_item",
        crate::src::shared::events::EventPayloadIdentity::QueueItem(std::rc::Rc::downgrade(
            item_handle,
        )),
    );
    let arguments = args_print_cstring(&*args_0);
    event_payload_set_string(&mut ep, c"arguments", |out| write_cstr(out, &*arguments));
    i = 0 as u_int;
    while i < args_count(args_0) {
        xformat(&mut tmp, format_args!("argument_{}", i as u32));
        event_payload_set_string(
            &mut ep,
            CStr::from_ptr(&raw mut tmp as *mut ::core::ffi::c_char),
            |out| {
                write_cstr(
                    out,
                    nullable_cstr(
                        args_string(&mut *(args_0), i)
                            .map_or(std::ptr::null(), |value| value.as_ptr()),
                    ),
                )
            },
        );
        i = i.wrapping_add(1);
    }
    for flag in args_flags(&*args_0) {
        value =
            args_get(&*(args_0), flag as u_char).map_or(std::ptr::null(), |value| value.as_ptr());
        xformat_with(&mut tmp, |out| {
            out.write_all(b"flag_")?;
            out.write_all(&[flag])
        });
        if value.is_null() {
            event_payload_set_string(
                &mut ep,
                CStr::from_ptr(&raw mut tmp as *mut ::core::ffi::c_char),
                |out| out.write_all(b"1"),
            );
        } else {
            event_payload_set_string(
                &mut ep,
                CStr::from_ptr(&raw mut tmp as *mut ::core::ffi::c_char),
                |out| write_cstr(out, nullable_cstr(value)),
            );
        }
        i = 0 as u_int;
        for av in args_flag_values(&*args_0, flag as u_char) {
            xformat_with(&mut tmp, |out| {
                out.write_all(b"flag_")?;
                out.write_all(&[flag])?;
                write!(out, "_{}", i)
            });
            event_payload_set_string(
                &mut ep,
                CStr::from_ptr(&raw mut tmp as *mut ::core::ffi::c_char),
                |out| write_cstr(out, av.as_string()),
            );
            i = i.wrapping_add(1);
        }
    }
    events_fire(&name, ep);
}
pub unsafe fn cmdq_continue(item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>) {
    let item = item_handle.get();
    match &mut (*item).work {
        Work::Executing { resumed } => *resumed = true,
        work @ Work::Waiting => *work = Work::Fired,
        _ => {}
    }
}
/// Install the executing item's continuation and return the value its command
/// or callback must return. Each wake polls the continuation at once; dropping
/// the item drops it, which cancels the wait.
#[must_use = "return the suspension from the command"]
pub(crate) unsafe fn cmdq_suspend(
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
    future: impl std::future::Future<Output = ()> + 'static,
) -> cmd_retval {
    let item = item_handle.get();
    assert!(
        matches!((*item).work, Work::Executing { .. }),
        "only an executing command item can suspend"
    );
    let waker = std::task::LocalWaker::from(std::rc::Rc::new(ItemWake(std::rc::Rc::downgrade(
        item_handle,
    ))));
    (*item).work = Work::Suspended {
        future: Box::pin(future),
        waker,
    };
    SUSPENDED
}
/// Retain the item from inside its continuation. Only valid while it is
/// polled; drop the handle before the next `.await`.
pub(crate) unsafe fn cmdq_resumed(
    item: &std::rc::Weak<std::cell::UnsafeCell<cmdq_item>>,
) -> std::rc::Rc<std::cell::UnsafeCell<cmdq_item>> {
    let owner = item.upgrade().expect("polled command item");
    assert!(
        matches!((*owner.get()).work, Work::Polling { .. }),
        "command item resumed outside its poll"
    );
    owner
}
/// Poll a suspended item's continuation outside any queue borrow, again while
/// it is woken during the poll. A completed item is Fired before its future is
/// dropped; that drop can free the item's own queue, so nothing here touches
/// the queue. The caller's strong owner keeps the item alive.
unsafe fn cmdq_poll(item_owner: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>) {
    let item = item_owner.get();
    let Work::Suspended { mut future, waker } =
        std::mem::replace(&mut (*item).work, Work::Polling { rewoken: false })
    else {
        panic!("only a suspended command item is polled");
    };
    let mut context = std::task::ContextBuilder::from_waker(std::task::Waker::noop())
        .local_waker(&waker)
        .build();
    loop {
        let ready = future.as_mut().poll(&mut context).is_ready();
        // The queue and the poller, or only the poller once the queue is freed.
        assert!(
            std::rc::Rc::strong_count(item_owner) <= 2,
            "command continuation retained its item across a poll"
        );
        let Work::Polling { rewoken } = std::mem::replace(&mut (*item).work, Work::Fired) else {
            panic!("a polled command item stays polling until the poll returns");
        };
        if ready {
            drop(future);
            return;
        }
        if !rewoken {
            (*item).work = Work::Suspended { future, waker };
            return;
        }
        (*item).work = Work::Polling { rewoken: false };
    }
}
/// The resuming end of a plain wait. Dropping it never resumes the item.
pub struct CmdqWaiter {
    /// The waiting item's own client, not one lent for its dispatch.
    client: ClientWeak,
    done: tiri_rt::oneshot::Sender<()>,
}
/// Suspend the executing item until its waiter resumes it, and return the
/// waiter with the value its command or callback must return.
#[must_use = "return the suspension from the command"]
pub(crate) unsafe fn cmdq_wait(
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
) -> (CmdqWaiter, cmd_retval) {
    let item = item_handle.get();
    // tmux reads cmdq_get_client lazily, after a lent client is restored.
    let client = if (*item).client_lent {
        std::rc::Weak::new()
    } else {
        (*item).client.clone()
    };
    let (done, resumed) = tiri_rt::oneshot::channel();
    let retval = cmdq_suspend(item_handle, async move {
        if resumed.await.is_none() {
            std::future::pending::<()>().await;
        }
    });
    (CmdqWaiter { client, done }, retval)
}
impl CmdqWaiter {
    pub(crate) fn client(&self) -> Option<ClientRef> {
        self.client.upgrade()
    }
    /// The waiting item was dropped, and its wait with it.
    pub(crate) fn is_cancelled(&self) -> bool {
        self.done.is_closed()
    }
    /// Complete the wait; a wake outside exec completes the item at once.
    pub(crate) unsafe fn resume(self) {
        let _ = self.done.send(());
    }
    /// Report `status` as the return value of a client without a session at
    /// this moment, then resume (tmux's popup_free_cb, window_pane_wait_finish).
    pub(crate) unsafe fn finish(self, status: i32) {
        if let Some(client) = self.client() {
            if client.attached_session().upgrade().is_none() {
                client.set_return_value(status);
            }
        }
        self.resume();
    }
    /// A waiter for no item; the receiver observes its resume.
    #[cfg(test)]
    pub(crate) fn detached(client: ClientWeak) -> (Self, tiri_rt::oneshot::Receiver<()>) {
        let (done, resumed) = tiri_rt::oneshot::channel();
        (Self { client, done }, resumed)
    }
}
// The caller transfers the queue owner after unlinking it and releasing guards.
unsafe fn cmdq_remove(item_handle: std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>) {
    assert_eq!(
        std::rc::Rc::strong_count(&item_handle),
        1,
        "command item removal requires its sole strong owner"
    );
    let item = item_handle.get();
    assert!(
        std::rc::Weak::ptr_eq(&(*item).wait_file, &std::rc::Weak::new()),
        "file wait must finish or cancel before queue item removal"
    );
    drop(item_handle);
}
unsafe fn cmdq_remove_group(item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>) {
    let item = item_handle.get();
    if (*item).group == 0 as u_int {
        return;
    }
    let queue = (*item).queue.clone().expect("queued group anchor");
    let mut position = queue.with_queue(|queue| queue.position(item_handle) + 1);
    loop {
        let next = queue.with_queue(|queue| {
            let owner = queue.list.get(position)?;
            if (*owner.get()).group == (*item).group {
                Some(queue.list.remove(position))
            } else {
                Some(None)
            }
        });
        match next {
            Some(Some(owner)) => cmdq_remove(owner),
            Some(None) => position += 1,
            None => break,
        }
    }
}
#[must_use = "enqueue the detached command chain"]
pub unsafe fn cmdq_get_command(
    commands: &std::rc::Rc<std::cell::RefCell<cmd_list>>,
    state: Option<&std::rc::Rc<cmdq_state>>,
) -> std::rc::Rc<std::cell::UnsafeCell<cmdq_item>> {
    let cmdlist = commands.borrow();
    if cmdlist.list.is_empty() {
        return cmdq_get_callback_owned(
            c"cmdq_empty_command",
            Some(Box::new(|_| CMD_RETURN_NORMAL)),
        );
    }
    let state = state
        .cloned()
        .unwrap_or_else(|| cmdq_new_state(std::ptr::null_mut(), std::ptr::null_mut(), 0));
    let mut remaining = None;
    for command in cmdlist.list.iter().rev() {
        let cmd = command.downgrade();
        let owner = cmdq_new_named_item(cmd_get_entry(cmd.get_unchecked()).name);
        let item = owner.get();
        (*item).group = cmd_get_group(cmd.clone());
        (*item).state = Some(state.clone());
        (*item).cmd = command.downgrade();
        (*item).cmdlist = Some(commands.clone());
        (*item).next = remaining;
        remaining = Some(owner);
    }
    remaining.expect("nonempty command list")
}
unsafe fn cmdq_find_flag(
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
    mut fs: *mut cmd_find_state,
    flag: &cmd_entry_flag,
) -> cmd_retval {
    let item = item_handle.get();
    let mut value: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    if flag.flag as ::core::ffi::c_int == 0 as ::core::ffi::c_int {
        let target_client = cmdq_get_target_client((item).as_ref());
        *fs = cmd_find_state::find_from_client(target_client.as_ref(), 0 as ::core::ffi::c_int)
            .unwrap_or_else(|| cmd_find_state::clear_state(0 as ::core::ffi::c_int));
        return CMD_RETURN_NORMAL;
    }
    value = args_get(
        &*(cmd_get_args_mut(((*item).command_handle()).get_mut_unchecked())
            .map_or(std::ptr::null_mut(), |args| args)),
        flag.flag as u_char,
    )
    .map_or(std::ptr::null(), |value| value.as_ptr());
    if (*fs).find_target(
        Some(item_handle),
        nullable_cstr(value),
        flag.type_0,
        flag.flags,
    ) != 0 as ::core::ffi::c_int
    {
        *fs = cmd_find_state::clear_state(0 as ::core::ffi::c_int);
        return CMD_RETURN_ERROR;
    }
    CMD_RETURN_NORMAL
}
unsafe fn cmdq_add_message(item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>) {
    let item = item_handle.get();
    let c_owner = cmdq_get_client((item).as_ref());
    let mut c: Option<ClientRef> = c_owner.clone();
    let state = cmdq_get_state(&*item).expect("command queue state").clone();
    let mut uid: uid_t = 0;
    let mut pw: *mut passwd = ::core::ptr::null_mut::<passwd>();
    let tmp = cmd_print_cstring(((*item).command_handle()).get_unchecked());
    if let Some(c_value) = c.as_ref() {
        uid = c_value.peer_uid();
        let user: CString = if uid != -(1 as ::core::ffi::c_int) as uid_t && uid != getuid() {
            pw = getpwuid(uid as __uid_t);
            if !pw.is_null() {
                let name = CStr::from_ptr((*pw).pw_name).to_bytes();
                let mut bytes = Vec::with_capacity(name.len() + 2);
                bytes.push(b'[');
                bytes.extend_from_slice(name);
                bytes.push(b']');
                CString::new(bytes).expect("passwd name is a C string")
            } else {
                c"[unknown]".to_owned()
            }
        } else {
            c"".to_owned()
        };
        if !c_value.attached_session().upgrade().is_none()
            && state.event.key != KEYC_NONE as ::core::ffi::c_ulong as key_code
        {
            let key = key_string_format(state.event.key, false);
            server_add_message(|out| {
                write_cstr(out, (c_value.name()).as_deref())?;
                write_cstr(out, &*user)?;
                out.write_all(b" key ")?;
                write_cstr(out, &*key)?;
                out.write_all(b": ")?;
                write_cstr(out, &*tmp)
            });
        } else {
            server_add_message(|out| {
                write_cstr(out, (c_value.name()).as_deref())?;
                write_cstr(out, &*user)?;
                out.write_all(b" command: ")?;
                write_cstr(out, &*tmp)
            });
        }
    } else {
        server_add_message(|out| {
            out.write_all(b"command: ")?;
            write_cstr(out, &*tmp)
        });
    }
}
unsafe fn cmdq_fire_command(
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
) -> cmd_retval {
    let item = item_handle.get();
    let mut current_block: u64;
    let saved_client = cmdq_get_client((item).as_ref());
    let name = cmdq_name(saved_client.as_ref());
    let state = cmdq_get_state(&*item).expect("command queue state").clone();
    let mut cmd: refbox::Weak<cmd> = (*item).command_handle();
    let mut args: *mut args =
        cmd_get_args_mut(cmd.get_mut_unchecked()).map_or(std::ptr::null_mut(), |args| args);
    let entry = cmd_get_entry(cmd.get_unchecked());
    let mut tc = None;
    let saved = (*item).client.clone();
    let mut retval: cmd_retval = CMD_RETURN_NORMAL;
    let mut fsp: *mut cmd_find_state = ::core::ptr::null_mut::<cmd_find_state>();
    let mut fs: cmd_find_state = cmd_find_state {
        flags: 0,
        s: Default::default(),
        wl: Default::default(),
        w: Default::default(),
        wp: Default::default(),
        idx: 0,
    };
    let mut flags: ::core::ffi::c_int = 0;
    let mut quiet: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    // A freed client is as dead as one still marked dead.
    let client_freed =
        saved_client.is_none() && !std::rc::Weak::ptr_eq(&saved, &std::rc::Weak::new());
    if client_freed || saved_client.as_ref().is_some_and(|c| c.is_dead()) {
        return CMD_RETURN_ERROR;
    }
    if cfg_finished != 0 {
        cmdq_add_message(item_handle);
    }
    if log_get_level() > 1 as ::core::ffi::c_int {
        let tmp = cmd_print_cstring(cmd.get_unchecked());
        log_debug(format_args!(
            "{} {}: ({}) {}",
            "cmdq_fire_command",
            crate::src::log::log_bytes(name.as_bytes()),
            { (*item).group },
            crate::src::log::log_bytes(tmp.as_bytes())
        ));
    }
    flags = (state.flags & CMDQ_STATE_CONTROL != 0) as ::core::ffi::c_int;
    cmdq_guard(item_handle, c"begin", flags);
    if (*item).client.upgrade().is_none() {
        let context = cmd_find_client(
            Some(item_handle),
            nullable_cstr(::core::ptr::null::<::core::ffi::c_char>()),
            1 as ::core::ffi::c_int,
        );
        (*item).client = context
            .as_ref()
            .map_or_else(std::rc::Weak::new, std::rc::Rc::downgrade);
        (*item).client_lent = true;
    }
    let execution_client = cmdq_get_client((item).as_ref());
    if entry.flags & CMD_CLIENT_CANFAIL != 0 {
        quiet = 1 as ::core::ffi::c_int;
    }
    if entry.flags & CMD_CLIENT_CFLAG != 0 {
        tc = cmd_find_client(
            Some(item_handle),
            nullable_cstr(
                args_get(&*(args), 'c' as i32 as u_char)
                    .map_or(std::ptr::null(), |value| value.as_ptr()),
            ),
            quiet,
        );
        if tc.is_none() && quiet == 0 {
            retval = CMD_RETURN_ERROR;
            current_block = 7379054416801212160;
        } else {
            current_block = 18317007320854588510;
        }
    } else if entry.flags & CMD_CLIENT_TFLAG != 0 {
        tc = cmd_find_client(
            Some(item_handle),
            nullable_cstr(
                args_get(&*(args), 't' as i32 as u_char)
                    .map_or(std::ptr::null(), |value| value.as_ptr()),
            ),
            quiet,
        );
        if tc.is_none() && quiet == 0 {
            retval = CMD_RETURN_ERROR;
            current_block = 7379054416801212160;
        } else {
            current_block = 18317007320854588510;
        }
    } else {
        tc = cmd_find_client(
            Some(item_handle),
            nullable_cstr(::core::ptr::null::<::core::ffi::c_char>()),
            1 as ::core::ffi::c_int,
        );
        current_block = 18317007320854588510;
    }
    if current_block == 18317007320854588510 {
        (*item).target_client = tc
            .as_ref()
            .map_or_else(std::rc::Weak::new, std::rc::Rc::downgrade);
        let _target_client = cmdq_get_target_client((item).as_ref());
        retval = cmdq_find_flag(item_handle, &raw mut (*item).source, &entry.source);
        if !(retval as ::core::ffi::c_int == CMD_RETURN_ERROR as ::core::ffi::c_int) {
            retval = cmdq_find_flag(item_handle, &raw mut (*item).target, &entry.target);
            if !(retval as ::core::ffi::c_int == CMD_RETURN_ERROR as ::core::ffi::c_int) {
                retval = entry.exec.expect("non-null function pointer")(cmd.clone(), item_handle);
                if !(retval as ::core::ffi::c_int == CMD_RETURN_ERROR as ::core::ffi::c_int)
                    && entry.flags & CMD_AFTERHOOK != 0
                {
                    if (*item).target.is_valid() {
                        fsp = &raw mut (*item).target;
                        current_block = 8704759739624374314;
                    } else if state.current.borrow().is_valid() {
                        fs = state.current_snapshot();
                        fsp = &mut fs;
                        current_block = 8704759739624374314;
                    } else if let Some(found) = cmd_find_state::find_from_client(
                        execution_client.as_ref(),
                        0 as ::core::ffi::c_int,
                    ) {
                        fs = found;
                        fsp = &raw mut fs;
                        current_block = 8704759739624374314;
                    } else {
                        current_block = 7379054416801212160;
                    }
                    match current_block {
                        7379054416801212160 => {}
                        _ => {
                            cmdq_insert_hook(
                                (*fsp).session_handle().as_ref(),
                                item_handle,
                                fsp,
                                |out| {
                                    out.write_all(b"after-")?;
                                    write_cstr(out, &*entry.name)
                                },
                            );
                        }
                    }
                }
            }
        }
    }
    (*item).client = saved;
    (*item).client_lent = false;
    if retval as ::core::ffi::c_int == CMD_RETURN_ERROR as ::core::ffi::c_int {
        fsp = ::core::ptr::null_mut::<cmd_find_state>();
        if (*item).target.is_valid() {
            fsp = &raw mut (*item).target;
        } else if state.current.borrow().is_valid() {
            fs = state.current_snapshot();
            fsp = &mut fs;
        } else if let Some(found) =
            cmd_find_state::find_from_client(saved_client.as_ref(), 0 as ::core::ffi::c_int)
        {
            fs = found;
            fsp = &raw mut fs;
        }
        cmdq_insert_hook(
            (if !fsp.is_null() {
                (*fsp).session_handle()
            } else {
                None
            })
            .as_ref(),
            item_handle,
            fsp,
            |out| out.write_all(b"command-error"),
        );
        cmdq_guard(item_handle, c"error", flags);
    } else {
        cmdq_guard(item_handle, c"end", flags);
    }
    retval
}
#[must_use = "enqueue the detached command chain"]
pub unsafe fn cmdq_get_callback_owned(
    name: &'static CStr,
    cb: cmdq_cb,
) -> std::rc::Rc<std::cell::UnsafeCell<cmdq_item>> {
    let owner = cmdq_new_named_item(name);
    let item = owner.get();
    (*item).group = 0 as u_int;
    (*item).state = Some(cmdq_new_state(
        ::core::ptr::null_mut::<cmd_find_state>(),
        ::core::ptr::null_mut::<key_event>(),
        0 as ::core::ffi::c_int,
    ));
    (*item).work = Work::Callback(cb.expect("non-null queue callback"));
    owner
}

#[must_use = "enqueue the detached command chain"]
pub unsafe fn cmdq_get_error(error: &CStr) -> std::rc::Rc<std::cell::UnsafeCell<cmdq_item>> {
    let error = error.to_owned();
    cmdq_get_callback_owned(
        c"cmdq_error_callback",
        Some(Box::new(move |item| unsafe {
            cmdq_error(item, |out| write_cstr(out, &*error));
            CMD_RETURN_NORMAL
        })),
    )
}
pub unsafe fn cmdq_next(owner: Option<&ClientRef>) -> u_int {
    let queue = QueueTarget::for_client(owner);
    let name = cmdq_name(owner);
    let mut items = 0;
    log_debug(format_args!(
        "cmdq_next {}: enter",
        crate::src::log::log_bytes(name.as_bytes())
    ));
    static mut number: u_int = 0;
    while let Some(item_owner) = queue.with_queue(|queue| queue.first()) {
        let item = item_owner.get();
        queue.with_queue(|queue| queue.item = std::rc::Rc::downgrade(&item_owner));
        log_debug(format_args!(
            "cmdq_next {}: {} ({})",
            crate::src::log::log_bytes(name.as_bytes()),
            crate::src::log::log_bytes((*item).name.as_deref().unwrap_or(c"(null)").to_bytes()),
            (*item).work.label()
        ));
        match (*item).work {
            // Re-entered from inside a dispatch or a poll, or blocked on a wait.
            Work::Executing { .. }
            | Work::Polling { .. }
            | Work::Waiting
            | Work::Suspended { .. } => return items,
            Work::Fired => {}
            Work::Command | Work::Callback(_) => {
                (*item).time = time(std::ptr::null_mut());
                number = number.wrapping_add(1);
                (*item).number = number;
                // Executing before dispatch: a synchronously completed operation
                // can call cmdq_continue before its command returns CMD_RETURN_WAIT.
                let command = matches!((*item).work, Work::Command);
                let retval = match std::mem::replace(
                    &mut (*item).work,
                    Work::Executing { resumed: false },
                ) {
                    Work::Command => cmdq_fire_command(&item_owner),
                    Work::Callback(cb) => cb(&item_owner),
                    _ => unreachable!(),
                };
                if retval == CMD_RETURN_ERROR && command {
                    cmdq_remove_group(&item_owner);
                }
                match std::mem::replace(&mut (*item).work, Work::Fired) {
                    Work::Executing { resumed } => {
                        if retval == CMD_RETURN_WAIT && !resumed {
                            (*item).work = Work::Waiting;
                            return items;
                        }
                    }
                    work @ Work::Suspended { .. } => {
                        assert_eq!(
                            retval, SUSPENDED,
                            "a suspended command returns cmdq_suspend's result"
                        );
                        // First poll in the same pass: a wait that already
                        // ended completes now.
                        (*item).work = work;
                        cmdq_poll(&item_owner);
                        if !matches!((*item).work, Work::Fired) {
                            return items;
                        }
                    }
                    _ => panic!("a dispatched queue item stays executing until it returns"),
                }
                items += 1;
            }
        }
        drop(item_owner);
        let owner = queue
            .with_queue(|queue| queue.list.pop_front())
            .expect("executed queue item");
        cmdq_remove(owner);
    }
    queue.with_queue(|queue| queue.item = std::rc::Weak::new());
    items
}

/// The global queue's current item while its command or callback runs, or
/// after it was continued or completed until it is removed. None while it
/// waits, including while its continuation runs (tmux's `CMDQ_WAITING`).
pub unsafe fn cmdq_running() -> std::rc::Weak<std::cell::UnsafeCell<cmdq_item>> {
    let queue = QueueTarget::Global;
    let Some(owner) = queue.with_queue(|queue| queue.item.upgrade()) else {
        return std::rc::Weak::new();
    };
    if let Work::Waiting | Work::Suspended { .. } | Work::Polling { .. } = (*owner.get()).work {
        return std::rc::Weak::new();
    }
    std::rc::Rc::downgrade(&owner)
}

#[cfg(test)]
pub(crate) fn cmdq_is_waiting(item: &cmdq_item) -> bool {
    matches!(item.work, Work::Waiting)
}
/// The number of items in a queue, without running them.
#[cfg(test)]
pub(crate) unsafe fn cmdq_len(owner: Option<&ClientRef>) -> usize {
    QueueTarget::for_client(owner).with_queue(|queue| queue.list.len())
}
pub unsafe fn cmdq_guard(
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
    guard: &CStr,
    mut flags: ::core::ffi::c_int,
) {
    let item = item_handle.get();
    let c_owner = cmdq_get_client((item).as_ref());
    let mut c: Option<ClientRef> = c_owner.clone();
    let mut t: ::core::ffi::c_long = (*item).time as ::core::ffi::c_long;
    let mut number: u_int = (*item).number;
    if !c.is_none() && c.as_ref().expect("live client").flags() & CLIENT_CONTROL as uint64_t != 0 {
        control_write_guard(&c.clone().expect("live client"), guard, t, number, flags);
    }
}
pub unsafe fn cmdq_print_data(
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
    mut evb: &mut SegmentedBuf,
) {
    let item = item_handle.get();
    let client = cmdq_get_client((item).as_ref());
    ClientRef::print_to(client.as_ref(), (1 as ::core::ffi::c_int) != 0, evb);
}
pub unsafe fn cmdq_print(
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
    write: impl FnOnce(&mut dyn std::io::Write) -> std::io::Result<()>,
) {
    let mut evb = evbuffer_new();
    evbuffer_add_formatted(&mut evb, write);
    cmdq_print_data(item_handle, &mut evb);
}
pub unsafe fn cmdq_error(
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
    write: impl FnOnce(&mut dyn std::io::Write) -> std::io::Result<()>,
) {
    let item = item_handle.get();
    let c_owner = cmdq_get_client((item).as_ref());
    let mut c: Option<ClientRef> = c_owner.clone();
    let mut cmd: refbox::Weak<cmd> = (*item).command_handle();
    let mut file: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    let mut line: u_int = 0;
    let mut msg = format_message_with(write);
    log_debug(format_args!(
        "{}: {}",
        "cmdq_error",
        crate::src::log::log_bytes(msg.as_bytes())
    ));
    match c.as_ref() {
        None => {
            let (source, source_line) = cmd_get_source(cmd.get_unchecked());
            file = source.map_or(std::ptr::null(), |file| file.as_ptr());
            line = source_line;
            if cfg_finished == 0 {
                if !file.is_null() {
                    cfg_add_cause(|out| {
                        write_cstr(out, nullable_cstr(file))?;
                        write!(out, ":{}: ", (line) as u32)?;
                        write_cstr(out, &*msg)
                    });
                } else {
                    cfg_add_cause(|out| write_cstr(out, &*msg));
                }
            } else if !file.is_null() {
                server_add_message(|out| {
                    out.write_all(b"message: ")?;
                    write_cstr(out, nullable_cstr(file))?;
                    write!(out, ":{}: ", (line) as u32)?;
                    write_cstr(out, &*msg)
                });
            } else {
                server_add_message(|out| {
                    out.write_all(b"message: ")?;
                    write_cstr(out, &*msg)
                });
            }
        }
        Some(client)
            if client.attached_session().upgrade().is_none()
                || client.flags() & CLIENT_CONTROL as uint64_t != 0 =>
        {
            server_add_message(|out| {
                write_cstr(out, (client.name()).as_deref())?;
                out.write_all(b" message: ")?;
                write_cstr(out, &*msg)
            });
            if !client.flags() & CLIENT_UTF8 as uint64_t != 0 {
                msg = utf8_sanitize_cstring(msg.as_c_str());
            }
            if client.flags() & CLIENT_CONTROL as uint64_t != 0 {
                control_write(client, |out| write_cstr(out, &*msg));
            } else {
                file_error(c_owner.as_ref(), |out| {
                    write_cstr(out, &*msg)?;
                    out.write_all(b"\n")
                });
            }
            client.set_return_value(1);
        }
        Some(_) => {
            let mut bytes = msg.into_bytes_with_nul();
            bytes[0] = ::libc::toupper(bytes[0] as ::core::ffi::c_int) as u8;
            // The bytes came from a CString; toupper maps NUL to NUL and a
            // non-NUL byte to a non-NUL byte, so the terminator stays at the end.
            msg = CString::from_vec_with_nul_unchecked(bytes);
            status_message_set(
                c.as_ref(),
                -(1 as ::core::ffi::c_int),
                1 as ::core::ffi::c_int,
                0 as ::core::ffi::c_int,
                0 as ::core::ffi::c_int,
                |out| write_cstr(out, &*msg),
            );
        }
    }
}

#[cfg(test)]
mod lifecycle_tests {
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;

    pub(super) struct SetOnDrop(pub(super) Rc<Cell<bool>>);
    impl Drop for SetOnDrop {
        fn drop(&mut self) {
            self.0.set(true);
        }
    }

    /// A callback item whose callback sets `released` when freed.
    unsafe fn tracked_item(
        name: &'static CStr,
        released: &Rc<Cell<bool>>,
    ) -> std::rc::Rc<std::cell::UnsafeCell<cmdq_item>> {
        let capture = SetOnDrop(released.clone());
        cmdq_get_callback_owned(
            name,
            Some(Box::new(move |_| {
                let _capture = &capture;
                CMD_RETURN_NORMAL
            })),
        )
    }

    #[test]
    fn removal_rejects_an_extra_strong_owner_before_cleanup() {
        unsafe {
            let released = Rc::new(Cell::new(false));
            let item = tracked_item(c"extra owner", &released);
            let observer = Rc::downgrade(&item);
            let guard = item.clone();
            let owner = item;
            let result =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| cmdq_remove(owner)));
            assert!(result.is_err());
            assert!(!released.get());
            cmdq_remove(guard);
            assert!(released.get());
            assert!(observer.upgrade().is_none());
        }
    }

    #[test]
    fn dropping_an_unqueued_item_releases_its_callback() {
        unsafe {
            let released = Rc::new(Cell::new(false));
            let item = tracked_item(c"unqueued item", &released);
            let observer = Rc::downgrade(&item);
            drop(item);
            assert!(released.get());
            assert!(observer.upgrade().is_none());
        }
    }

    #[test]
    fn running_global_callback_item_is_visible() {
        unsafe {
            let ran = Rc::new(Cell::new(false));
            let observed = ran.clone();
            let item = cmdq_append(
                None,
                cmdq_get_callback_owned(
                    c"running callback",
                    Some(Box::new(move |item| {
                        assert!(cmdq_running().ptr_eq(&Rc::downgrade(item)));
                        observed.set(true);
                        CMD_RETURN_NORMAL
                    })),
                ),
            );
            assert_eq!(cmdq_next(None), 1);
            assert!(ran.get());
            assert!(item.upgrade().is_none());
            assert!(cmdq_running().upgrade().is_none());
        }
    }

    #[test]
    fn waiting_item_is_hidden_until_continued_then_visible_until_removed() {
        unsafe {
            let item = cmdq_append(
                None,
                cmdq_get_callback_owned(c"waiting callback", Some(Box::new(|_| CMD_RETURN_WAIT))),
            );
            assert_eq!(cmdq_next(None), 0);
            assert!(item.upgrade().is_some());
            assert!(cmdq_running().upgrade().is_none());
            cmdq_continue(&item.upgrade().unwrap());
            assert!(cmdq_running().ptr_eq(&item));
            assert_eq!(cmdq_next(None), 0);
            assert!(item.upgrade().is_none());
            assert!(cmdq_running().upgrade().is_none());
        }
    }

    /// Append a global callback item that records `tag` when it runs.
    pub(super) unsafe fn append_recording(
        name: &'static CStr,
        trace: &Rc<std::cell::RefCell<Vec<&'static str>>>,
        tag: &'static str,
        retval: cmd_retval,
    ) -> std::rc::Weak<std::cell::UnsafeCell<cmdq_item>> {
        let trace = trace.clone();
        cmdq_append(
            None,
            cmdq_get_callback_owned(
                name,
                Some(Box::new(move |_| {
                    trace.borrow_mut().push(tag);
                    retval
                })),
            ),
        )
    }

    #[test]
    fn continued_wait_is_removed_on_the_next_pass_before_the_next_item_runs() {
        unsafe {
            let trace = Rc::new(std::cell::RefCell::new(Vec::new()));
            let head = append_recording(c"waiting head", &trace, "head", CMD_RETURN_WAIT);
            let next = append_recording(c"next item", &trace, "next", CMD_RETURN_NORMAL);
            assert_eq!(cmdq_next(None), 0);
            assert!(cmdq_is_waiting(&*head.upgrade().unwrap().get()));
            assert_eq!(cmdq_next(None), 0, "a waiting head blocks its queue");
            assert_eq!(*trace.borrow(), ["head"]);
            cmdq_continue(&head.upgrade().unwrap());
            assert!(!cmdq_is_waiting(&*head.upgrade().unwrap().get()));
            assert_eq!(cmdq_next(None), 1);
            assert!(head.upgrade().is_none());
            assert!(next.upgrade().is_none());
            assert_eq!(*trace.borrow(), ["head", "next"]);
        }
    }

    #[test]
    fn continue_during_exec_completes_in_the_same_pass() {
        unsafe {
            let trace = Rc::new(std::cell::RefCell::new(Vec::new()));
            let head = cmdq_append(
                None,
                cmdq_get_callback_owned(
                    c"continued during exec",
                    Some(Box::new(|item| {
                        cmdq_continue(item);
                        // Still running after the continue.
                        assert!(cmdq_running().ptr_eq(&Rc::downgrade(item)));
                        CMD_RETURN_WAIT
                    })),
                ),
            );
            let next = append_recording(c"next item", &trace, "next", CMD_RETURN_NORMAL);
            assert_eq!(cmdq_next(None), 2);
            assert!(head.upgrade().is_none());
            assert!(next.upgrade().is_none());
            assert_eq!(*trace.borrow(), ["next"]);
        }
    }

    #[test]
    fn reentrant_next_during_exec_returns_without_removing() {
        unsafe {
            let trace = Rc::new(std::cell::RefCell::new(Vec::new()));
            let reentered = Rc::new(Cell::new(None));
            let observed = reentered.clone();
            let inner_trace = trace.clone();
            let head = cmdq_append(
                None,
                cmdq_get_callback_owned(
                    c"reentrant head",
                    Some(Box::new(move |item| {
                        observed.set(Some(cmdq_next(None)));
                        assert!(inner_trace.borrow().is_empty());
                        assert!(cmdq_running().ptr_eq(&Rc::downgrade(item)));
                        CMD_RETURN_NORMAL
                    })),
                ),
            );
            let next = append_recording(c"next item", &trace, "next", CMD_RETURN_NORMAL);
            assert_eq!(cmdq_next(None), 2);
            assert_eq!(reentered.get(), Some(0));
            assert!(head.upgrade().is_none());
            assert!(next.upgrade().is_none());
            assert_eq!(*trace.borrow(), ["next"]);
        }
    }
}

#[cfg(test)]
mod suspend_tests {
    use super::lifecycle_tests::{append_recording, SetOnDrop};
    use super::*;
    use std::cell::{Cell, RefCell};
    use std::future::Future;
    use std::rc::{Rc, Weak};
    use std::task::{LocalWaker, Poll};
    use tiri_rt::oneshot::channel;

    type Trace = Rc<RefCell<Vec<&'static str>>>;

    /// Append a global callback item that suspends on `future`.
    unsafe fn append_suspended(
        name: &'static CStr,
        future: impl Future<Output = ()> + 'static,
    ) -> Weak<std::cell::UnsafeCell<cmdq_item>> {
        cmdq_append(
            None,
            cmdq_get_callback_owned(name, Some(Box::new(move |item| cmdq_suspend(item, future)))),
        )
    }

    #[test]
    fn send_runs_the_body_inside_send_and_the_next_pass_moves_on() {
        unsafe {
            let trace: Trace = Default::default();
            let (tx, rx) = channel::<()>();
            let body = trace.clone();
            let head = append_suspended(c"suspended head", async move {
                assert_eq!(rx.await, Some(()));
                body.borrow_mut().push("body");
            });
            let next = append_recording(c"next item", &trace, "next", CMD_RETURN_NORMAL);
            assert_eq!(cmdq_next(None), 0, "the first poll is pending");
            assert_eq!(cmdq_next(None), 0, "a suspended head blocks its queue");
            assert!(trace.borrow().is_empty());
            tx.send(()).unwrap();
            assert_eq!(*trace.borrow(), ["body"], "the body runs inside send");
            assert!(head.upgrade().is_some(), "removal waits for cmdq_next");
            assert_eq!(cmdq_next(None), 1);
            assert!(head.upgrade().is_none());
            assert!(next.upgrade().is_none());
            assert_eq!(*trace.borrow(), ["body", "next"]);
        }
    }

    #[test]
    fn send_during_exec_completes_in_the_same_pass() {
        unsafe {
            let trace: Trace = Default::default();
            let body = trace.clone();
            let head = cmdq_append(
                None,
                cmdq_get_callback_owned(
                    c"sent during exec",
                    Some(Box::new(move |item| {
                        let (tx, rx) = channel::<()>();
                        let inner = body.clone();
                        let retval = cmdq_suspend(item, async move {
                            assert_eq!(rx.await, Some(()));
                            inner.borrow_mut().push("body");
                        });
                        tx.send(()).unwrap();
                        assert!(body.borrow().is_empty(), "the first poll follows exec");
                        retval
                    })),
                ),
            );
            let next = append_recording(c"next item", &trace, "next", CMD_RETURN_NORMAL);
            assert_eq!(cmdq_next(None), 2);
            assert!(head.upgrade().is_none());
            assert!(next.upgrade().is_none());
            assert_eq!(*trace.borrow(), ["body", "next"]);
        }
    }

    #[test]
    fn wakes_outside_a_suspension_are_ignored() {
        unsafe {
            let waker = Rc::new(RefCell::new(None::<LocalWaker>));
            let polls = Rc::new(Cell::new(0));
            let ready = Rc::new(Cell::new(false));
            let (stored, counted, done) = (waker.clone(), polls.clone(), ready.clone());
            let head = cmdq_append(
                None,
                cmdq_get_callback_owned(
                    c"woken outside suspension",
                    Some(Box::new(move |item| {
                        LocalWaker::from(Rc::new(ItemWake(Rc::downgrade(item)))).wake();
                        assert!(matches!((*item.get()).work, Work::Executing { .. }));
                        cmdq_suspend(
                            item,
                            std::future::poll_fn(move |cx| {
                                counted.set(counted.get() + 1);
                                if done.get() {
                                    return Poll::Ready(());
                                }
                                *stored.borrow_mut() = Some(cx.local_waker().clone());
                                Poll::Pending
                            }),
                        )
                    })),
                ),
            );
            assert_eq!(cmdq_next(None), 0);
            assert_eq!(polls.get(), 1);
            let waker = waker.borrow_mut().take().unwrap();
            ready.set(true);
            waker.wake_by_ref();
            assert_eq!(polls.get(), 2);
            let owner = head.upgrade().unwrap();
            assert!(matches!((*owner.get()).work, Work::Fired));
            drop(owner);
            waker.wake_by_ref();
            assert_eq!(polls.get(), 2, "a fired item is not polled again");
            assert_eq!(cmdq_next(None), 0);
            assert!(head.upgrade().is_none());
            waker.wake();
            assert_eq!(polls.get(), 2, "a freed item is not polled");
        }
    }

    #[test]
    fn self_wake_during_poll_polls_again() {
        unsafe {
            let polls = Rc::new(Cell::new(0));
            let counted = polls.clone();
            let head = append_suspended(
                c"self-woken",
                std::future::poll_fn(move |cx| {
                    counted.set(counted.get() + 1);
                    match counted.get() {
                        1 => cx.local_waker().wake_by_ref(),
                        2 => cx.local_waker().clone().wake(),
                        _ => {}
                    }
                    Poll::Pending
                }),
            );
            assert_eq!(cmdq_next(None), 0);
            assert_eq!(polls.get(), 3);
            let owner = head.upgrade().unwrap();
            assert!(matches!((*owner.get()).work, Work::Suspended { .. }));
        }
    }

    #[test]
    fn running_is_none_inside_the_body_and_the_head_once_ready() {
        unsafe {
            let (tx, rx) = channel::<()>();
            let ran = Rc::new(Cell::new(false));
            let body = ran.clone();
            let head = cmdq_append(
                None,
                cmdq_get_callback_owned(
                    c"observed body",
                    Some(Box::new(move |item| {
                        let observer = Rc::downgrade(item);
                        cmdq_suspend(item, async move {
                            assert!(cmdq_running().upgrade().is_none(), "first poll");
                            assert_eq!(rx.await, Some(()));
                            let item = cmdq_resumed(&observer);
                            assert!(cmdq_running().upgrade().is_none(), "body");
                            assert!(cmdq_get_state(&*item.get()).is_some());
                            drop(item);
                            body.set(true);
                        })
                    })),
                ),
            );
            assert_eq!(cmdq_next(None), 0);
            assert!(cmdq_running().upgrade().is_none());
            tx.send(()).unwrap();
            assert!(ran.get());
            assert!(cmdq_running().ptr_eq(&head), "completed until removed");
            assert_eq!(cmdq_next(None), 0);
            assert!(head.upgrade().is_none());
            assert!(cmdq_running().upgrade().is_none());
        }
    }

    #[test]
    fn retaining_the_item_across_an_await_trips_the_post_poll_assert() {
        unsafe {
            let (_tx, rx) = channel::<()>();
            cmdq_append(
                None,
                cmdq_get_callback_owned(
                    c"retaining body",
                    Some(Box::new(move |item| {
                        let observer = Rc::downgrade(item);
                        cmdq_suspend(item, async move {
                            let item = cmdq_resumed(&observer);
                            let _ = rx.await;
                            drop(item);
                        })
                    })),
                ),
            );
            let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| cmdq_next(None)))
                .unwrap_err();
            let message = panic
                .downcast_ref::<&str>()
                .copied()
                .or_else(|| panic.downcast_ref::<String>().map(String::as_str))
                .unwrap();
            assert!(
                message.contains("retained its item across a poll"),
                "{message}"
            );
        }
    }

    #[test]
    #[should_panic(expected = "a suspended command returns cmdq_suspend's result")]
    fn suspending_then_returning_normal_panics() {
        unsafe {
            cmdq_append(
                None,
                cmdq_get_callback_owned(
                    c"mismatched return",
                    Some(Box::new(|item| {
                        let _ = cmdq_suspend(item, async {});
                        CMD_RETURN_NORMAL
                    })),
                ),
            );
            cmdq_next(None);
        }
    }

    #[test]
    #[should_panic(expected = "only an executing command item can suspend")]
    fn suspending_outside_exec_panics() {
        unsafe {
            let item = cmdq_get_callback_owned(c"idle", Some(Box::new(|_| CMD_RETURN_NORMAL)));
            let _ = cmdq_suspend(&item, async {});
        }
    }

    #[test]
    #[should_panic(expected = "command item resumed outside its poll")]
    fn resuming_outside_a_poll_panics() {
        unsafe {
            cmdq_append(
                None,
                cmdq_get_callback_owned(
                    c"resumed during exec",
                    Some(Box::new(|item| {
                        drop(cmdq_resumed(&Rc::downgrade(item)));
                        CMD_RETURN_NORMAL
                    })),
                ),
            );
            cmdq_next(None);
        }
    }

    #[test]
    fn reentrant_next_from_inside_a_body_returns_zero() {
        unsafe {
            let trace: Trace = Default::default();
            let reentered = Rc::new(RefCell::new(Vec::new()));
            let observed = reentered.clone();
            let (tx, rx) = channel::<()>();
            let head = append_suspended(c"reentrant body", async move {
                let items = cmdq_next(None);
                observed.borrow_mut().push(items);
                assert_eq!(rx.await, Some(()));
                let items = cmdq_next(None);
                observed.borrow_mut().push(items);
            });
            let next = append_recording(c"next item", &trace, "next", CMD_RETURN_NORMAL);
            assert_eq!(cmdq_next(None), 0);
            tx.send(()).unwrap();
            assert_eq!(*reentered.borrow(), [0, 0]);
            assert!(trace.borrow().is_empty());
            assert!(head.upgrade().is_some());
            assert_eq!(cmdq_next(None), 1);
            assert!(head.upgrade().is_none());
            assert!(next.upgrade().is_none());
            assert_eq!(*trace.borrow(), ["next"]);
        }
    }

    #[test]
    fn a_wake_from_another_queues_dispatch_runs_the_body_before_that_exec_returns() {
        unsafe {
            let trace: Trace = Default::default();
            let (tx, rx) = channel::<()>();
            let body = trace.clone();
            let head = append_suspended(c"global head", async move {
                assert_eq!(rx.await, Some(()));
                body.borrow_mut().push("body");
            });
            let next = append_recording(c"global next", &trace, "next", CMD_RETURN_NORMAL);
            assert_eq!(cmdq_next(None), 0);
            let owner = crate::src::server_client::queue_test_clients::with_queue();
            let exec = trace.clone();
            let sender = cmdq_append(
                Some(&owner),
                cmdq_get_callback_owned(
                    c"client sender",
                    Some(Box::new(move |_| {
                        tx.send(()).unwrap();
                        exec.borrow_mut().push("sent");
                        CMD_RETURN_NORMAL
                    })),
                ),
            );
            assert_eq!(cmdq_next(Some(&owner)), 1);
            assert!(sender.upgrade().is_none());
            assert_eq!(*trace.borrow(), ["body", "sent"]);
            assert!(
                head.upgrade().is_some(),
                "the global queue moves on at its pass"
            );
            assert_eq!(cmdq_next(None), 1);
            assert!(head.upgrade().is_none());
            assert!(next.upgrade().is_none());
            assert_eq!(*trace.borrow(), ["body", "sent", "next"]);
        }
    }

    #[test]
    fn send_after_runtime_shutdown_completes_the_item() {
        use tiri_rt::Runtime as _;
        let runtime = tiri_rt::mio::Runtime::new().unwrap();
        unsafe {
            let trace: Trace = Default::default();
            let (tx, rx) = channel::<()>();
            let body = trace.clone();
            let released = Rc::new(Cell::new(false));
            let guard = SetOnDrop(released.clone());
            let head = append_suspended(c"suspended across shutdown", async move {
                let _guard = guard;
                assert_eq!(rx.await, Some(()));
                body.borrow_mut().push("body");
            });
            let next = append_recording(c"next item", &trace, "next", CMD_RETURN_NORMAL);
            assert_eq!(cmdq_next(None), 0);
            crate::src::reactor::shutdown_runtime(runtime);
            tx.send(()).unwrap();
            assert_eq!(*trace.borrow(), ["body"]);
            assert!(released.get());
            assert_eq!(cmdq_next(None), 1);
            assert!(head.upgrade().is_none());
            assert!(next.upgrade().is_none());
            assert_eq!(*trace.borrow(), ["body", "next"]);
        }
    }
}

#[cfg(test)]
mod wait_tests {
    use super::lifecycle_tests::append_recording;
    use super::*;
    use crate::src::server_client::queue_test_clients as clients;
    use crate::src::session::session_new_for_test;
    use std::cell::RefCell;
    use std::rc::Rc;

    type Slot = Rc<RefCell<Option<CmdqWaiter>>>;

    /// A callback item that waits and leaves its waiter in `slot`.
    unsafe fn waiting(
        name: &'static CStr,
        slot: &Slot,
    ) -> std::rc::Rc<std::cell::UnsafeCell<cmdq_item>> {
        let slot = slot.clone();
        cmdq_get_callback_owned(
            name,
            Some(Box::new(move |item| {
                let (waiter, retval) = cmdq_wait(item);
                *slot.borrow_mut() = Some(waiter);
                retval
            })),
        )
    }

    /// Run `exec` with `lent` lent to a clientless item, as cmdq_fire_command
    /// does around a command's exec.
    unsafe fn with_lent_client(
        item: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
        lent: &ClientRef,
        exec: impl FnOnce() -> cmd_retval,
    ) -> cmd_retval {
        let saved = std::mem::replace(&mut (*item.get()).client, Rc::downgrade(lent));
        (*item.get()).client_lent = true;
        let retval = exec();
        (*item.get()).client = saved;
        (*item.get()).client_lent = false;
        retval
    }

    #[test]
    fn a_clientless_item_records_no_client_while_exec_sees_the_lent_one() {
        unsafe {
            let lent = clients::with_queue();
            let slot: Slot = Default::default();
            let (stored, substitute) = (slot.clone(), lent.clone());
            let head = cmdq_append(
                None,
                cmdq_get_callback_owned(
                    c"lent waiter",
                    Some(Box::new(move |item| {
                        with_lent_client(item, &substitute, || {
                            let c = cmdq_get_client(Some(&*item.get()));
                            assert!(c.is_some_and(|c| Rc::ptr_eq(&c, &substitute)));
                            let (waiter, retval) = cmdq_wait(item);
                            *stored.borrow_mut() = Some(waiter);
                            retval
                        })
                    })),
                ),
            );
            assert_eq!(cmdq_next(None), 0);
            let waiter = slot.take().unwrap();
            assert!(waiter.client().is_none());
            // The sessionless lent client would take the status if recorded.
            waiter.finish(5);
            assert_eq!(clients::return_value(&lent), 0);
            assert_eq!(cmdq_next(None), 0);
            assert!(head.upgrade().is_none());
        }
    }

    #[test]
    fn a_client_queue_item_records_its_client() {
        unsafe {
            let owner = clients::with_queue();
            let slot: Slot = Default::default();
            let head = cmdq_append(Some(&owner), waiting(c"client waiter", &slot));
            assert_eq!(cmdq_next(Some(&owner)), 0);
            let waiter = slot.take().unwrap();
            assert!(waiter.client().is_some_and(|c| Rc::ptr_eq(&c, &owner)));
            waiter.resume();
            assert_eq!(cmdq_next(Some(&owner)), 0);
            assert!(head.upgrade().is_none());
        }
    }

    #[test]
    fn an_item_inserted_behind_a_lent_item_records_the_lent_client() {
        unsafe {
            let lent = clients::with_queue();
            let slot: Slot = Default::default();
            let (inserted, substitute) = (slot.clone(), lent.clone());
            let head = cmdq_append(
                None,
                cmdq_get_callback_owned(
                    c"lent head",
                    Some(Box::new(move |item| {
                        with_lent_client(item, &substitute, || {
                            // tmux copies after->client.
                            cmdq_insert_after(item, waiting(c"inserted waiter", &inserted));
                            CMD_RETURN_NORMAL
                        })
                    })),
                ),
            );
            assert_eq!(cmdq_next(None), 1);
            assert!(head.upgrade().is_none());
            let waiter = slot.take().unwrap();
            assert!(waiter.client().is_some_and(|c| Rc::ptr_eq(&c, &lent)));
            waiter.finish(3);
            assert_eq!(clients::return_value(&lent), 3);
            assert_eq!(cmdq_next(None), 0);
        }
    }

    #[test]
    fn finish_reports_the_status_only_to_a_client_sessionless_at_finish() {
        unsafe {
            let owner = clients::with_queue();
            let session = session_new_for_test();
            clients::attach(&owner, Some(&session));
            let (first, second): (Slot, Slot) = Default::default();
            let first_item = cmdq_append(Some(&owner), waiting(c"first waiter", &first));
            let second_item = cmdq_append(Some(&owner), waiting(c"second waiter", &second));
            assert_eq!(cmdq_next(Some(&owner)), 0);
            // Attached when it waited, detached when it finishes.
            clients::attach(&owner, None);
            first.take().unwrap().finish(7);
            assert_eq!(clients::return_value(&owner), 7);
            assert_eq!(cmdq_next(Some(&owner)), 0);
            assert!(first_item.upgrade().is_none());
            // Detached when it waited, attached when it finishes.
            clients::attach(&owner, Some(&session));
            second.take().unwrap().finish(9);
            assert_eq!(clients::return_value(&owner), 7);
            assert_eq!(cmdq_next(Some(&owner)), 0);
            assert!(second_item.upgrade().is_none());
            clients::attach(&owner, None);
        }
    }

    #[test]
    fn a_dropped_waiter_never_resumes_the_item() {
        unsafe {
            let trace = Rc::new(RefCell::new(Vec::new()));
            let slot: Slot = Default::default();
            let head = cmdq_append(None, waiting(c"abandoned waiter", &slot));
            let next = append_recording(c"next item", &trace, "next", CMD_RETURN_NORMAL);
            assert_eq!(cmdq_next(None), 0);
            drop(slot.take().unwrap());
            assert!(matches!(
                (*head.upgrade().unwrap().get()).work,
                Work::Suspended { .. }
            ));
            assert_eq!(cmdq_next(None), 0);
            assert!(next.upgrade().is_some());
            assert!(trace.borrow().is_empty());

            // Dropped during exec: the first poll parks it too.
            let owner = clients::with_queue();
            let head = cmdq_append(
                Some(&owner),
                cmdq_get_callback_owned(
                    c"abandoned during exec",
                    Some(Box::new(|item| {
                        let (waiter, retval) = cmdq_wait(item);
                        drop(waiter);
                        retval
                    })),
                ),
            );
            assert_eq!(cmdq_next(Some(&owner)), 0);
            assert!(matches!(
                (*head.upgrade().unwrap().get()).work,
                Work::Suspended { .. }
            ));
        }
    }

    #[test]
    fn a_waiter_is_cancelled_once_its_item_drops() {
        unsafe {
            let owner = clients::with_queue();
            let slot: Slot = Default::default();
            let head = cmdq_append(Some(&owner), waiting(c"cancelled waiter", &slot));
            assert_eq!(cmdq_next(Some(&owner)), 0);
            let waiter = slot.take().unwrap();
            assert!(!waiter.is_cancelled());
            let client = Rc::downgrade(&owner);
            drop(owner);
            assert!(
                client.upgrade().is_none(),
                "a waiter does not retain its client"
            );
            assert!(head.upgrade().is_none());
            assert!(waiter.is_cancelled());
            assert!(waiter.client().is_none());
            waiter.resume();
        }
    }
}
