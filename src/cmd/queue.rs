use crate::src::arguments::{
    args_count, args_flag_values, args_flags, args_get, args_print_cstring, args_string,
};
use crate::src::cfg::{cfg_add_cause, cfg_finished};
use crate::src::cmd::find::{
    cmd_find_client, cmd_find_copy_state, cmd_find_target, cmd_find_valid_state,
};
use crate::src::cmd::{
    cmd_get_args_mut, cmd_get_entry, cmd_get_group, cmd_get_source, cmd_print_cstring,
};
use crate::src::control::{control_write, control_write_guard};
use crate::src::events::events_fire;
use crate::src::events_payload::{
    event_payload_create, event_payload_set_identity, event_payload_set_string,
    event_payload_set_target,
};
use crate::src::ffi::libc::{__ctype_toupper_loc, getpwuid, getuid, time};
use crate::src::file::{file_cancel_cmdq_wait, file_error};
use crate::src::format::bytes::format_message_with;
use crate::src::format::bytes::nullable_cstr;
use crate::src::format::bytes::write_cstr;
use crate::src::format::bytes::{xformat, xformat_with};
use crate::src::format::{
    format_add, format_add_cstr, format_create_owned, format_merge, format_owner_ptr,
};
use crate::src::key_string::key_string_format;
use crate::src::log::{fatalx, log_cstr, log_debug, log_get_level};
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
    cmd, cmd_entry_flag, cmd_find_state, cmd_list, cmdq_cb, cmdq_state, cmdq_type,
};
use crate::src::shared::command::{
    CMDQ_FIRED, CMDQ_STATE_CONTROL, CMDQ_STATE_NOHOOKS, CMDQ_WAITING, CMD_AFTERHOOK,
    CMD_CLIENT_CANFAIL, CMD_CLIENT_CFLAG, CMD_CLIENT_TFLAG,
};
use crate::src::shared::format::format_tree;
use crate::src::shared::format::FORMAT_NONE;
use crate::src::shared::key::key_event;
use crate::src::shared::key::*;
use crate::src::shared::session::SessionRef;
use crate::src::status::status_message_set;
use crate::src::text::utf8::utf8_sanitize_cstring;
use hmux_buffer::SegmentedBuf;
use std::ffi::{CStr, CString};

/// Queue-created command state. Only this module may construct or remove it.
///
/// ```compile_fail
/// use hmux::src::shared::command::cmdq_item;
/// let item = cmdq_item::empty();
/// ```
pub struct cmdq_item {
    pub name: Option<std::ffi::CString>,
    queue: Option<QueueTarget>,
    /// Own the remaining detached chain until enqueue consumes it.
    next: Option<std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>>,
    /// Runtime destruction marker: only cmdq_remove may set this to true.
    removed: bool,
    /// Nonowning execution context, which can temporarily differ from the queue owner.
    pub client: ClientWeak,
    client_owner: Option<ClientRef>,
    /// Nonowning command target; upgrade before accessing the client.
    pub target_client: ClientWeak,
    pub type_0: cmdq_type,
    pub group: u_int,
    pub number: u_int,
    pub time: time_t,
    pub flags: ::core::ffi::c_int,
    pub state: Option<std::rc::Rc<cmdq_state>>,
    pub source: cmd_find_state,
    pub target: cmd_find_state,
    cmdlist: Option<std::rc::Rc<std::cell::RefCell<cmd_list>>>,
    pub cmd: refbox::Weak<cmd>,
    pub cb: cmdq_cb,
    cancel_data: Option<Box<dyn FnOnce()>>,
    /// A foreground command owns its delayed work until dispatch or removal.
    wait_timer: Option<crate::src::reactor::Timer>,
    file_task: Option<hmux_rt::mio::Task>,
    wait_file: std::rc::Weak<std::cell::UnsafeCell<client_file>>,
}

impl Drop for cmdq_item {
    fn drop(&mut self) {
        assert!(self.removed, "command item dropped without cmdq_remove");
    }
}

impl cmdq_item {
    pub(crate) fn is_removed(&self) -> bool {
        self.removed
    }

    /// Observe the command retained by this queue item's command list.
    pub fn command_handle(&self) -> refbox::Weak<cmd> {
        self.cmd.clone()
    }

    fn empty() -> Self {
        Self {
            name: Default::default(),
            queue: Default::default(),
            next: Default::default(),
            removed: false,
            client: Default::default(),
            client_owner: None,
            target_client: Default::default(),
            type_0: Default::default(),
            group: Default::default(),
            number: Default::default(),
            time: Default::default(),
            flags: Default::default(),
            state: Default::default(),
            source: Default::default(),
            target: Default::default(),
            cmdlist: Default::default(),
            cmd: Default::default(),
            cb: Default::default(),
            cancel_data: Default::default(),
            wait_timer: None,
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
/// use hmux::src::cmd::queue::cmdq_new;
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

pub const CMDQ_CALLBACK: cmdq_type = 1;
pub const CMDQ_COMMAND: cmdq_type = 0;

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

/// Register cleanup to run when an unfired callback item is removed.
pub(crate) fn cmdq_set_cancel_callback(item: &mut cmdq_item, cancel: Box<dyn FnOnce()>) {
    assert_eq!(item.type_0, CMDQ_CALLBACK);
    assert!(
        item.cancel_data.is_none(),
        "callback cancel hook already set"
    );
    item.cancel_data = Some(cancel);
}

pub(crate) fn cmdq_set_wait_timer(item: &mut cmdq_item, timer: crate::src::reactor::Timer) {
    assert!(
        item.wait_timer.is_none(),
        "command already owns delayed work"
    );
    item.wait_timer = Some(timer);
}

pub(crate) fn cmdq_clear_wait_timer(item: &mut cmdq_item) {
    drop(item.wait_timer.take());
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

unsafe fn cmdq_cancel_unfired_data(owner: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>) {
    let item = owner.get();
    if (*item).flags & CMDQ_FIRED != 0 {
        return;
    }
    if let Some(cancel) = (*item).cancel_data.take() {
        cancel();
    }
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

pub(crate) fn cmdq_set_file_task(item: &mut cmdq_item, task: hmux_rt::mio::Task) {
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

/// A dead client cannot resume a file read or delayed command. Cancel its
/// owned work first, then remove the waiting item and its queued suffix.
/// Other wait families need their own cancellation before they can be drained.
pub(crate) unsafe fn cmdq_abort_owned_wait(owner: &ClientRef) {
    let queue = QueueTarget::for_client(Some(owner));
    let Some(first) = queue.with_queue(|queue| queue.first()) else {
        return;
    };
    if (*first.get()).flags & (CMDQ_FIRED | CMDQ_WAITING) != (CMDQ_FIRED | CMDQ_WAITING) {
        return;
    }
    if let Some(file) = (*first.get()).wait_file.upgrade() {
        file_cancel_cmdq_wait(&file);
    } else if (*first.get()).wait_timer.is_some() {
        cmdq_clear_wait_timer(&mut *first.get());
    } else {
        return;
    }
    drop(first);
    queue.with_queue(|queue| queue.item = std::rc::Weak::new());
    while let Some(item) = queue.with_queue(|queue| queue.list.pop_front()) {
        cmdq_remove(item);
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

impl Drop for cmdq_list {
    fn drop(&mut self) {
        if !self.list.is_empty() {
            unsafe { fatalx(|out| out.write_all(b"queue not empty")) };
        }
    }
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
    if !current.is_null() && cmd_find_valid_state(&*current) != 0 {
        cmd_find_copy_state(&mut target, &*current);
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
        assert!(
            (*item).queue.is_none() && !(*item).removed,
            "detached command item"
        );
        remaining = (*item).next.take();
        (*item).client_owner = owner.cloned();
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
    assert!(
        !(*after.get()).removed && (*after.get()).queue.is_some(),
        "queued insertion anchor"
    );
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
        assert!(
            (*item).queue.is_none() && !(*item).removed,
            "detached command item"
        );
        remaining = (*item).next.take();
        (*item).client_owner = c_owner.clone();
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
    events_fire(&*name, ep);
}
pub unsafe fn cmdq_continue(item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>) {
    let item = item_handle.get();
    assert!(!(*item).removed, "cannot resume a removed command item");
    (*item).flags &= !CMDQ_WAITING;
}
// The caller transfers the queue owner after unlinking it and releasing guards.
unsafe fn cmdq_remove(item_handle: std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>) {
    assert_eq!(
        std::rc::Rc::strong_count(&item_handle),
        1,
        "command item removal requires its sole strong owner"
    );
    let item = item_handle.get();
    assert!(!(*item).removed, "command item already removed");
    (*item).removed = true;
    cmdq_clear_wait_timer(&mut *item);
    assert!(
        std::rc::Weak::ptr_eq(&(*item).wait_file, &std::rc::Weak::new()),
        "file wait must finish or cancel before queue item removal"
    );
    cmdq_cancel_unfired_data(&item_handle);
    if let Some(client) = (*item).client_owner.take() {
        (client).release();
    }
    drop((*item).cmdlist.take());
    drop((*item).state.take());
    if let Some(queue) = (*item).queue.take() {
        queue.with_queue(|queue| {
            if queue.item.ptr_eq(&std::rc::Rc::downgrade(&item_handle)) {
                queue.item = std::rc::Weak::new();
            }
        });
    }
    // Release resources explicitly before destroying the item.
    drop((*item).cb.take());
    drop((*item).cancel_data.take());
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
        (*item).type_0 = CMDQ_COMMAND;
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
    if cmd_find_target(
        fs,
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
                assert!(
                    !(*item).removed,
                    "executing command item removed during dispatch"
                );
                if !(retval as ::core::ffi::c_int == CMD_RETURN_ERROR as ::core::ffi::c_int)
                    && entry.flags & CMD_AFTERHOOK != 0
                {
                    if cmd_find_valid_state(&(*item).target) != 0 {
                        fsp = &raw mut (*item).target;
                        current_block = 8704759739624374314;
                    } else if cmd_find_valid_state(&state.current.borrow()) != 0 {
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
    if retval as ::core::ffi::c_int == CMD_RETURN_ERROR as ::core::ffi::c_int {
        fsp = ::core::ptr::null_mut::<cmd_find_state>();
        if cmd_find_valid_state(&(*item).target) != 0 {
            fsp = &raw mut (*item).target;
        } else if cmd_find_valid_state(&state.current.borrow()) != 0 {
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
    (*item).type_0 = CMDQ_CALLBACK;
    (*item).group = 0 as u_int;
    (*item).state = Some(cmdq_new_state(
        ::core::ptr::null_mut::<cmd_find_state>(),
        ::core::ptr::null_mut::<key_event>(),
        0 as ::core::ffi::c_int,
    ));
    (*item).cb = cb;
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
unsafe fn cmdq_fire_callback(
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
) -> cmd_retval {
    let item = item_handle.get();
    (*item).flags |= CMDQ_FIRED;
    (*item).cb.take().expect("non-null queue callback")(item_handle)
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
        assert!(!(*item).removed, "queued command item");
        log_debug(format_args!(
            "cmdq_next {}: {} ({}), flags {:x}",
            crate::src::log::log_bytes(name.as_bytes()),
            crate::src::log::log_bytes((*item).name.as_deref().unwrap_or(c"(null)").to_bytes()),
            (*item).type_0,
            (*item).flags
        ));
        if (*item).flags & CMDQ_WAITING != 0 {
            return items;
        }
        if (*item).flags & CMDQ_FIRED == 0 {
            (*item).time = time(std::ptr::null_mut());
            number = number.wrapping_add(1);
            (*item).number = number;
            // Arm before dispatch: a synchronously completed operation can
            // call cmdq_continue before its command returns CMD_RETURN_WAIT.
            (*item).flags |= CMDQ_WAITING;
            let retval = match (*item).type_0 {
                CMDQ_COMMAND => cmdq_fire_command(&item_owner),
                CMDQ_CALLBACK => cmdq_fire_callback(&item_owner),
                _ => CMD_RETURN_ERROR,
            };
            // Dispatch may insert items, but cannot remove its own active item.
            assert!(
                !(*item).removed,
                "executing command item removed during dispatch"
            );
            if retval == CMD_RETURN_ERROR && (*item).type_0 == CMDQ_COMMAND {
                cmdq_remove_group(&item_owner);
            }
            (*item).flags |= CMDQ_FIRED;
            if retval == CMD_RETURN_WAIT && (*item).flags & CMDQ_WAITING != 0 {
                return items;
            }
            (*item).flags &= !CMDQ_WAITING;
            items += 1;
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

pub unsafe fn cmdq_running() -> std::rc::Weak<std::cell::UnsafeCell<cmdq_item>> {
    let queue = QueueTarget::Global;
    let Some(owner) = queue.with_queue(|queue| queue.item.upgrade()) else {
        return std::rc::Weak::new();
    };
    if (*owner.get()).removed
        || (*owner.get()).flags & (CMDQ_FIRED | CMDQ_WAITING) == (CMDQ_FIRED | CMDQ_WAITING)
    {
        return std::rc::Weak::new();
    }
    std::rc::Rc::downgrade(&owner)
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
    if c.is_none() {
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
    } else if c
        .as_ref()
        .expect("live client")
        .attached_session()
        .upgrade()
        .is_none()
        || c.as_ref().expect("live client").flags() & CLIENT_CONTROL as uint64_t != 0
    {
        server_add_message(|out| {
            write_cstr(out, (c.as_ref().expect("live client").name()).as_deref())?;
            out.write_all(b" message: ")?;
            write_cstr(out, &*msg)
        });
        if !c.as_ref().expect("live client").flags() & CLIENT_UTF8 as uint64_t != 0 {
            msg = utf8_sanitize_cstring(msg.as_c_str());
        }
        if c.as_ref().expect("live client").flags() & CLIENT_CONTROL as uint64_t != 0 {
            control_write(&c.clone().expect("live client"), |out| {
                write_cstr(out, &*msg)
            });
        } else {
            file_error(c_owner.as_ref(), |out| {
                write_cstr(out, &*msg)?;
                out.write_all(b"\n")
            });
        }
        c.as_ref().expect("live client").set_return_value(1);
    } else {
        let mut bytes = msg.into_bytes_with_nul();
        bytes[0] = *(*__ctype_toupper_loc()).offset(bytes[0] as isize) as u8;
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

#[cfg(test)]
mod lifecycle_tests {
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;

    #[test]
    fn removal_rejects_an_extra_strong_owner_before_cleanup() {
        unsafe {
            let item = cmdq_get_callback_owned(c"extra owner", None);
            let observer = Rc::downgrade(&item);
            let guard = item.clone();
            let cancelled = Rc::new(Cell::new(false));
            let called = cancelled.clone();
            cmdq_set_cancel_callback(&mut *item.get(), Box::new(move || called.set(true)));
            let owner = item;
            let result =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| cmdq_remove(owner)));
            assert!(result.is_err());
            assert!(!cancelled.get());
            assert!(!(*guard.get()).removed);
            cmdq_remove(guard);
            assert!(cancelled.get());
            assert!(observer.upgrade().is_none());
        }
    }

    #[test]
    fn dropping_without_removal_panics_and_does_not_run_cancellation() {
        unsafe {
            let item = cmdq_get_callback_owned(c"unreleased detached item", None);
            let observer = Rc::downgrade(&item);
            let cancelled = Rc::new(Cell::new(false));
            let called = cancelled.clone();
            cmdq_set_cancel_callback(&mut *item.get(), Box::new(move || called.set(true)));
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(item)));
            let panic = result.expect_err("dropping without removal must panic");
            assert_eq!(
                panic
                    .downcast_ref::<String>()
                    .map(String::as_str)
                    .or_else(|| panic.downcast_ref::<&str>().copied()),
                Some("command item dropped without cmdq_remove")
            );
            assert!(observer.upgrade().is_none());
            assert!(!cancelled.get());
        }
    }
}
