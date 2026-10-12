//! Authoritative declarations shared by the translated C modules.
//!
//! Anonymous header types are named by their owning fields, never by their
//! translation-unit-local C2Rust suffix. Implementation-only types stay local.
//! Frozen fixtures verify the translated C ABI before and after migration.

pub mod abi;
pub mod account;
pub mod alerts;
pub mod arguments;
pub mod borders;
pub mod client;
pub mod colour;
pub mod command;
pub mod control;
pub mod control_character;
pub mod ctype;
pub mod display;
pub mod environment;
pub mod errno;
pub mod event;
pub mod events;
pub mod format;
pub mod grid;
pub mod hyperlinks;
pub mod input;
pub mod job;
pub mod json;
pub mod key;
pub mod layout;
pub mod limits;
pub mod menu;
pub mod mode_tree;
pub mod monitor;
pub mod mouse;
pub mod options;
pub mod pane;
pub mod paste;
pub mod popup;
pub mod posix_io;
pub mod posix_terminal;
pub mod process;
pub mod prompt;
pub mod rc;
pub mod redraw;
pub mod screen;
pub mod screen_write;
pub mod server_acl;
pub mod session;
pub mod signal;
pub mod socket;
pub mod sort;
pub mod spawn;
pub mod status;
pub mod stdio;
pub mod style;
pub mod terminal;
pub mod time;
pub mod tree;
pub mod tty;
pub mod utf8;
pub mod vis;
pub mod window;
