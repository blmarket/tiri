use crate::src::cmd::parse::cmd_parse_and_append;
use crate::src::cmd::queue::{
    cmdq_append, cmdq_get_callback_owned, cmdq_get_client, cmdq_guard, cmdq_new_state,
};
use crate::src::format::bytes::format_message_with;
use crate::src::format::bytes::write_cstr;
use crate::src::log::{fatalx, log_bytes, log_cstr, log_cstr_n, log_debug};
use crate::src::monitor::{monitor_add, monitor_create_client_owned, monitor_remove};
use crate::src::reactor::{
    bufferevent_disable, bufferevent_enable, bufferevent_get_input, bufferevent_write,
    bufferevent_write_buffer, evbuffer_add, evbuffer_add_formatted, evbuffer_get_length,
    evbuffer_new, evbuffer_pullup, evbuffer_readln,
};
use crate::src::server_client::Client as _;
use crate::src::session::Session;
use crate::src::shared::abi::*;
use crate::src::shared::client::ClientRef;
use crate::src::shared::client::{
    CLIENT_CONTROL_NOOUTPUT, CLIENT_CONTROL_PAUSEAFTER, CLIENT_EXIT, CLIENT_UNATTACHEDFLAGS,
};
use crate::src::shared::command::CMDQ_STATE_CONTROL;
use crate::src::shared::command::*;
use crate::src::shared::command::{cmd_find_state, cmdq_item};
use crate::src::shared::control::{
    control_block, control_pane, control_panes, control_state, control_window, control_windows,
};
use crate::src::shared::event::*;
use crate::src::shared::event::{EV_READ, EV_WRITE};
use crate::src::shared::key::key_event;
use crate::src::shared::monitor::{monitor_callback, monitor_change};
use crate::src::shared::monitor::{monitor_type, MONITOR_NOTIFY_INITIAL};
use crate::src::shared::pane::window_pane;
use crate::src::shared::pane::window_pane_offset;
use crate::src::shared::session::SessionRef;
use crate::src::tmux::get_timer;
use crate::src::window::{winlink_find_by_window, Window, WindowPane};
use std::cell::UnsafeCell;
use std::collections::VecDeque;
use std::ffi::{CStr, CString};
use std::rc::Rc;
use tiri_buffer::SegmentedBuf;

pub const CONTROL_PANE_OFF: ::core::ffi::c_int = 0x1 as ::core::ffi::c_int;
pub const CONTROL_PANE_PAUSED: ::core::ffi::c_int = 0x2 as ::core::ffi::c_int;
pub const CONTROL_BUFFER_LOW: ::core::ffi::c_int = 512 as ::core::ffi::c_int;
pub const CONTROL_BUFFER_HIGH: ::core::ffi::c_int = 8192 as ::core::ffi::c_int;
pub const CONTROL_WRITE_MINIMUM: ::core::ffi::c_int = 32 as ::core::ffi::c_int;
pub const CONTROL_MAXIMUM_AGE: ::core::ffi::c_int = 300000 as ::core::ffi::c_int;
pub const CONTROL_MAXIMUM_REPLY_BUFFER: ::core::ffi::c_int =
    64 as ::core::ffi::c_int * 1024 as ::core::ffi::c_int * 1024 as ::core::ffi::c_int;
pub const CONTROL_IGNORE_FLAGS: ::core::ffi::c_int =
    CLIENT_CONTROL_NOOUTPUT | CLIENT_UNATTACHEDFLAGS;

impl control_block {
    fn new(line: Option<CString>, size: size_t) -> refbox::RefBox<Self> {
        refbox::RefBox::new(control_block { size, line, t: 0 })
    }
}

impl control_state {
    fn add_block(&mut self, owner: refbox::RefBox<control_block>) -> refbox::Weak<control_block> {
        let block = owner.downgrade();
        self.all_blocks.push_back(owner);
        block
    }

    fn block(&self, index: usize) -> refbox::Weak<control_block> {
        self.all_blocks
            .get(index)
            .map_or_else(refbox::Weak::new, refbox::RefBox::downgrade)
    }

    fn pending_snapshot(&self) -> Vec<u_int> {
        self.pending_panes.iter().copied().collect()
    }

    fn remove_pending(&mut self, pane: u_int) -> bool {
        let Some(index) = self.pending_panes.iter().position(|queued| *queued == pane) else {
            return false;
        };
        self.pending_panes.remove(index).is_some()
    }
}

fn control_first_block(cs: &control_state) -> refbox::Weak<control_block> {
    cs.block(0)
}

fn control_first_pane_block(cp: &control_pane) -> refbox::Weak<control_block> {
    cp.blocks.front().cloned().unwrap_or_default()
}

fn control_remove_pane_block(cp: &mut control_pane, block: &refbox::Weak<control_block>) {
    let index = cp
        .blocks
        .iter()
        .position(|candidate| candidate == block)
        .expect("control pane block must be queued on its pane");
    cp.blocks.remove(index).expect("located pane block");
}

fn control_add_block(
    cs: &mut control_state,
    owner: refbox::RefBox<control_block>,
) -> refbox::Weak<control_block> {
    cs.add_block(owner)
}

unsafe fn control_free_block(cs: &mut control_state, cb: &refbox::Weak<control_block>) {
    control_release_block(&mut cs.all_blocks, &mut cs.queued_reply_bytes, cb);
}

unsafe fn control_release_block(
    blocks: &mut VecDeque<refbox::RefBox<control_block>>,
    queued_reply_bytes: &mut size_t,
    cb: &refbox::Weak<control_block>,
) {
    let size = {
        let block = cb.try_borrow_mut().expect("live control block");
        (block.size == 0)
            .then(|| {
                block
                    .line
                    .as_ref()
                    .map(|line| line.as_bytes_with_nul().len())
            })
            .flatten()
    };
    if let Some(size) = size {
        *queued_reply_bytes = queued_reply_bytes.saturating_sub(size);
    }
    let index = blocks
        .iter()
        .position(|owner| cb.is(owner))
        .expect("control block must be owned by its state");
    drop(blocks.remove(index).expect("located control block"));
}

fn control_add_pane(
    panes: &mut control_panes,
    id: u32,
    offset: window_pane_offset,
) -> &mut control_pane {
    panes
        .entry(id)
        .or_insert_with(|| {
            Box::new(control_pane {
                pane: id,
                offset,
                queued: offset,
                flags: 0,
                pending_flag: 0,
                blocks: VecDeque::new(),
            })
        })
        .as_mut()
}
pub unsafe fn control_set_window_size(c: &ClientRef, window: u_int, sx: u_int, sy: u_int) {
    if let Some(mut cs) = c.borrow_control_mut() {
        control_windows_set(&mut cs.windows, window, sx, sy);
    }
}

pub unsafe fn control_get_window_size(
    c: &ClientRef,
    window: u_int,
    sx: *mut u_int,
    sy: *mut u_int,
) -> ::core::ffi::c_int {
    let Some(cs) = c.borrow_control_mut() else {
        return 0;
    };
    let Some(cw) = cs.windows.get(&window) else {
        return 0;
    };
    *sx = cw.sx;
    *sy = cw.sy;
    1
}

pub unsafe fn control_clear_window_size(c: &ClientRef, window: u_int) {
    if let Some(mut cs) = c.borrow_control_mut() {
        cs.windows.remove(&window);
    }
}

unsafe fn control_discard_pane(cs: &mut control_state, pane: u_int) {
    let Some(cp) = cs.panes.get_mut(&pane) else {
        return;
    };
    while let Some(block) = cp.blocks.pop_front() {
        control_release_block(&mut cs.all_blocks, &mut cs.queued_reply_bytes, &block);
    }
}

unsafe fn control_session_has_pane(
    session: &SessionRef,
    pane: &Rc<UnsafeCell<window_pane>>,
) -> bool {
    let parent = pane.window_observer().upgrade().expect("live pane parent");
    let present = session.with_winlinks(|links| winlink_find_by_window(links, &parent).is_alive());
    drop(parent);
    present
}
unsafe fn control_window_pane(c: &ClientRef, pane: u_int) -> Option<Rc<UnsafeCell<window_pane>>> {
    let session = c.attached_session().upgrade()?;
    let pane = std::rc::Rc::<std::cell::UnsafeCell<window_pane>>::find_by_id(pane)?;
    control_session_has_pane(&session, &pane).then_some(pane)
}
pub unsafe fn control_reset_offsets(c: &ClientRef) {
    let mut state = c.borrow_control_mut().expect("control client state");
    let cs = &mut *state;
    for (_, mut pane) in std::mem::take(&mut cs.panes) {
        while let Some(block) = pane.blocks.pop_front() {
            control_free_block(cs, &block);
        }
    }
    cs.pending_panes.clear();
}
pub unsafe fn control_pane_offset<'a>(
    cs: &'a mut control_state,
    client_flags: uint64_t,
    pane: u_int,
    off: &mut ::core::ffi::c_int,
) -> Option<&'a mut window_pane_offset> {
    *off = 0;
    if client_flags & CLIENT_CONTROL_NOOUTPUT as uint64_t != 0 {
        return None;
    }
    let cp = cs.panes.get_mut(&pane)?;
    if cp.flags & CONTROL_PANE_PAUSED != 0 {
        return None;
    }
    if cp.flags & CONTROL_PANE_OFF != 0 {
        *off = 1;
        return None;
    }
    *off = (cs
        .write_event
        .with_ptr(|stream| unsafe { evbuffer_get_length(&(*stream).output) })
        .unwrap_or(0)
        >= CONTROL_BUFFER_LOW as size_t) as ::core::ffi::c_int;
    Some(&mut cp.offset)
}
pub unsafe fn control_set_pane_on(c: &ClientRef, pane: &Rc<UnsafeCell<window_pane>>) {
    let (id, offset) = (pane.id(), pane.output_offset());
    let mut state = c.borrow_control_mut().expect("control client state");
    let Some(cp) = state.panes.get_mut(&id) else {
        return;
    };
    if cp.flags & CONTROL_PANE_OFF != 0 {
        cp.flags &= !CONTROL_PANE_OFF;
        cp.offset.used = offset.used;
        cp.queued.used = offset.used;
    }
}

pub unsafe fn control_set_pane_off(c: &ClientRef, pane: &Rc<UnsafeCell<window_pane>>) {
    let (id, offset) = (pane.id(), pane.output_offset());
    let mut state = c.borrow_control_mut().expect("control client state");
    let cs = &mut *state;
    control_add_pane(&mut cs.panes, id, offset);
    control_discard_pane(cs, id);
    let cp = cs.panes.get_mut(&id).expect("indexed control pane");
    cp.offset.used = offset.used;
    cp.queued.used = offset.used;
    cp.flags |= CONTROL_PANE_OFF;
}

pub unsafe fn control_continue_pane(c: &ClientRef, pane: &Rc<UnsafeCell<window_pane>>) {
    let (id, offset) = (pane.id(), pane.output_offset());
    {
        let mut state = c.borrow_control_mut().expect("control client state");
        let Some(cp) = state.panes.get_mut(&id) else {
            return;
        };
        if cp.flags & CONTROL_PANE_PAUSED == 0 {
            return;
        }
        cp.flags &= !CONTROL_PANE_PAUSED;
        cp.offset.used = offset.used;
        cp.queued.used = offset.used;
    }
    control_notify_write(c, |out| write!(out, "%continue %{}", pane.id()));
}

pub unsafe fn control_pause_pane(c: &ClientRef, pane: &Rc<UnsafeCell<window_pane>>) {
    let (id, offset) = (pane.id(), pane.output_offset());
    {
        let mut state = c.borrow_control_mut().expect("control client state");
        let cs = &mut *state;
        let cp = control_add_pane(&mut cs.panes, id, offset);
        if cp.flags & CONTROL_PANE_PAUSED != 0 {
            return;
        }
        cp.flags |= CONTROL_PANE_PAUSED;
        control_discard_pane(cs, id);
    }
    control_notify_write(c, |out| write!(out, "%pause %{}", pane.id()));
}

pub unsafe fn control_reset_pane(c: &ClientRef, pane: &Rc<UnsafeCell<window_pane>>) {
    let (id, offset) = (pane.id(), pane.output_offset());
    let Some(mut state) = c.borrow_control_mut() else {
        return;
    };
    let cs = &mut *state;
    control_discard_pane(cs, id);
    if let Some(cp) = cs.panes.get_mut(&id) {
        cp.offset.used = offset.used;
        cp.queued.used = offset.used;
    }
}

unsafe fn control_write_line(c_owner: &ClientRef, line: CString) {
    let size = line.as_bytes_with_nul().len();
    if !c_owner.accept_control_reply(size) {
        return;
    }
    let name_owner = c_owner.name();
    let name = name_owner.as_deref().unwrap_or(c"(null)");
    let (stream, immediate) = {
        let Some(mut cs) = c_owner.borrow_control_mut() else {
            return;
        };
        let stream = cs.write_event.clone();
        if !control_first_block(cs).is_alive() {
            log_debug(format_args!(
                "control_write_line: {}: writing line: {}",
                log_cstr(name),
                log_cstr(&line)
            ));
            (stream, Some(line))
        } else {
            let cb = control_add_block(cs, control_block::new(Some(line), 0));
            cs.queued_reply_bytes = cs.queued_reply_bytes.wrapping_add(size);
            cb.try_borrow_mut().expect("live control block").t = get_timer();
            log_debug(format_args!(
                "control_write_line: {}: storing line: {}",
                log_cstr(name),
                log_cstr(
                    cb.try_borrow_mut()
                        .expect("live control block")
                        .line
                        .as_deref()
                        .unwrap_or(c"(null)")
                )
            ));
            (stream, None)
        }
    };
    let _ = stream.with_ptr(|stream| {
        if let Some(line) = immediate {
            bufferevent_write(stream, &line.to_bytes()[..size - 1]);
            bufferevent_write(stream, b"\n");
        }
        bufferevent_enable(stream, EV_WRITE as i16)
    });
}
unsafe fn control_flush_deferred(c_owner: &ClientRef) {
    loop {
        let line = c_owner
            .borrow_control_mut()
            .and_then(|mut cs| cs.deferred.pop_front());
        let Some(line) = line else { break };
        control_write_line(c_owner, line);
    }
}
pub unsafe fn control_write(
    c_owner: &ClientRef,
    write: impl FnOnce(&mut dyn std::io::Write) -> std::io::Result<()>,
) {
    if c_owner.borrow_control_mut().is_none() {
        return;
    }
    let line = format_message_with(write);
    control_write_line(c_owner, line);
}
pub unsafe fn control_write_guard(
    c_owner: &ClientRef,
    guard: &CStr,
    t: ::core::ffi::c_long,
    number: u_int,
    flags: ::core::ffi::c_int,
) {
    let begin = guard == c"begin";
    {
        let Some(mut cs) = c_owner.borrow_control_mut() else {
            return;
        };
        if begin {
            cs.guard_depth += 1;
        }
    }
    control_write(c_owner, |out| {
        out.write_all(b"%")?;
        write_cstr(out, &*guard)?;
        write!(out, " {} {} {}", t, number, flags)
    });
    let flush = {
        let Some(mut cs) = c_owner.borrow_control_mut() else {
            return;
        };
        if !begin && cs.guard_depth > 0 {
            cs.guard_depth -= 1;
            cs.guard_depth == 0
        } else {
            false
        }
    };
    if flush {
        control_flush_deferred(c_owner);
    }
}
pub unsafe fn control_notify_write(
    c_owner: &ClientRef,
    write: impl FnOnce(&mut dyn std::io::Write) -> std::io::Result<()>,
) {
    if c_owner.borrow_control_mut().is_none() {
        return;
    }
    let line = format_message_with(write);
    let name = c_owner.name();
    {
        let Some(mut cs) = c_owner.borrow_control_mut() else {
            return;
        };
        if cs.guard_depth != 0 {
            log_debug(format_args!(
                "control_notify_write: {}: deferring notification: {}",
                log_cstr(name.as_deref().unwrap_or(c"(null)")),
                log_cstr(&line)
            ));
            cs.deferred.push_back(line);
            return;
        }
    }
    control_write_line(c_owner, line);
}
unsafe fn control_check_age(
    client: &ClientRef,
    window_pane: &Rc<UnsafeCell<window_pane>>,
    pane: u_int,
) -> i32 {
    let block = {
        let state = client.borrow_control_mut().expect("control client state");
        control_first_pane_block(state.panes.get(&pane).expect("indexed control pane"))
    };
    if !block.is_alive() {
        return 0;
    }
    let now = get_timer();
    let timestamp = block.try_borrow_mut().expect("live control block").t;
    if timestamp >= now {
        return 0;
    }
    let age = now.wrapping_sub(timestamp);
    let name = client.name();
    log_debug(format_args!(
        "control_check_age: {}: %{} is {} behind",
        log_cstr(name.as_deref().unwrap_or(c"(null)")),
        window_pane.id(),
        age
    ));
    if let Some(limit) = client.control_pause_after() {
        if age < limit {
            return 0;
        }
        {
            let mut state = client.borrow_control_mut().expect("control client state");
            state
                .panes
                .get_mut(&pane)
                .expect("indexed control pane")
                .flags |= CONTROL_PANE_PAUSED;
            control_discard_pane(state, pane);
        }
        control_notify_write(client, |out| write!(out, "%pause %{}", window_pane.id()));
    } else {
        if age < CONTROL_MAXIMUM_AGE as u64 {
            return 0;
        }
        client.exit_with_message(c"too far behind".to_owned(), None);
        control_discard(client);
    }
    1
}

pub unsafe fn control_write_output(client: &ClientRef, wp: &Rc<UnsafeCell<window_pane>>) {
    let pane = wp.id();
    let session = client
        .attached_session()
        .upgrade()
        .expect("attached control client");
    if !control_session_has_pane(&session, wp) {
        return;
    }
    drop(session);
    let ignored = if client.flags() & (CONTROL_IGNORE_FLAGS | CLIENT_EXIT) as u64 != 0 {
        let state = client.borrow_control_mut().expect("control client state");
        if !state.panes.contains_key(&pane) {
            return;
        }
        true
    } else {
        let offset = wp.output_offset();
        let mut state = client.borrow_control_mut().expect("control client state");
        let cp = control_add_pane(&mut state.panes, pane, offset);
        cp.flags & (CONTROL_PANE_OFF | CONTROL_PANE_PAUSED) != 0
    };
    if !ignored {
        if control_check_age(client, wp, pane) != 0 {
            return;
        }
        let mut queued = {
            let state = client.borrow_control_mut().expect("control client state");
            state.panes.get(&pane).expect("indexed control pane").queued
        };
        let previous = queued.used;
        wp.advance_output(&mut queued, usize::MAX);
        let size = queued.used.wrapping_sub(previous);
        let name = client.name();
        let stream = {
            let mut state = client.borrow_control_mut().expect("control client state");
            let cs = &mut *state;
            cs.panes
                .get_mut(&pane)
                .expect("indexed control pane")
                .queued = queued;
            if size == 0 {
                return;
            }
            let block = control_add_block(cs, control_block::new(None, size));
            block.try_borrow_mut().expect("live control block").t = get_timer();
            let cp = cs.panes.get_mut(&pane).expect("indexed control pane");
            cp.blocks.push_back(block);
            log_debug(format_args!(
                "control_write_output: {}: new output block of {} for %{}",
                log_cstr(name.as_deref().unwrap_or(c"(null)")),
                size,
                pane
            ));
            if cp.pending_flag == 0 {
                log_debug(format_args!(
                    "control_write_output: {}: %{} now pending",
                    log_cstr(name.as_deref().unwrap_or(c"(null)")),
                    pane
                ));
                cs.pending_panes.push_back(pane);
                cp.pending_flag = 1;
            }
            cs.write_event.clone()
        };
        let _ = stream.with_ptr(|stream| bufferevent_enable(stream, EV_WRITE as i16));
        return;
    }
    let name = client.name();
    log_debug(format_args!(
        "control_write_output: {}: ignoring pane %{}",
        log_cstr(name.as_deref().unwrap_or(c"(null)")),
        pane
    ));
    let (mut offset, mut queued) = {
        let state = client.borrow_control_mut().expect("control client state");
        let cp = state.panes.get(&pane).expect("indexed control pane");
        (cp.offset, cp.queued)
    };
    wp.advance_output(&mut offset, usize::MAX);
    wp.advance_output(&mut queued, usize::MAX);
    let mut state = client.borrow_control_mut().expect("control client state");
    let cp = state.panes.get_mut(&pane).expect("indexed control pane");
    cp.offset = offset;
    cp.queued = queued;
}
unsafe fn control_error(
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
    error: Option<CString>,
) -> cmd_retval {
    let item = item_handle.get();
    let c_owner = cmdq_get_client((item).as_ref());
    let mut c: Option<ClientRef> = c_owner.clone();
    cmdq_guard(item_handle, c"begin", 1 as ::core::ffi::c_int);
    control_write(&c.clone().expect("live client"), |out| {
        out.write_all(b"parse error: ")?;
        write_cstr(out, error.as_deref())
    });
    cmdq_guard(item_handle, c"error", 1 as ::core::ffi::c_int);
    CMD_RETURN_NORMAL
}
unsafe fn control_error_callback(owner: &ClientRef) {
    let mut c: Option<ClientRef> = Some(owner.clone());
    c.as_ref()
        .expect("live client")
        .update_flags(CLIENT_EXIT as uint64_t, 0);
}
unsafe fn control_read_callback(owner: &ClientRef) {
    let mut c: Option<ClientRef> = Some(owner.clone());
    loop {
        let stream = {
            let Some(state) = owner.borrow_control_mut() else {
                break;
            };
            state.read_event.clone()
        };
        let Some(line) = stream
            .with_ptr(|stream| unsafe { evbuffer_readln(bufferevent_get_input(&mut *stream)) })
            .flatten()
        else {
            break;
        };
        log_debug(format_args!(
            "{}: {}: {}",
            "control_read_callback",
            log_cstr(
                (c.as_ref().expect("live client").name())
                    .as_deref()
                    .unwrap_or(c"(null)")
            ),
            log_bytes(&line)
        ));
        if line[0] == 0 {
            c.as_ref()
                .expect("live client")
                .update_flags(CLIENT_EXIT as uint64_t, 0);
            break;
        } else {
            let state = cmdq_new_state(
                ::core::ptr::null_mut::<cmd_find_state>(),
                ::core::ptr::null_mut::<key_event>(),
                CMDQ_STATE_CONTROL,
            );
            if let Err(error) = cmd_parse_and_append(
                CStr::from_ptr(line.as_ptr().cast::<::core::ffi::c_char>()),
                Some(owner),
                Some(&state),
            ) {
                let error_item_allocation = cmdq_get_callback_owned(
                    c"control_error",
                    Some(Box::new(move |item| unsafe { control_error(item, error) })),
                );
                cmdq_append(Some(owner), error_item_allocation);
            }
        }
    }
}
pub unsafe fn control_all_done(c: &ClientRef) -> ::core::ffi::c_int {
    let state = c.borrow_control_mut().expect("control client state");
    let cs = &*state;
    if control_first_block(cs).is_alive() {
        return 0 as ::core::ffi::c_int;
    }
    (cs.write_event
        .with_ptr(|stream| unsafe { evbuffer_get_length(&(*stream).output) })
        .unwrap_or(0)
        == 0 as size_t) as ::core::ffi::c_int
}
pub unsafe fn control_wait_exit() {
    use std::os::fd::AsFd;
    use tiri_rt::{AsyncRead as _, Runtime as _};
    let stdin = std::io::stdin();
    let fd = stdin.as_fd();
    let Ok(was_nonblocking) = tiri_rt::unix::set_nonblocking(fd, true) else {
        return;
    };
    let result = (|| -> std::io::Result<()> {
        let mut runtime = tiri_rt::mio::Runtime::new()?;
        let source = runtime.enter(|| tiri_rt::mio::Io::new(fd.try_clone_to_owned()?))?;
        runtime.block_on(async {
            let mut buffer = evbuffer_new();
            loop {
                if let Some(line) = evbuffer_readln(&mut buffer) {
                    if line[0] == 0 {
                        return Ok(());
                    }
                } else {
                    let mut bytes = vec![0; 65536];
                    let received = source.read(&mut bytes).await?;
                    if received.bytes == 0 {
                        return Ok(());
                    }
                    bytes.truncate(received.bytes);
                    use tiri_buffer::BufMut as _;
                    buffer.put(tiri_buffer::SegmentedBuf::from(bytes));
                }
            }
        })?
    })();
    let _ = result;
    let _ = tiri_rt::unix::set_nonblocking(fd, was_nonblocking);
}
unsafe fn control_flush_all_blocks(client: &ClientRef) {
    loop {
        let (block, line, stream) = {
            let state = client.borrow_control_mut().expect("control client state");
            let block = control_first_block(state);
            if !block.is_alive() {
                break;
            }
            let data = block.try_borrow_mut().expect("live control block");
            if data.size != 0 {
                break;
            }
            let line = data.line.clone().expect("reply block has a line");
            drop(data);
            (block, line, state.write_event.clone())
        };
        let name = client.name();
        log_debug(format_args!(
            "control_flush_all_blocks: {}: flushing line: {}",
            log_cstr(name.as_deref().unwrap_or(c"(null)")),
            log_cstr(&line)
        ));
        let Some(()) = stream.with_ptr(|stream| {
            bufferevent_write(stream, line.as_bytes());
            bufferevent_write(stream, b"\n");
        }) else {
            break;
        };
        let mut state = client.borrow_control_mut().expect("control client state");
        control_free_block(state, &block);
    }
}
unsafe fn control_append_data(
    c_owner: &ClientRef,
    offset: &mut window_pane_offset,
    mut age: uint64_t,
    message: Option<Box<SegmentedBuf>>,
    wp_owner: &Rc<UnsafeCell<window_pane>>,
    mut size: size_t,
) -> Box<SegmentedBuf> {
    let mut c: Option<ClientRef> = Some(c_owner.clone());

    let mut new_data: *mut u_char = ::core::ptr::null_mut::<u_char>();
    let mut new_size: size_t = 0;
    let mut start: size_t = 0;
    let mut i: u_int = 0;
    let mut message = message.unwrap_or_else(|| {
        let mut message = evbuffer_new();
        if c.as_ref().expect("live client").flags() as ::core::ffi::c_ulonglong
            & CLIENT_CONTROL_PAUSEAFTER
            != 0
        {
            evbuffer_add_formatted(&mut message, |out| {
                write!(out, "%extended-output %{} {} : ", { wp_owner.id() }, {
                    age as ::core::ffi::c_ulonglong
                })
            });
        } else {
            evbuffer_add_formatted(&mut message, |out| {
                write!(out, "%output %{} ", { wp_owner.id() })
            });
        }
        message
    });
    let mut data = vec![0; size];
    new_size = wp_owner.copy_output(offset, &mut data);
    new_data = data.as_ptr().cast_mut();
    if new_size < size {
        fatalx(|out| {
            write!(out, "not enough data: {} < {}", (new_size) as usize, {
                size
            })
        });
    }
    i = 0 as u_int;
    while (i as size_t) < size {
        if (*new_data.offset(i as isize) as ::core::ffi::c_int) < ' ' as i32
            || *new_data.offset(i as isize) as ::core::ffi::c_int == '\\' as i32
        {
            evbuffer_add_formatted(&mut message, |out| {
                write!(
                    out,
                    "\\{:03o}",
                    (*new_data.offset(i as isize) as ::core::ffi::c_int) as u32
                )
            });
        } else {
            start = i as size_t;
            while (i.wrapping_add(1 as u_int) as size_t) < size
                && *new_data.offset(i.wrapping_add(1 as u_int) as isize) as ::core::ffi::c_int
                    >= ' ' as i32
                && *new_data.offset(i.wrapping_add(1 as u_int) as isize) as ::core::ffi::c_int
                    != '\\' as i32
            {
                i = i.wrapping_add(1);
            }
            evbuffer_add(
                &mut message,
                std::slice::from_raw_parts(
                    new_data.add(start).cast(),
                    (i as size_t).wrapping_sub(start).wrapping_add(1 as size_t),
                ),
            );
        }
        i = i.wrapping_add(1);
    }
    wp_owner.advance_output(offset, size);
    message
}
unsafe fn control_write_data(c_owner: &ClientRef, mut message: Box<SegmentedBuf>) {
    let mut c: Option<ClientRef> = Some(c_owner.clone());

    log_debug(format_args!(
        "{}: {}: {}",
        "control_write_data",
        log_cstr(
            (c.as_ref().expect("live client").name())
                .as_deref()
                .unwrap_or(c"(null)")
        ),
        log_cstr_n(evbuffer_pullup(&mut message, -1).map_or(&[][..], |bytes| bytes))
    ));
    evbuffer_add(&mut message, b"\n");
    let stream = {
        c_owner
            .borrow_control_mut()
            .expect("control client state")
            .write_event
            .clone()
    };
    let _ = stream.with_ptr(|stream| unsafe { bufferevent_write_buffer(stream, &mut message) });
}
unsafe fn control_write_pending(client: &ClientRef, pane: u_int, limit: size_t) -> i32 {
    let now = get_timer();
    let owner = control_window_pane(client, pane);
    if owner.as_ref().is_none_or(|pane| !pane.has_tty()) {
        {
            let mut state = client.borrow_control_mut().expect("control client state");
            control_discard_pane(state, pane);
        }
        control_flush_all_blocks(client);
        return 0;
    }
    let owner = owner.expect("live control pane");
    let mut message = None;
    let mut used = 0;
    while used != limit {
        let empty = {
            let state = client.borrow_control_mut().expect("control client state");
            state
                .panes
                .get(&pane)
                .expect("indexed control pane")
                .blocks
                .is_empty()
        };
        if empty {
            break;
        }
        if control_check_age(client, &owner, pane) != 0 {
            message = None;
            break;
        }
        let (block, mut offset) = {
            let state = client.borrow_control_mut().expect("control client state");
            let cp = state.panes.get(&pane).expect("indexed control pane");
            (control_first_pane_block(cp), cp.offset)
        };
        let (size, timestamp) = {
            let data = block.try_borrow_mut().expect("live control block");
            (data.size, data.t)
        };
        let age = if timestamp < now {
            now.wrapping_sub(timestamp)
        } else {
            0
        };
        let name = client.name();
        log_debug(format_args!(
            "control_write_pending: {}: output block {} (age {}) for %{} (used {}/{})",
            log_cstr(name.as_deref().unwrap_or(c"(null)")),
            size,
            age,
            pane,
            used,
            limit
        ));
        let size = size.min(limit.wrapping_sub(used));
        used = used.wrapping_add(size);
        message = Some(control_append_data(
            client,
            &mut offset,
            age,
            message,
            &owner,
            size,
        ));
        let flush = {
            let mut state = client.borrow_control_mut().expect("control client state");
            let cs = &mut *state;
            cs.panes
                .get_mut(&pane)
                .expect("indexed control pane")
                .offset = offset;
            let left = {
                let mut data = block.try_borrow_mut().expect("live control block");
                data.size = data.size.wrapping_sub(size);
                data.size
            };
            if left == 0 {
                control_remove_pane_block(
                    cs.panes.get_mut(&pane).expect("indexed control pane"),
                    &block,
                );
                control_free_block(cs, &block);
                let first = control_first_block(cs);
                first.is_alive() && first.try_borrow_mut().expect("live control block").size == 0
            } else {
                false
            }
        };
        if flush {
            if let Some(message) = message.take() {
                control_write_data(client, message);
            }
            control_flush_all_blocks(client);
        }
    }
    if let Some(message) = message {
        control_write_data(client, message);
    }
    let state = client.borrow_control_mut().expect("control client state");
    (!state
        .panes
        .get(&pane)
        .expect("indexed control pane")
        .blocks
        .is_empty()) as i32
}

unsafe fn control_write_callback(owner: &ClientRef) {
    if owner.borrow_control_mut().is_none() {
        return;
    }
    control_flush_all_blocks(owner);
    loop {
        let (space, pending) = {
            let Some(state) = owner.borrow_control_mut() else {
                return;
            };
            let Some(buffered) = state
                .write_event
                .with_ptr(|stream| evbuffer_get_length(&(*stream).output))
            else {
                return;
            };
            if buffered >= CONTROL_BUFFER_HIGH as usize || state.pending_panes.is_empty() {
                break;
            }
            (
                CONTROL_BUFFER_HIGH as usize - buffered,
                state.pending_snapshot(),
            )
        };
        let name = owner.name();
        log_debug(format_args!(
            "control_write_callback: {}: {} bytes available, {} panes",
            log_cstr(name.as_deref().unwrap_or(c"(null)")),
            space,
            pending.len()
        ));
        let limit = (space / pending.len() / 3).max(CONTROL_WRITE_MINIMUM as usize);
        for pane in pending {
            {
                let Some(state) = owner.borrow_control_mut() else {
                    return;
                };
                let Some(buffered) = state
                    .write_event
                    .with_ptr(|stream| evbuffer_get_length(&(*stream).output))
                else {
                    return;
                };
                if buffered >= CONTROL_BUFFER_HIGH as usize {
                    break;
                }
                if !state.pending_panes.contains(&pane) {
                    continue;
                }
            }
            if control_write_pending(owner, pane, limit) == 0 {
                let Some(mut state) = owner.borrow_control_mut() else {
                    return;
                };
                if state.remove_pending(pane) {
                    state
                        .panes
                        .get_mut(&pane)
                        .expect("indexed control pane")
                        .pending_flag = 0;
                }
            }
        }
    }
    let stream = {
        let Some(state) = owner.borrow_control_mut() else {
            return;
        };
        state.write_event.clone()
    };
    if stream.with_ptr(|stream| evbuffer_get_length(&(*stream).output)) == Some(0) {
        let _ = stream.with_ptr(|stream| bufferevent_disable(stream, EV_WRITE as i16));
    }
}
unsafe fn control_sub_change(change: &monitor_change) {
    let Some(client_owner) = change.c.upgrade() else {
        return;
    };
    if client_owner.is_dead() {
        drop(client_owner);
        return;
    }
    let Some(session_owner) = change.s.upgrade() else {
        drop(client_owner);
        return;
    };
    let mut link = change.wl.try_borrow_mut().ok();
    if !change.wl.is_empty() && link.is_none() {
        drop(session_owner);
        drop(client_owner);
        return;
    }
    let wl = link
        .as_mut()
        .map_or(std::ptr::null_mut(), |link| &raw mut **link);
    let pane_owner = std::rc::Rc::<std::cell::UnsafeCell<window_pane>>::from_observer(&change.wp);
    if !std::rc::Weak::ptr_eq(&change.wp, &std::rc::Weak::new()) && pane_owner.is_none() {
        drop(session_owner);
        drop(client_owner);
        return;
    }
    let session_id = session_owner.id();
    let location = if !wl.is_null() {
        let (window_id, pane_id) = if let Some(pane) = pane_owner.as_ref() {
            let window = pane
                .window_observer()
                .upgrade()
                .expect("live subscription pane parent");
            let id = window.id();
            drop(window);
            (id, Some(pane.id()))
        } else {
            (
                (*wl)
                    .window_handle()
                    .expect("live subscription window")
                    .id(),
                None,
            )
        };
        Some((window_id, (*wl).idx as u32, pane_id))
    } else {
        None
    };
    drop(link);
    control_notify_write(&client_owner, |out| {
        out.write_all(b"%subscription-changed ")?;
        out.write_all(change.name.to_bytes())?;
        match location {
            Some((window, index, Some(pane))) => {
                write!(out, " ${session_id} @{window} {index} %{pane} : ")?
            }
            Some((window, index, None)) => write!(out, " ${session_id} @{window} {index} - : ")?,
            None => write!(out, " ${session_id} - - - : ")?,
        }
        out.write_all(change.value.to_bytes())
    });
    drop(session_owner);
    drop(client_owner);
}
pub(crate) fn control_stream_callbacks(
    owner: &ClientRef,
) -> (
    bufferevent_data_cb,
    bufferevent_data_cb,
    bufferevent_event_cb,
) {
    let read = Rc::downgrade(owner);
    let write = read.clone();
    let error = read.clone();
    (
        bufferevent_data_callback(move |_| {
            if let Some(owner) = read.upgrade() {
                unsafe { control_read_callback(&owner) };
            }
        }),
        bufferevent_data_callback(move |_| {
            if let Some(owner) = write.upgrade() {
                unsafe { control_write_callback(&owner) };
            }
        }),
        bufferevent_event_callback(move |_, _| {
            if let Some(owner) = error.upgrade() {
                unsafe { control_error_callback(&owner) };
            }
        }),
    )
}

pub unsafe fn control_start(owner: &ClientRef) {
    owner.start_control();
}

pub(crate) unsafe fn control_subscriptions(
    owner: &ClientRef,
) -> refbox::RefBox<crate::src::shared::monitor::monitor_set> {
    monitor_create_client_owned(
        Some(owner),
        monitor_callback(|change| unsafe { control_sub_change(change) }),
    )
}

pub unsafe fn control_ready(c: &ClientRef) {
    let stream = {
        c.borrow_control_mut()
            .expect("control client state")
            .read_event
            .clone()
    };
    let _ = stream.with_ptr(|stream| unsafe { bufferevent_enable(stream, EV_READ as i16) });
}
pub unsafe fn control_discard(c: &ClientRef) {
    let mut state = c.borrow_control_mut().expect("control client state");
    control_discard_pane_output(state);
}

/// Component-only work: no Client access and no synchronous model callbacks.
pub(crate) unsafe fn control_discard_pane_output(cs: &mut control_state) {
    for cp in cs.panes.values_mut() {
        while let Some(block) = cp.blocks.pop_front() {
            control_release_block(&mut cs.all_blocks, &mut cs.queued_reply_bytes, &block);
        }
    }
    let _ = cs
        .read_event
        .with_ptr(|stream| unsafe { bufferevent_disable(stream, EV_READ as ::core::ffi::c_short) });
}

pub unsafe fn control_discard_all(c: &ClientRef) {
    control_discard(c);
    let mut state = c.borrow_control_mut().expect("control client state");
    let cs = &mut *state;
    loop {
        let cb = control_first_block(cs);
        if !cb.is_alive() {
            break;
        }
        control_free_block(cs, &cb);
    }
    cs.queued_reply_bytes = 0 as size_t;
    let _ = cs.write_event.with_ptr(|stream| unsafe {
        bufferevent_disable(stream, EV_WRITE as ::core::ffi::c_short)
    });
}
pub unsafe fn control_stop(owner: &ClientRef) {
    owner.stop_control();
}

pub(crate) unsafe fn control_clear_remaining(cs: &mut control_state) {
    cs.windows.clear();
    loop {
        let block = control_first_block(cs);
        if !block.is_alive() {
            break;
        }
        control_free_block(cs, &block);
    }
}

pub unsafe fn control_add_sub(
    c_owner: &ClientRef,
    name: &CStr,
    mut type_0: monitor_type,
    mut id: ::core::ffi::c_int,
    format: &CStr,
) {
    let subscriptions = {
        let state = c_owner.borrow_control_mut().expect("control client state");
        state
            .subs
            .as_ref()
            .expect("control subscriptions")
            .downgrade()
    };
    monitor_add(
        &subscriptions,
        name,
        type_0,
        id,
        format,
        MONITOR_NOTIFY_INITIAL,
    );
}
pub unsafe fn control_remove_sub(c_owner: &ClientRef, name: &CStr) {
    let subscriptions = {
        let state = c_owner.borrow_control_mut().expect("control client state");
        state
            .subs
            .as_ref()
            .expect("control subscriptions")
            .downgrade()
    };
    monitor_remove(&subscriptions, name);
}

fn control_windows_set(windows: &mut control_windows, window: u_int, sx: u_int, sy: u_int) {
    let entry = windows
        .entry(window)
        .or_insert_with(|| Box::new(control_window { window, sx, sy }));
    entry.sx = sx;
    entry.sy = sy;
}

#[cfg(test)]
mod control_queue_tests {
    use super::*;

    #[test]
    fn pane_index_owns_pane_boxes_and_keeps_addresses_stable() {
        let mut index = Default::default();
        let mut id = 4;
        let mut offset = window_pane_offset { used: 123 };
        let first = control_add_pane(&mut index, id, offset);
        assert_eq!((first.offset.used, first.queued.used), (123, 123));
        first.flags = CONTROL_PANE_OFF;
        let address = first as *mut control_pane as usize;
        for id in 5..100 {
            control_add_pane(&mut index, id, offset);
        }
        id = 4;
        offset.used = 456;
        let first = control_add_pane(&mut index, id, offset);
        assert_eq!(first as *mut control_pane as usize, address);
        assert_eq!(
            (first.offset.used, first.queued.used, first.flags),
            (123, 123, CONTROL_PANE_OFF)
        );
        let moved = index;
        assert_eq!(moved.get(&9).unwrap().pane, 9);
        assert!(!moved.contains_key(&100));
        drop(moved);
    }

    #[test]
    fn window_index_owns_overrides_and_borrows_stable_entries() {
        let mut index: control_windows = Default::default();
        assert!(!index.contains_key(&4));
        index.remove(&4);
        assert!(index.is_empty());

        control_windows_set(&mut index, 4, 80, 24);
        let address = index.get(&4).unwrap().as_ref() as *const control_window as usize;
        for id in 5..100 {
            control_windows_set(&mut index, id, id, id + 1);
        }
        control_windows_set(&mut index, 4, 120, 40);
        let mut moved = index;
        let first = moved.get(&4).unwrap();
        assert_eq!(first.as_ref() as *const control_window as usize, address);
        assert_eq!((first.sx, first.sy), (120, 40));
        moved.remove(&4);
        assert!(!moved.contains_key(&4));
        assert_eq!(moved.get(&9).unwrap().sx, 9);
        drop(moved);

        let mut index = Default::default();
        control_windows_set(&mut index, 4, 80, 24);
        index.remove(&4);
        assert!(index.is_empty());
        control_windows_set(&mut index, 4, 90, 30);
        assert_eq!(index.get(&4).unwrap().sy, 30);
    }
}
