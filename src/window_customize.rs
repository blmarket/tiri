use crate::src::format::bytes::nullable_cstr;
use crate::src::format::bytes::write_cstr;
use crate::src::format::bytes::xformat;
use crate::src::options::OptionsScope;
use crate::src::session::Session;
use crate::src::shared::client::ClientRef;
use crate::src::shared::mode_tree::ModeTreeItemSnapshot;
use crate::src::shared::session::{SessionRef, SessionWeak};
use crate::src::window::Window as _;
use crate::src::window_pane::WindowPane as _;
use refbox::RefBox;
use std::borrow::Cow;
use std::cell::UnsafeCell;
use std::ffi::{CStr, CString};
use std::ptr::NonNull;
use std::rc::{Rc, Weak};

use crate::src::arguments::{args_get, args_has};
use crate::src::cmd::cmd_list_print_cstring;
use crate::src::cmd::parse::{cmd_parse_error_uppercase_first, cmd_parse_from_string};
use crate::src::environ::{environ_clear, environ_find, environ_iter, environ_set, environ_unset};
use crate::src::ffi::libc::{__ctype_tolower_loc, strchr, strcmp, strcspn, strlcat, strlen};
use crate::src::format::bytes::format_message_with;
use crate::src::format::{
    format_add, format_create_from_state, format_expand_cstring, format_pretty_time_cstring,
    format_true,
};
use crate::src::grid::grid_default_cell;
use crate::src::hooks::{
    hooks_add_event, hooks_is_event, hooks_monitor_get_fire_count, hooks_monitor_get_fire_time,
    hooks_monitor_to_cstring,
};
use crate::src::key_bindings::{
    key_bindings_add, key_bindings_get, key_bindings_get_default, key_bindings_remove,
    key_bindings_reset, key_bindings_set_note, key_tables,
};
use crate::src::key_string::{key_string_format, key_string_parse_cstr};
use crate::src::mode_tree::{
    mode_tree_add, mode_tree_build, mode_tree_count_tagged, mode_tree_draw,
    mode_tree_draw_as_parent, mode_tree_each_tagged, mode_tree_free, mode_tree_get_current,
    mode_tree_get_current_name, mode_tree_key, mode_tree_no_tag, mode_tree_remove,
    mode_tree_resize, mode_tree_set_prompt, mode_tree_start, mode_tree_up,
};
use crate::src::options::{
    options_array_get, options_array_get_index, options_create, options_default,
    options_default_to_cstring, options_get_fire_count, options_get_fire_time,
    options_get_number_ref, options_match_owned, options_push_changes, options_set_number,
    options_to_cstring,
};
use crate::src::screen_write::{
    screen_write_box, screen_write_clearcharacter, screen_write_cursormove, screen_write_nputs,
    screen_write_start, screen_write_stop, screen_write_text,
};
use crate::src::shared::abi::*;
use crate::src::shared::abi::{__int32_t, ssize_t};
use crate::src::shared::arguments::args;
use crate::src::shared::colour::{COLOUR_FLAG_THEME, COLOUR_THEME_LIGHT_GREY};
use crate::src::shared::command::cmd_parse_input;
use crate::src::shared::command::*;
use crate::src::shared::command::{cmd_find_state, cmdq_item};
use crate::src::shared::environment::ENVIRON_HIDDEN;
use crate::src::shared::environment::{environ, environ_entry};
use crate::src::shared::format::format_tree;
use crate::src::shared::grid::*;
use crate::src::shared::key::KEY_BINDING_REPEAT;
use crate::src::shared::key::*;
use crate::src::shared::key::{key_binding, key_table};
use crate::src::shared::layout::*;
use crate::src::shared::limits::INT_MAX;
use crate::src::shared::menu::menu_item;
use crate::src::shared::mode_tree::{
    mode_tree_data, mode_tree_help_info, mode_tree_prompt_input_cb, ModeTreeItemData,
    ModeTreeItemRef,
};
use crate::src::shared::mouse::mouse_event;
use crate::src::shared::options::*;
use crate::src::shared::options::{
    OPTIONS_TABLE_IS_ARRAY, OPTIONS_TABLE_IS_COLOUR, OPTIONS_TABLE_IS_HOOK, OPTIONS_TABLE_IS_STYLE,
    OPTIONS_TABLE_PANE, OPTIONS_TABLE_SERVER, OPTIONS_TABLE_SESSION, OPTIONS_TABLE_WINDOW,
};
use crate::src::shared::pane::window_pane;
use crate::src::shared::prompt::*;
use crate::src::shared::prompt::{
    prompt_free_cb, prompt_result, PROMPT_ACCEPT, PROMPT_CLOSE, PROMPT_NOFORMAT, PROMPT_SINGLE,
};
use crate::src::shared::screen::screen;
use crate::src::shared::screen_write::screen_write_ctx;
use crate::src::shared::spawn::{spawn_editor_state, EditorHandle};
use crate::src::shared::window::{window_mode, window_mode_entry, winlink};
use crate::src::spawn::{spawn_editor, spawn_editor_write};
use crate::src::status::status_message_set;
use crate::src::tmux::global_environ;

fn window_customize_uppercase_cause(cause: &mut Option<CString>) {
    if let Some(message) = cause.take() {
        let mut bytes = message.into_bytes();
        if let Some(first) = bytes.first_mut() {
            first.make_ascii_uppercase();
        }
        *cause = Some(CString::new(bytes).expect("error message has no interior NUL"));
    }
}

#[repr(C)]
pub struct window_customize_modedata {
    pub wp: Weak<UnsafeCell<window_pane>>,
    pub dead: ::core::ffi::c_int,
    pub data: Option<std::rc::Rc<std::cell::UnsafeCell<mode_tree_data>>>,
    pub editor: Option<EditorHandle>,
    pub format: CString,
    pub hide_global: ::core::ffi::c_int,
    pub hide_default: ::core::ffi::c_int,
    pub prompt_flags: ::core::ffi::c_int,
    item_list: Vec<refbox::RefBox<window_customize_itemdata>>,
    pub fs: cmd_find_state,
    pub change: window_customize_change,
}

impl window_customize_modedata {
    fn tree_owner(&self) -> Rc<UnsafeCell<mode_tree_data>> {
        self.data.as_ref().expect("mode tree owner").clone()
    }
}

pub type window_customize_change = ::core::ffi::c_uint;
pub const WINDOW_CUSTOMIZE_RESET: window_customize_change = 1;
pub const WINDOW_CUSTOMIZE_UNSET: window_customize_change = 0;
#[derive(Clone)]
pub struct window_customize_itemdata {
    pub type_0: window_customize_item_type,
    pub option_type: window_customize_option_type,
    pub scope: window_customize_scope,
    pub table: Option<std::ffi::CString>,
    pub key: key_code,
    pub oo: Option<OptionsScope>,
    environ: Option<CustomizeEnvironment>,
    pub environ_flags: ::core::ffi::c_int,
    pub name: Option<std::ffi::CString>,
    pub array_key: Option<std::ffi::CString>,
}

impl window_customize_itemdata {
    fn new() -> Self {
        window_customize_itemdata {
            type_0: 0,
            option_type: 0,
            scope: 0,
            table: None,
            key: 0,
            oo: None,
            environ: None,
            environ_flags: 0,
            name: None,
            array_key: None,
        }
    }
}

/// Rows keep target identity without retaining a session for the entire mode.
#[derive(Clone)]
enum CustomizeEnvironment {
    Global,
    Session(SessionWeak),
}

impl CustomizeEnvironment {
    fn session(s: &SessionRef) -> Self {
        Self::Session(Rc::downgrade(s))
    }

    fn matches(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Global, Self::Global) => true,
            (Self::Session(a), Self::Session(b)) => a.ptr_eq(b),
            _ => false,
        }
    }

    fn resolve(&self) -> Option<CustomizeEnvironmentBorrow> {
        match self {
            Self::Global => Some(CustomizeEnvironmentBorrow::Global),
            Self::Session(session) => session.upgrade().map(CustomizeEnvironmentBorrow::Session),
        }
    }
}

/// Retain a session only for the duration of an environment operation. All
/// environment references are tied to this guard, rather than a saved pointer.
enum CustomizeEnvironmentBorrow {
    Global,
    Session(SessionRef),
}

impl CustomizeEnvironmentBorrow {
    unsafe fn read<R>(&self, read: impl FnOnce(&environ) -> R) -> Option<R> {
        match self {
            Self::Global => global_environ.as_deref().map(read),
            Self::Session(owner) => {
                let env = owner.borrow_environment()?;
                Some(read(env))
            }
        }
    }

    // Formatting, drawing and prompt installation can reenter the target.
    // A snapshot owns the entry strings throughout those operations.
    unsafe fn get(&self) -> Option<environ> {
        self.read(Clone::clone)
    }

    unsafe fn rows(&self) -> Option<Vec<(u64, environ_entry)>> {
        self.read(|env| {
            environ_iter(env)
                .map(|entry| {
                    // The low bit separates environment records from static
                    // option tags, which share the same category bits.
                    ((2_u64 << 62) | (entry.id() << 1), entry.clone())
                })
                .collect()
        })
    }

    unsafe fn edit<R>(&self, edit: impl FnOnce(&mut environ) -> R) -> Option<R> {
        match self {
            Self::Global => global_environ.as_deref_mut().map(edit),
            Self::Session(owner) => {
                let mut env = owner.borrow_environment_mut()?;
                Some(edit(env))
            }
        }
    }
}

// Detached prompt and editor copies own their strings independently of the rows.
fn window_customize_set_table(item: &mut window_customize_itemdata, value: Option<&CStr>) {
    item.table = value.map(CStr::to_owned);
}

fn window_customize_set_name(item: &mut window_customize_itemdata, value: Option<&CStr>) {
    item.name = value.map(CStr::to_owned);
}

fn window_customize_set_item_array_key(item: &mut window_customize_itemdata, value: Option<&CStr>) {
    item.array_key = value.map(CStr::to_owned);
}

fn window_customize_new_item() -> Box<window_customize_itemdata> {
    Box::new(window_customize_itemdata::new())
}
pub type window_customize_scope = ::core::ffi::c_uint;
pub const WINDOW_CUSTOMIZE_SESSION_ENVIRONMENT: window_customize_scope = 9;
pub const WINDOW_CUSTOMIZE_GLOBAL_ENVIRONMENT: window_customize_scope = 8;
pub const WINDOW_CUSTOMIZE_PANE: window_customize_scope = 7;
pub const WINDOW_CUSTOMIZE_WINDOW: window_customize_scope = 6;
pub const WINDOW_CUSTOMIZE_GLOBAL_WINDOW: window_customize_scope = 5;
pub const WINDOW_CUSTOMIZE_SESSION: window_customize_scope = 4;
pub const WINDOW_CUSTOMIZE_GLOBAL_SESSION: window_customize_scope = 3;
pub const WINDOW_CUSTOMIZE_SERVER: window_customize_scope = 2;
pub const WINDOW_CUSTOMIZE_KEY: window_customize_scope = 1;
pub const WINDOW_CUSTOMIZE_NONE: window_customize_scope = 0;
pub type window_customize_option_type = ::core::ffi::c_uint;
pub const WINDOW_CUSTOMIZE_HOOKS: window_customize_option_type = 1;
pub const WINDOW_CUSTOMIZE_OPTIONS: window_customize_option_type = 0;
pub type window_customize_item_type = ::core::ffi::c_uint;
pub const WINDOW_CUSTOMIZE_ITEM_ENVIRONMENT: window_customize_item_type = 2;
pub const WINDOW_CUSTOMIZE_ITEM_KEY: window_customize_item_type = 1;
pub const WINDOW_CUSTOMIZE_ITEM_OPTION: window_customize_item_type = 0;
#[repr(C)]
pub struct window_customize_editdata {
    pub wp_id: u_int,
    pub edit_type: window_customize_edit_type,
    pub item: Box<window_customize_itemdata>,
}
pub type window_customize_edit_type = ::core::ffi::c_uint;
pub const WINDOW_CUSTOMIZE_EDIT_ENVIRONMENT: window_customize_edit_type = 3;
pub const WINDOW_CUSTOMIZE_EDIT_KEY_NOTE: window_customize_edit_type = 2;
pub const WINDOW_CUSTOMIZE_EDIT_KEY_COMMAND: window_customize_edit_type = 1;
pub const WINDOW_CUSTOMIZE_EDIT_OPTION: window_customize_edit_type = 0;

#[inline]
unsafe fn tolower(mut __c: ::core::ffi::c_int) -> ::core::ffi::c_int {
    if __c >= -(128 as ::core::ffi::c_int) && __c < 256 as ::core::ffi::c_int {
        *(*__ctype_tolower_loc()).offset(__c as isize) as ::core::ffi::c_int
    } else {
        __c
    }
}

pub const WINDOW_CUSTOMIZE_DEFAULT_FORMAT: &CStr = c"#{?is_option,#{?option_is_global,,#[reverse](#{option_scope})#[default] }#[fg=themelightgrey]#[ignore]#{option_value}#{?option_unit, #{option_unit},},#{?is_environment,#[fg=themelightgrey]#[ignore]#{environment_value},#{key}}}";
static window_customize_menu_items: [menu_item<'static>; 11] = [
    menu_item {
        name: c"Select",
        key: '\r' as i32 as key_code,
        command: None,
    },
    menu_item {
        name: c"Edit",
        key: 'e' as i32 as key_code,
        command: None,
    },
    menu_item {
        name: c"Expand",
        key: KEYC_RIGHT as ::core::ffi::c_ulong as key_code,
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
        name: c"Changed Only",
        key: 'C' as i32 as key_code,
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
pub static window_customize_mode: window_mode = {
    window_mode {
        name: c"options-mode",
        default_format: Some(WINDOW_CUSTOMIZE_DEFAULT_FORMAT),
        flags: 0,
        init: Some(
            window_customize_init
                as unsafe fn(
                    refbox::Weak<window_mode_entry>,
                    Option<&std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>>,
                    *mut cmd_find_state,
                    *mut args,
                ) -> *mut screen,
        ),
        free: Some(window_customize_free as unsafe fn(refbox::Weak<window_mode_entry>) -> ()),
        resize: Some(
            window_customize_resize
                as unsafe fn(refbox::Weak<window_mode_entry>, u_int, u_int) -> (),
        ),
        update: Some(window_customize_update as unsafe fn(refbox::Weak<window_mode_entry>) -> ()),
        style_changed: None,
        key: Some(
            window_customize_key
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
        display_screen: Some(window_customize_get_screen),
    }
};
const CUSTOMIZE_SERVER_OPTIONS: u_int = 1;
const CUSTOMIZE_SESSION_OPTIONS: u_int = 2;
const CUSTOMIZE_WINDOW_OPTIONS: u_int = 3;
const CUSTOMIZE_SESSION_HOOKS: u_int = 4;
const CUSTOMIZE_WINDOW_HOOKS: u_int = 5;
const CUSTOMIZE_GLOBAL_ENVIRONMENT: u_int = 6;
const CUSTOMIZE_SESSION_ENVIRONMENT: u_int = 7;

// Resolve the existing entry for this synchronous operation. Persistent UI rows
// already store their scope and name and resolve again when acted on.
unsafe fn window_customize_find_option(
    scope: &OptionsScope,
    name: &CStr,
) -> Option<(OptionsScope, *mut options_entry)> {
    let owner = scope.resolve(name, false)?;
    let entry = owner.with_entry(name, |entry| entry as *mut options_entry)?;
    Some((owner, entry))
}

fn window_customize_option_tag(option: &options_entry) -> u64 {
    let Some(definition) = option.tableentry else {
        return option.id();
    };
    let index = crate::src::options_table::options_table
        .iter()
        .position(|candidate| std::ptr::eq(candidate, definition))
        .expect("static option definition");
    (2_u64 << 62) | ((index as u64) << 32) | 1
}

fn window_customize_top_tag(group: u_int) -> uint64_t {
    let (kind, table, scope) = match group {
        CUSTOMIZE_SERVER_OPTIONS => (0, OPTIONS_TABLE_SERVER, 1),
        CUSTOMIZE_SESSION_OPTIONS => (0, OPTIONS_TABLE_SESSION, 1),
        CUSTOMIZE_WINDOW_OPTIONS => (0, OPTIONS_TABLE_WINDOW, 1),
        CUSTOMIZE_SESSION_HOOKS => (1, OPTIONS_TABLE_SESSION, 1),
        CUSTOMIZE_WINDOW_HOOKS => (1, OPTIONS_TABLE_WINDOW, 1),
        CUSTOMIZE_GLOBAL_ENVIRONMENT => (2, 0, 1),
        CUSTOMIZE_SESSION_ENVIRONMENT => (2, 0, 3),
        _ => unreachable!("unknown customize section"),
    };
    (3_u64 << 62) | ((kind as u64) << 8) | ((table as u64) << 1) | scope
}

fn window_customize_key_tag(ptr: *const ::core::ffi::c_void, field: u_int) -> uint64_t {
    ptr as uint64_t | field as uint64_t
}
unsafe fn window_customize_get_tree(
    scope: window_customize_scope,
    fs: &cmd_find_state,
) -> Option<OptionsScope> {
    match scope {
        WINDOW_CUSTOMIZE_SERVER => Some(OptionsScope::GlobalServer),
        WINDOW_CUSTOMIZE_GLOBAL_SESSION => Some(OptionsScope::GlobalSession),
        WINDOW_CUSTOMIZE_SESSION => Some(OptionsScope::Session(fs.s.clone())),
        WINDOW_CUSTOMIZE_GLOBAL_WINDOW => Some(OptionsScope::GlobalWindow),
        WINDOW_CUSTOMIZE_WINDOW => Some(OptionsScope::Window(fs.w.clone())),
        WINDOW_CUSTOMIZE_PANE => Some(OptionsScope::Pane(fs.wp.clone())),
        _ => None,
    }
}

unsafe fn window_customize_get_environment(
    scope: window_customize_scope,
    fs: &cmd_find_state,
) -> Option<CustomizeEnvironment> {
    match scope {
        WINDOW_CUSTOMIZE_GLOBAL_ENVIRONMENT => Some(CustomizeEnvironment::Global),
        WINDOW_CUSTOMIZE_SESSION_ENVIRONMENT if !fs.session_handle().is_none() => Some(
            CustomizeEnvironment::session(&fs.session_handle().expect("target session")),
        ),
        _ => None,
    }
}
unsafe fn window_customize_check_item(
    data: &window_customize_modedata,
    item: &window_customize_itemdata,
    output: Option<&mut cmd_find_state>,
) -> ::core::ffi::c_int {
    let Some(mode_pane_owner) = Rc::<UnsafeCell<window_pane>>::from_observer(&data.wp) else {
        return 0;
    };
    let mut fallback = cmd_find_state::default();
    let fs = output.unwrap_or(&mut fallback);
    if data.fs.is_valid() {
        let mut source = data.fs.clone();
        fs.copy_from(&source);
    } else {
        *fs = cmd_find_state::find_from_pane(&mode_pane_owner, 0)
            .unwrap_or_else(|| cmd_find_state::clear_state(0));
    }
    if item.type_0 == WINDOW_CUSTOMIZE_ITEM_ENVIRONMENT {
        return item
            .environ
            .as_ref()
            .zip(window_customize_get_environment(item.scope, fs).as_ref())
            .is_some_and(|(a, b)| a.matches(b)) as ::core::ffi::c_int;
    }
    (item.oo == window_customize_get_tree(item.scope, fs)) as ::core::ffi::c_int
}
unsafe fn window_customize_get_key_table(
    item: &window_customize_itemdata,
) -> Option<Rc<std::cell::RefCell<key_table>>> {
    let table = crate::src::key_bindings::key_bindings_get_table(item.table.as_deref()?, 0)?;
    key_bindings_get(&table.borrow(), item.key)?;
    Some(table)
}
unsafe fn window_customize_scope_text(
    scope: window_customize_scope,
    fs: &cmd_find_state,
) -> CString {
    let mut idx: u_int = 0;
    match scope as ::core::ffi::c_uint {
        7 => {
            let pane = fs.pane_handle().expect("customize target pane");
            let window = pane.window_observer().upgrade().expect("pane parent");
            idx = window
                .pane_index(&Rc::downgrade(&pane))
                .expect("pane belongs to window ordering");
            drop(window);
            CString::new(format!("pane {idx}")).expect("pane index contains no NUL")
        }
        4 | 9 => {
            let mut bytes = b"session ".to_vec();
            bytes.extend_from_slice(fs.session_handle().expect("live session").name().as_bytes());
            CString::new(bytes).expect("session name contains no NUL")
        }
        6 => CString::new(format!(
            "window {}",
            (fs.winlink_handle()).get_unchecked().idx
        ))
        .expect("window index contains no NUL"),
        _ => CString::new(Vec::new()).expect("empty scope text"),
    }
}
unsafe fn window_customize_write_hook_fire(
    mut ctx: *mut screen_write_ctx,
    mut cx: u_int,
    mut sx: u_int,
    mut sy: u_int,
    owner: &OptionsScope,
    option: *mut options_entry,
) -> ::core::ffi::c_int {
    let Some((fire_count, fire_time)) = owner.with_entry(&(*option).name, |entry| {
        if entry.monitor_data.is_some() {
            (
                hooks_monitor_get_fire_count(entry),
                hooks_monitor_get_fire_time(entry),
            )
        } else {
            (options_get_fire_count(entry), options_get_fire_time(entry))
        }
    }) else {
        return 0;
    };
    if fire_time != 0 as time_t {
        let fire_time_string = format_pretty_time_cstring(fire_time);
        if screen_write_text(
            &mut *ctx,
            cx,
            sx,
            sy,
            0 as ::core::ffi::c_int,
            &grid_default_cell,
            |out| {
                write!(out, "This hook has been fired {} times, last ", {
                    fire_count
                })?;
                write_cstr(out, &*fire_time_string)?;
                out.write_all(b".")
            },
        ) == 0
        {
            return 0 as ::core::ffi::c_int;
        }
        return 1 as ::core::ffi::c_int;
    }
    screen_write_text(
        &mut *ctx,
        cx,
        sx,
        sy,
        0 as ::core::ffi::c_int,
        &grid_default_cell,
        |out| write!(out, "This hook has been fired {} times.", { fire_count }),
    )
}
fn window_customize_add_item(
    items: &mut Vec<refbox::RefBox<window_customize_itemdata>>,
    item: window_customize_itemdata,
) -> refbox::Weak<window_customize_itemdata> {
    let item = refbox::RefBox::new(item);
    let handle = item.downgrade();
    items.push(item);
    handle
}
unsafe fn window_customize_write_value(
    mut ctx: *mut screen_write_ctx,
    mut cx: u_int,
    mut sx: u_int,
    mut sy: u_int,
    mut more: ::core::ffi::c_int,
    label: &CStr,
    write: impl FnOnce(&mut dyn std::io::Write) -> std::io::Result<()>,
) -> ::core::ffi::c_int {
    let mut s: *mut screen = (*ctx).screen_ptr();
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
    let mut cy: u_int = (*s).cy;
    let mut retval: ::core::ffi::c_int = 0;
    if sy == 0 as u_int {
        return 0 as ::core::ffi::c_int;
    }
    if screen_write_text(
        &mut *ctx,
        cx,
        sx,
        sy,
        1 as ::core::ffi::c_int,
        &grid_default_cell,
        |out| write_cstr(out, nullable_cstr(label.as_ptr())),
    ) == 0
    {
        return 0 as ::core::ffi::c_int;
    }
    if (*s).cy.wrapping_sub(cy) >= sy {
        return 0 as ::core::ffi::c_int;
    }
    sy = sy.wrapping_sub((*s).cy.wrapping_sub(cy));
    gc = grid_default_cell;
    gc.fg = COLOUR_THEME_LIGHT_GREY as ::core::ffi::c_int | COLOUR_FLAG_THEME;
    let value = format_message_with(write);
    retval = screen_write_text(&mut *ctx, cx, sx, sy, more, &gc, |out| {
        write_cstr(out, &*value)
    });
    retval
}

fn window_customize_copy_item(item: &window_customize_itemdata) -> Box<window_customize_itemdata> {
    Box::new(item.clone())
}

unsafe fn window_customize_draw_waiting(mode_owner: &Rc<UnsafeCell<window_customize_modedata>>) {
    let data = mode_owner.get();
    if (*data).dead != 0 {
        return;
    }
    let Some(_mode_pane_owner) = Rc::<UnsafeCell<window_pane>>::from_observer(&(*data).wp) else {
        return;
    };
    let mut ctx: screen_write_ctx = screen_write_ctx {
        wp: std::rc::Weak::new(),
        target: Default::default(),
        flags: 0,
        init_ctx_cb: None,
        item: None,
        scrolled: 0,
        bg: 0,
    };
    let tree_owner = (*data).tree_owner();
    let s: *mut screen = &raw mut (*tree_owner.get()).screen;
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
    let mut text: [u8; 128] = [0; 128];
    let mut sx: u_int = 0;
    let mut sy: u_int = 0;
    let mut box_w: u_int = 0;
    let mut box_h: u_int = 0;
    let mut x: u_int = 0;
    let mut y: u_int = 0;
    let mut text_x: u_int = 0;
    let mut textlen: size_t = 0;
    let mut pid: pid_t = 0;
    let Some(editor) = (*data).editor.as_ref() else {
        return;
    };
    sx = (*s).grid().sx;
    sy = (*s).grid().sy;
    if sx == 0 as u_int || sy == 0 as u_int {
        return;
    }
    pid = editor.pid();
    if pid == -(1 as ::core::ffi::c_int) {
        xformat(&mut text, format_args!("WAITING FOR EDITOR"));
    } else {
        xformat(
            &mut text,
            format_args!("WAITING FOR EDITOR (PID {})", pid as ::core::ffi::c_long),
        );
    }
    textlen = strlen(&raw mut text as *mut ::core::ffi::c_char);
    box_w = textlen.wrapping_add(4 as size_t) as u_int;
    box_h = 3 as u_int;
    if sx < box_w || sy < box_h {
        return;
    }
    x = sx.wrapping_sub(box_w).wrapping_div(2 as u_int);
    y = sy.wrapping_sub(box_h).wrapping_div(2 as u_int);
    text_x = (x as size_t).wrapping_add(
        (box_w as size_t)
            .wrapping_sub(textlen)
            .wrapping_div(2 as size_t),
    ) as u_int;
    gc = grid_default_cell;
    screen_write_start(&mut ctx, s);
    screen_write_cursormove(
        &mut ctx,
        x as ::core::ffi::c_int,
        y as ::core::ffi::c_int,
        0 as ::core::ffi::c_int,
    );
    screen_write_box(&mut ctx, box_w, box_h, BOX_LINES_DEFAULT, Some(&gc), None);
    screen_write_cursormove(
        &mut ctx,
        x.wrapping_add(1 as u_int) as ::core::ffi::c_int,
        y.wrapping_add(1 as u_int) as ::core::ffi::c_int,
        0 as ::core::ffi::c_int,
    );
    screen_write_clearcharacter(&mut ctx, box_w.wrapping_sub(2 as u_int), gc.bg as u_int);
    screen_write_cursormove(
        &mut ctx,
        text_x as ::core::ffi::c_int,
        y.wrapping_add(1 as u_int) as ::core::ffi::c_int,
        0 as ::core::ffi::c_int,
    );
    screen_write_nputs(
        &mut ctx,
        box_w.wrapping_sub(2 as u_int) as ssize_t,
        &gc,
        |out| write_cstr(out, CStr::from_ptr(text.as_ptr().cast())),
    );
    screen_write_stop(&mut ctx);
}
unsafe fn window_customize_set_option_value(
    item: &window_customize_itemdata,
    value: Option<&CStr>,
    cause: *mut Option<CString>,
) -> ::core::ffi::c_int {
    let mut value: *const ::core::ffi::c_char = value.map_or(std::ptr::null(), CStr::as_ptr);
    let scope = item.oo.as_ref().expect("option row has a scope");
    let name = item.name.as_deref().expect("option row has a name");
    let Some((owner, option)) = window_customize_find_option(scope, name) else {
        return -1;
    };
    let value = (!value.is_null()).then(|| CStr::from_ptr(value));
    let result = if (*option)
        .tableentry
        .is_some_and(|definition| definition.flags & OPTIONS_TABLE_IS_ARRAY != 0)
    {
        let key = if let Some(key) = &item.array_key {
            key.clone()
        } else {
            let index = owner
                .with_entry(&(*option).name, |entry| {
                    let mut index = 0_u32;
                    while index < INT_MAX as u32 && options_array_get_index(entry, index).is_some()
                    {
                        index += 1;
                    }
                    index
                })
                .expect("array entry remains live");
            CString::new(index.to_string()).expect("decimal array index")
        };
        owner.set_array_item(&(*option).name, &key, value, false)
    } else {
        scope.set_from_string((*option).tableentry, name, value, false)
    };
    if let Err(error) = result {
        if !cause.is_null() {
            *cause = Some(error);
        }
        return -1;
    }
    if item.option_type == WINDOW_CUSTOMIZE_HOOKS && name.to_bytes().first() == Some(&b'@') {
        hooks_add_event(name);
    }
    options_push_changes(name);
    0
}

unsafe fn window_customize_option_editable(
    data: &window_customize_modedata,
    item: &window_customize_itemdata,
) -> ::core::ffi::c_int {
    if item.type_0 != WINDOW_CUSTOMIZE_ITEM_OPTION
        || window_customize_check_item(data, item, None) == 0
    {
        return 0;
    }
    let Some((_owner, option)) = window_customize_find_option(
        item.oo.as_ref().expect("option scope"),
        item.name.as_deref().expect("option name"),
    ) else {
        return 0;
    };
    (!(*option).tableentry.is_some_and(|definition| {
        matches!(definition.type_0, OPTIONS_TABLE_FLAG | OPTIONS_TABLE_CHOICE)
    })) as i32
}

unsafe fn window_customize_set_command_value(
    item: &window_customize_itemdata,
    s: &CStr,
    cause: *mut Option<CString>,
) -> ::core::ffi::c_int {
    let Some(table) = window_customize_get_key_table(item) else {
        return -(1 as ::core::ffi::c_int);
    };
    let mut kt = table.borrow_mut();
    let mut pr = cmd_parse_from_string(s, ::core::ptr::null_mut::<cmd_parse_input>());
    if pr.status == CMD_PARSE_ERROR {
        if !cause.is_null() {
            *cause = pr.error;
        }
        return -(1 as ::core::ffi::c_int);
    }
    let Some(bd) = kt.key_bindings.get_mut(item.key) else {
        drop(pr.cmdlist.take());
        return -1;
    };
    bd.commands = pr.cmdlist.take().expect("successful command parse");
    0 as ::core::ffi::c_int
}
unsafe fn window_customize_set_note_value(
    item: &window_customize_itemdata,
    s: &CStr,
) -> ::core::ffi::c_int {
    let s: *const ::core::ffi::c_char = s.as_ptr();
    let Some(table) = window_customize_get_key_table(item) else {
        return -(1 as ::core::ffi::c_int);
    };
    let mut kt = table.borrow_mut();
    let bd = kt.key_bindings.get_mut(item.key).expect("live binding");
    if *s as ::core::ffi::c_int == '\0' as i32 {
        key_bindings_set_note(bd, None);
    } else {
        key_bindings_set_note(bd, Some(CStr::from_ptr(s)));
    }
    0 as ::core::ffi::c_int
}
unsafe fn window_customize_set_environment_value(item: &window_customize_itemdata, s: &CStr) {
    let Some(mut environment) = item
        .environ
        .as_ref()
        .and_then(CustomizeEnvironment::resolve)
    else {
        return;
    };
    environment.edit(|env| {
        let mut envent: Option<&environ_entry> = None;
        let mut flags: ::core::ffi::c_int = 0;
        flags = item.environ_flags;
        envent = environ_find(env, (item.name).as_deref().expect("string argument"));
        if let Some(envent_value) = envent {
            flags = envent_value.flags;
        }
        environ_set(
            env,
            (item.name).as_deref().expect("string argument"),
            flags,
            |out| write_cstr(out, nullable_cstr(s.as_ptr())),
        );
    });
}
unsafe fn window_customize_option_is_changed(
    owner: &OptionsScope,
    option: *mut options_entry,
    key: Option<&CStr>,
) -> ::core::ffi::c_int {
    let Some(definition) = (*option).tableentry else {
        return 1;
    };
    if (*option).monitor_data.is_some()
        || ((*option).name.as_bytes().first() == Some(&b'@')
            && hooks_is_event(&((*option).name)) != 0)
    {
        return 1;
    }
    if definition.flags & OPTIONS_TABLE_IS_ARRAY != 0 {
        // Default commands may evaluate formats. The scratch tree is detached
        // and no model table remains borrowed while defaults are parsed.
        let mut defaults_owner = options_create(None);
        let defaults = options_default(&mut *defaults_owner, definition);
        let current = owner.with_entry(&(*option).name, |entry| {
            let present = key.is_none_or(|key| options_array_get(entry, key).is_some());
            (present, options_to_cstring(entry, key, 0))
        });
        let present_default = key.is_none_or(|key| options_array_get(&*defaults, key).is_some());
        let result = if let Some((present, value)) = current {
            if !present || !present_default {
                present != present_default
            } else {
                value != options_to_cstring(defaults, key, 0)
            }
        } else {
            true
        };
        drop(defaults_owner);
        return result as i32;
    }
    let value = options_to_cstring(option, nullable_cstr(std::ptr::null()), 0);
    (value != options_default_to_cstring(definition)) as i32
}

unsafe fn window_customize_key_is_changed(kt: &key_table, bd: &key_binding) -> ::core::ffi::c_int {
    let Some(default_bd) = key_bindings_get_default(kt, bd.key) else {
        return 1;
    };
    if bd.flags != default_bd.flags || bd.note != default_bd.note {
        return 1;
    }
    let cmd = cmd_list_print_cstring(&bd.cmdlist().borrow(), 0);
    let default_cmd = cmd_list_print_cstring(&default_bd.cmdlist().borrow(), 0);
    (cmd.as_bytes() != default_cmd.as_bytes()) as ::core::ffi::c_int
}
unsafe fn window_customize_build_array(
    mode_owner: &Rc<UnsafeCell<window_customize_modedata>>,
    top: &ModeTreeItemRef,
    scope: window_customize_scope,
    owner: &OptionsScope,
    option: *mut options_entry,
    ft: *mut format_tree,
) -> u_int {
    let data = mode_owner.get();
    // Formatting can edit the table; retain its name and resolve each row again.
    let option_name = (*option).name.clone();
    let Some(keys) = owner.with_entry(&(*option).name, |entry| {
        crate::src::options::options_array_iter(entry)
            .map(|item| item.key.clone())
            .collect::<Vec<_>>()
    }) else {
        return 0;
    };
    let mut count = 0_u32;
    for key in keys {
        let Some((_, option)) = window_customize_find_option(owner, &option_name) else {
            break;
        };
        let exists = owner.with_entry(&(*option).name, |entry| {
            options_array_get(entry, &key).is_some()
        });
        if exists != Some(true) {
            break;
        }
        if (*data).hide_default != 0
            && window_customize_option_is_changed(owner, option, Some(&key)) == 0
        {
            continue;
        }
        let Some(Some((id, value))) = owner.with_entry(&(*option).name, |entry| {
            crate::src::options::options_array_iter(entry)
                .find(|item| item.key == key)
                .map(|item| (item.id(), options_to_cstring(entry, Some(&*key), 0)))
        }) else {
            break;
        };
        let mut name = (*option).name.as_bytes().to_vec();
        name.push(b'[');
        name.extend_from_slice(key.as_bytes());
        name.push(b']');
        let name = CString::new(name).expect("option name and array key contain no NUL");
        format_add(ft, c"option_name", |out| write_cstr(out, &*name));
        format_add(ft, c"option_value", |out| write_cstr(out, &*value));
        let item_owner = window_customize_add_item(
            &mut (*data).item_list,
            window_customize_itemdata {
                type_0: WINDOW_CUSTOMIZE_ITEM_OPTION,
                option_type: if (*option)
                    .tableentry
                    .is_some_and(|definition| definition.flags & OPTIONS_TABLE_IS_HOOK != 0)
                {
                    WINDOW_CUSTOMIZE_HOOKS
                } else {
                    WINDOW_CUSTOMIZE_OPTIONS
                },
                scope,
                oo: Some(owner.clone()),
                name: Some((*option).name.clone()),
                array_key: Some(key),
                ..window_customize_itemdata::new()
            },
        );
        let text = format_expand_cstring(ft, (*data).format.as_ptr());
        mode_tree_add(
            &mut *(*data).tree_owner().get(),
            Some(top),
            ModeTreeItemData::Customize(item_owner),
            id,
            &name,
            Some(&text),
            -1,
        );
        count = count.wrapping_add(1);
    }
    count
}

unsafe fn window_customize_build_option(
    mode_owner: &Rc<UnsafeCell<window_customize_modedata>>,
    top: &ModeTreeItemRef,
    mut scope: window_customize_scope,
    owner: &OptionsScope,
    option: *mut options_entry,
    mut ft: *mut format_tree,
    filter: Option<&CStr>,
    mut fs: *mut cmd_find_state,
    mut type_0: window_customize_option_type,
) -> u_int {
    let filter: *const ::core::ffi::c_char = filter.map_or(std::ptr::null(), CStr::as_ptr);
    let data = mode_owner.get();
    let oe = (*option).tableentry.map_or(std::ptr::null(), |entry| {
        entry as *const options_table_entry
    });
    let name_owner = (*option).name.clone();
    let name = name_owner.as_ptr();
    let mut global: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    let mut array: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    let mut is_hook: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    let mut is_monitor: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    let mut is_user_hook: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    let mut is_any_hook: ::core::ffi::c_int = 0;
    if !oe.is_null() && (*oe).flags & OPTIONS_TABLE_IS_HOOK != 0 {
        is_hook = 1 as ::core::ffi::c_int;
    }
    if (*option).monitor_data.is_some() {
        is_monitor = 1 as ::core::ffi::c_int;
    }
    if *name as ::core::ffi::c_int == '@' as i32 && hooks_is_event(CStr::from_ptr(name)) != 0 {
        is_user_hook = 1 as ::core::ffi::c_int;
    }
    is_any_hook = (is_hook != 0 || is_monitor != 0 || is_user_hook != 0) as ::core::ffi::c_int;
    match type_0 as ::core::ffi::c_uint {
        0 => {
            if is_any_hook != 0 {
                return 0 as u_int;
            }
        }
        1 if is_any_hook == 0 => {
            return 0 as u_int;
        }
        _ => {}
    }
    if !oe.is_null() && (*oe).flags & OPTIONS_TABLE_IS_ARRAY != 0 {
        array = 1 as ::core::ffi::c_int;
    }
    if scope as ::core::ffi::c_uint
        == WINDOW_CUSTOMIZE_SERVER as ::core::ffi::c_int as ::core::ffi::c_uint
        || scope as ::core::ffi::c_uint
            == WINDOW_CUSTOMIZE_GLOBAL_SESSION as ::core::ffi::c_int as ::core::ffi::c_uint
        || scope as ::core::ffi::c_uint
            == WINDOW_CUSTOMIZE_GLOBAL_WINDOW as ::core::ffi::c_int as ::core::ffi::c_uint
    {
        global = 1 as ::core::ffi::c_int;
    }
    if (*data).hide_global != 0 && global != 0 {
        return 0 as u_int;
    }
    if (*data).hide_default != 0 && window_customize_option_is_changed(owner, option, None) == 0 {
        return 0 as u_int;
    }
    format_add(ft, c"option_name", |out| {
        write_cstr(out, nullable_cstr(name))
    });
    format_add(ft, c"option_is_global", |out| {
        write!(out, "{}", (global) as i32)
    });
    format_add(ft, c"option_is_array", |out| {
        write!(out, "{}", (array) as i32)
    });
    format_add(ft, c"option_is_hook", |out| {
        write!(out, "{}", (is_hook) as i32)
    });
    format_add(ft, c"option_is_monitor", |out| {
        write!(out, "{}", (is_monitor) as i32)
    });
    let scope_text = window_customize_scope_text(scope, &*fs);
    format_add(ft, c"option_scope", |out| write_cstr(out, &*scope_text));
    drop(scope_text);
    if !oe.is_null() && (*oe).unit.is_some() {
        format_add(ft, c"option_unit", |out| write_cstr(out, (*oe).unit));
    } else {
        format_add(ft, c"option_unit", |out| write_cstr(out, c""));
    }
    if is_monitor != 0 {
        if let Some(monitor) = hooks_monitor_to_cstring(option) {
            format_add(ft, c"option_monitor", |out| write_cstr(out, &*monitor));
        } else {
            format_add(ft, c"option_monitor", |out| write_cstr(out, c""));
        }
    } else {
        format_add(ft, c"option_monitor", |out| write_cstr(out, c""));
    }
    if array == 0 {
        let value = options_to_cstring(option, nullable_cstr(std::ptr::null()), 0);
        format_add(ft, c"option_value", |out| write_cstr(out, &*value));
    }
    if !filter.is_null() {
        let expanded = format_expand_cstring(ft, filter);
        if format_true(expanded.as_ptr()) == 0 {
            return 0 as u_int;
        }
    }
    let Some((_, option)) = window_customize_find_option(owner, &name_owner) else {
        return 0;
    };
    let tag = window_customize_option_tag(&*option);
    let item_owner = window_customize_add_item(
        &mut (*data).item_list,
        window_customize_itemdata {
            type_0: WINDOW_CUSTOMIZE_ITEM_OPTION,
            option_type: type_0,
            scope,
            oo: Some(owner.clone()),
            name: Some(CStr::from_ptr(name).to_owned()),
            ..window_customize_itemdata::new()
        },
    );
    let text = (array == 0).then(|| format_expand_cstring(ft, (*data).format.as_ptr()));
    let top = mode_tree_add(
        &mut *(*data).tree_owner().get(),
        Some(top),
        ModeTreeItemData::Customize(item_owner.clone()),
        tag,
        CStr::from_ptr(name),
        text.as_deref(),
        0 as ::core::ffi::c_int,
    );
    if array == 0 {
        return 1 as u_int;
    }
    (1 as u_int).wrapping_add(window_customize_build_array(
        mode_owner, &top, scope, owner, option, ft,
    ))
}
unsafe fn window_customize_find_user_options(scope: &OptionsScope, list: &mut Vec<CString>) {
    let names = scope.with_local(|table| {
        crate::src::options::options_iter(table)
            .map(|entry| entry.name.clone())
            .collect::<Vec<_>>()
    });
    for name in names {
        if name.as_bytes().first() == Some(&b'@') && !list.contains(&name) {
            list.push(name);
        }
    }
}

unsafe fn window_customize_build_options(
    mode_owner: &Rc<UnsafeCell<window_customize_modedata>>,
    title: &CStr,
    group: u_int,
    scope0: window_customize_scope,
    oo0: Option<OptionsScope>,
    scope1: window_customize_scope,
    oo1: Option<OptionsScope>,
    scope2: window_customize_scope,
    oo2: Option<OptionsScope>,
    ft: *mut format_tree,
    filter: Option<&CStr>,
    fs: *mut cmd_find_state,
    type_0: window_customize_option_type,
) {
    let data = mode_owner.get();
    let oo0 = oo0.expect("global option scope");
    let top = mode_tree_add(
        &mut *(*data).tree_owner().get(),
        None,
        ModeTreeItemData::None,
        window_customize_top_tag(group),
        title,
        None,
        0,
    );
    mode_tree_no_tag(&top);
    let mut count = 0_u32;
    let mut names = Vec::new();
    window_customize_find_user_options(&oo0, &mut names);
    if let Some(owner) = &oo1 {
        window_customize_find_user_options(owner, &mut names);
    }
    if let Some(owner) = &oo2 {
        window_customize_find_user_options(owner, &mut names);
    }
    let scope_for = |owner: &OptionsScope| {
        if oo2.as_ref() == Some(owner) {
            scope2
        } else if oo1.as_ref() == Some(owner) {
            scope1
        } else {
            scope0
        }
    };
    for name in names {
        let option = oo2
            .as_ref()
            .and_then(|owner| window_customize_find_option(owner, &name))
            .or_else(|| {
                oo1.as_ref()
                    .and_then(|owner| window_customize_find_option(owner, &name))
            })
            .or_else(|| window_customize_find_option(&oo0, &name));
        let Some((owner, option)) = option else {
            continue;
        };
        count = count.wrapping_add(window_customize_build_option(
            mode_owner,
            &top,
            scope_for(&owner),
            &owner,
            option,
            ft,
            filter,
            fs,
            type_0,
        ));
    }
    let names = oo0.with_local(|table| {
        crate::src::options::options_iter(table)
            .map(|entry| entry.name.clone())
            .collect::<Vec<_>>()
    });
    for name in names {
        if oo0.with_entry(&name, |_| ()).is_none() {
            break;
        }
        if name.as_bytes().first() == Some(&b'@') {
            continue;
        }
        let owner = oo2.as_ref().or(oo1.as_ref()).unwrap_or(&oo0);
        let Some((owner, option)) = window_customize_find_option(owner, &name) else {
            continue;
        };
        count = count.wrapping_add(window_customize_build_option(
            mode_owner,
            &top,
            scope_for(&owner),
            &owner,
            option,
            ft,
            filter,
            fs,
            type_0,
        ));
    }
    if (*data).hide_default != 0 && count == 0 {
        mode_tree_remove(&mut *(*data).tree_owner().get(), &top);
    }
}

fn window_customize_key_detail(value: &[u8]) -> CString {
    let mut text = b"#[fg=themelightgrey]#[ignore]".to_vec();
    text.extend_from_slice(value);
    CString::new(text).expect("key detail contains no NUL")
}

unsafe fn window_customize_build_keys(
    mode_owner: &Rc<UnsafeCell<window_customize_modedata>>,
    kt: &key_table,
    mut ft: *mut format_tree,
    filter: Option<&CStr>,
    mut fs: *mut cmd_find_state,
) {
    let filter: *const ::core::ffi::c_char = filter.map_or(std::ptr::null(), CStr::as_ptr);
    let data = mode_owner.get();
    let mut count: u_int = 0 as u_int;
    let mut title_bytes = b"Key Table - ".to_vec();
    title_bytes.extend_from_slice(kt.name.as_bytes());
    let title = CString::new(title_bytes).expect("key table name contains no NUL");
    let top = mode_tree_add(
        &mut *(*data).tree_owner().get(),
        None,
        ModeTreeItemData::None,
        (1_u64 << 62) | kt.identity,
        &title,
        None,
        0 as ::core::ffi::c_int,
    );
    mode_tree_no_tag(&top);
    drop(title);
    let mut ft_owner = format_create_from_state(None, None, &*fs);
    ft = &raw mut *ft_owner;
    format_add(ft, c"is_option", |out| out.write_all(b"0"));
    format_add(ft, c"is_key", |out| out.write_all(b"1"));
    format_add(ft, c"is_environment", |out| out.write_all(b"0"));
    for bd in kt.key_bindings.iter() {
        if (*data).hide_default != 0 && window_customize_key_is_changed(kt, bd) == 0 {
            continue;
        } else {
            let key_string = key_string_format(bd.key, false);
            format_add(ft, c"key", |out| write_cstr(out, &*key_string));
            if !bd.note.is_none() {
                format_add(ft, c"key_note", |out| write_cstr(out, (bd.note).as_deref()));
            }
            if !filter.is_null() {
                let expanded = format_expand_cstring(ft, filter);
                if format_true(expanded.as_ptr()) == 0 {
                    continue;
                }
            }
            let item_owner = window_customize_add_item(
                &mut (*data).item_list,
                window_customize_itemdata {
                    type_0: WINDOW_CUSTOMIZE_ITEM_KEY,
                    scope: WINDOW_CUSTOMIZE_KEY,
                    table: Some(kt.name.clone()),
                    key: bd.key,
                    name: Some(key_string_format(bd.key, false)),
                    ..window_customize_itemdata::new()
                },
            );
            let expanded = format_expand_cstring(ft, (*data).format.as_ptr());
            let child = mode_tree_add(
                &mut *(*data).tree_owner().get(),
                Some(&top),
                ModeTreeItemData::Customize(item_owner.clone()),
                window_customize_key_tag(std::ptr::from_ref(bd).cast(), 0),
                &expanded,
                None,
                0 as ::core::ffi::c_int,
            );
            let tmp = cmd_list_print_cstring(&bd.cmdlist().borrow(), 0);
            let text = window_customize_key_detail(tmp.as_bytes());
            let mti = mode_tree_add(
                &mut *(*data).tree_owner().get(),
                Some(&child),
                ModeTreeItemData::Customize(item_owner.clone()),
                window_customize_key_tag(std::ptr::from_ref(bd).cast(), 1),
                c"Command",
                Some(&text),
                -(1 as ::core::ffi::c_int),
            );
            mode_tree_draw_as_parent(&mti);
            mode_tree_no_tag(&mti);
            drop(text);
            let text = if !bd.note.is_none() {
                window_customize_key_detail(
                    (bd.note).as_deref().expect("string is present").to_bytes(),
                )
            } else {
                CString::new(Vec::new()).expect("empty key note")
            };
            let mti = mode_tree_add(
                &mut *(*data).tree_owner().get(),
                Some(&child),
                ModeTreeItemData::Customize(item_owner.clone()),
                window_customize_key_tag(std::ptr::from_ref(bd).cast(), 2),
                c"Note",
                Some(&text),
                -(1 as ::core::ffi::c_int),
            );
            mode_tree_draw_as_parent(&mti);
            mode_tree_no_tag(&mti);
            drop(text);
            let flag = if bd.flags & KEY_BINDING_REPEAT != 0 {
                b"on".as_slice()
            } else {
                b"off".as_slice()
            };
            let text = window_customize_key_detail(flag);
            let mti = mode_tree_add(
                &mut *(*data).tree_owner().get(),
                Some(&child),
                ModeTreeItemData::Customize(item_owner.clone()),
                window_customize_key_tag(std::ptr::from_ref(bd).cast(), 3),
                c"Repeat",
                Some(&text),
                -(1 as ::core::ffi::c_int),
            );
            mode_tree_draw_as_parent(&mti);
            mode_tree_no_tag(&mti);
            drop(text);
            count = count.wrapping_add(1);
        }
    }
    drop(ft_owner);
    if (*data).hide_default != 0 && count == 0 as u_int {
        mode_tree_remove(&mut *(*data).tree_owner().get(), &top);
    }
}
unsafe fn window_customize_build_environment(
    mode_owner: &Rc<UnsafeCell<window_customize_modedata>>,
    title: &CStr,
    group: u_int,
    mut scope: window_customize_scope,
    target: CustomizeEnvironment,
    mut ft: *mut format_tree,
    filter: Option<&CStr>,
    mut fs: *mut cmd_find_state,
) {
    let filter: *const ::core::ffi::c_char = filter.map_or(std::ptr::null(), CStr::as_ptr);
    let data = mode_owner.get();
    let Some(environment) = target.resolve() else {
        return;
    };
    let Some(rows) = environment.rows() else {
        return;
    };

    let mut global: ::core::ffi::c_int = 0;
    if (*data).hide_default != 0 {
        return;
    }
    let top = mode_tree_add(
        &mut *(*data).tree_owner().get(),
        None,
        ModeTreeItemData::None,
        window_customize_top_tag(group),
        title,
        None,
        0 as ::core::ffi::c_int,
    );
    mode_tree_no_tag(&top);
    global = (scope as ::core::ffi::c_uint
        == WINDOW_CUSTOMIZE_GLOBAL_ENVIRONMENT as ::core::ffi::c_int as ::core::ffi::c_uint)
        as ::core::ffi::c_int;
    format_add(ft, c"is_option", |out| out.write_all(b"0"));
    format_add(ft, c"is_key", |out| out.write_all(b"0"));
    format_add(ft, c"is_environment", |out| out.write_all(b"1"));
    format_add(ft, c"environment_is_global", |out| {
        write!(out, "{}", (global) as i32)
    });
    let scope_text = window_customize_scope_text(scope, &*fs);
    format_add(ft, c"environment_scope", |out| {
        write_cstr(out, &*scope_text)
    });
    drop(scope_text);
    for (tag, entry) in rows {
        let envent = &entry;
        format_add(ft, c"environment_name", |out| {
            write_cstr(out, &*(envent.name))
        });
        format_add(ft, c"environment_hidden", |out| {
            write!(out, "{}", {
                (envent.flags & ENVIRON_HIDDEN != 0) as ::core::ffi::c_int
            })
        });
        format_add(ft, c"environment_removed", |out| {
            write!(out, "{}", {
                (envent.value
                    == if (NULL as *mut ::core::ffi::c_char).is_null() {
                        None
                    } else {
                        Some(
                            ::std::ffi::CStr::from_ptr(NULL as *mut ::core::ffi::c_char).to_owned(),
                        )
                    }) as ::core::ffi::c_int
            })
        });
        let value = if envent.value.is_none() {
            c""
        } else {
            (envent.value).as_deref().expect("string is present")
        };
        format_add(ft, c"environment_value", |out| write_cstr(out, &*value));
        if !filter.is_null() {
            let expanded = format_expand_cstring(ft, filter);
            if format_true(expanded.as_ptr()) == 0 {
                continue;
            }
        }
        let item_owner = window_customize_add_item(
            &mut (*data).item_list,
            window_customize_itemdata {
                type_0: WINDOW_CUSTOMIZE_ITEM_ENVIRONMENT,
                scope,
                environ: Some(target.clone()),
                environ_flags: envent.flags,
                name: Some(envent.name.clone()),
                ..window_customize_itemdata::new()
            },
        );
        let text;
        let name: Cow<'_, CStr> = if envent.value.is_none() {
            let entry_name = envent.name.as_c_str();
            let mut bytes = Vec::with_capacity(entry_name.to_bytes().len() + 1);
            bytes.push(b'-');
            bytes.extend_from_slice(entry_name.to_bytes());
            text = None;
            Cow::Owned(CString::new(bytes).expect("environment name contains no NUL"))
        } else {
            text = Some(format_expand_cstring(ft, (*data).format.as_ptr()));
            Cow::Borrowed(envent.name.as_c_str())
        };
        mode_tree_add(
            &mut *(*data).tree_owner().get(),
            Some(&top),
            ModeTreeItemData::Customize(item_owner.clone()),
            tag,
            &name,
            text.as_deref(),
            0 as ::core::ffi::c_int,
        );
    }
}
unsafe fn window_customize_live_mode(
    observer: &Weak<UnsafeCell<window_customize_modedata>>,
) -> Option<Rc<UnsafeCell<window_customize_modedata>>> {
    let owner = observer.upgrade()?;
    if (*owner.get()).dead != 0 {
        return None;
    }
    Some(owner)
}

unsafe fn window_customize_build(
    mode_owner: &Rc<UnsafeCell<window_customize_modedata>>,
    filter: Option<&CStr>,
) {
    let data = mode_owner.get();
    let Some(mode_pane_owner) = Rc::<UnsafeCell<window_pane>>::from_observer(&(*data).wp) else {
        return;
    };
    let mut fs: cmd_find_state = cmd_find_state {
        flags: 0,
        s: Default::default(),
        wl: Default::default(),
        w: Default::default(),
        wp: Default::default(),
        idx: 0,
    };
    let mut ft: *mut format_tree = ::core::ptr::null_mut::<format_tree>();
    (*data).item_list.clear();
    if (*data).fs.is_valid() {
        fs.copy_from(&(*data).fs);
    } else {
        fs = cmd_find_state::find_from_pane(&mode_pane_owner, 0 as ::core::ffi::c_int)
            .unwrap_or_else(|| cmd_find_state::clear_state(0 as ::core::ffi::c_int));
    }
    let mut ft_owner = format_create_from_state(None, None, &fs);
    ft = &raw mut *ft_owner;
    format_add(ft, c"is_option", |out| out.write_all(b"1"));
    format_add(ft, c"is_key", |out| out.write_all(b"0"));
    format_add(ft, c"is_environment", |out| out.write_all(b"0"));
    window_customize_build_options(
        mode_owner,
        c"Server Options",
        CUSTOMIZE_SERVER_OPTIONS,
        WINDOW_CUSTOMIZE_SERVER,
        Some(OptionsScope::GlobalServer),
        WINDOW_CUSTOMIZE_NONE,
        None,
        WINDOW_CUSTOMIZE_NONE,
        None,
        ft,
        filter,
        &raw mut fs,
        WINDOW_CUSTOMIZE_OPTIONS,
    );
    window_customize_build_options(
        mode_owner,
        c"Session Options",
        CUSTOMIZE_SESSION_OPTIONS,
        WINDOW_CUSTOMIZE_GLOBAL_SESSION,
        Some(OptionsScope::GlobalSession),
        WINDOW_CUSTOMIZE_SESSION,
        window_customize_get_tree(WINDOW_CUSTOMIZE_SESSION, &fs),
        WINDOW_CUSTOMIZE_NONE,
        None,
        ft,
        filter,
        &raw mut fs,
        WINDOW_CUSTOMIZE_OPTIONS,
    );
    window_customize_build_options(
        mode_owner,
        c"Window & Pane Options",
        CUSTOMIZE_WINDOW_OPTIONS,
        WINDOW_CUSTOMIZE_GLOBAL_WINDOW,
        Some(OptionsScope::GlobalWindow),
        WINDOW_CUSTOMIZE_WINDOW,
        window_customize_get_tree(WINDOW_CUSTOMIZE_WINDOW, &fs),
        WINDOW_CUSTOMIZE_PANE,
        window_customize_get_tree(WINDOW_CUSTOMIZE_PANE, &fs),
        ft,
        filter,
        &raw mut fs,
        WINDOW_CUSTOMIZE_OPTIONS,
    );
    window_customize_build_options(
        mode_owner,
        c"Session Hooks",
        CUSTOMIZE_SESSION_HOOKS,
        WINDOW_CUSTOMIZE_GLOBAL_SESSION,
        Some(OptionsScope::GlobalSession),
        WINDOW_CUSTOMIZE_SESSION,
        window_customize_get_tree(WINDOW_CUSTOMIZE_SESSION, &fs),
        WINDOW_CUSTOMIZE_NONE,
        None,
        ft,
        filter,
        &raw mut fs,
        WINDOW_CUSTOMIZE_HOOKS,
    );
    window_customize_build_options(
        mode_owner,
        c"Window & Pane Hooks",
        CUSTOMIZE_WINDOW_HOOKS,
        WINDOW_CUSTOMIZE_GLOBAL_WINDOW,
        Some(OptionsScope::GlobalWindow),
        WINDOW_CUSTOMIZE_WINDOW,
        window_customize_get_tree(WINDOW_CUSTOMIZE_WINDOW, &fs),
        WINDOW_CUSTOMIZE_PANE,
        window_customize_get_tree(WINDOW_CUSTOMIZE_PANE, &fs),
        ft,
        filter,
        &raw mut fs,
        WINDOW_CUSTOMIZE_HOOKS,
    );
    window_customize_build_environment(
        mode_owner,
        c"Global Environment",
        CUSTOMIZE_GLOBAL_ENVIRONMENT,
        WINDOW_CUSTOMIZE_GLOBAL_ENVIRONMENT,
        CustomizeEnvironment::Global,
        ft,
        filter,
        &raw mut fs,
    );
    window_customize_build_environment(
        mode_owner,
        c"Session Environment",
        CUSTOMIZE_SESSION_ENVIRONMENT,
        WINDOW_CUSTOMIZE_SESSION_ENVIRONMENT,
        CustomizeEnvironment::session(&fs.session_handle().expect("target session")),
        ft,
        filter,
        &raw mut fs,
    );
    drop(ft_owner);
    let mut ft_owner = format_create_from_state(None, None, &fs);
    ft = &raw mut *ft_owner;
    format_add(ft, c"is_environment", |out| out.write_all(b"0"));
    for table_owner in key_tables::snapshot() {
        let kt = table_owner.borrow();
        if !kt.key_bindings.storage.is_empty() {
            window_customize_build_keys(mode_owner, &kt, ft, filter, &raw mut fs);
        }
    }
    drop(ft_owner);
}
unsafe fn window_customize_draw_key(
    item: &window_customize_itemdata,
    mut ctx: *mut screen_write_ctx,
    mut sx: u_int,
    mut sy: u_int,
) {
    let mut s: *mut screen = (*ctx).screen_ptr();
    let mut cx: u_int = (*s).cx;
    let mut cy: u_int = (*s).cy;
    let mut note: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    let mut period: *const ::core::ffi::c_char = c"".as_ptr();
    let Some(table) = window_customize_get_key_table(item) else {
        return;
    };
    let kt = table.borrow();
    let bd = kt.key_bindings.get(item.key).expect("live binding");
    note = (bd.note)
        .as_ref()
        .map_or(::core::ptr::null_mut(), |value| value.as_ptr().cast_mut());
    if note.is_null() {
        note = c"There is no note for this key.".as_ptr();
    }
    if *note as ::core::ffi::c_int != '\0' as i32
        && *note.add(strlen(note).wrapping_sub(1 as size_t)) as ::core::ffi::c_int != '.' as i32
    {
        period = c".".as_ptr();
    }
    if screen_write_text(
        &mut *ctx,
        cx,
        sx,
        sy,
        0 as ::core::ffi::c_int,
        &grid_default_cell,
        |out| {
            write_cstr(out, nullable_cstr(note))?;
            write_cstr(out, nullable_cstr(period))
        },
    ) == 0
    {
        return;
    }
    screen_write_cursormove(
        &mut *ctx,
        cx as ::core::ffi::c_int,
        (*s).cy.wrapping_add(1 as u_int) as ::core::ffi::c_int,
        0 as ::core::ffi::c_int,
    );
    if (*s).cy >= cy.wrapping_add(sy).wrapping_sub(1 as u_int) {
        return;
    }
    if screen_write_text(
        &mut *ctx,
        cx,
        sx,
        sy.wrapping_sub((*s).cy.wrapping_sub(cy)),
        0 as ::core::ffi::c_int,
        &grid_default_cell,
        |out| {
            out.write_all(b"This key is in the ")?;
            write_cstr(out, &*(kt.name))?;
            out.write_all(b" table.")
        },
    ) == 0
    {
        return;
    }
    if window_customize_write_value(
        ctx,
        cx,
        sx,
        sy.wrapping_sub((*s).cy.wrapping_sub(cy)),
        0 as ::core::ffi::c_int,
        c"Repeat: ",
        |out| {
            write_cstr(
                out,
                nullable_cstr(if bd.flags & KEY_BINDING_REPEAT != 0 {
                    c"on".as_ptr()
                } else {
                    c"off".as_ptr()
                }),
            )
        },
    ) == 0
    {
        return;
    }
    screen_write_cursormove(
        &mut *ctx,
        cx as ::core::ffi::c_int,
        (*s).cy.wrapping_add(1 as u_int) as ::core::ffi::c_int,
        0 as ::core::ffi::c_int,
    );
    if (*s).cy >= cy.wrapping_add(sy).wrapping_sub(1 as u_int) {
        return;
    }
    let cmd = cmd_list_print_cstring(&bd.cmdlist().borrow(), 0);
    if window_customize_write_value(
        ctx,
        cx,
        sx,
        sy.wrapping_sub((*s).cy.wrapping_sub(cy)),
        0 as ::core::ffi::c_int,
        c"Command: ",
        |out| write_cstr(out, &*cmd),
    ) == 0
    {
        return;
    }
    if let Some(default_bd) = key_bindings_get_default(&kt, bd.key) {
        let default_cmd = cmd_list_print_cstring(&default_bd.cmdlist().borrow(), 0);
        if cmd.as_bytes() != default_cmd.as_bytes()
            && window_customize_write_value(
                ctx,
                cx,
                sx,
                sy.wrapping_sub((*s).cy.wrapping_sub(cy)),
                0 as ::core::ffi::c_int,
                c"The default is: ",
                |out| write_cstr(out, &*default_cmd),
            ) == 0
        {}
    }
}
unsafe fn window_customize_draw_option(
    mode_owner: &Rc<UnsafeCell<window_customize_modedata>>,
    item: &window_customize_itemdata,
    mut ctx: *mut screen_write_ctx,
    mut sx: u_int,
    mut sy: u_int,
) {
    let data = mode_owner.get();
    let mut current_block: u64;
    let mut s: *mut screen = (*ctx).screen_ptr();
    let mut cx: u_int = (*s).cx;
    let mut cy: u_int = (*s).cy;
    let mut go: Option<OptionsScope> = None;
    let mut wo: Option<OptionsScope> = None;
    let mut oe: *const options_table_entry = ::core::ptr::null::<options_table_entry>();
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
    let mut text: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    let mut name: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    let mut array_key: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    let mut space: *const ::core::ffi::c_char = c"".as_ptr();
    let mut unit: *const ::core::ffi::c_char = c"".as_ptr();
    let mut value: *mut ::core::ffi::c_char = ::core::ptr::null_mut::<::core::ffi::c_char>();
    let mut value_owner: Option<CString> = None;
    let mut label: [u8; 64] = [0; 64];
    let mut default_value: Option<CString> = None;
    let mut choices: [::core::ffi::c_char; 256] = ::core::mem::transmute::<
        [u8; 256],
        [::core::ffi::c_char; 256],
    >(
        *b"\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
    );
    let mut fs: cmd_find_state = cmd_find_state {
        flags: 0,
        s: Default::default(),
        wl: Default::default(),
        w: Default::default(),
        wp: Default::default(),
        idx: 0,
    };
    let mut ft: *mut format_tree = ::core::ptr::null_mut::<format_tree>();
    let mut is_hook: ::core::ffi::c_int = 0;
    let mut is_monitor: ::core::ffi::c_int = 0;
    let mut is_user_hook: ::core::ffi::c_int = 0;
    let mut is_any_hook: ::core::ffi::c_int = 0;
    if window_customize_check_item(&*data, item, Some(&mut fs)) == 0 {
        return;
    }
    name = (item.name)
        .as_ref()
        .map_or(::core::ptr::null_mut(), |value| value.as_ptr().cast_mut());
    array_key = (item.array_key)
        .as_ref()
        .map_or(::core::ptr::null_mut(), |value| value.as_ptr().cast_mut());
    let Some((owner, option)) = window_customize_find_option(
        item.oo.as_ref().expect("option scope"),
        CStr::from_ptr(name),
    ) else {
        return;
    };
    oe = (*option).tableentry.map_or(std::ptr::null(), |entry| {
        entry as *const options_table_entry
    });
    is_hook = (!oe.is_null() && (*oe).flags & OPTIONS_TABLE_IS_HOOK != 0) as ::core::ffi::c_int;
    is_monitor = (*option).monitor_data.is_some() as ::core::ffi::c_int;
    is_user_hook = (*name as ::core::ffi::c_int == '@' as i32
        && hooks_is_event(CStr::from_ptr(name)) != 0) as ::core::ffi::c_int;
    is_any_hook = (is_hook != 0 || is_monitor != 0 || is_user_hook != 0) as ::core::ffi::c_int;
    if !oe.is_null() && (*oe).unit.is_some() {
        space = c" ".as_ptr();
        unit = (*oe).unit.map_or(std::ptr::null(), CStr::as_ptr);
    }
    let mut ft_owner = format_create_from_state(None, None, &fs);
    ft = &raw mut *ft_owner;
    if oe.is_null() || (*oe).text.is_none() {
        if is_monitor != 0 {
            text = c"This hook runs when a monitor changes.".as_ptr();
        } else if is_user_hook != 0 {
            text = c"This hook doesn't have a description.".as_ptr();
        } else {
            text = c"This option doesn't have a description.".as_ptr();
        }
    } else {
        text = (*oe).text.map_or(std::ptr::null(), CStr::as_ptr);
    }
    if !(screen_write_text(
        &mut *ctx,
        cx,
        sx,
        sy,
        0 as ::core::ffi::c_int,
        &grid_default_cell,
        |out| write_cstr(out, nullable_cstr(text)),
    ) == 0)
    {
        screen_write_cursormove(
            &mut *ctx,
            cx as ::core::ffi::c_int,
            (*s).cy.wrapping_add(1 as u_int) as ::core::ffi::c_int,
            0 as ::core::ffi::c_int,
        );
        if !((*s).cy >= cy.wrapping_add(sy).wrapping_sub(1 as u_int)) {
            if is_monitor != 0 {
                if screen_write_text(
                    &mut *ctx,
                    cx,
                    sx,
                    sy.wrapping_sub((*s).cy.wrapping_sub(cy)),
                    0 as ::core::ffi::c_int,
                    &grid_default_cell,
                    |out| out.write_all(b"This is a monitor hook."),
                ) == 0
                {
                    current_block = 4086289836260337793;
                } else {
                    current_block = 14134146928577265803;
                }
            } else {
                if oe.is_null() {
                    text = c"user".as_ptr();
                } else if (*oe).scope & (OPTIONS_TABLE_WINDOW | OPTIONS_TABLE_PANE)
                    == OPTIONS_TABLE_WINDOW | OPTIONS_TABLE_PANE
                {
                    text = c"window and pane".as_ptr();
                } else if (*oe).scope & OPTIONS_TABLE_WINDOW != 0 {
                    text = c"window".as_ptr();
                } else if (*oe).scope & OPTIONS_TABLE_SESSION != 0 {
                    text = c"session".as_ptr();
                } else {
                    text = c"server".as_ptr();
                }
                if is_user_hook != 0 {
                    if screen_write_text(
                        &mut *ctx,
                        cx,
                        sx,
                        sy.wrapping_sub((*s).cy.wrapping_sub(cy)),
                        0 as ::core::ffi::c_int,
                        &grid_default_cell,
                        |out| out.write_all(b"This is a user hook."),
                    ) == 0
                    {
                        current_block = 4086289836260337793;
                    } else {
                        current_block = 14134146928577265803;
                    }
                } else if is_hook != 0 {
                    if screen_write_text(
                        &mut *ctx,
                        cx,
                        sx,
                        sy.wrapping_sub((*s).cy.wrapping_sub(cy)),
                        0 as ::core::ffi::c_int,
                        &grid_default_cell,
                        |out| {
                            out.write_all(b"This is a ")?;
                            write_cstr(out, nullable_cstr(text))?;
                            out.write_all(b" hook.")
                        },
                    ) == 0
                    {
                        current_block = 4086289836260337793;
                    } else {
                        current_block = 14134146928577265803;
                    }
                } else if screen_write_text(
                    &mut *ctx,
                    cx,
                    sx,
                    sy.wrapping_sub((*s).cy.wrapping_sub(cy)),
                    0 as ::core::ffi::c_int,
                    &grid_default_cell,
                    |out| {
                        out.write_all(b"This is a ")?;
                        write_cstr(out, nullable_cstr(text))?;
                        out.write_all(b" option.")
                    },
                ) == 0
                {
                    current_block = 4086289836260337793;
                } else {
                    current_block = 14134146928577265803;
                }
            }
            match current_block {
                4086289836260337793 => {}
                _ => {
                    if let Some(monitor) = hooks_monitor_to_cstring(option) {
                        if window_customize_write_value(
                            ctx,
                            cx,
                            sx,
                            sy.wrapping_sub((*s).cy.wrapping_sub(cy)),
                            0 as ::core::ffi::c_int,
                            c"Monitor: ",
                            |out| write_cstr(out, &*monitor),
                        ) == 0
                        {
                            current_block = 4086289836260337793;
                        } else {
                            current_block = 13460095289871124136;
                        }
                    } else {
                        current_block = 13460095289871124136;
                    }
                    match current_block {
                        4086289836260337793 => {}
                        _ => {
                            if !oe.is_null() && (*oe).flags & OPTIONS_TABLE_IS_ARRAY != 0 {
                                if is_hook != 0 {
                                    if array_key.is_null() {
                                        if screen_write_text(
                                            &mut *ctx,
                                            cx,
                                            sx,
                                            sy.wrapping_sub((*s).cy.wrapping_sub(cy)),
                                            0 as ::core::ffi::c_int,
                                            &grid_default_cell,
                                            |out| out.write_all(b"This is an array hook."),
                                        ) == 0
                                        {
                                            current_block = 4086289836260337793;
                                        } else {
                                            window_customize_write_hook_fire(
                                                ctx,
                                                cx,
                                                sx,
                                                sy.wrapping_sub((*s).cy.wrapping_sub(cy)),
                                                &owner,
                                                option,
                                            );
                                            current_block = 4086289836260337793;
                                        }
                                    } else {
                                        current_block = 168769493162332264;
                                    }
                                } else if !array_key.is_null() {
                                    if screen_write_text(
                                        &mut *ctx,
                                        cx,
                                        sx,
                                        sy.wrapping_sub((*s).cy.wrapping_sub(cy)),
                                        0 as ::core::ffi::c_int,
                                        &grid_default_cell,
                                        |out| {
                                            out.write_all(b"This is an array option, key ")?;
                                            write_cstr(out, nullable_cstr(array_key))?;
                                            out.write_all(b".")
                                        },
                                    ) == 0
                                    {
                                        current_block = 4086289836260337793;
                                    } else {
                                        current_block = 168769493162332264;
                                    }
                                } else if screen_write_text(
                                    &mut *ctx,
                                    cx,
                                    sx,
                                    sy.wrapping_sub((*s).cy.wrapping_sub(cy)),
                                    0 as ::core::ffi::c_int,
                                    &grid_default_cell,
                                    |out| out.write_all(b"This is an array option."),
                                ) == 0
                                {
                                    current_block = 4086289836260337793;
                                } else {
                                    current_block = 168769493162332264;
                                }
                                match current_block {
                                    4086289836260337793 => {}
                                    _ => {
                                        if array_key.is_null() {
                                            current_block = 4086289836260337793;
                                        } else {
                                            current_block = 8835654301469918283;
                                        }
                                    }
                                }
                            } else {
                                current_block = 8835654301469918283;
                            }
                            match current_block {
                                4086289836260337793 => {}
                                _ => {
                                    screen_write_cursormove(
                                        &mut *ctx,
                                        cx as ::core::ffi::c_int,
                                        (*s).cy.wrapping_add(1 as u_int) as ::core::ffi::c_int,
                                        0 as ::core::ffi::c_int,
                                    );
                                    if !((*s).cy >= cy.wrapping_add(sy).wrapping_sub(1 as u_int)) {
                                        value_owner = Some(options_to_cstring(
                                            option,
                                            item.array_key.as_deref(),
                                            0,
                                        ));
                                        value = value_owner.as_ref().unwrap().as_ptr().cast_mut();
                                        if !oe.is_null() && array_key.is_null() {
                                            let rendered = options_default_to_cstring(&*oe);
                                            if strcmp(rendered.as_ptr(), value)
                                                != 0 as ::core::ffi::c_int
                                            {
                                                default_value = Some(rendered);
                                            }
                                        }
                                        if is_any_hook != 0 {
                                            if window_customize_write_value(
                                                ctx,
                                                cx,
                                                sx,
                                                sy.wrapping_sub((*s).cy.wrapping_sub(cy)),
                                                0 as ::core::ffi::c_int,
                                                c"Hook command: ",
                                                |out| {
                                                    write_cstr(out, nullable_cstr(value))?;
                                                    write_cstr(out, nullable_cstr(space))?;
                                                    write_cstr(out, nullable_cstr(unit))
                                                },
                                            ) == 0
                                                || (window_customize_write_hook_fire(
                                                    ctx,
                                                    cx,
                                                    sx,
                                                    sy.wrapping_sub((*s).cy.wrapping_sub(cy)),
                                                    &owner,
                                                    option,
                                                ) == 0)
                                            {
                                                current_block = 4086289836260337793;
                                            } else {
                                                current_block = 7385833325316299293;
                                            }
                                        } else if window_customize_write_value(
                                            ctx,
                                            cx,
                                            sx,
                                            sy.wrapping_sub((*s).cy.wrapping_sub(cy)),
                                            0 as ::core::ffi::c_int,
                                            c"Option value: ",
                                            |out| {
                                                write_cstr(out, nullable_cstr(value))?;
                                                write_cstr(out, nullable_cstr(space))?;
                                                write_cstr(out, nullable_cstr(unit))
                                            },
                                        ) == 0
                                        {
                                            current_block = 4086289836260337793;
                                        } else {
                                            current_block = 7385833325316299293;
                                        }
                                        match current_block {
                                            4086289836260337793 => {}
                                            _ => {
                                                if oe.is_null()
                                                    || (*oe).type_0 as ::core::ffi::c_uint
                                                        == OPTIONS_TABLE_STRING
                                                            as ::core::ffi::c_int
                                                            as ::core::ffi::c_uint
                                                {
                                                    let expanded = format_expand_cstring(ft, value);
                                                    if strcmp(expanded.as_ptr(), value)
                                                        != 0 as ::core::ffi::c_int
                                                    {
                                                        if window_customize_write_value(
                                                            ctx,
                                                            cx,
                                                            sx,
                                                            sy.wrapping_sub(
                                                                (*s).cy.wrapping_sub(cy),
                                                            ),
                                                            0 as ::core::ffi::c_int,
                                                            c"This expands to: ",
                                                            |out| write_cstr(out, &*expanded),
                                                        ) == 0
                                                        {
                                                            current_block = 4086289836260337793;
                                                        } else {
                                                            current_block = 479107131381816815;
                                                        }
                                                    } else {
                                                        current_block = 479107131381816815;
                                                    }
                                                    match current_block {
                                                        4086289836260337793 => {}
                                                        _ => {
                                                            current_block = 16108440464692313034;
                                                        }
                                                    }
                                                } else {
                                                    current_block = 16108440464692313034;
                                                }
                                                match current_block {
                                                    4086289836260337793 => {}
                                                    _ => {
                                                        if !oe.is_null()
                                                            && (*oe).type_0 as ::core::ffi::c_uint
                                                                == OPTIONS_TABLE_CHOICE
                                                                    as ::core::ffi::c_int
                                                                    as ::core::ffi::c_uint
                                                        {
                                                            for choice in (*oe).choices {
                                                                strlcat(
                                                                    &raw mut choices
                                                                        as *mut ::core::ffi::c_char,
                                                                    choice.as_ptr(),
                                                                    ::core::mem::size_of::<
                                                                        [::core::ffi::c_char; 256],
                                                                    >(
                                                                    )
                                                                        as size_t,
                                                                );
                                                                strlcat(
                                                                    &raw mut choices
                                                                        as *mut ::core::ffi::c_char,
                                                                    c", ".as_ptr(),
                                                                    ::core::mem::size_of::<
                                                                        [::core::ffi::c_char; 256],
                                                                    >(
                                                                    )
                                                                        as size_t,
                                                                );
                                                            }
                                                            choices[strlen(
                                                                &raw mut choices
                                                                    as *mut ::core::ffi::c_char,
                                                            )
                                                            .wrapping_sub(2 as size_t)
                                                                as usize] =
                                                                '\0' as i32 as ::core::ffi::c_char;
                                                            if window_customize_write_value(
                                                                ctx,
                                                                cx,
                                                                sx,
                                                                sy.wrapping_sub(
                                                                    (*s).cy.wrapping_sub(cy),
                                                                ),
                                                                0 as ::core::ffi::c_int,
                                                                c"Available values are: ",
                                                                |out| {
                                                                    write_cstr(out, nullable_cstr(&raw mut choices
                                                                    as *mut ::core::ffi::c_char))
                                                                },
                                                            ) == 0
                                                            {
                                                                current_block = 4086289836260337793;
                                                            } else {
                                                                current_block =
                                                                    17688141731389699982;
                                                            }
                                                        } else {
                                                            current_block = 17688141731389699982;
                                                        }
                                                        match current_block {
                                                            4086289836260337793 => {}
                                                            _ => {
                                                                if !oe.is_null()
                                                                    && (*oe).type_0
                                                                        as ::core::ffi::c_uint
                                                                        == OPTIONS_TABLE_COLOUR
                                                                            as ::core::ffi::c_int
                                                                            as ::core::ffi::c_uint
                                                                {
                                                                    if screen_write_text(
                                                                        &mut *ctx,
                                                                        cx,
                                                                        sx,
                                                                        sy.wrapping_sub(
                                                                            (*s).cy
                                                                                .wrapping_sub(cy),
                                                                        ),
                                                                        1 as ::core::ffi::c_int,
                                                                        &grid_default_cell,
                                                                        |out| {
                                                                            out.write_all(b"This is a colour option: ")
                                                                        },
                                                                    ) == 0
                                                                    {
                                                                        current_block =
                                                                            4086289836260337793;
                                                                    } else {
                                                                        gc = grid_default_cell;
                                                                        gc.fg = item
                                                                            .oo
                                                                            .as_ref()
                                                                            .expect("option scope")
                                                                            .with_local(|table| {
                                                                                options_get_number_ref(
                                                                                    table, CStr::from_ptr(name),
                                                                                )
                                                                            })
                                                                            as ::core::ffi::c_int;
                                                                        if screen_write_text(
                                                                            &mut *ctx,
                                                                            cx,
                                                                            sx,
                                                                            sy.wrapping_sub(
                                                                                (*s).cy
                                                                                    .wrapping_sub(
                                                                                        cy,
                                                                                    ),
                                                                            ),
                                                                            0 as ::core::ffi::c_int,
                                                                            &gc,
                                                                            |out| {
                                                                                out.write_all(
                                                                                    b"EXAMPLE",
                                                                                )
                                                                            },
                                                                        ) == 0
                                                                        {
                                                                            current_block =
                                                                                4086289836260337793;
                                                                        } else {
                                                                            current_block = 10887629115603254199;
                                                                        }
                                                                    }
                                                                } else {
                                                                    current_block =
                                                                        10887629115603254199;
                                                                }
                                                                match current_block {
                                                                    4086289836260337793 => {}
                                                                    _ => {
                                                                        if !oe.is_null()
                                                                            && (*oe).flags & OPTIONS_TABLE_IS_COLOUR != 0
                                                                        {
                                                                            if screen_write_text(&mut *ctx,
cx,
sx,
sy.wrapping_sub((*s).cy.wrapping_sub(cy)),
1 as ::core::ffi::c_int,
&grid_default_cell,
|out| {
out.write_all(b"This is a colour option: ")
}) == 0
                                                                            {
                                                                                current_block = 4086289836260337793;
                                                                            } else {
                                                                                crate::src::style::style_apply_with_options(&mut gc, CStr::from_ptr(name), ft.as_mut(), |visit| item.oo.as_ref().expect("option scope").with_local(visit));
                                                                                if screen_write_text(&mut *ctx,
cx,
sx,
sy.wrapping_sub((*s).cy.wrapping_sub(cy)),
0 as ::core::ffi::c_int,
&gc,
|out| {
out.write_all(b"EXAMPLE")
}) == 0
                                                                                {
                                                                                    current_block = 4086289836260337793;
                                                                                } else {
                                                                                    current_block = 17995254032144898061;
                                                                                }
                                                                            }
                                                                        } else {
                                                                            current_block = 17995254032144898061;
                                                                        }
                                                                        match current_block {
                                                                            4086289836260337793 => {
                                                                            }
                                                                            _ => {
                                                                                if !oe.is_null()
                                                                                    && (*oe).flags & OPTIONS_TABLE_IS_STYLE != 0
                                                                                {
                                                                                    if screen_write_text(&mut *ctx,
cx,
sx,
sy.wrapping_sub((*s).cy.wrapping_sub(cy)),
1 as ::core::ffi::c_int,
&grid_default_cell,
|out| {
out.write_all(b"This is a style option: ")
}) == 0
                                                                                    {
                                                                                        current_block = 4086289836260337793;
                                                                                    } else {
                                                                                        crate::src::style::style_apply_with_options(&mut gc, CStr::from_ptr(name), ft.as_mut(), |visit| item.oo.as_ref().expect("option scope").with_local(visit));
                                                                                        if screen_write_text(&mut *ctx,
cx,
sx,
sy.wrapping_sub((*s).cy.wrapping_sub(cy)),
0 as ::core::ffi::c_int,
&gc,
|out| {
out.write_all(b"EXAMPLE")
}) == 0
                                                                                        {
                                                                                            current_block = 4086289836260337793;
                                                                                        } else {
                                                                                            current_block = 1934991416718554651;
                                                                                        }
                                                                                    }
                                                                                } else {
                                                                                    current_block = 1934991416718554651;
                                                                                }
                                                                                match current_block {
                                                                                    4086289836260337793 => {}
                                                                                    _ => {
                                                                                        if let Some(default_value) = &default_value {
                                                                                            if window_customize_write_value(ctx,
cx,
sx,
sy.wrapping_sub((*s).cy.wrapping_sub(cy)),
0 as ::core::ffi::c_int,
c"The default is: ",
|out| {
write_cstr(out, default_value.as_c_str())?;
write_cstr(out, nullable_cstr(space))?;
write_cstr(out, nullable_cstr(unit))
}) == 0
                                                                                            {
                                                                                                current_block = 4086289836260337793;
                                                                                            } else {
                                                                                                current_block = 15622658527355336244;
                                                                                            }
                                                                                        } else {
                                                                                            current_block = 15622658527355336244;
                                                                                        }
                                                                                        match current_block {
                                                                                            4086289836260337793 => {}
                                                                                            _ => {
                                                                                                screen_write_cursormove(
                                                                                                    &mut *ctx,
                                                                                                    cx as ::core::ffi::c_int,
                                                                                                    (*s).cy.wrapping_add(1 as u_int) as ::core::ffi::c_int,
                                                                                                    0 as ::core::ffi::c_int,
                                                                                                );
                                                                                                if !((*s).cy > cy.wrapping_add(sy).wrapping_sub(1 as u_int))
                                                                                                {
                                                                                                    if !oe.is_null()
                                                                                                        && (*oe).flags & OPTIONS_TABLE_IS_ARRAY != 0
                                                                                                    {
                                                                                                        wo = None;
                                                                                                        go = None;
                                                                                                    } else {
                                                                                                        match item.scope as ::core::ffi::c_uint {
                                                                                                            7 => {
                                                                                                                wo = item.oo.as_ref().expect("option scope").parent();
                                                                                                                go = wo.as_ref().and_then(|parent| parent.parent());
                                                                                                            }
                                                                                                            6 | 4 => {
                                                                                                                wo = None;
                                                                                                                go = item.oo.as_ref().expect("option scope").parent();
                                                                                                            }
                                                                                                            _ => {
                                                                                                                wo = None;
                                                                                                                go = None;
                                                                                                            }
                                                                                                        }
                                                                                                    }
                                                                                                    if wo.as_ref().is_some_and(|parent| parent != &owner) {
                                                                                                        let parent_value = wo.as_ref().expect("parent scope").with_entry(CStr::from_ptr(name), |entry| options_to_cstring(entry, nullable_cstr(std::ptr::null()), 0));
                                                                                                        if let Some(parent_value) = parent_value {
                                                                                                            value_owner = Some(parent_value);
                                                                                                            value = value_owner.as_ref().unwrap().as_ptr().cast_mut();
                                                                                                            xformat(&mut label, format_args!("Window value (from window {}): " , ((fs.winlink_handle()).get_unchecked().idx) as u32));
                                                                                                            if window_customize_write_value(ctx,
(*s).cx,
sx,
sy.wrapping_sub((*s).cy.wrapping_sub(cy)),
0 as ::core::ffi::c_int,
CStr::from_ptr(&raw mut label as *mut ::core::ffi::c_char),
|out| {
write_cstr(out, nullable_cstr(value))?;
write_cstr(out, nullable_cstr(space))?;
write_cstr(out, nullable_cstr(unit))
}) == 0
                                                                                                            {
                                                                                                                current_block = 4086289836260337793;
                                                                                                            } else {
                                                                                                                current_block = 9073771928613846474;
                                                                                                            }
                                                                                                        } else {
                                                                                                            current_block = 9073771928613846474;
                                                                                                        }
                                                                                                    } else {
                                                                                                        current_block = 9073771928613846474;
                                                                                                    }
                                                                                                    match current_block {
                                                                                                        4086289836260337793 => {}
                                                                                                        _ => {
                                                                                                            if go.as_ref().is_some_and(|parent| parent != &owner) {
                                                                                                                let parent_value = go.as_ref().expect("parent scope").with_entry(CStr::from_ptr(name), |entry| options_to_cstring(entry, nullable_cstr(std::ptr::null()), 0));
                                                                                                                if let Some(parent_value) = parent_value {
                                                                                                                    value_owner = Some(parent_value);
                                                                                                                    value = value_owner.as_ref().unwrap().as_ptr().cast_mut();
                                                                                                                    window_customize_write_value(ctx,
(*s).cx,
sx,
sy.wrapping_sub((*s).cy.wrapping_sub(cy)),
0 as ::core::ffi::c_int,
c"Global value: ",
|out| {
write_cstr(out, nullable_cstr(value))?;
write_cstr(out, nullable_cstr(space))?;
write_cstr(out, nullable_cstr(unit))
});
                                                                                                                }
                                                                                                            }
                                                                                                        }
                                                                                                    }
                                                                                                }
                                                                                            }
                                                                                        }
                                                                                    }
                                                                                }
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    drop(default_value);
    drop(ft_owner);
}
unsafe fn window_customize_draw_environment(
    mode_owner: &Rc<UnsafeCell<window_customize_modedata>>,
    item: &window_customize_itemdata,
    mut ctx: *mut screen_write_ctx,
    mut sx: u_int,
    mut sy: u_int,
) {
    let data = mode_owner.get();
    let Some(mut environment) = item
        .environ
        .as_ref()
        .and_then(CustomizeEnvironment::resolve)
    else {
        return;
    };
    let Some(env) = environment.get() else {
        return;
    };

    let mut s: *mut screen = (*ctx).screen_ptr();
    let mut cx: u_int = (*s).cx;
    let mut cy: u_int = (*s).cy;
    let mut envent: Option<&environ_entry> = None;
    let mut parent: Option<&environ_entry> = None;
    let mut fs: cmd_find_state = cmd_find_state {
        flags: 0,
        s: Default::default(),
        wl: Default::default(),
        w: Default::default(),
        wp: Default::default(),
        idx: 0,
    };
    let mut text: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    if window_customize_check_item(&*data, item, Some(&mut fs)) == 0 {
        return;
    }
    envent = environ_find(&env, (item.name).as_deref().expect("string argument"));
    if envent.is_none() {
        return;
    }
    if item.scope as ::core::ffi::c_uint
        == WINDOW_CUSTOMIZE_GLOBAL_ENVIRONMENT as ::core::ffi::c_int as ::core::ffi::c_uint
    {
        text = c"global".as_ptr();
    } else {
        text = c"session".as_ptr();
    }
    if screen_write_text(
        &mut *ctx,
        cx,
        sx,
        sy,
        0 as ::core::ffi::c_int,
        &grid_default_cell,
        |out| {
            out.write_all(b"This is a ")?;
            write_cstr(out, nullable_cstr(text))?;
            out.write_all(b" environment variable.")
        },
    ) == 0
    {
        return;
    }
    if envent.unwrap().flags & ENVIRON_HIDDEN != 0
        && screen_write_text(
            &mut *ctx,
            cx,
            sx,
            sy.wrapping_sub((*s).cy.wrapping_sub(cy)),
            0 as ::core::ffi::c_int,
            &grid_default_cell,
            |out| out.write_all(b"This variable is hidden."),
        ) == 0
    {
        return;
    }
    screen_write_cursormove(
        &mut *ctx,
        cx as ::core::ffi::c_int,
        (*s).cy.wrapping_add(1 as u_int) as ::core::ffi::c_int,
        0 as ::core::ffi::c_int,
    );
    if (*s).cy >= cy.wrapping_add(sy).wrapping_sub(1 as u_int) {
        return;
    }
    if envent.unwrap().value.is_none() {
        if screen_write_text(
            &mut *ctx,
            cx,
            sx,
            sy.wrapping_sub((*s).cy.wrapping_sub(cy)),
            0 as ::core::ffi::c_int,
            &grid_default_cell,
            |out| out.write_all(b"Variable is removed."),
        ) == 0
        {
            return;
        }
    } else if window_customize_write_value(
        ctx,
        cx,
        sx,
        sy.wrapping_sub((*s).cy.wrapping_sub(cy)),
        0 as ::core::ffi::c_int,
        c"Variable value: ",
        |out| write_cstr(out, (envent.unwrap().value).as_deref()),
    ) == 0
    {
        return;
    }
    if item.scope as ::core::ffi::c_uint
        != WINDOW_CUSTOMIZE_SESSION_ENVIRONMENT as ::core::ffi::c_int as ::core::ffi::c_uint
    {
        return;
    }
    parent = environ_find(
        global_environ.as_deref().expect("environment"),
        (item.name).as_deref().expect("string argument"),
    );
    if parent.is_none() {
        return;
    }
    if parent.unwrap().value.is_none() {
        if screen_write_text(
            &mut *ctx,
            cx,
            sx,
            sy.wrapping_sub((*s).cy.wrapping_sub(cy)),
            0 as ::core::ffi::c_int,
            &grid_default_cell,
            |out| out.write_all(b"Global variable is removed."),
        ) == 0
        {}
    } else if window_customize_write_value(
        ctx,
        cx,
        sx,
        sy.wrapping_sub((*s).cy.wrapping_sub(cy)),
        0 as ::core::ffi::c_int,
        c"Global value: ",
        |out| write_cstr(out, (parent.unwrap().value).as_deref()),
    ) == 0
    {
    }
}
unsafe fn window_customize_draw(
    mode_owner: &Rc<UnsafeCell<window_customize_modedata>>,
    item: &window_customize_itemdata,
    ctx: &mut screen_write_ctx,
    mut sx: u_int,
    mut sy: u_int,
) {
    if item.type_0 as ::core::ffi::c_uint
        == WINDOW_CUSTOMIZE_ITEM_KEY as ::core::ffi::c_int as ::core::ffi::c_uint
    {
        window_customize_draw_key(item, ctx, sx, sy);
    } else if item.type_0 as ::core::ffi::c_uint
        == WINDOW_CUSTOMIZE_ITEM_ENVIRONMENT as ::core::ffi::c_int as ::core::ffi::c_uint
    {
        window_customize_draw_environment(mode_owner, item, ctx, sx, sy);
    } else {
        window_customize_draw_option(mode_owner, item, ctx, sx, sy);
    };
}
unsafe fn window_customize_menu(
    mode_owner: &Rc<UnsafeCell<window_customize_modedata>>,
    c: &ClientRef,
    mut key: key_code,
) {
    let data = mode_owner.get();
    let Some(mode_pane_owner) = Rc::<UnsafeCell<window_pane>>::from_observer(&(*data).wp) else {
        return;
    };
    let mut wme: refbox::Weak<window_mode_entry> = refbox::Weak::new();
    wme = mode_pane_owner.mode_entry();
    if !wme.is_alive()
        || wme
            .get_unchecked()
            .shared_data_ptr::<window_customize_modedata>()
            != Some(data)
    {
        return;
    }
    window_customize_key(
        wme.clone(),
        c,
        (refbox::Weak::new()).clone(),
        key,
        ::core::ptr::null_mut::<mouse_event>(),
    );
}
unsafe fn window_customize_height() -> u_int {
    12 as u_int
}
static window_customize_help_lines: &[&CStr] = &[
    c"#[fg=themelightgrey]   Enter, s #[#{E:tree-mode-border-style},acs]x#[default] Set %1 value",
    c"#[fg=themelightgrey]          S #[#{E:tree-mode-border-style},acs]x#[default] Set global %1 value",
    c"#[fg=themelightgrey]          w #[#{E:tree-mode-border-style},acs]x#[default] Set window %1 value",
    c"#[fg=themelightgrey]          d #[#{E:tree-mode-border-style},acs]x#[default] Set to default value",
    c"#[fg=themelightgrey]          D #[#{E:tree-mode-border-style},acs]x#[default] Set tagged %1s to default value",
    c"#[fg=themelightgrey]          u #[#{E:tree-mode-border-style},acs]x#[default] Unset an %1",
    c"#[fg=themelightgrey]          U #[#{E:tree-mode-border-style},acs]x#[default] Unset tagged %1s",
    c"#[fg=themelightgrey]          a #[#{E:tree-mode-border-style},acs]x#[default] Change array key",
    c"#[fg=themelightgrey]          e #[#{E:tree-mode-border-style},acs]x#[default] Open %1 value in editor",
    c"#[fg=themelightgrey]          f #[#{E:tree-mode-border-style},acs]x#[default] Enter a filter",
    c"#[fg=themelightgrey]          C #[#{E:tree-mode-border-style},acs]x#[default] Toggle only changed items",
    c"#[fg=themelightgrey]          v #[#{E:tree-mode-border-style},acs]x#[default] Toggle information",
];
fn window_customize_help() -> mode_tree_help_info {
    mode_tree_help_info {
        width: 52 as u_int,
        item: c"item",
        lines: window_customize_help_lines,
    }
}
unsafe fn window_customize_init(
    mut wme: refbox::Weak<window_mode_entry>,
    _item_handle: Option<&std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>>,
    mut fs: *mut cmd_find_state,
    mut args: *mut args,
) -> *mut screen {
    let mode_pane_owner = wme
        .get_unchecked()
        .wp
        .upgrade()
        .expect("mode belongs to a live pane");
    let mut data: *mut window_customize_modedata =
        ::core::ptr::null_mut::<window_customize_modedata>();
    let mut s: *mut screen = ::core::ptr::null_mut::<screen>();
    let format = if args.is_null() || args_has(args, 'F' as i32 as u_char) == 0 {
        WINDOW_CUSTOMIZE_DEFAULT_FORMAT.to_owned()
    } else {
        args_get(&*(args), 'F' as i32 as u_char)
            .expect("argument is present")
            .to_owned()
    };
    let owner = Rc::new(std::cell::UnsafeCell::new(window_customize_modedata {
        wp: std::rc::Rc::downgrade(&mode_pane_owner),
        dead: 0,
        data: None,
        editor: None,
        format,
        hide_global: 0,
        hide_default: 0,
        prompt_flags: 0,
        item_list: Vec::new(),
        fs: (*fs).clone(),
        change: WINDOW_CUSTOMIZE_UNSET,
    }));
    data = crate::src::shared::rc::as_ptr(&owner);
    let build_mode = Rc::downgrade(&owner);
    wme.get_mut_unchecked().data_owner = Some(owner);
    let draw_mode = build_mode.clone();
    let menu_mode = build_mode.clone();
    if args_has(args, 'y' as i32 as u_char) != 0 {
        (*data).prompt_flags = PROMPT_ACCEPT;
    }
    (*data).data = Some(mode_tree_start(
        &mode_pane_owner,
        args,
        Some(Box::new(move |_, tag, filter| {
            let Some(mode) = window_customize_live_mode(&build_mode) else {
                return tag;
            };
            let mut selected = tag.unwrap_or(::core::primitive::u64::MAX as uint64_t);
            window_customize_build(&mode, filter);
            (selected != ::core::primitive::u64::MAX as uint64_t).then_some(selected)
        })),
        Some(Box::new(move |itemdata, ctx, sx, sy| {
            let Some(mode) = window_customize_live_mode(&draw_mode) else {
                return;
            };
            let Some(item_owner) = itemdata.as_customize() else {
                return;
            };
            window_customize_draw(&mode, &item_owner, ctx, sx, sy)
        })),
        None,
        Some(Box::new(move |client, key| {
            let Some(mode) = window_customize_live_mode(&menu_mode) else {
                return;
            };
            window_customize_menu(&mode, client, key)
        })),
        Some(Box::new(move |_| window_customize_height())),
        None,
        None,
        None,
        Some(window_customize_help),
        &window_customize_menu_items,
        &raw mut s,
    ));
    mode_tree_build((*data).data.clone().as_ref().expect("mode tree owner"));
    mode_tree_draw((*data).data.clone().as_ref().expect("mode tree owner"));
    s
}
unsafe fn window_customize_get_screen(wme: refbox::Weak<window_mode_entry>) -> *mut screen {
    let Some(data) = wme
        .get_unchecked()
        .shared_data_ptr::<window_customize_modedata>()
    else {
        return std::ptr::null_mut();
    };
    (*data)
        .data
        .as_ref()
        .map_or(std::ptr::null_mut(), |tree| &raw mut (*tree.get()).screen)
}

unsafe fn window_customize_free(mut wme: refbox::Weak<window_mode_entry>) {
    let Some(mode_owner) = wme
        .get_unchecked()
        .retained_data::<UnsafeCell<window_customize_modedata>>()
    else {
        return;
    };
    let data = mode_owner.get();
    (*data).dead = 1 as ::core::ffi::c_int;
    if let Some(editor) = (*data).editor.as_ref() {
        editor.cancel();
    }
    mode_tree_free((*data).data.take().expect("mode tree owner"));
    drop(wme.get_mut_unchecked().data_owner.take());
}
unsafe fn window_customize_resize(
    mut wme: refbox::Weak<window_mode_entry>,
    mut sx: u_int,
    mut sy: u_int,
) {
    let Some(mode_owner) = wme
        .get_unchecked()
        .retained_data::<UnsafeCell<window_customize_modedata>>()
    else {
        return;
    };
    let data = mode_owner.get();
    if (*data).dead != 0 {
        return;
    }
    mode_tree_resize(
        (*data).data.clone().as_ref().expect("mode tree owner"),
        sx,
        sy,
    );
}
unsafe fn window_customize_update(mut wme: refbox::Weak<window_mode_entry>) {
    let Some(mode_owner) = wme
        .get_unchecked()
        .retained_data::<UnsafeCell<window_customize_modedata>>()
    else {
        return;
    };
    let data = mode_owner.get();
    if (*data).dead != 0 {
        return;
    }
    window_customize_draw_waiting(&mode_owner);
}

// Fields drop in declaration order: the detached item before the retained mode.
struct CustomizePromptItem {
    item: Box<window_customize_itemdata>,
    mode: Rc<UnsafeCell<window_customize_modedata>>,
}

fn window_customize_prompt_callbacks<T: 'static>(
    owner: RefBox<T>,
    callback: unsafe fn(Option<&ClientRef>, &T, Option<&CStr>, prompt_key_result) -> prompt_result,
) -> (mode_tree_prompt_input_cb, prompt_free_cb) {
    let weak = RefBox::downgrade(&owner);
    let inputcb: mode_tree_prompt_input_cb = Some(Box::new(move |client, text, key| {
        let Ok(owner) = weak.try_borrow_mut() else {
            return PROMPT_CLOSE;
        };
        // Keep the callback's borrowed input alive if it closes its own prompt.
        unsafe { callback(client, &owner, text, key) }
    }));
    // mode_tree invokes cleanup before releasing its own retained tree. Cached
    // input callbacks only keep a weak reference and cannot extend this lifetime.
    let freecb: prompt_free_cb = Some(Box::new(move || drop(owner)));
    (inputcb, freecb)
}

/// Prompt input handler that receives the live customize mode owner.
type window_customize_mode_prompt_fn = unsafe fn(
    Option<&ClientRef>,
    &Rc<UnsafeCell<window_customize_modedata>>,
    Option<&CStr>,
    prompt_key_result,
) -> prompt_result;

fn window_customize_mode_prompt_callbacks(
    owner: Rc<UnsafeCell<window_customize_modedata>>,
    callback: window_customize_mode_prompt_fn,
) -> (mode_tree_prompt_input_cb, prompt_free_cb) {
    let weak = Rc::downgrade(&owner);
    let inputcb: mode_tree_prompt_input_cb = Some(Box::new(move |client, text, key| {
        let Some(owner) = (unsafe { window_customize_live_mode(&weak) }) else {
            return PROMPT_CLOSE;
        };
        // Keep the callback's borrowed input alive if it closes its own prompt.
        unsafe { callback(client, &owner, text, key) }
    }));
    // mode_tree invokes cleanup before releasing its own retained tree. Cached
    // input callbacks only keep a weak reference and cannot extend this lifetime.
    let freecb: prompt_free_cb = Some(Box::new(move || drop(owner)));
    (inputcb, freecb)
}

unsafe fn window_customize_set_option_callback(
    c: Option<&ClientRef>,
    owner: &CustomizePromptItem,
    value: Option<&CStr>,
    _key: prompt_key_result,
) -> prompt_result {
    let item = &*owner.item;
    let data = owner.mode.get();
    let Some(mode_pane_owner) = Rc::<UnsafeCell<window_pane>>::from_observer(&(*data).wp) else {
        return PROMPT_CLOSE;
    };
    let Some(value) = value.filter(|value| !value.is_empty()) else {
        return PROMPT_CLOSE;
    };
    if (*data).dead != 0 || window_customize_check_item(&*data, item, None) == 0 {
        return PROMPT_CLOSE;
    }
    let mut cause = None;
    if window_customize_set_option_value(item, Some(value), &mut cause) != 0 {
        window_customize_uppercase_cause(&mut cause);
        if let Some(cause_value) = cause.as_ref() {
            status_message_set(c, -1, 1, 0, 0, |out| {
                write_cstr(out, cause_value.as_c_str())
            });
        }
        return PROMPT_CLOSE;
    }
    mode_tree_build((*data).data.clone().as_ref().expect("mode tree owner"));
    mode_tree_draw((*data).data.clone().as_ref().expect("mode tree owner"));
    mode_pane_owner.request_redraw(false);
    PROMPT_CLOSE
}

unsafe fn window_customize_set_environment_callback(
    _c: Option<&ClientRef>,
    owner: &CustomizePromptItem,
    s: Option<&CStr>,
    _key: prompt_key_result,
) -> prompt_result {
    let mut s = s.map_or(::core::ptr::null(), CStr::as_ptr);
    let item = &*owner.item;
    let Some(mut environment) = item
        .environ
        .as_ref()
        .and_then(CustomizeEnvironment::resolve)
    else {
        return PROMPT_CLOSE;
    };
    if environment.read(|_| ()).is_none() {
        return PROMPT_CLOSE;
    }
    let data = crate::src::shared::rc::as_ptr(&owner.mode);
    let Some(mode_pane_owner) = Rc::<UnsafeCell<window_pane>>::from_observer(&(*data).wp) else {
        return PROMPT_CLOSE;
    };
    if s.is_null() || (*data).dead != 0 {
        return PROMPT_CLOSE;
    }
    if window_customize_check_item(&*data, item, None) == 0 {
        return PROMPT_CLOSE;
    }
    environment.edit(|env| {
        let name = item.name.as_ref().expect("environment name").as_ptr();
        let flags =
            environ_find(env, CStr::from_ptr(name)).map_or(item.environ_flags, |entry| entry.flags);
        environ_set(env, CStr::from_ptr(name), flags, |out| {
            write_cstr(out, nullable_cstr(s))
        });
    });
    mode_tree_build((*data).data.clone().as_ref().expect("mode tree owner"));
    mode_tree_draw((*data).data.clone().as_ref().expect("mode tree owner"));
    mode_pane_owner.request_redraw(false);
    PROMPT_CLOSE
}
unsafe fn window_customize_set_environment(
    client_owner: Option<&ClientRef>,
    mode_owner: &Rc<UnsafeCell<window_customize_modedata>>,
    item: &window_customize_itemdata,
    mut global: ::core::ffi::c_int,
) {
    let data = mode_owner.get();
    let Some(mut environment) = item
        .environ
        .as_ref()
        .and_then(CustomizeEnvironment::resolve)
    else {
        return;
    };
    let Some(env) = environment.get() else {
        return;
    };

    let mut envent: Option<&environ_entry> = None;
    let target;
    let mut scope: window_customize_scope = WINDOW_CUSTOMIZE_NONE;
    let mut fs: cmd_find_state = cmd_find_state {
        flags: 0,
        s: Default::default(),
        wl: Default::default(),
        w: Default::default(),
        wp: Default::default(),
        idx: 0,
    };
    let mut space: *const ::core::ffi::c_char = c"".as_ptr();
    if window_customize_check_item(&*data, item, Some(&mut fs)) == 0 {
        return;
    }
    envent = environ_find(&env, (item.name).as_deref().expect("string argument"));
    if envent.is_none() {
        return;
    }
    if global != 0 {
        scope = WINDOW_CUSTOMIZE_GLOBAL_ENVIRONMENT;
        target = CustomizeEnvironment::Global;
    } else {
        scope = item.scope;
        target = item.environ.clone().expect("environment target");
    }
    let scope_text = window_customize_scope_text(scope, &fs);
    if !scope_text.as_bytes().is_empty() {
        space = c", for ".as_ptr();
    } else if scope as ::core::ffi::c_uint
        == WINDOW_CUSTOMIZE_GLOBAL_ENVIRONMENT as ::core::ffi::c_int as ::core::ffi::c_uint
    {
        space = c", global".as_ptr();
    }
    let mut prompt_bytes = Vec::new();
    prompt_bytes.extend_from_slice(b"(");
    prompt_bytes.extend_from_slice(
        (item.name)
            .as_deref()
            .expect("string is present")
            .to_bytes(),
    );
    prompt_bytes.extend_from_slice(CStr::from_ptr(space).to_bytes());
    prompt_bytes.extend_from_slice(scope_text.as_bytes());
    prompt_bytes.extend_from_slice(b") ");
    let prompt = CString::new(prompt_bytes).expect("environment prompt contains no NUL");
    drop(scope_text);
    let mut new_item = window_customize_new_item();

    new_item.type_0 = WINDOW_CUSTOMIZE_ITEM_ENVIRONMENT;
    new_item.scope = scope;
    new_item.environ = Some(target);
    new_item.environ_flags = envent.unwrap().flags;
    window_customize_set_name(&mut new_item, item.name.as_deref());
    let value = envent.unwrap().value.clone().unwrap_or_default();
    let owner = RefBox::new(CustomizePromptItem {
        item: new_item,
        mode: mode_owner.clone(),
    });
    let (inputcb, freecb) =
        window_customize_prompt_callbacks(owner, window_customize_set_environment_callback);
    mode_tree_set_prompt(
        (*data).data.as_ref().expect("mode tree owner").clone(),
        client_owner,
        &prompt,
        Some(&value),
        PROMPT_TYPE_COMMAND,
        PROMPT_NOFORMAT,
        inputcb,
        freecb,
    );
}
unsafe fn window_customize_add_option_callback(
    c: Option<&ClientRef>,
    owner: &CustomizePromptItem,
    s: Option<&CStr>,
    _key: prompt_key_result,
) -> prompt_result {
    let mut s = s.map_or(::core::ptr::null(), CStr::as_ptr);
    let item = &*owner.item;
    let data = crate::src::shared::rc::as_ptr(&owner.mode);
    let Some(mode_pane_owner) = Rc::<UnsafeCell<window_pane>>::from_observer(&(*data).wp) else {
        return PROMPT_CLOSE;
    };
    let mut value: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    let mut what: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    let mut namelen: size_t = 0;
    if s.is_null() || *s as ::core::ffi::c_int == '\0' as i32 || (*data).dead != 0 {
        return PROMPT_CLOSE;
    }
    if window_customize_check_item(&*data, item, None) == 0 {
        return PROMPT_CLOSE;
    }
    namelen = strcspn(s, c" \t".as_ptr()) as size_t;
    if namelen == 0 as size_t || *s.add(namelen) as ::core::ffi::c_int == '\0' as i32 {
        status_message_set(
            c,
            -(1 as ::core::ffi::c_int),
            1 as ::core::ffi::c_int,
            0 as ::core::ffi::c_int,
            0 as ::core::ffi::c_int,
            |out| out.write_all(b"User option must be @name value"),
        );
        return PROMPT_CLOSE;
    }
    value = s.add(namelen);
    while *value as ::core::ffi::c_int == ' ' as i32 || *value as ::core::ffi::c_int == '\t' as i32
    {
        value = value.offset(1);
    }
    if *value as ::core::ffi::c_int == '\0' as i32 {
        status_message_set(
            c,
            -(1 as ::core::ffi::c_int),
            1 as ::core::ffi::c_int,
            0 as ::core::ffi::c_int,
            0 as ::core::ffi::c_int,
            |out| out.write_all(b"User option must be @name value"),
        );
        return PROMPT_CLOSE;
    }
    let copy = std::ffi::CString::new(std::slice::from_raw_parts(s.cast::<u8>(), namelen))
        .expect("strcspn stops at the first NUL");
    let matched = options_match_owned(copy.as_c_str());
    if !matches!(&matched, Ok(parsed) if parsed.name.as_bytes().first() == Some(&b'@') && parsed.array_key.is_none())
    {
        what = if item.option_type as ::core::ffi::c_uint
            == WINDOW_CUSTOMIZE_HOOKS as ::core::ffi::c_int as ::core::ffi::c_uint
        {
            c"hook".as_ptr()
        } else {
            c"option".as_ptr()
        };
        status_message_set(
            c,
            -(1 as ::core::ffi::c_int),
            1 as ::core::ffi::c_int,
            0 as ::core::ffi::c_int,
            0 as ::core::ffi::c_int,
            |out| {
                out.write_all(b"User ")?;
                write_cstr(out, nullable_cstr(what))?;
                out.write_all(b" name must start with @")
            },
        );
        return PROMPT_CLOSE;
    }
    let name_owned = matched.expect("valid user option was checked").name;
    let name = name_owned.as_ptr();
    item.oo
        .as_ref()
        .expect("option scope")
        .set_from_string(None, &name_owned, Some(CStr::from_ptr(value)), false)
        .expect("valid user string option");
    if item.option_type as ::core::ffi::c_uint
        == WINDOW_CUSTOMIZE_HOOKS as ::core::ffi::c_int as ::core::ffi::c_uint
    {
        hooks_add_event(CStr::from_ptr(name));
    }
    options_push_changes(CStr::from_ptr(name));
    mode_tree_build((*data).data.clone().as_ref().expect("mode tree owner"));
    mode_tree_draw((*data).data.clone().as_ref().expect("mode tree owner"));
    mode_pane_owner.request_redraw(false);
    PROMPT_CLOSE
}
unsafe fn window_customize_add_option(
    client_owner: Option<&ClientRef>,
    mode_owner: &Rc<UnsafeCell<window_customize_modedata>>,
    mut scope: window_customize_scope,
    oo: Option<OptionsScope>,
    mut type_0: window_customize_option_type,
) {
    let data = mode_owner.get();
    let prompt = if type_0 as ::core::ffi::c_uint
        == WINDOW_CUSTOMIZE_HOOKS as ::core::ffi::c_int as ::core::ffi::c_uint
    {
        c"New user hook: "
    } else {
        c"New user option: "
    };
    let mut new_item = window_customize_new_item();

    new_item.type_0 = WINDOW_CUSTOMIZE_ITEM_OPTION;
    new_item.option_type = type_0;
    new_item.scope = scope;
    new_item.oo = oo;
    let owner = RefBox::new(CustomizePromptItem {
        item: new_item,
        mode: mode_owner.clone(),
    });
    let (inputcb, freecb) =
        window_customize_prompt_callbacks(owner, window_customize_add_option_callback);
    mode_tree_set_prompt(
        (*data).data.as_ref().expect("mode tree owner").clone(),
        client_owner,
        prompt,
        Some(c"@"),
        PROMPT_TYPE_COMMAND,
        PROMPT_NOFORMAT,
        inputcb,
        freecb,
    );
}
unsafe fn window_customize_add_environment_callback(
    c: Option<&ClientRef>,
    owner: &CustomizePromptItem,
    s: Option<&CStr>,
    _key: prompt_key_result,
) -> prompt_result {
    let mut s = s.map_or(::core::ptr::null(), CStr::as_ptr);
    let item = &*owner.item;
    let Some(mut environment) = item
        .environ
        .as_ref()
        .and_then(CustomizeEnvironment::resolve)
    else {
        return PROMPT_CLOSE;
    };
    if environment.read(|_| ()).is_none() {
        return PROMPT_CLOSE;
    }
    let data = crate::src::shared::rc::as_ptr(&owner.mode);
    let Some(mode_pane_owner) = Rc::<UnsafeCell<window_pane>>::from_observer(&(*data).wp) else {
        return PROMPT_CLOSE;
    };
    let mut value: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    if s.is_null() || *s as ::core::ffi::c_int == '\0' as i32 || (*data).dead != 0 {
        return PROMPT_CLOSE;
    }
    if window_customize_check_item(&*data, item, None) == 0 {
        return PROMPT_CLOSE;
    }
    if *s as ::core::ffi::c_int == '-' as i32 {
        if *s.offset(1 as ::core::ffi::c_int as isize) as ::core::ffi::c_int == '\0' as i32
            || !strchr(s.offset(1 as ::core::ffi::c_int as isize), '=' as i32).is_null()
        {
            status_message_set(
                c,
                -(1 as ::core::ffi::c_int),
                1 as ::core::ffi::c_int,
                0 as ::core::ffi::c_int,
                0 as ::core::ffi::c_int,
                |out| {
                    out.write_all(b"Bad environment variable: ")?;
                    write_cstr(out, nullable_cstr(s))
                },
            );
            return PROMPT_CLOSE;
        }
        environment.edit(|env| {
            environ_clear(
                env,
                CStr::from_ptr(s.offset(1 as ::core::ffi::c_int as isize)),
            )
        });
    } else {
        value = strchr(s, '=' as i32);
        if value.is_null() || value == s {
            status_message_set(
                c,
                -(1 as ::core::ffi::c_int),
                1 as ::core::ffi::c_int,
                0 as ::core::ffi::c_int,
                0 as ::core::ffi::c_int,
                |out| out.write_all(b"Environment variable must be NAME=value"),
            );
            return PROMPT_CLOSE;
        }
        let name = CString::new(&CStr::from_ptr(s).to_bytes()[..value.offset_from(s) as usize])
            .expect("environment name contains no NUL");
        environment.edit(|env| {
            environ_set(env, &name, 0, |out| {
                write_cstr(
                    out,
                    nullable_cstr(value.offset(1 as ::core::ffi::c_int as isize)),
                )
            })
        });
    }
    mode_tree_build((*data).data.clone().as_ref().expect("mode tree owner"));
    mode_tree_draw((*data).data.clone().as_ref().expect("mode tree owner"));
    mode_pane_owner.request_redraw(false);
    PROMPT_CLOSE
}
unsafe fn window_customize_add_environment(
    client_owner: Option<&ClientRef>,
    mode_owner: &Rc<UnsafeCell<window_customize_modedata>>,
    mut scope: window_customize_scope,
    target: CustomizeEnvironment,
) {
    let data = mode_owner.get();
    let mut new_item = window_customize_new_item();

    new_item.type_0 = WINDOW_CUSTOMIZE_ITEM_ENVIRONMENT;
    new_item.scope = scope;
    new_item.environ = Some(target);
    let owner = RefBox::new(CustomizePromptItem {
        item: new_item,
        mode: mode_owner.clone(),
    });
    let (inputcb, freecb) =
        window_customize_prompt_callbacks(owner, window_customize_add_environment_callback);
    mode_tree_set_prompt(
        (*data).data.as_ref().expect("mode tree owner").clone(),
        client_owner,
        c"New environment: ",
        Some(c""),
        PROMPT_TYPE_COMMAND,
        PROMPT_NOFORMAT,
        inputcb,
        freecb,
    );
}
unsafe fn window_customize_edit_close_cb(
    editor: NonNull<spawn_editor_state>,
    buf: Option<Vec<u8>>,
    ed: window_customize_editdata,
) {
    let mut current_block: u64;
    let item = &*ed.item;
    let mut wme: refbox::Weak<window_mode_entry> = refbox::Weak::new();
    let mut mode_owner = None;
    let mut cause: Option<CString> = None;
    let lookup_wp_owner = Rc::<UnsafeCell<window_pane>>::find_by_id(ed.wp_id);
    if let Some(pane) = lookup_wp_owner.as_ref() {
        wme = pane.mode_entry();
        if !!wme.is_alive() && std::ptr::eq(wme.get_unchecked().mode, &window_customize_mode) {
            mode_owner = wme
                .get_unchecked()
                .retained_data::<UnsafeCell<window_customize_modedata>>();
            if let Some(owner) = mode_owner.as_ref() {
                if (*owner.get())
                    .editor
                    .as_ref()
                    .is_some_and(|handle| handle.matches(editor.as_ref()))
                {
                    (*owner.get()).editor = None;
                }
            }
        }
    }
    let Some(mut value) = buf else {
        return;
    };
    let Some(mode_owner) = mode_owner else {
        return;
    };
    let data = mode_owner.get();
    if value.is_empty() || (*data).dead != 0 {
        return;
    }
    if value.last() == Some(&b'\n') {
        value.pop();
    }
    value.push(0);
    let value_ptr = value.as_ptr().cast::<::core::ffi::c_char>();
    match ed.edit_type as ::core::ffi::c_uint {
        0 => {
            if window_customize_option_editable(&*data, item) != 0
                && window_customize_set_option_value(item, nullable_cstr(value_ptr), &raw mut cause)
                    != 0 as ::core::ffi::c_int
            {
                current_block = 8846462416050848735;
            } else {
                current_block = 1608152415753874203;
            }
        }
        1 => {
            if window_customize_set_command_value(item, CStr::from_ptr(value_ptr), &raw mut cause)
                != 0 as ::core::ffi::c_int
            {
                current_block = 8846462416050848735;
            } else {
                current_block = 1608152415753874203;
            }
        }
        2 => {
            if window_customize_set_note_value(item, CStr::from_ptr(value_ptr))
                != 0 as ::core::ffi::c_int
            {
                current_block = 8846462416050848735;
            } else {
                current_block = 1608152415753874203;
            }
        }
        3 => {
            if window_customize_check_item(&*data, item, None) == 0 {
                current_block = 8846462416050848735;
            } else {
                window_customize_set_environment_value(item, CStr::from_ptr(value_ptr));
                current_block = 1608152415753874203;
            }
        }
        _ => {
            current_block = 1608152415753874203;
        }
    }
    if current_block == 1608152415753874203 {
        mode_tree_build((*data).data.clone().as_ref().expect("mode tree owner"));
        mode_tree_draw((*data).data.clone().as_ref().expect("mode tree owner"));
        lookup_wp_owner
            .as_ref()
            .expect("editor mode pane")
            .request_redraw(false);
    }
    drop(value);
}
unsafe fn window_customize_start_edit(
    mode_owner: &Rc<UnsafeCell<window_customize_modedata>>,
    item: &window_customize_itemdata,
    client_owner: Option<&ClientRef>,
) {
    let data = mode_owner.get();
    let Some(client_owner) = client_owner else {
        return;
    };
    let Some(mode_pane_owner) = Rc::<UnsafeCell<window_pane>>::from_observer(&(*data).wp) else {
        return;
    };
    let mut envent: Option<&environ_entry> = None;
    let value: Cow<'_, CStr>;
    let mut edit_type: window_customize_edit_type = WINDOW_CUSTOMIZE_EDIT_OPTION;
    if (*data).editor.is_some() {
        return;
    }
    if item.type_0 as ::core::ffi::c_uint
        == WINDOW_CUSTOMIZE_ITEM_OPTION as ::core::ffi::c_int as ::core::ffi::c_uint
    {
        if window_customize_option_editable(&*data, item) == 0 {
            return;
        }
        let Some((_owner, option)) = window_customize_find_option(
            item.oo.as_ref().expect("option scope"),
            item.name.as_deref().expect("option name"),
        ) else {
            return;
        };
        let option_value = options_to_cstring(option, item.array_key.as_deref(), 0);
        value = Cow::Owned(option_value);
        edit_type = WINDOW_CUSTOMIZE_EDIT_OPTION;
    } else if item.type_0 as ::core::ffi::c_uint
        == WINDOW_CUSTOMIZE_ITEM_KEY as ::core::ffi::c_int as ::core::ffi::c_uint
    {
        let name = mode_tree_get_current_name(&*(*data).tree_owner().get());
        let Some(table) = window_customize_get_key_table(item) else {
            return;
        };
        let kt = table.borrow();
        let bd = kt.key_bindings.get(item.key).expect("live binding");
        if name.as_ref() == c"Command" {
            value = Cow::Owned(cmd_list_print_cstring(
                &bd.cmdlist().borrow(),
                0 as ::core::ffi::c_int,
            ));
            edit_type = WINDOW_CUSTOMIZE_EDIT_KEY_COMMAND;
        } else if name.as_ref() == c"Note" {
            value = Cow::Owned(bd.note.clone().unwrap_or_default());
            edit_type = WINDOW_CUSTOMIZE_EDIT_KEY_NOTE;
        } else {
            return;
        }
    } else if item.type_0 as ::core::ffi::c_uint
        == WINDOW_CUSTOMIZE_ITEM_ENVIRONMENT as ::core::ffi::c_int as ::core::ffi::c_uint
    {
        if window_customize_check_item(&*data, item, None) == 0 {
            return;
        }
        let Some(environment) = item
            .environ
            .as_ref()
            .and_then(CustomizeEnvironment::resolve)
        else {
            return;
        };
        let Some(env) = environment.get() else {
            return;
        };
        envent = environ_find(&env, (item.name).as_deref().expect("string argument"));
        if envent.is_none() || envent.unwrap().value.is_none() {
            return;
        }
        value = Cow::Owned(envent.unwrap().value.clone().expect("environment value"));
        edit_type = WINDOW_CUSTOMIZE_EDIT_ENVIRONMENT;
    } else {
        return;
    }
    // The callback owns the record and receives editor identity at dispatch.
    // Startup failure and cancellation drop the capture normally.
    let ed = window_customize_editdata {
        wp_id: mode_pane_owner.id(),
        edit_type,
        item: window_customize_copy_item(item),
    };
    let bytes = value.to_bytes();
    let bytes = if bytes.is_empty() { b"\n" } else { bytes };
    let editor = spawn_editor(
        client_owner,
        |stream| spawn_editor_write(stream, bytes),
        Some(Box::new(move |editor, buf| unsafe {
            window_customize_edit_close_cb(editor, buf, ed)
        })),
    );
    (*data).editor = editor;
}
unsafe fn window_customize_set_option(
    client_owner: Option<&ClientRef>,
    mode_owner: &Rc<UnsafeCell<window_customize_modedata>>,
    item: &window_customize_itemdata,
    mut global: ::core::ffi::c_int,
    mut pane: ::core::ffi::c_int,
) {
    let data = mode_owner.get();
    let mut oe: *const options_table_entry = ::core::ptr::null::<options_table_entry>();
    let mut oo: Option<OptionsScope> = None;
    let mut flag: ::core::ffi::c_int = 0;
    let mut scope: window_customize_scope = WINDOW_CUSTOMIZE_NONE;
    let mut choice: u_int = 0;
    let mut name: *const ::core::ffi::c_char = (item.name)
        .as_ref()
        .map_or(::core::ptr::null_mut(), |value| value.as_ptr().cast_mut());
    let mut space: *const ::core::ffi::c_char = c"".as_ptr();
    let mut array_key: *const ::core::ffi::c_char = (item.array_key)
        .as_ref()
        .map_or(::core::ptr::null_mut(), |value| value.as_ptr().cast_mut());
    let mut fs: cmd_find_state = cmd_find_state {
        flags: 0,
        s: Default::default(),
        wl: Default::default(),
        w: Default::default(),
        wp: Default::default(),
        idx: 0,
    };
    if window_customize_check_item(&*data, item, Some(&mut fs)) == 0 {
        return;
    }
    let Some((_owner, option)) = window_customize_find_option(
        item.oo.as_ref().expect("option scope"),
        CStr::from_ptr(name),
    ) else {
        return;
    };
    oe = (*option).tableentry.map_or(std::ptr::null(), |entry| {
        entry as *const options_table_entry
    });
    if !oe.is_null() && !(*oe).scope & OPTIONS_TABLE_PANE != 0 {
        pane = 0 as ::core::ffi::c_int;
    }
    if !oe.is_null() && (*oe).flags & OPTIONS_TABLE_IS_ARRAY != 0 {
        scope = item.scope;
        oo = item.oo.clone();
    } else {
        if global != 0 {
            match item.scope as ::core::ffi::c_uint {
                0 | 1 | 2 | 3 | 5 | 8 | 9 => {
                    scope = item.scope;
                }
                4 => {
                    scope = WINDOW_CUSTOMIZE_GLOBAL_SESSION;
                }
                6 | 7 => {
                    scope = WINDOW_CUSTOMIZE_GLOBAL_WINDOW;
                }
                _ => {}
            }
        } else {
            match item.scope as ::core::ffi::c_uint {
                0 | 1 | 2 | 4 => {
                    scope = item.scope;
                }
                6 | 7 => {
                    if pane != 0 {
                        scope = WINDOW_CUSTOMIZE_PANE;
                    } else {
                        scope = WINDOW_CUSTOMIZE_WINDOW;
                    }
                }
                3 => {
                    scope = WINDOW_CUSTOMIZE_SESSION;
                }
                5 => {
                    if pane != 0 {
                        scope = WINDOW_CUSTOMIZE_PANE;
                    } else {
                        scope = WINDOW_CUSTOMIZE_WINDOW;
                    }
                }
                8 | 9 => {
                    scope = item.scope;
                }
                _ => {}
            }
        }
        if scope as ::core::ffi::c_uint == item.scope as ::core::ffi::c_uint {
            oo = item.oo.clone();
        } else {
            oo = window_customize_get_tree(scope, &fs);
        }
    }
    let oo = oo.expect("option target scope");
    if !oe.is_null()
        && (*oe).type_0 as ::core::ffi::c_uint
            == OPTIONS_TABLE_FLAG as ::core::ffi::c_int as ::core::ffi::c_uint
    {
        flag = oo.with_local(|table| options_get_number_ref(table, CStr::from_ptr(name)))
            as ::core::ffi::c_int;
        oo.with_local(|table| {
            options_set_number(table, CStr::from_ptr(name), (flag == 0) as i64);
        });
    } else if !oe.is_null()
        && (*oe).type_0 as ::core::ffi::c_uint
            == OPTIONS_TABLE_CHOICE as ::core::ffi::c_int as ::core::ffi::c_uint
    {
        choice =
            oo.with_local(|table| options_get_number_ref(table, CStr::from_ptr(name))) as u_int;
        let choices = &(*oe).choices;
        if choice as usize + 1 >= choices.len() {
            choice = 0 as u_int;
        } else {
            choice = choice.wrapping_add(1);
        }
        oo.with_local(|table| {
            options_set_number(table, CStr::from_ptr(name), choice as i64);
        });
    } else {
        let scope_text = window_customize_scope_text(scope, &fs);
        if !scope_text.as_bytes().is_empty() {
            space = c", for ".as_ptr();
        } else if scope as ::core::ffi::c_uint
            != WINDOW_CUSTOMIZE_SERVER as ::core::ffi::c_int as ::core::ffi::c_uint
        {
            space = c", global".as_ptr();
        }
        let mut prompt_bytes = Vec::new();
        prompt_bytes.extend_from_slice(b"(");
        prompt_bytes.extend_from_slice(CStr::from_ptr(name).to_bytes());
        if !oe.is_null() && (*oe).flags & OPTIONS_TABLE_IS_ARRAY != 0 {
            if array_key.is_null() {
                prompt_bytes.extend_from_slice(b"[+]");
            } else {
                prompt_bytes.extend_from_slice(b"[");
                prompt_bytes.extend_from_slice(CStr::from_ptr(array_key).to_bytes());
                prompt_bytes.extend_from_slice(b"]");
            }
        }
        prompt_bytes.extend_from_slice(CStr::from_ptr(space).to_bytes());
        prompt_bytes.extend_from_slice(scope_text.as_bytes());
        prompt_bytes.extend_from_slice(b") ");
        let prompt = CString::new(prompt_bytes).expect("option prompt contains no NUL");
        drop(scope_text);
        let value = options_to_cstring(option, item.array_key.as_deref(), 0);
        let mut new_item = window_customize_new_item();

        new_item.type_0 = WINDOW_CUSTOMIZE_ITEM_OPTION;
        new_item.option_type = item.option_type;
        new_item.scope = scope;
        new_item.oo = Some(oo);
        window_customize_set_name(&mut new_item, Some(CStr::from_ptr(name)));
        if !array_key.is_null() {
            window_customize_set_item_array_key(&mut new_item, Some(CStr::from_ptr(array_key)));
        }
        let owner = RefBox::new(CustomizePromptItem {
            item: new_item,
            mode: mode_owner.clone(),
        });
        let (inputcb, freecb) =
            window_customize_prompt_callbacks(owner, window_customize_set_option_callback);
        mode_tree_set_prompt(
            (*data).data.as_ref().expect("mode tree owner").clone(),
            client_owner,
            &prompt,
            Some(&value),
            PROMPT_TYPE_COMMAND,
            PROMPT_NOFORMAT,
            inputcb,
            freecb,
        );
    };
}
unsafe fn window_customize_set_array_key_callback(
    c: Option<&ClientRef>,
    owner: &CustomizePromptItem,
    value: Option<&CStr>,
    _key: prompt_key_result,
) -> prompt_result {
    let item = &*owner.item;
    let data = owner.mode.get();
    let Some(mode_pane_owner) = Rc::<UnsafeCell<window_pane>>::from_observer(&(*data).wp) else {
        return PROMPT_CLOSE;
    };
    let Some(new_key) = value.filter(|value| !value.is_empty()) else {
        return PROMPT_CLOSE;
    };
    if (*data).dead != 0 || window_customize_check_item(&*data, item, None) == 0 {
        return PROMPT_CLOSE;
    }
    let Some(old_key) = item.array_key.as_deref() else {
        return PROMPT_CLOSE;
    };
    let name = item.name.as_deref().expect("option name");
    let Some((owner, option)) =
        window_customize_find_option(item.oo.as_ref().expect("option scope"), name)
    else {
        return PROMPT_CLOSE;
    };
    if owner.with_entry(name, |entry| options_array_get(entry, new_key).is_some()) != Some(false) {
        return PROMPT_CLOSE;
    }
    let value = options_to_cstring(option, Some(old_key), 0);
    if let Err(error) = owner.set_array_item(name, new_key, Some(&value), false) {
        let mut cause = Some(error);
        window_customize_uppercase_cause(&mut cause);
        status_message_set(c, -1, 1, 0, 0, |out| {
            write_cstr(out, (cause.as_ref().unwrap()).as_c_str())
        });
        return PROMPT_CLOSE;
    }
    let _ = owner.set_array_item(name, old_key, None, false);
    options_push_changes(name);
    mode_tree_build((*data).data.clone().as_ref().expect("mode tree owner"));
    mode_tree_draw((*data).data.clone().as_ref().expect("mode tree owner"));
    mode_pane_owner.request_redraw(false);
    PROMPT_CLOSE
}

unsafe fn window_customize_set_array_key(
    client_owner: Option<&ClientRef>,
    mode_owner: &Rc<UnsafeCell<window_customize_modedata>>,
    item: &window_customize_itemdata,
) {
    let data = mode_owner.get();
    if item.array_key.is_none() || window_customize_check_item(&*data, item, None) == 0 {
        return;
    }
    let mut prompt_bytes = Vec::new();
    prompt_bytes.extend_from_slice(b"(");
    prompt_bytes.extend_from_slice(
        (item.name)
            .as_deref()
            .expect("string is present")
            .to_bytes(),
    );
    prompt_bytes.extend_from_slice(b"[");
    prompt_bytes.extend_from_slice(
        (item.array_key)
            .as_deref()
            .expect("string is present")
            .to_bytes(),
    );
    prompt_bytes.extend_from_slice(b"]) ");
    let prompt = CString::new(prompt_bytes).expect("array-key prompt contains no NUL");
    let mut new_item = window_customize_new_item();

    new_item.type_0 = WINDOW_CUSTOMIZE_ITEM_OPTION;
    new_item.option_type = item.option_type;
    new_item.scope = item.scope;
    new_item.oo = item.oo.clone();
    window_customize_set_name(&mut new_item, item.name.as_deref());
    window_customize_set_item_array_key(&mut new_item, item.array_key.as_deref());
    let owner = RefBox::new(CustomizePromptItem {
        item: new_item,
        mode: mode_owner.clone(),
    });
    let (inputcb, freecb) =
        window_customize_prompt_callbacks(owner, window_customize_set_array_key_callback);
    mode_tree_set_prompt(
        (*data).data.as_ref().expect("mode tree owner").clone(),
        client_owner,
        &prompt,
        item.array_key.as_deref(),
        PROMPT_TYPE_COMMAND,
        PROMPT_NOFORMAT,
        inputcb,
        freecb,
    );
}
unsafe fn window_customize_unset_environment(
    mode_owner: &Rc<UnsafeCell<window_customize_modedata>>,
    item: &ModeTreeItemSnapshot<window_customize_itemdata>,
) {
    let data = mode_owner.get();
    let Some(mut environment) = item
        .environ
        .as_ref()
        .and_then(CustomizeEnvironment::resolve)
    else {
        return;
    };
    let Some(env) = environment.get() else {
        return;
    };

    if window_customize_check_item(&*data, item, None) == 0 {
        return;
    }
    if environ_find(&env, (item.name).as_deref().expect("string argument")).is_none() {
        return;
    }
    if mode_tree_get_current(&*(*data).tree_owner().get()).is_customize(item) {
        mode_tree_up(&mut *(*data).tree_owner().get(), 0 as ::core::ffi::c_int);
    }
    environment.edit(|env| environ_unset(env, (item.name).as_deref().expect("string argument")));
}
unsafe fn window_customize_unset_option(
    mode_owner: &Rc<UnsafeCell<window_customize_modedata>>,
    item: &ModeTreeItemSnapshot<window_customize_itemdata>,
) {
    let data = mode_owner.get();
    if window_customize_check_item(&*data, item, None) == 0 {
        return;
    }
    let name = item.name.as_deref().expect("option name");
    let Some(owner) = item.oo.as_ref().expect("option scope").resolve(name, false) else {
        return;
    };
    if item.array_key.is_some()
        && mode_tree_get_current(&*(*data).tree_owner().get()).is_customize(item)
    {
        mode_tree_up(&mut *(*data).tree_owner().get(), 0);
    }
    let _ = owner.remove_or_default(name, item.array_key.as_deref());
}

unsafe fn window_customize_reset_option(
    mode_owner: &Rc<UnsafeCell<window_customize_modedata>>,
    item: &window_customize_itemdata,
) {
    let data = mode_owner.get();
    if window_customize_check_item(&*data, item, None) == 0 || item.array_key.is_some() {
        return;
    }
    let name = item.name.as_deref().expect("option name");
    let mut scope = item.oo.clone();
    while let Some(owner) = scope {
        let _ = owner.remove_or_default(name, None);
        scope = owner.parent();
    }
}

unsafe fn window_customize_set_command_callback(
    c: Option<&ClientRef>,
    owner: &CustomizePromptItem,
    s: Option<&CStr>,
    _key: prompt_key_result,
) -> prompt_result {
    let mut s = s.map_or(::core::ptr::null(), CStr::as_ptr);
    let item = &*owner.item;
    let data = crate::src::shared::rc::as_ptr(&owner.mode);
    let Some(mode_pane_owner) = Rc::<UnsafeCell<window_pane>>::from_observer(&(*data).wp) else {
        return PROMPT_CLOSE;
    };
    if s.is_null() || *s as ::core::ffi::c_int == '\0' as i32 || (*data).dead != 0 {
        return PROMPT_CLOSE;
    }
    let Some(table) = window_customize_get_key_table(item) else {
        return PROMPT_CLOSE;
    };
    let mut kt = table.borrow_mut();
    let mut pr = cmd_parse_from_string(
        CStr::from_ptr(s),
        ::core::ptr::null_mut::<cmd_parse_input>(),
    );
    match pr.status as ::core::ffi::c_uint {
        0 => {
            cmd_parse_error_uppercase_first(&mut pr.error);
            status_message_set(
                c,
                -(1 as ::core::ffi::c_int),
                1 as ::core::ffi::c_int,
                0 as ::core::ffi::c_int,
                0 as ::core::ffi::c_int,
                |out| write_cstr(out, pr.error.as_deref()),
            );
            PROMPT_CLOSE
        }
        _ => {
            let Some(bd) = kt.key_bindings.get_mut(item.key) else {
                drop(pr.cmdlist.take());
                return PROMPT_CLOSE;
            };
            bd.commands = pr.cmdlist.take().expect("successful command parse");
            drop(kt);
            mode_tree_build((*data).data.clone().as_ref().expect("mode tree owner"));
            mode_tree_draw((*data).data.clone().as_ref().expect("mode tree owner"));
            mode_pane_owner.request_redraw(false);
            PROMPT_CLOSE
        }
    }
}
unsafe fn window_customize_set_note_callback(
    _c: Option<&ClientRef>,
    owner: &CustomizePromptItem,
    s: Option<&CStr>,
    _key: prompt_key_result,
) -> prompt_result {
    let mut s = s.map_or(::core::ptr::null(), CStr::as_ptr);
    let item = &*owner.item;
    let data = crate::src::shared::rc::as_ptr(&owner.mode);
    let Some(mode_pane_owner) = Rc::<UnsafeCell<window_pane>>::from_observer(&(*data).wp) else {
        return PROMPT_CLOSE;
    };
    if s.is_null() || *s as ::core::ffi::c_int == '\0' as i32 || (*data).dead != 0 {
        return PROMPT_CLOSE;
    }
    let Some(table) = window_customize_get_key_table(item) else {
        return PROMPT_CLOSE;
    };
    let mut kt = table.borrow_mut();
    let bd = kt.key_bindings.get_mut(item.key).expect("live binding");
    key_bindings_set_note(bd, Some(CStr::from_ptr(s)));
    drop(kt);
    mode_tree_build((*data).data.clone().as_ref().expect("mode tree owner"));
    mode_tree_draw((*data).data.clone().as_ref().expect("mode tree owner"));
    mode_pane_owner.request_redraw(false);
    PROMPT_CLOSE
}
fn window_customize_key_prompt(key_string: &CStr) -> CString {
    let mut prompt = Vec::with_capacity(key_string.to_bytes().len() + 3);
    prompt.extend_from_slice(b"(");
    prompt.extend_from_slice(key_string.to_bytes());
    prompt.extend_from_slice(b") ");
    CString::new(prompt).expect("formatted key contains no NUL")
}

unsafe fn window_customize_set_key(
    client_owner: Option<&ClientRef>,
    mode_owner: &Rc<UnsafeCell<window_customize_modedata>>,
    item: &window_customize_itemdata,
) {
    let data = mode_owner.get();
    let mut key: key_code = item.key;
    let Some(table) = window_customize_get_key_table(item) else {
        return;
    };
    let mut kt = table.borrow_mut();
    let bd = kt.key_bindings.get_mut(item.key).expect("live binding");
    let s = mode_tree_get_current_name(&*(*data).tree_owner().get());
    if s.as_ref() == c"Repeat" {
        bd.flags ^= KEY_BINDING_REPEAT;
    } else if s.as_ref() == c"Command" {
        let key_string = key_string_format(key, false);
        let prompt = window_customize_key_prompt(&key_string);
        let value = cmd_list_print_cstring(&bd.cmdlist().borrow(), 0 as ::core::ffi::c_int);
        let mut new_item = window_customize_new_item();

        new_item.type_0 = WINDOW_CUSTOMIZE_ITEM_KEY;
        new_item.scope = item.scope;
        window_customize_set_table(&mut new_item, item.table.as_deref());
        new_item.key = key;
        let owner = RefBox::new(CustomizePromptItem {
            item: new_item,
            mode: mode_owner.clone(),
        });
        let (inputcb, freecb) =
            window_customize_prompt_callbacks(owner, window_customize_set_command_callback);
        drop(kt);
        mode_tree_set_prompt(
            (*data).data.as_ref().expect("mode tree owner").clone(),
            client_owner,
            &prompt,
            Some(&value),
            PROMPT_TYPE_COMMAND,
            PROMPT_NOFORMAT,
            inputcb,
            freecb,
        );
    } else if s.as_ref() == c"Note" {
        let note = bd.note.clone().unwrap_or_default();
        let key_string = key_string_format(key, false);
        let prompt = window_customize_key_prompt(&key_string);
        let mut new_item = window_customize_new_item();

        new_item.type_0 = WINDOW_CUSTOMIZE_ITEM_KEY;
        new_item.scope = item.scope;
        window_customize_set_table(&mut new_item, item.table.as_deref());
        new_item.key = key;
        let owner = RefBox::new(CustomizePromptItem {
            item: new_item,
            mode: mode_owner.clone(),
        });
        let (inputcb, freecb) =
            window_customize_prompt_callbacks(owner, window_customize_set_note_callback);
        drop(kt);
        mode_tree_set_prompt(
            (*data).data.as_ref().expect("mode tree owner").clone(),
            client_owner,
            &prompt,
            Some(&note),
            PROMPT_TYPE_COMMAND,
            PROMPT_NOFORMAT,
            inputcb,
            freecb,
        );
    }
}
unsafe fn window_customize_add_key_callback(
    c: Option<&ClientRef>,
    owner: &CustomizePromptItem,
    s: Option<&CStr>,
    _key0: prompt_key_result,
) -> prompt_result {
    let mut s = s.map_or(::core::ptr::null(), CStr::as_ptr);
    let item = &*owner.item;
    let data = crate::src::shared::rc::as_ptr(&owner.mode);
    let Some(mode_pane_owner) = Rc::<UnsafeCell<window_pane>>::from_observer(&(*data).wp) else {
        return PROMPT_CLOSE;
    };
    let mut key: key_code = 0;
    let mut command: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    let mut keylen: size_t = 0;
    if s.is_null() || *s as ::core::ffi::c_int == '\0' as i32 || (*data).dead != 0 {
        return PROMPT_CLOSE;
    }
    keylen = strcspn(s, c" \t".as_ptr()) as size_t;
    if keylen == 0 as size_t || *s.add(keylen) as ::core::ffi::c_int == '\0' as i32 {
        status_message_set(
            c,
            -(1 as ::core::ffi::c_int),
            1 as ::core::ffi::c_int,
            0 as ::core::ffi::c_int,
            0 as ::core::ffi::c_int,
            |out| out.write_all(b"Key binding must be key command"),
        );
        return PROMPT_CLOSE;
    }
    command = s.add(keylen);
    while *command as ::core::ffi::c_int == ' ' as i32
        || *command as ::core::ffi::c_int == '\t' as i32
    {
        command = command.offset(1);
    }
    if *command as ::core::ffi::c_int == '\0' as i32 {
        status_message_set(
            c,
            -(1 as ::core::ffi::c_int),
            1 as ::core::ffi::c_int,
            0 as ::core::ffi::c_int,
            0 as ::core::ffi::c_int,
            |out| out.write_all(b"Key binding must be key command"),
        );
        return PROMPT_CLOSE;
    }
    // strcspn stops before the first NUL, so this prefix is a valid C string.
    let keystr = std::ffi::CString::new(std::slice::from_raw_parts(s.cast::<u8>(), keylen))
        .expect("key name contains no NUL");
    key = key_string_parse_cstr(&keystr).unwrap_or(KEYC_UNKNOWN);
    if key == KEYC_NONE as ::core::ffi::c_ulong as key_code
        || key == KEYC_UNKNOWN as ::core::ffi::c_ulong as key_code
    {
        status_message_set(
            c,
            -(1 as ::core::ffi::c_int),
            1 as ::core::ffi::c_int,
            0 as ::core::ffi::c_int,
            0 as ::core::ffi::c_int,
            |out| {
                out.write_all(b"Unknown key: ")?;
                write_cstr(out, &*keystr)
            },
        );
        return PROMPT_CLOSE;
    }
    drop(keystr);
    let mut pr = cmd_parse_from_string(
        CStr::from_ptr(command),
        ::core::ptr::null_mut::<cmd_parse_input>(),
    );
    match pr.status as ::core::ffi::c_uint {
        0 => {
            cmd_parse_error_uppercase_first(&mut pr.error);
            status_message_set(
                c,
                -(1 as ::core::ffi::c_int),
                1 as ::core::ffi::c_int,
                0 as ::core::ffi::c_int,
                0 as ::core::ffi::c_int,
                |out| write_cstr(out, pr.error.as_deref()),
            );
            PROMPT_CLOSE
        }
        _ => {
            key_bindings_add(
                item.table.as_deref().expect("key binding table name"),
                key,
                {
                    let note: *const ::core::ffi::c_char =
                        ::core::ptr::null::<::core::ffi::c_char>();
                    (!note.is_null()).then(|| std::ffi::CStr::from_ptr(note))
                },
                0 as ::core::ffi::c_int,
                pr.take_cmdlist(),
            );
            mode_tree_build((*data).data.clone().as_ref().expect("mode tree owner"));
            mode_tree_draw((*data).data.clone().as_ref().expect("mode tree owner"));
            mode_pane_owner.request_redraw(false);
            PROMPT_CLOSE
        }
    }
}
unsafe fn window_customize_add_key(
    client_owner: Option<&ClientRef>,
    mode_owner: &Rc<UnsafeCell<window_customize_modedata>>,
    table: &CStr,
) {
    let data = mode_owner.get();
    let mut prompt_bytes = b"New key in ".to_vec();
    prompt_bytes.extend_from_slice(table.to_bytes());
    prompt_bytes.extend_from_slice(b": ");
    let prompt = CString::new(prompt_bytes).expect("key table name contains no NUL");
    let mut new_item = window_customize_new_item();

    new_item.type_0 = WINDOW_CUSTOMIZE_ITEM_KEY;
    new_item.scope = WINDOW_CUSTOMIZE_KEY;
    window_customize_set_table(&mut new_item, Some(table));
    let owner = RefBox::new(CustomizePromptItem {
        item: new_item,
        mode: mode_owner.clone(),
    });
    let (inputcb, freecb) =
        window_customize_prompt_callbacks(owner, window_customize_add_key_callback);
    mode_tree_set_prompt(
        (*data).data.as_ref().expect("mode tree owner").clone(),
        client_owner,
        &prompt,
        Some(c""),
        PROMPT_TYPE_COMMAND,
        PROMPT_NOFORMAT,
        inputcb,
        freecb,
    );
}
unsafe fn window_customize_unset_key(
    mode_owner: &Rc<UnsafeCell<window_customize_modedata>>,
    item: &ModeTreeItemSnapshot<window_customize_itemdata>,
) {
    let data = mode_owner.get();
    let Some(table) = window_customize_get_key_table(item) else {
        return;
    };
    let name = table.borrow().name.clone();
    if mode_tree_get_current(&*(*data).tree_owner().get()).is_customize(item) {
        mode_tree_up(&mut *(*data).tree_owner().get(), 0);
    }
    key_bindings_remove(&name, item.key);
}
unsafe fn window_customize_reset_key(
    mode_owner: &Rc<UnsafeCell<window_customize_modedata>>,
    item: &ModeTreeItemSnapshot<window_customize_itemdata>,
) {
    let data = mode_owner.get();
    let Some(table) = window_customize_get_key_table(item) else {
        return;
    };
    let table_ref = table.borrow();
    let bd = key_bindings_get(&table_ref, item.key).expect("live binding");
    let default = key_bindings_get_default(&table_ref, item.key);
    if default.is_some_and(|default| Rc::ptr_eq(&bd.commands, &default.commands)) {
        return;
    }
    let has_default = default.is_some();
    let name = table_ref.name.clone();
    drop(table_ref);
    if !has_default && mode_tree_get_current(&*(*data).tree_owner().get()).is_customize(item) {
        mode_tree_up(&mut *(*data).tree_owner().get(), 0);
    }
    key_bindings_reset(&name, item.key);
}
unsafe fn window_customize_change_each(
    mode_owner: &Rc<UnsafeCell<window_customize_modedata>>,
    item: &ModeTreeItemSnapshot<window_customize_itemdata>,
) {
    let data = mode_owner.get();
    let mut type_0: window_customize_item_type = item.type_0;
    let name = if type_0 as ::core::ffi::c_uint
        == WINDOW_CUSTOMIZE_ITEM_OPTION as ::core::ffi::c_int as ::core::ffi::c_uint
    {
        Some(
            (item.name)
                .as_deref()
                .expect("string is present")
                .to_owned(),
        )
    } else {
        None
    };
    match (*data).change as ::core::ffi::c_uint {
        0 => {
            if type_0 as ::core::ffi::c_uint
                == WINDOW_CUSTOMIZE_ITEM_KEY as ::core::ffi::c_int as ::core::ffi::c_uint
            {
                window_customize_unset_key(mode_owner, item);
            } else if type_0 as ::core::ffi::c_uint
                == WINDOW_CUSTOMIZE_ITEM_ENVIRONMENT as ::core::ffi::c_int as ::core::ffi::c_uint
            {
                window_customize_unset_environment(mode_owner, item);
            } else {
                window_customize_unset_option(mode_owner, item);
            }
        }
        1 => {
            if type_0 as ::core::ffi::c_uint
                == WINDOW_CUSTOMIZE_ITEM_KEY as ::core::ffi::c_int as ::core::ffi::c_uint
            {
                window_customize_reset_key(mode_owner, item);
            } else if type_0 as ::core::ffi::c_uint
                == WINDOW_CUSTOMIZE_ITEM_OPTION as ::core::ffi::c_int as ::core::ffi::c_uint
            {
                window_customize_reset_option(mode_owner, item);
            }
        }
        _ => {}
    }
    if type_0 as ::core::ffi::c_uint
        == WINDOW_CUSTOMIZE_ITEM_OPTION as ::core::ffi::c_int as ::core::ffi::c_uint
    {
        options_push_changes(name.as_deref().expect("option name was copied"));
    }
}
unsafe fn window_customize_change_current_callback(
    _c: Option<&ClientRef>,
    owner: &Rc<UnsafeCell<window_customize_modedata>>,
    s: Option<&CStr>,
    _key: prompt_key_result,
) -> prompt_result {
    let data = owner.get();
    let Some(mode_pane_owner) = Rc::<UnsafeCell<window_pane>>::from_observer(&(*data).wp) else {
        return PROMPT_CLOSE;
    };
    let mut s = s.map_or(::core::ptr::null(), CStr::as_ptr);
    if s.is_null() || *s as ::core::ffi::c_int == '\0' as i32 || (*data).dead != 0 {
        return PROMPT_CLOSE;
    }
    if ({
        let mut __res: ::core::ffi::c_int = 0;
        if ::core::mem::size_of::<u_char>() as usize > 1_usize {
            if 0 != 0 {
                let mut __c: ::core::ffi::c_int =
                    *s.offset(0 as ::core::ffi::c_int as isize) as u_char as ::core::ffi::c_int;
                __res = (if __c < -(128 as ::core::ffi::c_int) || __c > 255 as ::core::ffi::c_int {
                    __c as __int32_t
                } else {
                    *(*__ctype_tolower_loc()).offset(__c as isize)
                }) as ::core::ffi::c_int;
            } else {
                __res = tolower(
                    *s.offset(0 as ::core::ffi::c_int as isize) as u_char as ::core::ffi::c_int
                );
            }
        } else {
            __res = *(*__ctype_tolower_loc()).offset(*s.offset(0 as ::core::ffi::c_int as isize)
                as u_char as ::core::ffi::c_int
                as isize) as ::core::ffi::c_int;
        }
        __res
    }) != 'y' as i32
        || *s.offset(1 as ::core::ffi::c_int as isize) as ::core::ffi::c_int != '\0' as i32
    {
        return PROMPT_CLOSE;
    }
    let item_owner = mode_tree_get_current(&*(*data).tree_owner().get());
    let Some(item) = item_owner.as_customize() else {
        return PROMPT_CLOSE;
    };
    window_customize_change_each(owner, &item);
    mode_tree_build((*data).data.clone().as_ref().expect("mode tree owner"));
    mode_tree_draw((*data).data.clone().as_ref().expect("mode tree owner"));
    mode_pane_owner.request_redraw(false);
    PROMPT_CLOSE
}
unsafe fn window_customize_change_tagged_callback(
    _c: Option<&ClientRef>,
    owner: &Rc<UnsafeCell<window_customize_modedata>>,
    s: Option<&CStr>,
    _key: prompt_key_result,
) -> prompt_result {
    let data = owner.get();
    let Some(mode_pane_owner) = Rc::<UnsafeCell<window_pane>>::from_observer(&(*data).wp) else {
        return PROMPT_CLOSE;
    };
    let mut s = s.map_or(::core::ptr::null(), CStr::as_ptr);
    if s.is_null() || *s as ::core::ffi::c_int == '\0' as i32 || (*data).dead != 0 {
        return PROMPT_CLOSE;
    }
    if ({
        let mut __res: ::core::ffi::c_int = 0;
        if ::core::mem::size_of::<u_char>() as usize > 1_usize {
            if 0 != 0 {
                let mut __c: ::core::ffi::c_int =
                    *s.offset(0 as ::core::ffi::c_int as isize) as u_char as ::core::ffi::c_int;
                __res = (if __c < -(128 as ::core::ffi::c_int) || __c > 255 as ::core::ffi::c_int {
                    __c as __int32_t
                } else {
                    *(*__ctype_tolower_loc()).offset(__c as isize)
                }) as ::core::ffi::c_int;
            } else {
                __res = tolower(
                    *s.offset(0 as ::core::ffi::c_int as isize) as u_char as ::core::ffi::c_int
                );
            }
        } else {
            __res = *(*__ctype_tolower_loc()).offset(*s.offset(0 as ::core::ffi::c_int as isize)
                as u_char as ::core::ffi::c_int
                as isize) as ::core::ffi::c_int;
        }
        __res
    }) != 'y' as i32
        || *s.offset(1 as ::core::ffi::c_int as isize) as ::core::ffi::c_int != '\0' as i32
    {
        return PROMPT_CLOSE;
    }
    mode_tree_each_tagged(
        (*data).data.clone().as_ref().expect("live mode tree"),
        |row, _| unsafe {
            let itemdata = row.borrow().itemdata.clone();
            window_customize_change_each(
                owner,
                &itemdata.as_customize().expect("tagged customize row"),
            )
        },
        KEYC_NONE as ::core::ffi::c_ulong as key_code,
        0 as ::core::ffi::c_int,
    );
    mode_tree_build((*data).data.clone().as_ref().expect("mode tree owner"));
    mode_tree_draw((*data).data.clone().as_ref().expect("mode tree owner"));
    mode_pane_owner.request_redraw(false);
    PROMPT_CLOSE
}
unsafe fn window_customize_add_current(
    client_owner: Option<&ClientRef>,
    mode_owner: &Rc<UnsafeCell<window_customize_modedata>>,
) -> ::core::ffi::c_int {
    let data = mode_owner.get();
    let Some(mode_pane_owner) = Rc::<UnsafeCell<window_pane>>::from_observer(&(*data).wp) else {
        return 1;
    };
    let mut fs: cmd_find_state = cmd_find_state {
        flags: 0,
        s: Default::default(),
        wl: Default::default(),
        w: Default::default(),
        wp: Default::default(),
        idx: 0,
    };
    let name = mode_tree_get_current_name(&*(*data).tree_owner().get());
    if (*data).fs.is_valid() {
        fs.copy_from(&(*data).fs);
    } else {
        fs = cmd_find_state::find_from_pane(&mode_pane_owner, 0 as ::core::ffi::c_int)
            .unwrap_or_else(|| cmd_find_state::clear_state(0 as ::core::ffi::c_int));
    }
    if name.as_ref() == c"Server Options" {
        window_customize_add_option(
            client_owner,
            mode_owner,
            WINDOW_CUSTOMIZE_SERVER,
            Some(OptionsScope::GlobalServer),
            WINDOW_CUSTOMIZE_OPTIONS,
        );
        return 1 as ::core::ffi::c_int;
    }
    if name.as_ref() == c"Session Options" {
        window_customize_add_option(
            client_owner,
            mode_owner,
            WINDOW_CUSTOMIZE_SESSION,
            window_customize_get_tree(WINDOW_CUSTOMIZE_SESSION, &fs),
            WINDOW_CUSTOMIZE_OPTIONS,
        );
        return 1 as ::core::ffi::c_int;
    }
    if name.as_ref() == c"Window & Pane Options" {
        window_customize_add_option(
            client_owner,
            mode_owner,
            WINDOW_CUSTOMIZE_PANE,
            window_customize_get_tree(WINDOW_CUSTOMIZE_PANE, &fs),
            WINDOW_CUSTOMIZE_OPTIONS,
        );
        return 1 as ::core::ffi::c_int;
    }
    if name.as_ref() == c"Session Hooks" {
        window_customize_add_option(
            client_owner,
            mode_owner,
            WINDOW_CUSTOMIZE_SESSION,
            window_customize_get_tree(WINDOW_CUSTOMIZE_SESSION, &fs),
            WINDOW_CUSTOMIZE_HOOKS,
        );
        return 1 as ::core::ffi::c_int;
    }
    if name.as_ref() == c"Window & Pane Hooks" {
        window_customize_add_option(
            client_owner,
            mode_owner,
            WINDOW_CUSTOMIZE_PANE,
            window_customize_get_tree(WINDOW_CUSTOMIZE_PANE, &fs),
            WINDOW_CUSTOMIZE_HOOKS,
        );
        return 1 as ::core::ffi::c_int;
    }
    if name.as_ref() == c"Global Environment" {
        window_customize_add_environment(
            client_owner,
            mode_owner,
            WINDOW_CUSTOMIZE_GLOBAL_ENVIRONMENT,
            CustomizeEnvironment::Global,
        );
        return 1 as ::core::ffi::c_int;
    }
    if name.as_ref() == c"Session Environment" {
        window_customize_add_environment(
            client_owner,
            mode_owner,
            WINDOW_CUSTOMIZE_SESSION_ENVIRONMENT,
            CustomizeEnvironment::session(&fs.session_handle().expect("target session")),
        );
        return 1 as ::core::ffi::c_int;
    }
    if let Some(table) = name.to_bytes_with_nul().strip_prefix(b"Key Table - ") {
        let table = CStr::from_bytes_with_nul(table).expect("key table suffix is NUL terminated");
        window_customize_add_key(client_owner, mode_owner, table);
        return 1;
    }
    0 as ::core::ffi::c_int
}
unsafe fn window_customize_key(
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
    let mode_owner = wme
        .get_unchecked()
        .retained_data::<UnsafeCell<window_customize_modedata>>()
        .expect("live mode payload");
    let data = mode_owner.get();
    let mut finished: ::core::ffi::c_int = 0;
    let mut tagged: u_int = 0;
    if (*data).editor.is_some() {
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
        if (*data).dead != 0 {
            return;
        }
        let item_owner = mode_tree_get_current(&*(*data).tree_owner().get());
        let item = item_owner.as_customize();
        match key {
            101 => {
                if let Some(item) = item {
                    window_customize_start_edit(&mode_owner, &item, Some(client_owner));
                }
            }
            97 => {
                if let Some(item) = item.filter(|item| item.type_0 == WINDOW_CUSTOMIZE_ITEM_OPTION)
                {
                    window_customize_set_array_key(Some(client_owner), &mode_owner, &item);
                }
            }
            13 | 115 => {
                if let Some(item) = item {
                    if item.type_0 as ::core::ffi::c_uint
                        == WINDOW_CUSTOMIZE_ITEM_KEY as ::core::ffi::c_int as ::core::ffi::c_uint
                    {
                        window_customize_set_key(Some(client_owner), &mode_owner, &item);
                    } else if item.type_0 as ::core::ffi::c_uint
                        == WINDOW_CUSTOMIZE_ITEM_ENVIRONMENT as ::core::ffi::c_int
                            as ::core::ffi::c_uint
                    {
                        window_customize_set_environment(
                            Some(client_owner),
                            &mode_owner,
                            &item,
                            0 as ::core::ffi::c_int,
                        );
                    } else {
                        window_customize_set_option(
                            Some(client_owner),
                            &mode_owner,
                            &item,
                            0 as ::core::ffi::c_int,
                            1 as ::core::ffi::c_int,
                        );
                        options_push_changes(item.name.as_deref().expect("option name"));
                    }
                    mode_tree_build((*data).data.clone().as_ref().expect("mode tree owner"));
                } else if window_customize_add_current(Some(client_owner), &mode_owner) != 0 {
                    mode_tree_build((*data).data.clone().as_ref().expect("mode tree owner"));
                }
            }
            119 => {
                if let Some(item) = item.filter(|item| item.type_0 == WINDOW_CUSTOMIZE_ITEM_OPTION)
                {
                    window_customize_set_option(
                        Some(client_owner),
                        &mode_owner,
                        &item,
                        0 as ::core::ffi::c_int,
                        0 as ::core::ffi::c_int,
                    );
                    options_push_changes(item.name.as_deref().expect("option name"));
                    mode_tree_build((*data).data.clone().as_ref().expect("mode tree owner"));
                }
            }
            83 | 87 => {
                if let Some(item) = item.filter(|item| item.type_0 != WINDOW_CUSTOMIZE_ITEM_KEY) {
                    if item.type_0 as ::core::ffi::c_uint
                        == WINDOW_CUSTOMIZE_ITEM_ENVIRONMENT as ::core::ffi::c_int
                            as ::core::ffi::c_uint
                    {
                        window_customize_set_environment(
                            Some(client_owner),
                            &mode_owner,
                            &item,
                            1 as ::core::ffi::c_int,
                        );
                    } else {
                        window_customize_set_option(
                            Some(client_owner),
                            &mode_owner,
                            &item,
                            1 as ::core::ffi::c_int,
                            0 as ::core::ffi::c_int,
                        );
                        options_push_changes(item.name.as_deref().expect("option name"));
                    }
                    mode_tree_build((*data).data.clone().as_ref().expect("mode tree owner"));
                }
            }
            100 => {
                if let Some(item) = item.filter(|item| {
                    !(item.type_0 == WINDOW_CUSTOMIZE_ITEM_OPTION && item.array_key.is_some()
                        || item.type_0 == WINDOW_CUSTOMIZE_ITEM_ENVIRONMENT)
                }) {
                    let mut prompt_bytes = b"Reset ".to_vec();
                    prompt_bytes.extend_from_slice(
                        (item.name)
                            .as_deref()
                            .expect("string is present")
                            .to_bytes(),
                    );
                    prompt_bytes.extend_from_slice(b" to default? ");
                    let reset_prompt =
                        CString::new(prompt_bytes).expect("C string parts have no NUL");
                    let owner = mode_owner.clone();
                    let (inputcb, freecb) = window_customize_mode_prompt_callbacks(
                        owner,
                        window_customize_change_current_callback,
                    );
                    (*data).change = WINDOW_CUSTOMIZE_RESET;
                    mode_tree_set_prompt(
                        (*data).data.as_ref().expect("mode tree owner").clone(),
                        Some(client_owner),
                        &reset_prompt,
                        Some(c""),
                        PROMPT_TYPE_COMMAND,
                        PROMPT_SINGLE | PROMPT_NOFORMAT | (*data).prompt_flags,
                        inputcb,
                        freecb,
                    );
                }
            }
            68 => {
                tagged = mode_tree_count_tagged(&*(*data).tree_owner().get());
                if !(tagged == 0 as u_int) {
                    let reset_prompt = CString::new(format!("Reset {tagged} tagged to default? "))
                        .expect("formatted number has no NUL");
                    let owner = mode_owner.clone();
                    let (inputcb, freecb) = window_customize_mode_prompt_callbacks(
                        owner,
                        window_customize_change_tagged_callback,
                    );
                    (*data).change = WINDOW_CUSTOMIZE_RESET;
                    mode_tree_set_prompt(
                        (*data).data.as_ref().expect("mode tree owner").clone(),
                        Some(client_owner),
                        &reset_prompt,
                        Some(c""),
                        PROMPT_TYPE_COMMAND,
                        PROMPT_SINGLE | PROMPT_NOFORMAT | (*data).prompt_flags,
                        inputcb,
                        freecb,
                    );
                }
            }
            117 => {
                if let Some(item) = item {
                    let mut prompt_bytes = b"Unset ".to_vec();
                    prompt_bytes.extend_from_slice(
                        (item.name)
                            .as_deref()
                            .expect("string is present")
                            .to_bytes(),
                    );
                    if !item.array_key.is_none() {
                        prompt_bytes.push(b'[');
                        prompt_bytes.extend_from_slice(
                            (item.array_key)
                                .as_deref()
                                .expect("string is present")
                                .to_bytes(),
                        );
                        prompt_bytes.push(b']');
                    }
                    prompt_bytes.extend_from_slice(b"? ");
                    let prompt =
                        CString::new(prompt_bytes).expect("C strings have no interior NUL");
                    let owner = mode_owner.clone();
                    let (inputcb, freecb) = window_customize_mode_prompt_callbacks(
                        owner,
                        window_customize_change_current_callback,
                    );
                    (*data).change = WINDOW_CUSTOMIZE_UNSET;
                    mode_tree_set_prompt(
                        (*data).data.as_ref().expect("mode tree owner").clone(),
                        Some(client_owner),
                        &prompt,
                        Some(c""),
                        PROMPT_TYPE_COMMAND,
                        PROMPT_SINGLE | PROMPT_NOFORMAT | (*data).prompt_flags,
                        inputcb,
                        freecb,
                    );
                }
            }
            85 => {
                tagged = mode_tree_count_tagged(&*(*data).tree_owner().get());
                if !(tagged == 0 as u_int) {
                    let prompt = CString::new(format!("Unset {tagged} tagged? ")).unwrap();
                    let owner = mode_owner.clone();
                    let (inputcb, freecb) = window_customize_mode_prompt_callbacks(
                        owner,
                        window_customize_change_tagged_callback,
                    );
                    (*data).change = WINDOW_CUSTOMIZE_UNSET;
                    mode_tree_set_prompt(
                        (*data).data.as_ref().expect("mode tree owner").clone(),
                        Some(client_owner),
                        &prompt,
                        Some(c""),
                        PROMPT_TYPE_COMMAND,
                        PROMPT_SINGLE | PROMPT_NOFORMAT | (*data).prompt_flags,
                        inputcb,
                        freecb,
                    );
                }
            }
            72 => {
                (*data).hide_global = ((*data).hide_global == 0) as ::core::ffi::c_int;
                mode_tree_build((*data).data.clone().as_ref().expect("mode tree owner"));
            }
            67 => {
                (*data).hide_default = ((*data).hide_default == 0) as ::core::ffi::c_int;
                mode_tree_build((*data).data.clone().as_ref().expect("mode tree owner"));
            }
            _ => {}
        }
    }
    if finished != 0 {
        mode_pane_owner.reset_mode();
    } else {
        mode_tree_draw((*data).data.clone().as_ref().expect("mode tree owner"));
        window_customize_draw_waiting(&mode_owner);
        mode_pane_owner.request_redraw(false);
    };
}

#[cfg(test)]
mod tag_tests {
    use super::*;

    #[test]
    fn section_tags_match_tmux_encoding() {
        assert_eq!(
            window_customize_top_tag(CUSTOMIZE_SERVER_OPTIONS),
            (3_u64 << 62) | ((OPTIONS_TABLE_SERVER as u64) << 1) | 1
        );
        assert_eq!(
            window_customize_top_tag(CUSTOMIZE_SESSION_ENVIRONMENT),
            (3_u64 << 62) | (2_u64 << 8) | 3
        );
    }
}

#[cfg(test)]
mod item_owner_tests {
    use super::*;
    use crate::src::spawn::spawn_cancel_editor;

    #[test]
    fn detached_copy_owns_byte_preserving_strings() {
        let mut item = window_customize_new_item();
        let table = CString::new(b"\xfftable".to_vec()).unwrap();
        window_customize_set_table(&mut item, Some(&table));
        window_customize_set_name(&mut item, Some(c"first"));
        window_customize_set_item_array_key(&mut item, Some(c""));
        let copy = window_customize_copy_item(&item);

        window_customize_set_table(&mut item, Some(c"changed"));
        window_customize_set_name(&mut item, Some(c"second"));
        drop(item);

        assert_eq!(copy.table.as_ref().unwrap().as_bytes(), b"\xfftable");
        assert_eq!(copy.name.as_deref(), Some(c"first"));
        assert_eq!(copy.array_key.as_deref(), Some(c""));
    }

    #[test]
    fn mode_list_keeps_items_stable_until_last_callback_reference() {
        unsafe fn read_mode(
            _: Option<&ClientRef>,
            owner: &Rc<UnsafeCell<window_customize_modedata>>,
            _: Option<&CStr>,
            _: prompt_key_result,
        ) -> prompt_result {
            assert_eq!(Rc::strong_count(owner), 3);
            assert_eq!((*owner.get()).dead, 0);
            PROMPT_CONTINUE
        }
        unsafe fn read_item(
            _client: Option<&ClientRef>,
            owner: &CustomizePromptItem,
            _text: Option<&CStr>,
            _key: prompt_key_result,
        ) -> prompt_result {
            assert_eq!(owner.item.name.as_deref(), Some(c"stable"));
            assert_eq!(Rc::strong_count(&owner.mode), 1);
            PROMPT_CONTINUE
        }
        unsafe {
            let owner = Rc::new(std::cell::UnsafeCell::new(window_customize_modedata {
                wp: Weak::new(),
                dead: 0,
                data: None,
                editor: None,
                format: CString::new(Vec::new()).unwrap(),
                hide_global: 0,
                hide_default: 0,
                prompt_flags: 0,
                item_list: Vec::new(),
                fs: cmd_find_state {
                    flags: 0,
                    s: Default::default(),
                    wl: Default::default(),
                    w: Default::default(),
                    wp: Default::default(),
                    idx: 0,
                },
                change: WINDOW_CUSTOMIZE_UNSET,
            }));
            let data = crate::src::shared::rc::as_ptr(&owner);
            let first_owner = window_customize_add_item(
                &mut (*data).item_list,
                window_customize_itemdata {
                    name: Some(c"stable".to_owned()),
                    ..window_customize_itemdata::new()
                },
            );
            let observer = first_owner.clone();
            // A key's parent and detail rows share a single immutable record.
            let selected = ModeTreeItemData::Customize(first_owner.clone());
            let detail = selected.clone();
            assert!(selected.same_identity(&detail));
            let snapshot = selected.as_customize().unwrap();
            for _ in 0..128 {
                window_customize_add_item(&mut (*data).item_list, window_customize_itemdata::new());
            }
            assert!(first_owner.is(&(&(*data).item_list)[0]));
            let mode_observer = Rc::downgrade(&owner);
            let live = window_customize_live_mode(&mode_observer).unwrap();
            assert!(Rc::ptr_eq(&live, &owner));
            assert_eq!(Rc::strong_count(&owner), 2);
            drop(live);
            let (mut mode_input, mode_free) =
                window_customize_mode_prompt_callbacks(owner.clone(), read_mode);
            assert_eq!(Rc::strong_count(&owner), 2);
            assert_eq!(
                mode_input.as_mut().unwrap()(None, None, PROMPT_KEY_CLOSE),
                PROMPT_CONTINUE
            );
            (*data).dead = 1;
            assert!(window_customize_live_mode(&mode_observer).is_none());
            assert_eq!(
                mode_input.as_mut().unwrap()(None, None, PROMPT_KEY_CLOSE),
                PROMPT_CLOSE
            );
            mode_free.unwrap()();
            (*data).dead = 0;
            assert_eq!(Rc::strong_count(&owner), 1);
            let prompt_owner = RefBox::new(CustomizePromptItem {
                item: window_customize_copy_item(&snapshot),
                mode: mode_observer.upgrade().unwrap(),
            });
            let prompt_observer = RefBox::downgrade(&prompt_owner);
            let (mut inputcb, freecb) = window_customize_prompt_callbacks(prompt_owner, read_item);
            drop(owner);
            assert_eq!(
                inputcb.as_mut().unwrap()(None, None, PROMPT_KEY_CLOSE),
                PROMPT_CONTINUE
            );
            freecb.unwrap()();
            assert!(mode_observer.upgrade().is_none());
            assert_eq!(
                mode_input.as_mut().unwrap()(None, None, PROMPT_KEY_CLOSE),
                PROMPT_CLOSE
            );
            assert!(window_customize_live_mode(&mode_observer).is_none());
            assert!(matches!(
                prompt_observer.try_borrow_mut(),
                Err(refbox::BorrowError::Dropped)
            ));
            // Caching the input callback must not keep a closed prompt alive.
            assert_eq!(
                inputcb.as_mut().unwrap()(None, None, PROMPT_KEY_CLOSE),
                PROMPT_CLOSE
            );
            assert!(!observer.is_alive());
            assert!(selected.as_customize().is_none());
            assert!(detail.as_customize().is_none());
            assert_eq!(snapshot.name.as_deref(), Some(c"stable"));
        }
    }

    #[test]
    fn editor_cancellation_drops_its_owned_record_without_dispatch() {
        let edit = window_customize_editdata {
            wp_id: u_int::MAX,
            edit_type: WINDOW_CUSTOMIZE_EDIT_OPTION,
            item: window_customize_new_item(),
        };
        let capture = (edit, Rc::new(()));
        let observer = Rc::downgrade(&capture.1);
        let mut state = spawn_editor_state {
            id: Default::default(),
            path: c"unused".to_owned(),
            pid: 0,
            cb: Some(Box::new(move |_, _| {
                drop(capture);
                panic!("cancelled editor callback ran");
            })),
        };
        assert!(observer.upgrade().is_some());
        unsafe {
            spawn_cancel_editor(&mut state);
        }
        assert!(observer.upgrade().is_none());
        assert!(state.cb.is_none());
        unsafe {
            spawn_cancel_editor(&mut state);
        }
    }
}
