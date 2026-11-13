//! Authoritative control character declarations.

pub const C0_US: control_character_code = 31;

pub const C0_RS: control_character_code = 30;

pub const C0_GS: control_character_code = 29;

pub const C0_FS: control_character_code = 28;

pub const C0_ESC: control_character_code = 27;

pub const C0_SUB: control_character_code = 26;

pub const C0_EM: control_character_code = 25;

pub const C0_CAN: control_character_code = 24;

pub const C0_ETB: control_character_code = 23;

pub const C0_SYN: control_character_code = 22;

pub const C0_NAK: control_character_code = 21;

pub const C0_DC4: control_character_code = 20;

pub const C0_DC3: control_character_code = 19;

pub const C0_DC2: control_character_code = 18;

pub const C0_DC1: control_character_code = 17;

pub const C0_DLE: control_character_code = 16;

pub const C0_SI: control_character_code = 15;

pub const C0_SO: control_character_code = 14;

pub const C0_CR: control_character_code = 13;

pub const C0_FF: control_character_code = 12;

pub const C0_VT: control_character_code = 11;

pub const C0_LF: control_character_code = 10;

pub const C0_HT: control_character_code = 9;

pub const C0_BS: control_character_code = 8;

pub const C0_BEL: control_character_code = 7;

pub const C0_ASC: control_character_code = 6;

pub const C0_ENQ: control_character_code = 5;

pub const C0_EOT: control_character_code = 4;

pub const C0_ETX: control_character_code = 3;

pub const C0_STX: control_character_code = 2;

pub const C0_SOH: control_character_code = 1;

pub const C0_NUL: control_character_code = 0;

pub type control_character_code = ::core::ffi::c_uint;
