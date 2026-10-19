use crate::arguments::{args, args_get, args_has, args_parse};
use crate::cmd::cmd_get_args_mut;
use crate::cmd::queue::{cmdq_error, cmdq_get_client, cmdq_print};
use crate::format::bytes::nullable_cstr;
use crate::format::bytes::write_cstr;
use crate::format::{
    format_add, format_create_with_client, format_defaults, format_expand_cstring, format_true,
};
use crate::session::sessions;

use crate::session::Session;
use crate::shared::abi::*;
use crate::shared::client::ClientRef;
use crate::shared::command::CMD_AFTERHOOK;
use crate::shared::command::*;
use crate::shared::command::{cmd, cmd_entry, cmd_entry_flag, cmd_find_state, cmdq_item};
use crate::shared::format::FORMAT_NONE;
use crate::shared::session::SessionRef;
use crate::shared::sort::sort_criteria;
use crate::shared::sort::*;
use crate::shared::tree::RB_NEGINF;
use crate::shared::window::winlink;
use crate::sort::{sort_get_panes_window, sort_order_from_string};
use crate::window::{winlinks_minmax, winlinks_next};
pub static cmd_list_panes_entry: cmd_entry = {
    cmd_entry {
        name: c"list-panes",
        alias: Some(c"lsp"),
        args: args_parse {
            template: c"aF:f:O:rst:",
            lower: 0 as ::core::ffi::c_int,
            upper: 0 as ::core::ffi::c_int,
            cb: None,
        },
        usage: c"[-asr] [-F format] [-f filter] [-O order][-t target-window]",
        source: cmd_entry_flag {
            flag: 0,
            type_0: CMD_FIND_PANE,
            flags: 0,
        },
        target: cmd_entry_flag {
            flag: 't' as i32 as ::core::ffi::c_char,
            type_0: CMD_FIND_WINDOW,
            flags: 0 as ::core::ffi::c_int,
        },
        flags: CMD_AFTERHOOK,
        exec: Some(cmd_list_panes_exec),
    }
};
unsafe fn cmd_list_panes_exec(
    mut self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
) -> CmdOutcome {
    let item = item_handle.borrow();
    let mut args: *mut args =
        cmd_get_args_mut(self_0.get_mut_unchecked()).map_or(std::ptr::null_mut(), |args| args);
    let mut target_state = crate::cmd::queue::cmdq_get_target(&item).clone();
    let mut target: *mut cmd_find_state = &mut target_state;
    let mut s: Option<SessionRef> = (*target).session_handle();
    let mut wl: refbox::Weak<winlink> = (*target).winlink_handle();
    let mut order: sort_order = SORT_ACTIVITY;
    order = sort_order_from_string(nullable_cstr(
        args_get(&*(args), 'O' as i32 as u_char).map_or(std::ptr::null(), |value| value.as_ptr()),
    ));
    if order as ::core::ffi::c_uint == SORT_END as ::core::ffi::c_int as ::core::ffi::c_uint
        && args_has(&*args, 'O' as i32 as u_char) != 0
    {
        cmdq_error(item_handle, |out| out.write_all(b"invalid sort order"));
        return CmdOutcome::Error;
    }
    if args_has(&*args, 'a' as i32 as u_char) != 0 {
        cmd_list_panes_server(self_0.clone(), item_handle);
    } else if args_has(&*args, 's' as i32 as u_char) != 0 {
        cmd_list_panes_session(
            self_0.clone(),
            s.as_ref().expect("live session"),
            item_handle,
            1 as ::core::ffi::c_int,
        );
    } else {
        cmd_list_panes_window(
            self_0.clone(),
            s.as_ref().expect("live session"),
            wl.clone(),
            item_handle,
            0 as ::core::ffi::c_int,
        );
    }
    CmdOutcome::Normal
}
unsafe fn cmd_list_panes_server(
    mut self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
) {
    let mut s: Option<SessionRef> = None;
    let mut s_owner = sessions::first();
    s = s_owner.clone();
    while !s.is_none() {
        cmd_list_panes_session(
            self_0.clone(),
            s.as_ref().expect("live session"),
            item_handle,
            2 as ::core::ffi::c_int,
        );
        s_owner = sessions::after(s.as_ref().expect("live session").name().as_c_str());
        s = s_owner.clone();
    }
}
unsafe fn cmd_list_panes_session(
    mut self_0: refbox::Weak<cmd>,
    s_owner: &SessionRef,
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
    mut type_0: ::core::ffi::c_int,
) {
    let s = Some(s_owner.clone());
    let mut wl: refbox::Weak<winlink> = refbox::Weak::new();
    wl = s
        .as_ref()
        .expect("live session")
        .with_winlinks(|links| winlinks_minmax(links, RB_NEGINF));
    while wl.is_alive() {
        cmd_list_panes_window(self_0.clone(), s_owner, wl.clone(), item_handle, type_0);
        wl = winlinks_next(wl.get_unchecked());
    }
}
unsafe fn cmd_list_panes_window(
    mut self_0: refbox::Weak<cmd>,
    s_owner: &SessionRef,
    mut wl: refbox::Weak<winlink>,
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
    mut type_0: ::core::ffi::c_int,
) {
    let item = item_handle.borrow();

    let queue_client = cmdq_get_client(Some(&*item));
    let mut args: *mut args =
        cmd_get_args_mut(self_0.get_mut_unchecked()).map_or(std::ptr::null_mut(), |args| args);
    let c_owner = cmdq_get_client(Some(&*item));
    let mut c: Option<ClientRef> = c_owner.clone();
    let mut i: u_int = 0;
    let mut template: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    let mut filter: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    let mut flag: ::core::ffi::c_int = 0;
    let mut sort_crit: sort_criteria = sort_criteria {
        order: SORT_ACTIVITY,
        reversed: 0,
        order_seq: &[],
    };
    template =
        args_get(&*(args), 'F' as i32 as u_char).map_or(std::ptr::null(), |value| value.as_ptr());
    if template.is_null() {
        match type_0 {
            0 => {
                template = c"#{pane_index}: [#{pane_width}x#{pane_height}] [history #{history_size}/#{history_limit}, #{history_bytes} bytes] #{pane_id}#{?pane_active, (active),}#{?pane_dead, (dead),}".as_ptr();
            }
            1 => {
                template = c"#{window_index}.#{pane_index}: [#{pane_width}x#{pane_height}] [history #{history_size}/#{history_limit}, #{history_bytes} bytes] #{pane_id}#{?pane_active, (active),}#{?pane_dead, (dead),}".as_ptr();
            }
            2 => {
                template = c"#{session_name}:#{window_index}.#{pane_index}: [#{pane_width}x#{pane_height}] [history #{history_size}/#{history_limit}, #{history_bytes} bytes] #{pane_id}#{?pane_active, (active),}#{?pane_dead, (dead),}".as_ptr();
            }
            _ => {}
        }
    }
    filter =
        args_get(&*(args), 'f' as i32 as u_char).map_or(std::ptr::null(), |value| value.as_ptr());
    sort_crit.order = sort_order_from_string(nullable_cstr(
        args_get(&*(args), 'O' as i32 as u_char).map_or(std::ptr::null(), |value| value.as_ptr()),
    ));
    sort_crit.reversed = args_has(&*args, 'r' as i32 as u_char);
    let l = sort_get_panes_window(
        wl.get_unchecked().window_handle().expect("linked window"),
        &sort_crit,
    );
    let n = u_int::try_from(l.len()).expect("too many panes to list");
    i = 0 as u_int;
    while i < n {
        let pane = &l[i as usize];
        let mut ft_owner = format_create_with_client(
            queue_client.as_ref(),
            Some(item_handle),
            FORMAT_NONE,
            0 as ::core::ffi::c_int,
        );
        let ft = &mut *ft_owner;
        format_add(ft, c"line", |out| write!(out, "{}", { n }));
        format_defaults(ft, c.as_ref(), Some(s_owner), wl.clone(), Some(pane));
        if !filter.is_null() {
            let expanded = format_expand_cstring(ft, filter);
            flag = format_true(expanded.as_ptr());
        } else {
            flag = 1 as ::core::ffi::c_int;
        }
        if flag != 0 {
            let line = format_expand_cstring(ft, template);
            cmdq_print(item_handle, |out| write_cstr(out, &*line));
        }
        drop(ft_owner);
        i = i.wrapping_add(1);
    }
}
