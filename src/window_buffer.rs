use crate::arguments::{args, args_count, args_get, args_has, args_string};
use crate::format::{
    format_add, format_create, format_defaults, format_defaults_paste_buffer,
    format_expand_cstring, format_true,
};
use crate::grid::grid_default_cell;
use crate::key_string::key_string_parse_cstr;
use crate::mode_tree::ModeTreeItemSnapshot;
use crate::mode_tree::{
    mode_tree_build, mode_tree_draw, mode_tree_each_tagged, mode_tree_free, mode_tree_key,
    mode_tree_resize, mode_tree_run_command, mode_tree_start, mode_tree_visit_screen,
};
use crate::mode_tree::{mode_tree_data, mode_tree_help_info, ModeTreeItemData};
use crate::paste::{
    paste_buffer_data, paste_buffer_name, paste_buffer_order, paste_free, paste_get_name,
    paste_is_empty, paste_replace_owned,
};
use crate::screen_write::{screen_write_cursormove, screen_write_nputs};
use crate::shared::abi::ssize_t;
use crate::shared::abi::*;
use crate::shared::client::ClientRef;
use crate::shared::command::{cmd_find_state, cmdq_item};
use crate::shared::format::format_tree;
use crate::shared::format::FORMAT_NONE;
use crate::shared::key::*;
use crate::shared::menu::menu_item;
use crate::shared::mouse::mouse_event;
use crate::shared::pane::window_pane;
use crate::shared::paste::PasteBufferRef;
use crate::shared::screen::screen;
use crate::shared::screen_write::screen_write_ctx;
use crate::shared::session::SessionRef;
use crate::shared::sort::sort_criteria;
use crate::shared::sort::*;
use crate::shared::spawn::{spawn_editor_state, EditorHandle};
use crate::shared::vis::{VIS_CSTYLE, VIS_OCTAL, VIS_TAB};
use crate::shared::window::{window_mode, window_mode_entry, winlink};
use crate::sort::sort_get_buffers;
use crate::spawn::{spawn_editor, spawn_editor_write};
use crate::text::utf8::utf8_strvis;
use crate::window_pane::WindowPane as _;
use libc::{strcasestr, strstr};
use std::cell::UnsafeCell;
use std::ffi::{CStr, CString};
use std::ptr::NonNull;
use std::rc::{Rc, Weak};

#[repr(C)]
pub struct window_buffer_modedata {
    pub wp: Weak<UnsafeCell<window_pane>>,
    pub fs: cmd_find_state,
    pub data: Option<std::rc::Rc<std::cell::RefCell<mode_tree_data>>>,
    pub editor: Option<EditorHandle>,
    pub command: CString,
    pub format: CString,
    pub key_format: CString,
    item_list: Vec<refbox::RefBox<window_buffer_itemdata>>,
}

impl window_buffer_modedata {
    fn tree_owner(&self) -> Rc<std::cell::RefCell<mode_tree_data>> {
        self.data.as_ref().expect("mode tree owner").clone()
    }
}

#[repr(C)]
#[derive(Clone)]
pub struct window_buffer_itemdata {
    pub name: std::ffi::CString,
    pub order: u_int,
    pub size: size_t,
}

pub struct window_buffer_editdata {
    pub wp_id: u_int,
    pub name: Option<::std::ffi::CString>,
    pub pb: PasteBufferRef,
}

pub const WINDOW_BUFFER_DEFAULT_COMMAND: &std::ffi::CStr = c"paste-buffer -p -b '%%'";
pub const WINDOW_BUFFER_DEFAULT_FORMAT: &CStr = c"#{t/p:buffer_created}: #{buffer_sample}";
pub const WINDOW_BUFFER_DEFAULT_KEY_FORMAT: &std::ffi::CStr =
    c"#{?#{e|<:#{line},10},#{line},#{e|<:#{line},36},M-#{a:#{e|+:97,#{e|-:#{line},10}}}}";
static window_buffer_menu_items: [menu_item<'static>; 11] = [
    menu_item {
        name: c"Paste",
        key: 'p' as i32 as key_code,
        command: None,
    },
    menu_item {
        name: c"Paste Tagged",
        key: 'P' as i32 as key_code,
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
        name: c"Delete",
        key: 'd' as i32 as key_code,
        command: None,
    },
    menu_item {
        name: c"Delete Tagged",
        key: 'D' as i32 as key_code,
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
pub static window_buffer_mode: window_mode = {
    window_mode {
        name: c"buffer-mode",
        default_format: Some(WINDOW_BUFFER_DEFAULT_FORMAT),
        flags: 0,
        init: Some(
            window_buffer_init
                as unsafe fn(
                    refbox::Weak<window_mode_entry>,
                    Option<&std::rc::Rc<std::cell::RefCell<cmdq_item>>>,
                    *mut cmd_find_state,
                    *mut args,
                ) -> bool,
        ),
        free: Some(window_buffer_free as unsafe fn(refbox::Weak<window_mode_entry>) -> ()),
        resize: Some(
            window_buffer_resize as unsafe fn(refbox::Weak<window_mode_entry>, u_int, u_int) -> (),
        ),
        update: Some(window_buffer_update as unsafe fn(refbox::Weak<window_mode_entry>) -> ()),
        style_changed: None,
        key: Some(
            window_buffer_key
                as unsafe fn(
                    refbox::Weak<window_mode_entry>,
                    &ClientRef,
                    refbox::Weak<winlink>,
                    key_code,
                    *mut mouse_event,
                ) -> (),
        ),
        key_table: None,
        command: None,
        formats: None,
        get_screen: None,
        display_screen: Some(window_buffer_get_screen),
    }
};
static window_buffer_order_seq: [sort_order; 3] = [SORT_CREATION, SORT_NAME, SORT_SIZE];
fn window_buffer_add_item(
    items: &mut Vec<refbox::RefBox<window_buffer_itemdata>>,
    name: &CStr,
    order: u_int,
    size: size_t,
) -> refbox::Weak<window_buffer_itemdata> {
    let item = refbox::RefBox::new(window_buffer_itemdata {
        name: name.to_owned(),
        order,
        size,
    });
    let handle = item.downgrade();
    items.push(item);
    handle
}

fn window_buffer_clear_items(items: &mut Vec<refbox::RefBox<window_buffer_itemdata>>) {
    items.clear();
}
unsafe fn window_buffer_build(
    data: *mut window_buffer_modedata,
    mut sort_crit: *mut sort_criteria,
    filter: Option<&CStr>,
) {
    let filter: *const ::core::ffi::c_char = filter.map_or(std::ptr::null(), CStr::as_ptr);
    let mut i: u_int = 0;
    let mut ft: *mut format_tree = ::core::ptr::null_mut::<format_tree>();
    let mut s: Option<SessionRef> = None;
    let mut wl: refbox::Weak<winlink> = refbox::Weak::new();
    let mut pane_owner = None;
    window_buffer_clear_items(&mut (*data).item_list);
    let buffers = sort_get_buffers(&*sort_crit);
    for pb in buffers {
        let buffer = pb.borrow();
        let name = paste_buffer_name(&buffer);
        window_buffer_add_item(
            &mut (*data).item_list,
            name,
            paste_buffer_order(&buffer),
            buffer.size(),
        );
    }
    if (*data).fs.is_valid() {
        s = (*data).fs.session_handle();
        wl = (*data).fs.winlink_handle();
        pane_owner = (*data).fs.pane_handle();
    }
    let mut current_block_32: u64;
    i = 0 as u_int;
    while (i as usize) < (*data).item_list.len() {
        let item_handle = (&(*data).item_list)[i as usize].downgrade();
        let item = ModeTreeItemData::Buffer(item_handle.clone())
            .as_buffer()
            .unwrap();
        if let Some(pb) = paste_get_name(&item.name) {
            let mut ft_owner = format_create(None, None, FORMAT_NONE, 0 as ::core::ffi::c_int);
            ft = &raw mut *ft_owner;
            format_defaults(ft, None, s.as_ref(), wl.clone(), pane_owner.as_ref());
            format_defaults_paste_buffer(&mut *ft, &pb);
            if !filter.is_null() {
                let cp = format_expand_cstring(ft, filter);
                if format_true(cp.as_ptr()) == 0 {
                    current_block_32 = 5948590327928692120;
                } else {
                    current_block_32 = 1608152415753874203;
                }
            } else {
                current_block_32 = 1608152415753874203;
            }
            match current_block_32 {
                5948590327928692120 => {}
                _ => {
                    let text = format_expand_cstring(ft, (*data).format.as_ptr());
                    (*data).tree_owner().borrow_mut().add(
                        None,
                        ModeTreeItemData::Buffer(item_handle.clone()),
                        item.order as uint64_t,
                        &item.name,
                        Some(&text),
                        -(1 as ::core::ffi::c_int),
                    );
                }
            }
            drop(ft_owner);
        }
        i = i.wrapping_add(1);
    }
}
unsafe fn window_buffer_draw(
    item: &window_buffer_itemdata,
    ctx: &mut screen_write_ctx,
    mut sx: u_int,
    mut sy: u_int,
) {
    let cx = ctx.screen.cx;
    let cy = ctx.screen.cy;
    let Some(pb) = paste_get_name(&item.name) else {
        return;
    };
    let buffer = pb.borrow();
    let data = paste_buffer_data(&buffer).unwrap_or_default();
    let mut buf = Vec::<u8>::new();
    for (row, line) in data
        .split(|&byte| byte == b'\n')
        .take(sy as usize)
        .enumerate()
    {
        buf.resize(4 * (line.len() + 1), 0);
        utf8_strvis(&mut buf, line, VIS_OCTAL | VIS_CSTYLE | VIS_TAB);
        let escaped = CStr::from_bytes_until_nul(&buf).expect("escaped line is terminated");
        if !escaped.is_empty() {
            screen_write_cursormove(ctx, cx as i32, cy.wrapping_add(row as u_int) as i32, 0);
            screen_write_nputs(ctx, sx as ssize_t, &grid_default_cell, |out| {
                out.write_all(escaped.to_bytes())
            });
        }
    }
}
fn window_buffer_find(data: &[u8], find: &[u8], icase: bool) -> bool {
    if find.is_empty() {
        return false;
    }
    data.windows(find.len()).any(|candidate| {
        if icase {
            candidate.iter().zip(find).all(|(&a, &b)| unsafe {
                libc::tolower(i32::from(a)) == libc::tolower(i32::from(b))
            })
        } else {
            candidate == find
        }
    })
}
unsafe fn window_buffer_search(item: &window_buffer_itemdata, search: &CStr, icase: bool) -> bool {
    let Some(buffer) = paste_get_name(&item.name) else {
        return false;
    };
    let name_match = if icase {
        strcasestr(item.name.as_ptr(), search.as_ptr())
    } else {
        strstr(item.name.as_ptr(), search.as_ptr())
    };
    if !name_match.is_null() {
        return true;
    }
    let buffer = buffer.borrow();
    window_buffer_find(
        paste_buffer_data(&buffer).unwrap_or_default(),
        search.to_bytes(),
        icase,
    )
}
unsafe fn window_buffer_menu(data: *mut window_buffer_modedata, c: &ClientRef, mut key: key_code) {
    let Some(mode_pane_owner) =
        std::rc::Rc::<std::cell::UnsafeCell<window_pane>>::from_observer(&(*data).wp)
    else {
        return;
    };
    let mut wme: refbox::Weak<window_mode_entry> = refbox::Weak::new();
    wme = mode_pane_owner.mode_entry();
    if !wme.is_alive()
        || wme
            .get_unchecked()
            .boxed_data_ptr::<window_buffer_modedata>()
            != Some(data)
    {
        return;
    }
    window_buffer_key(
        wme.clone(),
        c,
        (refbox::Weak::new()).clone(),
        key,
        ::core::ptr::null_mut::<mouse_event>(),
    );
}
unsafe fn window_buffer_get_key(
    data: *mut window_buffer_modedata,
    item: &window_buffer_itemdata,
    mut line: u_int,
) -> key_code {
    let mut ft: *mut format_tree = ::core::ptr::null_mut::<format_tree>();
    let mut s: Option<SessionRef> = None;
    let mut wl: refbox::Weak<winlink> = refbox::Weak::new();
    let mut pane_owner = None;
    let mut key: key_code = 0;
    if (*data).fs.is_valid() {
        s = (*data).fs.session_handle();
        wl = (*data).fs.winlink_handle();
        pane_owner = (*data).fs.pane_handle();
    }
    let Some(pb) = paste_get_name(&item.name) else {
        return KEYC_NONE;
    };
    let mut ft_owner = format_create(None, None, FORMAT_NONE, 0 as ::core::ffi::c_int);
    ft = &raw mut *ft_owner;
    format_defaults(ft, None, None, (refbox::Weak::new()).clone(), None);
    format_defaults(ft, None, s.as_ref(), wl.clone(), pane_owner.as_ref());
    format_defaults_paste_buffer(&mut *ft, &pb);
    format_add(ft, c"line", |out| write!(out, "{}", { line }));
    let expanded = format_expand_cstring(ft, (*data).key_format.as_ptr());
    key = key_string_parse_cstr(expanded.as_c_str()).unwrap_or(KEYC_UNKNOWN);
    drop(ft_owner);
    key
}
fn window_buffer_sort(sort_crit: &mut sort_criteria) {
    {
        sort_crit.order_seq = &window_buffer_order_seq;
        if sort_crit.order as ::core::ffi::c_uint
            == SORT_END as ::core::ffi::c_int as ::core::ffi::c_uint
        {
            sort_crit.order = sort_crit.order_seq[0];
        }
    }
}
static window_buffer_help_lines: &[&CStr] = &[
    c"#[fg=themelightgrey]      Enter #[#{E:tree-mode-border-style},acs]x#[default] Paste selected %1",
    c"#[fg=themelightgrey]          p #[#{E:tree-mode-border-style},acs]x#[default] Paste selected %1",
    c"#[fg=themelightgrey]          P #[#{E:tree-mode-border-style},acs]x#[default] Paste tagged %1s",
    c"#[fg=themelightgrey]          d #[#{E:tree-mode-border-style},acs]x#[default] Delete selected %1",
    c"#[fg=themelightgrey]          D #[#{E:tree-mode-border-style},acs]x#[default] Delete tagged %1s",
    c"#[fg=themelightgrey]          e #[#{E:tree-mode-border-style},acs]x#[default] Open %1 in editor",
    c"#[fg=themelightgrey]          f #[#{E:tree-mode-border-style},acs]x#[default] Enter a filter",
];
fn window_buffer_help() -> mode_tree_help_info {
    mode_tree_help_info {
        width: 0 as u_int,
        item: c"buffer",
        lines: window_buffer_help_lines,
    }
}
unsafe fn window_buffer_data(wme: refbox::Weak<window_mode_entry>) -> *mut window_buffer_modedata {
    wme.get_unchecked()
        .boxed_data_ptr::<window_buffer_modedata>()
        .expect("buffer mode payload")
}

unsafe fn window_buffer_init(
    mut wme: refbox::Weak<window_mode_entry>,
    _item_handle: Option<&std::rc::Rc<std::cell::RefCell<cmdq_item>>>,
    mut fs: *mut cmd_find_state,
    mut args: *mut args,
) -> bool {
    let mode_pane_owner = wme
        .get_unchecked()
        .wp
        .upgrade()
        .expect("mode belongs to a live pane");
    let mut data: *mut window_buffer_modedata = ::core::ptr::null_mut::<window_buffer_modedata>();
    let format = if args.is_null() || args_has(&*args, 'F' as i32 as u_char) == 0 {
        WINDOW_BUFFER_DEFAULT_FORMAT.to_owned()
    } else {
        args_get(&*(args), 'F' as i32 as u_char)
            .expect("argument is present")
            .to_owned()
    };
    let key_format = if args.is_null() || args_has(&*args, 'K' as i32 as u_char) == 0 {
        WINDOW_BUFFER_DEFAULT_KEY_FORMAT.to_owned()
    } else {
        args_get(&*(args), 'K' as i32 as u_char)
            .expect("argument is present")
            .to_owned()
    };
    let command = if args.is_null() || args_count(&*args) == 0 as u_int {
        WINDOW_BUFFER_DEFAULT_COMMAND.to_owned()
    } else {
        args_string(&mut *(args), 0 as u_int)
            .expect("argument is present")
            .to_owned()
    };
    let owner = Box::new(std::cell::UnsafeCell::new(window_buffer_modedata {
        wp: std::rc::Rc::downgrade(&mode_pane_owner),
        fs: Default::default(),
        data: None,
        editor: None,
        command,
        format,
        key_format,
        item_list: Vec::new(),
    }));
    data = owner.get();
    wme.get_mut_unchecked().boxed_data = Some(owner);
    let data_handle = std::ptr::NonNull::new(data).expect("live buffer mode data");
    (*data).fs.copy_from(&*fs);
    (*data).data = Some(mode_tree_start(
        &mode_pane_owner,
        args,
        Some(Box::new(move |sort, tag, filter| {
            let mut selected = tag.unwrap_or(::core::primitive::u64::MAX as uint64_t);
            window_buffer_build(data_handle.as_ptr(), sort as *mut sort_criteria, filter);
            (selected != ::core::primitive::u64::MAX as uint64_t).then_some(selected)
        })),
        Some(Box::new(move |itemdata, ctx, sx, sy| {
            let item = itemdata.as_buffer().expect("buffer row payload");
            window_buffer_draw(&item, ctx, sx, sy)
        })),
        Some(Box::new(move |itemdata, search, icase| {
            let item = itemdata.as_buffer().expect("buffer row payload");
            window_buffer_search(&item, search, icase)
        })),
        Some(Box::new(move |client, key| {
            window_buffer_menu(data_handle.as_ptr(), client, key)
        })),
        None,
        Some(Box::new(move |itemdata, line| {
            let item = itemdata.as_buffer().expect("buffer row payload");
            window_buffer_get_key(data_handle.as_ptr(), &item, line)
        })),
        None,
        Some(window_buffer_sort),
        Some(window_buffer_help),
        &window_buffer_menu_items,
    ));
    mode_tree_build((*data).data.clone().as_ref().expect("mode tree owner"));
    mode_tree_draw((*data).data.clone().as_ref().expect("mode tree owner"));
    true
}
unsafe fn window_buffer_get_screen(
    wme: refbox::Weak<window_mode_entry>,
    visit: &mut dyn FnMut(&mut screen),
) {
    let Some(data) = wme
        .get_unchecked()
        .boxed_data_ptr::<window_buffer_modedata>()
    else {
        return;
    };
    if let Some(tree_owner) = (*data).data.as_ref() {
        mode_tree_visit_screen(tree_owner, visit);
    }
}

unsafe fn window_buffer_free(wme: refbox::Weak<window_mode_entry>) {
    let Some(data) = wme
        .get_unchecked()
        .boxed_data_ptr::<window_buffer_modedata>()
    else {
        return;
    };
    if let Some(editor) = (*data).editor.as_ref() {
        editor.cancel();
    }
    mode_tree_free((*data).data.take().expect("mode tree owner"));
}
unsafe fn window_buffer_resize(
    mut wme: refbox::Weak<window_mode_entry>,
    mut sx: u_int,
    mut sy: u_int,
) {
    let mut data: *mut window_buffer_modedata = window_buffer_data(wme.clone());
    mode_tree_resize(
        (*data).data.clone().as_ref().expect("mode tree owner"),
        sx,
        sy,
    );
}
unsafe fn window_buffer_update(mut wme: refbox::Weak<window_mode_entry>) {
    let mut data: *mut window_buffer_modedata = window_buffer_data(wme.clone());
    let Some(mode_pane_owner) =
        std::rc::Rc::<std::cell::UnsafeCell<window_pane>>::from_observer(&(*data).wp)
    else {
        return;
    };
    mode_tree_build((*data).data.clone().as_ref().expect("mode tree owner"));
    mode_tree_draw((*data).data.clone().as_ref().expect("mode tree owner"));
    window_buffer_draw_waiting(data);
    mode_pane_owner.request_redraw(false);
}
unsafe fn window_buffer_do_delete(
    mut data: *mut window_buffer_modedata,
    item: &ModeTreeItemSnapshot<window_buffer_itemdata>,
) {
    if (*data).tree_owner().borrow().get_current().is_buffer(item)
        && (*data)
            .tree_owner()
            .borrow_mut()
            .down(0 as ::core::ffi::c_int)
            == 0
    {
        (*data)
            .tree_owner()
            .borrow_mut()
            .up(0 as ::core::ffi::c_int);
    }
    if let Some(pb) = paste_get_name(&item.name) {
        paste_free(&pb);
    }
}
unsafe fn window_buffer_do_paste(
    mut data: *mut window_buffer_modedata,
    item: &window_buffer_itemdata,
    client_owner: &ClientRef,
) {
    if paste_get_name(&item.name).is_some() {
        mode_tree_run_command(Some(client_owner), None, &(*data).command, &item.name);
    }
}
unsafe fn window_buffer_draw_waiting(data: *mut window_buffer_modedata) {
    let Some(pane) = Rc::<UnsafeCell<window_pane>>::from_observer(&(*data).wp) else {
        return;
    };
    let Some(editor) = (*data).editor.as_ref() else {
        return;
    };
    pane.draw_editor_waiting(editor.pid());
}
unsafe fn window_buffer_edit_close_cb(
    editor: NonNull<spawn_editor_state>,
    buf: Option<Vec<u8>>,
    ed: window_buffer_editdata,
) {
    let mut data: *mut window_buffer_modedata = ::core::ptr::null_mut::<window_buffer_modedata>();
    let mut wme: refbox::Weak<window_mode_entry> = refbox::Weak::new();
    if let Some(pane) = Rc::<UnsafeCell<window_pane>>::find_by_id(ed.wp_id) {
        wme = pane.mode_entry();
        if !!wme.is_alive() && std::ptr::eq(wme.get_unchecked().mode, &window_buffer_mode) {
            data = window_buffer_data(wme.clone());
            if (*data)
                .editor
                .as_ref()
                .is_some_and(|handle| handle.matches(editor.as_ref()))
            {
                (*data).editor = None;
            }
        }
    }
    let Some(mut buf) = buf else {
        return;
    };
    let mut len = buf.len();
    if len == 0 {
        return;
    }
    let Some(pb) = ed.name.as_deref().and_then(paste_get_name) else {
        return;
    };
    if ed.pb != pb {
        return;
    }
    let strip_newline = {
        let buffer = pb.borrow();
        paste_buffer_data(&buffer)
            .unwrap_or_default()
            .last()
            .is_some_and(|&byte| byte != b'\n')
    };
    if strip_newline && buf[len - 1] == b'\n' {
        len = len.wrapping_sub(1);
    }
    if len != 0 as size_t {
        buf.truncate(len);
        paste_replace_owned(&pb, buf.into_boxed_slice());
    }
    if let Some(pane) = Rc::<UnsafeCell<window_pane>>::find_by_id(ed.wp_id) {
        wme = pane.mode_entry();
        if !!wme.is_alive() && std::ptr::eq(wme.get_unchecked().mode, &window_buffer_mode) {
            data = window_buffer_data(wme.clone());
            mode_tree_build((*data).data.clone().as_ref().expect("mode tree owner"));
            mode_tree_draw((*data).data.clone().as_ref().expect("mode tree owner"));
            window_buffer_draw_waiting(data);
        }
        pane.request_redraw(false);
    }
}
unsafe fn window_buffer_start_edit(
    mut data: *mut window_buffer_modedata,
    item: &window_buffer_itemdata,
    client_owner: &ClientRef,
) {
    let Some(mode_pane_owner) =
        std::rc::Rc::<std::cell::UnsafeCell<window_pane>>::from_observer(&(*data).wp)
    else {
        return;
    };
    if (*data).editor.is_some() {
        return;
    }
    let Some(pb) = paste_get_name(&item.name) else {
        return;
    };
    let name = paste_buffer_name(&pb.borrow()).to_owned();
    let ed = window_buffer_editdata {
        wp_id: mode_pane_owner.id(),
        name: Some(name),
        pb: pb.clone(),
    };
    let editor = spawn_editor(
        client_owner,
        |stream| spawn_editor_write(stream, paste_buffer_data(&pb.borrow()).unwrap_or_default()),
        Some(Box::new(move |editor, buf| unsafe {
            window_buffer_edit_close_cb(editor, buf, ed)
        })),
    );
    (*data).editor = editor;
}
unsafe fn window_buffer_key(
    mut wme: refbox::Weak<window_mode_entry>,
    client_owner: &ClientRef,
    _wl: refbox::Weak<winlink>,
    mut key: key_code,
    mut m: *mut mouse_event,
) {
    let mode_pane_owner = wme
        .get_unchecked()
        .wp
        .upgrade()
        .expect("mode belongs to a live pane");
    let mut data: *mut window_buffer_modedata = window_buffer_data(wme.clone());
    let tree_owner = (*data).tree_owner();
    let mut finished: ::core::ffi::c_int = 0;
    if paste_is_empty() != 0 {
        finished = 1 as ::core::ffi::c_int;
    } else if (*data).editor.is_some() {
        if key == 'q' as i32 as key_code
            || key == '\u{1b}' as i32 as key_code
            || key == '\u{3}' as i32 as key_code
        {
            finished = 1 as ::core::ffi::c_int;
        } else {
            finished = 0 as ::core::ffi::c_int;
        }
    } else {
        finished = mode_tree_key(
            (*data).data.as_ref().expect("mode tree owner").clone(),
            Some(client_owner),
            &raw mut key,
            m,
            ::core::ptr::null_mut::<u_int>(),
            ::core::ptr::null_mut::<u_int>(),
        );
        match key {
            101 => {
                let item_owner = tree_owner.borrow().get_current();
                if let Some(item) = item_owner.as_buffer() {
                    window_buffer_start_edit(data, &item, client_owner);
                }
            }
            100 => {
                let item_owner = tree_owner.borrow().get_current();
                if let Some(item) = item_owner.as_buffer() {
                    window_buffer_do_delete(data, &item);
                }
                mode_tree_build((*data).data.clone().as_ref().expect("mode tree owner"));
            }
            68 => {
                mode_tree_each_tagged(
                    (*data).data.clone().as_ref().expect("live mode tree"),
                    |row, _| unsafe {
                        let itemdata = row.borrow().itemdata.clone();
                        window_buffer_do_delete(
                            data,
                            &itemdata.as_buffer().expect("buffer row payload"),
                        )
                    },
                    key,
                    0 as ::core::ffi::c_int,
                );
                mode_tree_build((*data).data.clone().as_ref().expect("mode tree owner"));
            }
            80 => {
                mode_tree_each_tagged(
                    (*data).data.clone().as_ref().expect("live mode tree"),
                    |row, _| unsafe {
                        let itemdata = row.borrow().itemdata.clone();
                        window_buffer_do_paste(
                            data,
                            &itemdata.as_buffer().expect("buffer row payload"),
                            client_owner,
                        )
                    },
                    key,
                    0 as ::core::ffi::c_int,
                );
                finished = 1 as ::core::ffi::c_int;
            }
            112 | 13 => {
                let item_owner = tree_owner.borrow().get_current();
                if let Some(item) = item_owner.as_buffer() {
                    window_buffer_do_paste(data, &item, client_owner);
                }
                finished = 1 as ::core::ffi::c_int;
            }
            _ => {}
        }
    }
    if finished != 0 || paste_is_empty() != 0 {
        mode_pane_owner.reset_mode();
    } else {
        mode_tree_draw((*data).data.clone().as_ref().expect("mode tree owner"));
        window_buffer_draw_waiting(data);
        mode_pane_owner.request_redraw(false);
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buffer_search_matches_binary_substrings_without_unicode_case_folding() {
        unsafe {
            assert!(!libc::setlocale(libc::LC_CTYPE, c"C.UTF-8".as_ptr()).is_null());
        }
        let bytes = b"\xffA\0ZbA\n";
        for (find, sensitive, insensitive) in [
            (&b""[..], false, false),
            (&b"A\0Z"[..], true, true),
            (&b"a\0z"[..], false, true),
            (&b"\xffa"[..], false, true),
            (&b"A\n"[..], true, true),
            (&b"\n"[..], true, true),
            (&b"\nZ"[..], false, false),
            (&b"\xffA\0ZbA\nextra"[..], false, false),
        ] {
            assert_eq!(
                window_buffer_find(bytes, find, false),
                sensitive,
                "{find:?}"
            );
            assert_eq!(
                window_buffer_find(bytes, find, true),
                insensitive,
                "{find:?}"
            );
        }
        assert!(!window_buffer_find(b"", b"x", true));
        assert!(!window_buffer_find("É".as_bytes(), "é".as_bytes(), true));
    }

    #[test]
    fn buffer_items_own_names_and_keep_callback_addresses_stable() {
        let mut items = Vec::new();
        let source = CString::new(b"\xffbuffer".to_vec()).unwrap();
        let first = window_buffer_add_item(&mut items, &source, 7, 42);
        let first_name = first.try_borrow_mut().unwrap().name.as_ptr();
        drop(source);

        let empty = c"";
        let empty_item = window_buffer_add_item(&mut items, empty, 0, 0);
        for _ in 0..512 {
            window_buffer_add_item(&mut items, empty, 0, 0);
        }

        assert!(first.is(&items[0]));
        assert_eq!(
            unsafe { CStr::from_ptr(first_name).to_bytes() },
            b"\xffbuffer"
        );
        assert_eq!(first.try_borrow_mut().unwrap().name.as_ptr(), first_name);
        assert_eq!(empty_item.try_borrow_mut().unwrap().name.as_bytes(), b"");

        let selected = ModeTreeItemData::Buffer(first.clone());
        let snapshot = selected.as_buffer().unwrap();
        window_buffer_clear_items(&mut items);
        assert!(items.is_empty());
        assert!(!first.is_alive());
        assert!(selected.as_buffer().is_none());
        assert_eq!(snapshot.name.as_bytes(), b"\xffbuffer");
        assert_eq!((snapshot.order, snapshot.size), (7, 42));
    }
}
