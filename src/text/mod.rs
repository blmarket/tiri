//! UTF-8 decoding, encoding, display width, and combined-character storage,
//! plus the C numeric parsing rules they and the compat parsers share.

pub mod base64;
pub(crate) mod numbers;
pub mod utf8;
mod utf8_cache;
pub mod utf8_combined;
pub mod utf8_decode;
mod utf8_width;
