//! Option tables, entries, and array values.
use super::diagnostic::options_string_cause;
use super::parse::{
    match_option_name, parse_array_index, parse_option_name, ArrayIndex, ArrayIndexError,
    OptionNameMatch, OptionNameMatchError,
};
use super::scope::OptionsScope;
use super::table::{options_other_names, options_table};
use super::types::*;
use crate::src::cmd::cmd_list_print_cstring;
use crate::src::cmd::parse::cmd_parse_from_string;
use crate::src::format::bytes::format_message_with;
use crate::src::format::bytes::write_cstr;
use crate::src::key_string::key_string_format;
use crate::src::log::fatalx;
use crate::src::server::current_time;

use crate::src::style::colour::{colour_format, colour_parse_cstr};

use crate::src::shared::abi::*;
use crate::src::shared::command::cmd_list;
use crate::src::shared::command::{cmd_parse_input, cmd_parse_result};
use crate::src::shared::key::*;
use crate::src::shared::limits::UINT_MAX;
use std::collections::btree_map;
use std::ffi::{CStr, CString};

fn set_scalar_string(o: &mut options_entry, value: CString) {
    o.value = options_value::String(value);
}

fn set_array_string(ov: &mut options_value, value: CString) {
    *ov = options_value::String(value);
}

impl OptionsArrayKey {
    /// Parse a key already normalized by `options_array::correct_key`.
    pub(crate) fn parse(key: &CStr) -> Self {
        match parse_array_index(key.to_bytes()).expect("validated array key") {
            ArrayIndex::Numeric(number) => Self::Numeric(number),
            ArrayIndex::Text(bytes) => Self::Text(bytes.to_vec()),
        }
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
        o.value = options_value::Array(options_array::default());
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
        let array = o.value.array_mut().expect("empty array option");
        if oe.default_arr.is_none() {
            array
                .assign(oe, oe.default_str.expect("array option default"))
                .expect("valid array option default");
            return o;
        }
        for (i, value) in oe.default_arr.expect("array defaults").iter().enumerate() {
            let key = format_message_with(|out| write!(out, "{}", i as u32));
            array
                .set(oe, &key, Some(*value), false)
                .expect("valid array option default");
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
pub fn options_get_monitor_data(o: &options_entry) -> Option<&crate::src::hooks::hooks_monitor> {
    o.monitor_data.as_deref()
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
impl options_array {
    pub(crate) fn correct_key(key: &CStr) -> Option<CString> {
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

    pub fn get(&self, key: &CStr) -> Option<&options_value> {
        let key = Self::correct_key(key)?;
        self.entries
            .get(&OptionsArrayKey::parse(&key))
            .map(|item| &item.value)
    }

    pub fn get_mut(&mut self, key: &CStr) -> Option<&mut options_value> {
        let key = Self::correct_key(key)?;
        self.entries
            .get_mut(&OptionsArrayKey::parse(&key))
            .map(|item| &mut item.value)
    }

    /// Look up a value by its decimal numeric index.
    pub fn get_index(&self, index: u_int) -> Option<&options_value> {
        let key = CString::new(index.to_string()).expect("decimal index contains no NUL");
        self.get(&key)
    }

    pub fn get_index_mut(&mut self, index: u_int) -> Option<&mut options_value> {
        let key = CString::new(index.to_string()).expect("decimal index contains no NUL");
        self.get_mut(&key)
    }

    /// The item at a normalized key, inserting an empty one if absent.
    pub(super) fn get_or_insert(&mut self, key: &CStr) -> &mut options_value {
        &mut self
            .entries
            .entry(OptionsArrayKey::parse(key))
            .or_insert_with(|| {
                Box::new(options_array_item {
                    key: key.to_owned(),
                    value: options_value::Empty,
                })
            })
            .value
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &options_array_item> {
        self.entries.values().map(Box::as_ref)
    }

    /// The lowest unused numeric index, or UINT_MAX when every index is used.
    pub(super) fn next_free(&self) -> u_int {
        (0..UINT_MAX)
            .find(|&i| self.get_index(i).is_none())
            .unwrap_or(UINT_MAX)
    }

    /// Set or, with no value, remove one item of an array defined by `oe`.
    pub unsafe fn set(
        &mut self,
        oe: &options_table_entry,
        key: &CStr,
        value: Option<&CStr>,
        append: bool,
    ) -> Result<(), CString> {
        let Some(new_key) = Self::correct_key(key) else {
            return Err(options_string_cause(c"bad array key: %s", &[key]));
        };
        let Some(value) = value else {
            self.entries.remove(&OptionsArrayKey::parse(&new_key));
            return Ok(());
        };
        if oe.type_0 == OPTIONS_TABLE_COMMAND {
            let mut pr = cmd_parse_from_string(value, ::core::ptr::null_mut::<cmd_parse_input>());
            if pr.status as ::core::ffi::c_uint == 0 {
                return Err(pr.error.expect("command parse error"));
            }
            *self.get_or_insert(&new_key) =
                options_value::Command(OptionCommand(pr.cmdlist.take()));
            return Ok(());
        }
        if oe.type_0 == OPTIONS_TABLE_STRING {
            let owned_value = if let Some(ov) = self.get(&new_key).filter(|_| append) {
                let previous = ov.string_ptr().expect("string option").to_bytes();
                let suffix = value.to_bytes();
                let mut bytes = Vec::with_capacity(previous.len() + suffix.len());
                bytes.extend_from_slice(previous);
                bytes.extend_from_slice(suffix);
                CString::new(bytes).expect("C string parts contain no NUL")
            } else {
                value.to_owned()
            };
            set_array_string(self.get_or_insert(&new_key), owned_value);
            return Ok(());
        }
        if oe.type_0 == OPTIONS_TABLE_COLOUR {
            let Some(number) = colour_parse_cstr(value) else {
                return Err(options_string_cause(c"bad colour: %s", &[value]));
            };
            *self.get_or_insert(&new_key) =
                options_value::Number(number as ::core::ffi::c_longlong);
            return Ok(());
        }
        Err(c"wrong array type".to_owned())
    }

    /// Append `s`, split by the separator of `oe`, at the next free indices.
    pub unsafe fn assign(&mut self, oe: &options_table_entry, s: &CStr) -> Result<(), CString> {
        let separator = oe.separator.unwrap_or(c" ,").to_bytes();
        if s.is_empty() {
            return Ok(());
        }
        if separator.is_empty() {
            let key = format_message_with(|out| write!(out, "{}", self.next_free()));
            return self.set(oe, &key, Some(s), false);
        }
        for token in s
            .to_bytes()
            .split(|b| separator.contains(b))
            .filter(|token| !token.is_empty())
        {
            let i = self.next_free();
            if i == UINT_MAX {
                break;
            }
            let key = format_message_with(|out| write!(out, "{}", i));
            let token = CString::new(token).expect("substring of CStr contains no NUL");
            self.set(oe, &key, Some(&token), false)?;
        }
        Ok(())
    }
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
            let values = entry
                .value
                .array()
                .into_iter()
                .flat_map(options_array::iter)
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
        return entry
            .value
            .array()
            .and_then(|array| array.get(CStr::from_ptr(key)))
            .map_or_else(
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
