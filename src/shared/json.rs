#![forbid(unsafe_code)]

//! Authoritative JSON model declarations.

use super::abi::int64_t;
use std::collections::BTreeMap;
use std::ffi::CString;

/// Nodes stay separately allocated; containers own their children for the
/// lifetime of the parsed document. Traversal borrows the owning container.
#[derive(Debug)]
pub struct json_node {
    pub key: Option<CString>,
    pub value: JsonValue,
}

impl json_node {
    pub fn type_0(&self) -> json_node_type {
        self.value.kind()
    }
}

#[derive(Debug)]
pub enum JsonValue {
    String(CString),
    Number(int64_t),
    Boolean(::core::ffi::c_int),
    Object(json_fields),
    Array(json_members_storage),
}

impl JsonValue {
    pub fn kind(&self) -> json_node_type {
        match self {
            Self::String(_) => 0,
            Self::Number(_) => 1,
            Self::Boolean(_) => 2,
            Self::Object(_) => 3,
            Self::Array(_) => 4,
        }
    }
}

#[derive(Debug, Default)]
pub struct json_members_storage {
    pub(crate) members: Vec<json_node>,
}

#[derive(Debug, Default)]
pub struct json_fields {
    pub(crate) entries: BTreeMap<Vec<u8>, Box<json_node>>,
}

pub type json_node_type = ::core::ffi::c_uint;
