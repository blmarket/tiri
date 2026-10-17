//! Authoritative option-table value domains.

use std::collections::BTreeMap;
use std::ffi::CString;

use super::abi::{time_t, u_int};
use super::command::cmd_list;
use super::style::style;
pub type options_table_type = ::core::ffi::c_uint;
pub const OPTIONS_TABLE_NUMBER: options_table_type = 1;
pub const OPTIONS_TABLE_KEY: options_table_type = 2;
pub const OPTIONS_TABLE_COLOUR: options_table_type = 3;
pub const OPTIONS_TABLE_FLAG: options_table_type = 4;
pub const OPTIONS_TABLE_CHOICE: options_table_type = 5;
pub const OPTIONS_TABLE_COMMAND: options_table_type = 6;
pub const OPTIONS_TABLE_STRING: options_table_type = 0;

pub const OPTIONS_TABLE_NONE: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
pub const OPTIONS_TABLE_WINDOW: ::core::ffi::c_int = 0x4 as ::core::ffi::c_int;
pub const OPTIONS_TABLE_IS_HOOK: ::core::ffi::c_int = 0x2 as ::core::ffi::c_int;
pub const OPTIONS_TABLE_SERVER: ::core::ffi::c_int = 0x1 as ::core::ffi::c_int;
pub const OPTIONS_TABLE_SESSION: ::core::ffi::c_int = 0x2 as ::core::ffi::c_int;
pub const OPTIONS_TABLE_PANE: ::core::ffi::c_int = 0x8 as ::core::ffi::c_int;
pub const OPTIONS_TABLE_IS_ARRAY: ::core::ffi::c_int = 0x1 as ::core::ffi::c_int;
pub const OPTIONS_TABLE_IS_STYLE: ::core::ffi::c_int = 0x4 as ::core::ffi::c_int;
pub const OPTIONS_TABLE_IS_COLOUR: ::core::ffi::c_int = 0x8 as ::core::ffi::c_int;
pub const OPTIONS_TABLE_STATUS_FORMAT1: &std::ffi::CStr = c"#[align=left range=left #{E:status-left-style}]#[push-default]#{T;=/#{status-left-length}:status-left}#[pop-default]#[norange default]#[list=on align=#{status-justify}]#[list=left-marker]<#[list=right-marker]>#[list=on]#{W:#[range=window|#{window_index} #{E:window-status-style}#{?#{&&:#{window_last_flag},#{!=:#{E:window-status-last-style},default}}, #{E:window-status-last-style},}#{?#{&&:#{window_bell_flag},#{!=:#{E:window-status-bell-style},default}}, #{E:window-status-bell-style},#{?#{&&:#{||:#{window_activity_flag},#{window_silence_flag}},#{!=:#{E:window-status-activity-style},default}}, #{E:window-status-activity-style},}}]#[push-default]#{T:window-status-format}#[pop-default]#[norange default]#{?loop_last_flag,,#{E:window-status-separator}},#[range=window|#{window_index} list=focus #{?#{!=:#{E:window-status-current-style},default},#{E:window-status-current-style},#{E:window-status-style}}#{?#{&&:#{window_last_flag},#{!=:#{E:window-status-last-style},default}}, #{E:window-status-last-style},}#{?#{&&:#{window_bell_flag},#{!=:#{E:window-status-bell-style},default}}, #{E:window-status-bell-style},#{?#{&&:#{||:#{window_activity_flag},#{window_silence_flag}},#{!=:#{E:window-status-activity-style},default}}, #{E:window-status-activity-style},}}]#[push-default]#{T:window-status-current-format}#[pop-default]#[norange list=on default]#{?loop_last_flag,,#{E:window-status-separator}}}#[nolist align=right range=right #{E:status-right-style}]#[push-default]#{T;=/#{status-right-length}:status-right}#[pop-default]#[norange default]";
pub const OPTIONS_TABLE_STATUS_FORMAT2: &std::ffi::CStr = c"#[align=left]#{R: ,#{e|-:#{w;T;=/#{status-left-length}:status-left},3}}P: #[norange default]#[list=on align=#{status-justify}]#[list=left-marker]<#[list=right-marker]>#[list=on]#{P:#[range=pane|#{pane_id} #{E:pane-status-style}]#[push-default]#{T:window-pane-status-format}#[pop-default]#[norange list=on default]#{?loop_last_flag,,#{E:window-status-separator}},#[range=pane|#{pane_id} list=focus #{?#{!=:#{E:pane-status-current-style},default},#{E:pane-status-current-style},#{E:pane-status-style}}]#[push-default]#{T:window-pane-current-status-format}#[pop-default]#[norange list=on default]#{?loop_last_flag,,#{E:window-status-separator}}}";
pub const OPTIONS_TABLE_STATUS_FORMAT3: &std::ffi::CStr = c"#[align=left]#{R: ,#{e|-:#{w;T;=/#{status-left-length}:status-left},3}}S: #[norange default]#[list=on align=#{status-justify}]#[list=left-marker]<#[list=right-marker]>#[list=on]#{S:#[range=session|#{session_id} #{E:session-status-style}]#[push-default]#S#{q/h:session_alert}#[pop-default]#[norange list=on default]  ,#[range=session|#{session_id} list=focus #{?#{!=:#{E:session-status-current-style},default},#{E:session-status-current-style},#{E:session-status-style}}]#[push-default]#S*#{q/h:session_alert}#[pop-default]#[norange list=on default] }";

#[cfg(test)]
mod tests {
    use super::*;
    use ::core::mem::{align_of, size_of};

    #[test]
    fn option_table_domain_matches_translated_c_baseline() {
        assert_eq!(size_of::<options_table_type>(), 4);
        assert_eq!(align_of::<options_table_type>(), 4);
        assert_eq!(OPTIONS_TABLE_STRING, 0);
        assert_eq!(OPTIONS_TABLE_COMMAND, 6);
    }
}

/// Box-owned by options_create; inheritance carries the parent's owner identity.
#[repr(C)]
pub struct options {
    pub(crate) tree: BTreeMap<CString, Box<options_entry>>,
    pub parent: Option<crate::src::options::OptionsScope>,
}

/// Box-owned by options_array_new; returned key and value pointers expire when
/// the item is removed.
#[repr(C)]
pub struct options_array_item {
    pub key: CString,
    pub value: options_value,
}

#[repr(C)]
pub struct options_entry {
    pub name: ::std::ffi::CString,
    pub tableentry: Option<&'static options_table_entry>,
    pub value: options_value,
    pub cached: ::core::ffi::c_int,
    pub style: style,
    pub monitor_data: Option<Box<crate::src::hooks::hooks_monitor>>,
    pub fire_count: u_int,
    pub fire_time: time_t,
}

impl options_entry {
    pub fn id(&self) -> u64 {
        self as *const Self as u64
    }
}
impl options_array_item {
    pub fn id(&self) -> u64 {
        self as *const Self as u64
    }
}

pub struct OptionCommand(pub Option<std::rc::Rc<std::cell::RefCell<cmd_list>>>);

/// The stored value owns its payload. Accessors expose borrowed pointers to
/// callers only while the containing option record remains alive.
pub enum options_value {
    Empty,
    String(CString),
    Number(::core::ffi::c_longlong),
    Array(options_array_storage),
    Command(OptionCommand),
}

impl options_value {
    pub fn string_ptr(&self) -> Option<&std::ffi::CStr> {
        match self {
            Self::String(value) => Some(value.as_c_str()),
            Self::Empty => None,
            _ => panic!("option value is not a string"),
        }
    }

    pub fn number(&self) -> ::core::ffi::c_longlong {
        match self {
            Self::Number(value) => *value,
            _ => panic!("option value is not a number"),
        }
    }

    pub fn commands(&self) -> Option<&std::rc::Rc<std::cell::RefCell<cmd_list>>> {
        match self {
            Self::Command(value) => value.0.as_ref(),
            Self::Empty => None,
            _ => panic!("option value is not a command"),
        }
    }

    pub fn array_storage(&mut self) -> &mut options_array_storage {
        match self {
            Self::Array(value) => value,
            _ => panic!("option value is not an array"),
        }
    }
}

#[derive(Copy, Clone, Default)]
#[repr(C)]
pub struct options_table_entry {
    pub name: Option<&'static std::ffi::CStr>,
    pub alternative_name: Option<&'static std::ffi::CStr>,
    pub type_0: options_table_type,
    pub scope: ::core::ffi::c_int,
    pub flags: ::core::ffi::c_int,
    pub minimum: u_int,
    pub maximum: u_int,
    pub choices: &'static [&'static std::ffi::CStr],
    pub default_str: Option<&'static std::ffi::CStr>,
    pub default_num: ::core::ffi::c_longlong,
    pub default_arr: Option<&'static [&'static std::ffi::CStr]>,
    pub separator: Option<&'static std::ffi::CStr>,
    pub pattern: Option<&'static std::ffi::CStr>,
    pub text: Option<&'static std::ffi::CStr>,
    pub unit: Option<&'static std::ffi::CStr>,
}

/// Numeric indices precede bytewise-ordered text keys.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum OptionsArrayKey {
    Numeric(u32),
    Text(Vec<u8>),
}

/// Sole owner of boxed array items and their values.
#[derive(Default)]
pub struct options_array_storage {
    pub(crate) entries: BTreeMap<OptionsArrayKey, Box<options_array_item>>,
}

impl options_entry {
    pub fn tableentry_ptr(&self) -> Option<&'static options_table_entry> {
        self.tableentry
    }
}
