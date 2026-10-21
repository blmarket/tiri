use crate::arguments::{
    args_count, args_flag_values, args_flags, args_get, args_print_cstring, args_string,
};
use crate::cfg::{cfg_add_cause, cfg_finished};
use crate::cmd::find::cmd_find_client;
use crate::cmd::{cmd_get_args, cmd_get_entry, cmd_get_group, cmd_get_source, cmd_print_cstring};
use crate::control::{control_write, control_write_guard};
use crate::events::events_fire;
use crate::events_payload::{
    event_payload_create, event_payload_set_identity, event_payload_set_string,
    event_payload_set_target,
};
use crate::file::file_error;
use crate::format::bytes::format_message_with;
use crate::format::bytes::nullable_cstr;
use crate::format::bytes::write_cstr;
use crate::format::{format_add, format_add_cstr, format_create, format_merge};
use crate::key_string::key_string_format;
use crate::log::{log_cstr, log_debug, log_get_level};
use crate::reactor::{evbuffer_add_formatted, evbuffer_new};
use crate::server::server_add_message;
use crate::server_client::Client as _;
use crate::shared::abi::*;
use crate::shared::abi::{__uid_t, uid_t};
use crate::shared::account::passwd;
use crate::shared::client::{ClientRef, ClientWeak};
use crate::shared::client::{CLIENT_CONTROL, CLIENT_UTF8};
use crate::shared::command::*;
use crate::shared::command::{cmd, cmd_entry_flag, cmd_find_state, cmd_list, cmdq_cb, cmdq_state};
use crate::shared::command::{
    CMDQ_STATE_CONTROL, CMDQ_STATE_NOHOOKS, CMD_AFTERHOOK, CMD_CLIENT_CANFAIL, CMD_CLIENT_CFLAG,
    CMD_CLIENT_TFLAG,
};
use crate::shared::format::format_tree;
use crate::shared::format::FORMAT_NONE;
use crate::shared::key::key_event;
use crate::shared::key::*;
use crate::shared::session::SessionRef;
use crate::status::status_message_set;
use crate::text::utf8::utf8_sanitize_cstring;
use libc::{getpwuid, getuid};
use std::ffi::{CStr, CString};
use tiri_buffer::SegmentedBuf;

/// Queue-created command state. Only this module may construct or remove it.
///
/// ```compile_fail
/// use tiri::shared::command::cmdq_item;
/// let item = cmdq_item::empty();
/// ```
pub struct cmdq_item {
    pub name: Option<std::ffi::CString>,
    queue: Option<QueueTarget>,
    /// Own the remaining detached chain until enqueue consumes it.
    next: Option<std::rc::Rc<std::cell::RefCell<cmdq_item>>>,
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
    /// Runs the item when it reaches the head: `cmdq_fire_command` for a
    /// command, the queued callback otherwise. Taken when the item starts.
    start: cmdq_cb,
}

impl Drop for cmdq_item {
    fn drop(&mut self) {
        drop(self.cmdlist.take());
        drop(self.state.take());
        drop(self.start.take());
    }
}

/// A queue's dispatch: one future that starts each head in turn and awaits a
/// suspended head's continuation. It never completes.
type Drain = std::pin::Pin<Box<dyn std::future::Future<Output = ()>>>;

/// Where a queue's drain is. It is moved out while it is polled, so polling
/// never holds a queue borrow.
enum DrainSlot {
    /// Not polled yet; the first `cmdq_next` creates it.
    Unstarted,
    Parked(Drain),
    /// Being polled. `rewoken` records a wake that arrived during the poll.
    Polling {
        rewoken: bool,
    },
}

/// What the queue's head is doing, for `cmdq_running` and wakes.
#[derive(Clone, Copy, PartialEq, Eq)]
enum HeadState {
    /// No head has started, or the last one was removed.
    Idle,
    /// Inside its command or callback, or finished until `cmdq_next` removes it.
    Running,
    /// Blocks its queue on a continuation, including while it is polled.
    Suspended,
}

thread_local! {
    /// Open while `cmdq_next` polls a drain, closed while a wake does: only
    /// `cmdq_next` moves a queue past its head (tmux runs a continuation's
    /// completion before the next item only on a later `cmdq_next`).
    static CMDQ_LOOP: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Set the loop gate for one poll, restoring it when the poll returns or unwinds.
struct LoopGate(bool);

impl LoopGate {
    fn enter(open: bool) -> Self {
        Self(CMDQ_LOOP.replace(open))
    }
}

impl Drop for LoopGate {
    fn drop(&mut self) {
        CMDQ_LOOP.set(self.0);
    }
}

/// Pending until a `cmdq_next` polls the drain. `server_loop` polls every
/// queue on each pass, so the gate needs no waker.
fn loop_turn() -> impl std::future::Future<Output = ()> {
    std::future::poll_fn(|_| {
        if CMDQ_LOOP.get() {
            std::task::Poll::Ready(())
        } else {
            std::task::Poll::Pending
        }
    })
}

/// Polls a queue's suspended head on wake. It never touches the runtime, so a
/// wait may end after runtime shutdown.
struct QueueWake(QueueTarget);

impl std::task::LocalWake for QueueWake {
    fn wake(self: std::rc::Rc<Self>) {
        self.wake_by_ref();
    }
    fn wake_by_ref(self: &std::rc::Rc<Self>) {
        unsafe {
            let drain = self.0.try_with_queue(|queue| {
                if queue.head != HeadState::Suspended {
                    return None;
                }
                match std::mem::replace(&mut queue.drain, DrainSlot::Polling { rewoken: false }) {
                    DrainSlot::Parked(drain) => Some(drain),
                    DrainSlot::Polling { .. } => {
                        queue.drain = DrainSlot::Polling { rewoken: true };
                        None
                    }
                    DrainSlot::Unstarted => unreachable!("a suspended head has a drain"),
                }
            });
            if let Some(Some(drain)) = drain {
                cmdq_poll(&self.0, drain, false);
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
            start: Some(Box::new(|item| unsafe { cmdq_fire_command(item) })),
        }
    }
}

#[repr(C)]
/// Box-owned by a client until client destruction; the lazy global queue lives for
/// the process. The private deque owns stable command item allocations, and
/// the drain owns a suspended head's continuation.
///
/// ```compile_fail
/// use tiri::cmd::queue::cmdq_new;
/// let mut queue = cmdq_new();
/// queue.list.pop_front();
/// ```
pub struct cmdq_list {
    /// Dropped first, so pending continuations are cancelled before items go.
    drain: DrainSlot,
    head: HeadState,
    /// Items finished inside `cmdq_next`, read and reset by it.
    ran: u_int,
    /// Current execution position, observed without retaining the item.
    item: std::rc::Weak<std::cell::RefCell<cmdq_item>>,
    list: std::collections::VecDeque<std::rc::Rc<std::cell::RefCell<cmdq_item>>>,
    /// Detached commands belong to the server's global queue, not a client.
    background: std::collections::BTreeMap<u64, crate::reactor::Timer>,
    next_background_id: u64,
}

impl cmdq_list {
    fn first(&self) -> Option<std::rc::Rc<std::cell::RefCell<cmdq_item>>> {
        self.list.front().cloned()
    }

    fn position(&self, item: &std::rc::Rc<std::cell::RefCell<cmdq_item>>) -> usize {
        self.list
            .iter()
            .position(|owner| std::rc::Rc::ptr_eq(owner, item))
            .expect("command item belongs to queue")
    }
}

fn cmdq_new_named_item(label: &'static CStr) -> std::rc::Rc<std::cell::RefCell<cmdq_item>> {
    let owner = std::rc::Rc::new(std::cell::RefCell::new(cmdq_item::empty()));
    let address = owner.as_ptr();
    owner.borrow_mut().name = Some(format_message_with(|out| {
        out.write_all(b"[")?;
        out.write_all(label.to_bytes())?;
        write!(out, "/{address:p}]")
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
    let timer = crate::reactor::Timer::new(delay, move || unsafe {
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
        self.try_with_queue(operation)
            .expect("live command queue owner")
    }

    /// As `with_queue`, but `None` once a client queue has been freed.
    unsafe fn try_with_queue<R>(&self, operation: impl FnOnce(&mut cmdq_list) -> R) -> Option<R> {
        thread_local! {
            static GLOBAL_QUEUE: std::cell::RefCell<Box<cmdq_list>> =
                std::cell::RefCell::new(cmdq_new());
        }
        match self {
            Self::Global => Some(GLOBAL_QUEUE.with_borrow_mut(|queue| operation(queue))),
            Self::Client(observer) => {
                let owner = observer.upgrade()?;
                let mut queue = owner.borrow_queue_mut();
                Some(operation(queue))
            }
        }
    }
}

fn cmdq_name(c: Option<&ClientRef>) -> CString {
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

fn cmdq_queue_name(queue: &QueueTarget) -> CString {
    match queue {
        QueueTarget::Global => cmdq_name(None),
        QueueTarget::Client(observer) => cmdq_name(observer.upgrade().as_ref()),
    }
}

pub fn cmdq_new() -> Box<cmdq_list> {
    Box::new(cmdq_list {
        drain: DrainSlot::Unstarted,
        head: HeadState::Idle,
        ran: 0,
        item: std::rc::Weak::new(),
        list: std::collections::VecDeque::new(),
        background: std::collections::BTreeMap::new(),
        next_background_id: 0,
    })
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
pub fn cmdq_get_source(item: &cmdq_item) -> &cmd_find_state {
    &item.source
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
    current: Option<&cmd_find_state>,
    event: Option<&key_event>,
    mut flags: ::core::ffi::c_int,
) -> std::rc::Rc<cmdq_state> {
    let snapshot = if let Some(event) = event {
        event.metadata_snapshot()
    } else {
        key_event {
            client: std::rc::Weak::new(),
            key: KEYC_NONE as key_code,
            m: Default::default(),
            bytes: None,
        }
    };
    let mut target = cmd_find_state::default();
    if let Some(current) = current.filter(|current| current.is_valid()) {
        target.copy_from(current);
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
    current: Option<&cmd_find_state>,
) -> std::rc::Rc<cmdq_state> {
    let event = state.event.metadata_snapshot();
    if let Some(current) = current {
        return cmdq_new_state(Some(current), Some(&event), state.flags);
    }
    let current = state.current_snapshot();
    cmdq_new_state(Some(&current), Some(&event), state.flags)
}
/// Copy a literal key and value into the command queue state's formats.
pub unsafe fn cmdq_add_format(state: &cmdq_state, key: &CStr, value: &CStr) {
    let mut formats = state.formats.borrow_mut();
    let formats = formats
        .get_or_insert_with(|| format_create(None, None, FORMAT_NONE, 0 as ::core::ffi::c_int));
    format_add_cstr(formats, key, value);
}
pub unsafe fn cmdq_add_formats(state: &cmdq_state, ft: &format_tree) {
    let mut formats = state.formats.borrow_mut();
    let formats = formats
        .get_or_insert_with(|| format_create(None, None, FORMAT_NONE, 0 as ::core::ffi::c_int));
    format_merge(formats, ft);
}
pub unsafe fn cmdq_merge_formats(
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
    ft: &mut format_tree,
) {
    let (command, state) = {
        let item = item_handle.borrow();
        (item.command_handle(), cmdq_get_state_owned(&item))
    };
    if command.is_alive() {
        let entry = cmd_get_entry(&command.try_borrow().expect("live command"));
        format_add(ft, c"command", |out| write_cstr(out, &*entry.name));
    }
    let mut formats = state.formats.borrow_mut();
    if formats.is_some() {
        format_merge(ft, formats.as_deref().expect("command formats"));
    }
}
/// Consume a detached chain, moving its owners into the queue in order.
pub unsafe fn cmdq_append(
    owner: Option<&ClientRef>,
    item: std::rc::Rc<std::cell::RefCell<cmdq_item>>,
) -> std::rc::Weak<std::cell::RefCell<cmdq_item>> {
    let queue = QueueTarget::for_client(owner);
    let mut remaining = Some(item);
    let mut last = std::rc::Weak::new();
    while let Some(owner_item) = remaining {
        let name = {
            let mut item = owner_item.borrow_mut();
            assert!(item.queue.is_none(), "detached command item");
            remaining = item.next.take();
            item.client = owner.map(std::rc::Rc::downgrade).unwrap_or_default();
            item.queue = Some(queue.clone());
            item.name.clone()
        };
        last = std::rc::Rc::downgrade(&owner_item);
        queue.with_queue(|queue| queue.list.push_back(owner_item));
        log_debug(format_args!(
            "{} {}: {}",
            "cmdq_append",
            crate::log::log_bytes(cmdq_name(owner).as_bytes()),
            log_cstr(name.as_deref().unwrap_or(c"(null)"))
        ));
    }
    last
}

/// Consume a detached chain and return the final queued insertion anchor.
pub unsafe fn cmdq_insert_after(
    after: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
    item: std::rc::Rc<std::cell::RefCell<cmdq_item>>,
) -> std::rc::Weak<std::cell::RefCell<cmdq_item>> {
    let (c_owner, queue, mut previous) = {
        let after = after.borrow();
        let queue = after.queue.clone().expect("queued insertion anchor");
        (after.client.upgrade(), queue, after.name.clone())
    };
    let mut position = queue.with_queue(|queue| queue.position(after) + 1);
    let mut remaining = Some(item);
    let mut last = std::rc::Weak::new();
    while let Some(owner_item) = remaining {
        let name = {
            let mut item = owner_item.borrow_mut();
            assert!(item.queue.is_none(), "detached command item");
            remaining = item.next.take();
            item.client = c_owner
                .as_ref()
                .map(std::rc::Rc::downgrade)
                .unwrap_or_default();
            item.queue = Some(queue.clone());
            item.name.clone()
        };
        last = std::rc::Rc::downgrade(&owner_item);
        queue.with_queue(|queue| queue.list.insert(position, owner_item));
        position += 1;
        log_debug(format_args!(
            "{} {}: {} after {}",
            "cmdq_insert_after",
            crate::log::log_bytes(cmdq_name(c_owner.as_ref()).as_bytes()),
            log_cstr(name.as_deref().unwrap_or(c"(null)")),
            log_cstr(previous.as_deref().unwrap_or(c"(null)"))
        ));
        previous = name;
    }
    last
}
pub unsafe fn cmdq_insert_hook(
    _s_owner: Option<&SessionRef>,
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
    current: Option<&cmd_find_state>,
    write: impl FnOnce(&mut dyn std::io::Write) -> std::io::Result<()>,
) {
    let (cmd, flags) = {
        let item = item_handle.borrow();
        (item.command_handle(), cmdq_get_flags(&item))
    };
    let cmd_ref = cmd.try_borrow().expect("live command");
    let args_0 = cmd_get_args(&cmd_ref);
    let mut i: u_int = 0;
    let mut value: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    if flags & CMDQ_STATE_NOHOOKS != 0 {
        return;
    }
    let name = format_message_with(write);
    let mut ep = event_payload_create();
    if let Some(current) = current {
        event_payload_set_target(&mut ep, current);
    }
    event_payload_set_identity(
        &mut ep,
        c"_cmdq_item",
        crate::shared::events::EventPayloadIdentity::QueueItem(std::rc::Rc::downgrade(item_handle)),
    );
    let arguments = args_print_cstring(args_0);
    event_payload_set_string(&mut ep, c"arguments", |out| write_cstr(out, &*arguments));
    i = 0 as u_int;
    while i < args_count(args_0) {
        let key = format_message_with(|out| write!(out, "argument_{}", i));
        event_payload_set_string(&mut ep, &key, |out| {
            write_cstr(
                out,
                nullable_cstr(
                    args_string(args_0, i).map_or(std::ptr::null(), |value| value.as_ptr()),
                ),
            )
        });
        i = i.wrapping_add(1);
    }
    for flag in args_flags(args_0) {
        value = args_get(args_0, flag as u_char).map_or(std::ptr::null(), |value| value.as_ptr());
        let key = format_message_with(|out| {
            out.write_all(b"flag_")?;
            out.write_all(&[flag])
        });
        if value.is_null() {
            event_payload_set_string(&mut ep, &key, |out| out.write_all(b"1"));
        } else {
            event_payload_set_string(&mut ep, &key, |out| write_cstr(out, nullable_cstr(value)));
        }
        i = 0 as u_int;
        for av in args_flag_values(args_0, flag as u_char) {
            let key = format_message_with(|out| {
                out.write_all(b"flag_")?;
                out.write_all(&[flag])?;
                write!(out, "_{}", i)
            });
            event_payload_set_string(&mut ep, &key, |out| write_cstr(out, av.as_string()));
            i = i.wrapping_add(1);
        }
    }
    events_fire(&name, ep);
}
/// Retain the item from inside its continuation. Only valid while it is
/// polled; drop the handle before the next `.await`.
pub(crate) unsafe fn cmdq_resumed(
    item: &std::rc::Weak<std::cell::RefCell<cmdq_item>>,
) -> std::rc::Rc<std::cell::RefCell<cmdq_item>> {
    let owner = item.upgrade().expect("polled command item");
    let queue = owner.borrow().queue.clone().expect("queued command item");
    let polled = queue.with_queue(|queue| {
        queue.head == HeadState::Suspended
            && matches!(queue.drain, DrainSlot::Polling { .. })
            && queue.item.ptr_eq(item)
    });
    assert!(polled, "command item resumed outside its poll");
    owner
}
/// The head once a `cmdq_next` polls the drain; new items start only there.
async unsafe fn cmdq_next_head(queue: &QueueTarget) -> std::rc::Rc<std::cell::RefCell<cmdq_item>> {
    std::future::poll_fn(|_| {
        if !CMDQ_LOOP.get() {
            return std::task::Poll::Pending;
        }
        match queue.try_with_queue(|queue| queue.first()) {
            Some(Some(item)) => std::task::Poll::Ready(item),
            _ => std::task::Poll::Pending,
        }
    })
    .await
}
/// Start each head in turn. A suspended head's continuation is awaited here,
/// so a wake polls it at once; its completion waits for a `cmdq_next` before
/// the item is removed. No strong item handle is held across an `.await`.
async unsafe fn cmdq_drain(queue: QueueTarget) {
    static mut number: u_int = 0;
    loop {
        let item_owner = cmdq_next_head(&queue).await;
        let start = {
            let mut item = item_owner.borrow_mut();
            log_debug(format_args!(
                "cmdq_next {}: {}",
                crate::log::log_bytes(cmdq_queue_name(&queue).as_bytes()),
                crate::log::log_bytes(item.name.as_deref().unwrap_or(c"(null)").to_bytes()),
            ));
            item.start.take().expect("queue head starts once")
        };
        queue.with_queue(|queue| {
            queue.item = std::rc::Rc::downgrade(&item_owner);
            queue.head = HeadState::Running;
        });
        number = number.wrapping_add(1);
        {
            let mut item = item_owner.borrow_mut();
            item.time = crate::shared::time::unix_seconds(std::time::SystemTime::now());
            item.number = number;
        }
        let counted = match start(&item_owner) {
            CmdOutcome::Suspend(continuation) => {
                queue.with_queue(|queue| queue.head = HeadState::Suspended);
                drop(item_owner);
                continuation.await;
                // Only a completion inside `cmdq_next` counts toward its result.
                let counted = CMDQ_LOOP.get();
                if queue
                    .try_with_queue(|queue| queue.head = HeadState::Running)
                    .is_none()
                {
                    return std::future::pending().await;
                }
                loop_turn().await;
                counted
            }
            outcome => {
                if matches!(outcome, CmdOutcome::Error) {
                    cmdq_remove_group(&item_owner);
                }
                drop(item_owner);
                true
            }
        };
        let owner = queue
            .with_queue(|queue| {
                queue.head = HeadState::Idle;
                queue.item = std::rc::Weak::new();
                if counted {
                    queue.ran += 1;
                }
                queue.list.pop_front()
            })
            .expect("executed queue item");
        cmdq_remove(owner);
    }
}
/// Poll `queue`'s drain outside any queue borrow, again while it is woken
/// during the poll, then park it. `open` opens the loop gate (`cmdq_next`).
/// A continuation may free its own client queue; the drain is then dropped.
unsafe fn cmdq_poll(queue: &QueueTarget, mut drain: Drain, open: bool) {
    let _gate = LoopGate::enter(open);
    let waker = std::task::LocalWaker::from(std::rc::Rc::new(QueueWake(queue.clone())));
    let mut context = std::task::ContextBuilder::from_waker(std::task::Waker::noop())
        .local_waker(&waker)
        .build();
    loop {
        assert!(
            drain.as_mut().poll(&mut context).is_pending(),
            "a queue drain never completes"
        );
        let polled = queue.try_with_queue(|queue| {
            let head = if queue.head == HeadState::Suspended {
                queue.item.clone()
            } else {
                Default::default()
            };
            let DrainSlot::Polling { rewoken } =
                std::mem::replace(&mut queue.drain, DrainSlot::Polling { rewoken: false })
            else {
                panic!("a polled drain stays out until its poll returns");
            };
            (head, rewoken)
        });
        let Some((head, rewoken)) = polled else {
            drop(drain);
            return;
        };
        if let Some(head) = head.upgrade() {
            assert!(
                std::rc::Rc::strong_count(&head) <= 2,
                "command continuation retained its item across a poll"
            );
        }
        if !rewoken {
            break;
        }
    }
    let mut drain = Some(drain);
    queue.try_with_queue(|queue| queue.drain = DrainSlot::Parked(drain.take().unwrap()));
    drop(drain);
}
/// The resuming end of a plain wait. Dropping it never resumes the item.
pub struct CmdqWaiter {
    /// The waiting item's own client, not one lent for its dispatch.
    client: ClientWeak,
    done: tiri_rt::oneshot::Sender<()>,
}
/// Suspend the executing item until its waiter resumes it, and return the
/// waiter with the outcome its command or callback must return.
#[must_use = "return the suspension from the command"]
pub(crate) fn cmdq_wait(
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
) -> (CmdqWaiter, CmdOutcome) {
    let client = {
        let item = item_handle.borrow();
        if item.client_lent {
            std::rc::Weak::new()
        } else {
            item.client.clone()
        }
    };
    let (done, resumed) = tiri_rt::oneshot::channel();
    let outcome = CmdOutcome::Suspend(Box::pin(async move {
        if resumed.await.is_none() {
            std::future::pending::<()>().await;
        }
    }));
    (CmdqWaiter { client, done }, outcome)
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
    pub(crate) fn resume(self) {
        let _ = self.done.send(());
    }
    /// Report `status` as the return value of a client without a session at
    /// this moment, then resume (tmux's popup_free_cb, window_pane_wait_finish).
    pub(crate) fn finish(self, status: i32) {
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
fn cmdq_remove(item_handle: std::rc::Rc<std::cell::RefCell<cmdq_item>>) {
    assert_eq!(
        std::rc::Rc::strong_count(&item_handle),
        1,
        "command item removal requires its sole strong owner"
    );
}
unsafe fn cmdq_remove_group(item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>) {
    let (group, queue) = {
        let item = item_handle.borrow();
        (item.group, item.queue.clone())
    };
    if group == 0 as u_int {
        return;
    }
    let queue = queue.expect("queued group anchor");
    let mut position = queue.with_queue(|queue| queue.position(item_handle) + 1);
    loop {
        let next = queue.with_queue(|queue| {
            let owner = queue.list.get(position)?;
            if owner.borrow().group == group {
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
) -> std::rc::Rc<std::cell::RefCell<cmdq_item>> {
    let cmdlist = commands.borrow();
    if cmdlist.list.is_empty() {
        return cmdq_get_callback_owned(
            c"cmdq_empty_command",
            Some(Box::new(|_| CmdOutcome::Normal)),
        );
    }
    let state = state
        .cloned()
        .unwrap_or_else(|| cmdq_new_state(None, None, 0));
    let mut remaining = None;
    for command in cmdlist.list.iter().rev() {
        let cmd = command.downgrade();
        let owner =
            cmdq_new_named_item(cmd_get_entry(&cmd.try_borrow().expect("live command")).name);
        {
            let mut item = owner.borrow_mut();
            item.group = cmd_get_group(cmd.clone());
            item.state = Some(state.clone());
            item.cmd = command.downgrade();
            item.cmdlist = Some(commands.clone());
            item.next = remaining;
        }
        remaining = Some(owner);
    }
    remaining.expect("nonempty command list")
}
/// Resolve `flag` outside any item borrow, then store it in the `slot` state.
unsafe fn cmdq_find_flag_into(
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
    flag: &cmd_entry_flag,
    slot: impl FnOnce(&mut cmdq_item) -> &mut cmd_find_state,
) -> CmdOutcome {
    let mut fs = cmd_find_state::default();
    let retval = cmdq_find_flag(item_handle, &mut fs, flag);
    *slot(&mut item_handle.borrow_mut()) = fs;
    retval
}
unsafe fn cmdq_find_flag(
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
    fs: &mut cmd_find_state,
    flag: &cmd_entry_flag,
) -> CmdOutcome {
    let mut value: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    if flag.flag == 0 {
        let target_client = cmdq_get_target_client(Some(&item_handle.borrow()));
        *fs = cmd_find_state::find_from_client(target_client.as_ref(), 0 as ::core::ffi::c_int)
            .unwrap_or_else(|| cmd_find_state::clear_state(0 as ::core::ffi::c_int));
        return CmdOutcome::Normal;
    }
    value = args_get(
        cmd_get_args(
            &item_handle
                .borrow()
                .command_handle()
                .try_borrow()
                .expect("live command"),
        ),
        flag.flag,
    )
    .map_or(std::ptr::null(), |value| value.as_ptr());
    if fs.find_target(
        Some(item_handle),
        nullable_cstr(value),
        flag.type_0,
        flag.flags,
    ) != 0 as ::core::ffi::c_int
    {
        *fs = cmd_find_state::clear_state(0 as ::core::ffi::c_int);
        return CmdOutcome::Error;
    }
    CmdOutcome::Normal
}
unsafe fn cmdq_add_message(item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>) {
    let (c_owner, state, command) = {
        let item = item_handle.borrow();
        (
            cmdq_get_client(Some(&item)),
            cmdq_get_state_owned(&item),
            item.command_handle(),
        )
    };
    let mut c: Option<ClientRef> = c_owner.clone();
    let mut uid: uid_t = 0;
    let mut pw: *mut passwd = ::core::ptr::null_mut::<passwd>();
    let tmp = cmd_print_cstring(&command.try_borrow().expect("live command"));
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
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
) -> CmdOutcome {
    let mut current_block: u64;
    let (saved_client, state, cmd, saved, group) = {
        let item = item_handle.borrow();
        (
            cmdq_get_client(Some(&item)),
            cmdq_get_state_owned(&item),
            item.command_handle(),
            item.client.clone(),
            item.group,
        )
    };
    let name = cmdq_name(saved_client.as_ref());
    let cmd_ref = cmd.try_borrow().expect("live command");
    let args = cmd_get_args(&cmd_ref);
    let entry = cmd_get_entry(&cmd_ref);
    let mut tc = None;
    let mut retval: CmdOutcome = CmdOutcome::Normal;
    let mut fsp: Option<&cmd_find_state> = None;
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
    let client_freed =
        saved_client.is_none() && !std::rc::Weak::ptr_eq(&saved, &std::rc::Weak::new());
    if client_freed || saved_client.as_ref().is_some_and(|c| c.is_dead()) {
        return CmdOutcome::Error;
    }
    if cfg_finished != 0 {
        cmdq_add_message(item_handle);
    }
    if log_get_level() > 1 as ::core::ffi::c_int {
        let tmp = cmd_print_cstring(&cmd_ref);
        log_debug(format_args!(
            "{} {}: ({}) {}",
            "cmdq_fire_command",
            crate::log::log_bytes(name.as_bytes()),
            group,
            crate::log::log_bytes(tmp.as_bytes())
        ));
    }
    flags = (state.flags & CMDQ_STATE_CONTROL != 0) as ::core::ffi::c_int;
    cmdq_guard(item_handle, c"begin", flags);
    if item_handle.borrow().client.upgrade().is_none() {
        let context = cmd_find_client(
            Some(item_handle),
            nullable_cstr(::core::ptr::null::<::core::ffi::c_char>()),
            1 as ::core::ffi::c_int,
        );
        let mut item = item_handle.borrow_mut();
        item.client = context
            .as_ref()
            .map_or_else(std::rc::Weak::new, std::rc::Rc::downgrade);
        item.client_lent = true;
    }
    let execution_client = cmdq_get_client(Some(&item_handle.borrow()));
    if entry.flags & CMD_CLIENT_CANFAIL != 0 {
        quiet = 1 as ::core::ffi::c_int;
    }
    if entry.flags & CMD_CLIENT_CFLAG != 0 {
        tc = cmd_find_client(
            Some(item_handle),
            nullable_cstr(
                args_get(args, 'c' as i32 as u_char)
                    .map_or(std::ptr::null(), |value| value.as_ptr()),
            ),
            quiet,
        );
        if tc.is_none() && quiet == 0 {
            retval = CmdOutcome::Error;
            current_block = 7379054416801212160;
        } else {
            current_block = 18317007320854588510;
        }
    } else if entry.flags & CMD_CLIENT_TFLAG != 0 {
        tc = cmd_find_client(
            Some(item_handle),
            nullable_cstr(
                args_get(args, 't' as i32 as u_char)
                    .map_or(std::ptr::null(), |value| value.as_ptr()),
            ),
            quiet,
        );
        if tc.is_none() && quiet == 0 {
            retval = CmdOutcome::Error;
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
        item_handle.borrow_mut().target_client = tc
            .as_ref()
            .map_or_else(std::rc::Weak::new, std::rc::Rc::downgrade);
        retval = cmdq_find_flag_into(item_handle, &entry.source, |item| &mut item.source);
        if !matches!(retval, CmdOutcome::Error) {
            retval = cmdq_find_flag_into(item_handle, &entry.target, |item| &mut item.target);
            if !matches!(retval, CmdOutcome::Error) {
                retval = entry.exec.expect("non-null function pointer")(cmd.clone(), item_handle);
                if !matches!(retval, CmdOutcome::Error) && entry.flags & CMD_AFTERHOOK != 0 {
                    let target = cmdq_get_target(&item_handle.borrow()).clone();
                    if target.is_valid() {
                        fs = target;
                        fsp = Some(&fs);
                        current_block = 8704759739624374314;
                    } else if state.current.borrow().is_valid() {
                        fs = state.current_snapshot();
                        fsp = Some(&fs);
                        current_block = 8704759739624374314;
                    } else if let Some(found) = cmd_find_state::find_from_client(
                        execution_client.as_ref(),
                        0 as ::core::ffi::c_int,
                    ) {
                        fs = found;
                        fsp = Some(&fs);
                        current_block = 8704759739624374314;
                    } else {
                        current_block = 7379054416801212160;
                    }
                    match current_block {
                        7379054416801212160 => {}
                        _ => {
                            cmdq_insert_hook(
                                fsp.and_then(|fs| fs.session_handle()).as_ref(),
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
    {
        let mut item = item_handle.borrow_mut();
        item.client = saved;
        item.client_lent = false;
    }
    if matches!(retval, CmdOutcome::Error) {
        fsp = None;
        let target = cmdq_get_target(&item_handle.borrow()).clone();
        if target.is_valid() {
            fs = target;
            fsp = Some(&fs);
        } else if state.current.borrow().is_valid() {
            fs = state.current_snapshot();
            fsp = Some(&fs);
        } else if let Some(found) =
            cmd_find_state::find_from_client(saved_client.as_ref(), 0 as ::core::ffi::c_int)
        {
            fs = found;
            fsp = Some(&fs);
        }
        cmdq_insert_hook(
            fsp.and_then(|fs| fs.session_handle()).as_ref(),
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
) -> std::rc::Rc<std::cell::RefCell<cmdq_item>> {
    let owner = cmdq_new_named_item(name);
    {
        let mut item = owner.borrow_mut();
        item.group = 0 as u_int;
        item.state = Some(cmdq_new_state(None, None, 0 as ::core::ffi::c_int));
        item.start = Some(cb.expect("non-null queue callback"));
    }
    owner
}

#[must_use = "enqueue the detached command chain"]
pub unsafe fn cmdq_get_error(error: &CStr) -> std::rc::Rc<std::cell::RefCell<cmdq_item>> {
    let error = error.to_owned();
    cmdq_get_callback_owned(
        c"cmdq_error_callback",
        Some(Box::new(move |item| unsafe {
            cmdq_error(item, |out| write_cstr(out, &*error));
            CmdOutcome::Normal
        })),
    )
}
/// Run the queue until it is empty or its head suspends, and return how many
/// items finished. Re-entered while its drain is polled, it runs nothing.
pub unsafe fn cmdq_next(owner: Option<&ClientRef>) -> u_int {
    let queue = QueueTarget::for_client(owner);
    log_debug(format_args!(
        "cmdq_next {}: enter",
        crate::log::log_bytes(cmdq_name(owner).as_bytes())
    ));
    let drain = queue.with_queue(|queue| {
        match std::mem::replace(&mut queue.drain, DrainSlot::Polling { rewoken: false }) {
            DrainSlot::Unstarted => {
                Some(Box::pin(cmdq_drain(QueueTarget::for_client(owner))) as Drain)
            }
            DrainSlot::Parked(drain) => Some(drain),
            polling @ DrainSlot::Polling { .. } => {
                queue.drain = polling;
                None
            }
        }
    });
    let Some(drain) = drain else {
        return 0;
    };
    cmdq_poll(&queue, drain, true);
    queue.with_queue(|queue| std::mem::take(&mut queue.ran))
}

/// The global queue's current item while its command or callback runs, or
/// after it completed until it is removed. None while it is suspended,
/// including while its continuation runs, as tmux hides a waiting item.
pub unsafe fn cmdq_running() -> std::rc::Weak<std::cell::RefCell<cmdq_item>> {
    QueueTarget::Global.with_queue(|queue| match queue.head {
        HeadState::Suspended => std::rc::Weak::new(),
        HeadState::Idle | HeadState::Running => queue.item.clone(),
    })
}
pub unsafe fn cmdq_guard(
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
    guard: &CStr,
    mut flags: ::core::ffi::c_int,
) {
    let (c_owner, time, number) = {
        let item = item_handle.borrow();
        (cmdq_get_client(Some(&item)), item.time, item.number)
    };
    let mut c: Option<ClientRef> = c_owner.clone();
    let mut t: ::core::ffi::c_long = time as ::core::ffi::c_long;
    if !c.is_none() && c.as_ref().expect("live client").flags() & CLIENT_CONTROL as uint64_t != 0 {
        control_write_guard(&c.clone().expect("live client"), guard, t, number, flags);
    }
}
pub fn cmdq_print_data(
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
    mut evb: &mut SegmentedBuf,
) {
    let client = cmdq_get_client(Some(&item_handle.borrow()));
    ClientRef::print_to(client.as_ref(), (1 as ::core::ffi::c_int) != 0, evb);
}
pub fn cmdq_print(
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
    write: impl FnOnce(&mut dyn std::io::Write) -> std::io::Result<()>,
) {
    let mut evb = evbuffer_new();
    evbuffer_add_formatted(&mut evb, write);
    cmdq_print_data(item_handle, &mut evb);
}
pub unsafe fn cmdq_error(
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
    write: impl FnOnce(&mut dyn std::io::Write) -> std::io::Result<()>,
) {
    let (c_owner, cmd) = {
        let item = item_handle.borrow();
        (cmdq_get_client(Some(&item)), item.command_handle())
    };
    let mut c: Option<ClientRef> = c_owner.clone();
    let mut file: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    let mut line: u_int = 0;
    let mut msg = format_message_with(write);
    log_debug(format_args!(
        "{}: {}",
        "cmdq_error",
        crate::log::log_bytes(msg.as_bytes())
    ));
    match c.as_ref() {
        None => {
            let cmd_ref = cmd.try_borrow().expect("live command");
            let (source, source_line) = cmd_get_source(&cmd_ref);
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
            bytes[0] = bytes[0].to_ascii_uppercase();
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
    ) -> std::rc::Rc<std::cell::RefCell<cmdq_item>> {
        let capture = SetOnDrop(released.clone());
        cmdq_get_callback_owned(
            name,
            Some(Box::new(move |_| {
                let _capture = &capture;
                CmdOutcome::Normal
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
                        CmdOutcome::Normal
                    })),
                ),
            );
            assert_eq!(cmdq_next(None), 1);
            assert!(ran.get());
            assert!(item.upgrade().is_none());
            assert!(cmdq_running().upgrade().is_none());
        }
    }

    /// Append a global callback item that records `tag` when it runs.
    pub(super) unsafe fn append_recording(
        name: &'static CStr,
        trace: &Rc<std::cell::RefCell<Vec<&'static str>>>,
        tag: &'static str,
        retval: CmdOutcome,
    ) -> std::rc::Weak<std::cell::RefCell<cmdq_item>> {
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
                        CmdOutcome::Normal
                    })),
                ),
            );
            let next = append_recording(c"next item", &trace, "next", CmdOutcome::Normal);
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

    use tiri_rt::oneshot::channel;

    type Trace = Rc<RefCell<Vec<&'static str>>>;

    /// Append a global callback item that suspends on `future`.
    unsafe fn append_suspended(
        name: &'static CStr,
        future: impl Future<Output = ()> + 'static,
    ) -> Weak<std::cell::RefCell<cmdq_item>> {
        cmdq_append(
            None,
            cmdq_get_callback_owned(
                name,
                Some(Box::new(move |_| CmdOutcome::Suspend(Box::pin(future)))),
            ),
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
            let next = append_recording(c"next item", &trace, "next", CmdOutcome::Normal);
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
                    Some(Box::new(move |_| {
                        let (tx, rx) = channel::<()>();
                        let inner = body.clone();
                        let retval = CmdOutcome::Suspend(Box::pin(async move {
                            assert_eq!(rx.await, Some(()));
                            inner.borrow_mut().push("body");
                        }));
                        tx.send(()).unwrap();
                        assert!(body.borrow().is_empty(), "the first poll follows exec");
                        retval
                    })),
                ),
            );
            let next = append_recording(c"next item", &trace, "next", CmdOutcome::Normal);
            assert_eq!(cmdq_next(None), 2);
            assert!(head.upgrade().is_none());
            assert!(next.upgrade().is_none());
            assert_eq!(*trace.borrow(), ["body", "next"]);
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
                        CmdOutcome::Suspend(Box::pin(async move {
                            assert!(cmdq_running().upgrade().is_none(), "first poll");
                            assert_eq!(rx.await, Some(()));
                            let item = cmdq_resumed(&observer);
                            assert!(cmdq_running().upgrade().is_none(), "body");
                            assert!(cmdq_get_state(&item.borrow()).is_some());
                            drop(item);
                            body.set(true);
                        }))
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
                        CmdOutcome::Suspend(Box::pin(async move {
                            let item = cmdq_resumed(&observer);
                            let _ = rx.await;
                            drop(item);
                        }))
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
    #[should_panic(expected = "command item resumed outside its poll")]
    fn resuming_outside_a_poll_panics() {
        unsafe {
            cmdq_append(
                None,
                cmdq_get_callback_owned(
                    c"resumed during exec",
                    Some(Box::new(|item| {
                        drop(cmdq_resumed(&Rc::downgrade(item)));
                        CmdOutcome::Normal
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
            let next = append_recording(c"next item", &trace, "next", CmdOutcome::Normal);
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
            let next = append_recording(c"global next", &trace, "next", CmdOutcome::Normal);
            assert_eq!(cmdq_next(None), 0);
            let owner = crate::server_client::queue_test_clients::with_queue();
            let exec = trace.clone();
            let sender = cmdq_append(
                Some(&owner),
                cmdq_get_callback_owned(
                    c"client sender",
                    Some(Box::new(move |_| {
                        tx.send(()).unwrap();
                        exec.borrow_mut().push("sent");
                        CmdOutcome::Normal
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
            let next = append_recording(c"next item", &trace, "next", CmdOutcome::Normal);
            assert_eq!(cmdq_next(None), 0);
            crate::reactor::shutdown_runtime(runtime);
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
    use super::*;
    use crate::server_client::queue_test_clients as clients;
    use crate::session::session_new_for_test;
    use std::cell::RefCell;
    use std::rc::Rc;

    type Slot = Rc<RefCell<Option<CmdqWaiter>>>;

    /// A callback item that waits and leaves its waiter in `slot`.
    unsafe fn waiting(
        name: &'static CStr,
        slot: &Slot,
    ) -> std::rc::Rc<std::cell::RefCell<cmdq_item>> {
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
    fn with_lent_client(
        item: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
        lent: &ClientRef,
        exec: impl FnOnce() -> CmdOutcome,
    ) -> CmdOutcome {
        let saved = {
            let mut item = item.borrow_mut();
            item.client_lent = true;
            std::mem::replace(&mut item.client, Rc::downgrade(lent))
        };
        let retval = exec();
        let mut item = item.borrow_mut();
        item.client = saved;
        item.client_lent = false;
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
                            let c = cmdq_get_client(Some(&item.borrow()));
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
                            cmdq_insert_after(item, waiting(c"inserted waiter", &inserted));
                            CmdOutcome::Normal
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
            clients::attach(&owner, None);
            first.take().unwrap().finish(7);
            assert_eq!(clients::return_value(&owner), 7);
            assert_eq!(cmdq_next(Some(&owner)), 0);
            assert!(first_item.upgrade().is_none());
            clients::attach(&owner, Some(&session));
            second.take().unwrap().finish(9);
            assert_eq!(clients::return_value(&owner), 7);
            assert_eq!(cmdq_next(Some(&owner)), 0);
            assert!(second_item.upgrade().is_none());
            clients::attach(&owner, None);
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
