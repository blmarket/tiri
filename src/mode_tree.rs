use crate::src::arguments::{args, args_get, args_has};
use crate::src::cmd::parse::{cmd_parse_and_append, cmd_parse_error_uppercase_first};
use crate::src::cmd::queue::{
    cmdq_append, cmdq_get_callback_owned, cmdq_get_client, cmdq_new_state,
};
use crate::src::cmd::{cmd_mouse_at, cmd_template_replace_cstring};
use crate::src::format::bytes::nullable_cstr;
use crate::src::format::bytes::write_cstr;
use crate::src::format::{format_add, format_create_defaults, format_expand_cstring};
use crate::src::format_draw::{format_draw, format_width};
use crate::src::grid::grid_default_cell;
use crate::src::key_string::key_string_format;
use crate::src::log::{log_bytes, log_debug};
use crate::src::menu::{menu_add_items, menu_create, menu_display};
use crate::src::options::options_get_number;
use crate::src::options::OptionsScope;
use crate::src::prompt::{
    prompt_closed, prompt_create, prompt_draw, prompt_free, prompt_key, prompt_mouse,
    prompt_set_options,
};
use crate::src::screen::{screen_init, screen_resize};
use crate::src::screen_write::{
    screen_write_box, screen_write_clearcharacter, screen_write_clearendofline,
    screen_write_clearscreen, screen_write_cursormove, screen_write_puts, screen_write_start,
    screen_write_stop,
};
use crate::src::server_client::Client as _;
use crate::src::session::Session as _;
use crate::src::shared::abi::__int32_t;
use crate::src::shared::abi::*;
use crate::src::shared::client::{ClientRef, ClientWeak};
use crate::src::shared::colour::{COLOUR_FLAG_THEME, COLOUR_THEME_CYAN};
use crate::src::shared::command::cmd_find_state;
use crate::src::shared::command::*;
use crate::src::shared::format::format_tree;
use crate::src::shared::grid::*;
use crate::src::shared::key::*;
use crate::src::shared::layout::*;
use crate::src::shared::menu::{menu_choice_cb, menu_item, MenuSelection};
use crate::src::shared::mode_tree::{
    mode_tree_build_cb, mode_tree_data, mode_tree_draw_cb, mode_tree_height_cb, mode_tree_help_cb,
    mode_tree_item, mode_tree_key_cb, mode_tree_line, mode_tree_list, mode_tree_menu_cb,
    mode_tree_prompt, mode_tree_prompt_input_cb, mode_tree_search_cb, mode_tree_search_dir,
    mode_tree_sort_cb, mode_tree_swap_cb, ModeTreeItemData, ModeTreeItemRef,
};
use crate::src::shared::mouse::{mouse_event, MOUSE_BUTTON_1, MOUSE_MASK_BUTTONS, MOUSE_MASK_DRAG};
use crate::src::shared::pane::window_pane;
use crate::src::shared::prompt::*;
use crate::src::shared::prompt::{prompt_create_data, prompt_draw_data};
use crate::src::shared::prompt::{
    prompt_free_cb, prompt_result, PROMPT_ACCEPT, PROMPT_CLOSE, PROMPT_CONTINUE, PROMPT_ISMODE,
    PROMPT_NOFORMAT, PROMPT_SINGLE,
};
use crate::src::shared::screen::{screen, MODE_CURSOR};
use crate::src::shared::screen_write::screen_write_ctx;
use crate::src::shared::style::*;
use libc::{strcasestr, strstr};

use crate::src::sort::{sort_next_order, sort_order_from_string, sort_order_to_string};
use crate::src::status::status_message_set;
use crate::src::style::style_apply_with_options;
use crate::src::window_pane::WindowPane as _;
use std::cell::UnsafeCell;
use std::ffi::{CStr, CString};
use std::rc::{Rc, Weak};

pub const MODE_TREE_SEARCH_BACKWARD: mode_tree_search_dir = 1;
pub const MODE_TREE_SEARCH_FORWARD: mode_tree_search_dir = 0;

impl mode_tree_item {
    fn set_keystr(&mut self, keystr: Option<CString>) {
        self.keystr = keystr;

        self.keylen = self.keystr.as_ref().map_or(0, |s| s.as_bytes().len());
    }
}
pub type mode_tree_preview = ::core::ffi::c_uint;
pub const MODE_TREE_PREVIEW_BIG: mode_tree_preview = 2;
pub const MODE_TREE_PREVIEW_NORMAL: mode_tree_preview = 1;
pub const MODE_TREE_PREVIEW_OFF: mode_tree_preview = 0;
#[inline]
unsafe fn tolower(mut __c: ::core::ffi::c_int) -> ::core::ffi::c_int {
    if __c >= -(128 as ::core::ffi::c_int) && __c < 256 as ::core::ffi::c_int {
        ::libc::tolower(__c as ::core::ffi::c_int) as ::core::ffi::c_int
    } else {
        __c
    }
}
pub const UINT64_MAX: ::core::ffi::c_ulong = 18446744073709551615 as ::core::ffi::c_ulong;
static mode_tree_menu_items: [menu_item<'static>; 4] = [
    menu_item {
        name: c"Scroll Left",
        key: '<' as i32 as key_code,
        command: None,
    },
    menu_item {
        name: c"Scroll Right",
        key: '>' as i32 as key_code,
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
static mode_tree_help_start: &[&CStr] = &[
    c"#[fg=themelightgrey]      Up, k #[#{E:tree-mode-border-style},acs]x#[default] Move cursor up",
    c"#[fg=themelightgrey]    Down, j #[#{E:tree-mode-border-style},acs]x#[default] Move cursor down",
    c"#[fg=themelightgrey]          g #[#{E:tree-mode-border-style},acs]x#[default] Go to top",
    c"#[fg=themelightgrey]          G #[#{E:tree-mode-border-style},acs]x#[default] Go to bottom",
    c"#[fg=themelightgrey] PPage, C-b #[#{E:tree-mode-border-style},acs]x#[default] Page up",
    c"#[fg=themelightgrey] NPage, C-f #[#{E:tree-mode-border-style},acs]x#[default] Page down",
    c"#[fg=themelightgrey]    Left, h #[#{E:tree-mode-border-style},acs]x#[default] Collapse %1",
    c"#[fg=themelightgrey]   Right, l #[#{E:tree-mode-border-style},acs]x#[default] Expand %1",
    c"#[fg=themelightgrey]        M-- #[#{E:tree-mode-border-style},acs]x#[default] Collapse all %1s",
    c"#[fg=themelightgrey]        M-+ #[#{E:tree-mode-border-style},acs]x#[default] Expand all %1s",
    c"#[fg=themelightgrey]          t #[#{E:tree-mode-border-style},acs]x#[default] Toggle %1 tag",
    c"#[fg=themelightgrey]          T #[#{E:tree-mode-border-style},acs]x#[default] Untag all %1s",
    c"#[fg=themelightgrey]        C-t #[#{E:tree-mode-border-style},acs]x#[default] Tag all %1s",
    c"#[fg=themelightgrey]        C-s #[#{E:tree-mode-border-style},acs]x#[default] Search forward",
    c"#[fg=themelightgrey]          n #[#{E:tree-mode-border-style},acs]x#[default] Repeat search forward",
    c"#[fg=themelightgrey]          N #[#{E:tree-mode-border-style},acs]x#[default] Repeat search backward",
    c"#[fg=themelightgrey]          f #[#{E:tree-mode-border-style},acs]x#[default] Filter %1s",
    c"#[fg=themelightgrey]          O #[#{E:tree-mode-border-style},acs]x#[default] Change sort order",
    c"#[fg=themelightgrey]          r #[#{E:tree-mode-border-style},acs]x#[default] Reverse sort order",
    c"#[fg=themelightgrey]          v #[#{E:tree-mode-border-style},acs]x#[default] Toggle preview",
];
static mode_tree_help_end: &[&CStr] =
    &[c"#[fg=themelightgrey]  q, Escape #[#{E:tree-mode-border-style},acs]x#[default] Exit mode"];
pub const MODE_TREE_HELP_DEFAULT_WIDTH: ::core::ffi::c_int = 39 as ::core::ffi::c_int;
unsafe fn mode_tree_is_lowercase(mut ptr: *const ::core::ffi::c_char) -> ::core::ffi::c_int {
    while *ptr as ::core::ffi::c_int != '\0' as i32 {
        if *ptr as ::core::ffi::c_int
            != ({
                let mut __res: ::core::ffi::c_int = 0;
                if ::core::mem::size_of::<u_char>() as usize > 1_usize {
                    if 0 != 0 {
                        let mut __c: ::core::ffi::c_int = *ptr as u_char as ::core::ffi::c_int;
                        __res = (if __c < -(128 as ::core::ffi::c_int)
                            || __c > 255 as ::core::ffi::c_int
                        {
                            __c as __int32_t
                        } else {
                            ::libc::tolower(__c as ::core::ffi::c_int)
                        }) as ::core::ffi::c_int;
                    } else {
                        __res = tolower(*ptr as u_char as ::core::ffi::c_int);
                    }
                } else {
                    __res =
                        ::libc::tolower(*ptr as u_char as ::core::ffi::c_int) as ::core::ffi::c_int;
                }
                __res
            })
        {
            return 0 as ::core::ffi::c_int;
        }
        ptr = ptr.offset(1);
    }
    1 as ::core::ffi::c_int
}
fn mode_tree_find_item(list: &mode_tree_list, tag: uint64_t) -> Option<ModeTreeItemRef> {
    for item in &list.items {
        let row = item.try_borrow_mut().expect("unborrowed row");
        if row.tag == tag {
            return Some(ModeTreeItemRef::observe(item));
        }
        if let Some(child) = mode_tree_find_item(&row.children, tag) {
            return Some(child);
        }
    }
    None
}
fn mode_tree_free_items(list: &mut mode_tree_list) {
    list.items.clear();
}

fn mode_tree_sibling(
    mtd: &mode_tree_data,
    item: &ModeTreeItemRef,
    forward: bool,
) -> Option<ModeTreeItemRef> {
    let parent = item.borrow().parent.clone();
    let sibling = |list: &mode_tree_list| {
        if forward {
            list.next(item)
        } else {
            list.previous(item)
        }
    };
    if !parent.is_empty() {
        sibling(&parent.borrow().children)
    } else {
        sibling(&mtd.children)
    }
}

fn mode_tree_check_selected(mtd: &mut mode_tree_data) {
    if mtd.height == 0 {
        return;
    }

    let line_count = mode_tree_line_count(mtd);
    if line_count <= mtd.height {
        mtd.offset = 0;
    } else if mtd.offset > line_count - mtd.height {
        mtd.offset = line_count - mtd.height;
    }

    if mtd.current < mtd.offset {
        mtd.offset = mtd.current;
    } else if mtd.current > mtd.offset + mtd.height - 1 {
        mtd.offset = mtd.current - mtd.height + 1;
    }
}
fn mode_tree_alloc_data() -> std::rc::Rc<std::cell::UnsafeCell<mode_tree_data>> {
    Rc::new(UnsafeCell::new(mode_tree_data::default()))
}

#[inline]
fn mode_tree_line_count(mtd: &mode_tree_data) -> u_int {
    mtd.lines.len() as u_int
}

fn mode_tree_clear_lines(mtd: &mut mode_tree_data) {
    mtd.lines = Vec::new();
}
unsafe fn mode_tree_build_lines(
    tree_owner: &Rc<UnsafeCell<mode_tree_data>>,
    items: &[ModeTreeItemRef],
    depth: u_int,
) {
    {
        let tree = &mut *tree_owner.get();
        tree.depth = depth;
        tree.maxdepth = tree.maxdepth.max(depth);
    }
    let mut flat = 1;
    for (index, item) in items.iter().enumerate() {
        let line = {
            let tree = &mut *tree_owner.get();
            if tree.dead != 0 {
                return;
            }
            if !item.is_alive() {
                continue;
            }
            let line = tree.lines.len() as u_int;
            tree.lines.push(mode_tree_line {
                item: item.clone(),
                depth,
                last: (index + 1 == items.len()) as i32,
                flat: 0,
            });
            line
        };
        let (children, expanded) = {
            let mut row = item.borrow_mut();
            row.line = line;
            if !row.children.items.is_empty() {
                flat = 0;
            }
            (row.children.snapshot(), row.expanded)
        };
        if expanded != 0 {
            mode_tree_build_lines(tree_owner, &children, depth.wrapping_add(1));
        }
        if (&*tree_owner.get()).dead != 0 {
            return;
        }
        let Some(data) = item.try_borrow().map(|row| row.itemdata.clone()) else {
            continue;
        };
        let callback = (&mut *tree_owner.get()).keycb.take();
        let key = if let Some(mut callback) = callback {
            let key = callback(&data, line);
            let tree = &mut *tree_owner.get();
            if tree.dead != 0 {
                return;
            }
            if tree.keycb.is_none() {
                tree.keycb = Some(callback);
            }
            if key == KEYC_UNKNOWN {
                KEYC_NONE
            } else {
                key
            }
        } else if line < 10 {
            (b'0' as u_int).wrapping_add(line) as key_code
        } else if line < 36 {
            KEYC_META | (b'a' as u_int).wrapping_add(line).wrapping_sub(10) as key_code
        } else {
            KEYC_NONE
        };
        if (&*tree_owner.get()).dead != 0 {
            return;
        }
        let keystr = (key != KEYC_NONE).then(|| key_string_format(key, false));
        let Some(mut row) = item.try_borrow() else {
            continue;
        };
        row.key = key;
        row.set_keystr(keystr);
    }
    let tree = &mut *tree_owner.get();
    for item in items {
        let Some(row) = item.try_borrow() else {
            continue;
        };
        if let Some(line) = tree.lines.get_mut(row.line as usize) {
            if line.item == *item {
                line.flat = flat;
            }
        }
    }
}
fn mode_tree_clear_tagged(list: &mode_tree_list) {
    for item in &list.items {
        let mut row = item.try_borrow_mut().expect("unborrowed row");
        row.tagged = 0;
        mode_tree_clear_tagged(&row.children);
    }
}
pub fn mode_tree_up(mtd: &mut mode_tree_data, mut wrap: ::core::ffi::c_int) {
    if mode_tree_line_count(mtd) == 0 as u_int {
        return;
    }
    if mtd.current == 0 as u_int {
        if wrap != 0 {
            mtd.current = mode_tree_line_count(mtd).wrapping_sub(1 as u_int);
            if mode_tree_line_count(mtd) >= mtd.height {
                mtd.offset = mode_tree_line_count(mtd).wrapping_sub(mtd.height);
            }
        }
    } else {
        mtd.current = mtd.current.wrapping_sub(1);
        if mtd.current < mtd.offset {
            mtd.offset = mtd.offset.wrapping_sub(1);
        }
    };
}
pub fn mode_tree_down(
    mtd: &mut mode_tree_data,
    mut wrap: ::core::ffi::c_int,
) -> ::core::ffi::c_int {
    if mode_tree_line_count(mtd) == 0 as u_int {
        return 0 as ::core::ffi::c_int;
    }
    if mtd.current == mode_tree_line_count(mtd).wrapping_sub(1 as u_int) {
        if wrap != 0 {
            mtd.current = 0 as u_int;
            mtd.offset = 0 as u_int;
        } else {
            return 0 as ::core::ffi::c_int;
        }
    } else {
        mtd.current = mtd.current.wrapping_add(1);
        if mtd.current > mtd.offset.wrapping_add(mtd.height).wrapping_sub(1 as u_int) {
            mtd.offset = mtd.offset.wrapping_add(1);
        }
    }
    1 as ::core::ffi::c_int
}
unsafe fn mode_tree_swap(tree_owner: &Rc<UnsafeCell<mode_tree_data>>, direction: i32) {
    let (mut callback, current, other, mut sort, swap_with) = {
        let mtd = &mut *tree_owner.get();
        if mtd.swapcb.is_none() {
            return;
        }
        let depth = (&mtd.lines)[mtd.current as usize].depth;
        let mut swap_with = mtd.current;
        loop {
            if direction < 0 && swap_with < -direction as u_int {
                return;
            }
            let next = swap_with.wrapping_add(direction as u_int);
            if direction > 0 && next >= mode_tree_line_count(&*mtd) {
                return;
            }
            swap_with = next;
            let other_depth = (&mtd.lines)[swap_with as usize].depth;
            if other_depth > depth {
                continue;
            }
            if other_depth != depth {
                return;
            }
            break;
        }
        let current = (&mtd.lines)[mtd.current as usize]
            .item
            .borrow()
            .itemdata
            .clone();
        let other = (&mtd.lines)[swap_with as usize]
            .item
            .borrow()
            .itemdata
            .clone();
        (
            mtd.swapcb.take().unwrap(),
            current,
            other,
            mtd.sort_crit,
            swap_with,
        )
    };
    let swapped = callback(&current, &other, &mut sort);
    {
        let tree = &mut *tree_owner.get();
        if tree.dead != 0 {
            return;
        }
        tree.sort_crit = sort;
        if tree.swapcb.is_none() {
            tree.swapcb = Some(callback);
        }
        if swapped {
            tree.current = swap_with;
        }
    }
    if swapped {
        mode_tree_build(tree_owner);
    }
}
pub fn mode_tree_get_current(mtd: &mode_tree_data) -> ModeTreeItemData {
    if mtd.lines.is_empty() {
        return ModeTreeItemData::None;
    }
    mtd.lines[mtd.current as usize]
        .item
        .try_borrow()
        .map_or(ModeTreeItemData::None, |row| row.itemdata.clone())
}
pub fn mode_tree_get_current_name(mtd: &mode_tree_data) -> CString {
    mtd.lines[mtd.current as usize]
        .item
        .try_borrow()
        .map_or_else(CString::default, |row| row.name.clone())
}
pub unsafe fn mode_tree_expand_current(tree_owner: &Rc<UnsafeCell<mode_tree_data>>) {
    let mtd = &*tree_owner.get();
    let item = (&mtd.lines)[mtd.current as usize].item.clone();
    if item.borrow().expanded == 0 {
        item.borrow_mut().expanded = 1;
        mode_tree_build(tree_owner);
    }
}
fn mode_tree_get_tag(mtd: &mode_tree_data, tag: uint64_t) -> Option<u_int> {
    mtd.lines
        .iter()
        .position(|line| line.item.borrow().tag == tag)
        .map(|index| index as u_int)
}
pub unsafe fn mode_tree_expand(tree_owner: &Rc<UnsafeCell<mode_tree_data>>, tag: uint64_t) {
    let mtd = &*tree_owner.get();
    let Some(index) = mode_tree_get_tag(mtd, tag) else {
        return;
    };
    let item = (&mtd.lines)[index as usize].item.clone();
    if item.borrow().expanded == 0 {
        item.borrow_mut().expanded = 1;
        mode_tree_build(tree_owner);
    }
}
pub fn mode_tree_set_current(mtd: &mut mode_tree_data, mut tag: uint64_t) -> ::core::ffi::c_int {
    if let Some(found) = mode_tree_get_tag(mtd, tag) {
        mtd.current = found;
        mode_tree_check_selected(mtd);
        return 1 as ::core::ffi::c_int;
    }
    if mtd.current >= mode_tree_line_count(mtd) {
        if mode_tree_line_count(mtd) == 0 as u_int {
            return 0 as ::core::ffi::c_int;
        }
        mtd.current = mode_tree_line_count(mtd).wrapping_sub(1 as u_int);
        mode_tree_check_selected(mtd);
    }
    0 as ::core::ffi::c_int
}
pub fn mode_tree_count_tagged(mtd: &mode_tree_data) -> u_int {
    mtd.lines
        .iter()
        .filter(|line| line.item.try_borrow().is_some_and(|row| row.tagged != 0))
        .count() as u_int
}
pub unsafe fn mode_tree_each_tagged(
    tree_owner: &std::rc::Rc<std::cell::UnsafeCell<mode_tree_data>>,
    mut cb: impl FnMut(&ModeTreeItemRef, key_code),
    key: key_code,
    current: i32,
) {
    let mtd = tree_owner.get();
    let mut fired = false;
    let mut index = 0;
    while (*mtd).dead == 0 && index < (*mtd).lines.len() {
        let item = (&(*mtd).lines)[index].item.clone();
        if item.try_borrow().is_some_and(|row| row.tagged != 0) {
            fired = true;
            cb(&item, key);
        }
        index += 1;
    }
    if (*mtd).dead == 0 && !fired && current != 0 && !(*mtd).lines.is_empty() {
        let item = (&(*mtd).lines)[(*mtd).current as usize].item.clone();
        if item.is_alive() {
            cb(&item, key);
        }
    }
}
pub unsafe fn mode_tree_start(
    pane_owner: &std::rc::Rc<std::cell::UnsafeCell<window_pane>>,
    args: *mut args,
    buildcb: mode_tree_build_cb,
    drawcb: mode_tree_draw_cb,
    searchcb: mode_tree_search_cb,
    menucb: mode_tree_menu_cb,
    heightcb: mode_tree_height_cb,
    keycb: mode_tree_key_cb,
    swapcb: mode_tree_swap_cb,
    sortcb: mode_tree_sort_cb,
    helpcb: mode_tree_help_cb,
    menu: &'static [menu_item<'static>],
    s: *mut *mut screen,
) -> std::rc::Rc<std::cell::UnsafeCell<mode_tree_data>> {
    let (pane_sx, pane_sy) = pane_owner.screen_size(false);
    let owner = mode_tree_alloc_data();
    let mtd = &mut *owner.get();
    mtd.wp = std::rc::Rc::downgrade(pane_owner);
    mtd.menu = menu;
    if drawcb.is_none() {
        mtd.preview = MODE_TREE_PREVIEW_OFF as ::core::ffi::c_int;
    } else if args_has(&*args, 'N' as i32 as u_char) > 1 as ::core::ffi::c_int {
        mtd.preview = MODE_TREE_PREVIEW_BIG as ::core::ffi::c_int;
    } else if args_has(&*args, 'N' as i32 as u_char) != 0 {
        mtd.preview = MODE_TREE_PREVIEW_OFF as ::core::ffi::c_int;
    } else {
        mtd.preview = MODE_TREE_PREVIEW_NORMAL as ::core::ffi::c_int;
    }
    mtd.sort_crit.order = sort_order_from_string(nullable_cstr(
        args_get(&*(args), 'O' as i32 as u_char).map_or(std::ptr::null(), |value| value.as_ptr()),
    ));
    mtd.sort_crit.reversed = args_has(&*args, 'r' as i32 as u_char);
    if args_has(&*args, 'f' as i32 as u_char) != 0 {
        mtd.filter = Some(
            args_get(&*(args), 'f' as i32 as u_char)
                .expect("argument is present")
                .to_owned(),
        );
    } else {
        mtd.filter = None;
    }
    mtd.buildcb = buildcb;
    mtd.drawcb = drawcb;
    mtd.searchcb = searchcb;
    mtd.menucb = menucb;
    mtd.heightcb = heightcb;
    mtd.keycb = keycb;
    mtd.swapcb = swapcb;
    mtd.sortcb = sortcb;
    mtd.helpcb = helpcb;
    *s = &raw mut mtd.screen;
    screen_init(&mut **s, pane_sx, pane_sy, 0 as u_int);
    (**s).mode &= !MODE_CURSOR;
    owner
}
unsafe fn mode_tree_set_height(tree_owner: &Rc<UnsafeCell<mode_tree_data>>) {
    let callback = (&mut *tree_owner.get()).heightcb.take();
    if let Some(mut callback) = callback {
        let sy = (&*tree_owner.get()).screen.grid().sy;
        let height = callback(sy);
        let mtd = &mut *tree_owner.get();
        if mtd.dead != 0 {
            return;
        }
        if mtd.heightcb.is_none() {
            mtd.heightcb = Some(callback);
        }
        if height < mtd.screen.grid().sy {
            mtd.height = mtd.screen.grid().sy.wrapping_sub(height);
        }
    } else {
        let mtd = &mut *tree_owner.get();
        if mtd.preview == MODE_TREE_PREVIEW_NORMAL as ::core::ffi::c_int {
            mtd.height = mtd
                .screen
                .grid()
                .sy
                .wrapping_div(3 as u_int)
                .wrapping_mul(2 as u_int);
            if mtd.height > mode_tree_line_count(mtd) {
                mtd.height = mtd.screen.grid().sy.wrapping_div(2 as u_int);
            }
            if mtd.height < 10 as u_int {
                mtd.height = mtd.screen.grid().sy;
            }
        } else if mtd.preview == MODE_TREE_PREVIEW_BIG as ::core::ffi::c_int {
            mtd.height = mtd.screen.grid().sy.wrapping_div(4 as u_int);
            if mtd.height > mode_tree_line_count(mtd) {
                mtd.height = mode_tree_line_count(mtd);
            }
            if mtd.height < 2 as u_int {
                mtd.height = 2 as u_int;
            }
        } else {
            mtd.height = mtd.screen.grid().sy;
        }
    }
    let mtd = &mut *tree_owner.get();
    if mtd.screen.grid().sy.wrapping_sub(mtd.height) < 2 as u_int {
        mtd.height = mtd.screen.grid().sy;
    }
}
unsafe fn mode_tree_call_build(
    tree_owner: &Rc<UnsafeCell<mode_tree_data>>,
    tag: Option<uint64_t>,
    filtered: bool,
) -> Option<uint64_t> {
    let (mut callback, mut sort, filter) = {
        let tree = &mut *tree_owner.get();
        (
            tree.buildcb.take().expect("non-null build callback"),
            tree.sort_crit,
            if filtered { tree.filter.clone() } else { None },
        )
    };
    let tag = callback(&mut sort, tag, filter.as_deref());
    let tree = &mut *tree_owner.get();
    if tree.dead == 0 {
        tree.sort_crit = sort;
        if tree.buildcb.is_none() {
            tree.buildcb = Some(callback);
        }
    }
    tag
}

pub unsafe fn mode_tree_build(tree_owner: &Rc<UnsafeCell<mode_tree_data>>) {
    let mut tag = {
        let tree = &mut *tree_owner.get();
        let tag = if tree.lines.is_empty() {
            None
        } else {
            Some(tree.lines[tree.current as usize].item.borrow().tag)
        };
        tree.saved.items.append(&mut tree.children.items);
        if let Some(sort) = tree.sortcb {
            sort(&mut tree.sort_crit);
        }
        tag
    };
    tag = mode_tree_call_build(tree_owner, tag, true);
    let retry = {
        let tree = &mut *tree_owner.get();
        if tree.dead != 0 {
            return;
        }
        tree.no_matches = tree.children.items.is_empty() as i32;
        tree.no_matches != 0
    };
    if retry {
        tag = mode_tree_call_build(tree_owner, tag, false);
    }
    let children = {
        let tree = &mut *tree_owner.get();
        if tree.dead != 0 {
            return;
        }
        mode_tree_free_items(&mut tree.saved);
        mode_tree_clear_lines(tree);
        tree.maxdepth = 0;
        tree.children.snapshot()
    };
    mode_tree_build_lines(tree_owner, &children, 0);
    let preview = {
        let tree = &mut *tree_owner.get();
        if tree.dead != 0 {
            return;
        }
        if !tree.lines.is_empty() && tag.is_none() {
            tag = Some(tree.lines[tree.current as usize].item.borrow().tag);
        }
        tree.width = tree.screen.grid().sx;
        if tree.preview == MODE_TREE_PREVIEW_OFF as ::core::ffi::c_int {
            tree.height = tree.screen.grid().sy;
            false
        } else {
            true
        }
    };
    if preview {
        mode_tree_set_height(tree_owner);
    }
    let tree = &mut *tree_owner.get();
    if tree.dead == 0 {
        mode_tree_set_current(tree, tag.unwrap_or(UINT64_MAX as uint64_t));
        mode_tree_check_selected(tree);
    }
}

pub unsafe fn mode_tree_free(owner: std::rc::Rc<std::cell::UnsafeCell<mode_tree_data>>) {
    mode_tree_clear_prompt(&owner);
    let mtd = &mut *owner.get();
    mode_tree_free_items(&mut mtd.children);
    mode_tree_clear_lines(&mut *mtd);
    drop(std::mem::take(&mut mtd.screen));
    mtd.search = None;
    mtd.filter = None;
    mtd.dead = 1 as ::core::ffi::c_int;
}
pub unsafe fn mode_tree_resize(
    tree_owner: &Rc<UnsafeCell<mode_tree_data>>,
    mut sx: u_int,
    mut sy: u_int,
) {
    let mtd = &mut *tree_owner.get();
    let Some(mode_pane_owner) =
        std::rc::Rc::<std::cell::UnsafeCell<window_pane>>::from_observer(&mtd.wp)
    else {
        return;
    };
    screen_resize(&mut mtd.screen, sx, sy, 0 as ::core::ffi::c_int);
    mode_tree_build(tree_owner);
    mode_tree_draw(tree_owner);
    mode_pane_owner.request_redraw(false);
}
pub fn mode_tree_add(
    mtd: &mut mode_tree_data,
    parent: Option<&ModeTreeItemRef>,
    itemdata: ModeTreeItemData,
    tag: uint64_t,
    name: &CStr,
    text: Option<&CStr>,
    expanded: i32,
) -> ModeTreeItemRef {
    log_debug(format_args!(
        "mode_tree_add: {tag}, {} {}",
        log_bytes(name.to_bytes()),
        log_bytes(text.unwrap_or(c"").to_bytes())
    ));
    let mut row = mode_tree_item {
        parent: parent.cloned().unwrap_or_default(),
        itemdata,
        tag,
        name: name.to_owned(),
        text: text.map(CStr::to_owned),
        ..mode_tree_item::empty()
    };
    if let Some(saved) = mode_tree_find_item(&mtd.saved, tag) {
        let saved = saved.borrow();
        if parent.is_none_or(|parent| parent.borrow().expanded != 0) {
            row.tagged = saved.tagged;
        }
        row.expanded = saved.expanded;
    } else {
        row.expanded = if expanded == -1 { 1 } else { expanded };
    }
    let owner = refbox::RefBox::new(row);
    let observer = ModeTreeItemRef::observe(&owner);
    if let Some(parent) = parent {
        parent.borrow_mut().children.items.push(owner);
    } else {
        mtd.children.items.push(owner);
    }
    observer
}
pub fn mode_tree_view_name(mtd: &mut mode_tree_data, name: Option<&'static CStr>) {
    mtd.view_name = name;
}
pub fn mode_tree_draw_as_parent(item: &ModeTreeItemRef) {
    item.borrow_mut().draw_as_parent = 1;
}
pub fn mode_tree_no_tag(item: &ModeTreeItemRef) {
    item.borrow_mut().no_tag = 1;
}
pub fn mode_tree_align(item: &ModeTreeItemRef) {
    item.borrow_mut().align = 1;
}
pub fn mode_tree_remove(mtd: &mut mode_tree_data, item: &ModeTreeItemRef) {
    let parent = item.borrow().parent.clone();
    if !parent.is_empty() {
        let mut parent = parent.borrow_mut();
        let position = parent.children.position(item);
        parent.children.items.remove(position);
    } else {
        let position = mtd.children.position(item);
        mtd.children.items.remove(position);
    }
    mtd.lines.retain(|line| line.item.is_alive());
    for (index, line) in mtd.lines.iter().enumerate() {
        line.item.borrow_mut().line = index as u_int;
    }
    mtd.current = mtd.current.min(mtd.lines.len().saturating_sub(1) as u_int);
    mtd.offset = mtd.offset.min(mtd.current);
}
fn mode_tree_append_printf_string(bytes: &mut Vec<u8>, value: Option<&CStr>) {
    if let Some(value) = value {
        bytes.extend_from_slice(value.to_bytes());
    } else {
        bytes.extend_from_slice(b"(null)");
    }
}
pub unsafe fn mode_tree_draw(tree_owner: &Rc<UnsafeCell<mode_tree_data>>) {
    let mtd = tree_owner.get();
    if (*mtd).dead != 0 {
        return;
    }
    let Some(mode_pane_owner) =
        std::rc::Rc::<std::cell::UnsafeCell<window_pane>>::from_observer(&(*mtd).wp)
    else {
        return;
    };
    let mut s: *mut screen = &raw mut (*mtd).screen;
    let options_window = mode_pane_owner.window_observer();
    let mut ctx: screen_write_ctx = screen_write_ctx {
        wp: std::rc::Weak::new(),
        target: Default::default(),
        flags: 0,
        init_ctx_cb: None,
        item: None,
        scrolled: 0,
        bg: 0,
    };
    let mut ft: *mut format_tree = ::core::ptr::null_mut::<format_tree>();
    let mut gc0: grid_cell = grid_cell {
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
    let mut box_gc: grid_cell = grid_cell {
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
    let mut w: u_int = 0;
    let mut h: u_int = 0;
    let mut i: u_int = 0;
    let mut sy: u_int = 0;
    let mut box_x: u_int = 0;
    let mut box_y: u_int = 0;
    let mut width: u_int = 0;
    let mut text_width: u_int = 0;
    let mut prefix_width: u_int = 0;
    let mut left: u_int = 0;
    let mut n: size_t = 0;
    let mut keylen: ::core::ffi::c_int = 0;
    let vla = (*mtd).maxdepth.wrapping_add(1 as u_int) as usize;
    let mut alignlen: Vec<::core::ffi::c_int> = ::std::vec::from_elem(0, vla);
    let mut dfg: ::core::ffi::c_int = 0;
    let mut dfg0: ::core::ffi::c_int = 0;
    if mode_tree_line_count(&*mtd) == 0 as u_int {
        return;
    }
    w = (*mtd).width;
    h = (*mtd).height;
    if w == 0 as u_int || h == 0 as u_int {
        return;
    }
    gc0 = grid_default_cell;
    gc = grid_default_cell;
    style_apply_with_options(
        &mut gc,
        c"tree-mode-selection-style",
        None,
        &OptionsScope::Window(options_window.clone()),
    );
    box_gc = grid_default_cell;
    style_apply_with_options(
        &mut box_gc,
        c"tree-mode-border-style",
        None,
        &OptionsScope::Window(options_window.clone()),
    );
    dfg = gc.fg;
    dfg0 = gc0.fg;
    screen_write_start(&mut ctx, s);
    screen_write_clearscreen(&mut ctx, 8 as u_int);
    let mut ft_owner = format_create_defaults(
        None,
        None,
        None,
        (refbox::Weak::new()).clone(),
        Some(&mode_pane_owner),
    );
    ft = &raw mut *ft_owner;
    keylen = 0 as ::core::ffi::c_int;
    i = 0 as u_int;
    while i < mode_tree_line_count(&*mtd) {
        let mti = (&(*mtd).lines)[i as usize].item.borrow();
        if !(mti.key == KEYC_NONE as ::core::ffi::c_ulong as key_code)
            && mti.keylen as ::core::ffi::c_int + 3 as ::core::ffi::c_int > keylen
        {
            keylen = mti.keylen.wrapping_add(3 as size_t) as ::core::ffi::c_int;
        }
        i = i.wrapping_add(1);
    }
    i = 0 as u_int;
    while i < (*mtd).maxdepth.wrapping_add(1 as u_int) {
        *alignlen.as_mut_ptr().offset(i as isize) = 0 as ::core::ffi::c_int;
        i = i.wrapping_add(1);
    }
    i = 0 as u_int;
    while i < mode_tree_line_count(&*mtd) {
        let line = (&(*mtd).lines)[i as usize].clone();
        let mti = line.item.borrow();
        if mti.align != 0
            && mti.name.as_bytes().len() as ::core::ffi::c_int
                > *alignlen.as_mut_ptr().offset(line.depth as isize)
        {
            *alignlen.as_mut_ptr().offset(line.depth as isize) =
                mti.name.as_bytes().len() as ::core::ffi::c_int;
        }
        i = i.wrapping_add(1);
    }
    i = 0 as u_int;
    while i < mode_tree_line_count(&*mtd) {
        if !(i < (*mtd).offset) {
            if i > (*mtd).offset.wrapping_add(h).wrapping_sub(1 as u_int) {
                break;
            }
            let line = (&(*mtd).lines)[i as usize].clone();
            let mti = line.item.borrow();
            screen_write_cursormove(
                &mut ctx,
                0 as ::core::ffi::c_int,
                i.wrapping_sub((*mtd).offset) as ::core::ffi::c_int,
                0 as ::core::ffi::c_int,
            );
            if mti.key != KEYC_NONE as ::core::ffi::c_ulong as key_code {
                format_add(ft, c"mode_tree_key", |out| {
                    write_cstr(out, (mti.keystr).as_deref())
                });
            } else {
                format_add(ft, c"mode_tree_key", |out| write_cstr(out, c""));
            }
            format_add(ft, c"mode_tree_key_width", |out| {
                write!(out, "{}", (keylen) as i32)
            });
            format_add(ft, c"mode_tree_selected", |out| {
                write!(
                    out,
                    "{}",
                    ((i == (*mtd).current) as ::core::ffi::c_int) as i32
                )
            });
            if line.depth == 0 as u_int {
                format_add(ft, c"mode_tree_repeat", |out| {
                    write!(out, "{}", (0 as ::core::ffi::c_int) as u32)
                });
                format_add(ft, c"mode_tree_branch", |out| out.write_all(b"0"));
                format_add(ft, c"mode_tree_parent_last", |out| out.write_all(b"0"));
            } else {
                format_add(ft, c"mode_tree_repeat", |out| {
                    write!(out, "{}", (line.depth.wrapping_sub(1 as u_int)) as u32)
                });
                format_add(ft, c"mode_tree_branch", |out| out.write_all(b"1"));
                if !mti.parent.is_empty()
                    && (&(*mtd).lines)[mti.parent.borrow().line as usize].last != 0
                {
                    format_add(ft, c"mode_tree_parent_last", |out| out.write_all(b"1"));
                } else {
                    format_add(ft, c"mode_tree_parent_last", |out| out.write_all(b"0"));
                }
            }
            if mti.children.items.is_empty() {
                format_add(ft, c"mode_tree_has_children", |out| out.write_all(b"0"));
            } else {
                format_add(ft, c"mode_tree_has_children", |out| out.write_all(b"1"));
            }
            format_add(ft, c"mode_tree_last", |out| {
                write!(out, "{}", (line.last) as i32)
            });
            format_add(ft, c"mode_tree_expanded", |out| {
                write!(out, "{}", (mti.expanded) as i32)
            });
            format_add(ft, c"mode_tree_flat", |out| {
                write!(out, "{}", (line.flat) as i32)
            });
            let prefix = format_expand_cstring(
                ft,
                c"#[fg=themelightgrey]#[bg=default]#[noacs]#{p/#{mode_tree_key_width}:#{?#{!=:#{mode_tree_key},},(#{mode_tree_key}),}}#{R:#{?mode_tree_parent_last,    ,#[acs]x#[fg=themelightgrey]#[bg=default]#[noacs]   },#{mode_tree_repeat}}#{?mode_tree_branch,#[acs]#{?mode_tree_last,mq,tq}+#[fg=themelightgrey]#[bg=default]#[noacs] ,}#{?mode_tree_has_children,#{?mode_tree_expanded,#[fg=themered]-#[fg=themelightgrey]#[bg=default]#[noacs] ,#[fg=themegreen]+#[fg=themelightgrey]#[bg=default]#[noacs] },#{?mode_tree_flat,,  }}".as_ptr(),
            );
            prefix_width = format_width(prefix.as_ptr());
            if prefix_width > w {
                prefix_width = w;
            }
            let tag: &[u8] = if mti.tagged != 0 { b"*" } else { b"" };
            let separator: &[u8] = if !mti.text.is_none() {
                b"#[fg=themelightgrey]: #[default]"
            } else {
                b""
            };
            let field_width = mti.align * *alignlen.as_mut_ptr().offset(line.depth as isize);
            let mut name = Vec::new();
            mode_tree_append_printf_string(&mut name, Some(mti.name.as_c_str()));
            let padding = (field_width.unsigned_abs() as usize).saturating_sub(name.len());
            let mut row = Vec::with_capacity(name.len() + padding + tag.len() + separator.len());
            if field_width >= 0 {
                row.extend(std::iter::repeat_n(b' ', padding));
            }
            row.extend_from_slice(&name);
            if field_width < 0 {
                row.extend(std::iter::repeat_n(b' ', padding));
            }
            row.extend_from_slice(tag);
            row.extend_from_slice(separator);
            let text = CString::new(row).expect("mode-tree row contains no NUL");
            text_width = format_width(text.as_ptr());
            left = if prefix_width < w {
                w.wrapping_sub(prefix_width)
            } else {
                0 as u_int
            };
            if text_width > left {
                text_width = left;
            }
            width = prefix_width.wrapping_add(text_width);
            if mti.tagged != 0 {
                gc.fg = COLOUR_THEME_CYAN as ::core::ffi::c_int | COLOUR_FLAG_THEME;
                gc0.fg = COLOUR_THEME_CYAN as ::core::ffi::c_int | COLOUR_FLAG_THEME;
            }
            if i != (*mtd).current {
                screen_write_clearendofline(&mut ctx, 8 as u_int);
                format_draw(
                    &raw mut ctx,
                    &grid_default_cell,
                    prefix_width,
                    prefix.as_ptr(),
                    ::core::ptr::null_mut::<style_ranges>(),
                    0 as ::core::ffi::c_int,
                );
                if left != 0 as u_int {
                    screen_write_cursormove(
                        &mut ctx,
                        prefix_width as ::core::ffi::c_int,
                        i.wrapping_sub((*mtd).offset) as ::core::ffi::c_int,
                        0 as ::core::ffi::c_int,
                    );
                    format_draw(
                        &raw mut ctx,
                        &gc0,
                        left,
                        text.as_ptr(),
                        ::core::ptr::null_mut::<style_ranges>(),
                        0 as ::core::ffi::c_int,
                    );
                    if !mti.text.is_none() && width < w {
                        screen_write_cursormove(
                            &mut ctx,
                            width as ::core::ffi::c_int,
                            i.wrapping_sub((*mtd).offset) as ::core::ffi::c_int,
                            0 as ::core::ffi::c_int,
                        );
                        format_draw(
                            &raw mut ctx,
                            &gc0,
                            w.wrapping_sub(width),
                            (mti.text)
                                .as_ref()
                                .map_or(::core::ptr::null_mut(), |value| value.as_ptr().cast_mut()),
                            ::core::ptr::null_mut::<style_ranges>(),
                            0 as ::core::ffi::c_int,
                        );
                    }
                }
            } else {
                screen_write_clearendofline(&mut ctx, gc.bg as u_int);
                format_draw(
                    &raw mut ctx,
                    &gc,
                    prefix_width,
                    prefix.as_ptr(),
                    ::core::ptr::null_mut::<style_ranges>(),
                    1 as ::core::ffi::c_int,
                );
                if left != 0 as u_int {
                    screen_write_cursormove(
                        &mut ctx,
                        prefix_width as ::core::ffi::c_int,
                        i.wrapping_sub((*mtd).offset) as ::core::ffi::c_int,
                        0 as ::core::ffi::c_int,
                    );
                    format_draw(
                        &raw mut ctx,
                        &gc,
                        left,
                        text.as_ptr(),
                        ::core::ptr::null_mut::<style_ranges>(),
                        1 as ::core::ffi::c_int,
                    );
                    if !mti.text.is_none() && width < w {
                        screen_write_cursormove(
                            &mut ctx,
                            width as ::core::ffi::c_int,
                            i.wrapping_sub((*mtd).offset) as ::core::ffi::c_int,
                            0 as ::core::ffi::c_int,
                        );
                        format_draw(
                            &raw mut ctx,
                            &gc,
                            w.wrapping_sub(width),
                            (mti.text)
                                .as_ref()
                                .map_or(::core::ptr::null_mut(), |value| value.as_ptr().cast_mut()),
                            ::core::ptr::null_mut::<style_ranges>(),
                            1 as ::core::ffi::c_int,
                        );
                    }
                }
            }
            drop(text);
            if mti.tagged != 0 {
                gc.fg = dfg;
                gc0.fg = dfg0;
            }
        }
        i = i.wrapping_add(1);
    }
    drop(ft_owner);
    if !((*mtd).preview == MODE_TREE_PREVIEW_OFF as ::core::ffi::c_int) {
        sy = (*s).grid().sy;
        if !(sy <= 4 as u_int
            || h < 2 as u_int
            || sy.wrapping_sub(h) <= 4 as u_int
            || w <= 4 as u_int)
        {
            let line = (&(*mtd).lines)[(*mtd).current as usize].clone();
            let mut owner = line.item.clone();
            if owner.borrow().draw_as_parent != 0 {
                let parent = owner.borrow().parent.clone();
                assert!(!parent.is_empty(), "preview row has a parent");
                owner = parent;
            }
            let mti = owner.borrow();
            screen_write_cursormove(
                &mut ctx,
                0 as ::core::ffi::c_int,
                h as ::core::ffi::c_int,
                0 as ::core::ffi::c_int,
            );
            screen_write_box(
                &mut ctx,
                w,
                sy.wrapping_sub(h),
                BOX_LINES_DEFAULT,
                Some(&box_gc),
                None,
            );
            let mut label_bytes = b" ".to_vec();
            mode_tree_append_printf_string(&mut label_bytes, Some(mti.name.as_c_str()));
            let order_seq = &(*mtd).sort_crit.order_seq;
            if !order_seq.is_empty() {
                label_bytes.extend_from_slice(b" (sort: ");
                let order = sort_order_to_string((*mtd).sort_crit.order);
                mode_tree_append_printf_string(&mut label_bytes, order);
                if (*mtd).sort_crit.reversed != 0 {
                    label_bytes.extend_from_slice(b", reversed");
                }
                label_bytes.push(b')');
                if let Some(view_name) = (*mtd).view_name {
                    label_bytes.extend_from_slice(b" (view: ");
                    mode_tree_append_printf_string(&mut label_bytes, Some(view_name));
                    label_bytes.push(b')');
                }
            }
            let label = CString::new(label_bytes).expect("preview label contains no NUL");
            let label_len = label.as_bytes().len() as size_t;
            if w.wrapping_sub(2 as u_int) as size_t >= label_len {
                screen_write_cursormove(
                    &mut ctx,
                    1 as ::core::ffi::c_int,
                    h as ::core::ffi::c_int,
                    0 as ::core::ffi::c_int,
                );
                screen_write_puts(&mut ctx, &box_gc, |out| write_cstr(out, &*label));
                if (*mtd).no_matches != 0 {
                    n = (::core::mem::size_of::<[::core::ffi::c_char; 11]>() as usize)
                        .wrapping_sub(1_usize) as size_t;
                } else {
                    n = (::core::mem::size_of::<[::core::ffi::c_char; 7]>() as usize)
                        .wrapping_sub(1_usize) as size_t;
                }
                if (*mtd).filter.is_some()
                    && w.wrapping_sub(2 as u_int) as size_t
                        >= label_len
                            .wrapping_add(10 as size_t)
                            .wrapping_add(n)
                            .wrapping_add(2 as size_t)
                {
                    screen_write_puts(&mut ctx, &box_gc, |out| out.write_all(b" (filter: "));
                    if (*mtd).no_matches != 0 {
                        screen_write_puts(&mut ctx, &box_gc, |out| out.write_all(b"no matches"));
                    } else {
                        screen_write_puts(&mut ctx, &box_gc, |out| out.write_all(b"active"));
                    }
                    screen_write_puts(&mut ctx, &box_gc, |out| out.write_all(b") "));
                } else {
                    screen_write_puts(&mut ctx, &box_gc, |out| out.write_all(b" "));
                }
            }
            drop(label);
            box_x = w.wrapping_sub(4 as u_int);
            box_y = sy.wrapping_sub(h).wrapping_sub(2 as u_int);
            if box_x != 0 as u_int && box_y != 0 as u_int {
                screen_write_cursormove(
                    &mut ctx,
                    2 as ::core::ffi::c_int,
                    h.wrapping_add(1 as u_int) as ::core::ffi::c_int,
                    0 as ::core::ffi::c_int,
                );
                let itemdata = mti.itemdata.clone();
                drop(mti);
                (*mtd).drawcb.as_mut().expect("non-null draw callback")(
                    &itemdata, &mut ctx, box_x, box_y,
                );
            }
        }
    }
    if (*mtd).help != 0 {
        mode_tree_draw_help(tree_owner, &raw mut ctx);
    }
    if (*mtd).prompt.is_some() {
        mode_tree_draw_prompt(tree_owner, &mut ctx);
    } else {
        (*s).mode &= !MODE_CURSOR;
        screen_write_cursormove(
            &mut ctx,
            0 as ::core::ffi::c_int,
            (*mtd).current.wrapping_sub((*mtd).offset) as ::core::ffi::c_int,
            0 as ::core::ffi::c_int,
        );
    }
    screen_write_stop(&mut ctx);
}
unsafe fn mode_tree_draw_prompt(
    tree_owner: &Rc<UnsafeCell<mode_tree_data>>,
    ctx: &mut screen_write_ctx,
) {
    let mtd = tree_owner.get();
    let mut s: *mut screen = &raw mut (*mtd).screen;
    let mut sx: u_int = (*s).grid().sx;
    let mut sy: u_int = (*s).grid().sy;
    let mut py: u_int = 0;
    if sx == 0 as u_int || sy == 0 as u_int {
        return;
    }
    if (*mtd).prompt_top != 0 {
        py = 0 as u_int;
    } else {
        py = sy.wrapping_sub(1 as u_int);
    }
    let pdd = prompt_draw_data {
        area_x: 0 as u_int,
        area_width: sx,
        prompt_line: py,
    };
    (*s).mode |= MODE_CURSOR;
    (*mtd).prompt_cx = prompt_draw(
        &(*mtd)
            .prompt
            .as_ref()
            .expect("active prompt")
            .try_borrow_mut()
            .expect("unborrowed prompt"),
        ctx,
        pdd,
    );
    screen_write_cursormove(
        ctx,
        (*mtd).prompt_cx as ::core::ffi::c_int,
        py as ::core::ffi::c_int,
        0 as ::core::ffi::c_int,
    );
}
pub unsafe fn mode_tree_clear_prompt(tree_owner: &Rc<UnsafeCell<mode_tree_data>>) {
    let mtd = tree_owner.get();
    if let Some(prompt) = (*mtd).prompt.take() {
        prompt_free(&prompt.downgrade());
        (*mtd).screen.mode &= !MODE_CURSOR;
    }
}
fn mode_tree_prompt_accept(tree: Rc<UnsafeCell<mode_tree_data>>) -> cmdq_cb {
    Some(Box::new(move |item| unsafe {
        let mtd = crate::src::shared::rc::as_ptr(&tree);
        let c_owner = cmdq_get_client(Some(&item.borrow()));
        let mut key = b'y' as key_code;
        if (*mtd).prompt.is_some() && c_owner.is_some() {
            mode_tree_key(
                tree.clone(),
                c_owner.as_ref(),
                &mut key,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            );
        }
        CmdOutcome::Normal
    }))
}
fn mode_tree_prompt_input_callback(
    data: &refbox::Weak<mode_tree_prompt>,
    input: Option<&CStr>,
    key: prompt_key_result,
) -> prompt_result {
    let (client, callback) = {
        let Ok(mut state) = data.try_borrow_mut() else {
            return PROMPT_CLOSE;
        };
        (state.c.upgrade(), state.inputcb.take())
    };
    let Some(mut callback) = callback else {
        return PROMPT_CLOSE;
    };
    let result = callback(client.as_ref(), input, key);
    if let Ok(mut state) = data.try_borrow_mut() {
        state.inputcb = Some(callback);
    }
    result
}
unsafe fn mode_tree_prompt_free_callback(
    data: &refbox::RefBox<crate::src::shared::mode_tree::mode_tree_prompt>,
) {
    let (mtd, callback, inputcb) = {
        let mut state = data.try_borrow_mut().expect("prompt cleanup record");
        (state.mtd.take(), state.freecb.take(), state.inputcb.take())
    };
    if let Some(mtd) = &mtd {
        let mtd = crate::src::shared::rc::as_ptr(mtd);
        if (*mtd).prompt_data.is(data) {
            (*mtd).prompt_data = refbox::Weak::new();
        }
    }
    if let Some(callback) = callback {
        callback();
    }
    drop(mtd);
    drop(inputcb);
}
pub unsafe fn mode_tree_set_prompt(
    tree: std::rc::Rc<std::cell::UnsafeCell<mode_tree_data>>,
    client_owner: Option<&ClientRef>,
    prompt: &CStr,
    input: Option<&CStr>,
    mut type_0: prompt_type,
    mut flags: ::core::ffi::c_int,
    mut inputcb: mode_tree_prompt_input_cb,
    mut freecb: prompt_free_cb,
) {
    let mtd = crate::src::shared::rc::as_ptr(&tree);
    let Some(mode_pane_owner) =
        std::rc::Rc::<std::cell::UnsafeCell<window_pane>>::from_observer(&(*mtd).wp)
    else {
        if let Some(freecb) = freecb {
            freecb();
        }
        return;
    };
    let session_owner = client_owner.and_then(|client| client.attached_session().upgrade());
    let _s = session_owner.clone();
    let mut pd = prompt_create_data::default();
    mode_tree_clear_prompt(&tree);
    let mtp = refbox::RefBox::new(mode_tree_prompt {
        mtd: Some(tree.clone()),
        c: client_owner.map(Rc::downgrade).unwrap_or_default(),
        inputcb,
        freecb,
    });
    let position = if let Some(session) = session_owner.as_ref() {
        options_get_number(&session.borrow_options_mut(), c"status-position")
    } else {
        OptionsScope::GlobalSession.with_local(|oo| options_get_number(oo, c"status-position"))
    };
    (*mtd).prompt_top = (position == 0) as i32;
    prompt_set_options(&mut pd, session_owner.as_ref());
    pd.prompt = prompt;
    pd.input = input;
    pd.type_0 = type_0;
    pd.flags = flags | PROMPT_ISMODE;
    let input_data = mtp.downgrade();
    let identity = input_data.clone();
    pd.inputcb = Some(Box::new(move |s, key| {
        mode_tree_prompt_input_callback(&input_data, s, key)
    }));
    let free_data = mtp;
    pd.freecb = Some(Box::new(move || unsafe {
        mode_tree_prompt_free_callback(&free_data)
    }));
    let prompt = prompt_create(pd);
    (*mtd).prompt = Some(prompt);
    (*mtd).prompt_data = identity;
    mode_tree_draw(&tree);
    mode_pane_owner.request_redraw(false);
    if flags & PROMPT_SINGLE != 0 && flags & PROMPT_ACCEPT != 0 && client_owner.is_some() {
        let tree = tree.clone();
        let item_allocation =
            cmdq_get_callback_owned(c"mode_tree_prompt_accept", mode_tree_prompt_accept(tree));
        cmdq_append(client_owner, item_allocation);
    }
}
unsafe fn mode_tree_search_backward(
    tree_owner: &Rc<UnsafeCell<mode_tree_data>>,
) -> Option<ModeTreeItemRef> {
    mode_tree_search(tree_owner, false)
}
unsafe fn mode_tree_search_forward(
    tree_owner: &Rc<UnsafeCell<mode_tree_data>>,
) -> Option<ModeTreeItemRef> {
    mode_tree_search(tree_owner, true)
}

fn mode_tree_search_next(mtd: &mode_tree_data, item: &ModeTreeItemRef) -> Option<ModeTreeItemRef> {
    if let Some(child) = item.borrow().children.first() {
        return Some(child);
    }
    let mut current = item.clone();
    loop {
        if let Some(next) = mode_tree_sibling(mtd, &current, true) {
            return Some(next);
        }
        let parent = current.borrow().parent.clone();
        if parent.is_empty() {
            return mtd.children.first();
        }
        current = parent;
    }
}

fn mode_tree_search_previous(
    mtd: &mode_tree_data,
    item: &ModeTreeItemRef,
) -> Option<ModeTreeItemRef> {
    let parent = item.borrow().parent.clone();
    let mut previous = if let Some(previous) = mode_tree_sibling(mtd, item, false) {
        previous
    } else if !parent.is_empty() {
        return Some(parent);
    } else {
        mtd.children.last()?
    };
    loop {
        let last = previous.borrow().children.last();
        let Some(last) = last else {
            return Some(previous);
        };
        previous = last;
    }
}

unsafe fn mode_tree_search(
    tree_owner: &Rc<UnsafeCell<mode_tree_data>>,
    forward: bool,
) -> Option<ModeTreeItemRef> {
    let (search, icase, last) = {
        let tree = &*tree_owner.get();
        (
            tree.search.clone()?,
            tree.search_icase != 0,
            tree.lines[tree.current as usize].item.clone(),
        )
    };
    let mut item = last.clone();
    loop {
        item = if forward {
            mode_tree_search_next(&*tree_owner.get(), &item)?
        } else {
            mode_tree_search_previous(&*tree_owner.get(), &item)?
        };
        if item == last {
            return None;
        }
        let (name, itemdata) = {
            let row = item.borrow();
            (row.name.clone(), row.itemdata.clone())
        };
        let callback = (&mut *tree_owner.get()).searchcb.take();
        let matched = if let Some(mut callback) = callback {
            let matched = callback(&itemdata, &search, icase);
            let tree = &mut *tree_owner.get();
            if tree.dead != 0 {
                return None;
            }
            if tree.searchcb.is_none() {
                tree.searchcb = Some(callback);
            }
            matched
        } else if icase {
            !strcasestr(name.as_ptr(), search.as_ptr()).is_null()
        } else {
            !strstr(name.as_ptr(), search.as_ptr()).is_null()
        };
        if (&*tree_owner.get()).dead != 0 || !item.is_alive() || !last.is_alive() {
            return None;
        }
        if matched {
            return Some(item);
        }
    }
}
unsafe fn mode_tree_search_set(tree_owner: &Rc<UnsafeCell<mode_tree_data>>) {
    let mtd = &mut *tree_owner.get();
    let Some(mode_pane_owner) =
        std::rc::Rc::<std::cell::UnsafeCell<window_pane>>::from_observer(&mtd.wp)
    else {
        return;
    };
    let found = if mtd.search_dir == MODE_TREE_SEARCH_FORWARD {
        mode_tree_search_forward(tree_owner)
    } else {
        mode_tree_search_backward(tree_owner)
    };
    let Some(item) = found else {
        return;
    };
    let tag = item.borrow().tag;
    let mut parent = item.borrow().parent.clone();
    while !parent.is_empty() {
        let owner = parent;
        let mut row = owner.borrow_mut();
        row.expanded = 1;
        parent = row.parent.clone();
    }
    mode_tree_build(tree_owner);
    let mtd = &mut *tree_owner.get();
    if mtd.dead != 0 {
        return;
    }
    mode_tree_set_current(mtd, tag);
    mode_tree_draw(tree_owner);
    mode_pane_owner.request_redraw(false);
}
fn mode_tree_observed_prompt_callback(
    tree: &Rc<UnsafeCell<mode_tree_data>>,
    callback: unsafe fn(
        &Rc<UnsafeCell<mode_tree_data>>,
        Option<&CStr>,
        prompt_key_result,
    ) -> prompt_result,
) -> mode_tree_prompt_input_cb {
    let observer = Rc::downgrade(tree);
    Some(Box::new(move |_, input, key| {
        let Some(owner) = observer.upgrade() else {
            return PROMPT_CLOSE;
        };
        unsafe {
            if (*owner.get()).dead != 0 {
                return PROMPT_CLOSE;
            }
            callback(&owner, input, key)
        }
    }))
}

unsafe fn mode_tree_search_callback(
    tree_owner: &Rc<UnsafeCell<mode_tree_data>>,
    s: Option<&CStr>,
    mut key: prompt_key_result,
) -> prompt_result {
    let mtd = &mut *tree_owner.get();
    if mtd.dead != 0 {
        return PROMPT_CLOSE;
    }
    let replacement = s
        .filter(|text| !text.to_bytes().is_empty())
        .map(CStr::to_owned);
    mtd.search = replacement;
    if let Some(search) = mtd.search.as_ref() {
        mtd.search_icase = mode_tree_is_lowercase(search.as_ptr());
        mode_tree_search_set(tree_owner);
    }
    if key as ::core::ffi::c_uint == PROMPT_KEY_HANDLED as ::core::ffi::c_int as ::core::ffi::c_uint
    {
        return PROMPT_CONTINUE;
    }
    PROMPT_CLOSE
}
unsafe fn mode_tree_filter_callback(
    tree_owner: &Rc<UnsafeCell<mode_tree_data>>,
    s: Option<&CStr>,
    mut key: prompt_key_result,
) -> prompt_result {
    let mtd = &mut *tree_owner.get();
    let Some(mode_pane_owner) =
        std::rc::Rc::<std::cell::UnsafeCell<window_pane>>::from_observer(&mtd.wp)
    else {
        return PROMPT_CLOSE;
    };
    if mtd.dead != 0 {
        return PROMPT_CLOSE;
    }
    let replacement = s
        .filter(|text| !text.to_bytes().is_empty())
        .map(CStr::to_owned);
    mtd.filter = replacement;
    mode_tree_build(tree_owner);
    mode_tree_draw(tree_owner);
    mode_pane_owner.request_redraw(false);
    if key as ::core::ffi::c_uint == PROMPT_KEY_HANDLED as ::core::ffi::c_int as ::core::ffi::c_uint
    {
        return PROMPT_CONTINUE;
    }
    PROMPT_CLOSE
}
unsafe fn mode_tree_clear_filter(tree_owner: &Rc<UnsafeCell<mode_tree_data>>) {
    let mtd = &mut *tree_owner.get();
    let Some(mode_pane_owner) =
        std::rc::Rc::<std::cell::UnsafeCell<window_pane>>::from_observer(&mtd.wp)
    else {
        return;
    };
    mtd.filter = None;
    mode_tree_build(tree_owner);
    mode_tree_draw(tree_owner);
    mode_pane_owner.request_redraw(false);
}
fn mode_tree_menu_callback(
    tree: Rc<UnsafeCell<mode_tree_data>>,
    client: ClientWeak,
    line: u_int,
) -> menu_choice_cb {
    Some(Box::new(move |selection| unsafe {
        let MenuSelection::Selected { key, .. } = selection else {
            return;
        };
        let mtd = &mut *tree.get();
        if mtd.dead != 0 || key == KEYC_NONE || line >= mode_tree_line_count(&*mtd) {
            return;
        }
        let Some(client) = client.upgrade() else {
            return;
        };
        if client.is_dead() {
            return;
        }
        mtd.current = line;
        let callback = mtd.menucb.take();
        if let Some(mut callback) = callback {
            callback(&client, key);
            let mtd = &mut *tree.get();
            if mtd.dead == 0 && mtd.menucb.is_none() {
                mtd.menucb = Some(callback);
            }
        }
    }))
}

unsafe fn mode_tree_display_menu(
    tree: &Rc<UnsafeCell<mode_tree_data>>,
    client_owner: Option<&ClientRef>,
    mut x: u_int,
    mut y: u_int,
    mut outside: ::core::ffi::c_int,
) {
    let Some(client_owner) = client_owner else {
        return;
    };
    let mtd = &*tree.get();
    let Some(mode_pane_owner) =
        std::rc::Rc::<std::cell::UnsafeCell<window_pane>>::from_observer(&mtd.wp)
    else {
        return;
    };
    let items: &[menu_item<'_>];
    let mut line: u_int = 0;
    if mtd.offset.wrapping_add(y) > mode_tree_line_count(mtd).wrapping_sub(1 as u_int) {
        line = mtd.current;
    } else {
        line = mtd.offset.wrapping_add(y);
    }

    let title = if outside == 0 {
        items = mtd.menu;
        let mut bytes = b"#[align=centre]".to_vec();
        bytes.extend_from_slice((&mtd.lines)[line as usize].item.borrow().name.to_bytes());
        CString::new(bytes).expect("mode tree item names contain no NUL")
    } else {
        items = &mode_tree_menu_items;
        c"".to_owned()
    };
    let mut menu = menu_create(&title);
    menu_add_items(&mut menu, items, Some(client_owner));
    drop(title);
    let client = Rc::downgrade(client_owner);
    if x >= menu.width.wrapping_add(4 as u_int).wrapping_div(2 as u_int) {
        x = x.wrapping_sub(menu.width.wrapping_add(4 as u_int).wrapping_div(2 as u_int));
    } else {
        x = 0 as u_int;
    }
    let (_, _, xoff, yoff) = mode_pane_owner.geometry();
    x = x.wrapping_add(xoff as u_int);
    y = y.wrapping_add(yoff as u_int);
    menu_display(
        menu,
        0 as ::core::ffi::c_int,
        0 as ::core::ffi::c_int,
        None,
        x,
        y,
        Some(client_owner),
        BOX_LINES_DEFAULT,
        None,
        None,
        None,
        ::core::ptr::null_mut::<cmd_find_state>(),
        mode_tree_menu_callback(tree.clone(), client, line),
    );
}
unsafe fn mode_tree_draw_help_line(
    mut ctx: *mut screen_write_ctx,
    gc: &grid_cell,
    mut ft: *mut format_tree,
    line: &CStr,
    item: &CStr,
    mut x: u_int,
    mut y: u_int,
    mut w: u_int,
) {
    let replaced = cmd_template_replace_cstring(line, item, 1 as ::core::ffi::c_int);
    let expanded = format_expand_cstring(ft, replaced.as_ptr());
    drop(replaced);
    screen_write_cursormove(
        &mut *ctx,
        x as ::core::ffi::c_int,
        y as ::core::ffi::c_int,
        0 as ::core::ffi::c_int,
    );
    screen_write_clearcharacter(&mut *ctx, w, gc.bg as u_int);
    screen_write_cursormove(
        &mut *ctx,
        x as ::core::ffi::c_int,
        y as ::core::ffi::c_int,
        0 as ::core::ffi::c_int,
    );
    format_draw(
        ctx,
        gc,
        w,
        expanded.as_ptr(),
        ::core::ptr::null_mut::<style_ranges>(),
        0 as ::core::ffi::c_int,
    );
}
unsafe fn mode_tree_draw_help(
    tree_owner: &Rc<UnsafeCell<mode_tree_data>>,
    mut ctx: *mut screen_write_ctx,
) {
    let mtd = tree_owner.get();
    let Some(mode_pane_owner) =
        std::rc::Rc::<std::cell::UnsafeCell<window_pane>>::from_observer(&(*mtd).wp)
    else {
        return;
    };
    let mut s: *mut screen = &raw mut (*mtd).screen;
    let options_window = mode_pane_owner.window_observer();
    let mut box_gc: grid_cell = grid_cell {
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
    let mut ft: *mut format_tree = ::core::ptr::null_mut::<format_tree>();
    let help = (*mtd).helpcb.map(|callback| callback());
    let lines: &[&'static CStr] = help.map_or(&[], |info| info.lines);
    let item = help.map_or(c"item", |info| info.item);
    let mut sx: u_int = (*s).grid().sx;
    let mut sy: u_int = (*s).grid().sy;
    let mut x: u_int = 0;
    let mut y: u_int = 0;
    let mut w = help
        .map_or(MODE_TREE_HELP_DEFAULT_WIDTH as u_int, |info| info.width)
        .max(MODE_TREE_HELP_DEFAULT_WIDTH as u_int);
    let h = (mode_tree_help_start.len() + lines.len() + mode_tree_help_end.len()) as u_int;
    let mut box_w: u_int = 0;
    let mut box_h: u_int = 0;
    box_w = w.wrapping_add(2 as u_int);
    box_h = h.wrapping_add(2 as u_int);
    if sx < box_w || sy < box_h {
        return;
    }
    x = sx.wrapping_sub(box_w).wrapping_div(2 as u_int);
    y = sy.wrapping_sub(box_h).wrapping_div(2 as u_int);
    box_gc = grid_default_cell;
    style_apply_with_options(
        &mut box_gc,
        c"tree-mode-border-style",
        None,
        &OptionsScope::Window(options_window.clone()),
    );
    gc = grid_default_cell;
    let mut ft_owner = format_create_defaults(
        None,
        None,
        None,
        (refbox::Weak::new()).clone(),
        Some(&mode_pane_owner),
    );
    ft = &raw mut *ft_owner;
    screen_write_cursormove(
        &mut *ctx,
        x as ::core::ffi::c_int,
        y as ::core::ffi::c_int,
        0 as ::core::ffi::c_int,
    );
    screen_write_box(
        &mut *ctx,
        box_w,
        box_h,
        BOX_LINES_DEFAULT,
        Some(&box_gc),
        None,
    );
    y = y.wrapping_add(1);
    x = x.wrapping_add(1);
    for line in mode_tree_help_start.iter().copied() {
        mode_tree_draw_help_line(ctx, &gc, ft, line, item, x, y, w);
        y = y.wrapping_add(1);
    }
    for line in lines.iter().copied() {
        mode_tree_draw_help_line(ctx, &gc, ft, line, item, x, y, w);
        y = y.wrapping_add(1);
    }
    for line in mode_tree_help_end.iter().copied() {
        mode_tree_draw_help_line(ctx, &gc, ft, line, item, x, y, w);
        y = y.wrapping_add(1);
    }
    drop(ft_owner);
}
unsafe fn mode_tree_display_help(tree_owner: &Rc<UnsafeCell<mode_tree_data>>) {
    let mtd = tree_owner.get();
    (*mtd).help = 1 as ::core::ffi::c_int;
    mode_tree_draw(tree_owner);
}
pub unsafe fn mode_tree_key(
    tree: std::rc::Rc<std::cell::UnsafeCell<mode_tree_data>>,
    client_owner: Option<&ClientRef>,
    mut key: *mut key_code,
    mut m: *mut mouse_event,
    mut xp: *mut u_int,
    mut yp: *mut u_int,
) -> ::core::ffi::c_int {
    let mtd = crate::src::shared::rc::as_ptr(&tree);
    let Some(mode_pane_owner) =
        std::rc::Rc::<std::cell::UnsafeCell<window_pane>>::from_observer(&(*mtd).wp)
    else {
        *key = KEYC_NONE;
        return 0;
    };
    let mut i: u_int = 0;
    let mut x: u_int = 0;
    let mut y: u_int = 0;
    let mut py: u_int = 0;
    let mut sx: u_int = 0;
    let mut choice: ::core::ffi::c_int = 0;
    let mut preview: ::core::ffi::c_int = 0;
    let mut result: prompt_key_result = PROMPT_KEY_NOT_HANDLED;
    let mut redraw: ::core::ffi::c_int = 0;
    if mode_tree_line_count(&*mtd) == 0 as u_int {
        *key = KEYC_NONE as ::core::ffi::c_ulong as key_code;
        return 1 as ::core::ffi::c_int;
    }
    if let Some(prompt) = (*mtd).prompt.as_ref().map(|prompt| prompt.downgrade()) {
        redraw = 0 as ::core::ffi::c_int;
        let mtp = (*mtd).prompt_data.clone();
        if !mtp.is_empty() {
            mtp.try_borrow_mut().expect("live prompt callback record").c =
                client_owner.map(Rc::downgrade).unwrap_or_default();
        }
        if *key & KEYC_MASK_KEY == KEYC_MOUSE as ::core::ffi::c_ulong as ::core::ffi::c_ulonglong
            || *key & KEYC_MASK_TYPE
                >= (KEYC_TYPE_MOUSEMOVE as ::core::ffi::c_int as ::core::ffi::c_ulonglong)
                    << 32 as ::core::ffi::c_int
                && *key & KEYC_MASK_TYPE
                    <= (KEYC_TYPE_TRIPLECLICK as ::core::ffi::c_int as ::core::ffi::c_ulonglong)
                        << 32 as ::core::ffi::c_int
        {
            if m.is_null()
                || (*m).b & MOUSE_MASK_BUTTONS as u_int != MOUSE_BUTTON_1 as u_int
                || (*m).b & MOUSE_MASK_DRAG as u_int != 0
                || (*m).b & MOUSE_MASK_BUTTONS as u_int == 3 as u_int
                || cmd_mouse_at(
                    &mode_pane_owner,
                    m,
                    &raw mut x,
                    &raw mut y,
                    0 as ::core::ffi::c_int,
                ) != 0 as ::core::ffi::c_int
            {
                result = PROMPT_KEY_NOT_HANDLED;
            } else {
                sx = (*mtd).screen.grid().sx;
                if (*mtd).prompt_top != 0 {
                    py = 0 as u_int;
                } else {
                    py = (*mtd).screen.grid().sy.wrapping_sub(1 as u_int);
                }
                if y == py {
                    result = prompt_mouse(
                        &mut prompt.try_borrow_mut().expect("live unborrowed prompt"),
                        x,
                        0 as u_int,
                        sx,
                        Some(&mut redraw),
                    );
                } else {
                    result = PROMPT_KEY_NOT_HANDLED;
                }
            }
        } else {
            result = prompt_key(&prompt, *key, &mut redraw);
        }
        if (*mtd).dead != 0 {
            *key = KEYC_NONE;
            return 0;
        }
        if !mtp.is_empty() && (*mtd).prompt_data == mtp {
            mtp.try_borrow_mut().expect("live prompt callback record").c = Weak::new();
        }
        if (*mtd)
            .prompt
            .as_ref()
            .is_some_and(|current| prompt.is(current))
            && (result as ::core::ffi::c_uint
                == PROMPT_KEY_CLOSE as ::core::ffi::c_int as ::core::ffi::c_uint
                || prompt_closed(&prompt.try_borrow_mut().expect("live unborrowed prompt")) != 0)
        {
            mode_tree_clear_prompt(&tree);
        }
        if (*mtd).dead != 0 {
            *key = KEYC_NONE;
            return 0;
        }
        if redraw != 0
            || !(*mtd)
                .prompt
                .as_ref()
                .is_some_and(|current| prompt.is(current))
        {
            mode_tree_draw(&tree);
            mode_pane_owner.request_redraw(false);
        }
        if result as ::core::ffi::c_uint
            != PROMPT_KEY_NOT_HANDLED as ::core::ffi::c_int as ::core::ffi::c_uint
        {
            *key = KEYC_NONE as ::core::ffi::c_ulong as key_code;
            return 0 as ::core::ffi::c_int;
        }
    }
    if (*mtd).help != 0 {
        if *key & KEYC_MASK_KEY == KEYC_MOUSE as ::core::ffi::c_ulong as ::core::ffi::c_ulonglong
            || *key & KEYC_MASK_TYPE
                >= (KEYC_TYPE_MOUSEMOVE as ::core::ffi::c_int as ::core::ffi::c_ulonglong)
                    << 32 as ::core::ffi::c_int
                && *key & KEYC_MASK_TYPE
                    <= (KEYC_TYPE_TRIPLECLICK as ::core::ffi::c_int as ::core::ffi::c_ulonglong)
                        << 32 as ::core::ffi::c_int
        {
            *key = KEYC_NONE as ::core::ffi::c_ulong as key_code;
            return 0 as ::core::ffi::c_int;
        }
        if *key == KEYC_FOCUS_IN as ::core::ffi::c_ulong as key_code
            || *key == KEYC_FOCUS_OUT as ::core::ffi::c_ulong as key_code
        {
            *key = KEYC_NONE as ::core::ffi::c_ulong as key_code;
            return 0 as ::core::ffi::c_int;
        }
        (*mtd).help = 0 as ::core::ffi::c_int;
        mode_tree_draw(&tree);
        *key = KEYC_NONE as ::core::ffi::c_ulong as key_code;
        return 0 as ::core::ffi::c_int;
    }
    if (*key & KEYC_MASK_KEY == KEYC_MOUSE as ::core::ffi::c_ulong as ::core::ffi::c_ulonglong
        || *key & KEYC_MASK_TYPE
            >= (KEYC_TYPE_MOUSEMOVE as ::core::ffi::c_int as ::core::ffi::c_ulonglong)
                << 32 as ::core::ffi::c_int
            && *key & KEYC_MASK_TYPE
                <= (KEYC_TYPE_TRIPLECLICK as ::core::ffi::c_int as ::core::ffi::c_ulonglong)
                    << 32 as ::core::ffi::c_int)
        && !m.is_null()
    {
        if cmd_mouse_at(
            &mode_pane_owner,
            m,
            &raw mut x,
            &raw mut y,
            0 as ::core::ffi::c_int,
        ) != 0 as ::core::ffi::c_int
        {
            *key = KEYC_NONE as ::core::ffi::c_ulong as key_code;
            return 0 as ::core::ffi::c_int;
        }
        if !xp.is_null() {
            *xp = x;
        }
        if !yp.is_null() {
            *yp = y;
        }
        if x > (*mtd).width || y > (*mtd).height {
            preview = (*mtd).preview;
            if *key == KEYC_MOUSEDOWN3_PANE as ::core::ffi::c_ulong as key_code {
                mode_tree_display_menu(&tree, client_owner, x, y, 1 as ::core::ffi::c_int);
            }
            if preview == MODE_TREE_PREVIEW_OFF as ::core::ffi::c_int {
                *key = KEYC_NONE as ::core::ffi::c_ulong as key_code;
            }
            return 0 as ::core::ffi::c_int;
        }
        if (*mtd).offset.wrapping_add(y) < mode_tree_line_count(&*mtd) {
            if *key == KEYC_MOUSEDOWN1_PANE as ::core::ffi::c_ulong as key_code
                || *key == KEYC_MOUSEDOWN3_PANE as ::core::ffi::c_ulong as key_code
                || *key == KEYC_DOUBLECLICK1_PANE as ::core::ffi::c_ulong as key_code
            {
                (*mtd).current = (*mtd).offset.wrapping_add(y);
            }
            if *key == KEYC_DOUBLECLICK1_PANE as ::core::ffi::c_ulong as key_code {
                *key = '\r' as i32 as key_code;
            } else {
                if *key == KEYC_MOUSEDOWN3_PANE as ::core::ffi::c_ulong as key_code {
                    mode_tree_display_menu(&tree, client_owner, x, y, 0 as ::core::ffi::c_int);
                }
                *key = KEYC_NONE as ::core::ffi::c_ulong as key_code;
            }
        } else {
            if *key == KEYC_MOUSEDOWN3_PANE as ::core::ffi::c_ulong as key_code {
                mode_tree_display_menu(&tree, client_owner, x, y, 0 as ::core::ffi::c_int);
            }
            *key = KEYC_NONE as ::core::ffi::c_ulong as key_code;
        }
        return 0 as ::core::ffi::c_int;
    }
    let line = (&(*mtd).lines)[(*mtd).current as usize].clone();
    let current = &line.item;
    choice = -(1 as ::core::ffi::c_int);
    i = 0 as u_int;
    while i < mode_tree_line_count(&*mtd) {
        if *key == (&(*mtd).lines)[i as usize].item.borrow().key {
            choice = i as ::core::ffi::c_int;
            break;
        } else {
            i = i.wrapping_add(1);
        }
    }
    if choice != -(1 as ::core::ffi::c_int) {
        if choice as u_int > mode_tree_line_count(&*mtd).wrapping_sub(1 as u_int) {
            *key = KEYC_NONE as ::core::ffi::c_ulong as key_code;
            return 0 as ::core::ffi::c_int;
        }
        (*mtd).current = choice as u_int;
        *key = '\r' as i32 as key_code;
        return 0 as ::core::ffi::c_int;
    }
    match *key {
        113 | 27 | 35184372088923 | 35184372088935 => return 1 as ::core::ffi::c_int,
        8589934600 | 35184372088936 => {
            mode_tree_display_help(&tree);
        }
        8589934619 | 107 | 38654705664 | 35184372088944 => {
            mode_tree_up(&mut *mtd, 1 as ::core::ffi::c_int);
        }
        8589934620 | 106 | 34359738368 | 35184372088942 => {
            mode_tree_down(&mut *mtd, 1 as ::core::ffi::c_int);
        }
        70377334112283 | 75 => {
            mode_tree_swap(&tree, -(1 as ::core::ffi::c_int));
        }
        70377334112284 | 74 => {
            mode_tree_swap(&tree, 1 as ::core::ffi::c_int);
        }
        8589934617 | 35184372088930 => {
            i = 0 as u_int;
            while i < (*mtd).height {
                if (*mtd).current == 0 as u_int {
                    break;
                }
                mode_tree_up(&mut *mtd, 1 as ::core::ffi::c_int);
                i = i.wrapping_add(1);
            }
        }
        8589934616 | 35184372088934 => {
            i = 0 as u_int;
            while i < (*mtd).height {
                if (*mtd).current == mode_tree_line_count(&*mtd).wrapping_sub(1 as u_int) {
                    break;
                }
                mode_tree_down(&mut *mtd, 1 as ::core::ffi::c_int);
                i = i.wrapping_add(1);
            }
        }
        103 | 8589934614 => {
            (*mtd).current = 0 as u_int;
            (*mtd).offset = 0 as u_int;
        }
        71 | 8589934615 => {
            (*mtd).current = mode_tree_line_count(&*mtd).wrapping_sub(1 as u_int);
            if (*mtd).current > (*mtd).height.wrapping_sub(1 as u_int) {
                (*mtd).offset = (*mtd)
                    .current
                    .wrapping_sub((*mtd).height)
                    .wrapping_add(1 as u_int);
            } else {
                (*mtd).offset = 0 as u_int;
            }
        }
        116 => {
            let mut row = current.borrow_mut();
            if row.no_tag == 0 {
                if row.tagged == 0 {
                    let mut parent = row.parent.clone();
                    while !parent.is_empty() {
                        let owner = parent;
                        let mut row = owner.borrow_mut();
                        row.tagged = 0;
                        parent = row.parent.clone();
                    }
                    mode_tree_clear_tagged(&row.children);
                    row.tagged = 1;
                } else {
                    row.tagged = 0;
                }
                drop(row);
                if !m.is_null() {
                    mode_tree_down(&mut *mtd, 0);
                }
            }
        }
        84 => {
            for line in &(*mtd).lines {
                line.item.borrow_mut().tagged = 0;
            }
        }
        35184372088948 => {
            for line in &(*mtd).lines {
                let mut row = line.item.borrow_mut();
                row.tagged = if !row.parent.is_empty() {
                    (row.parent.borrow().no_tag != 0) as i32
                } else {
                    (row.no_tag == 0) as i32
                };
            }
        }
        79 => {
            sort_next_order(&raw mut (*mtd).sort_crit);
            mode_tree_build(&tree);
        }
        114 => {
            (*mtd).sort_crit.reversed = ((*mtd).sort_crit.reversed == 0) as ::core::ffi::c_int;
            mode_tree_build(&tree);
        }
        8589934621 | 104 | 45 => {
            let target = if line.flat != 0 || current.borrow().expanded == 0 {
                current.borrow().parent.clone()
            } else {
                current.clone()
            };
            if !target.is_empty() {
                let mut row = target.borrow_mut();
                row.expanded = 0;
                (*mtd).current = row.line;
                drop(row);
                mode_tree_build(&tree);
            } else {
                mode_tree_up(&mut *mtd, 0);
            }
        }
        8589934622 | 108 | 43 => {
            if line.flat != 0 || current.borrow().expanded != 0 {
                mode_tree_down(&mut *mtd, 0);
            } else {
                current.borrow_mut().expanded = 1;
                mode_tree_build(&tree);
            }
        }
        17592186044461 => {
            for item in &(*mtd).children.items {
                item.try_borrow_mut().expect("unborrowed row").expanded = 0;
            }
            mode_tree_build(&tree);
        }
        17592186044459 => {
            for item in &(*mtd).children.items {
                item.try_borrow_mut().expect("unborrowed row").expanded = 1;
            }
            mode_tree_build(&tree);
        }
        63 | 47 | 35184372088947 => {
            (*mtd).search_dir = MODE_TREE_SEARCH_FORWARD;
            mode_tree_set_prompt(
                tree.clone(),
                client_owner,
                c"(search) ",
                Some(c""),
                PROMPT_TYPE_SEARCH,
                PROMPT_NOFORMAT,
                mode_tree_observed_prompt_callback(&tree, mode_tree_search_callback),
                None,
            );
        }
        110 => {
            (*mtd).search_dir = MODE_TREE_SEARCH_FORWARD;
            mode_tree_search_set(&tree);
        }
        78 => {
            (*mtd).search_dir = MODE_TREE_SEARCH_BACKWARD;
            mode_tree_search_set(&tree);
        }
        102 => {
            mode_tree_set_prompt(
                tree.clone(),
                client_owner,
                c"(filter) ",
                Some((*mtd).filter.as_deref().unwrap_or(c"")),
                PROMPT_TYPE_SEARCH,
                PROMPT_NOFORMAT,
                mode_tree_observed_prompt_callback(&tree, mode_tree_filter_callback),
                None,
            );
        }
        99 => {
            mode_tree_clear_prompt(&tree);
            mode_tree_clear_filter(&tree);
        }
        118 => {
            match (*mtd).preview {
                0 => {
                    (*mtd).preview = MODE_TREE_PREVIEW_BIG as ::core::ffi::c_int;
                }
                1 => {
                    (*mtd).preview = MODE_TREE_PREVIEW_OFF as ::core::ffi::c_int;
                }
                2 => {
                    (*mtd).preview = MODE_TREE_PREVIEW_NORMAL as ::core::ffi::c_int;
                }
                _ => {}
            }
            mode_tree_build(&tree);
            if (*mtd).preview != MODE_TREE_PREVIEW_OFF as ::core::ffi::c_int {
                mode_tree_check_selected(&mut *mtd);
            }
        }
        _ => {}
    }
    0 as ::core::ffi::c_int
}
pub unsafe fn mode_tree_run_command(
    client_owner: Option<&ClientRef>,
    fs: Option<&cmd_find_state>,
    template: &CStr,
    name: &CStr,
) {
    let command = cmd_template_replace_cstring(template, name, 1);
    if command.as_bytes().is_empty() {
        return;
    }
    let state = cmdq_new_state(
        fs.map_or(std::ptr::null_mut(), |fs| std::ptr::from_ref(fs).cast_mut()),
        std::ptr::null_mut(),
        0,
    );
    if let Err(mut error) = cmd_parse_and_append(&command, client_owner, Some(&state)) {
        if let Some(owner) = client_owner {
            cmd_parse_error_uppercase_first(&mut error);
            status_message_set(Some(owner), -1, 1, 0, 0, |out| {
                write_cstr(out, error.as_deref())
            });
        }
    }
}

#[cfg(test)]
mod mode_tree_tests {
    use super::*;

    #[test]
    fn child_owners_preserve_search_order_and_survive_removal() {
        unsafe {
            let mtd_owner = mode_tree_alloc_data();
            let mtd = crate::src::shared::rc::as_ptr(&mtd_owner);
            let add = move |parent, tag, name: &CStr| {
                mode_tree_add(
                    &mut *mtd,
                    parent,
                    ModeTreeItemData::None,
                    tag,
                    name,
                    None,
                    1,
                )
            };
            let first = add(None, 1, c"match first");
            let branch = add(None, 2, c"branch");
            let child = add(Some(&branch), 3, c"match child");
            let tail = add(None, 4, c"match tail");
            for tag in 5..100 {
                add(None, tag, c"other");
            }
            branch.borrow_mut().expanded = 0;
            mode_tree_build_lines(&mtd_owner, &(*mtd).children.snapshot(), 0);
            (*mtd).search = Some(c"match".to_owned());
            (*mtd).current = first.borrow().line;
            assert!(mode_tree_search_forward(&mtd_owner).unwrap() == child);
            assert!(mode_tree_search_backward(&mtd_owner).unwrap() == tail);
            (*mtd).current = tail.borrow().line;
            assert!(mode_tree_search_backward(&mtd_owner).unwrap() == child);
            assert!(mode_tree_search_forward(&mtd_owner).unwrap() == first);
            (*mtd).search = Some(c"missing".to_owned());
            assert!(mode_tree_search_forward(&mtd_owner).is_none());
            assert!(mode_tree_search_backward(&mtd_owner).is_none());

            mode_tree_remove(&mut *mtd, &child);
            assert!(branch.borrow().children.items.is_empty());
            add(Some(&branch), 100, c"replacement child");
            mode_tree_remove(&mut *mtd, &branch);
            mode_tree_remove(&mut *mtd, &first);
            let last = (*mtd).children.last().unwrap();
            mode_tree_remove(&mut *mtd, &last);
            assert!((*mtd).children.first().unwrap() == tail);
            assert!(mode_tree_find_item(&(*mtd).children, 100).is_none());
            mode_tree_clear_lines(&mut *mtd);
            mode_tree_build_lines(&mtd_owner, &(*mtd).children.snapshot(), 0);
            assert_eq!((*mtd).lines.len(), 95);
            assert_eq!((*mtd).lines.last().unwrap().last, 1);
            drop(mtd_owner);
        }
    }

    #[test]
    fn item_key_label_tracks_repeated_line_builds() {
        unsafe {
            let mtd_owner = mode_tree_alloc_data();
            let mtd = crate::src::shared::rc::as_ptr(&mtd_owner);
            let mut key = b'x' as key_code;
            let key_ptr = &raw const key;
            (*mtd).keycb = Some(Box::new(move |_, _| *key_ptr));
            let item = mode_tree_add(&mut *mtd, None, ModeTreeItemData::None, 1, c"row", None, 1);
            for (next, expected) in [
                (b'x' as key_code, Some(b"x".as_slice())),
                (KEYC_NONE, None),
                (b'y' as key_code, Some(b"y".as_slice())),
            ] {
                key = next;
                mode_tree_clear_lines(&mut *mtd);
                mode_tree_build_lines(&mtd_owner, &(*mtd).children.snapshot(), 0);
                match expected {
                    Some(expected) => {
                        assert_eq!(
                            (item.borrow().keystr)
                                .as_deref()
                                .expect("string is present")
                                .to_bytes(),
                            expected
                        );
                        assert_eq!(item.borrow().keylen, expected.len());
                    }
                    None => {
                        assert!(item.borrow().keystr.is_none());
                        assert_eq!(item.borrow().keylen, 0);
                    }
                }
            }
            mode_tree_free_items(&mut (*mtd).children);
            mode_tree_clear_lines(&mut *mtd);
            drop(mtd_owner);
        }
    }

    unsafe fn nested_build(tree: &Rc<UnsafeCell<mode_tree_data>>, empty: bool) {
        if empty {
            return;
        }
        let mtd = &mut *tree.get();
        let parent = mode_tree_add(mtd, None, ModeTreeItemData::None, 1, c"parent", None, 1);
        for id in 0..64 {
            mode_tree_add(
                mtd,
                Some(&parent),
                ModeTreeItemData::None,
                id as u64 + 2,
                c"child",
                None,
                1,
            );
        }
    }

    #[test]
    fn nested_lines_survive_growth_and_clear_on_empty_rebuild() {
        unsafe {
            let mtd_owner = mode_tree_alloc_data();
            let mtd = crate::src::shared::rc::as_ptr(&mtd_owner);
            (*mtd).preview = MODE_TREE_PREVIEW_OFF as ::core::ffi::c_int;
            let empty = Rc::new(std::cell::Cell::new(false));
            let callback_empty = empty.clone();
            let callback_tree = Rc::downgrade(&mtd_owner);
            (*mtd).buildcb = Some(Box::new(move |_, _, _| {
                let tree = callback_tree
                    .upgrade()
                    .expect("live mode tree during build");
                nested_build(&tree, callback_empty.get());
                None
            }));
            (*mtd).screen.grid = Some(crate::src::grid::grid_create(80, 24, 0));

            mode_tree_build(&mtd_owner);
            assert_eq!((*mtd).lines.len(), 65);
            assert_eq!((&(*mtd).lines)[0].depth, 0);
            for i in 1..65 {
                assert_eq!((&(*mtd).lines)[i].depth, 1);
                assert_eq!((&(*mtd).lines)[i].item.borrow().line, i as u_int);
            }
            assert_eq!(mode_tree_set_current(&mut *mtd, 65), 1);
            assert_eq!((*mtd).current, 64);

            empty.set(true);
            mode_tree_build(&mtd_owner);
            assert!((*mtd).lines.is_empty());
            mode_tree_free_items(&mut (*mtd).children);
            drop((*mtd).screen.grid.take());
            drop(mtd_owner);
        }
    }

    #[test]
    fn build_callback_can_replace_itself_and_mutate_filter() {
        unsafe {
            let owner = mode_tree_alloc_data();
            let observer = Rc::downgrade(&owner);
            let callback_tree = observer.clone();
            let tree = &mut *owner.get();
            tree.filter = Some(c"original".to_owned());
            tree.buildcb = Some(Box::new(move |sort, tag, filter| {
                assert_eq!(tag, Some(7));
                assert_eq!(filter, Some(c"original"));
                let owner = callback_tree.upgrade().unwrap();
                let tree = &mut *owner.get();
                assert!(tree.buildcb.is_none());
                tree.filter = Some(c"replacement".to_owned());
                tree.buildcb = Some(Box::new(|_, _, filter| {
                    assert!(filter.is_none());
                    Some(9)
                }));
                sort.reversed = 1;
                assert_eq!(filter, Some(c"original"));
                Some(8)
            }));
            assert_eq!(mode_tree_call_build(&owner, Some(7), true), Some(8));
            assert_eq!((&*owner.get()).sort_crit.reversed, 1);
            assert_eq!((&*owner.get()).filter.as_deref(), Some(c"replacement"));
            assert_eq!(mode_tree_call_build(&owner, None, false), Some(9));
            drop(owner);
            assert!(observer.upgrade().is_none());
        }
    }

    #[test]
    fn numeric_tags_restore_saved_row_state() {
        unsafe {
            let mtd_owner = mode_tree_alloc_data();
            let mtd = crate::src::shared::rc::as_ptr(&mtd_owner);
            let prior = mode_tree_add(&mut *mtd, None, ModeTreeItemData::None, 7, c"row", None, 1);
            prior.borrow_mut().tagged = 1;
            prior.borrow_mut().expanded = 0;
            (*mtd).saved = std::mem::take(&mut (*mtd).children);

            let restored =
                mode_tree_add(&mut *mtd, None, ModeTreeItemData::None, 7, c"row", None, 1);
            let different = mode_tree_add(
                &mut *mtd,
                None,
                ModeTreeItemData::None,
                8,
                c"other",
                None,
                1,
            );
            assert_eq!(
                {
                    let row = restored.borrow();
                    (row.tagged, row.expanded)
                },
                (1, 0)
            );
            assert_eq!(
                {
                    let row = different.borrow();
                    (row.tagged, row.expanded)
                },
                (0, 1)
            );

            mode_tree_free_items(&mut (*mtd).children);
            mode_tree_free_items(&mut (*mtd).saved);
            drop(mtd_owner);
        }
    }
}

#[cfg(test)]
mod mode_prompt_data_tests {
    use super::*;
    use crate::src::shared::rc;
    use std::cell::{Cell, UnsafeCell};

    fn data(
        tree: &Rc<UnsafeCell<mode_tree_data>>,
    ) -> refbox::RefBox<crate::src::shared::mode_tree::mode_tree_prompt> {
        refbox::RefBox::new(mode_tree_prompt {
            mtd: Some(tree.clone()),
            c: Weak::new(),
            inputcb: None,
            freecb: None,
        })
    }

    #[test]
    fn cleanup_releases_tree_ownership_without_clearing_replacement_data() {
        unsafe {
            let tree = mode_tree_alloc_data();
            let mtd = rc::as_ptr(&tree);
            let weak_tree = Rc::downgrade(&tree);
            let old = data(&tree);
            let replacement = data(&tree);
            (*mtd).prompt_data = replacement.downgrade();
            let frees = Rc::new(Cell::new(0));
            let count = frees.clone();
            let weak_old = old.downgrade();
            let observed = weak_old.clone();
            old.try_borrow_mut().unwrap().freecb = Some(Box::new(move || {
                assert!(observed.try_borrow_mut().unwrap().mtd.is_none());
                count.set(count.get() + 1);
            }));
            assert_eq!(Rc::strong_count(&tree), 3);
            mode_tree_prompt_free_callback(&old);
            assert_eq!(Rc::strong_count(&tree), 2);
            assert!(old.try_borrow_mut().unwrap().mtd.is_none());
            assert!((*mtd).prompt_data.is(&replacement));
            mode_tree_prompt_free_callback(&old);
            assert_eq!(frees.get(), 1);
            assert_eq!(Rc::strong_count(&tree), 2);
            mode_tree_prompt_free_callback(&replacement);
            assert!((*mtd).prompt_data.is_empty());
            assert_eq!(Rc::strong_count(&tree), 1);
            drop(tree);
            assert!(weak_tree.upgrade().is_none());
            drop(old);
            assert!(!weak_old.is_alive());
        }
    }
}

#[cfg(test)]
mod queued_prompt_accept_tests {
    use super::*;

    #[test]
    fn observed_prompt_callbacks_retain_only_during_live_dispatch() {
        use std::sync::atomic::{AtomicU32, Ordering};
        static CALLS: AtomicU32 = AtomicU32::new(0);
        unsafe fn callback(
            tree: &Rc<UnsafeCell<mode_tree_data>>,
            _: Option<&CStr>,
            _: prompt_key_result,
        ) -> prompt_result {
            assert_eq!(Rc::strong_count(tree), 2);
            CALLS.fetch_add(1, Ordering::Relaxed);
            PROMPT_CONTINUE
        }
        unsafe {
            let tree = mode_tree_alloc_data();
            let observer = Rc::downgrade(&tree);
            let mut input = mode_tree_observed_prompt_callback(&tree, callback).unwrap();
            assert_eq!(Rc::strong_count(&tree), 1);
            assert_eq!(
                input(None, None, PROMPT_KEY_HANDLED) as u32,
                PROMPT_CONTINUE
            );
            assert_eq!(CALLS.load(Ordering::Relaxed), 1);
            (*tree.get()).dead = 1;
            assert_eq!(input(None, None, PROMPT_KEY_HANDLED) as u32, PROMPT_CLOSE);
            assert_eq!(CALLS.load(Ordering::Relaxed), 1);
            drop(tree);
            assert!(observer.upgrade().is_none());
            assert_eq!(input(None, None, PROMPT_KEY_HANDLED) as u32, PROMPT_CLOSE);
        }
    }
}

#[cfg(test)]
mod row_owner_tests {
    use super::*;
    use crate::src::shared::rc;

    #[test]
    fn row_observers_expire_and_borrowed_data_and_names_survive_teardown() {
        unsafe {
            let tree_owner = mode_tree_alloc_data();
            let tree = crate::src::shared::rc::as_ptr(&tree_owner);
            let observer = Rc::downgrade(&tree_owner);
            let parent = mode_tree_add(
                &mut *tree,
                None,
                ModeTreeItemData::None,
                1,
                c"parent",
                None,
                1,
            );
            let name = CString::new(b"child \xff".to_vec()).unwrap();
            let text = CString::new("description").unwrap();
            let child = mode_tree_add(
                &mut *tree,
                Some(&parent),
                ModeTreeItemData::None,
                2,
                &name,
                Some(&text),
                1,
            );
            drop(name);
            drop(text);
            let parent_observer = parent.clone();
            let child_observer = child.clone();
            mode_tree_build_lines(&tree_owner, &(*tree).children.snapshot(), 0);
            (*tree).current = 1;
            let selected = (&(*tree).lines)[1].clone();
            let name = mode_tree_get_current_name(&*tree);
            let borrowed = selected.item.borrow();
            drop(parent);
            drop(child);
            mode_tree_free(tree_owner);
            assert!(observer.upgrade().is_none());
            assert!(!parent_observer.is_alive());
            assert!(!borrowed.parent.is_alive());
            assert_eq!(borrowed.text.as_deref(), Some(c"description"));
            assert!(!child_observer.is_alive());
            drop(borrowed);
            drop(selected);
            assert!(!child_observer.is_alive());
            assert_eq!(name.to_bytes(), b"child \xff");
            drop(name);
        }
    }

    #[test]
    fn key_callback_can_remove_its_row_and_expire_the_traversal_snapshot() {
        unsafe {
            let owner = mode_tree_alloc_data();
            let tree = rc::as_ptr(&owner);
            let first = mode_tree_add(
                &mut *tree,
                None,
                ModeTreeItemData::None,
                1,
                c"first",
                None,
                1,
            );
            let second = mode_tree_add(
                &mut *tree,
                None,
                ModeTreeItemData::None,
                2,
                c"second",
                None,
                1,
            );
            let removed = first.clone();
            let mut once = false;
            (*tree).keycb = Some(Box::new(move |_, _| {
                if !once {
                    once = true;
                    mode_tree_remove(&mut *tree, &removed);
                    assert!(!removed.is_alive());
                }
                b'x' as key_code
            }));
            mode_tree_build_lines(&owner, &(*tree).children.snapshot(), 0);
            assert!(!first.is_alive());
            assert_eq!((*tree).lines.len(), 1);
            assert!((&(*tree).lines)[0].item == second);
            assert_eq!(second.borrow().key, b'x' as key_code);
        }
    }

    #[test]
    fn search_callback_cannot_return_a_removed_row() {
        unsafe {
            let owner = mode_tree_alloc_data();
            let tree = rc::as_ptr(&owner);
            mode_tree_add(
                &mut *tree,
                None,
                ModeTreeItemData::None,
                1,
                c"first",
                None,
                1,
            );
            let second = mode_tree_add(
                &mut *tree,
                None,
                ModeTreeItemData::None,
                2,
                c"second",
                None,
                1,
            );
            mode_tree_build_lines(&owner, &(*tree).children.snapshot(), 0);
            (*tree).search = Some(c"query".to_owned());
            let removed = second.clone();
            (*tree).searchcb = Some(Box::new(move |_, query, _| {
                mode_tree_remove(&mut *tree, &removed);
                (*tree).search = None;
                assert_eq!(query, c"query");
                true
            }));
            assert!(mode_tree_search_forward(&owner).is_none());
            assert!(!second.is_alive());
        }
    }

    #[test]
    fn tagged_callback_can_destroy_the_tree_and_stop_dispatch() {
        unsafe {
            let tree_owner = mode_tree_alloc_data();
            let tree = crate::src::shared::rc::as_ptr(&tree_owner);
            let observer = Rc::downgrade(&tree_owner);
            let first = mode_tree_add(
                &mut *tree,
                None,
                ModeTreeItemData::None,
                1,
                c"first",
                None,
                1,
            );
            first.borrow_mut().tagged = 1;
            mode_tree_build_lines(&tree_owner, &(*tree).children.snapshot(), 0);
            let mut calls = 0;
            let mut tree_owner = Some(tree_owner);
            mode_tree_each_tagged(
                &observer.upgrade().expect("live tree"),
                |row, _| {
                    calls += 1;
                    mode_tree_free(tree_owner.take().unwrap());
                    assert!(!row.is_alive());
                },
                KEYC_NONE,
                1,
            );
            assert_eq!(calls, 1);
            assert!(!first.is_alive());
            assert!(observer.upgrade().is_none());
        }
    }

    #[test]
    fn build_callback_can_destroy_its_tree() {
        unsafe {
            let owner = mode_tree_alloc_data();
            let observer = Rc::downgrade(&owner);
            let tree = owner.get();
            let calls = Rc::new(std::cell::Cell::new(0));
            let callback_calls = calls.clone();
            let mut callback_owner = Some(owner);
            (*tree).buildcb = Some(Box::new(move |_, _, _| {
                callback_calls.set(callback_calls.get() + 1);
                mode_tree_free(callback_owner.take().expect("first build invocation"));
                None
            }));
            mode_tree_build(&observer.upgrade().expect("tree retained by callback"));
            assert_eq!(calls.get(), 1);
            assert!(observer.upgrade().is_none());
        }
    }

    #[test]
    fn height_callback_can_close_tree_without_accessing_freed_screen() {
        unsafe {
            let tree = mode_tree_alloc_data();
            (*tree.get()).screen.grid = Some(crate::src::grid::grid_create(80, 24, 0));
            let observer = Rc::downgrade(&tree);
            (*tree.get()).heightcb = Some(Box::new(move |height| {
                assert_eq!(height, 24);
                mode_tree_free(observer.upgrade().unwrap());
                5
            }));
            mode_tree_set_height(&tree);
            assert_eq!((*tree.get()).dead, 1);
            assert!((*tree.get()).screen.grid.is_none());
            assert!((*tree.get()).heightcb.is_none());
            assert_eq!(Rc::strong_count(&tree), 1);
        }
    }

    #[test]
    fn height_callback_preserves_replacement_callback() {
        unsafe {
            let tree = mode_tree_alloc_data();
            (*tree.get()).screen.grid = Some(crate::src::grid::grid_create(80, 24, 0));
            let observer = Rc::downgrade(&tree);
            (*tree.get()).heightcb = Some(Box::new(move |_| {
                (*observer.upgrade().unwrap().get()).heightcb = Some(Box::new(|_| 7));
                5
            }));
            mode_tree_set_height(&tree);
            assert_eq!((*tree.get()).height, 19);
            mode_tree_set_height(&tree);
            assert_eq!((*tree.get()).height, 17);
        }
    }

    #[test]
    fn tag_callbacks_can_clear_rows_without_borrowing_or_lifetime_conflicts() {
        for tagged in [false, true] {
            unsafe {
                let owner = mode_tree_alloc_data();
                let tree = rc::as_ptr(&owner);
                let first = mode_tree_add(
                    &mut *tree,
                    None,
                    ModeTreeItemData::None,
                    1,
                    c"first",
                    None,
                    1,
                );
                first.borrow_mut().tagged = tagged as i32;
                let second = mode_tree_add(
                    &mut *tree,
                    None,
                    ModeTreeItemData::None,
                    2,
                    c"second",
                    None,
                    1,
                );
                second.borrow_mut().tagged = tagged as i32;
                let first_observer = first.clone();
                let second_observer = second.clone();
                mode_tree_build_lines(&owner, &(*tree).children.snapshot(), 0);
                drop(first);
                drop(second);
                let mut calls = 0;
                mode_tree_each_tagged(
                    &owner,
                    |row, _| {
                        calls += 1;
                        row.borrow_mut().tagged = 0;
                        let name = row.borrow().name.clone();
                        mode_tree_clear_lines(&mut *tree);
                        mode_tree_free_items(&mut (*tree).children);
                        assert!(!first_observer.is_alive());
                        assert!(!second_observer.is_alive());
                        assert!(row.try_borrow().is_none());
                        assert_eq!(name.as_c_str(), c"first");
                    },
                    KEYC_NONE,
                    1,
                );
                assert_eq!(calls, 1);
                assert!(!first_observer.is_alive());
            }
        }
    }
}

#[cfg(test)]
mod payload_owner_tests {
    use super::*;
    use crate::src::shared::rc;
    use crate::src::window_buffer::window_buffer_itemdata;

    fn payload(name: &CStr) -> refbox::RefBox<window_buffer_itemdata> {
        refbox::RefBox::new(window_buffer_itemdata {
            name: name.to_owned(),
            order: 0,
            size: 0,
        })
    }

    #[test]
    fn row_payloads_expire_on_list_replacement_but_action_snapshots_survive() {
        unsafe {
            let tree_owner = mode_tree_alloc_data();
            let tree = crate::src::shared::rc::as_ptr(&tree_owner);
            let owner = payload(c"original");
            let original = ModeTreeItemData::Buffer(owner.downgrade());
            let row = mode_tree_add(&mut *tree, None, original.clone(), 1, c"row", None, 1);
            mode_tree_add(
                &mut *tree,
                Some(&row),
                original.clone(),
                2,
                c"detail",
                None,
                1,
            );
            mode_tree_build_lines(&tree_owner, &(*tree).children.snapshot(), 0);
            let selected = mode_tree_get_current(&*tree);
            (*tree).current = 1;
            let detail = mode_tree_get_current(&*tree);
            assert!(selected.same_identity(&detail));
            let snapshot = selected.as_buffer().unwrap();
            assert_eq!(detail.as_buffer().unwrap().name.as_c_str(), c"original");
            drop(owner);
            assert!(selected.as_buffer().is_none());
            assert!(detail.as_buffer().is_none());
            mode_tree_clear_lines(&mut *tree);
            mode_tree_free_items(&mut (*tree).children);
            let owner = payload(c"replacement");
            let replacement = ModeTreeItemData::Buffer(owner.downgrade());
            assert!(!selected.same_identity(&replacement));
            assert!(!replacement.is_buffer(&snapshot));
            assert!(selected.is_buffer(&snapshot));
            mode_tree_free(tree_owner);
            assert_eq!(snapshot.name.as_c_str(), c"original");
        }
    }

    #[test]
    fn key_dispatch_snapshot_survives_removing_its_row_and_payload_owner() {
        unsafe {
            let tree_owner = mode_tree_alloc_data();
            let tree = rc::as_ptr(&tree_owner);
            let mut owner = Some(payload(c"callback snapshot"));
            let observer = owner.as_ref().unwrap().downgrade();
            let original = ModeTreeItemData::Buffer(observer.clone());
            let row = mode_tree_add(&mut *tree, None, original, 1, c"row", None, 1);
            let callback_row = row.clone();
            (*tree).keycb = Some(Box::new(move |item, _| {
                let snapshot = item.as_buffer().unwrap();
                callback_row.borrow_mut().itemdata = ModeTreeItemData::None;
                drop(owner.take());
                assert!(item.as_buffer().is_none());
                assert_eq!(snapshot.name.as_c_str(), c"callback snapshot");
                b'x' as key_code
            }));
            mode_tree_build_lines(&tree_owner, &(*tree).children.snapshot(), 0);
            assert_eq!(row.borrow().key, b'x' as key_code);
            assert!(matches!(
                mode_tree_get_current(&*tree),
                ModeTreeItemData::None
            ));
            assert!(!observer.is_alive());
        }
    }
}
