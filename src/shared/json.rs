#![forbid(unsafe_code)]

//! Authoritative JSON model declarations.

use super::abi::int64_t;
use std::collections::BTreeMap;
use std::ffi::CString;

/// Nodes stay separately allocated; containers own their children for the
/// lifetime of the parsed document. Traversal borrows the owning container.
#[derive(Debug)]
pub struct json_node {
    pub value: JsonValue,
}

#[derive(Debug)]
pub enum JsonValue {
    String(CString),
    Number(int64_t),
    Boolean(::core::ffi::c_int),
    Object(json_fields),
    Array(json_members_storage),
}

#[derive(Debug, Default)]
pub struct json_members_storage {
    pub(crate) members: Vec<json_node>,
}

#[derive(Debug, Default)]
pub struct json_fields {
    pub(crate) entries: BTreeMap<std::ffi::CString, Box<json_node>>,
}
