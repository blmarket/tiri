//! Foreign declarations supplied by systemd.

use crate::shared::abi::{pid_t, size_t, uint64_t, uint8_t};
#[derive(Copy, Clone)]
#[repr(C)]
pub struct sd_bus_error {
    pub name: *const ::core::ffi::c_char,
    pub message: *const ::core::ffi::c_char,
    pub _need_free: ::core::ffi::c_int,
}

pub type sd_bus_message_handler_t = Option<
    unsafe extern "C" fn(
        *mut sd_bus_message,
        *mut ::core::ffi::c_void,
        *mut sd_bus_error,
    ) -> ::core::ffi::c_int,
>;

#[derive(Copy, Clone)]
#[repr(C)]
pub union sd_id128 {
    pub bytes: [uint8_t; 16],
    pub qwords: [uint64_t; 2],
}

pub type sd_id128_t = sd_id128;

extern "C" {
    pub type sd_bus;
    pub fn sd_bus_call(
        bus: *mut sd_bus,
        m: *mut sd_bus_message,
        usec: uint64_t,
        reterr_error: *mut sd_bus_error,
        ret_reply: *mut *mut sd_bus_message,
    ) -> ::core::ffi::c_int;
    pub fn sd_bus_default_user(ret: *mut *mut sd_bus) -> ::core::ffi::c_int;
    pub fn sd_bus_error_free(e: *mut sd_bus_error);
    pub fn sd_bus_match_signal(
        bus: *mut sd_bus,
        ret: *mut *mut sd_bus_slot,
        sender: *const ::core::ffi::c_char,
        path: *const ::core::ffi::c_char,
        interface: *const ::core::ffi::c_char,
        member: *const ::core::ffi::c_char,
        callback: sd_bus_message_handler_t,
        userdata: *mut ::core::ffi::c_void,
    ) -> ::core::ffi::c_int;
    pub type sd_bus_message;
    pub fn sd_bus_message_append(
        m: *mut sd_bus_message,
        types: *const ::core::ffi::c_char,
        ...
    ) -> ::core::ffi::c_int;
    pub fn sd_bus_message_close_container(m: *mut sd_bus_message) -> ::core::ffi::c_int;
    pub fn sd_bus_message_new_method_call(
        bus: *mut sd_bus,
        ret: *mut *mut sd_bus_message,
        destination: *const ::core::ffi::c_char,
        path: *const ::core::ffi::c_char,
        interface: *const ::core::ffi::c_char,
        member: *const ::core::ffi::c_char,
    ) -> ::core::ffi::c_int;
    pub fn sd_bus_message_open_container(
        m: *mut sd_bus_message,
        type_0: ::core::ffi::c_char,
        contents: *const ::core::ffi::c_char,
    ) -> ::core::ffi::c_int;
    pub fn sd_bus_message_read(
        m: *mut sd_bus_message,
        types: *const ::core::ffi::c_char,
        ...
    ) -> ::core::ffi::c_int;
    pub fn sd_bus_message_unref(p: *mut sd_bus_message) -> *mut sd_bus_message;
    pub fn sd_bus_process(bus: *mut sd_bus, ret: *mut *mut sd_bus_message) -> ::core::ffi::c_int;
    pub type sd_bus_slot;
    pub fn sd_bus_slot_unref(p: *mut sd_bus_slot) -> *mut sd_bus_slot;
    pub fn sd_bus_unref(p: *mut sd_bus) -> *mut sd_bus;
    pub fn sd_bus_wait(bus: *mut sd_bus, timeout_usec: uint64_t) -> ::core::ffi::c_int;
    pub fn sd_id128_randomize(ret: *mut sd_id128_t) -> ::core::ffi::c_int;
    pub fn sd_is_socket_unix(
        fd: ::core::ffi::c_int,
        type_0: ::core::ffi::c_int,
        listening: ::core::ffi::c_int,
        path: *const ::core::ffi::c_char,
        length: size_t,
    ) -> ::core::ffi::c_int;
    pub fn sd_listen_fds(unset_environment: ::core::ffi::c_int) -> ::core::ffi::c_int;
    pub fn sd_pid_get_unit(
        pid: pid_t,
        ret_unit: *mut *mut ::core::ffi::c_char,
    ) -> ::core::ffi::c_int;
    pub fn sd_pid_get_user_slice(
        pid: pid_t,
        ret_slice: *mut *mut ::core::ffi::c_char,
    ) -> ::core::ffi::c_int;
    pub fn sd_pid_get_user_unit(
        pid: pid_t,
        ret_unit: *mut *mut ::core::ffi::c_char,
    ) -> ::core::ffi::c_int;
}
