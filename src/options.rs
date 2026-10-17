use crate::src::alerts::alerts_reset_all;
use crate::src::arguments::{args_get, args_has};
use crate::src::cmd::cmd_list_print_cstring;
use crate::src::cmd::parse::cmd_parse_from_string;
use crate::src::compat::strtonum::strtonum;
use crate::src::ffi::libc::strsep;
use crate::src::format::bytes::format_message_with;
use crate::src::format::bytes::nullable_cstr;
use crate::src::format::bytes::write_cstr;
use crate::src::grid::grid_default_cell;
use crate::src::input::input_set_buffer_size;
use crate::src::key_string::{key_string_format, key_string_parse_cstr};
use crate::src::log::{fatalx, log_cstr, log_debug};
use crate::src::options_parse::{
    match_option_name, parse_array_index, parse_option_name, ArrayIndex, ArrayIndexError,
    OptionNameMatch, OptionNameMatchError,
};
use crate::src::options_table::{options_other_names, options_table};
use crate::src::resize::recalculate_sizes;
use crate::src::screen_redraw::redraw_invalidate_all_scenes;
use crate::src::server::current_time;
use crate::src::server_client::Client as _;
use crate::src::server_client::ClientRegistry;
use crate::src::shared::window::WindowRef;
use libc::{fnmatch, strcasecmp, strcmp, strncmp, strstr};

use crate::src::server_fn::server_redraw_client;
use crate::src::session::sessions;

use crate::src::session::Session;
use crate::src::shared::client::ClientRef;
use crate::src::shared::session::SessionRef;
use crate::src::status::status_timer_start_all;
use crate::src::style::colour::{colour_format, colour_parse_cstr};
use crate::src::style::{style_parse, style_parse_colour};
use crate::src::text::utf8::utf8_update_width_cache;
use crate::src::tmux::{checkshell, global_options};
use crate::src::tty::tty_invalidate;
use crate::src::tty_keys::tty_keys_build;

use crate::src::window::{windows, Window, WindowPane};
use std::collections::btree_map;
use std::ffi::{CStr, CString};

mod mutation;
mod scope;
pub use scope::{OptionsBorrow, OptionsScope};

macro_rules! format_options_cause {
    ($cause:expr, $fmt:expr $(, $arg:expr)* $(,)?) => {{
        let cause = $cause;
        if !cause.is_null() {
            *cause = Some(options_string_cause($fmt, &[$($arg),*]));
        }
    }};
}

/// The option diagnostics use only literal text and `%s` substitutions.
fn options_string_cause(fmt: &CStr, args: &[&CStr]) -> CString {
    let fmt = fmt.to_bytes();
    let mut message = Vec::with_capacity(fmt.len());
    let mut at = 0;
    let mut arg = 0;
    while at < fmt.len() {
        if fmt[at..].starts_with(b"%s") {
            message.extend_from_slice(args[arg].to_bytes());
            at += 2;
            arg += 1;
        } else {
            message.push(fmt[at]);
            at += 1;
        }
    }
    debug_assert_eq!(arg, args.len());
    CString::new(message).expect("option diagnostic contains no NUL")
}

use crate::src::shared::abi::*;
use crate::src::shared::arguments::args;
use crate::src::shared::command::{cmd_find_state, cmd_list};
use crate::src::shared::command::{cmd_parse_input, cmd_parse_result};
use crate::src::shared::format::format_tree;
use crate::src::shared::grid::*;
use crate::src::shared::key::*;
use crate::src::shared::limits::UINT_MAX;
use crate::src::shared::options::*;
use crate::src::shared::options::{
    options, options_array_item, options_array_storage, options_entry, options_table_entry,
    options_value, OptionCommand, OptionsArrayKey,
};
use crate::src::shared::options::{
    OPTIONS_TABLE_IS_ARRAY, OPTIONS_TABLE_IS_COLOUR, OPTIONS_TABLE_IS_STYLE, OPTIONS_TABLE_PANE,
    OPTIONS_TABLE_SERVER, OPTIONS_TABLE_SESSION, OPTIONS_TABLE_WINDOW,
};
use crate::src::shared::style::*;
use crate::src::shared::tty::TTY_OPENED;

fn set_scalar_string(o: &mut options_entry, value: CString) {
    o.value = options_value::String(value);
}

fn set_array_string(ov: &mut options_value, value: CString) {
    *ov = options_value::String(value);
}

fn options_array_correct_key(key: &CStr) -> Option<CString> {
    match parse_array_index(key.to_bytes()) {
        Ok(ArrayIndex::Numeric(number)) => {
            Some(CString::new(number.to_string()).expect("numeric key has no NUL"))
        }
        Ok(ArrayIndex::Text(bytes)) => {
            Some(CString::new(bytes).expect("C string key has no interior NUL"))
        }
        Err(ArrayIndexError::Empty | ArrayIndexError::NumericOverflow) => None,
    }
}
pub(crate) fn options_array_index(key: &CStr) -> OptionsArrayKey {
    match parse_array_index(key.to_bytes()).expect("validated array key") {
        ArrayIndex::Numeric(number) => OptionsArrayKey::Numeric(number),
        ArrayIndex::Text(bytes) => OptionsArrayKey::Text(bytes.to_vec()),
    }
}

unsafe fn options_parent_table_entry(
    oo: &options,
    name: &CStr,
) -> Option<&'static options_table_entry> {
    let parent = oo.parent.clone().unwrap_or_else(|| {
        fatalx(|out| {
            out.write_all(b"no parent options for ")?;
            write_cstr(out, &*name)
        })
    });
    let source = parent.resolve(name, false).unwrap_or_else(|| {
        fatalx(|out| {
            write_cstr(out, &*name)?;
            out.write_all(b" not in parent options")
        })
    });
    source.entry(name)?.tableentry
}
unsafe fn options_value_to_cstring(
    o: &options_entry,
    ov: &options_value,
    numeric: ::core::ffi::c_int,
) -> CString {
    let Some(tableentry) = o.tableentry else {
        return ov.string_ptr().expect("string option").to_owned();
    };
    match tableentry.type_0 {
        OPTIONS_TABLE_COMMAND => {
            cmd_list_print_cstring(&ov.commands().expect("command option").borrow(), 0)
        }
        OPTIONS_TABLE_NUMBER => {
            CString::new(ov.number().to_string()).expect("decimal number has no NUL")
        }
        OPTIONS_TABLE_KEY => key_string_format(ov.number() as key_code, false),
        OPTIONS_TABLE_COLOUR => colour_format(ov.number() as ::core::ffi::c_int),
        OPTIONS_TABLE_FLAG => {
            if numeric != 0 {
                CString::new(ov.number().to_string()).expect("decimal number has no NUL")
            } else {
                if ov.number() != 0 { c"on" } else { c"off" }.to_owned()
            }
        }
        OPTIONS_TABLE_CHOICE => tableentry.choices[ov.number() as usize].to_owned(),
        OPTIONS_TABLE_STRING => ov.string_ptr().expect("string option").to_owned(),
        _ => c"".to_owned(),
    }
}

pub fn options_create_owned(parent: Option<OptionsScope>) -> Box<options> {
    Box::new(options {
        tree: Default::default(),
        parent,
    })
}

/// Return the sole owner for explicit cleanup or transfer to a model.
pub fn options_create(parent: Option<OptionsScope>) -> Box<options> {
    options_create_owned(parent)
}

impl Drop for options {
    fn drop(&mut self) {
        let names: Vec<_> = self.tree.keys().cloned().collect();
        for name in names {
            if let btree_map::Entry::Occupied(entry) = options_local_entry(self, &name) {
                options_remove(entry);
            }
        }
    }
}

pub fn options_iter(oo: &options) -> impl DoubleEndedIterator<Item = (&CStr, &options_entry)> {
    oo.tree
        .iter()
        .map(|(name, entry)| (name.as_c_str(), entry.as_ref()))
}

pub fn options_local_entry<'a>(
    oo: &'a mut options,
    name: &CStr,
) -> btree_map::Entry<'a, CString, Box<options_entry>> {
    oo.tree.entry(name.to_owned())
}

/// Read an entry synchronously, copying the result before an inherited table's
/// guard is returned. The visitor must not reenter models or return observers.
/// Prefer OptionsScope::resolve/entry for work spanning callbacks.
pub unsafe fn options_read_entry<R>(
    table: &options,
    name: &CStr,
    read: impl FnOnce(&options_entry) -> R,
) -> Option<R> {
    if let Some(entry) = table.tree.get(name).map(Box::as_ref) {
        return Some(read(entry));
    }
    let parent = table.parent.as_ref()?;
    let source = parent.resolve(name, false)?;
    source.entry(name).as_deref_mut().map(|entry| read(entry))
}
pub fn options_empty<'a>(
    oo: &'a mut options,
    definition: &'static options_table_entry,
) -> &'a mut options_entry {
    let o = options_add(oo, definition.name.expect("named option definition"));
    o.tableentry = Some(definition);
    if definition.flags & OPTIONS_TABLE_IS_ARRAY != 0 {
        o.value = options_value::Array(options_array_storage::default());
    }
    o
}
pub unsafe fn options_default<'a>(
    oo: &'a mut options,
    oe: &'static options_table_entry,
) -> &'a mut options_entry {
    let mut pr: cmd_parse_result = cmd_parse_result::empty();
    let o = options_empty(oo, oe);
    if oe.flags & OPTIONS_TABLE_IS_ARRAY != 0 {
        if oe.default_arr.is_none() {
            options_array_assign(o, oe.default_str.expect("array option default"))
                .expect("valid array option default");
            return o;
        }
        for (i, value) in oe.default_arr.expect("array defaults").iter().enumerate() {
            let key = format_message_with(|out| write!(out, "{}", i as u32));
            options_array_set(o, &key, Some(*value), false).expect("valid array option default");
        }
        return o;
    }
    match oe.type_0 as ::core::ffi::c_uint {
        0 => {
            set_scalar_string(o, oe.default_str.expect("string option default").to_owned());
        }
        6 => {
            pr = cmd_parse_from_string(
                oe.default_str.expect("string option default"),
                ::core::ptr::null_mut::<cmd_parse_input>(),
            );
            match pr.status as ::core::ffi::c_uint {
                0 => {}
                1 => {
                    o.value = options_value::Command(OptionCommand(pr.cmdlist.take()));
                }
                _ => {}
            }
        }
        _ => {
            o.value = options_value::Number(oe.default_num);
        }
    }
    o
}
/// Format a built-in default for Rust callers. The exported C API keeps its
/// libc-owned return value below.
pub(crate) unsafe fn options_default_to_cstring(oe: &options_table_entry) -> CString {
    match oe.type_0 as ::core::ffi::c_uint {
        0 | 6 => oe.default_str.expect("string option default").to_owned(),
        1 => CString::new(oe.default_num.to_string())
            .expect("decimal option default contains no NUL"),
        2 => key_string_format(oe.default_num as key_code, false),
        3 => colour_format(oe.default_num as ::core::ffi::c_int),
        4 => if oe.default_num != 0 { c"on" } else { c"off" }.to_owned(),
        5 => oe.choices[oe.default_num as usize].to_owned(),
        _ => {
            fatalx(|out| out.write_all(b"unknown option type"));
        }
    }
}
fn options_add<'a>(oo: &'a mut options, name: &CStr) -> &'a mut options_entry {
    let owned = Box::new(options_entry {
        tableentry: None,
        value: options_value::Empty,
        style: None,
        monitor_data: None,
        fire_count: 0,
        fire_time: 0,
    });
    match oo.tree.entry(name.to_owned()) {
        btree_map::Entry::Vacant(slot) => &mut **slot.insert(owned),
        btree_map::Entry::Occupied(mut slot) => {
            drop(slot.insert(owned));
            &mut **slot.into_mut()
        }
    }
}
fn options_remove(entry: btree_map::OccupiedEntry<'_, CString, Box<options_entry>>) {
    drop(entry.remove());
}
pub fn options_get_monitor_data(
    o: &mut options_entry,
) -> Option<&mut crate::src::hooks::hooks_monitor> {
    o.monitor_data.as_deref_mut()
}
pub unsafe fn options_hook_fired(o: &mut options_entry) {
    o.fire_count = o.fire_count.wrapping_add(1);
    o.fire_time = current_time;
}
pub fn options_get_fire_count(o: &options_entry) -> u_int {
    o.fire_count
}
pub fn options_get_fire_time(o: &options_entry) -> time_t {
    o.fire_time
}
pub fn options_table_entry(o: &options_entry) -> Option<&'static options_table_entry> {
    o.tableentry
}
fn options_array_new<'a>(o: &'a mut options_entry, key: &CStr) -> &'a mut options_value {
    let item = o
        .value
        .array_storage()
        .entries
        .entry(options_array_index(key))
        .insert_entry(Box::new(options_array_item {
            key: key.to_owned(),
            value: options_value::Empty,
        }))
        .into_mut();
    &mut item.value
}

pub fn options_array_clear(o: &mut options_entry) {
    if !o
        .tableentry
        .is_some_and(|entry| entry.flags & OPTIONS_TABLE_IS_ARRAY != 0)
    {
        return;
    }

    o.value.array_storage().entries.clear();
}

pub fn options_array_get<'a>(o: &'a options_entry, key: &CStr) -> Option<&'a options_value> {
    let key = options_array_correct_key(key)?;
    let options_value::Array(array) = &o.value else {
        return None;
    };
    array
        .entries
        .get(&options_array_index(&key))
        .map(|item| &item.value)
}
pub fn options_array_get_mut<'a>(
    o: &'a mut options_entry,
    key: &CStr,
) -> Option<&'a mut options_value> {
    let key = options_array_correct_key(key)?;
    let options_value::Array(array) = &mut o.value else {
        return None;
    };
    array
        .entries
        .get_mut(&options_array_index(&key))
        .map(|item| &mut item.value)
}
/// Look up an array value by its decimal numeric index.
pub fn options_array_get_index(o: &options_entry, index: u_int) -> Option<&options_value> {
    let key = CString::new(index.to_string()).expect("decimal index contains no NUL");
    options_array_get(o, &key)
}
pub fn options_array_get_index_mut(
    o: &mut options_entry,
    index: u_int,
) -> Option<&mut options_value> {
    let key = CString::new(index.to_string()).expect("decimal index contains no NUL");
    options_array_get_mut(o, &key)
}
pub unsafe fn options_array_set(
    o: &mut options_entry,
    key: &CStr,
    value: Option<&CStr>,
    append: bool,
) -> Result<(), CString> {
    let mut value: *const ::core::ffi::c_char = value.map_or(std::ptr::null(), CStr::as_ptr);
    let mut pr: cmd_parse_result = cmd_parse_result::empty();
    let mut number: ::core::ffi::c_longlong = 0;
    if !(o
        .tableentry
        .is_some_and(|entry| entry.flags & OPTIONS_TABLE_IS_ARRAY != 0))
    {
        return Err(c"not an array".to_owned());
    }
    let Some(new_key) = options_array_correct_key(key) else {
        return Err(options_string_cause(c"bad array key: %s", &[key]));
    };
    if value.is_null() {
        let key = options_array_index(&new_key);
        o.value.array_storage().entries.remove(&key);
        return Ok(());
    }
    if o.tableentry
        .is_some_and(|entry| entry.type_0 == OPTIONS_TABLE_COMMAND)
    {
        pr = cmd_parse_from_string(
            CStr::from_ptr(value),
            ::core::ptr::null_mut::<cmd_parse_input>(),
        );
        if pr.status as ::core::ffi::c_uint == 0 {
            return Err(pr.error.expect("command parse error"));
        }
        let ov = match options_array_get_mut(o, &new_key) {
            Some(ov) => ov,
            None => options_array_new(o, &new_key),
        };
        *ov = options_value::Command(OptionCommand(pr.cmdlist.take()));
        return Ok(());
    }
    if o.tableentry
        .is_none_or(|entry| entry.type_0 == OPTIONS_TABLE_STRING)
    {
        let ov = options_array_get_mut(o, &new_key);
        let owned_value = if let Some(ov) = ov.as_ref().filter(|_| append) {
            let previous = ov.string_ptr().expect("string option").to_bytes();
            let suffix = CStr::from_ptr(value).to_bytes();
            let mut bytes = Vec::with_capacity(previous.len() + suffix.len());
            bytes.extend_from_slice(previous);
            bytes.extend_from_slice(suffix);
            CString::new(bytes).expect("C string parts contain no NUL")
        } else {
            CStr::from_ptr(value).to_owned()
        };
        let ov = match ov {
            Some(ov) => ov,
            None => options_array_new(o, &new_key),
        };
        set_array_string(ov, owned_value);
        return Ok(());
    }
    if o.tableentry.expect("table option").type_0 as ::core::ffi::c_uint
        == OPTIONS_TABLE_COLOUR as ::core::ffi::c_int as ::core::ffi::c_uint
    {
        number = colour_parse_cstr(std::ffi::CStr::from_ptr(value)).unwrap_or(-1)
            as ::core::ffi::c_longlong;
        if number == -(1 as ::core::ffi::c_int) as ::core::ffi::c_longlong {
            return Err(options_string_cause(
                c"bad colour: %s",
                &[CStr::from_ptr(value)],
            ));
        }
        let ov = match options_array_get_mut(o, &new_key) {
            Some(ov) => ov,
            None => options_array_new(o, &new_key),
        };
        *ov = options_value::Number(number);
        return Ok(());
    }
    Err(c"wrong array type".to_owned())
}
pub unsafe fn options_array_assign(o: &mut options_entry, s: &CStr) -> Result<(), CString> {
    let s: *const ::core::ffi::c_char = s.as_ptr();
    let mut separator: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    let mut next: *mut ::core::ffi::c_char = ::core::ptr::null_mut::<::core::ffi::c_char>();
    let mut i: u_int = 0;
    separator = o
        .tableentry
        .expect("table option")
        .separator
        .map_or(std::ptr::null(), CStr::as_ptr);
    if separator.is_null() {
        separator = c" ,".as_ptr();
    }
    if *separator as ::core::ffi::c_int == '\0' as i32 {
        if *s as ::core::ffi::c_int == '\0' as i32 {
            return Ok(());
        }
        i = 0 as u_int;
        while i < UINT_MAX {
            if crate::src::options::options_array_get_index_mut(o, i)
                .map_or(std::ptr::null_mut(), |value| value)
                .is_null()
            {
                break;
            }
            i = i.wrapping_add(1);
        }
        let key = format_message_with(|out| write!(out, "{}", i));
        return options_array_set(o, &key, nullable_cstr(s), false);
    }
    if *s as ::core::ffi::c_int == '\0' as i32 {
        return Ok(());
    }
    let mut copy = CStr::from_ptr(s).to_bytes_with_nul().to_vec();
    let mut string = copy.as_mut_ptr().cast::<::core::ffi::c_char>();
    loop {
        next = strsep(&raw mut string, separator);
        if next.is_null() {
            break;
        }
        if *next as ::core::ffi::c_int == '\0' as i32 {
            continue;
        }
        i = 0 as u_int;
        while i < UINT_MAX {
            if crate::src::options::options_array_get_index_mut(o, i)
                .map_or(std::ptr::null_mut(), |value| value)
                .is_null()
            {
                break;
            }
            i = i.wrapping_add(1);
        }
        if i == UINT_MAX {
            break;
        }
        let key = format_message_with(|out| write!(out, "{}", i));
        options_array_set(o, &key, nullable_cstr(next), false)?;
    }
    Ok(())
}
pub fn options_array_iter(
    o: &options_entry,
) -> impl DoubleEndedIterator<Item = &options_array_item> {
    let entries = match &o.value {
        options_value::Array(array) => Some(&array.entries),
        _ => None,
    };
    entries
        .into_iter()
        .flat_map(|entries| entries.values().map(Box::as_ref))
}
pub fn options_is_array(o: &options_entry) -> ::core::ffi::c_int {
    (o.tableentry
        .is_some_and(|entry| entry.flags & OPTIONS_TABLE_IS_ARRAY != 0)) as ::core::ffi::c_int
}
pub fn options_is_string(o: &options_entry) -> ::core::ffi::c_int {
    (o.tableentry
        .is_none_or(|entry| entry.type_0 == OPTIONS_TABLE_STRING)) as ::core::ffi::c_int
}

pub unsafe fn options_to_cstring(
    entry: &options_entry,
    key: Option<&CStr>,
    numeric: i32,
) -> CString {
    let key: *const ::core::ffi::c_char = key.map_or(std::ptr::null(), CStr::as_ptr);
    if entry
        .tableentry
        .is_some_and(|table| table.flags & OPTIONS_TABLE_IS_ARRAY != 0)
    {
        if key.is_null() {
            let values = options_array_iter(entry)
                .map(|item| options_value_to_cstring(entry, &item.value, numeric));
            let mut bytes = Vec::new();
            for (index, value) in values.enumerate() {
                if index != 0 {
                    bytes.push(b' ');
                }
                bytes.extend_from_slice(value.as_bytes());
            }
            return CString::new(bytes).expect("option values have no NUL");
        }
        return options_array_get(entry, CStr::from_ptr(key)).map_or_else(
            || c"".to_owned(),
            |value| options_value_to_cstring(entry, value, numeric),
        );
    }
    options_value_to_cstring(entry, &entry.value, numeric)
}
/// Name and normalized array key owned through the command's synchronous use.
pub struct OwnedOptionName {
    pub name: CString,
    pub array_key: Option<CString>,
}

pub fn options_parse_owned(input: &CStr) -> Option<OwnedOptionName> {
    let bytes = input.to_bytes();
    let parsed = parse_option_name(bytes).ok()?;
    let array_key = match parsed.array_key {
        Some(ArrayIndex::Numeric(number)) => {
            Some(CString::new(number.to_string()).expect("numeric key has no NUL"))
        }
        Some(ArrayIndex::Text(bytes)) => {
            Some(CString::new(bytes).expect("C string key has no interior NUL"))
        }
        None => None,
    };
    let name = options_other_names
        .iter()
        .find_map(|(from, to)| (from.to_bytes() == parsed.name).then_some(to.to_bytes()))
        .unwrap_or(parsed.name);
    Some(OwnedOptionName {
        name: CString::new(name).expect("C string option name has no interior NUL"),
        array_key,
    })
}
pub fn options_search(name: &CStr) -> Option<&'static options_table_entry> {
    options_table.iter().find(|entry| entry.name == Some(name))
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionMatchFailure {
    Parse,
    Ambiguous,
    Invalid,
}

pub unsafe fn options_match_owned(s: &CStr) -> Result<OwnedOptionName, OptionMatchFailure> {
    let mut parsed = options_parse_owned(s).ok_or(OptionMatchFailure::Parse)?;

    let mut candidates: [&[u8]; 274] = [&[]; 274];
    let mut candidate_count = 0usize;
    for name in options_table.iter().map_while(|entry| entry.name) {
        if candidate_count == candidates.len() {
            break;
        }
        candidates[candidate_count] = name.to_bytes();
        candidate_count += 1;
    }

    match match_option_name(parsed.name.to_bytes(), &candidates[..candidate_count]) {
        Ok(OptionNameMatch::User) => Ok(parsed),
        Ok(OptionNameMatch::BuiltIn(index)) => {
            parsed.name = CString::new(candidates[index]).expect("option name has no NUL");
            Ok(parsed)
        }
        Err(OptionNameMatchError::Ambiguous) => Err(OptionMatchFailure::Ambiguous),
        Err(OptionNameMatchError::NotFound) => Err(OptionMatchFailure::Invalid),
    }
}
/// Copy an initialized string; no pointer into an inherited model escapes.
pub unsafe fn options_get_string(oo: &options, name: &CStr) -> CString {
    options_get_string_optional(oo, name).expect("initialized string option")
}
/// Preserve the Empty value used while publishing a new option.
pub unsafe fn options_get_string_optional(oo: &options, name: &CStr) -> Option<CString> {
    options_read_entry(oo, name, |entry| {
        if entry
            .tableentry
            .is_some_and(|table| table.type_0 != OPTIONS_TABLE_STRING)
        {
            fatalx(|out| {
                out.write_all(b"option ")?;
                write_cstr(out, &*name)?;
                out.write_all(b" is not a string")
            });
        }
        entry.value.string_ptr().map(CStr::to_owned)
    })
    .unwrap_or_else(|| {
        fatalx(|out| {
            out.write_all(b"missing option ")?;
            write_cstr(out, &*name)
        })
    })
}
/// Read a numeric option without borrowing the option tree mutably.
///
/// Parent models are accessed through their trait and only copied values escape.
pub unsafe fn options_get_number(table: &options, name: &CStr) -> ::core::ffi::c_longlong {
    options_read_entry(table, name, |entry| {
        if !entry.tableentry.is_some_and(|table| {
            matches!(
                table.type_0,
                OPTIONS_TABLE_NUMBER
                    | OPTIONS_TABLE_KEY
                    | OPTIONS_TABLE_COLOUR
                    | OPTIONS_TABLE_FLAG
                    | OPTIONS_TABLE_CHOICE
            )
        }) {
            fatalx(|out| {
                out.write_all(b"option ")?;
                write_cstr(out, &*name)?;
                out.write_all(b" is not a number")
            });
        }
        entry.value.number()
    })
    .unwrap_or_else(|| {
        fatalx(|out| {
            out.write_all(b"missing option ")?;
            write_cstr(out, &*name)
        })
    })
}

pub unsafe fn options_get_command(oo: &options) -> std::rc::Rc<std::cell::RefCell<cmd_list>> {
    let name = c"default-client-command";
    options_read_entry(oo, name, |entry| {
        if !entry
            .tableentry
            .is_some_and(|table| table.type_0 == OPTIONS_TABLE_COMMAND)
        {
            fatalx(|out| out.write_all(b"option default-client-command is not a command"));
        }
        entry
            .value
            .commands()
            .expect("default client command")
            .clone()
    })
    .unwrap_or_else(|| fatalx(|out| out.write_all(b"missing option default-client-command")))
}
/// Find the local entry, publishing a user option or the inherited built-in
/// default when the table has none yet.
unsafe fn options_local_or_default<'a>(oo: &'a mut options, name: &CStr) -> &'a mut options_entry {
    if oo.tree.contains_key(name) {
        return oo.tree.get_mut(name).expect("local option entry");
    }
    if name.to_bytes().starts_with(b"@") {
        return options_add(oo, name);
    }
    let definition = options_parent_table_entry(oo, name).expect("static option definition");
    options_default(oo, definition)
}
pub unsafe fn options_set_string<'a>(
    oo: &'a mut options,
    name: &CStr,
    append: ::core::ffi::c_int,
    write: impl FnOnce(&mut dyn std::io::Write) -> std::io::Result<()>,
) -> &'a mut options_entry {
    let formatted = format_message_with(write);
    let value = match oo.tree.get(name) {
        Some(o)
            if append != 0
                && o.tableentry
                    .is_none_or(|entry| entry.type_0 == OPTIONS_TABLE_STRING) =>
        {
            let separator = if name.to_bytes().starts_with(b"@") {
                c""
            } else {
                o.tableentry.expect("table option").separator.unwrap_or(c"")
            };
            let previous = o
                .value
                .string_ptr()
                .map_or(b"(null)".as_slice(), CStr::to_bytes);
            let separator = separator.to_bytes();
            let mut bytes =
                Vec::with_capacity(previous.len() + separator.len() + formatted.as_bytes().len());
            bytes.extend_from_slice(previous);
            bytes.extend_from_slice(separator);
            bytes.extend_from_slice(formatted.as_bytes());
            CString::new(bytes).expect("C-string fragments contain no NUL")
        }
        _ => formatted,
    };
    let o = options_local_or_default(oo, name);
    if !o
        .tableentry
        .is_none_or(|entry| entry.type_0 == OPTIONS_TABLE_STRING)
    {
        fatalx(|out| {
            out.write_all(b"option ")?;
            write_cstr(out, &*name)?;
            out.write_all(b" is not a string")
        });
    }
    set_scalar_string(o, value);
    o.style = None;
    o
}
pub unsafe fn options_set_number<'a>(
    oo: &'a mut options,
    name: &CStr,
    value: ::core::ffi::c_longlong,
) -> &'a mut options_entry {
    if name.to_bytes().starts_with(b"@") {
        fatalx(|out| {
            out.write_all(b"user option ")?;
            write_cstr(out, &*name)?;
            out.write_all(b" must be a string")
        });
    }
    let o = options_local_or_default(oo, name);
    if !o.tableentry.is_some_and(|entry| {
        matches!(
            entry.type_0,
            OPTIONS_TABLE_NUMBER
                | OPTIONS_TABLE_KEY
                | OPTIONS_TABLE_COLOUR
                | OPTIONS_TABLE_FLAG
                | OPTIONS_TABLE_CHOICE
        )
    }) {
        fatalx(|out| {
            out.write_all(b"option ")?;
            write_cstr(out, &*name)?;
            out.write_all(b" is not a number")
        });
    }
    o.value = options_value::Number(value);
    o
}
pub unsafe fn options_set_command<'a>(
    oo: &'a mut options,
    name: &CStr,
    value: Option<std::rc::Rc<std::cell::RefCell<cmd_list>>>,
) -> &'a mut options_entry {
    if name.to_bytes().starts_with(b"@") {
        fatalx(|out| {
            out.write_all(b"user option ")?;
            write_cstr(out, &*name)?;
            out.write_all(b" must be a string")
        });
    }
    let o = options_local_or_default(oo, name);
    if !o
        .tableentry
        .is_some_and(|entry| entry.type_0 == OPTIONS_TABLE_COMMAND)
    {
        fatalx(|out| {
            out.write_all(b"option ")?;
            write_cstr(out, &*name)?;
            out.write_all(b" is not a command")
        });
    }
    o.value = options_value::Command(OptionCommand(value));
    o
}
/// Select the options scope for `name`, returning the table kind and scope.
pub unsafe fn options_scope_from_name(
    args: *mut args,
    window: i32,
    name: &CStr,
    fs: &cmd_find_state,
) -> Result<(i32, OptionsScope), CString> {
    if name.to_bytes().starts_with(b"@") {
        return options_scope_from_flags(args, window, fs);
    }
    let Some(definition) = options_table.iter().find(|entry| entry.name == Some(name)) else {
        return Err(options_string_cause(c"unknown option: %s", &[name]));
    };
    let kind = if definition.scope == OPTIONS_TABLE_WINDOW | OPTIONS_TABLE_PANE {
        if args_has(args, b'p') != 0 {
            OPTIONS_TABLE_PANE
        } else {
            OPTIONS_TABLE_WINDOW
        }
    } else {
        definition.scope
    };
    options_select_scope(args, fs, kind)
}
pub unsafe fn options_scope_from_flags(
    args: *mut args,
    window: i32,
    fs: &cmd_find_state,
) -> Result<(i32, OptionsScope), CString> {
    let kind = if args_has(args, b's') != 0 {
        OPTIONS_TABLE_SERVER
    } else if args_has(args, b'p') != 0 {
        OPTIONS_TABLE_PANE
    } else if window != 0 || args_has(args, b'w') != 0 {
        OPTIONS_TABLE_WINDOW
    } else {
        OPTIONS_TABLE_SESSION
    };
    options_select_scope(args, fs, kind)
}
unsafe fn options_select_scope(
    args: *mut args,
    fs: &cmd_find_state,
    kind: i32,
) -> Result<(i32, OptionsScope), CString> {
    let global = args_has(args, b'g') != 0;
    let (scope, noun) = match kind {
        OPTIONS_TABLE_SERVER => (Some(OptionsScope::GlobalServer), c"server"),
        OPTIONS_TABLE_SESSION if global => (Some(OptionsScope::GlobalSession), c"session"),
        OPTIONS_TABLE_SESSION => (
            (fs.s.strong_count() != 0).then(|| OptionsScope::Session(fs.s.clone())),
            c"session",
        ),
        OPTIONS_TABLE_WINDOW if global => (Some(OptionsScope::GlobalWindow), c"window"),
        OPTIONS_TABLE_WINDOW => {
            let scope = if fs.wl.is_alive() {
                Some(OptionsScope::Window(WindowRef::downgrade(
                    fs.wl
                        .get_unchecked()
                        .window_handle()
                        .expect("option window"),
                )))
            } else {
                None
            };
            (scope, c"window")
        }
        OPTIONS_TABLE_PANE => (
            (fs.wp.strong_count() != 0).then(|| OptionsScope::Pane(fs.wp.clone())),
            c"pane",
        ),
        _ => unreachable!("unknown option scope kind"),
    };
    if let Some(scope) = scope {
        return Ok((kind, scope));
    }
    let target = args_get(&*args, b't');
    let diagnostic = if let Some(target) = target {
        let mut bytes = b"no such ".to_vec();
        bytes.extend_from_slice(noun.to_bytes());
        bytes.extend_from_slice(b": ");
        bytes.extend_from_slice(target.to_bytes());
        CString::new(bytes).unwrap()
    } else {
        let mut bytes = b"no current ".to_vec();
        bytes.extend_from_slice(noun.to_bytes());
        CString::new(bytes).unwrap()
    };
    Err(diagnostic)
}
/// Legacy detached/global-table adapter. Model callers must use the scoped
/// evaluator so no component borrow is held through format expansion.
pub unsafe fn options_string_to_style(
    oo: &mut options,
    name: &CStr,
    ft: *mut format_tree,
) -> Option<style> {
    crate::src::style::style_resolve_with_options(name, ft.as_mut(), |visit| visit(oo))
}
unsafe fn options_from_string_check(
    oe: &options_table_entry,
    value: &CStr,
    mut cause: *mut Option<CString>,
) -> ::core::ffi::c_int {
    let mut sy: style = style {
        gc: grid_cell {
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
        },
        ignore: 0,
        dim: 0,
        fill: 0,
        align: STYLE_ALIGN_DEFAULT,
        list: STYLE_LIST_OFF,
        range_type: STYLE_RANGE_NONE,
        range_argument: 0,
        range_string: [0; 16],
        width: 0,
        width_percentage: 0,
        pad: 0,
        default_type: STYLE_DEFAULT_BASE,
        link: 0,
    };
    if oe.name == Some(c"default-shell") && checkshell(nullable_cstr(value.as_ptr())) == 0 {
        format_options_cause!(cause, c"not a suitable shell: %s", value,);
        return -(1 as ::core::ffi::c_int);
    }
    if oe.pattern.is_some_and(|pattern| {
        fnmatch(pattern.as_ptr(), value.as_ptr(), 0 as ::core::ffi::c_int)
            != 0 as ::core::ffi::c_int
    }) {
        format_options_cause!(cause, c"value is invalid: %s", value,);
        return -(1 as ::core::ffi::c_int);
    }
    if oe.flags & OPTIONS_TABLE_IS_STYLE != 0
        && strstr(value.as_ptr(), c"#{".as_ptr()).is_null()
        && style_parse(&raw mut sy, &grid_default_cell, value) != 0 as ::core::ffi::c_int
    {
        format_options_cause!(cause, c"invalid style: %s", value,);
        return -(1 as ::core::ffi::c_int);
    }
    if oe.flags & OPTIONS_TABLE_IS_COLOUR != 0
        && strstr(value.as_ptr(), c"#{".as_ptr()).is_null()
        && style_parse_colour(&raw mut sy, &grid_default_cell, value) != 0 as ::core::ffi::c_int
    {
        format_options_cause!(cause, c"invalid colour: %s", value,);
        return -(1 as ::core::ffi::c_int);
    }
    0 as ::core::ffi::c_int
}
unsafe fn options_from_string_flag(
    oo: &mut options,
    name: &CStr,
    value: Option<&CStr>,
    mut cause: *mut Option<CString>,
) -> ::core::ffi::c_int {
    let value: *const ::core::ffi::c_char = value.map_or(std::ptr::null(), CStr::as_ptr);
    let mut flag: ::core::ffi::c_int = 0;
    if value.is_null() || *value as ::core::ffi::c_int == '\0' as i32 {
        flag = (options_get_number(oo, name) == 0) as ::core::ffi::c_int;
    } else if strcmp(value, c"1".as_ptr()) == 0 as ::core::ffi::c_int
        || strcasecmp(value, c"on".as_ptr()) == 0 as ::core::ffi::c_int
        || strcasecmp(value, c"yes".as_ptr()) == 0 as ::core::ffi::c_int
    {
        flag = 1 as ::core::ffi::c_int;
    } else if strcmp(value, c"0".as_ptr()) == 0 as ::core::ffi::c_int
        || strcasecmp(value, c"off".as_ptr()) == 0 as ::core::ffi::c_int
        || strcasecmp(value, c"no".as_ptr()) == 0 as ::core::ffi::c_int
    {
        flag = 0 as ::core::ffi::c_int;
    } else {
        format_options_cause!(cause, c"bad value: %s", CStr::from_ptr(value),);
        return -(1 as ::core::ffi::c_int);
    }
    options_set_number(oo, name, flag as ::core::ffi::c_longlong);
    0 as ::core::ffi::c_int
}
pub unsafe fn options_find_choice(
    oe: &options_table_entry,
    value: &CStr,
    mut cause: *mut Option<CString>,
) -> ::core::ffi::c_int {
    let mut choice: ::core::ffi::c_int = -1;
    for (n, candidate) in oe.choices.iter().enumerate() {
        if *candidate == value {
            choice = n as ::core::ffi::c_int;
        }
    }
    if choice == -(1 as ::core::ffi::c_int) {
        format_options_cause!(cause, c"unknown value: %s", value,);
        return -(1 as ::core::ffi::c_int);
    }
    choice
}
unsafe fn options_from_string_choice(
    oe: &options_table_entry,
    oo: &mut options,
    name: &CStr,
    value: Option<&CStr>,
    mut cause: *mut Option<CString>,
) -> ::core::ffi::c_int {
    let value: *const ::core::ffi::c_char = value.map_or(std::ptr::null(), CStr::as_ptr);
    let mut choice: ::core::ffi::c_int = -(1 as ::core::ffi::c_int);
    if value.is_null() {
        choice = options_get_number(oo, name) as ::core::ffi::c_int;
        if choice < 2 as ::core::ffi::c_int {
            choice = (choice == 0) as ::core::ffi::c_int;
        }
    } else {
        choice = options_find_choice(oe, CStr::from_ptr(value), cause);
        if choice < 0 as ::core::ffi::c_int {
            return -(1 as ::core::ffi::c_int);
        }
    }
    options_set_number(oo, name, choice as ::core::ffi::c_longlong);
    0 as ::core::ffi::c_int
}
pub unsafe fn options_from_string(
    oo: &mut options,
    oe: &options_table_entry,
    name: &CStr,
    value: Option<&CStr>,
    mut append: ::core::ffi::c_int,
    mut cause: *mut Option<CString>,
) -> ::core::ffi::c_int {
    let value: *const ::core::ffi::c_char = value.map_or(std::ptr::null(), CStr::as_ptr);
    let mut type_0: options_table_type = OPTIONS_TABLE_STRING;
    let mut number: ::core::ffi::c_longlong = 0;
    let _new: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    let mut key: key_code = 0;
    let mut pr: cmd_parse_result = cmd_parse_result::empty();
    if value.is_null() && oe.type_0 != OPTIONS_TABLE_FLAG && oe.type_0 != OPTIONS_TABLE_CHOICE {
        format_options_cause!(cause, c"empty value",);
        return -(1 as ::core::ffi::c_int);
    }
    type_0 = oe.type_0;
    match type_0 as ::core::ffi::c_uint {
        0 => {
            let old = options_get_string(oo, name);
            options_set_string(oo, name, append, |out| {
                write_cstr(out, nullable_cstr(value))
            });
            let new = options_get_string(oo, name);
            if options_from_string_check(oe, &new, cause) != 0 as ::core::ffi::c_int {
                options_set_string(oo, name, 0 as ::core::ffi::c_int, |out| {
                    write_cstr(out, &*old)
                });
                return -(1 as ::core::ffi::c_int);
            }
            return 0 as ::core::ffi::c_int;
        }
        1 => {
            let parsed = strtonum(
                CStr::from_ptr(value),
                oe.minimum as ::core::ffi::c_longlong,
                oe.maximum as ::core::ffi::c_longlong,
            );
            number = parsed.unwrap_or(0);
            if let Err(errstr) = parsed {
                format_options_cause!(cause, c"value is %s: %s", errstr, CStr::from_ptr(value),);
                return -(1 as ::core::ffi::c_int);
            }
            options_set_number(oo, name, number);
            return 0 as ::core::ffi::c_int;
        }
        2 => {
            key = key_string_parse_cstr(std::ffi::CStr::from_ptr(value)).unwrap_or(KEYC_UNKNOWN);
            if key == KEYC_UNKNOWN as ::core::ffi::c_ulong as key_code {
                format_options_cause!(cause, c"bad key: %s", CStr::from_ptr(value),);
                return -(1 as ::core::ffi::c_int);
            }
            options_set_number(oo, name, key as ::core::ffi::c_longlong);
            return 0 as ::core::ffi::c_int;
        }
        3 => {
            number = colour_parse_cstr(std::ffi::CStr::from_ptr(value)).unwrap_or(-1)
                as ::core::ffi::c_longlong;
            if number == -(1 as ::core::ffi::c_int) as ::core::ffi::c_longlong {
                format_options_cause!(cause, c"bad colour: %s", CStr::from_ptr(value),);
                return -(1 as ::core::ffi::c_int);
            }
            options_set_number(oo, name, number);
            return 0 as ::core::ffi::c_int;
        }
        4 => return options_from_string_flag(oo, name, nullable_cstr(value), cause),
        5 => return options_from_string_choice(oe, oo, name, nullable_cstr(value), cause),
        6 => {
            pr = cmd_parse_from_string(
                CStr::from_ptr(value),
                ::core::ptr::null_mut::<cmd_parse_input>(),
            );
            match pr.status as ::core::ffi::c_uint {
                0 => {
                    if !cause.is_null() {
                        *cause = pr.error;
                    }
                    return -(1 as ::core::ffi::c_int);
                }
                1 => {
                    options_set_command(oo, name, pr.take_cmdlist());
                    return 0 as ::core::ffi::c_int;
                }
                _ => {}
            }
        }
        _ => {}
    }
    -(1 as ::core::ffi::c_int)
}
pub unsafe fn options_push_changes(name: &CStr) {
    let name: *const ::core::ffi::c_char = name.as_ptr();
    let mut loop_0: Option<ClientRef> = None;
    let mut s: Option<SessionRef> = None;
    log_debug(format_args!(
        "{}: {}",
        "options_push_changes",
        log_cstr(CStr::from_ptr(name))
    ));
    if strcmp(name, c"theme".as_ptr()) == 0 as ::core::ffi::c_int
        || strncmp(name, c"dark-theme-".as_ptr(), 11 as size_t) == 0 as ::core::ffi::c_int
        || strncmp(name, c"light-theme-".as_ptr(), 12 as size_t) == 0 as ::core::ffi::c_int
    {
        let mut registry_loop_0_owner = ClientRegistry::first();
        loop_0 = registry_loop_0_owner.clone();
        while !loop_0.is_none() {
            if let Some(client) = loop_0.as_ref() {
                client.update_theme_colours();
            };
            {
                let terminal = &(loop_0.as_ref().expect("live client"));
                if {
                    let tty_state = terminal.borrow_terminal();
                    tty_state.flags
                } & TTY_OPENED
                    != 0
                {
                    tty_invalidate(terminal);
                }
            };
            server_redraw_client(loop_0.as_ref().expect("live client"));
            registry_loop_0_owner = ClientRegistry::next(
                registry_loop_0_owner
                    .as_ref()
                    .expect("current registry client"),
            );
            loop_0 = registry_loop_0_owner.clone();
        }
    }
    if strcmp(name, c"automatic-rename".as_ptr()) == 0 as ::core::ffi::c_int {
        let mut window_cursor = windows::first();
        while let Some(window_owner) = window_cursor.take() {
            if let Some(active) = window_owner.active_pane().upgrade() {
                if options_get_number(window_owner.borrow_options_mut(), c"automatic-rename") != 0 {
                    active.mark_changed();
                }
            }
            window_cursor = window_owner.next_window().upgrade();
            drop(window_owner);
        }
    }
    if strcmp(name, c"cursor-colour".as_ptr()) == 0 as ::core::ffi::c_int {
        for pane_owner in crate::src::shared::pane::window_pane_tree::snapshot() {
            pane_owner.reset_default_cursor();
        }
    }
    if strcmp(name, c"cursor-style".as_ptr()) == 0 as ::core::ffi::c_int {
        for pane_owner in crate::src::shared::pane::window_pane_tree::snapshot() {
            pane_owner.reset_default_cursor();
        }
    }
    if strcmp(name, c"fill-character".as_ptr()) == 0 as ::core::ffi::c_int {
        let mut window_cursor = windows::first();
        while let Some(window_owner) = window_cursor.take() {
            window_owner.refresh_fill_cells();
            window_cursor = window_owner.next_window().upgrade();
            drop(window_owner);
        }
    }
    if strcmp(name, c"key-table".as_ptr()) == 0 as ::core::ffi::c_int {
        let mut registry_loop_0_owner = ClientRegistry::first();
        loop_0 = registry_loop_0_owner.clone();
        while !loop_0.is_none() {
            loop_0.clone().expect("live client").set_key_table(None);
            registry_loop_0_owner = ClientRegistry::next(
                registry_loop_0_owner
                    .as_ref()
                    .expect("current registry client"),
            );
            loop_0 = registry_loop_0_owner.clone();
        }
    }
    if strcmp(name, c"utmp".as_ptr()) == 0 as ::core::ffi::c_int {
        for pane_owner in crate::src::shared::pane::window_pane_tree::snapshot() {
            pane_owner.refresh_utmp();
        }
    }
    if strcmp(name, c"user-keys".as_ptr()) == 0 as ::core::ffi::c_int {
        let mut registry_loop_0_owner = ClientRegistry::first();
        loop_0 = registry_loop_0_owner.clone();
        while !loop_0.is_none() {
            {
                let mut terminal = loop_0.as_ref().expect("live client").borrow_terminal_mut();
                if terminal.flags & TTY_OPENED != 0 {
                    tty_keys_build(&mut *terminal);
                }
            }
            registry_loop_0_owner = ClientRegistry::next(
                registry_loop_0_owner
                    .as_ref()
                    .expect("current registry client"),
            );
            loop_0 = registry_loop_0_owner.clone();
        }
    }
    if strcmp(name, c"status".as_ptr()) == 0 as ::core::ffi::c_int
        || strcmp(name, c"status-interval".as_ptr()) == 0 as ::core::ffi::c_int
    {
        status_timer_start_all();
    }
    if strcmp(name, c"status".as_ptr()) == 0 as ::core::ffi::c_int
        || strcmp(name, c"status-position".as_ptr()) == 0 as ::core::ffi::c_int
        || strcmp(name, c"pane-border-indicators".as_ptr()) == 0 as ::core::ffi::c_int
        || strcmp(name, c"pane-border-lines".as_ptr()) == 0 as ::core::ffi::c_int
        || strcmp(name, c"pane-border-status".as_ptr()) == 0 as ::core::ffi::c_int
        || strcmp(name, c"pane-scrollbars".as_ptr()) == 0 as ::core::ffi::c_int
        || strcmp(name, c"pane-scrollbars-timeout".as_ptr()) == 0 as ::core::ffi::c_int
        || strcmp(name, c"pane-scrollbars-position".as_ptr()) == 0 as ::core::ffi::c_int
        || strcmp(name, c"pane-scrollbars-style".as_ptr()) == 0 as ::core::ffi::c_int
    {
        redraw_invalidate_all_scenes();
    }
    if strcmp(name, c"monitor-silence".as_ptr()) == 0 as ::core::ffi::c_int {
        alerts_reset_all();
    }
    if strcmp(name, c"window-style".as_ptr()) == 0 as ::core::ffi::c_int
        || strcmp(name, c"window-active-style".as_ptr()) == 0 as ::core::ffi::c_int
    {
        for pane_owner in crate::src::shared::pane::window_pane_tree::snapshot() {
            pane_owner.mark_style_changed(true);
        }
    }
    if *name as ::core::ffi::c_int == '@' as i32 {
        for pane_owner in crate::src::shared::pane::window_pane_tree::snapshot() {
            pane_owner.mark_style_changed(false);
        }
    }
    if strcmp(name, c"pane-colours".as_ptr()) == 0 as ::core::ffi::c_int {
        for pane_owner in crate::src::shared::pane::window_pane_tree::snapshot() {
            pane_owner.refresh_palette();
        }
    }
    if strcmp(name, c"pane-border-status".as_ptr()) == 0 as ::core::ffi::c_int
        || strcmp(name, c"pane-scrollbars".as_ptr()) == 0 as ::core::ffi::c_int
        || strcmp(name, c"pane-scrollbars-position".as_ptr()) == 0 as ::core::ffi::c_int
    {
        let mut window_cursor = windows::first();
        while let Some(window_owner) = window_cursor.take() {
            window_owner.refresh_scrollbars();
            window_owner.refit();
            window_cursor = window_owner.next_window().upgrade();
            drop(window_owner);
        }
    }
    if strcmp(name, c"pane-scrollbars".as_ptr()) == 0 as ::core::ffi::c_int {
        for pane_owner in crate::src::shared::pane::window_pane_tree::snapshot() {
            pane_owner.hide_scrollbar();
        }
    }
    if strcmp(name, c"pane-scrollbars-style".as_ptr()) == 0 as ::core::ffi::c_int {
        for pane_owner in crate::src::shared::pane::window_pane_tree::snapshot() {
            pane_owner.refresh_scrollbar_style();
        }
        let mut window_cursor = windows::first();
        while let Some(window_owner) = window_cursor.take() {
            window_owner.refit();
            window_cursor = window_owner.next_window().upgrade();
            drop(window_owner);
        }
    }
    if strcmp(name, c"codepoint-widths".as_ptr()) == 0 as ::core::ffi::c_int {
        utf8_update_width_cache();
    }
    if strcmp(name, c"input-buffer-size".as_ptr()) == 0 as ::core::ffi::c_int {
        input_set_buffer_size(options_get_number(global_options(), c"input-buffer-size") as size_t);
    }
    if strcmp(name, c"history-limit".as_ptr()) == 0 as ::core::ffi::c_int {
        let mut s_owner = sessions::first();
        s = s_owner.clone();
        while !s.is_none() {
            s.as_ref().expect("live session").update_history();
            s_owner = sessions::after(s.as_ref().expect("live session").name().as_c_str());
            s = s_owner.clone();
        }
    }
    recalculate_sizes();
    let mut registry_loop_0_owner = ClientRegistry::first();
    loop_0 = registry_loop_0_owner.clone();
    while !loop_0.is_none() {
        if !loop_0
            .as_ref()
            .expect("live client")
            .attached_session()
            .upgrade()
            .is_none()
        {
            server_redraw_client(loop_0.as_ref().expect("live client"));
        }
        registry_loop_0_owner = ClientRegistry::next(
            registry_loop_0_owner
                .as_ref()
                .expect("current registry client"),
        );
        loop_0 = registry_loop_0_owner.clone();
    }
}

#[cfg(test)]
mod array_string_owner_tests {
    use super::*;

    #[test]
    fn formatted_diagnostic_preserves_non_utf8_arguments() {
        let message = options_string_cause(c"value is %s: %s", &[c"invalid", c"\xff"]);
        assert_eq!(message.to_bytes(), b"value is invalid: \xff");
    }
}
