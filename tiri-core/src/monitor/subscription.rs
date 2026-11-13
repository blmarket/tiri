use super::{monitor_set, monitor_type};
use crate::events::events_remove_sink;
use crate::shared::events::EventSinkId;
use std::ffi::CString;
#[repr(C)]
pub struct MonitorSubscription {
    pub generation: usize,
    pub set: Option<refbox::RefBox<monitor_set>>,
    pub sink: EventSinkId,
    pub type_0: monitor_type,
    pub id: ::core::ffi::c_int,
    pub format: CString,
}

impl Drop for MonitorSubscription {
    fn drop(&mut self) {
        events_remove_sink(self.sink);
        if let Some(set) = self.set.take() {
            crate::monitor::monitor_destroy(set);
        }
    }
}

use super::{monitor_get_fire_count, monitor_get_fire_time};
use crate::options::{options_entry, options_get_monitor_data};
use crate::shared::abi::*;
use std::ffi::CStr;
pub fn subscription_text(name: &CStr, o: &options_entry) -> Option<CString> {
    let mut bytes = name.to_bytes().to_vec();
    let hm = options_get_monitor_data(o)?;
    let target = match hm.type_0 {
        0 => b"::".as_slice(),
        1 => b":%".as_slice(),
        2 => b":%*:".as_slice(),
        3 => b":@".as_slice(),
        4 => b":@*:".as_slice(),
        _ => return None,
    };
    bytes.extend_from_slice(target);
    if matches!(hm.type_0, 1 | 3) {
        bytes.extend_from_slice(hm.id.to_string().as_bytes());
        bytes.push(b':');
    }
    bytes.extend_from_slice(hm.format.as_bytes());
    Some(CString::new(bytes).expect("C string parts contain no NUL"))
}
pub fn subscription_fire_count(name: &CStr, o: &options_entry) -> u_int {
    let Some(hm) = options_get_monitor_data(o) else {
        return 0 as u_int;
    };
    monitor_get_fire_count(&hm.set.as_ref().expect("hook monitor").downgrade(), name)
}
pub fn subscription_fire_time(name: &CStr, o: &options_entry) -> time_t {
    let Some(hm) = options_get_monitor_data(o) else {
        return 0 as time_t;
    };
    monitor_get_fire_time(&hm.set.as_ref().expect("hook monitor").downgrade(), name)
}
