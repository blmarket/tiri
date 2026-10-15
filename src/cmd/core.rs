pub use crate::src::arguments::args_parse;
use crate::src::arguments::ArgsParseError;
use crate::src::arguments::{args_copy, args_escape_cstring, args_print_cstring};
use crate::src::cmd::entries::attach_session::cmd_attach_session_entry;
use crate::src::cmd::entries::bind_key::cmd_bind_key_entry;
use crate::src::cmd::entries::break_pane::cmd_break_pane_entry;
use crate::src::cmd::entries::capture_pane::{cmd_capture_pane_entry, cmd_clear_history_entry};
use crate::src::cmd::entries::choose_tree::{
    cmd_choose_buffer_entry, cmd_choose_client_entry, cmd_choose_tree_entry,
    cmd_customize_mode_entry, cmd_display_panes_entry, cmd_switch_mode_entry,
};
use crate::src::cmd::entries::command_prompt::cmd_command_prompt_entry;
use crate::src::cmd::entries::confirm_before::cmd_confirm_before_entry;
use crate::src::cmd::entries::copy_mode::{cmd_clock_mode_entry, cmd_copy_mode_entry};
use crate::src::cmd::entries::detach_client::{cmd_detach_client_entry, cmd_suspend_client_entry};
use crate::src::cmd::entries::display_menu::{cmd_display_menu_entry, cmd_display_popup_entry};
use crate::src::cmd::entries::display_message::cmd_display_message_entry;
use crate::src::cmd::entries::find_window::cmd_find_window_entry;
use crate::src::cmd::entries::if_shell::cmd_if_shell_entry;
use crate::src::cmd::entries::join_pane::cmd_join_pane_entry;
use crate::src::cmd::entries::kill_pane::cmd_kill_pane_entry;
use crate::src::cmd::entries::kill_server::{cmd_kill_server_entry, cmd_start_server_entry};
use crate::src::cmd::entries::kill_session::cmd_kill_session_entry;
use crate::src::cmd::entries::kill_window::{cmd_kill_window_entry, cmd_unlink_window_entry};
use crate::src::cmd::entries::list_buffers::cmd_list_buffers_entry;
use crate::src::cmd::entries::list_clients::cmd_list_clients_entry;
use crate::src::cmd::entries::list_commands::cmd_list_commands_entry;
use crate::src::cmd::entries::list_keys::cmd_list_keys_entry;
use crate::src::cmd::entries::list_panes::cmd_list_panes_entry;
use crate::src::cmd::entries::list_sessions::cmd_list_sessions_entry;
use crate::src::cmd::entries::list_windows::cmd_list_windows_entry;
use crate::src::cmd::entries::load_buffer::cmd_load_buffer_entry;
use crate::src::cmd::entries::lock_server::{
    cmd_lock_client_entry, cmd_lock_server_entry, cmd_lock_session_entry,
};
use crate::src::cmd::entries::move_window::{cmd_link_window_entry, cmd_move_window_entry};
use crate::src::cmd::entries::new_session::{cmd_has_session_entry, cmd_new_session_entry};
use crate::src::cmd::entries::new_window::cmd_new_window_entry;
use crate::src::cmd::entries::paste_buffer::cmd_paste_buffer_entry;
use crate::src::cmd::entries::pipe_pane::cmd_pipe_pane_entry;
use crate::src::cmd::entries::refresh_client::cmd_refresh_client_entry;
use crate::src::cmd::entries::rename_session::cmd_rename_session_entry;
use crate::src::cmd::entries::rename_window::cmd_rename_window_entry;
use crate::src::cmd::entries::resize_pane::cmd_resize_pane_entry;
use crate::src::cmd::entries::resize_window::cmd_resize_window_entry;
use crate::src::cmd::entries::respawn_pane::cmd_respawn_pane_entry;
use crate::src::cmd::entries::respawn_window::cmd_respawn_window_entry;
use crate::src::cmd::entries::rotate_window::cmd_rotate_window_entry;
use crate::src::cmd::entries::run_shell::cmd_run_shell_entry;
use crate::src::cmd::entries::save_buffer::{cmd_save_buffer_entry, cmd_show_buffer_entry};
use crate::src::cmd::entries::select_layout::cmd_select_layout_entry;
use crate::src::cmd::entries::select_pane::{cmd_last_pane_entry, cmd_select_pane_entry};
use crate::src::cmd::entries::select_window::{
    cmd_last_window_entry, cmd_next_window_entry, cmd_previous_window_entry,
    cmd_select_window_entry,
};
use crate::src::cmd::entries::send_keys::{cmd_send_keys_entry, cmd_send_prefix_entry};
use crate::src::cmd::entries::server_access::cmd_server_access_entry;
use crate::src::cmd::entries::set_buffer::{cmd_delete_buffer_entry, cmd_set_buffer_entry};
use crate::src::cmd::entries::set_environment::cmd_set_environment_entry;
use crate::src::cmd::entries::set_option::{
    cmd_set_hook_entry, cmd_set_option_entry, cmd_set_window_option_entry,
};
use crate::src::cmd::entries::show_environment::cmd_show_environment_entry;
use crate::src::cmd::entries::show_messages::cmd_show_messages_entry;
use crate::src::cmd::entries::show_options::{
    cmd_show_hooks_entry, cmd_show_options_entry, cmd_show_window_options_entry,
};
use crate::src::cmd::entries::show_prompt_history::{
    cmd_clear_prompt_history_entry, cmd_show_prompt_history_entry,
};
use crate::src::cmd::entries::source_file::cmd_source_file_entry;
use crate::src::cmd::entries::split_window::{cmd_new_pane_entry, cmd_split_window_entry};
use crate::src::cmd::entries::swap_pane::cmd_swap_pane_entry;
use crate::src::cmd::entries::swap_window::cmd_swap_window_entry;
use crate::src::cmd::entries::switch_client::cmd_switch_client_entry;
use crate::src::cmd::entries::unbind_key::cmd_unbind_key_entry;
use crate::src::cmd::entries::wait_for::cmd_wait_for_entry;
use crate::src::ffi::libc::strlcpy;
use crate::src::log::{fatalx, log_bytes, log_debug};
use crate::src::window::Window as _;
use libc::{strchr, strncmp};

use crate::src::session::Session;
use crate::src::shared::abi::*;
pub use crate::src::shared::arguments::args;
use crate::src::shared::arguments::ArgumentValue;
use crate::src::shared::arguments::*;
pub use crate::src::shared::command::{cmd, cmd_entry, cmd_list, cmdq_item};
pub use crate::src::shared::command::{CMD_LIST_PRINT_ESCAPED, CMD_LIST_PRINT_NO_GROUPS};
use crate::src::shared::command::{CMD_READONLY, CMD_STARTSERVER};
pub use crate::src::shared::environment::environ;
pub use crate::src::shared::format::{format_job_tree, format_tree};
pub use crate::src::shared::key::key_event;
pub use crate::src::shared::mouse::mouse_event;
pub use crate::src::shared::options::{options, options_array_item, options_entry, options_value};
pub use crate::src::shared::pane::window_pane;
use crate::src::shared::session::SessionRef;
pub use crate::src::shared::tty::tty_term;
pub use crate::src::shared::window::winlink;
use crate::src::tmux::global_options;
use crate::src::window::winlink_find_by_window;
use crate::src::window_pane::WindowPane as _;
use std::ffi::{CStr, CString};

pub const DQ: C2RustUnnamed_38 = 2;
pub type C2RustUnnamed_38 = ::core::ffi::c_uint;
pub const SQ: C2RustUnnamed_38 = 1;
pub const NQ: C2RustUnnamed_38 = 0;
pub static cmd_table: [&cmd_entry; 89] = {
    [
        &cmd_attach_session_entry,
        &cmd_bind_key_entry,
        &cmd_break_pane_entry,
        &cmd_capture_pane_entry,
        &cmd_choose_buffer_entry,
        &cmd_choose_client_entry,
        &cmd_choose_tree_entry,
        &cmd_clear_history_entry,
        &cmd_clear_prompt_history_entry,
        &cmd_clock_mode_entry,
        &cmd_command_prompt_entry,
        &cmd_confirm_before_entry,
        &cmd_copy_mode_entry,
        &cmd_customize_mode_entry,
        &cmd_delete_buffer_entry,
        &cmd_detach_client_entry,
        &cmd_display_menu_entry,
        &cmd_display_message_entry,
        &cmd_display_popup_entry,
        &cmd_display_panes_entry,
        &cmd_find_window_entry,
        &cmd_has_session_entry,
        &cmd_if_shell_entry,
        &cmd_join_pane_entry,
        &cmd_kill_pane_entry,
        &cmd_kill_server_entry,
        &cmd_kill_session_entry,
        &cmd_kill_window_entry,
        &cmd_last_pane_entry,
        &cmd_last_window_entry,
        &cmd_link_window_entry,
        &cmd_list_buffers_entry,
        &cmd_list_clients_entry,
        &cmd_list_commands_entry,
        &cmd_list_keys_entry,
        &cmd_list_panes_entry,
        &cmd_list_sessions_entry,
        &cmd_list_windows_entry,
        &cmd_load_buffer_entry,
        &cmd_lock_client_entry,
        &cmd_lock_server_entry,
        &cmd_lock_session_entry,
        &cmd_move_window_entry,
        &cmd_new_pane_entry,
        &cmd_new_session_entry,
        &cmd_new_window_entry,
        &cmd_next_window_entry,
        &cmd_paste_buffer_entry,
        &cmd_pipe_pane_entry,
        &cmd_previous_window_entry,
        &cmd_refresh_client_entry,
        &cmd_rename_session_entry,
        &cmd_rename_window_entry,
        &cmd_resize_pane_entry,
        &cmd_resize_window_entry,
        &cmd_respawn_pane_entry,
        &cmd_respawn_window_entry,
        &cmd_rotate_window_entry,
        &cmd_run_shell_entry,
        &cmd_save_buffer_entry,
        &cmd_select_layout_entry,
        &cmd_select_pane_entry,
        &cmd_select_window_entry,
        &cmd_send_keys_entry,
        &cmd_send_prefix_entry,
        &cmd_server_access_entry,
        &cmd_set_buffer_entry,
        &cmd_set_environment_entry,
        &cmd_set_hook_entry,
        &cmd_set_option_entry,
        &cmd_set_window_option_entry,
        &cmd_show_buffer_entry,
        &cmd_show_environment_entry,
        &cmd_show_hooks_entry,
        &cmd_show_messages_entry,
        &cmd_show_options_entry,
        &cmd_show_prompt_history_entry,
        &cmd_show_window_options_entry,
        &cmd_source_file_entry,
        &cmd_split_window_entry,
        &cmd_start_server_entry,
        &cmd_suspend_client_entry,
        &cmd_swap_pane_entry,
        &cmd_swap_window_entry,
        &cmd_switch_client_entry,
        &cmd_switch_mode_entry,
        &cmd_unbind_key_entry,
        &cmd_unlink_window_entry,
        &cmd_wait_for_entry,
    ]
};
static mut cmd_list_next_group: u_int = 1 as u_int;
pub unsafe fn cmd_log_argv(argv: &[CString], prefix: &CStr) {
    for (i, arg) in argv.iter().enumerate() {
        log_debug(format_args!(
            "{}: argv[{}]={}",
            log_bytes(prefix.to_bytes()),
            i as ::core::ffi::c_int,
            log_bytes(arg.as_bytes())
        ));
    }
}
pub(crate) fn cmd_append_argv(argv: &mut Vec<CString>, arg: &CStr) {
    argv.push(arg.to_owned());
}
pub unsafe fn cmd_pack_argv(
    argv: &[CString],
    mut buf: *mut ::core::ffi::c_char,
    mut len: size_t,
) -> ::core::ffi::c_int {
    let mut arglen: size_t = 0;
    let mut i: ::core::ffi::c_int = 0;
    if argv.is_empty() {
        return 0 as ::core::ffi::c_int;
    }
    cmd_log_argv(argv, c"cmd_pack_argv");
    *buf = '\0' as i32 as ::core::ffi::c_char;
    i = 0 as ::core::ffi::c_int;
    while (i as usize) < argv.len() {
        if strlcpy(buf, argv[i as usize].as_ptr(), len) as size_t >= len {
            return -(1 as ::core::ffi::c_int);
        }
        arglen = argv[i as usize].as_bytes_with_nul().len() as size_t;
        buf = buf.add(arglen);
        len = len.wrapping_sub(arglen);
        i += 1;
    }
    0 as ::core::ffi::c_int
}

pub(crate) unsafe fn cmd_stringify_argv_cstring(argv: &[CString]) -> Option<CString> {
    let mut bytes = Vec::new();
    for (i, argument) in argv.iter().enumerate() {
        let escaped = args_escape_cstring(argument);
        log_debug(format_args!(
            "{}: {} {} = {}",
            "cmd_stringify_argv",
            (i) as u32,
            log_bytes(argument.as_bytes()),
            log_bytes(escaped.as_bytes())
        ));
        if i != 0 {
            bytes.push(b' ');
        }
        bytes.extend_from_slice(escaped.as_bytes());
    }
    Some(CString::new(bytes).expect("escaped argv contains no interior NUL"))
}
pub fn cmd_get_entry(cmd: &cmd) -> &'static cmd_entry {
    cmd.entry
}
pub fn cmd_get_args(cmd: &cmd) -> Option<&args> {
    cmd.args.as_deref()
}
pub fn cmd_get_args_mut(cmd: &mut cmd) -> Option<&mut args> {
    cmd.args.as_deref_mut()
}
pub unsafe fn cmd_get_group(mut cmd: refbox::Weak<cmd>) -> u_int {
    cmd.get_unchecked().group
}
pub fn cmd_get_source(cmd: &cmd) -> (Option<&CStr>, u32) {
    (cmd.file.as_deref(), cmd.line)
}
pub unsafe fn cmd_get_parse_flags(mut cmd: refbox::Weak<cmd>) -> ::core::ffi::c_int {
    cmd.get_unchecked().parse_flags
}
pub unsafe fn cmd_get_alias(name: &CStr) -> Option<CString> {
    let mut o: *mut options_entry = ::core::ptr::null_mut::<options_entry>();
    let mut a: *mut options_array_item = ::core::ptr::null_mut::<options_array_item>();
    let mut ov: *mut options_value = ::core::ptr::null_mut::<options_value>();
    let mut wanted: size_t = 0;
    let mut n: size_t = 0;
    let mut equals: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    o = crate::src::options::options_get_only_mut(
        &mut *(global_options),
        std::ffi::CStr::from_ptr(c"command-alias".as_ptr()),
    )
    .map_or(std::ptr::null_mut(), |entry| entry);
    if o.is_null() {
        return None;
    }
    wanted = name.to_bytes().len();
    let a_root = o;
    let mut a_keys = crate::src::options::options_array_iter(&*a_root)
        .map(|item| item.key.clone())
        .collect::<Vec<_>>()
        .into_iter();
    a = a_keys.next().map_or(std::ptr::null_mut(), |key| {
        crate::src::options::options_array_item(a_root, &key)
    });
    while !a.is_null() {
        ov = crate::src::options::options_array_item_value_mut(&mut *(a))
            as *mut crate::src::shared::options::options_value;
        equals = strchr(
            (*ov)
                .string_ptr()
                .map_or(std::ptr::null_mut(), |value| value.as_ptr().cast_mut()),
            '=' as i32,
        );
        if !equals.is_null() {
            n = equals.offset_from(
                (*ov)
                    .string_ptr()
                    .map_or(std::ptr::null_mut(), |value| value.as_ptr().cast_mut()),
            ) as ::core::ffi::c_long as size_t;
            if n == wanted
                && strncmp(
                    name.as_ptr(),
                    (*ov)
                        .string_ptr()
                        .map_or(std::ptr::null_mut(), |value| value.as_ptr().cast_mut()),
                    n,
                ) == 0 as ::core::ffi::c_int
            {
                return Some(
                    CStr::from_ptr(equals.offset(1 as ::core::ffi::c_int as isize)).to_owned(),
                );
            }
        }
        a = a_keys.next().map_or(std::ptr::null_mut(), |key| {
            crate::src::options::options_array_item(a_root, &key)
        });
    }
    None
}
pub fn cmd_find(name: &CStr) -> Result<&'static cmd_entry, CString> {
    let mut found = None;
    let mut ambiguous = false;
    for &entry in &cmd_table {
        if entry.alias == Some(name) {
            ambiguous = false;
            found = Some(entry);
            break;
        }
        if entry.name.to_bytes().starts_with(name.to_bytes()) {
            if found.is_some() {
                ambiguous = true;
            }
            found = Some(entry);
            if entry.name == name {
                break;
            }
        }
    }
    if ambiguous {
        // Keep the pinned diagnostic's bounded stack buffer and truncation.
        let mut candidates = [0u8; 8192];
        let mut len = 0;
        'entries: for &entry in &cmd_table {
            if !entry.name.to_bytes().starts_with(name.to_bytes()) {
                continue;
            }
            for bytes in [entry.name.to_bytes(), b", ".as_slice()] {
                let available = candidates.len() - 1 - len;
                let count = bytes.len().min(available);
                candidates[len..len + count].copy_from_slice(&bytes[..count]);
                len += count;
                if count < bytes.len() {
                    break 'entries;
                }
            }
        }
        let mut error = b"ambiguous command: ".to_vec();
        error.extend_from_slice(name.to_bytes());
        error.extend_from_slice(b", could be: ");
        error.extend_from_slice(&candidates[..len - 2]);
        return Err(CString::new(error).expect("command diagnostic contains no NUL"));
    }
    found.ok_or_else(|| {
        let mut error = b"unknown command: ".to_vec();
        error.extend_from_slice(name.to_bytes());
        CString::new(error).expect("command diagnostic contains no NUL")
    })
}

fn cmd_new_owned(entry: &'static cmd_entry, file: Option<&CStr>) -> refbox::RefBox<cmd> {
    refbox::RefBox::new(cmd {
        file: file.map(CStr::to_owned),
        ..cmd::new(entry)
    })
}

pub unsafe fn cmd_parse(
    values: &[ArgumentValue<'_>],
    file: Option<&CStr>,
    line: u_int,
    parse_flags: ::core::ffi::c_int,
) -> Result<refbox::RefBox<cmd>, CString> {
    let Some(command) = values.first().filter(|value| value.type_0() == ARGS_STRING) else {
        return Err(CString::new("no command").unwrap());
    };
    let entry = cmd_find(command.as_string().expect("command name"))?;
    let args = match args_parse(&entry.args, values) {
        Ok(args) => args,
        Err(ArgsParseError::Usage) => {
            let mut error = b"usage: ".to_vec();
            error.extend_from_slice(entry.name.to_bytes());
            error.push(b' ');
            error.extend_from_slice(entry.usage.to_bytes());
            return Err(CString::new(error).expect("command diagnostic contains no NUL"));
        }
        Err(ArgsParseError::Message(message)) => {
            let mut error = b"command ".to_vec();
            error.extend_from_slice(entry.name.to_bytes());
            error.extend_from_slice(b": ");
            error.extend_from_slice(message.as_bytes());
            return Err(CString::new(error).expect("command diagnostic contains no NUL"));
        }
    };
    let cmd = cmd_new_owned(entry, file);
    {
        let mut command = cmd.try_borrow_mut().expect("new command is not borrowed");
        command.args = Some(args);
        command.parse_flags = parse_flags;
        command.line = line;
    }
    Ok(cmd)
}
pub unsafe fn cmd_copy(cmd: &cmd, argv: &Vec<CString>) -> refbox::RefBox<cmd> {
    let new_cmd = cmd_new_owned(cmd.entry, cmd.file.as_deref());
    {
        let mut copy = new_cmd
            .try_borrow_mut()
            .expect("new command is not borrowed");
        copy.args = Some(args_copy(
            cmd.args.as_deref().expect("parsed command arguments"),
            argv,
        ));
        copy.line = cmd.line;
    }
    new_cmd
}
pub unsafe fn cmd_print(cmd: &cmd) -> CString {
    cmd_print_cstring(cmd)
}

pub(crate) unsafe fn cmd_print_cstring(cmd: &cmd) -> CString {
    let args = args_print_cstring(cmd.args.as_deref().expect("parsed command arguments"));
    let arguments = args.as_bytes();
    let name = cmd.entry.name.to_bytes();
    let mut buf = Vec::with_capacity(
        name.len()
            + if arguments.is_empty() {
                0
            } else {
                arguments.len() + 1
            },
    );
    buf.extend_from_slice(name);
    if !arguments.is_empty() {
        buf.push(b' ');
        buf.extend_from_slice(arguments);
    }
    CString::new(buf).expect("command print contains no interior NUL")
}
pub unsafe fn cmd_list_new() -> std::rc::Rc<std::cell::RefCell<cmd_list>> {
    let group = cmd_list_next_group;
    cmd_list_next_group = cmd_list_next_group.wrapping_add(1);
    std::rc::Rc::new(std::cell::RefCell::new(cmd_list {
        group,
        ..cmd_list::default()
    }))
}
pub fn cmd_list_append(
    cmdlist: &std::rc::Rc<std::cell::RefCell<cmd_list>>,
    command: refbox::RefBox<cmd>,
) {
    let mut cmdlist = cmdlist.borrow_mut();
    command
        .try_borrow_mut()
        .expect("command is not borrowed")
        .group = cmdlist.group;
    cmdlist.list.push(command);
}
pub fn cmd_list_append_all(
    cmdlist: &std::rc::Rc<std::cell::RefCell<cmd_list>>,
    from: &std::rc::Rc<std::cell::RefCell<cmd_list>>,
) {
    if std::rc::Rc::ptr_eq(cmdlist, from) {
        return;
    }
    let mut cmdlist = cmdlist.borrow_mut();
    let mut from = from.borrow_mut();
    let group = cmdlist.group;
    for command in &mut from.list {
        command
            .try_borrow_mut()
            .expect("command is not borrowed")
            .group = group;
    }
    cmdlist.list.append(&mut from.list);
}
pub unsafe fn cmd_list_move(
    cmdlist: &std::rc::Rc<std::cell::RefCell<cmd_list>>,
    from: &std::rc::Rc<std::cell::RefCell<cmd_list>>,
) {
    let mut destination = cmdlist.borrow_mut();
    if !std::rc::Rc::ptr_eq(cmdlist, from) {
        destination.list.append(&mut from.borrow_mut().list);
    }
    let group = cmd_list_next_group;
    cmd_list_next_group = cmd_list_next_group.wrapping_add(1);
    destination.group = group;
}
pub unsafe fn cmd_list_copy(
    cmdlist: &cmd_list,
    argv: &Vec<CString>,
) -> std::rc::Rc<std::cell::RefCell<cmd_list>> {
    let mut group: u_int = cmdlist.group;
    let s = cmd_list_print_cstring(cmdlist, 0);
    log_debug(format_args!(
        "{}: {}",
        "cmd_list_copy",
        log_bytes(s.as_bytes())
    ));
    let owner = cmd_list_new();

    for command in &cmdlist.list {
        let cmd = command.try_borrow_mut().expect("command is not borrowed");
        if cmd.group != group {
            let fresh7 = cmd_list_next_group;
            cmd_list_next_group = cmd_list_next_group.wrapping_add(1);
            owner.borrow_mut().group = fresh7;
            group = cmd.group;
        }
        let new_cmd = cmd_copy(&cmd, argv);
        cmd_list_append(&owner, new_cmd);
    }
    let s = cmd_list_print_cstring(&owner.borrow(), 0);
    log_debug(format_args!(
        "{}: {}",
        "cmd_list_copy",
        log_bytes(s.as_bytes())
    ));
    owner
}
pub(crate) unsafe fn cmd_list_print_cstring(cmdlist: &cmd_list, flags: i32) -> CString {
    let mut buf = Vec::new();
    let commands = &cmdlist.list;
    for (index, owner) in commands.iter().enumerate() {
        let cmd = owner.try_borrow_mut().expect("command is not borrowed");
        let this = cmd_print_cstring(&cmd);
        buf.extend_from_slice(this.as_bytes());

        if let Some(next) = commands.get(index + 1) {
            let next = next.try_borrow_mut().expect("command is not borrowed");
            let grouped = flags & CMD_LIST_PRINT_NO_GROUPS == 0 && cmd.group != next.group;
            let separator: &[u8] = match (flags & CMD_LIST_PRINT_ESCAPED != 0, grouped) {
                (false, false) => b" ; ",
                (false, true) => b" ;; ",
                (true, false) => b" \\; ",
                (true, true) => b" \\;\\; ",
            };
            buf.extend_from_slice(separator);
        }
    }
    // All fragments came from C strings or nonzero literal bytes.
    CString::new(buf).expect("command list contains no interior NUL")
}

pub unsafe fn cmd_list_print(cmdlist: &cmd_list, flags: ::core::ffi::c_int) -> CString {
    cmd_list_print_cstring(cmdlist, flags)
}
pub fn cmd_list_first(cmdlist: &cmd_list) -> Option<refbox::Borrow<'_, cmd>> {
    cmdlist
        .list
        .first()
        .map(|owner| owner.try_borrow_mut().expect("command is not borrowed"))
}
pub unsafe fn cmd_list_all_have(cmdlist: &cmd_list) -> ::core::ffi::c_int {
    let mut flag: ::core::ffi::c_int = CMD_READONLY;
    for owner in &cmdlist.list {
        let cmd = owner.try_borrow_mut().expect("command is not borrowed");
        if !cmd.entry.flags & flag != 0 {
            return 0 as ::core::ffi::c_int;
        }
    }
    1 as ::core::ffi::c_int
}
pub unsafe fn cmd_list_any_have(cmdlist: &cmd_list) -> ::core::ffi::c_int {
    let mut flag: ::core::ffi::c_int = CMD_STARTSERVER;
    for owner in &cmdlist.list {
        let cmd = owner.try_borrow_mut().expect("command is not borrowed");
        if cmd.entry.flags & flag != 0 {
            return 1 as ::core::ffi::c_int;
        }
    }
    0 as ::core::ffi::c_int
}
pub unsafe fn cmd_mouse_at(
    pane: &std::rc::Rc<std::cell::UnsafeCell<window_pane>>,
    mouse: *mut mouse_event,
    x: *mut u_int,
    y: *mut u_int,
    last: i32,
) -> i32 {
    let Some((mx, my)) = pane.mouse_position(&*mouse, last != 0) else {
        return -1;
    };
    if !x.is_null() {
        *x = mx;
    }
    if !y.is_null() {
        *y = my;
    }
    0
}
pub unsafe fn cmd_mouse_window(
    mut m: *mut mouse_event,
    sp: Option<&mut Option<SessionRef>>,
) -> refbox::Weak<winlink> {
    let mut s: Option<SessionRef> = None;
    let mut wl: refbox::Weak<winlink> = refbox::Weak::new();
    if (*m).valid == 0 {
        return refbox::Weak::new();
    }
    if (*m).s == -1 {
        return refbox::Weak::new();
    }
    let Some(session_owner) = crate::src::session::sessions::find_by_id((*m).s as u_int) else {
        return refbox::Weak::new();
    };
    s = Some(session_owner.clone());
    if (*m).w == -(1 as ::core::ffi::c_int) {
        wl = s.as_ref().expect("live session").current_winlink();
    } else {
        let Some(window) = crate::src::window::windows::resolve((*m).w as u_int) else {
            return refbox::Weak::new();
        };
        wl = s
            .as_ref()
            .expect("live session")
            .with_winlinks(|links| winlink_find_by_window(links, &window));
        drop(window);
    }
    if let Some(sp) = sp {
        *sp = Some(session_owner);
    }
    wl
}
pub unsafe fn cmd_mouse_pane(
    mut m: *mut mouse_event,
    sp: Option<&mut Option<SessionRef>>,
    mut wlp: *mut refbox::Weak<winlink>,
) -> Option<std::rc::Rc<std::cell::UnsafeCell<window_pane>>> {
    let link = cmd_mouse_window(m, sp);
    if !link.is_alive() {
        return None;
    }
    let window = link.get_unchecked().window_handle().expect("linked window");
    let pane = if (*m).wp == -1 {
        window.active_pane().upgrade()
    } else {
        let pane = std::rc::Rc::<std::cell::UnsafeCell<window_pane>>::find_by_id((*m).wp as u_int)?;
        if !window.contains_pane(&std::rc::Rc::downgrade(&pane)) {
            return None;
        }
        Some(pane)
    };
    if !wlp.is_null() {
        *wlp = link;
    }
    pane
}
pub unsafe fn cmd_template_replace(template: &CStr, s: &CStr, idx: ::core::ffi::c_int) -> CString {
    cmd_template_replace_cstring(template, s, idx)
}

pub(crate) unsafe fn cmd_template_replace_cstring(
    template: &CStr,
    s: &CStr,
    idx: ::core::ffi::c_int,
) -> CString {
    let template_bytes = template.to_bytes();
    if !template_bytes.contains(&b'%') {
        return template.to_owned();
    }

    let mut output = Vec::new();
    let mut replaced = false;
    let mut offset = 0;
    while offset < template_bytes.len() {
        let ch = template_bytes[offset];
        offset += 1;
        if ch == b'%' {
            let next = template_bytes.get(offset).copied().unwrap_or(0);
            let quote = if (b'1'..=b'9').contains(&next) && (next - b'0') as i32 == idx {
                offset += 1;
                if template_bytes.get(offset) == Some(&b'%') {
                    offset += 1;
                    Some(DQ)
                } else {
                    Some(NQ)
                }
            } else if next == b'%' && !replaced {
                replaced = true;
                offset += 1;
                if template_bytes.get(offset) == Some(&b'%') {
                    offset += 1;
                    Some(DQ)
                } else {
                    Some(SQ)
                }
            } else {
                None
            };
            if let Some(quote) = quote {
                let replacement = s.to_bytes();
                if replacement.len() >= usize::MAX / 4
                    || output.len() > usize::MAX - replacement.len() * 4 - 1
                {
                    fatalx(|out| out.write_all(b"argument too long"));
                }
                output.reserve(replacement.len() * 4);
                for &byte in replacement {
                    if quote == SQ && byte == b'\'' {
                        output.extend_from_slice(b"'\\''");
                    } else {
                        if quote == DQ && b"\"\\$;~".contains(&byte) {
                            output.push(b'\\');
                        }
                        output.push(byte);
                    }
                }
                continue;
            }
        }
        if output.len() > usize::MAX - 2 {
            fatalx(|out| out.write_all(b"argument too long"));
        }
        output.push(ch);
    }
    let text = CString::new(output).expect("C string template contains no embedded NUL");
    log_debug(format_args!(
        "{}: {} -> {}",
        "cmd_template_replace",
        log_bytes(template.to_bytes()),
        log_bytes(text.as_bytes())
    ));
    text
}

#[cfg(test)]
mod template_replace_tests {
    use super::*;

    /// (template, replacement, index, expected output)
    type TemplateCase<'a> = (&'a [u8], &'a [u8], i32, &'a [u8]);

    #[test]
    fn preserves_template_substitution_and_quoting_bytes() {
        let cases: &[TemplateCase] = &[
            (b"plain\0", b"unused\0", 1, b"plain"),
            (b"A%1B\0", b"\xff\0", 1, b"A\xffB"),
            (b"A%2B\0", b"x\0", 1, b"A%2B"),
            (b"A%%B%%C\0", b"a'b\0", 1, b"Aa'\\''bB%%C"),
            (b"A%1%B\0", b"\"\\$;~\0", 1, b"A\\\"\\\\\\$\\;\\~B"),
        ];
        for &(template, replacement, idx, expected) in cases {
            let actual = unsafe {
                cmd_template_replace_cstring(
                    CStr::from_bytes_with_nul(template).unwrap(),
                    CStr::from_bytes_with_nul(replacement).unwrap(),
                    idx,
                )
            };
            assert_eq!(actual.as_bytes(), expected);
        }

        let owned = unsafe { cmd_template_replace(c"%1", c"x", 1) };
        assert_eq!(owned.as_bytes(), b"x");
    }
}
