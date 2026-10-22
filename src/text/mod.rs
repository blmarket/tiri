//! UTF-8 decoding, encoding, display width, and combined-character storage,
//! plus the C numeric parsing and substring search rules shared across the crate.

pub mod base64;
pub(crate) mod numbers;
pub(crate) mod search;
pub mod utf8;
mod utf8_cache;
pub mod utf8_combined;
pub mod utf8_decode;
mod utf8_width;
