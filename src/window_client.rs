use crate::arguments::{args, args_count, args_get, args_has, args_string};
use crate::compat::imsg::*;
use crate::format::{
    format_add, format_create, format_create_defaults, format_defaults, format_expand_cstring,
    format_single_cstring, format_true,
};
use crate::format_draw::format_draw;
use crate::grid::grid_default_cell;
use crate::key_string::key_string_parse_cstr;
use crate::mode_tree::ModeTreeItemSnapshot;
use crate::mode_tree::{
    mode_tree_build, mode_tree_draw, mode_tree_each_tagged, mode_tree_free, mode_tree_key,
    mode_tree_resize, mode_tree_run_command, mode_tree_start, mode_tree_visit_screen,
};
use crate::mode_tree::{mode_tree_data, mode_tree_help_info, ModeTreeItemData};
use crate::options::OptionsScope;
use crate::screen_write::{
    screen_write_cursormove, screen_write_fast_copy, screen_write_hline, screen_write_preview,
    screen_write_vline,
};
use crate::server_client::Client as _;
use crate::session::Session;
use crate::shared::abi::*;
use crate::shared::client::ClientRef;
use crate::shared::client::CLIENT_UNATTACHEDFLAGS;
use crate::shared::command::{cmd_find_state, cmdq_item};
use crate::shared::format::FORMAT_NONE;
use crate::shared::grid::*;
use crate::shared::key::*;
use crate::shared::layout::*;
use crate::shared::menu::menu_item;
use crate::shared::mouse::mouse_event;
use crate::shared::pane::window_pane;
use crate::shared::screen::screen;
use crate::shared::screen_write::screen_write_ctx;
use crate::shared::session::SessionRef;
use crate::shared::sort::sort_criteria;
use crate::shared::sort::*;
use crate::shared::window::WindowRef;
use crate::shared::window::{
    window_mode, window_mode_entry, window_mode_screen_payload, winlink, ModeDataRef,
};
use crate::sort::sort_get_clients;
use crate::status::{status_at_line, status_line_size};
use crate::style::style_apply_with_options;
use crate::window::Window as _;
use crate::window_pane::WindowPane as _;
use crate::winlink::winlink_window;
use std::cell::RefCell;
use std::ffi::{CStr, CString};
use std::rc::{Rc, Weak};

#[repr(C)]
pub struct window_client_modedata {
    pub wp: Weak<RefCell<window_pane>>,
    pub data: Option<std::rc::Rc<std::cell::RefCell<mode_tree_data>>>,
    pub format: CString,
    pub key_format: CString,
    pub command: CString,
    pub hide_preview_this_pane: ::core::ffi::c_int,
    pub preview_is_info: ::core::ffi::c_int,
    pub items: Vec<refbox::RefBox<window_client_itemdata>>,
}

impl window_client_modedata {
    fn tree_owner(&self) -> Rc<std::cell::RefCell<mode_tree_data>> {
        self.data.as_ref().expect("mode tree owner").clone()
    }
}

#[derive(Clone)]
pub struct window_client_itemdata {
    c: Option<ClientRef>,
    pub ttyname: CString,
}

impl window_client_itemdata {
    fn new(c: &ClientRef, ttyname: &CStr) -> Self {
        Self {
            c: Some(Rc::clone(c)),
            ttyname: ttyname.to_owned(),
        }
    }

    fn client(&self) -> &ClientRef {
        self.c.as_ref().expect("live client row retains its client")
    }
}

pub const WINDOW_CLIENT_DEFAULT_COMMAND: &std::ffi::CStr = c"detach-client -t '%%'";
pub const WINDOW_CLIENT_DEFAULT_FORMAT: &CStr =
    c"#[fg=themelightgrey]#{t/p:client_activity}: session #[default]#{session_name}";
pub const WINDOW_CLIENT_DEFAULT_KEY_FORMAT: &std::ffi::CStr =
    c"#{?#{e|<:#{line},10},#{line},#{e|<:#{line},36},M-#{a:#{e|+:97,#{e|-:#{line},10}}}}";
const window_client_info_lines: [&CStr; 23] = [
    c"#[fg=themelightgrey]Client Name   #[#{E:tree-mode-border-style},acs]x#[default] #{client_name} #[fg=themelightgrey]#[fg=themelightgrey](PID #{client_pid})#[default]",
    c"#[fg=themelightgrey]Session       #[#{E:tree-mode-border-style},acs]x#[default] #{session_name}",
    c"#[fg=themelightgrey]Attach Time   #[#{E:tree-mode-border-style},acs]x#[default] #{t:client_created} #[fg=themelightgrey](#{t/r:client_created})#[default]",
    c"#[fg=themelightgrey]Activity Time #[#{E:tree-mode-border-style},acs]x#[default] #{t:client_activity} #[fg=themelightgrey](#{t/r:client_activity})#[default]",
    c"#[fg=themelightgrey]Terminal Type #[#{E:tree-mode-border-style},acs]x#[default] #{?client_termtype,#{client_termtype},Unknown}",
    c"#[fg=themelightgrey]TERM          #[#{E:tree-mode-border-style},acs]x#[default] #{client_termname}",
    c"#[fg=themelightgrey]Size          #[#{E:tree-mode-border-style},acs]x#[default] #{client_width}x#{client_height} #[fg=themelightgrey](cell #{client_cell_width}x#{client_cell_height})#[default]",
    c"#[fg=themelightgrey]Bytes Written #[#{E:tree-mode-border-style},acs]x#[default] #{client_written} #[fg=themelightgrey](#{client_discarded} discarded)#[default]",
    c"#[fg=themelightgrey]Features      #[#{E:tree-mode-border-style},acs]x#[default] #{?#{I/f:256},#[fg=themegreen],#[fg=themelightgrey]}#{p/15:#{l:256}}#[default] #{?#{I/f:RGB},#[fg=themegreen],#[fg=themelightgrey]}#{p/15:#{l:RGB}}#[default] #{?#{I/f:bpaste},#[fg=themegreen],#[fg=themelightgrey]}#{p/15:#{l:bpaste}}#[default] #{?#{I/f:ccolour},#[fg=themegreen],#[fg=themelightgrey]}#{p/15:#{l:ccolour}}#[default]",
    c"              #[#{E:tree-mode-border-style},acs]x#[default] #{?#{I/f:clipboard},#[fg=themegreen],#[fg=themelightgrey]}#{p/15:#{l:clipboard}}#[default] #{?#{I/f:cstyle},#[fg=themegreen],#[fg=themelightgrey]}#{p/15:#{l:cstyle}}#[default] #{?#{I/f:extkeys},#[fg=themegreen],#[fg=themelightgrey]}#{p/15:#{l:extkeys}}#[default] #{?#{I/f:focus},#[fg=themegreen],#[fg=themelightgrey]}#{p/15:#{l:focus}}#[default]",
    c"              #[#{E:tree-mode-border-style},acs]x#[default] #{?#{I/f:hyperlinks},#[fg=themegreen],#[fg=themelightgrey]}#{p/15:#{l:hyperlinks}}#[default] #{?#{I/f:ignorefkeys},#[fg=themegreen],#[fg=themelightgrey]}#{p/15:#{l:ignorefkeys}}#[default] #{?#{I/f:margins},#[fg=themegreen],#[fg=themelightgrey]}#{p/15:#{l:margins}}#[default] #{?#{I/f:mouse},#[fg=themegreen],#[fg=themelightgrey]}#{p/15:#{l:mouse}}#[default]",
    c"              #[#{E:tree-mode-border-style},acs]x#[default] #{?#{I/f:osc7},#[fg=themegreen],#[fg=themelightgrey]}#{p/15:#{l:osc7}}#[default] #{?#{I/f:overline},#[fg=themegreen],#[fg=themelightgrey]}#{p/15:#{l:overline}}#[default] #{?#{I/f:progressbar},#[fg=themegreen],#[fg=themelightgrey]}#{p/15:#{l:progressbar}}#[default] #{?#{I/f:rectfill},#[fg=themegreen],#[fg=themelightgrey]}#{p/15:#{l:rectfill}}#[default]",
    c"              #[#{E:tree-mode-border-style},acs]x#[default] #{?#{I/f:sixel},#[fg=themegreen],#[fg=themelightgrey]}#{p/15:#{l:sixel}}#[default] #{?#{I/f:strikethrough},#[fg=themegreen],#[fg=themelightgrey]}#{p/15:#{l:strikethrough}}#[default] #{?#{I/f:sync},#[fg=themegreen],#[fg=themelightgrey]}#{p/15:#{l:sync}}#[default] #{?#{I/f:title},#[fg=themegreen],#[fg=themelightgrey]}#{p/15:#{l:title}}#[default]",
    c"              #[#{E:tree-mode-border-style},acs]x#[default] #{?#{I/f:usstyle},#[fg=themegreen],#[fg=themelightgrey]}#{p/15:#{l:usstyle}}#[default] #{?#{I/f:utf8},#[fg=themegreen],#[fg=themelightgrey]}#{p/15:#{l:utf8}}#[default]",
    c"#[#{E:tree-mode-border-style},acs]qqqqqqqqqqqqqqn#{R:q,#{window_width}}#[default]",
    c"#[fg=themelightgrey]prefix        #[#{E:tree-mode-border-style},acs]x#[default] #{prefix}",
    c"#[fg=themelightgrey]mouse         #[#{E:tree-mode-border-style},acs]x#[default] #{?mouse,#{?#{I/c:kmous},,#[fg=themered]}on,#[fg=themelightgrey]off} #{?#{I/c:kmous},,#[align=right]unavailable: [kmous] missing}",
    c"#[fg=themelightgrey]set-clipboard #[#{E:tree-mode-border-style},acs]x#[default] #{?#{!=:#{set-clipboard},off},#{?#{I/c:Ms},,#[fg=themered]}#{set-clipboard},#[fg=themelightgrey]off} #{?#{I/c:Ms},,#[align=right]unavailable: [Ms] #{?clipboard_invalid,invalid,missing}}",
    c"#[fg=themelightgrey]get-clipboard #[#{E:tree-mode-border-style},acs]x#[default] #{?#{!=:#{get-clipboard},off},#{?#{I/c:Ms},,#[fg=themered]}#{get-clipboard},#[fg=themelightgrey]off} #{?#{I/c:Ms},,#[align=right]unavailable: [Ms] #{?clipboard_invalid,invalid,missing}}",
    c"#[fg=themelightgrey]focus-events  #[#{E:tree-mode-border-style},acs]x#[default] #{?focus-events,#{?#{I/f:focus},,#[fg=themered]}on,#[fg=themelightgrey]off} #{?#{I/f:focus},,#[align=right]unavailable: [Enfcs] or [Dcfcs] missing}",
    c"#[fg=themelightgrey]extended-keys #[#{E:tree-mode-border-style},acs]x#[default] #{?#{!=:#{extended-keys},off},#{?#{I/f:extkeys},,#[fg=themered]}#{extended-keys},#[fg=themelightgrey]off} #{?#{I/f:extkeys},,#[align=right]unavailable: [Eneks] or [Dseks] missing}",
    c"#[fg=themelightgrey]set-titles    #[#{E:tree-mode-border-style},acs]x#[default] #{?set-titles,on,#[fg=themelightgrey]off}",
    c"#[fg=themelightgrey]escape-time   #[#{E:tree-mode-border-style},acs]x#[default] #{escape-time} ms",
];
static window_client_menu_items: [menu_item<'static>; 8] = [
    menu_item {
        name: c"Detach",
        key: 'd' as i32 as key_code,
        command: None,
    },
    menu_item {
        name: c"Detach Tagged",
        key: 'D' as i32 as key_code,
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
        name: c"Cancel",
        key: 'q' as i32 as key_code,
        command: None,
    },
];
pub static window_client_mode: window_mode = {
    window_mode {
        name: c"client-mode",
        default_format: Some(WINDOW_CLIENT_DEFAULT_FORMAT),
        flags: 0,
        init: Some(
            window_client_init
                as unsafe fn(
                    refbox::Weak<window_mode_entry>,
                    Option<&std::rc::Rc<std::cell::RefCell<cmdq_item>>>,
                    Option<&cmd_find_state>,
                    Option<&args>,
                ) -> bool,
        ),
        free: Some(window_client_free as unsafe fn(refbox::Weak<window_mode_entry>) -> ()),
        resize: Some(
            window_client_resize as unsafe fn(refbox::Weak<window_mode_entry>, u_int, u_int) -> (),
        ),
        update: Some(window_client_update as unsafe fn(refbox::Weak<window_mode_entry>) -> ()),
        style_changed: None,
        key: Some(
            window_client_key
                as unsafe fn(
                    refbox::Weak<window_mode_entry>,
                    &ClientRef,
                    refbox::Weak<winlink>,
                    key_code,
                    Option<&mut mouse_event>,
                ) -> (),
        ),
        key_table: None,
        command: None,
        formats: None,
        get_screen: None,
        display_screen: Some(window_client_get_screen),
    }
};
static window_client_order_seq: [sort_order; 4] =
    [SORT_NAME, SORT_SIZE, SORT_CREATION, SORT_ACTIVITY];
unsafe fn window_client_add_item(
    items: &mut Vec<refbox::RefBox<window_client_itemdata>>,
    c: &ClientRef,
) {
    let ttyname = c.tty_name().expect("attached client has a terminal name");
    items.push(refbox::RefBox::new(window_client_itemdata::new(
        c, &ttyname,
    )));
}

unsafe fn window_client_build(
    data: &mut window_client_modedata,
    sort_crit: &sort_criteria,
    filter: Option<&CStr>,
) {
    let mut i: u_int = 0;
    let mut c: Option<ClientRef> = None;
    data.items.clear();
    let clients_sorted = sort_get_clients(sort_crit);
    i = 0 as u_int;
    while (i as usize) < clients_sorted.len() {
        let c = Some(clients_sorted[i as usize].clone());
        if !(c
            .as_ref()
            .expect("live client")
            .attached_session()
            .upgrade()
            .is_none()
            || c.as_ref().expect("live client").flags() & CLIENT_UNATTACHEDFLAGS as uint64_t != 0)
        {
            window_client_add_item(&mut data.items, &clients_sorted[i as usize]);
        }
        i = i.wrapping_add(1);
    }
    let mut current_block_21: u64;
    i = 0 as u_int;
    while (i as usize) < data.items.len() {
        let item_handle = (&data.items)[i as usize].downgrade();
        let item = ModeTreeItemData::Client(item_handle.clone())
            .as_client()
            .unwrap();
        c = Some(item.client().clone());
        if let Some(filter) = filter {
            let cp = format_single_cstring(
                None,
                filter,
                c.as_ref(),
                None,
                (refbox::Weak::new()).clone(),
                None,
            );
            if !format_true(&cp) {
                current_block_21 = 3512920355445576850;
            } else {
                current_block_21 = 12147880666119273379;
            }
        } else {
            current_block_21 = 12147880666119273379;
        }
        if current_block_21 == 12147880666119273379 {
            let text = format_single_cstring(
                None,
                &data.format,
                c.as_ref(),
                None,
                (refbox::Weak::new()).clone(),
                None,
            );
            data.tree_owner().borrow_mut().add(
                None,
                ModeTreeItemData::Client(item_handle.clone()),
                std::rc::Rc::as_ptr(c.as_ref().expect("live client")) as uint64_t,
                c.as_ref()
                    .expect("live client")
                    .name()
                    .as_deref()
                    .expect("attached client has a name"),
                Some(&text),
                -(1 as ::core::ffi::c_int),
            );
        }
        i = i.wrapping_add(1);
    }
}
unsafe fn window_client_draw_info(
    item: &window_client_itemdata,
    ctx: &mut screen_write_ctx,
    mut sx: u_int,
    mut sy: u_int,
) {
    let c = item.client();
    let options_window = WindowRef::downgrade(
        &winlink_window(
            &c.attached_session()
                .upgrade()
                .expect("live session")
                .current_winlink(),
        )
        .expect("live window"),
    );
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
    let mut ft_owner =
        format_create_defaults(None, Some(c), None, (refbox::Weak::new()).clone(), None);
    let ft = &mut *ft_owner;
    if c.borrow_terminal().clipboard_invalid() {
        format_add(ft, c"clipboard_invalid", |out| out.write_all(b"1"));
    } else {
        format_add(ft, c"clipboard_invalid", |out| out.write_all(b"0"));
    }
    screen_write_cursormove(
        ctx,
        cx as ::core::ffi::c_int,
        cy as ::core::ffi::c_int,
        0 as ::core::ffi::c_int,
    );
    i = 0 as u_int;
    while (i as usize) < window_client_info_lines.len() {
        if i == sy {
            break;
        }
        let expanded = format_expand_cstring(ft, window_client_info_lines[i as usize]);
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
    }
    if sx > 14 as u_int && i < sy {
        gc = grid_default_cell;
        style_apply_with_options(
            &mut gc,
            c"tree-mode-border-style",
            None,
            &OptionsScope::Window(options_window.clone()),
        );
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
unsafe fn window_client_draw(
    data: &window_client_modedata,
    item: &window_client_itemdata,
    ctx: &mut screen_write_ctx,
    mut sx: u_int,
    mut sy: u_int,
) {
    let Some(mode_pane_owner) =
        std::rc::Rc::<std::cell::RefCell<window_pane>>::from_observer(&data.wp)
    else {
        return;
    };
    let c = item.client();
    let mut session: Option<SessionRef> = c.attached_session().upgrade();
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
    let mut lines: u_int = 0;
    let mut at: u_int = 0;
    if session.is_none() || c.flags() & CLIENT_UNATTACHEDFLAGS as uint64_t != 0 {
        return;
    }
    if data.preview_is_info != 0 {
        window_client_draw_info(item, ctx, sx, sy);
        return;
    }
    let window_owner = winlink_window(&session.as_ref().expect("live session").current_winlink())
        .expect("live window")
        .clone();
    let options_window = WindowRef::downgrade(&window_owner);
    let mut preview_pane = window_owner.active_pane().upgrade();
    if data.hide_preview_this_pane != 0
        && preview_pane
            .as_ref()
            .is_some_and(|pane| std::rc::Rc::ptr_eq(pane, &mode_pane_owner))
    {
        preview_pane = window_owner.last_active_pane().upgrade();
    }
    drop(window_owner);
    lines = status_line_size(c);
    if lines >= sy {
        lines = 0 as u_int;
    }
    if status_at_line(c) == 0 as ::core::ffi::c_int {
        at = lines;
    } else {
        at = 0 as u_int;
    }
    screen_write_cursormove(
        ctx,
        cx as ::core::ffi::c_int,
        cy.wrapping_add(at) as ::core::ffi::c_int,
        0 as ::core::ffi::c_int,
    );
    if let Some(pane) = preview_pane.as_ref() {
        let (width, height) = pane.screen_size(false);
        let mut snapshot = screen::empty();
        pane.copy_screen(&mut snapshot, 0, 0, width, height, false);
        screen_write_preview(
            ctx,
            &snapshot,
            sx,
            sy.wrapping_sub(2 as u_int).wrapping_sub(lines),
        );
        drop(snapshot);
    }
    if let Some(pane) = preview_pane {
        drop(pane);
    }
    if at != 0 as u_int {
        screen_write_cursormove(
            ctx,
            cx as ::core::ffi::c_int,
            cy.wrapping_add(2 as u_int) as ::core::ffi::c_int,
            0 as ::core::ffi::c_int,
        );
    } else {
        screen_write_cursormove(
            ctx,
            cx as ::core::ffi::c_int,
            cy.wrapping_add(sy)
                .wrapping_sub(1 as u_int)
                .wrapping_sub(lines) as ::core::ffi::c_int,
            0 as ::core::ffi::c_int,
        );
    }
    gc = grid_default_cell;
    style_apply_with_options(
        &mut gc,
        c"tree-mode-border-style",
        None,
        &OptionsScope::Window(options_window.clone()),
    );
    screen_write_hline(
        ctx,
        sx,
        0 as ::core::ffi::c_int,
        0 as ::core::ffi::c_int,
        BOX_LINES_DEFAULT,
        Some(&gc),
    );
    if at != 0 as u_int {
        screen_write_cursormove(
            ctx,
            cx as ::core::ffi::c_int,
            cy as ::core::ffi::c_int,
            0 as ::core::ffi::c_int,
        );
    } else {
        screen_write_cursormove(
            ctx,
            cx as ::core::ffi::c_int,
            cy.wrapping_add(sy).wrapping_sub(lines) as ::core::ffi::c_int,
            0 as ::core::ffi::c_int,
        );
    }
    let mut preview = window_client_status_snapshot(c, lines);
    screen_write_fast_copy(ctx, &preview, 0, 0, sx, lines);
    drop(preview);
}
unsafe fn window_client_status_snapshot(c: &ClientRef, lines: u32) -> screen {
    let status = c.borrow_status();
    let source = status.screen.grid();
    let rows = lines.min(source.hsize.wrapping_add(source.sy));
    let mut snapshot = screen::empty();
    snapshot.grid = Some(crate::grid::grid_create(source.sx, rows, 0));
    crate::grid::grid_duplicate_lines(snapshot.grid_mut(), 0, source, 0, rows);
    snapshot
}

unsafe fn window_client_menu(
    wme: &refbox::Weak<window_mode_entry>,
    c: &ClientRef,
    mut key: key_code,
) {
    if !wme.is_alive() {
        return;
    }
    let Some(mode_pane_owner) = wme.try_borrow().expect("live mode entry").wp.upgrade() else {
        return;
    };
    if mode_pane_owner.mode_entry().as_ptr() != wme.as_ptr() {
        return;
    }
    window_client_key(wme.clone(), c, (refbox::Weak::new()).clone(), key, None);
}
unsafe fn window_client_get_key(
    data: &window_client_modedata,
    item: &window_client_itemdata,
    mut line: u_int,
) -> key_code {
    let mut key: key_code = 0;
    let mut ft_owner = format_create(None, None, FORMAT_NONE, 0 as ::core::ffi::c_int);
    let ft = &mut *ft_owner;
    format_defaults(
        ft,
        Some(item.client()),
        None,
        (refbox::Weak::new()).clone(),
        None,
    );
    format_add(ft, c"line", |out| write!(out, "{}", { line }));
    let expanded = format_expand_cstring(ft, &data.key_format);
    key = key_string_parse_cstr(expanded.as_c_str()).unwrap_or(KEYC_UNKNOWN);
    drop(ft_owner);
    key
}
fn window_client_sort(sort_crit: &mut sort_criteria) {
    {
        sort_crit.order_seq = &window_client_order_seq;
        if sort_crit.order as ::core::ffi::c_uint
            == SORT_END as ::core::ffi::c_int as ::core::ffi::c_uint
        {
            sort_crit.order = sort_crit.order_seq[0];
        }
    }
}
static window_client_help_lines: &[&CStr] = &[
    c"#[fg=themelightgrey]          i #[#{E:tree-mode-border-style},acs]x#[default] Toggle info view",
    c"#[fg=themelightgrey]      Enter #[#{E:tree-mode-border-style},acs]x#[default] Choose selected %1",
    c"#[fg=themelightgrey]          d #[#{E:tree-mode-border-style},acs]x#[default] Detach selected %1",
    c"#[fg=themelightgrey]          D #[#{E:tree-mode-border-style},acs]x#[default] Detach tagged %1s",
    c"#[fg=themelightgrey]          x #[#{E:tree-mode-border-style},acs]x#[default] Detach selected %1",
    c"#[fg=themelightgrey]          X #[#{E:tree-mode-border-style},acs]x#[default] Detach tagged %1s",
    c"#[fg=themelightgrey]          z #[#{E:tree-mode-border-style},acs]x#[default] Suspend selected %1",
    c"#[fg=themelightgrey]          Z #[#{E:tree-mode-border-style},acs]x#[default] Suspend tagged %1s",
    c"#[fg=themelightgrey]          f #[#{E:tree-mode-border-style},acs]x#[default] Enter a filter",
];
fn window_client_help() -> mode_tree_help_info {
    mode_tree_help_info {
        width: 0 as u_int,
        item: c"client",
        lines: window_client_help_lines,
    }
}
fn window_client_data(
    wme: &refbox::Weak<window_mode_entry>,
) -> ModeDataRef<'_, window_client_modedata> {
    ModeDataRef::new(wme.try_borrow().expect("live mode entry")).expect("client mode payload")
}

unsafe fn window_client_init(
    mut wme: refbox::Weak<window_mode_entry>,
    _item_handle: Option<&std::rc::Rc<std::cell::RefCell<cmdq_item>>>,
    _fs: Option<&cmd_find_state>,
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
            WINDOW_CLIENT_DEFAULT_FORMAT
        }
        .to_owned();
    let key_format =
        if let Some(args) = args.filter(|args| args_has(args, 'K' as i32 as u_char) != 0) {
            args_get(args, 'K' as i32 as u_char).expect("-K takes a value")
        } else {
            WINDOW_CLIENT_DEFAULT_KEY_FORMAT
        }
        .to_owned();
    let command = if let Some(args) = args.filter(|args| args_count(args) != 0 as u_int) {
        args_string(args, 0 as u_int).expect("argument 0 exists")
    } else {
        WINDOW_CLIENT_DEFAULT_COMMAND
    }
    .to_owned();
    let preview_is_info =
        args.is_some_and(|args| args_has(args, 'i' as i32 as u_char) != 0) as ::core::ffi::c_int;
    let owner = Box::new(std::cell::RefCell::new(window_client_modedata {
        wp: std::rc::Rc::downgrade(&mode_pane_owner),
        data: None,
        format,
        key_format,
        command,
        hide_preview_this_pane: args.is_some_and(|args| args_has(args, 'h' as i32 as u_char) != 0)
            as ::core::ffi::c_int,
        preview_is_info,
        items: Vec::new(),
    }));
    wme.try_borrow_mut().expect("live mode entry").boxed_data = Some(owner);
    let build_mode = wme.clone();
    let draw_mode = wme.clone();
    let menu_mode = wme.clone();
    let key_mode = wme.clone();
    let tree_owner = mode_tree_start(
        &mode_pane_owner,
        args.expect("mode arguments"),
        Some(Box::new(move |sort, tag, filter| {
            let mut selected = tag.unwrap_or(::core::primitive::u64::MAX as uint64_t);
            let data = window_client_data(&build_mode);
            window_client_build(&mut data.borrow_mut(), sort, filter);
            (selected != ::core::primitive::u64::MAX as uint64_t).then_some(selected)
        })),
        Some(Box::new(move |itemdata, ctx, sx, sy| {
            let item = itemdata.as_client().expect("client row payload");
            let data = window_client_data(&draw_mode);
            let data = data.borrow();
            window_client_draw(&data, &item, ctx, sx, sy)
        })),
        None,
        Some(Box::new(move |client, key| {
            window_client_menu(&menu_mode, client, key)
        })),
        None,
        Some(Box::new(move |itemdata, line| {
            let item = itemdata.as_client().expect("client row payload");
            let data = window_client_data(&key_mode);
            let data = data.borrow();
            window_client_get_key(&data, &item, line)
        })),
        None,
        Some(window_client_sort),
        Some(window_client_help),
        &window_client_menu_items,
    );
    window_client_data(&wme).borrow_mut().data = Some(tree_owner.clone());
    if preview_is_info != 0 {
        tree_owner.borrow_mut().set_view_name(Some(c"info"));
    } else {
        tree_owner.borrow_mut().set_view_name(Some(c"preview"));
    }
    mode_tree_build(&tree_owner);
    mode_tree_draw(&tree_owner);
    true
}
unsafe fn window_client_get_screen(
    wme: refbox::Weak<window_mode_entry>,
    visit: &mut dyn FnMut(&mut screen),
) {
    let Some(data) =
        ModeDataRef::<window_client_modedata>::new(wme.try_borrow().expect("live mode entry"))
    else {
        return;
    };
    let tree_owner = window_mode_screen_payload(&data, |data| data.data.clone());
    if let Some(tree_owner) = tree_owner {
        mode_tree_visit_screen(&tree_owner, visit);
    }
}

unsafe fn window_client_free(wme: refbox::Weak<window_mode_entry>) {
    let Some(data) =
        ModeDataRef::<window_client_modedata>::new(wme.try_borrow().expect("live mode entry"))
    else {
        return;
    };
    let tree_owner = data.borrow_mut().data.take().expect("mode tree owner");
    mode_tree_free(tree_owner);
}
unsafe fn window_client_resize(
    mut wme: refbox::Weak<window_mode_entry>,
    mut sx: u_int,
    mut sy: u_int,
) {
    let tree_owner = window_client_data(&wme).borrow().tree_owner();
    mode_tree_resize(&tree_owner, sx, sy);
}
unsafe fn window_client_update(mut wme: refbox::Weak<window_mode_entry>) {
    let (pane, tree_owner) = {
        let data_cell = window_client_data(&wme);
        let data = data_cell.borrow();
        (data.wp.clone(), data.tree_owner())
    };
    let Some(mode_pane_owner) =
        std::rc::Rc::<std::cell::RefCell<window_pane>>::from_observer(&pane)
    else {
        return;
    };
    mode_tree_build(&tree_owner);
    mode_tree_draw(&tree_owner);
    mode_pane_owner.request_redraw(false);
}
unsafe fn window_client_do_detach(
    tree_owner: &Rc<std::cell::RefCell<mode_tree_data>>,
    item: &ModeTreeItemSnapshot<window_client_itemdata>,
    mut key: key_code,
) {
    if tree_owner.borrow().get_current().is_client(item) {
        tree_owner.borrow_mut().down(0 as ::core::ffi::c_int);
    }
    if key == 'd' as i32 as key_code || key == 'D' as i32 as key_code {
        (item.client()).detach(MSG_DETACH);
    } else if key == 'x' as i32 as key_code || key == 'X' as i32 as key_code {
        (item.client()).detach(MSG_DETACHKILL);
    } else if key == 'z' as i32 as key_code || key == 'Z' as i32 as key_code {
        (item.client()).suspend();
    }
}
unsafe fn window_client_key(
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
    let data = window_client_data(&wme);
    let tree_owner = data.borrow().tree_owner();
    let mut finished: ::core::ffi::c_int = 0;
    finished = mode_tree_key(
        tree_owner.clone(),
        Some(client_owner),
        &mut key,
        m.as_deref(),
        None,
        None,
    );
    match key {
        100 | 120 | 122 => {
            let item_owner = tree_owner.borrow().get_current();
            if let Some(item) = item_owner.as_client() {
                window_client_do_detach(&tree_owner, &item, key);
            }
            mode_tree_build(&tree_owner);
        }
        68 | 88 | 90 => {
            mode_tree_each_tagged(
                &tree_owner,
                |row, key| unsafe {
                    let itemdata = row.borrow().itemdata.clone();
                    window_client_do_detach(
                        &tree_owner,
                        &itemdata.as_client().expect("client row payload"),
                        key,
                    )
                },
                key,
                0 as ::core::ffi::c_int,
            );
            mode_tree_build(&tree_owner);
        }
        105 => {
            let preview_is_info = {
                let mut data = data.borrow_mut();
                data.preview_is_info = (data.preview_is_info == 0) as ::core::ffi::c_int;
                data.preview_is_info
            };
            if preview_is_info != 0 {
                tree_owner.borrow_mut().set_view_name(Some(c"info"));
            } else {
                tree_owner.borrow_mut().set_view_name(Some(c"preview"));
            }
            mode_tree_build(&tree_owner);
        }
        13 => {
            let item_owner = tree_owner.borrow().get_current();
            if let Some(item) = item_owner.as_client() {
                let command = data.borrow().command.clone();
                mode_tree_run_command(Some(client_owner), None, &command, &item.ttyname);
            }
            finished = 1 as ::core::ffi::c_int;
        }
        _ => {}
    }
    if finished != 0 || ClientRef::attached_count() == 0 as u_int {
        mode_pane_owner.reset_mode();
    } else {
        mode_tree_draw(&tree_owner);
        mode_pane_owner.request_redraw(false);
    };
}
