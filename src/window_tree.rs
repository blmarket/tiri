use crate::arguments::{args, args_count, args_get, args_has, args_string};
use crate::cmd::queue::{cmdq_append, cmdq_get_callback_owned};
use crate::format::{
    format_add, format_create, format_defaults, format_expand_cstring, format_single_cstring,
    format_true,
};
use crate::format_draw::{format_draw, format_trim_left_bytes, format_width};
use crate::grid::grid_default_cell;
use crate::key_string::key_string_parse_cstr;
use crate::mode_tree::{
    mode_tree_align, mode_tree_build, mode_tree_draw, mode_tree_each_tagged, mode_tree_expand,
    mode_tree_expand_current, mode_tree_free, mode_tree_key, mode_tree_resize,
    mode_tree_run_command, mode_tree_set_prompt, mode_tree_start, mode_tree_visit_screen,
};
use crate::mode_tree::{mode_tree_data, mode_tree_help_info, ModeTreeItemData, ModeTreeItemRef};
use crate::options::options_get_string;
use crate::options::OptionsScope;
use crate::resize::recalculate_sizes;
use crate::screen_write::{
    screen_write_box, screen_write_clearcharacter, screen_write_cursormove, screen_write_hline,
    screen_write_preview, screen_write_putc, screen_write_puts, screen_write_vline,
};
use crate::server::{server_clear_marked, server_set_marked};
use crate::server_fn::{
    server_destroy_session, server_kill_pane, server_kill_window, server_redraw_session_group,
    server_renumber_all,
};
use crate::session::session_group_synchronize_from;
use crate::session::Session;
use crate::shared::abi::*;
use crate::shared::client::ClientRef;
use crate::shared::command::*;
use crate::shared::command::{cmd_find_state, cmdq_item};
use crate::shared::format::format_tree;
use crate::shared::format::{FORMAT_NONE, FORMAT_PANE, FORMAT_WINDOW};
use crate::shared::grid::*;
use crate::shared::key::*;
use crate::shared::layout::*;
use crate::shared::menu::menu_item;
use crate::shared::mouse::mouse_event;
use crate::shared::pane::window_pane;
use crate::shared::prompt::*;
use crate::shared::prompt::{
    prompt_result, PROMPT_ACCEPT, PROMPT_CLOSE, PROMPT_NOFORMAT, PROMPT_SINGLE,
};
use crate::shared::screen::screen;
use crate::shared::screen_write::screen_write_ctx;
use crate::shared::session::SessionRef;
use crate::shared::sort::sort_criteria;
use crate::shared::sort::*;
use crate::shared::tree::RB_NEGINF;
use crate::shared::window::WindowRef;
use crate::shared::window::{
    window_mode, window_mode_entry, window_mode_screen_payload, winlink, ModeCell,
};
use crate::sort::{
    sort_get_panes_window, sort_get_sessions, sort_get_winlinks_session,
    sort_would_window_tree_swap,
};
use crate::style::style_apply_with_options;
use crate::text::search::contains;
use crate::window::Window;
use crate::window::WindowPane;
use crate::window::{winlink_count, winlink_find_by_index, winlink_next, winlinks_minmax};
use crate::winlink::winlink_window;
use std::cell::RefCell;
use std::ffi::{CStr, CString};
use std::rc::{Rc, Weak};

#[repr(C)]
pub struct window_tree_modedata {
    pub wp: Weak<RefCell<window_pane>>,
    pub data: Option<std::rc::Rc<std::cell::RefCell<mode_tree_data>>>,
    pub format: CString,
    pub key_format: CString,
    pub command: CString,
    pub squash_groups: ::core::ffi::c_int,
    pub hide_preview_this_pane: ::core::ffi::c_int,
    pub preview_is_info: ::core::ffi::c_int,
    pub prompt_flags: ::core::ffi::c_int,
    pub item_list: Vec<refbox::RefBox<window_tree_itemdata>>,
    pub entered: Option<::std::ffi::CString>,
    pub fs: cmd_find_state,
    pub type_0: window_tree_type,
    pub offset: ::core::ffi::c_int,
    pub left: ::core::ffi::c_int,
    pub right: ::core::ffi::c_int,
    pub start: u_int,
    pub end: u_int,
    pub each: u_int,
}

/// The mode payload shared with prompt and queued command callbacks.
type TreeMode = Rc<ModeCell<window_tree_modedata>>;

impl window_tree_modedata {
    fn tree_owner(&self) -> Rc<std::cell::RefCell<mode_tree_data>> {
        self.data.as_ref().expect("mode tree owner").clone()
    }
}

pub type window_tree_type = ::core::ffi::c_uint;
pub const WINDOW_TREE_PANE: window_tree_type = 3;
pub const WINDOW_TREE_WINDOW: window_tree_type = 2;
pub const WINDOW_TREE_SESSION: window_tree_type = 1;
pub const WINDOW_TREE_NONE: window_tree_type = 0;
/// Immutable row identity; actions resolve these IDs against the live model.
#[derive(Copy, Clone)]
#[repr(C)]
pub struct window_tree_itemdata {
    pub type_0: window_tree_type,
    pub session: ::core::ffi::c_int,
    pub winlink: ::core::ffi::c_int,
    pub pane: ::core::ffi::c_int,
}

pub const WINDOW_TREE_DEFAULT_COMMAND: &std::ffi::CStr = c"switch-client -t '%%'";
pub const WINDOW_TREE_DEFAULT_FORMAT: &CStr = c"#{?pane_format,#{?pane_marked,#[fg=thememagenta],}#{pane_current_command}#[fg=themelightgrey]#{pane_flags}#{?#{&&:#{pane_title},#{!=:#{pane_title},#{host_short}}},: \"#{pane_title}\",},window_format,#{?window_marked_flag,#[fg=thememagenta],}#{window_name}#[fg=themelightgrey]#{window_flags}#{?#{&&:#{==:#{window_panes},1},#{&&:#{pane_title},#{!=:#{pane_title},#{host_short}}}},: \"#{pane_title}\",},#[fg=themelightgrey]#{session_windows} windows#{?session_grouped, (group #{session_group}: #{session_group_list}),}#{?session_attached, (attached),}}";
pub const WINDOW_TREE_DEFAULT_KEY_FORMAT: &std::ffi::CStr =
    c"#{?#{e|<:#{line},10},#{line},#{e|<:#{line},36},M-#{a:#{e|+:97,#{e|-:#{line},10}}}}";
static window_tree_menu_items: [menu_item<'static>; 12] = [
    menu_item {
        name: c"Select",
        key: '\r' as i32 as key_code,
        command: None,
    },
    menu_item {
        name: c"Expand",
        key: KEYC_RIGHT as ::core::ffi::c_ulong as key_code,
        command: None,
    },
    menu_item {
        name: c"Mark",
        key: 'm' as i32 as key_code,
        command: None,
    },
    menu_item {
        name: c"",
        key: KEYC_NONE as ::core::ffi::c_ulong as key_code,
        command: None,
    },
    menu_item {
        name: c"Tag",
        key: 't' as i32 as key_code,
        command: None,
    },
    menu_item {
        name: c"Tag All",
        key: '\u{14}' as i32 as key_code,
        command: None,
    },
    menu_item {
        name: c"Tag None",
        key: 'T' as i32 as key_code,
        command: None,
    },
    menu_item {
        name: c"",
        key: KEYC_NONE as ::core::ffi::c_ulong as key_code,
        command: None,
    },
    menu_item {
        name: c"Kill",
        key: 'x' as i32 as key_code,
        command: None,
    },
    menu_item {
        name: c"Kill Tagged",
        key: 'X' as i32 as key_code,
        command: None,
    },
    menu_item {
        name: c"",
        key: KEYC_NONE as ::core::ffi::c_ulong as key_code,
        command: None,
    },
    menu_item {
        name: c"Cancel",
        key: 'q' as i32 as key_code,
        command: None,
    },
];
pub static window_tree_mode: window_mode = {
    window_mode {
        name: c"tree-mode",
        default_format: Some(WINDOW_TREE_DEFAULT_FORMAT),
        flags: 0,
        init: Some(window_tree_init),
        free: Some(window_tree_free),
        resize: Some(window_tree_resize),
        update: Some(window_tree_update),
        style_changed: None,
        key: Some(window_tree_key),
        key_table: None,
        command: None,
        formats: None,
        get_screen: None,
        display_screen: Some(window_tree_get_screen),
    }
};
static window_tree_order_seq: [sort_order; 3] = [SORT_INDEX, SORT_NAME, SORT_ACTIVITY];
const window_tree_pane_info_lines: [&CStr; 8] = [
    c"#[fg=themelightgrey]Pane          #[#{E:tree-mode-border-style},acs]x#[default] #{pane_index} #[fg=themelightgrey](#{pane_id})#[default]",
    c"#[fg=themelightgrey]Title         #[#{E:tree-mode-border-style},acs]x#[default] #{pane_title}",
    c"#[fg=themelightgrey]Command       #[#{E:tree-mode-border-style},acs]x#[default] #{pane_current_command} #[fg=themelightgrey](PID #{pane_pid})#[default]",
    c"#[fg=themelightgrey]Path          #[#{E:tree-mode-border-style},acs]x#[default] #{pane_current_path}",
    c"#[fg=themelightgrey]TTY           #[#{E:tree-mode-border-style},acs]x#[default] #{pane_tty}",
    c"#[fg=themelightgrey]Position      #[#{E:tree-mode-border-style},acs]x#[default] #{pane_x},#{pane_y} #{pane_width}x#{pane_height}",
    c"#[fg=themelightgrey]Mode          #[#{E:tree-mode-border-style},acs]x#[default] #{?pane_in_mode,#{pane_mode},none}",
    c"#[fg=themelightgrey]Flags         #[#{E:tree-mode-border-style},acs]x#[default] #{?pane_active,#[fg=themegreen],#[fg=themelightgrey]}active#[default] #{?window_zoomed_flag,#[fg=themegreen],#[fg=themelightgrey]}zoomed#[default] #{?pane_marked,#[fg=themegreen],#[fg=themelightgrey]}marked#[default] #{?pane_synchronized,#[fg=themegreen],#[fg=themelightgrey]}sync#[default] #{?pane_dead,#[fg=themegreen],#[fg=themelightgrey]}dead#[default] #{?pane_pipe,#[fg=themegreen],#[fg=themelightgrey]}piped#[default]",
];
const window_tree_window_info_lines: [&CStr; 6] = [
    c"#[fg=themelightgrey]Window        #[#{E:tree-mode-border-style},acs]x#[default] #{window_index}: #{window_name} #[fg=themelightgrey](#{window_id})#[default]",
    c"#[fg=themelightgrey]Size          #[#{E:tree-mode-border-style},acs]x#[default] #{window_width}x#{window_height}",
    c"#[fg=themelightgrey]Panes         #[#{E:tree-mode-border-style},acs]x#[default] #{window_panes}",
    c"#[fg=themelightgrey]Activity Time #[#{E:tree-mode-border-style},acs]x#[default] #{t:window_activity} #[fg=themelightgrey](#{t/r:window_activity})#[default]",
    c"#[fg=themelightgrey]Sessions      #[#{E:tree-mode-border-style},acs]x#[default] #{s/,/ /:window_linked_sessions_list}",
    c"#[fg=themelightgrey]Flags         #[#{E:tree-mode-border-style},acs]x#[default] #{?window_active,#[fg=themegreen],#[fg=themelightgrey]}active#[default] #{?window_last_flag,#[fg=themegreen],#[fg=themelightgrey]}last#[default] #{?window_bell_flag,#[fg=themegreen],#[fg=themelightgrey]}bell#[default] #{?window_activity_flag,#[fg=themegreen],#[fg=themelightgrey]}activity#[default] #{?window_silence_flag,#[fg=themegreen],#[fg=themelightgrey]}silence#[default] #{?window_zoomed_flag,#[fg=themegreen],#[fg=themelightgrey]}zoomed#[default] #{?window_marked_flag,#[fg=themegreen],#[fg=themelightgrey]}marked#[default]",
];
const window_tree_session_info_lines: [&CStr; 9] = [
    c"#[fg=themelightgrey]Session       #[#{E:tree-mode-border-style},acs]x#[default] #{session_name} #[fg=themelightgrey](#{session_id})#[default]",
    c"#[fg=themelightgrey]Created Time  #[#{E:tree-mode-border-style},acs]x#[default] #{t:session_created} #[fg=themelightgrey](#{t/r:session_created})#[default]",
    c"#[fg=themelightgrey]Activity Time #[#{E:tree-mode-border-style},acs]x#[default] #{t:session_activity} #[fg=themelightgrey](#{t/r:session_activity})#[default]",
    c"#[fg=themelightgrey]Attached Time #[#{E:tree-mode-border-style},acs]x#[default] #{?#{t:session_last_attached},#{t:session_last_attached} #[fg=themelightgrey](#{t/r:session_last_attached})#[default],never}",
    c"#[fg=themelightgrey]Clients       #[#{E:tree-mode-border-style},acs]x#[default] #{s/,/ /:session_attached_list}",
    c"#[fg=themelightgrey]Windows       #[#{E:tree-mode-border-style},acs]x#[default] #{session_windows}",
    c"#[fg=themelightgrey]Path          #[#{E:tree-mode-border-style},acs]x#[default] #{session_path}",
    c"#[fg=themelightgrey]Group         #[#{E:tree-mode-border-style},acs]x#[default] #{?session_grouped,#{session_group} (#{session_group_size}),none}",
    c"#[fg=themelightgrey]Flags         #[#{E:tree-mode-border-style},acs]x#[default] #{?session_attached,#[fg=themegreen],#[fg=themelightgrey]}attached#[default] #{?session_grouped,#[fg=themegreen],#[fg=themelightgrey]}grouped#[default] #{?session_marked,#[fg=themegreen],#[fg=themelightgrey]}marked#[default] #{?session_bell_flag,#[fg=themegreen],#[fg=themelightgrey]}bell#[default] #{?session_activity_flag,#[fg=themegreen],#[fg=themelightgrey]}activity#[default] #{?session_silence_flag,#[fg=themegreen],#[fg=themelightgrey]}silence#[default]",
];
#[must_use = "retain the resolved session and pane while using the output pointers"]
#[derive(Default)]
struct WindowTreeTarget {
    session: Option<SessionRef>,
    winlink: refbox::Weak<winlink>,
    pane: Option<Rc<RefCell<window_pane>>>,
}

fn window_tree_pull_item(item: &window_tree_itemdata) -> WindowTreeTarget {
    let Some(session) = crate::session::sessions::find_by_id(item.session as u_int) else {
        return WindowTreeTarget::default();
    };
    let s = Some(session.clone());
    let wl = if item.type_0 == WINDOW_TREE_SESSION {
        s.as_ref().expect("live session").current_winlink()
    } else {
        s.as_ref()
            .expect("live session")
            .with_winlinks(|links| winlink_find_by_index(links, item.winlink))
    };
    let Ok(link) = wl.try_borrow() else {
        return WindowTreeTarget::default();
    };
    let pane = if item.type_0 == WINDOW_TREE_SESSION || item.type_0 == WINDOW_TREE_WINDOW {
        link.window_handle()
            .expect("live window")
            .active_pane()
            .upgrade()
    } else {
        let pane = Rc::<RefCell<window_pane>>::find_by_id(item.pane as u_int);
        if !pane.as_ref().is_some_and(|owner| {
            link.window_handle()
                .expect("live window")
                .contains_pane(&Rc::downgrade(owner))
        }) {
            return WindowTreeTarget::default();
        }
        pane
    };
    WindowTreeTarget {
        session: Some(session),
        winlink: wl.clone(),
        pane,
    }
}
fn window_tree_add_item(
    items: &mut Vec<refbox::RefBox<window_tree_itemdata>>,
    value: window_tree_itemdata,
) -> refbox::Weak<window_tree_itemdata> {
    let item = refbox::RefBox::new(value);
    let handle = item.downgrade();
    items.push(item);
    handle
}
fn window_tree_remove_last_item(
    items: &mut Vec<refbox::RefBox<window_tree_itemdata>>,
    tree: &mut mode_tree_data,
    item: &refbox::Weak<window_tree_itemdata>,
    mti: &ModeTreeItemRef,
) {
    debug_assert!(items.last().is_some_and(|last| item.is(last)));
    tree.remove(mti);
    items.pop();
}
fn window_tree_preview(
    pane: &Rc<RefCell<window_pane>>,
    ctx: &mut screen_write_ctx,
    sx: u32,
    sy: u32,
) {
    let (width, height) = pane.screen_size(false);
    let mut snapshot = screen::empty();
    pane.copy_screen(&mut snapshot, 0, 0, width, height, false);
    screen_write_preview(ctx, &snapshot, sx, sy);
    drop(snapshot);
}
fn window_tree_pane_index(pane: &Rc<RefCell<window_pane>>) -> Option<u32> {
    let window = pane.window_observer().upgrade()?;
    let index = window.pane_index(&Rc::downgrade(pane));
    drop(window);
    index
}
fn window_tree_build_pane(
    session_owner: &SessionRef,
    mut wl: refbox::Weak<winlink>,
    pane_owner: &Rc<RefCell<window_pane>>,
    data: &window_tree_modedata,
    items: &mut Vec<refbox::RefBox<window_tree_itemdata>>,
    parent: &ModeTreeItemRef,
) {
    let s = Some(session_owner.clone());
    let tag = Rc::as_ptr(pane_owner) as uint64_t;
    let mut idx: u_int = 0;
    idx = window_tree_pane_index(pane_owner).expect("pane belongs to window ordering");
    let item_owner = window_tree_add_item(
        items,
        window_tree_itemdata {
            type_0: WINDOW_TREE_PANE,
            session: s.as_ref().expect("live session").id() as ::core::ffi::c_int,
            winlink: wl.try_borrow().expect("live winlink").idx,
            pane: pane_owner.id() as ::core::ffi::c_int,
        },
    );
    let mut ft_owner = format_create(
        None,
        None,
        (FORMAT_PANE | pane_owner.id()) as ::core::ffi::c_int,
        0 as ::core::ffi::c_int,
    );
    let ft = &mut *ft_owner;
    format_defaults(ft, None, s.as_ref(), wl.clone(), Some(pane_owner));
    let text = format_expand_cstring(ft, &data.format);
    let name = CString::new(idx.to_string()).expect("pane index contains NUL");
    drop(ft_owner);
    let mti = data.tree_owner().borrow_mut().add(
        Some(parent),
        ModeTreeItemData::Tree(item_owner.clone()),
        tag,
        &name,
        Some(&text),
        -(1 as ::core::ffi::c_int),
    );
    mode_tree_align(&mti);
}
fn window_tree_filter_pane(
    session_owner: &SessionRef,
    mut wl: refbox::Weak<winlink>,
    pane_owner: &Rc<RefCell<window_pane>>,
    filter: Option<&CStr>,
) -> ::core::ffi::c_int {
    let Some(filter) = filter else {
        return 1 as ::core::ffi::c_int;
    };
    let cp = format_single_cstring(
        None,
        filter,
        None,
        Some(session_owner),
        wl.clone(),
        Some(pane_owner),
    );
    ::core::ffi::c_int::from(format_true(&cp))
}
fn window_tree_build_window(
    session_owner: &SessionRef,
    mut wl: refbox::Weak<winlink>,
    data: &window_tree_modedata,
    items: &mut Vec<refbox::RefBox<window_tree_itemdata>>,
    sort_crit: &sort_criteria,
    parent: &ModeTreeItemRef,
    filter: Option<&CStr>,
) -> ::core::ffi::c_int {
    let s = Some(session_owner.clone());
    let Some(mode_pane_owner) = Rc::<RefCell<window_pane>>::from_observer(&data.wp) else {
        return 0;
    };
    let mut i: u_int = 0;
    let mut found: u_int = 0;
    let mut expanded: ::core::ffi::c_int = 0;
    let mut tag: uint64_t = FORMAT_NONE as uint64_t;
    let item_owner = window_tree_add_item(
        items,
        window_tree_itemdata {
            type_0: WINDOW_TREE_WINDOW,
            session: s.as_ref().expect("live session").id() as ::core::ffi::c_int,
            winlink: wl.try_borrow().expect("live winlink").idx,
            pane: -(1 as ::core::ffi::c_int),
        },
    );
    if !winlink_window(&wl).is_none()
        && !((winlink_window(&wl).as_ref()).expect("live window"))
            .active_pane()
            .upgrade()
            .is_none()
    {
        tag = (FORMAT_PANE
            | winlink_window(&wl)
                .expect("live window")
                .active_pane()
                .upgrade()
                .expect("active pane")
                .id()) as uint64_t;
    }
    let mut ft_owner = format_create(
        None,
        None,
        tag as ::core::ffi::c_int,
        0 as ::core::ffi::c_int,
    );
    let ft = &mut *ft_owner;
    format_defaults(ft, None, s.as_ref(), wl.clone(), None);
    let text = format_expand_cstring(ft, &data.format);
    let name = CString::new((wl.try_borrow().expect("live winlink").idx as u_int).to_string())
        .expect("window index contains NUL");
    drop(ft_owner);
    if data.type_0 as ::core::ffi::c_uint
        == WINDOW_TREE_SESSION as ::core::ffi::c_int as ::core::ffi::c_uint
        || data.type_0 as ::core::ffi::c_uint
            == WINDOW_TREE_WINDOW as ::core::ffi::c_int as ::core::ffi::c_uint
    {
        expanded = 0 as ::core::ffi::c_int;
    } else {
        expanded = 1 as ::core::ffi::c_int;
    }
    let mti = data.tree_owner().borrow_mut().add(
        Some(parent),
        ModeTreeItemData::Tree(item_owner.clone()),
        wl.as_ptr() as uint64_t,
        &name,
        Some(&text),
        expanded,
    );
    mode_tree_align(&mti);
    let l = sort_get_panes_window(&winlink_window(&wl).expect("linked window"), sort_crit);
    let n = u_int::try_from(l.len()).expect("too many panes in window tree");
    found = 0 as u_int;
    i = 0 as u_int;
    while i < n {
        if !(window_tree_filter_pane(session_owner, wl.clone(), &l[i as usize], filter) == 0) {
            found = found.wrapping_add(1);
            if !(data.hide_preview_this_pane != 0 && Rc::ptr_eq(&l[i as usize], &mode_pane_owner)) {
                window_tree_build_pane(
                    session_owner,
                    wl.clone(),
                    &l[i as usize],
                    data,
                    items,
                    &mti,
                );
            }
        }
        i = i.wrapping_add(1);
    }
    if found == 0 as u_int {
        let tree_owner = data.tree_owner();
        window_tree_remove_last_item(items, &mut tree_owner.borrow_mut(), &item_owner, &mti);
        return 0 as ::core::ffi::c_int;
    }
    1 as ::core::ffi::c_int
}
fn window_tree_build_session(
    session_owner: &SessionRef,
    data: &window_tree_modedata,
    items: &mut Vec<refbox::RefBox<window_tree_itemdata>>,
    sort_crit: &sort_criteria,
    filter: Option<&CStr>,
) {
    let s = Some(session_owner.clone());
    let mut wl: refbox::Weak<winlink> = s.as_ref().expect("live session").current_winlink();
    let mut i: u_int = 0;
    let mut empty: u_int = 0;
    let mut expanded: ::core::ffi::c_int = 0;
    let mut tag: uint64_t = FORMAT_NONE as uint64_t;
    let item_owner = window_tree_add_item(
        items,
        window_tree_itemdata {
            type_0: WINDOW_TREE_SESSION,
            session: s.as_ref().expect("live session").id() as ::core::ffi::c_int,
            winlink: -(1 as ::core::ffi::c_int),
            pane: -(1 as ::core::ffi::c_int),
        },
    );
    if wl.is_alive()
        && !winlink_window(&wl).is_none()
        && !((winlink_window(&wl).as_ref()).expect("live window"))
            .active_pane()
            .upgrade()
            .is_none()
    {
        tag = (FORMAT_PANE
            | winlink_window(&wl)
                .expect("live window")
                .active_pane()
                .upgrade()
                .expect("active pane")
                .id()) as uint64_t;
    }
    let mut ft_owner = format_create(
        None,
        None,
        tag as ::core::ffi::c_int,
        0 as ::core::ffi::c_int,
    );
    let ft = &mut *ft_owner;
    format_defaults(ft, None, s.as_ref(), (refbox::Weak::new()).clone(), None);
    let text = format_expand_cstring(ft, &data.format);
    drop(ft_owner);
    if data.type_0 as ::core::ffi::c_uint
        == WINDOW_TREE_SESSION as ::core::ffi::c_int as ::core::ffi::c_uint
    {
        expanded = 0 as ::core::ffi::c_int;
    } else {
        expanded = 1 as ::core::ffi::c_int;
    }
    let mti = data.tree_owner().borrow_mut().add(
        None,
        ModeTreeItemData::Tree(item_owner.clone()),
        std::rc::Rc::as_ptr(s.as_ref().expect("live session")) as uint64_t,
        &s.as_ref().expect("live session").name(),
        Some(&text),
        expanded,
    );
    let l = sort_get_winlinks_session(s.as_ref().expect("live session"), sort_crit);
    let n = u_int::try_from(l.len()).expect("too many winlinks in window tree");
    empty = 0 as u_int;
    i = 0 as u_int;
    while i < n {
        if window_tree_build_window(
            session_owner,
            (l[i as usize]).clone(),
            data,
            items,
            sort_crit,
            &mti,
            filter,
        ) == 0
        {
            empty = empty.wrapping_add(1);
        }
        i = i.wrapping_add(1);
    }
    if empty == n {
        let tree_owner = data.tree_owner();
        window_tree_remove_last_item(items, &mut tree_owner.borrow_mut(), &item_owner, &mti);
    }
}
fn window_tree_live_mode(
    observer: &std::rc::Weak<ModeCell<window_tree_modedata>>,
) -> Option<TreeMode> {
    observer.upgrade().filter(|mode| !mode.dead.get())
}

fn window_tree_tree(mode: &TreeMode) -> Rc<std::cell::RefCell<mode_tree_data>> {
    mode.data.borrow().tree_owner()
}

fn window_tree_pane(mode: &TreeMode) -> Option<Rc<RefCell<window_pane>>> {
    Rc::<RefCell<window_pane>>::from_observer(&mode.data.borrow().wp)
}

fn window_tree_build(
    mode: &TreeMode,
    sort_crit: &sort_criteria,
    tag: &mut uint64_t,
    filter: Option<&CStr>,
) {
    drop(std::mem::take(&mut mode.data.borrow_mut().item_list));
    let mut items = Vec::new();
    window_tree_build_items(mode, sort_crit, tag, filter, &mut items);
    mode.data.borrow_mut().item_list = items;
}

fn window_tree_build_items(
    mode: &TreeMode,
    sort_crit: &sort_criteria,
    tag: &mut uint64_t,
    filter: Option<&CStr>,
    items: &mut Vec<refbox::RefBox<window_tree_itemdata>>,
) {
    let guard = mode.data.borrow();
    let data = &*guard;
    let mut squash_groups: ::core::ffi::c_int = data.squash_groups;
    let mut s: Option<SessionRef> = None;
    let mut sg = crate::shared::session::SessionGroupWeak::new();
    let mut current = crate::shared::session::SessionGroupWeak::new();
    let mut i: u_int = 0;
    current = crate::session::session_groups::for_session(&data.fs.s);
    let l = sort_get_sessions(sort_crit);
    let n = u_int::try_from(l.len()).expect("too many sessions for window tree");
    if n == 0 as u_int {
        return;
    }
    let mut current_block_12: u64;
    i = 0 as u_int;
    while i < n {
        s = Some(l[i as usize].clone());
        if squash_groups != 0 && {
            sg = crate::session::session_groups::for_session(
                &s.as_ref()
                    .map_or_else(std::rc::Weak::new, std::rc::Rc::downgrade),
            );
            sg.is_alive()
        } {
            if sg == current
                && !crate::shared::rc::same(s.as_ref(), data.fs.session_handle().as_ref())
                || sg != current
                    && !crate::shared::rc::same(
                        s.as_ref(),
                        crate::session::session_group_members(&sg).first(),
                    )
            {
                current_block_12 = 3640593987805443782;
            } else {
                current_block_12 = 4166486009154926805;
            }
        } else {
            current_block_12 = 4166486009154926805;
        }
        if current_block_12 == 4166486009154926805 {
            window_tree_build_session(&l[i as usize], data, items, sort_crit, filter);
        }
        i = i.wrapping_add(1);
    }
    match data.type_0 as ::core::ffi::c_uint {
        1 => {
            if let Some(session) = data.fs.session_handle() {
                *tag = std::rc::Rc::as_ptr(&session) as uint64_t;
            }
        }
        2 => {
            if !data.fs.session_handle().is_none() && data.fs.winlink_handle().is_alive() {
                *tag = data.fs.winlink_handle().as_ptr() as uint64_t;
            }
        }
        3 => {
            if winlink_window(&data.fs.winlink_handle())
                .expect("live window")
                .pane_snapshot()
                .len()
                == 1
            {
                *tag = data.fs.winlink_handle().as_ptr() as uint64_t;
            } else {
                *tag = data
                    .fs
                    .pane_handle()
                    .as_ref()
                    .map_or(0, |owner| Rc::as_ptr(owner) as uint64_t);
            }
        }
        _ => {}
    };
}
fn window_tree_draw_label(
    mut ctx: &mut screen_write_ctx,
    mut px: u_int,
    mut py: u_int,
    mut sx: u_int,
    mut sy: u_int,
    border_gc: &grid_cell,
    label_gc: &grid_cell,
    label: &CStr,
) {
    let mut width: u_int = 0;
    let mut ox: u_int = 0;
    let mut oy: u_int = 0;
    if sx < 5 as u_int || sy < 3 as u_int {
        return;
    }
    width = format_width(label);
    let trimmed_label = if width > sx.wrapping_sub(4 as u_int) {
        Some(
            CString::new(format_trim_left_bytes(label, sx.wrapping_sub(4 as u_int)))
                .expect("trimmed preview label contains no NUL"),
        )
    } else {
        None
    };
    if let Some(trimmed_label) = trimmed_label.as_ref() {
        width = format_width(trimmed_label);
    }
    let label = trimmed_label.as_deref().unwrap_or(label);
    if width == 0 as u_int {
        return;
    }
    ox = sx
        .wrapping_sub(width)
        .wrapping_add(1 as u_int)
        .wrapping_div(2 as u_int);
    oy = sy.wrapping_add(1 as u_int).wrapping_div(2 as u_int);
    screen_write_cursormove(
        ctx,
        px.wrapping_add(ox).wrapping_sub(2 as u_int) as ::core::ffi::c_int,
        py.wrapping_add(oy).wrapping_sub(1 as u_int) as ::core::ffi::c_int,
        0 as ::core::ffi::c_int,
    );
    screen_write_box(
        ctx,
        width.wrapping_add(4 as u_int),
        3 as u_int,
        BOX_LINES_DEFAULT,
        Some(border_gc),
        None,
    );
    screen_write_cursormove(
        ctx,
        px.wrapping_add(ox).wrapping_sub(1 as u_int) as ::core::ffi::c_int,
        py.wrapping_add(oy) as ::core::ffi::c_int,
        0 as ::core::ffi::c_int,
    );
    screen_write_clearcharacter(ctx, width.wrapping_add(2 as u_int), border_gc.bg as u_int);
    screen_write_cursormove(
        ctx,
        px.wrapping_add(ox) as ::core::ffi::c_int,
        py.wrapping_add(oy) as ::core::ffi::c_int,
        0 as ::core::ffi::c_int,
    );
    format_draw(ctx, label_gc, width, label, None, 0 as ::core::ffi::c_int);
}
fn window_tree_border_cell(gc: &mut grid_cell, scope: &OptionsScope, ft: Option<&mut format_tree>) {
    style_apply_with_options(gc, c"tree-mode-border-style", ft, scope);
}
fn window_tree_draw_session(
    data: &mut window_tree_modedata,
    session_owner: &SessionRef,
    mut ctx: &mut screen_write_ctx,
    mut sx: u_int,
    mut sy: u_int,
) {
    let s = Some(session_owner.clone());
    let Some(mode_pane_owner) = Rc::<RefCell<window_pane>>::from_observer(&data.wp) else {
        return;
    };
    let mut wl: refbox::Weak<winlink> = refbox::Weak::new();
    let mut cx: u_int = ctx.screen.cx;
    let mut cy: u_int = ctx.screen.cy;
    let mut loop_0: u_int = 0;
    let mut total: u_int = 0;
    let mut visible: u_int = 0;
    let mut each: u_int = 0;
    let mut width: u_int = 0;
    let mut offset: u_int = 0;
    let mut current: u_int = 0;
    let mut start: u_int = 0;
    let mut end: u_int = 0;
    let mut remaining: u_int = 0;
    let mut i: u_int = 0;
    let mut gc: grid_cell = grid_cell {
        data: utf8_data {
            data: [0; 32],
            have: 0,
            size: 0,
            width: 0,
        },
        attr: 0,
        flags: 0,
        fg: 0,
        bg: 0,
        us: 0,
        link: 0,
    };
    let mut label_gc: grid_cell = grid_cell {
        data: utf8_data {
            data: [0; 32],
            have: 0,
            size: 0,
            width: 0,
        },
        attr: 0,
        flags: 0,
        fg: 0,
        bg: 0,
        us: 0,
        link: 0,
    };
    let mut left: ::core::ffi::c_int = 0;
    let mut right: ::core::ffi::c_int = 0;
    total = s
        .as_ref()
        .expect("live session")
        .with_winlinks(winlink_count);
    if sx.wrapping_div(total) < 24 as u_int {
        visible = sx.wrapping_div(24 as u_int);
        if visible == 0 as u_int {
            visible = 1 as u_int;
        }
    } else {
        visible = total;
    }
    current = 0 as u_int;
    wl = s
        .as_ref()
        .expect("live session")
        .with_winlinks(|links| winlinks_minmax(links, RB_NEGINF));
    while wl.is_alive() {
        if wl == s.as_ref().expect("live session").current_winlink() {
            break;
        }
        current = current.wrapping_add(1);
        wl = winlink_next(&wl);
    }
    if current < visible {
        start = 0 as u_int;
        end = visible;
    } else if current >= total.wrapping_sub(visible) {
        start = total.wrapping_sub(visible);
        end = total;
    } else {
        start = current.wrapping_sub(visible.wrapping_div(2 as u_int));
        end = start.wrapping_add(visible);
    }
    if data.offset < -(start as ::core::ffi::c_int) {
        data.offset = -(start as ::core::ffi::c_int);
    }
    if data.offset > total.wrapping_sub(end) as ::core::ffi::c_int {
        data.offset = total.wrapping_sub(end) as ::core::ffi::c_int;
    }
    start = start.wrapping_add(data.offset as u_int);
    end = end.wrapping_add(data.offset as u_int);
    left = (start != 0 as u_int) as ::core::ffi::c_int;
    right = (end != total) as ::core::ffi::c_int;
    if left != 0 && right != 0 && sx <= 6 as u_int || (left != 0 || right != 0) && sx <= 3 as u_int
    {
        right = 0 as ::core::ffi::c_int;
        left = right;
    }
    if left != 0 && right != 0 {
        each = sx.wrapping_sub(6 as u_int).wrapping_div(visible);
        remaining = sx
            .wrapping_sub(6 as u_int)
            .wrapping_sub(visible.wrapping_mul(each));
    } else if left != 0 || right != 0 {
        each = sx.wrapping_sub(3 as u_int).wrapping_div(visible);
        remaining = sx
            .wrapping_sub(3 as u_int)
            .wrapping_sub(visible.wrapping_mul(each));
    } else {
        each = sx.wrapping_div(visible);
        remaining = sx.wrapping_sub(visible.wrapping_mul(each));
    }
    if each == 0 as u_int {
        return;
    }
    let border_window = mode_pane_owner.window_observer();
    window_tree_border_cell(&mut gc, &OptionsScope::Window(border_window.clone()), None);
    if left != 0 {
        data.left = cx.wrapping_add(2 as u_int) as ::core::ffi::c_int;
        screen_write_cursormove(
            ctx,
            cx.wrapping_add(2 as u_int) as ::core::ffi::c_int,
            cy as ::core::ffi::c_int,
            0 as ::core::ffi::c_int,
        );
        screen_write_vline(ctx, sy, Some(&gc));
        screen_write_cursormove(
            ctx,
            cx as ::core::ffi::c_int,
            cy.wrapping_add(sy.wrapping_div(2 as u_int)) as ::core::ffi::c_int,
            0 as ::core::ffi::c_int,
        );
        screen_write_puts(ctx, &gc, |out| out.write_all(b"<"));
    } else {
        data.left = -(1 as ::core::ffi::c_int);
    }
    if right != 0 {
        data.right = cx.wrapping_add(sx).wrapping_sub(3 as u_int) as ::core::ffi::c_int;
        screen_write_cursormove(
            ctx,
            cx.wrapping_add(sx).wrapping_sub(3 as u_int) as ::core::ffi::c_int,
            cy as ::core::ffi::c_int,
            0 as ::core::ffi::c_int,
        );
        screen_write_vline(ctx, sy, Some(&gc));
        screen_write_cursormove(
            ctx,
            cx.wrapping_add(sx).wrapping_sub(1 as u_int) as ::core::ffi::c_int,
            cy.wrapping_add(sy.wrapping_div(2 as u_int)) as ::core::ffi::c_int,
            0 as ::core::ffi::c_int,
        );
        screen_write_puts(ctx, &gc, |out| out.write_all(b">"));
    } else {
        data.right = -(1 as ::core::ffi::c_int);
    }
    data.start = start;
    data.end = end;
    data.each = each;
    loop_0 = 0 as u_int;
    i = loop_0;
    wl = s
        .as_ref()
        .expect("live session")
        .with_winlinks(|links| winlinks_minmax(links, RB_NEGINF));
    while wl.is_alive() {
        if loop_0 == end {
            break;
        }
        if loop_0 < start {
            loop_0 = loop_0.wrapping_add(1);
        } else {
            let window = winlink_window(&wl).expect("preview window");
            let mut ft_owner = format_create(
                None,
                None,
                (FORMAT_WINDOW | window.id()) as ::core::ffi::c_int,
                0 as ::core::ffi::c_int,
            );
            let ft = &mut *ft_owner;
            format_defaults(ft, None, s.as_ref(), wl.clone(), None);
            window_tree_border_cell(
                &mut gc,
                &OptionsScope::Window(WindowRef::downgrade(&window)),
                Some(ft),
            );
            label_gc = grid_default_cell;
            style_apply_with_options(
                &mut label_gc,
                c"tree-mode-preview-style",
                Some(&mut *ft),
                &OptionsScope::Window(WindowRef::downgrade(&window)),
            );
            label_gc.bg = gc.bg;
            if left != 0 {
                offset = (3 as u_int).wrapping_add(i.wrapping_mul(each));
            } else {
                offset = i.wrapping_mul(each);
            }
            if loop_0 == end.wrapping_sub(1 as u_int) {
                width = each.wrapping_add(remaining);
            } else {
                width = each.wrapping_sub(1 as u_int);
            }
            screen_write_cursormove(
                ctx,
                cx.wrapping_add(offset) as ::core::ffi::c_int,
                cy as ::core::ffi::c_int,
                0 as ::core::ffi::c_int,
            );
            let preview_pane = window.active_pane().upgrade().expect("active preview pane");
            window_tree_preview(&preview_pane, &mut *ctx, width, sy);
            drop(preview_pane);
            let format =
                options_get_string(&window.borrow_options_mut(), c"tree-mode-preview-format");
            if !format.as_bytes().is_empty() {
                let label = format_expand_cstring(ft, &format);
                if !label.as_bytes().is_empty() {
                    window_tree_draw_label(
                        ctx,
                        cx.wrapping_add(offset),
                        cy,
                        width,
                        sy,
                        &gc,
                        &label_gc,
                        &label,
                    );
                }
            }
            drop(ft_owner);
            drop(window);
            if loop_0 != end.wrapping_sub(1 as u_int) {
                screen_write_cursormove(
                    ctx,
                    cx.wrapping_add(offset).wrapping_add(width) as ::core::ffi::c_int,
                    cy as ::core::ffi::c_int,
                    0 as ::core::ffi::c_int,
                );
                screen_write_vline(ctx, sy, Some(&gc));
            }
            loop_0 = loop_0.wrapping_add(1);
            i = i.wrapping_add(1);
        }
        wl = winlink_next(&wl);
    }
}
fn window_tree_draw_window(
    data: &mut window_tree_modedata,
    session_owner: &SessionRef,
    mut wl: refbox::Weak<winlink>,
    mut ctx: &mut screen_write_ctx,
    mut sx: u_int,
    mut sy: u_int,
) {
    let s = Some(session_owner.clone());
    let Some(mode_pane_owner) = Rc::<RefCell<window_pane>>::from_observer(&data.wp) else {
        return;
    };
    let window = winlink_window(&wl).expect("preview window");
    (|| {
        let mut cx: u_int = ctx.screen.cx;
        let mut cy: u_int = ctx.screen.cy;
        let mut loop_0: u_int = 0;
        let mut total: u_int = 0;
        let mut visible: u_int = 0;
        let mut each: u_int = 0;
        let mut width: u_int = 0;
        let mut offset: u_int = 0;
        let mut current: u_int = 0;
        let mut start: u_int = 0;
        let mut end: u_int = 0;
        let mut remaining: u_int = 0;
        let mut i: u_int = 0;
        let mut gc: grid_cell = grid_cell {
            data: utf8_data {
                data: [0; 32],
                have: 0,
                size: 0,
                width: 0,
            },
            attr: 0,
            flags: 0,
            fg: 0,
            bg: 0,
            us: 0,
            link: 0,
        };
        let mut label_gc: grid_cell = grid_cell {
            data: utf8_data {
                data: [0; 32],
                have: 0,
                size: 0,
                width: 0,
            },
            attr: 0,
            flags: 0,
            fg: 0,
            bg: 0,
            us: 0,
            link: 0,
        };
        let mut left: ::core::ffi::c_int = 0;
        let mut right: ::core::ffi::c_int = 0;
        total = window.pane_snapshot().len() as u_int;
        if data.hide_preview_this_pane != 0
            && mode_pane_owner
                .window_observer()
                .ptr_eq(&WindowRef::downgrade(&window))
        {
            total = total.wrapping_sub(1);
        }
        if total == 0 as u_int {
            return;
        }
        if sx.wrapping_div(total) < 24 as u_int {
            visible = sx.wrapping_div(24 as u_int);
            if visible == 0 as u_int {
                visible = 1 as u_int;
            }
        } else {
            visible = total;
        }
        current = 0 as u_int;
        for pane_owner in window
            .pane_snapshot()
            .iter()
            .filter_map(std::rc::Weak::upgrade)
        {
            if !(data.hide_preview_this_pane != 0 && Rc::ptr_eq(&pane_owner, &mode_pane_owner)) {
                if window.active_pane().ptr_eq(&Rc::downgrade(&pane_owner)) {
                    break;
                }
                current = current.wrapping_add(1);
            }
        }
        if current < visible {
            start = 0 as u_int;
            end = visible;
        } else if current >= total.wrapping_sub(visible) {
            start = total.wrapping_sub(visible);
            end = total;
        } else {
            start = current.wrapping_sub(visible.wrapping_div(2 as u_int));
            end = start.wrapping_add(visible);
        }
        if data.offset < -(start as ::core::ffi::c_int) {
            data.offset = -(start as ::core::ffi::c_int);
        }
        if data.offset > total.wrapping_sub(end) as ::core::ffi::c_int {
            data.offset = total.wrapping_sub(end) as ::core::ffi::c_int;
        }
        start = start.wrapping_add(data.offset as u_int);
        end = end.wrapping_add(data.offset as u_int);
        left = (start != 0 as u_int) as ::core::ffi::c_int;
        right = (end != total) as ::core::ffi::c_int;
        if left != 0 && right != 0 && sx <= 6 as u_int
            || (left != 0 || right != 0) && sx <= 3 as u_int
        {
            right = 0 as ::core::ffi::c_int;
            left = right;
        }
        if left != 0 && right != 0 {
            each = sx.wrapping_sub(6 as u_int).wrapping_div(visible);
            remaining = sx
                .wrapping_sub(6 as u_int)
                .wrapping_sub(visible.wrapping_mul(each));
        } else if left != 0 || right != 0 {
            each = sx.wrapping_sub(3 as u_int).wrapping_div(visible);
            remaining = sx
                .wrapping_sub(3 as u_int)
                .wrapping_sub(visible.wrapping_mul(each));
        } else {
            each = sx.wrapping_div(visible);
            remaining = sx.wrapping_sub(visible.wrapping_mul(each));
        }
        if each == 0 as u_int {
            return;
        }
        let border_window = mode_pane_owner.window_observer();
        window_tree_border_cell(&mut gc, &OptionsScope::Window(border_window.clone()), None);
        if left != 0 {
            data.left = cx.wrapping_add(2 as u_int) as ::core::ffi::c_int;
            screen_write_cursormove(
                ctx,
                cx.wrapping_add(2 as u_int) as ::core::ffi::c_int,
                cy as ::core::ffi::c_int,
                0 as ::core::ffi::c_int,
            );
            screen_write_vline(ctx, sy, Some(&gc));
            screen_write_cursormove(
                ctx,
                cx as ::core::ffi::c_int,
                cy.wrapping_add(sy.wrapping_div(2 as u_int)) as ::core::ffi::c_int,
                0 as ::core::ffi::c_int,
            );
            screen_write_puts(ctx, &gc, |out| out.write_all(b"<"));
        } else {
            data.left = -(1 as ::core::ffi::c_int);
        }
        if right != 0 {
            data.right = cx.wrapping_add(sx).wrapping_sub(3 as u_int) as ::core::ffi::c_int;
            screen_write_cursormove(
                ctx,
                cx.wrapping_add(sx).wrapping_sub(3 as u_int) as ::core::ffi::c_int,
                cy as ::core::ffi::c_int,
                0 as ::core::ffi::c_int,
            );
            screen_write_vline(ctx, sy, Some(&gc));
            screen_write_cursormove(
                ctx,
                cx.wrapping_add(sx).wrapping_sub(1 as u_int) as ::core::ffi::c_int,
                cy.wrapping_add(sy.wrapping_div(2 as u_int)) as ::core::ffi::c_int,
                0 as ::core::ffi::c_int,
            );
            screen_write_puts(ctx, &gc, |out| out.write_all(b">"));
        } else {
            data.right = -(1 as ::core::ffi::c_int);
        }
        data.start = start;
        data.end = end;
        data.each = each;
        loop_0 = 0 as u_int;
        i = loop_0;
        for pane_owner in window
            .pane_snapshot()
            .iter()
            .filter_map(std::rc::Weak::upgrade)
        {
            if !(data.hide_preview_this_pane != 0 && Rc::ptr_eq(&pane_owner, &mode_pane_owner)) {
                if loop_0 == end {
                    break;
                }
                if loop_0 < start {
                    loop_0 = loop_0.wrapping_add(1);
                } else {
                    let options_pane = Rc::downgrade(&pane_owner);
                    let mut ft_owner = format_create(
                        None,
                        None,
                        (FORMAT_PANE | pane_owner.id()) as ::core::ffi::c_int,
                        0 as ::core::ffi::c_int,
                    );
                    let ft = &mut *ft_owner;
                    format_defaults(ft, None, s.as_ref(), wl.clone(), Some(&pane_owner));
                    window_tree_border_cell(
                        &mut gc,
                        &OptionsScope::Pane(options_pane.clone()),
                        Some(ft),
                    );
                    label_gc = grid_default_cell;
                    style_apply_with_options(
                        &mut label_gc,
                        c"tree-mode-preview-style",
                        Some(&mut *ft),
                        &OptionsScope::Pane(options_pane.clone()),
                    );
                    label_gc.bg = gc.bg;
                    if left != 0 {
                        offset = (3 as u_int).wrapping_add(i.wrapping_mul(each));
                    } else {
                        offset = i.wrapping_mul(each);
                    }
                    if loop_0 == end.wrapping_sub(1 as u_int) {
                        width = each.wrapping_add(remaining);
                    } else {
                        width = each.wrapping_sub(1 as u_int);
                    }
                    screen_write_cursormove(
                        ctx,
                        cx.wrapping_add(offset) as ::core::ffi::c_int,
                        cy as ::core::ffi::c_int,
                        0 as ::core::ffi::c_int,
                    );
                    window_tree_preview(&pane_owner, &mut *ctx, width, sy);
                    let format = options_get_string(
                        &options_pane
                            .upgrade()
                            .expect("live preview pane")
                            .borrow_options_mut(),
                        c"tree-mode-preview-format",
                    );
                    if !format.as_bytes().is_empty() {
                        let label = format_expand_cstring(ft, &format);
                        if !label.as_bytes().is_empty() {
                            window_tree_draw_label(
                                ctx,
                                cx.wrapping_add(offset),
                                cy,
                                width,
                                sy,
                                &gc,
                                &label_gc,
                                &label,
                            );
                        }
                    }
                    drop(ft_owner);
                    if loop_0 != end.wrapping_sub(1 as u_int) {
                        screen_write_cursormove(
                            ctx,
                            cx.wrapping_add(offset).wrapping_add(width) as ::core::ffi::c_int,
                            cy as ::core::ffi::c_int,
                            0 as ::core::ffi::c_int,
                        );
                        screen_write_vline(ctx, sy, Some(&gc));
                    }
                    loop_0 = loop_0.wrapping_add(1);
                    i = i.wrapping_add(1);
                }
            }
        }
    })();
    drop(window);
}
fn window_tree_draw_info(
    data: &mut window_tree_modedata,
    item: &window_tree_itemdata,
    ctx: &mut screen_write_ctx,
    mut sx: u_int,
    mut sy: u_int,
) {
    let Some(mode_pane_owner) = Rc::<RefCell<window_pane>>::from_observer(&data.wp) else {
        return;
    };
    let mut sp: Option<SessionRef> = None;
    let mut wl: refbox::Weak<winlink> = refbox::Weak::new();
    let mut gc: grid_cell = grid_cell {
        data: utf8_data {
            data: [0; 32],
            have: 0,
            size: 0,
            width: 0,
        },
        attr: 0,
        flags: 0,
        fg: 0,
        bg: 0,
        us: 0,
        link: 0,
    };
    let mut cx: u_int = ctx.screen.cx;
    let mut cy: u_int = ctx.screen.cy;
    let mut i: u_int = 0;
    let mut j: u_int = 0;
    let mut k: u_int = 0;
    let mut lines: [&[&CStr]; 3] = [&[]; 3];
    let mut n: u_int = 0 as u_int;
    let _target_owners_1 = window_tree_pull_item(item);
    sp = _target_owners_1.session;
    wl = _target_owners_1.winlink.clone();
    if sp.is_none() || _target_owners_1.pane.is_none() {
        return;
    }
    if item.type_0 as ::core::ffi::c_uint
        == WINDOW_TREE_PANE as ::core::ffi::c_int as ::core::ffi::c_uint
    {
        lines[n as usize] = &window_tree_pane_info_lines;
        n = n.wrapping_add(1);
    }
    if item.type_0 as ::core::ffi::c_uint
        == WINDOW_TREE_PANE as ::core::ffi::c_int as ::core::ffi::c_uint
        || item.type_0 as ::core::ffi::c_uint
            == WINDOW_TREE_WINDOW as ::core::ffi::c_int as ::core::ffi::c_uint
    {
        lines[n as usize] = &window_tree_window_info_lines;
        n = n.wrapping_add(1);
    }
    lines[n as usize] = &window_tree_session_info_lines;
    n = n.wrapping_add(1);
    let mut ft_owner = format_create(None, None, FORMAT_NONE, 0 as ::core::ffi::c_int);
    let ft = &mut *ft_owner;
    format_defaults(
        ft,
        None,
        sp.as_ref(),
        wl.clone(),
        _target_owners_1.pane.as_ref(),
    );
    i = 0 as u_int;
    j = 0 as u_int;
    while j < n {
        if j != 0 as u_int {
            if i == sy {
                break;
            }
            let border_window = mode_pane_owner.window_observer();
            window_tree_border_cell(&mut gc, &OptionsScope::Window(border_window.clone()), None);
            screen_write_cursormove(
                ctx,
                cx as ::core::ffi::c_int,
                cy.wrapping_add(i) as ::core::ffi::c_int,
                0 as ::core::ffi::c_int,
            );
            screen_write_hline(
                ctx,
                sx,
                0 as ::core::ffi::c_int,
                0 as ::core::ffi::c_int,
                BOX_LINES_DEFAULT,
                Some(&gc),
            );
            if sx > 14 as u_int {
                gc.attr = (gc.attr as ::core::ffi::c_int | GRID_ATTR_CHARSET) as u_short;
                screen_write_cursormove(
                    ctx,
                    cx.wrapping_add(14 as u_int) as ::core::ffi::c_int,
                    cy.wrapping_add(i) as ::core::ffi::c_int,
                    0 as ::core::ffi::c_int,
                );
                screen_write_putc(ctx, &gc, 'n' as i32 as u_char);
            }
            i = i.wrapping_add(1);
        }
        k = 0 as u_int;
        while (k as usize) < lines[j as usize].len() {
            if i == sy {
                break;
            }
            let expanded = format_expand_cstring(ft, lines[j as usize][k as usize]);
            screen_write_cursormove(
                ctx,
                cx as ::core::ffi::c_int,
                cy.wrapping_add(i) as ::core::ffi::c_int,
                0 as ::core::ffi::c_int,
            );
            format_draw(
                ctx,
                &grid_default_cell,
                sx,
                &expanded,
                None,
                0 as ::core::ffi::c_int,
            );
            i = i.wrapping_add(1);
            k = k.wrapping_add(1);
        }
        if i == sy {
            break;
        }
        j = j.wrapping_add(1);
    }
    if sx > 14 as u_int && i < sy {
        let border_window = mode_pane_owner.window_observer();
        window_tree_border_cell(&mut gc, &OptionsScope::Window(border_window.clone()), None);
        screen_write_cursormove(
            ctx,
            cx.wrapping_add(14 as u_int) as ::core::ffi::c_int,
            cy.wrapping_add(i) as ::core::ffi::c_int,
            0 as ::core::ffi::c_int,
        );
        screen_write_vline(ctx, sy.wrapping_sub(i), Some(&gc));
    }
    drop(ft_owner);
}
fn window_tree_draw(
    mode: &TreeMode,
    item: &window_tree_itemdata,
    ctx: &mut screen_write_ctx,
    mut sx: u_int,
    mut sy: u_int,
) {
    let mut guard = mode.data.borrow_mut();
    let data = &mut *guard;
    let Some(mode_pane_owner) = Rc::<RefCell<window_pane>>::from_observer(&data.wp) else {
        return;
    };
    let mut _sp: Option<SessionRef> = None;
    let mut wl: refbox::Weak<winlink> = refbox::Weak::new();
    let target_owners = window_tree_pull_item(item);
    _sp = target_owners.session.clone();
    wl = target_owners.winlink.clone();
    if target_owners.pane.is_none() {
        return;
    }
    if data.preview_is_info != 0 {
        window_tree_draw_info(data, item, ctx, sx, sy);
        return;
    }
    match item.type_0 as ::core::ffi::c_uint {
        1 => {
            window_tree_draw_session(
                data,
                target_owners
                    .session
                    .as_ref()
                    .expect("resolved preview session"),
                ctx,
                sx,
                sy,
            );
        }
        2 => {
            window_tree_draw_window(
                data,
                target_owners
                    .session
                    .as_ref()
                    .expect("resolved preview session"),
                wl.clone(),
                ctx,
                sx,
                sy,
            );
        }
        3 if (data.hide_preview_this_pane == 0
            || target_owners
                .pane
                .as_ref()
                .is_some_and(|pane| !Rc::ptr_eq(pane, &mode_pane_owner))) =>
        {
            window_tree_preview(
                target_owners.pane.as_ref().expect("preview pane"),
                &mut *ctx,
                sx,
                sy,
            );
        }
        _ => {}
    };
}
fn window_tree_search(
    item: &window_tree_itemdata,
    search: &CStr,
    mut icase: ::core::ffi::c_int,
) -> ::core::ffi::c_int {
    let icase = icase != 0;
    let mut s: Option<SessionRef> = None;
    let mut wl: refbox::Weak<winlink> = refbox::Weak::new();
    let _target_owners_3 = window_tree_pull_item(item);
    s = _target_owners_3.session;
    wl = _target_owners_3.winlink.clone();
    match item.type_0 as ::core::ffi::c_uint {
        0 => return 0 as ::core::ffi::c_int,
        1 => {
            if s.is_none() {
                return 0 as ::core::ffi::c_int;
            }
            let name = s.as_ref().expect("live session").name();
            return contains(name.to_bytes(), search.to_bytes(), icase) as ::core::ffi::c_int;
        }
        2 => {
            if s.is_none() || !wl.is_alive() {
                return 0 as ::core::ffi::c_int;
            }
            let name = winlink_window(&wl).expect("live window").name();
            return contains(name.to_bytes(), search.to_bytes(), icase) as ::core::ffi::c_int;
        }
        3 if !(s.is_none() || !wl.is_alive() || _target_owners_3.pane.is_none()) => {
            let Some(cmd) = _target_owners_3
                .pane
                .as_ref()
                .expect("search pane")
                .process_name()
            else {
                return 0 as ::core::ffi::c_int;
            };
            return contains(cmd.to_bytes(), search.to_bytes(), icase) as ::core::ffi::c_int;
        }
        _ => {}
    }
    0 as ::core::ffi::c_int
}
fn window_tree_menu(mode: &TreeMode, c: &ClientRef, mut key: key_code) {
    let Some(mode_pane_owner) = window_tree_pane(mode) else {
        return;
    };
    let mut wme: refbox::Weak<window_mode_entry> = refbox::Weak::new();
    wme = mode_pane_owner.mode_entry();
    if !wme.is_alive()
        || !wme
            .try_borrow()
            .expect("live mode entry")
            .data_owner
            .as_ref()
            .is_some_and(|owner| std::ptr::addr_eq(Rc::as_ptr(owner), Rc::as_ptr(mode)))
    {
        return;
    }
    window_tree_key(wme.clone(), c, (refbox::Weak::new()).clone(), key, None);
}
fn window_tree_get_key(
    data: &window_tree_modedata,
    item: &window_tree_itemdata,
    mut line: u_int,
) -> key_code {
    let mut s: Option<SessionRef> = None;
    let mut wl: refbox::Weak<winlink> = refbox::Weak::new();
    let mut key: key_code = 0;
    let mut ft_owner = format_create(None, None, FORMAT_NONE, 0 as ::core::ffi::c_int);
    let ft = &mut *ft_owner;
    let _target_owners_4 = window_tree_pull_item(item);
    s = _target_owners_4.session;
    wl = _target_owners_4.winlink.clone();
    if item.type_0 as ::core::ffi::c_uint
        == WINDOW_TREE_SESSION as ::core::ffi::c_int as ::core::ffi::c_uint
    {
        format_defaults(ft, None, s.as_ref(), (refbox::Weak::new()).clone(), None);
    } else if item.type_0 as ::core::ffi::c_uint
        == WINDOW_TREE_WINDOW as ::core::ffi::c_int as ::core::ffi::c_uint
    {
        format_defaults(ft, None, s.as_ref(), wl.clone(), None);
    } else {
        format_defaults(
            ft,
            None,
            s.as_ref(),
            wl.clone(),
            _target_owners_4.pane.as_ref(),
        );
    }
    format_add(ft, c"line", |out| write!(out, "{}", { line }));
    let expanded = format_expand_cstring(ft, &data.key_format);
    key = key_string_parse_cstr(expanded.as_c_str()).unwrap_or(KEYC_UNKNOWN);
    drop(ft_owner);
    key
}
fn window_tree_swap(
    cur: &window_tree_itemdata,
    other: &window_tree_itemdata,
    sort_crit: &mut sort_criteria,
) -> ::core::ffi::c_int {
    let mut cur_session: Option<SessionRef> = None;
    let mut other_session: Option<SessionRef> = None;
    let mut cur_winlink: refbox::Weak<winlink> = refbox::Weak::new();
    let mut other_winlink: refbox::Weak<winlink> = refbox::Weak::new();
    if cur.type_0 as ::core::ffi::c_uint != other.type_0 as ::core::ffi::c_uint {
        return 0 as ::core::ffi::c_int;
    }
    if cur.type_0 as ::core::ffi::c_uint
        != WINDOW_TREE_WINDOW as ::core::ffi::c_int as ::core::ffi::c_uint
    {
        return 0 as ::core::ffi::c_int;
    }
    let _target_owners_5 = window_tree_pull_item(cur);
    cur_session = _target_owners_5.session;
    cur_winlink = _target_owners_5.winlink.clone();
    let _target_owners_6 = window_tree_pull_item(other);
    other_session = _target_owners_6.session;
    other_winlink = _target_owners_6.winlink.clone();
    if cur_session.is_none() || !cur_winlink.is_alive() {
        return 0 as ::core::ffi::c_int;
    }
    if other_session.is_none() || !other_winlink.is_alive() {
        return 0 as ::core::ffi::c_int;
    }
    if !crate::shared::rc::same(cur_session.as_ref(), other_session.as_ref()) {
        return 0 as ::core::ffi::c_int;
    }
    if sort_would_window_tree_swap(sort_crit, (cur_winlink).clone(), (other_winlink).clone()) != 0 {
        return 0 as ::core::ffi::c_int;
    }
    winlink_window(&other_winlink)
        .expect("linked window")
        .remove_winlink(other_winlink.clone());
    winlink_window(&cur_winlink)
        .expect("linked window")
        .remove_winlink(cur_winlink.clone());
    if other_winlink != cur_winlink {
        std::mem::swap(
            &mut other_winlink
                .try_borrow_mut()
                .expect("live winlink")
                .window_owner,
            &mut cur_winlink
                .try_borrow_mut()
                .expect("live winlink")
                .window_owner,
        );
    }
    winlink_window(&other_winlink)
        .expect("linked window")
        .add_winlink(other_winlink.clone());
    winlink_window(&cur_winlink)
        .expect("linked window")
        .add_winlink(cur_winlink.clone());
    if cur_session
        .as_ref()
        .expect("live session")
        .current_winlink()
        == cur_winlink
    {
        (cur_session.as_ref().expect("live session")).select_winlink((other_winlink).clone());
    } else if cur_session
        .as_ref()
        .expect("live session")
        .current_winlink()
        == other_winlink
    {
        (cur_session.as_ref().expect("live session")).select_winlink((cur_winlink).clone());
    }
    session_group_synchronize_from(cur_session.as_ref().expect("live session"));
    server_redraw_session_group(cur_session.as_ref().expect("live session"));
    recalculate_sizes();
    1 as ::core::ffi::c_int
}
fn window_tree_sort(sort_crit: &mut sort_criteria) {
    {
        sort_crit.order_seq = &window_tree_order_seq;
        if sort_crit.order as ::core::ffi::c_uint
            == SORT_END as ::core::ffi::c_int as ::core::ffi::c_uint
        {
            sort_crit.order = sort_crit.order_seq[0];
        }
    }
}
static window_tree_help_lines: &[&CStr] = &[
    c"#[fg=themelightgrey]      Enter #[#{E:tree-mode-border-style},acs]x#[default] Choose selected item",
    c"#[fg=themelightgrey]       S-Up #[#{E:tree-mode-border-style},acs]x#[default] Swap current and previous window",
    c"#[fg=themelightgrey]     S-Down #[#{E:tree-mode-border-style},acs]x#[default] Swap current and next window",
    c"#[fg=themelightgrey]          x #[#{E:tree-mode-border-style},acs]x#[default] Kill selected item",
    c"#[fg=themelightgrey]          X #[#{E:tree-mode-border-style},acs]x#[default] Kill tagged items",
    c"#[fg=themelightgrey]          < #[#{E:tree-mode-border-style},acs]x#[default] Scroll previews left",
    c"#[fg=themelightgrey]          > #[#{E:tree-mode-border-style},acs]x#[default] Scroll previews right",
    c"#[fg=themelightgrey]          m #[#{E:tree-mode-border-style},acs]x#[default] Set the marked pane",
    c"#[fg=themelightgrey]          M #[#{E:tree-mode-border-style},acs]x#[default] Clear the marked pane",
    c"#[fg=themelightgrey]          i #[#{E:tree-mode-border-style},acs]x#[default] Toggle session, window and pane information",
    c"#[fg=themelightgrey]          : #[#{E:tree-mode-border-style},acs]x#[default] Run a command for each tagged item",
    c"#[fg=themelightgrey]          f #[#{E:tree-mode-border-style},acs]x#[default] Enter a format",
    c"#[fg=themelightgrey]          H #[#{E:tree-mode-border-style},acs]x#[default] Jump to the starting pane",
];
fn window_tree_help() -> mode_tree_help_info {
    mode_tree_help_info {
        width: 51 as u_int,
        item: c"item",
        lines: window_tree_help_lines,
    }
}
fn window_tree_init(
    mut wme: refbox::Weak<window_mode_entry>,
    _item_handle: Option<&std::rc::Rc<std::cell::RefCell<cmdq_item>>>,
    fs: Option<&cmd_find_state>,
    args: Option<&args>,
) -> bool {
    let mode_pane_owner = wme
        .try_borrow()
        .expect("live mode entry")
        .wp
        .upgrade()
        .expect("mode belongs to a live pane");
    let format =
        if let Some(args) = args.filter(|args| args_has(args, 'F' as i32 as u_char) != 0) {
            args_get(args, 'F' as i32 as u_char).expect("-F takes a value")
        } else {
            WINDOW_TREE_DEFAULT_FORMAT
        }
        .to_owned();
    let key_format =
        if let Some(args) = args.filter(|args| args_has(args, 'K' as i32 as u_char) != 0) {
            args_get(args, 'K' as i32 as u_char).expect("-K takes a value")
        } else {
            WINDOW_TREE_DEFAULT_KEY_FORMAT
        }
        .to_owned();
    let command = if let Some(args) = args.filter(|args| args_count(args) != 0 as u_int) {
        args_string(args, 0 as u_int).expect("argument 0 exists")
    } else {
        WINDOW_TREE_DEFAULT_COMMAND
    }
    .to_owned();
    let args = args.expect("mode arguments");
    let owner = Rc::new(ModeCell::new(window_tree_modedata {
        wp: std::rc::Rc::downgrade(&mode_pane_owner),
        data: None,
        format,
        key_format,
        command,
        squash_groups: (args_has(args, 'G' as i32 as u_char) == 0) as ::core::ffi::c_int,
        hide_preview_this_pane: args_has(args, 'h' as i32 as u_char),
        preview_is_info: 0,
        prompt_flags: if args_has(args, 'y' as i32 as u_char) != 0 {
            PROMPT_ACCEPT
        } else {
            0
        },
        item_list: Vec::new(),
        entered: None,
        fs: fs.expect("mode target").clone(),
        type_0: if args_has(args, 's' as i32 as u_char) != 0 {
            WINDOW_TREE_SESSION
        } else if args_has(args, 'w' as i32 as u_char) != 0 {
            WINDOW_TREE_WINDOW
        } else {
            WINDOW_TREE_PANE
        },
        offset: 0,
        left: 0,
        right: 0,
        start: 0,
        end: 0,
        each: 0,
    }));
    let mode = owner.clone();
    let build_mode = Rc::downgrade(&owner);
    wme.try_borrow_mut().expect("live mode entry").data_owner = Some(owner);
    let draw_mode = build_mode.clone();
    let menu_mode = build_mode.clone();
    let key_mode = build_mode.clone();
    let tree_owner = mode_tree_start(
        &mode_pane_owner,
        args,
        Some(Box::new(move |sort, tag, filter| {
            let Some(mode) = window_tree_live_mode(&build_mode) else {
                return tag;
            };
            let mut selected = tag.unwrap_or(::core::primitive::u64::MAX as uint64_t);
            window_tree_build(&mode, sort, &mut selected, filter);
            (selected != ::core::primitive::u64::MAX as uint64_t).then_some(selected)
        })),
        Some(Box::new(move |itemdata, ctx, sx, sy| {
            let Some(mode) = window_tree_live_mode(&draw_mode) else {
                return;
            };
            let item = itemdata.as_tree().expect("tree row payload");
            window_tree_draw(&mode, &item, ctx, sx, sy)
        })),
        Some(Box::new(move |itemdata, search, icase| {
            let item = itemdata.as_tree().expect("tree row payload");
            window_tree_search(&item, search, icase as ::core::ffi::c_int) != 0
        })),
        Some(Box::new(move |client, key| {
            let Some(mode) = window_tree_live_mode(&menu_mode) else {
                return;
            };
            window_tree_menu(&mode, client, key)
        })),
        None,
        Some(Box::new(move |itemdata, line| {
            let Some(mode) = window_tree_live_mode(&key_mode) else {
                return KEYC_NONE;
            };
            let item = itemdata.as_tree().expect("tree row payload");
            let key = window_tree_get_key(&mode.data.borrow(), &item, line);
            key
        })),
        Some(Box::new(move |current, other, sort| {
            window_tree_swap(
                &current.as_tree().expect("tree row payload"),
                &other.as_tree().expect("tree row payload"),
                sort,
            ) != 0
        })),
        Some(window_tree_sort),
        Some(window_tree_help),
        &window_tree_menu_items,
    );
    mode.data.borrow_mut().data = Some(tree_owner.clone());
    tree_owner.borrow_mut().set_view_name(Some(c"preview"));
    mode_tree_build(&tree_owner);
    mode_tree_draw(&tree_owner);
    mode.data.borrow_mut().type_0 = WINDOW_TREE_NONE;
    true
}
fn window_tree_get_screen(
    wme: refbox::Weak<window_mode_entry>,
    visit: &mut dyn FnMut(&mut screen),
) {
    let Some(mode) = wme
        .try_borrow()
        .expect("live mode entry")
        .retained_data::<ModeCell<window_tree_modedata>>()
    else {
        return;
    };
    let tree_owner = unsafe { window_mode_screen_payload(&mode.data, |data| data.data.clone()) };
    if let Some(tree_owner) = tree_owner {
        unsafe { mode_tree_visit_screen(&tree_owner, visit) };
    }
}

fn window_tree_free(wme: refbox::Weak<window_mode_entry>) {
    let Some(mode_owner) = wme
        .try_borrow()
        .expect("live mode entry")
        .retained_data::<ModeCell<window_tree_modedata>>()
    else {
        return;
    };
    mode_owner.dead.set(true);
    let tree_owner = mode_owner.data.borrow_mut().data.take();
    mode_tree_free(tree_owner.expect("mode tree owner"));
}
fn window_tree_resize(mut wme: refbox::Weak<window_mode_entry>, mut sx: u_int, mut sy: u_int) {
    let Some(mode_owner) = wme
        .try_borrow()
        .expect("live mode entry")
        .retained_data::<ModeCell<window_tree_modedata>>()
    else {
        return;
    };
    if mode_owner.dead.get() {
        return;
    }
    mode_tree_resize(&window_tree_tree(&mode_owner), sx, sy);
}
fn window_tree_update(mut wme: refbox::Weak<window_mode_entry>) {
    let Some(mode_owner) = wme
        .try_borrow()
        .expect("live mode entry")
        .retained_data::<ModeCell<window_tree_modedata>>()
    else {
        return;
    };
    if mode_owner.dead.get() {
        return;
    }
    let Some(mode_pane_owner) = window_tree_pane(&mode_owner) else {
        return;
    };
    let tree_owner = window_tree_tree(&mode_owner);
    mode_tree_build(&tree_owner);
    if mode_owner.dead.get() {
        return;
    }
    mode_tree_draw(&tree_owner);
    mode_pane_owner.request_redraw(false);
}
fn window_tree_get_target(item: &window_tree_itemdata, fs: &mut cmd_find_state) -> Option<CString> {
    let resolved = window_tree_pull_item(item);
    let link_index = match resolved.winlink.try_borrow() {
        Ok(link) => Some(link.idx),
        Err(refbox::BorrowError::Dropped) => None,
        Err(refbox::BorrowError::Borrowed) => panic!("tree target winlink already borrowed"),
    };
    let target = resolved.session.as_ref().and_then(|session_owner| {
        let mut bytes = Vec::from(b"=".as_slice());
        bytes.extend_from_slice(session_owner.name().as_bytes());
        bytes.push(b':');
        match item.type_0 {
            WINDOW_TREE_SESSION => {}
            WINDOW_TREE_WINDOW => {
                bytes.extend_from_slice(link_index?.to_string().as_bytes());
                bytes.push(b'.');
            }
            WINDOW_TREE_PANE => {
                let pane_owner = resolved.pane.as_ref()?;
                bytes.extend_from_slice(link_index?.to_string().as_bytes());
                bytes.extend_from_slice(b".%");
                bytes.extend_from_slice(pane_owner.id().to_string().as_bytes());
            }
            _ => return None,
        }
        Some(CString::new(bytes).expect("tree target contains no NUL"))
    });
    if target.is_none() {
        *fs = cmd_find_state::clear_state(0);
    } else {
        *fs = cmd_find_state::find_from_winlink_pane(
            (resolved.winlink.clone()).clone(),
            resolved.pane.as_ref().expect("target pane"),
            0,
        );
    }
    target
}
fn window_tree_command_each(
    mode: &TreeMode,
    item: &window_tree_itemdata,
    client_owner: Option<&ClientRef>,
) {
    let mut fs: cmd_find_state = cmd_find_state {
        flags: 0,
        s: Default::default(),
        wl: Default::default(),
        w: Default::default(),
        wp: Default::default(),
        idx: 0,
    };
    if let Some(name) = window_tree_get_target(item, &mut fs) {
        let entered = mode.data.borrow().entered.clone();
        mode_tree_run_command(
            client_owner,
            Some(&fs),
            entered.as_deref().expect("entered tree command"),
            &name,
        );
    }
}
fn window_tree_command_done(mode: TreeMode) -> cmdq_cb {
    Some(Box::new(move |_| {
        if !mode.dead.get() {
            let Some(mode_pane_owner) = window_tree_pane(&mode) else {
                return CmdOutcome::Normal;
            };
            let tree_owner = window_tree_tree(&mode);
            mode_tree_build(&tree_owner);
            mode_tree_draw(&tree_owner);
            mode_pane_owner.request_redraw(false);
        }
        CmdOutcome::Normal
    }))
}
fn window_tree_enqueue_command_done(client_owner: Option<&ClientRef>, mode: &TreeMode) {
    let item_allocation = cmdq_get_callback_owned(
        c"window_tree_command_done",
        window_tree_command_done(mode.clone()),
    );
    cmdq_append(client_owner, item_allocation);
}
/// Prompt input handler that receives the live tree mode owner.
type window_tree_prompt_fn =
    unsafe fn(Option<&ClientRef>, &TreeMode, Option<&CStr>, prompt_key_result) -> prompt_result;

fn window_tree_prompt_callbacks(
    mode: TreeMode,
    callback: window_tree_prompt_fn,
) -> (
    crate::mode_tree::mode_tree_prompt_input_cb,
    crate::shared::prompt::prompt_free_cb,
) {
    let observer = Rc::downgrade(&mode);
    let input: crate::mode_tree::mode_tree_prompt_input_cb =
        Some(Box::new(move |client, text, key| unsafe {
            let Some(owner) = window_tree_live_mode(&observer) else {
                return PROMPT_CLOSE;
            };
            callback(client, &owner, text, key)
        }));
    (input, Some(Box::new(move || drop(mode))))
}

fn window_tree_command_callback(
    client_owner: Option<&ClientRef>,
    mode_owner: &TreeMode,
    s: Option<&CStr>,
    _key: prompt_key_result,
) -> prompt_result {
    let Some(s) = s.filter(|text| !text.to_bytes().is_empty()) else {
        return PROMPT_CLOSE;
    };
    if mode_owner.dead.get() {
        return PROMPT_CLOSE;
    }
    mode_owner.data.borrow_mut().entered = Some(s.to_owned());
    mode_tree_each_tagged(
        &window_tree_tree(mode_owner),
        |row, _| {
            let itemdata = row.borrow().itemdata.clone();
            window_tree_command_each(
                mode_owner,
                &itemdata.as_tree().expect("tree row payload"),
                client_owner,
            )
        },
        KEYC_NONE as ::core::ffi::c_ulong as key_code,
        1 as ::core::ffi::c_int,
    );
    mode_owner.data.borrow_mut().entered = None;
    window_tree_enqueue_command_done(client_owner, mode_owner);
    PROMPT_CLOSE
}
fn window_tree_kill_each(item: &window_tree_itemdata) {
    let target = window_tree_pull_item(item);
    match item.type_0 {
        WINDOW_TREE_SESSION => {
            if let Some(session_owner) = target.session.as_ref() {
                server_destroy_session(session_owner);
                session_owner.destroy((1) != 0, c"window_tree_kill_each");
            }
        }
        WINDOW_TREE_WINDOW => {
            let window_owner = match target.winlink.try_borrow() {
                Ok(link) => link.window_owner.clone(),
                Err(refbox::BorrowError::Dropped) => None,
                Err(refbox::BorrowError::Borrowed) => {
                    panic!("tree target winlink already borrowed")
                }
            };
            if let Some(window_owner) = window_owner {
                server_kill_window(window_owner, 0);
            }
        }
        WINDOW_TREE_PANE => {
            if let Some(pane_owner) = target.pane.as_ref() {
                server_kill_pane(pane_owner);
            }
        }
        _ => {}
    }
}
fn window_tree_kill_current_callback(
    client_owner: Option<&ClientRef>,
    mode_owner: &TreeMode,
    s: Option<&CStr>,
    _key: prompt_key_result,
) -> prompt_result {
    let Some(s) = s.filter(|text| !text.to_bytes().is_empty()) else {
        return PROMPT_CLOSE;
    };
    if mode_owner.dead.get() {
        return PROMPT_CLOSE;
    }
    if !s.to_bytes().eq_ignore_ascii_case(b"y") {
        return PROMPT_CLOSE;
    }
    let tree_owner = window_tree_tree(mode_owner);
    let item_owner = tree_owner.borrow().get_current();
    if let Some(item) = item_owner.as_tree() {
        window_tree_kill_each(&item);
    }
    server_renumber_all();
    window_tree_enqueue_command_done(client_owner, mode_owner);
    PROMPT_CLOSE
}
fn window_tree_kill_tagged_callback(
    client_owner: Option<&ClientRef>,
    mode_owner: &TreeMode,
    s: Option<&CStr>,
    _key: prompt_key_result,
) -> prompt_result {
    let Some(s) = s.filter(|text| !text.to_bytes().is_empty()) else {
        return PROMPT_CLOSE;
    };
    if mode_owner.dead.get() {
        return PROMPT_CLOSE;
    }
    if !s.to_bytes().eq_ignore_ascii_case(b"y") {
        return PROMPT_CLOSE;
    }
    mode_tree_each_tagged(
        &window_tree_tree(mode_owner),
        |row, _| {
            let itemdata = row.borrow().itemdata.clone();
            window_tree_kill_each(&itemdata.as_tree().expect("tree row payload"))
        },
        KEYC_NONE as ::core::ffi::c_ulong as key_code,
        1 as ::core::ffi::c_int,
    );
    server_renumber_all();
    window_tree_enqueue_command_done(client_owner, mode_owner);
    PROMPT_CLOSE
}
fn window_tree_mouse(
    mode_owner: &TreeMode,
    mut key: key_code,
    mut x: u_int,
    item: &window_tree_itemdata,
) -> key_code {
    if key != KEYC_MOUSEDOWN1_PANE as ::core::ffi::c_ulong as key_code {
        return KEYC_NONE as ::core::ffi::c_ulong as key_code;
    }
    let (left, right, start, end, each) = {
        let data = mode_owner.data.borrow();
        (data.left, data.right, data.start, data.end, data.each)
    };
    if left != -(1 as ::core::ffi::c_int) && x <= left as u_int {
        return '<' as i32 as key_code;
    }
    if right != -(1 as ::core::ffi::c_int) && x >= right as u_int {
        return '>' as i32 as key_code;
    }
    if left != -(1 as ::core::ffi::c_int) {
        x = x.wrapping_sub(left as u_int);
    } else if x != 0 as u_int {
        x = x.wrapping_sub(1);
    }
    if x == 0 as u_int || end == 0 as u_int {
        x = 0 as u_int;
    } else {
        x = x.wrapping_div(each);
        if start.wrapping_add(x) >= end {
            x = end.wrapping_sub(1 as u_int);
        }
    }
    let target = window_tree_pull_item(item);
    if item.type_0 as ::core::ffi::c_uint
        == WINDOW_TREE_SESSION as ::core::ffi::c_int as ::core::ffi::c_uint
    {
        let Some(session_owner) = target.session.as_ref() else {
            return KEYC_NONE;
        };
        mode_tree_expand_current(&window_tree_tree(mode_owner));
        if mode_owner.dead.get() {
            return KEYC_NONE;
        }
        let mut loop_0 = 0 as u_int;
        let mut wl = session_owner.with_winlinks(|links| winlinks_minmax(links, RB_NEGINF));
        while wl.is_alive() {
            if loop_0 == mode_owner.data.borrow().start.wrapping_add(x) {
                break;
            }
            loop_0 = loop_0.wrapping_add(1);
            wl = winlink_next(&wl);
        }
        if wl.is_alive() {
            window_tree_tree(mode_owner)
                .borrow_mut()
                .set_current(wl.as_ptr() as uint64_t);
        }
        return '\r' as i32 as key_code;
    }
    if item.type_0 as ::core::ffi::c_uint
        == WINDOW_TREE_WINDOW as ::core::ffi::c_int as ::core::ffi::c_uint
    {
        if !target.winlink.is_alive() {
            return KEYC_NONE;
        }
        mode_tree_expand_current(&window_tree_tree(mode_owner));
        if mode_owner.dead.get() || !target.winlink.is_alive() {
            return KEYC_NONE;
        }
        let wl = target.winlink.clone();
        let window = winlink_window(&wl).expect("tree target window").clone();
        let pane = window
            .pane_snapshot()
            .get(mode_owner.data.borrow().start.wrapping_add(x) as usize)
            .and_then(std::rc::Weak::upgrade);
        if let Some(pane_owner) = pane {
            window_tree_tree(mode_owner)
                .borrow_mut()
                .set_current(std::rc::Rc::as_ptr(&pane_owner) as uint64_t);
        }
        drop(window);
        return '\r' as i32 as key_code;
    }
    KEYC_NONE as ::core::ffi::c_ulong as key_code
}
fn window_tree_key(
    mut wme: refbox::Weak<window_mode_entry>,
    client_owner: &ClientRef,
    _wl: refbox::Weak<winlink>,
    mut key: key_code,
    mut m: Option<&mut mouse_event>,
) {
    let mode_pane_owner = wme
        .try_borrow()
        .expect("live mode entry")
        .wp
        .upgrade()
        .expect("mode belongs to a live pane");
    let mode_owner = wme
        .try_borrow()
        .expect("live mode entry")
        .retained_data::<ModeCell<window_tree_modedata>>()
        .expect("live mode payload");
    let (tree_owner, prompt_flags) = {
        let data = mode_owner.data.borrow();
        (data.tree_owner(), data.prompt_flags)
    };
    let mut fs: cmd_find_state = cmd_find_state {
        flags: 0,
        s: Default::default(),
        wl: Default::default(),
        w: Default::default(),
        wp: Default::default(),
        idx: 0,
    };
    let mut finished: ::core::ffi::c_int = 0;
    let mut tagged: u_int = 0;
    let mut x: u_int = 0;
    let mut y: u_int = 0;
    let mut idx: u_int = 0;
    let mut ns: Option<SessionRef> = None;
    let mut nwl: refbox::Weak<winlink> = refbox::Weak::new();
    let mut selection = tree_owner.borrow().get_current();
    finished = mode_tree_key(
        tree_owner.clone(),
        Some(client_owner),
        &mut key,
        m.as_deref(),
        Some(&mut x),
        Some(&mut y),
    );
    if mode_owner.dead.get() {
        return;
    }
    loop {
        let next_selection = tree_owner.borrow().get_current();
        let same_item = selection.same_identity(&next_selection);
        selection = next_selection;
        let item = selection.as_tree();
        if !same_item {
            mode_owner.data.borrow_mut().offset = 0;
        }
        if !((key as ::core::ffi::c_ulonglong & KEYC_MASK_KEY
            == KEYC_MOUSE as ::core::ffi::c_ulong as ::core::ffi::c_ulonglong
            || key as ::core::ffi::c_ulonglong & KEYC_MASK_TYPE
                >= (KEYC_TYPE_MOUSEMOVE as ::core::ffi::c_int as ::core::ffi::c_ulonglong)
                    << 32 as ::core::ffi::c_int
                && key as ::core::ffi::c_ulonglong & KEYC_MASK_TYPE
                    <= (KEYC_TYPE_TRIPLECLICK as ::core::ffi::c_int as ::core::ffi::c_ulonglong)
                        << 32 as ::core::ffi::c_int)
            && m.is_some())
        {
            break;
        }
        key = item.as_deref().map_or(KEYC_NONE, |item| {
            window_tree_mouse(&mode_owner, key, x, item)
        });
        if mode_owner.dead.get() {
            return;
        }
    }
    let item = selection.as_tree();
    match key {
        60 => {
            mode_owner.data.borrow_mut().offset -= 1;
        }
        62 => {
            mode_owner.data.borrow_mut().offset += 1;
        }
        72 => {
            let target = mode_owner.data.borrow().fs.clone();
            mode_tree_expand(
                &tree_owner,
                target
                    .session_handle()
                    .as_ref()
                    .map_or(0, |owner| std::rc::Rc::as_ptr(owner) as uint64_t),
            );
            mode_tree_expand(&tree_owner, target.winlink_handle().as_ptr() as uint64_t);
            if tree_owner
                .borrow_mut()
                .set_current(Rc::as_ptr(&mode_pane_owner) as uint64_t)
                == 0
            {
                tree_owner
                    .borrow_mut()
                    .set_current(target.winlink_handle().as_ptr() as uint64_t);
            }
        }
        109 if item.is_some() => {
            let item = item.as_deref().unwrap();
            let _target_owners_10 = window_tree_pull_item(item);
            ns = _target_owners_10.session;
            nwl = _target_owners_10.winlink.clone();
            server_set_marked(ns.as_ref(), (nwl).clone(), _target_owners_10.pane.as_ref());
            mode_tree_build(&tree_owner);
        }
        77 => {
            server_clear_marked();
            mode_tree_build(&tree_owner);
        }
        105 => {
            let info = {
                let mut data = mode_owner.data.borrow_mut();
                data.preview_is_info = (data.preview_is_info == 0) as ::core::ffi::c_int;
                data.preview_is_info != 0
            };
            if info {
                tree_owner.borrow_mut().set_view_name(Some(c"info"));
            } else {
                tree_owner.borrow_mut().set_view_name(Some(c"preview"));
            }
        }
        120 if item.is_some() => {
            let item = item.as_deref().unwrap();
            let _target_owners_11 = window_tree_pull_item(item);
            ns = _target_owners_11.session;
            nwl = _target_owners_11.winlink.clone();
            let prompt = match item.type_0 as ::core::ffi::c_uint {
                1 => {
                    if let Some(ns_value) = ns.as_ref() {
                        let mut bytes = b"Kill session ".to_vec();
                        bytes.extend_from_slice(ns_value.name().as_bytes());
                        bytes.extend_from_slice(b"? ");
                        Some(CString::new(bytes).expect("session name contains no NUL"))
                    } else {
                        None
                    }
                }
                2 => {
                    if nwl.is_alive() {
                        Some(
                            CString::new(format!(
                                "Kill window {}? ",
                                nwl.try_borrow().expect("live winlink").idx as u32
                            ))
                            .expect("window index contains no NUL"),
                        )
                    } else {
                        None
                    }
                }
                3 => {
                    if !(_target_owners_11.pane.is_none()
                        || !_target_owners_11
                            .pane
                            .as_ref()
                            .and_then(|pane| window_tree_pane_index(pane))
                            .map(|value| {
                                idx = value;
                            })
                            .is_some())
                    {
                        Some(CString::new(format!("Kill pane {idx}? ")).unwrap())
                    } else {
                        None
                    }
                }
                _ => None,
            };
            if let Some(prompt) = prompt {
                let mode = mode_owner.clone();
                let (inputcb, freecb) =
                    window_tree_prompt_callbacks(mode, window_tree_kill_current_callback);
                mode_tree_set_prompt(
                    tree_owner.clone(),
                    Some(client_owner),
                    &prompt,
                    Some(c""),
                    PROMPT_TYPE_COMMAND,
                    PROMPT_SINGLE | PROMPT_NOFORMAT | prompt_flags,
                    inputcb,
                    freecb,
                );
            }
        }
        88 => {
            tagged = tree_owner.borrow().count_tagged();
            if !(tagged == 0 as u_int) {
                let prompt = CString::new(format!("Kill {tagged} tagged? ")).unwrap();
                let mode = mode_owner.clone();
                let (inputcb, freecb) =
                    window_tree_prompt_callbacks(mode, window_tree_kill_tagged_callback);
                mode_tree_set_prompt(
                    tree_owner.clone(),
                    Some(client_owner),
                    &prompt,
                    Some(c""),
                    PROMPT_TYPE_COMMAND,
                    PROMPT_SINGLE | PROMPT_NOFORMAT | prompt_flags,
                    inputcb,
                    freecb,
                );
            }
        }
        58 => {
            tagged = tree_owner.borrow().count_tagged();
            let prompt = if tagged != 0 as u_int {
                CString::new(format!("({tagged} tagged) ")).unwrap()
            } else {
                CString::new("(current) ").unwrap()
            };
            let mode = mode_owner.clone();
            let (inputcb, freecb) =
                window_tree_prompt_callbacks(mode, window_tree_command_callback);
            mode_tree_set_prompt(
                tree_owner.clone(),
                Some(client_owner),
                &prompt,
                Some(c""),
                PROMPT_TYPE_COMMAND,
                PROMPT_NOFORMAT,
                inputcb,
                freecb,
            );
        }
        13 => {
            if let Some(name) = item
                .as_deref()
                .and_then(|item| window_tree_get_target(item, &mut fs))
            {
                let command = mode_owner.data.borrow().command.clone();
                mode_tree_run_command(Some(client_owner), None, &command, &name);
            }
            finished = 1 as ::core::ffi::c_int;
        }
        _ => {}
    }
    if finished != 0 {
        mode_pane_owner.reset_mode();
    } else {
        mode_tree_draw(&tree_owner);
        mode_pane_owner.request_redraw(false);
    };
}
