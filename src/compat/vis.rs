use crate::shared::abi::*;
use crate::shared::limits::UCHAR_MAX;
use crate::shared::vis::{VIS_CSTYLE, VIS_DQ, VIS_NL, VIS_NOSLASH, VIS_OCTAL, VIS_SAFE, VIS_TAB};
use libc::memcpy;

pub const VIS_SP: ::core::ffi::c_int = 0x4 as ::core::ffi::c_int;

pub const VIS_ALL: ::core::ffi::c_int = 0x400 as ::core::ffi::c_int;

pub const VIS_GLOB: ::core::ffi::c_int = 0x100 as ::core::ffi::c_int;
pub unsafe fn vis(
    mut dst: *mut ::core::ffi::c_char,
    mut c: ::core::ffi::c_int,
    mut flag: ::core::ffi::c_int,
    mut nextc: ::core::ffi::c_int,
) -> *mut ::core::ffi::c_char {
    let mut current_block: u64;
    if (c == '\\' as i32 || flag & VIS_ALL == 0 as ::core::ffi::c_int)
        && (c as u_int <= UCHAR_MAX as u_int
            && c as u_char as ::core::ffi::c_int & !(0x7f as ::core::ffi::c_int)
                == 0 as ::core::ffi::c_int
            && (c != '*' as i32 && c != '?' as i32 && c != '[' as i32 && c != '#' as i32
                || flag & VIS_GLOB == 0 as ::core::ffi::c_int)
            && ::libc::isgraph(c as u_char as ::core::ffi::c_int) != 0
            || flag & VIS_SP == 0 as ::core::ffi::c_int && c == ' ' as i32
            || flag & VIS_TAB == 0 as ::core::ffi::c_int && c == '\t' as i32
            || flag & VIS_NL == 0 as ::core::ffi::c_int && c == '\n' as i32
            || flag & VIS_SAFE != 0
                && (c == '\u{8}' as i32
                    || c == '\u{7}' as i32
                    || c == '\r' as i32
                    || ::libc::isgraph(c as u_char as ::core::ffi::c_int) != 0))
    {
        if c == '"' as i32 && flag & VIS_DQ != 0 as ::core::ffi::c_int
            || c == '\\' as i32 && flag & VIS_NOSLASH == 0 as ::core::ffi::c_int
        {
            let fresh0 = dst;
            dst = dst.offset(1);
            *fresh0 = '\\' as i32 as ::core::ffi::c_char;
        }
        let fresh1 = dst;
        dst = dst.offset(1);
        *fresh1 = c as ::core::ffi::c_char;
        *dst = '\0' as i32 as ::core::ffi::c_char;
        return dst;
    }
    if flag & VIS_CSTYLE != 0 {
        match c {
            10 => {
                current_block = 17172381846212826752;
                match current_block {
                    17172381846212826752 => {
                        let fresh2 = dst;
                        dst = dst.offset(1);
                        *fresh2 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh3 = dst;
                        dst = dst.offset(1);
                        *fresh3 = 'n' as i32 as ::core::ffi::c_char;
                    }
                    6023546525629896002 => {
                        let fresh16 = dst;
                        dst = dst.offset(1);
                        *fresh16 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh17 = dst;
                        dst = dst.offset(1);
                        *fresh17 = 's' as i32 as ::core::ffi::c_char;
                    }
                    1901908384453561418 => {
                        let fresh14 = dst;
                        dst = dst.offset(1);
                        *fresh14 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh15 = dst;
                        dst = dst.offset(1);
                        *fresh15 = 'f' as i32 as ::core::ffi::c_char;
                    }
                    286618792822599835 => {
                        let fresh12 = dst;
                        dst = dst.offset(1);
                        *fresh12 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh13 = dst;
                        dst = dst.offset(1);
                        *fresh13 = 't' as i32 as ::core::ffi::c_char;
                    }
                    4505220464648351793 => {
                        let fresh10 = dst;
                        dst = dst.offset(1);
                        *fresh10 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh11 = dst;
                        dst = dst.offset(1);
                        *fresh11 = 'v' as i32 as ::core::ffi::c_char;
                    }
                    12286141215944697742 => {
                        let fresh8 = dst;
                        dst = dst.offset(1);
                        *fresh8 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh9 = dst;
                        dst = dst.offset(1);
                        *fresh9 = 'a' as i32 as ::core::ffi::c_char;
                    }
                    16114436840521083897 => {
                        let fresh6 = dst;
                        dst = dst.offset(1);
                        *fresh6 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh7 = dst;
                        dst = dst.offset(1);
                        *fresh7 = 'b' as i32 as ::core::ffi::c_char;
                    }
                    15896914202393627355 => {
                        let fresh4 = dst;
                        dst = dst.offset(1);
                        *fresh4 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh5 = dst;
                        dst = dst.offset(1);
                        *fresh5 = 'r' as i32 as ::core::ffi::c_char;
                    }
                    _ => {
                        let fresh18 = dst;
                        dst = dst.offset(1);
                        *fresh18 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh19 = dst;
                        dst = dst.offset(1);
                        *fresh19 = '0' as i32 as ::core::ffi::c_char;
                        if nextc as u_char as ::core::ffi::c_int >= '0' as i32
                            && nextc as u_char as ::core::ffi::c_int <= '7' as i32
                        {
                            let fresh20 = dst;
                            dst = dst.offset(1);
                            *fresh20 = '0' as i32 as ::core::ffi::c_char;
                            let fresh21 = dst;
                            dst = dst.offset(1);
                            *fresh21 = '0' as i32 as ::core::ffi::c_char;
                        }
                    }
                }
                current_block = 1609532167703825882;
            }
            13 => {
                current_block = 15896914202393627355;
                match current_block {
                    17172381846212826752 => {
                        let fresh2 = dst;
                        dst = dst.offset(1);
                        *fresh2 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh3 = dst;
                        dst = dst.offset(1);
                        *fresh3 = 'n' as i32 as ::core::ffi::c_char;
                    }
                    6023546525629896002 => {
                        let fresh16 = dst;
                        dst = dst.offset(1);
                        *fresh16 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh17 = dst;
                        dst = dst.offset(1);
                        *fresh17 = 's' as i32 as ::core::ffi::c_char;
                    }
                    1901908384453561418 => {
                        let fresh14 = dst;
                        dst = dst.offset(1);
                        *fresh14 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh15 = dst;
                        dst = dst.offset(1);
                        *fresh15 = 'f' as i32 as ::core::ffi::c_char;
                    }
                    286618792822599835 => {
                        let fresh12 = dst;
                        dst = dst.offset(1);
                        *fresh12 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh13 = dst;
                        dst = dst.offset(1);
                        *fresh13 = 't' as i32 as ::core::ffi::c_char;
                    }
                    4505220464648351793 => {
                        let fresh10 = dst;
                        dst = dst.offset(1);
                        *fresh10 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh11 = dst;
                        dst = dst.offset(1);
                        *fresh11 = 'v' as i32 as ::core::ffi::c_char;
                    }
                    12286141215944697742 => {
                        let fresh8 = dst;
                        dst = dst.offset(1);
                        *fresh8 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh9 = dst;
                        dst = dst.offset(1);
                        *fresh9 = 'a' as i32 as ::core::ffi::c_char;
                    }
                    16114436840521083897 => {
                        let fresh6 = dst;
                        dst = dst.offset(1);
                        *fresh6 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh7 = dst;
                        dst = dst.offset(1);
                        *fresh7 = 'b' as i32 as ::core::ffi::c_char;
                    }
                    15896914202393627355 => {
                        let fresh4 = dst;
                        dst = dst.offset(1);
                        *fresh4 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh5 = dst;
                        dst = dst.offset(1);
                        *fresh5 = 'r' as i32 as ::core::ffi::c_char;
                    }
                    _ => {
                        let fresh18 = dst;
                        dst = dst.offset(1);
                        *fresh18 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh19 = dst;
                        dst = dst.offset(1);
                        *fresh19 = '0' as i32 as ::core::ffi::c_char;
                        if nextc as u_char as ::core::ffi::c_int >= '0' as i32
                            && nextc as u_char as ::core::ffi::c_int <= '7' as i32
                        {
                            let fresh20 = dst;
                            dst = dst.offset(1);
                            *fresh20 = '0' as i32 as ::core::ffi::c_char;
                            let fresh21 = dst;
                            dst = dst.offset(1);
                            *fresh21 = '0' as i32 as ::core::ffi::c_char;
                        }
                    }
                }
                current_block = 1609532167703825882;
            }
            8 => {
                current_block = 16114436840521083897;
                match current_block {
                    17172381846212826752 => {
                        let fresh2 = dst;
                        dst = dst.offset(1);
                        *fresh2 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh3 = dst;
                        dst = dst.offset(1);
                        *fresh3 = 'n' as i32 as ::core::ffi::c_char;
                    }
                    6023546525629896002 => {
                        let fresh16 = dst;
                        dst = dst.offset(1);
                        *fresh16 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh17 = dst;
                        dst = dst.offset(1);
                        *fresh17 = 's' as i32 as ::core::ffi::c_char;
                    }
                    1901908384453561418 => {
                        let fresh14 = dst;
                        dst = dst.offset(1);
                        *fresh14 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh15 = dst;
                        dst = dst.offset(1);
                        *fresh15 = 'f' as i32 as ::core::ffi::c_char;
                    }
                    286618792822599835 => {
                        let fresh12 = dst;
                        dst = dst.offset(1);
                        *fresh12 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh13 = dst;
                        dst = dst.offset(1);
                        *fresh13 = 't' as i32 as ::core::ffi::c_char;
                    }
                    4505220464648351793 => {
                        let fresh10 = dst;
                        dst = dst.offset(1);
                        *fresh10 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh11 = dst;
                        dst = dst.offset(1);
                        *fresh11 = 'v' as i32 as ::core::ffi::c_char;
                    }
                    12286141215944697742 => {
                        let fresh8 = dst;
                        dst = dst.offset(1);
                        *fresh8 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh9 = dst;
                        dst = dst.offset(1);
                        *fresh9 = 'a' as i32 as ::core::ffi::c_char;
                    }
                    16114436840521083897 => {
                        let fresh6 = dst;
                        dst = dst.offset(1);
                        *fresh6 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh7 = dst;
                        dst = dst.offset(1);
                        *fresh7 = 'b' as i32 as ::core::ffi::c_char;
                    }
                    15896914202393627355 => {
                        let fresh4 = dst;
                        dst = dst.offset(1);
                        *fresh4 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh5 = dst;
                        dst = dst.offset(1);
                        *fresh5 = 'r' as i32 as ::core::ffi::c_char;
                    }
                    _ => {
                        let fresh18 = dst;
                        dst = dst.offset(1);
                        *fresh18 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh19 = dst;
                        dst = dst.offset(1);
                        *fresh19 = '0' as i32 as ::core::ffi::c_char;
                        if nextc as u_char as ::core::ffi::c_int >= '0' as i32
                            && nextc as u_char as ::core::ffi::c_int <= '7' as i32
                        {
                            let fresh20 = dst;
                            dst = dst.offset(1);
                            *fresh20 = '0' as i32 as ::core::ffi::c_char;
                            let fresh21 = dst;
                            dst = dst.offset(1);
                            *fresh21 = '0' as i32 as ::core::ffi::c_char;
                        }
                    }
                }
                current_block = 1609532167703825882;
            }
            7 => {
                current_block = 12286141215944697742;
                match current_block {
                    17172381846212826752 => {
                        let fresh2 = dst;
                        dst = dst.offset(1);
                        *fresh2 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh3 = dst;
                        dst = dst.offset(1);
                        *fresh3 = 'n' as i32 as ::core::ffi::c_char;
                    }
                    6023546525629896002 => {
                        let fresh16 = dst;
                        dst = dst.offset(1);
                        *fresh16 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh17 = dst;
                        dst = dst.offset(1);
                        *fresh17 = 's' as i32 as ::core::ffi::c_char;
                    }
                    1901908384453561418 => {
                        let fresh14 = dst;
                        dst = dst.offset(1);
                        *fresh14 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh15 = dst;
                        dst = dst.offset(1);
                        *fresh15 = 'f' as i32 as ::core::ffi::c_char;
                    }
                    286618792822599835 => {
                        let fresh12 = dst;
                        dst = dst.offset(1);
                        *fresh12 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh13 = dst;
                        dst = dst.offset(1);
                        *fresh13 = 't' as i32 as ::core::ffi::c_char;
                    }
                    4505220464648351793 => {
                        let fresh10 = dst;
                        dst = dst.offset(1);
                        *fresh10 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh11 = dst;
                        dst = dst.offset(1);
                        *fresh11 = 'v' as i32 as ::core::ffi::c_char;
                    }
                    12286141215944697742 => {
                        let fresh8 = dst;
                        dst = dst.offset(1);
                        *fresh8 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh9 = dst;
                        dst = dst.offset(1);
                        *fresh9 = 'a' as i32 as ::core::ffi::c_char;
                    }
                    16114436840521083897 => {
                        let fresh6 = dst;
                        dst = dst.offset(1);
                        *fresh6 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh7 = dst;
                        dst = dst.offset(1);
                        *fresh7 = 'b' as i32 as ::core::ffi::c_char;
                    }
                    15896914202393627355 => {
                        let fresh4 = dst;
                        dst = dst.offset(1);
                        *fresh4 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh5 = dst;
                        dst = dst.offset(1);
                        *fresh5 = 'r' as i32 as ::core::ffi::c_char;
                    }
                    _ => {
                        let fresh18 = dst;
                        dst = dst.offset(1);
                        *fresh18 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh19 = dst;
                        dst = dst.offset(1);
                        *fresh19 = '0' as i32 as ::core::ffi::c_char;
                        if nextc as u_char as ::core::ffi::c_int >= '0' as i32
                            && nextc as u_char as ::core::ffi::c_int <= '7' as i32
                        {
                            let fresh20 = dst;
                            dst = dst.offset(1);
                            *fresh20 = '0' as i32 as ::core::ffi::c_char;
                            let fresh21 = dst;
                            dst = dst.offset(1);
                            *fresh21 = '0' as i32 as ::core::ffi::c_char;
                        }
                    }
                }
                current_block = 1609532167703825882;
            }
            11 => {
                current_block = 4505220464648351793;
                match current_block {
                    17172381846212826752 => {
                        let fresh2 = dst;
                        dst = dst.offset(1);
                        *fresh2 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh3 = dst;
                        dst = dst.offset(1);
                        *fresh3 = 'n' as i32 as ::core::ffi::c_char;
                    }
                    6023546525629896002 => {
                        let fresh16 = dst;
                        dst = dst.offset(1);
                        *fresh16 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh17 = dst;
                        dst = dst.offset(1);
                        *fresh17 = 's' as i32 as ::core::ffi::c_char;
                    }
                    1901908384453561418 => {
                        let fresh14 = dst;
                        dst = dst.offset(1);
                        *fresh14 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh15 = dst;
                        dst = dst.offset(1);
                        *fresh15 = 'f' as i32 as ::core::ffi::c_char;
                    }
                    286618792822599835 => {
                        let fresh12 = dst;
                        dst = dst.offset(1);
                        *fresh12 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh13 = dst;
                        dst = dst.offset(1);
                        *fresh13 = 't' as i32 as ::core::ffi::c_char;
                    }
                    4505220464648351793 => {
                        let fresh10 = dst;
                        dst = dst.offset(1);
                        *fresh10 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh11 = dst;
                        dst = dst.offset(1);
                        *fresh11 = 'v' as i32 as ::core::ffi::c_char;
                    }
                    12286141215944697742 => {
                        let fresh8 = dst;
                        dst = dst.offset(1);
                        *fresh8 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh9 = dst;
                        dst = dst.offset(1);
                        *fresh9 = 'a' as i32 as ::core::ffi::c_char;
                    }
                    16114436840521083897 => {
                        let fresh6 = dst;
                        dst = dst.offset(1);
                        *fresh6 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh7 = dst;
                        dst = dst.offset(1);
                        *fresh7 = 'b' as i32 as ::core::ffi::c_char;
                    }
                    15896914202393627355 => {
                        let fresh4 = dst;
                        dst = dst.offset(1);
                        *fresh4 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh5 = dst;
                        dst = dst.offset(1);
                        *fresh5 = 'r' as i32 as ::core::ffi::c_char;
                    }
                    _ => {
                        let fresh18 = dst;
                        dst = dst.offset(1);
                        *fresh18 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh19 = dst;
                        dst = dst.offset(1);
                        *fresh19 = '0' as i32 as ::core::ffi::c_char;
                        if nextc as u_char as ::core::ffi::c_int >= '0' as i32
                            && nextc as u_char as ::core::ffi::c_int <= '7' as i32
                        {
                            let fresh20 = dst;
                            dst = dst.offset(1);
                            *fresh20 = '0' as i32 as ::core::ffi::c_char;
                            let fresh21 = dst;
                            dst = dst.offset(1);
                            *fresh21 = '0' as i32 as ::core::ffi::c_char;
                        }
                    }
                }
                current_block = 1609532167703825882;
            }
            9 => {
                current_block = 286618792822599835;
                match current_block {
                    17172381846212826752 => {
                        let fresh2 = dst;
                        dst = dst.offset(1);
                        *fresh2 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh3 = dst;
                        dst = dst.offset(1);
                        *fresh3 = 'n' as i32 as ::core::ffi::c_char;
                    }
                    6023546525629896002 => {
                        let fresh16 = dst;
                        dst = dst.offset(1);
                        *fresh16 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh17 = dst;
                        dst = dst.offset(1);
                        *fresh17 = 's' as i32 as ::core::ffi::c_char;
                    }
                    1901908384453561418 => {
                        let fresh14 = dst;
                        dst = dst.offset(1);
                        *fresh14 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh15 = dst;
                        dst = dst.offset(1);
                        *fresh15 = 'f' as i32 as ::core::ffi::c_char;
                    }
                    286618792822599835 => {
                        let fresh12 = dst;
                        dst = dst.offset(1);
                        *fresh12 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh13 = dst;
                        dst = dst.offset(1);
                        *fresh13 = 't' as i32 as ::core::ffi::c_char;
                    }
                    4505220464648351793 => {
                        let fresh10 = dst;
                        dst = dst.offset(1);
                        *fresh10 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh11 = dst;
                        dst = dst.offset(1);
                        *fresh11 = 'v' as i32 as ::core::ffi::c_char;
                    }
                    12286141215944697742 => {
                        let fresh8 = dst;
                        dst = dst.offset(1);
                        *fresh8 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh9 = dst;
                        dst = dst.offset(1);
                        *fresh9 = 'a' as i32 as ::core::ffi::c_char;
                    }
                    16114436840521083897 => {
                        let fresh6 = dst;
                        dst = dst.offset(1);
                        *fresh6 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh7 = dst;
                        dst = dst.offset(1);
                        *fresh7 = 'b' as i32 as ::core::ffi::c_char;
                    }
                    15896914202393627355 => {
                        let fresh4 = dst;
                        dst = dst.offset(1);
                        *fresh4 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh5 = dst;
                        dst = dst.offset(1);
                        *fresh5 = 'r' as i32 as ::core::ffi::c_char;
                    }
                    _ => {
                        let fresh18 = dst;
                        dst = dst.offset(1);
                        *fresh18 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh19 = dst;
                        dst = dst.offset(1);
                        *fresh19 = '0' as i32 as ::core::ffi::c_char;
                        if nextc as u_char as ::core::ffi::c_int >= '0' as i32
                            && nextc as u_char as ::core::ffi::c_int <= '7' as i32
                        {
                            let fresh20 = dst;
                            dst = dst.offset(1);
                            *fresh20 = '0' as i32 as ::core::ffi::c_char;
                            let fresh21 = dst;
                            dst = dst.offset(1);
                            *fresh21 = '0' as i32 as ::core::ffi::c_char;
                        }
                    }
                }
                current_block = 1609532167703825882;
            }
            12 => {
                current_block = 1901908384453561418;
                match current_block {
                    17172381846212826752 => {
                        let fresh2 = dst;
                        dst = dst.offset(1);
                        *fresh2 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh3 = dst;
                        dst = dst.offset(1);
                        *fresh3 = 'n' as i32 as ::core::ffi::c_char;
                    }
                    6023546525629896002 => {
                        let fresh16 = dst;
                        dst = dst.offset(1);
                        *fresh16 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh17 = dst;
                        dst = dst.offset(1);
                        *fresh17 = 's' as i32 as ::core::ffi::c_char;
                    }
                    1901908384453561418 => {
                        let fresh14 = dst;
                        dst = dst.offset(1);
                        *fresh14 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh15 = dst;
                        dst = dst.offset(1);
                        *fresh15 = 'f' as i32 as ::core::ffi::c_char;
                    }
                    286618792822599835 => {
                        let fresh12 = dst;
                        dst = dst.offset(1);
                        *fresh12 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh13 = dst;
                        dst = dst.offset(1);
                        *fresh13 = 't' as i32 as ::core::ffi::c_char;
                    }
                    4505220464648351793 => {
                        let fresh10 = dst;
                        dst = dst.offset(1);
                        *fresh10 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh11 = dst;
                        dst = dst.offset(1);
                        *fresh11 = 'v' as i32 as ::core::ffi::c_char;
                    }
                    12286141215944697742 => {
                        let fresh8 = dst;
                        dst = dst.offset(1);
                        *fresh8 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh9 = dst;
                        dst = dst.offset(1);
                        *fresh9 = 'a' as i32 as ::core::ffi::c_char;
                    }
                    16114436840521083897 => {
                        let fresh6 = dst;
                        dst = dst.offset(1);
                        *fresh6 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh7 = dst;
                        dst = dst.offset(1);
                        *fresh7 = 'b' as i32 as ::core::ffi::c_char;
                    }
                    15896914202393627355 => {
                        let fresh4 = dst;
                        dst = dst.offset(1);
                        *fresh4 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh5 = dst;
                        dst = dst.offset(1);
                        *fresh5 = 'r' as i32 as ::core::ffi::c_char;
                    }
                    _ => {
                        let fresh18 = dst;
                        dst = dst.offset(1);
                        *fresh18 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh19 = dst;
                        dst = dst.offset(1);
                        *fresh19 = '0' as i32 as ::core::ffi::c_char;
                        if nextc as u_char as ::core::ffi::c_int >= '0' as i32
                            && nextc as u_char as ::core::ffi::c_int <= '7' as i32
                        {
                            let fresh20 = dst;
                            dst = dst.offset(1);
                            *fresh20 = '0' as i32 as ::core::ffi::c_char;
                            let fresh21 = dst;
                            dst = dst.offset(1);
                            *fresh21 = '0' as i32 as ::core::ffi::c_char;
                        }
                    }
                }
                current_block = 1609532167703825882;
            }
            32 => {
                current_block = 6023546525629896002;
                match current_block {
                    17172381846212826752 => {
                        let fresh2 = dst;
                        dst = dst.offset(1);
                        *fresh2 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh3 = dst;
                        dst = dst.offset(1);
                        *fresh3 = 'n' as i32 as ::core::ffi::c_char;
                    }
                    6023546525629896002 => {
                        let fresh16 = dst;
                        dst = dst.offset(1);
                        *fresh16 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh17 = dst;
                        dst = dst.offset(1);
                        *fresh17 = 's' as i32 as ::core::ffi::c_char;
                    }
                    1901908384453561418 => {
                        let fresh14 = dst;
                        dst = dst.offset(1);
                        *fresh14 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh15 = dst;
                        dst = dst.offset(1);
                        *fresh15 = 'f' as i32 as ::core::ffi::c_char;
                    }
                    286618792822599835 => {
                        let fresh12 = dst;
                        dst = dst.offset(1);
                        *fresh12 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh13 = dst;
                        dst = dst.offset(1);
                        *fresh13 = 't' as i32 as ::core::ffi::c_char;
                    }
                    4505220464648351793 => {
                        let fresh10 = dst;
                        dst = dst.offset(1);
                        *fresh10 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh11 = dst;
                        dst = dst.offset(1);
                        *fresh11 = 'v' as i32 as ::core::ffi::c_char;
                    }
                    12286141215944697742 => {
                        let fresh8 = dst;
                        dst = dst.offset(1);
                        *fresh8 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh9 = dst;
                        dst = dst.offset(1);
                        *fresh9 = 'a' as i32 as ::core::ffi::c_char;
                    }
                    16114436840521083897 => {
                        let fresh6 = dst;
                        dst = dst.offset(1);
                        *fresh6 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh7 = dst;
                        dst = dst.offset(1);
                        *fresh7 = 'b' as i32 as ::core::ffi::c_char;
                    }
                    15896914202393627355 => {
                        let fresh4 = dst;
                        dst = dst.offset(1);
                        *fresh4 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh5 = dst;
                        dst = dst.offset(1);
                        *fresh5 = 'r' as i32 as ::core::ffi::c_char;
                    }
                    _ => {
                        let fresh18 = dst;
                        dst = dst.offset(1);
                        *fresh18 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh19 = dst;
                        dst = dst.offset(1);
                        *fresh19 = '0' as i32 as ::core::ffi::c_char;
                        if nextc as u_char as ::core::ffi::c_int >= '0' as i32
                            && nextc as u_char as ::core::ffi::c_int <= '7' as i32
                        {
                            let fresh20 = dst;
                            dst = dst.offset(1);
                            *fresh20 = '0' as i32 as ::core::ffi::c_char;
                            let fresh21 = dst;
                            dst = dst.offset(1);
                            *fresh21 = '0' as i32 as ::core::ffi::c_char;
                        }
                    }
                }
                current_block = 1609532167703825882;
            }
            0 => {
                current_block = 18317758206522205410;
                match current_block {
                    17172381846212826752 => {
                        let fresh2 = dst;
                        dst = dst.offset(1);
                        *fresh2 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh3 = dst;
                        dst = dst.offset(1);
                        *fresh3 = 'n' as i32 as ::core::ffi::c_char;
                    }
                    6023546525629896002 => {
                        let fresh16 = dst;
                        dst = dst.offset(1);
                        *fresh16 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh17 = dst;
                        dst = dst.offset(1);
                        *fresh17 = 's' as i32 as ::core::ffi::c_char;
                    }
                    1901908384453561418 => {
                        let fresh14 = dst;
                        dst = dst.offset(1);
                        *fresh14 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh15 = dst;
                        dst = dst.offset(1);
                        *fresh15 = 'f' as i32 as ::core::ffi::c_char;
                    }
                    286618792822599835 => {
                        let fresh12 = dst;
                        dst = dst.offset(1);
                        *fresh12 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh13 = dst;
                        dst = dst.offset(1);
                        *fresh13 = 't' as i32 as ::core::ffi::c_char;
                    }
                    4505220464648351793 => {
                        let fresh10 = dst;
                        dst = dst.offset(1);
                        *fresh10 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh11 = dst;
                        dst = dst.offset(1);
                        *fresh11 = 'v' as i32 as ::core::ffi::c_char;
                    }
                    12286141215944697742 => {
                        let fresh8 = dst;
                        dst = dst.offset(1);
                        *fresh8 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh9 = dst;
                        dst = dst.offset(1);
                        *fresh9 = 'a' as i32 as ::core::ffi::c_char;
                    }
                    16114436840521083897 => {
                        let fresh6 = dst;
                        dst = dst.offset(1);
                        *fresh6 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh7 = dst;
                        dst = dst.offset(1);
                        *fresh7 = 'b' as i32 as ::core::ffi::c_char;
                    }
                    15896914202393627355 => {
                        let fresh4 = dst;
                        dst = dst.offset(1);
                        *fresh4 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh5 = dst;
                        dst = dst.offset(1);
                        *fresh5 = 'r' as i32 as ::core::ffi::c_char;
                    }
                    _ => {
                        let fresh18 = dst;
                        dst = dst.offset(1);
                        *fresh18 = '\\' as i32 as ::core::ffi::c_char;
                        let fresh19 = dst;
                        dst = dst.offset(1);
                        *fresh19 = '0' as i32 as ::core::ffi::c_char;
                        if nextc as u_char as ::core::ffi::c_int >= '0' as i32
                            && nextc as u_char as ::core::ffi::c_int <= '7' as i32
                        {
                            let fresh20 = dst;
                            dst = dst.offset(1);
                            *fresh20 = '0' as i32 as ::core::ffi::c_char;
                            let fresh21 = dst;
                            dst = dst.offset(1);
                            *fresh21 = '0' as i32 as ::core::ffi::c_char;
                        }
                    }
                }
                current_block = 1609532167703825882;
            }
            _ => {
                current_block = 15345278821338558188;
            }
        }
    } else {
        current_block = 15345278821338558188;
    }
    if current_block == 15345278821338558188 {
        if c & 0o177 as ::core::ffi::c_int == ' ' as i32
            || flag & VIS_OCTAL != 0
            || flag & VIS_GLOB != 0
                && (c == '*' as i32 || c == '?' as i32 || c == '[' as i32 || c == '#' as i32)
        {
            let fresh22 = dst;
            dst = dst.offset(1);
            *fresh22 = '\\' as i32 as ::core::ffi::c_char;
            let fresh23 = dst;
            dst = dst.offset(1);
            *fresh23 = ((c as u_char as ::core::ffi::c_int >> 6 as ::core::ffi::c_int
                & 0o7 as ::core::ffi::c_int)
                + '0' as i32) as ::core::ffi::c_char;
            let fresh24 = dst;
            dst = dst.offset(1);
            *fresh24 = ((c as u_char as ::core::ffi::c_int >> 3 as ::core::ffi::c_int
                & 0o7 as ::core::ffi::c_int)
                + '0' as i32) as ::core::ffi::c_char;
            let fresh25 = dst;
            dst = dst.offset(1);
            *fresh25 = ((c as u_char as ::core::ffi::c_int & 0o7 as ::core::ffi::c_int)
                + '0' as i32) as ::core::ffi::c_char;
        } else {
            if flag & VIS_NOSLASH == 0 as ::core::ffi::c_int {
                let fresh26 = dst;
                dst = dst.offset(1);
                *fresh26 = '\\' as i32 as ::core::ffi::c_char;
            }
            if c & 0o200 as ::core::ffi::c_int != 0 {
                c &= 0o177 as ::core::ffi::c_int;
                let fresh27 = dst;
                dst = dst.offset(1);
                *fresh27 = 'M' as i32 as ::core::ffi::c_char;
            }
            if ::libc::iscntrl(c as u_char as ::core::ffi::c_int) != 0 {
                let fresh28 = dst;
                dst = dst.offset(1);
                *fresh28 = '^' as i32 as ::core::ffi::c_char;
                if c == 0o177 as ::core::ffi::c_int {
                    let fresh29 = dst;
                    dst = dst.offset(1);
                    *fresh29 = '?' as i32 as ::core::ffi::c_char;
                } else {
                    let fresh30 = dst;
                    dst = dst.offset(1);
                    *fresh30 = (c + '@' as i32) as ::core::ffi::c_char;
                }
            } else {
                let fresh31 = dst;
                dst = dst.offset(1);
                *fresh31 = '-' as i32 as ::core::ffi::c_char;
                let fresh32 = dst;
                dst = dst.offset(1);
                *fresh32 = c as ::core::ffi::c_char;
            }
        }
    }
    *dst = '\0' as i32 as ::core::ffi::c_char;
    dst
}
pub unsafe fn strvis(
    mut dst: *mut ::core::ffi::c_char,
    mut src: *const ::core::ffi::c_char,
    mut flag: ::core::ffi::c_int,
) -> ::core::ffi::c_int {
    let mut c: ::core::ffi::c_char = 0;
    let mut start: *mut ::core::ffi::c_char = ::core::ptr::null_mut::<::core::ffi::c_char>();
    start = dst;
    loop {
        c = *src;
        if !(c != 0) {
            break;
        }
        src = src.offset(1);
        dst = vis(
            dst,
            c as ::core::ffi::c_int,
            flag,
            *src as ::core::ffi::c_int,
        );
    }
    *dst = '\0' as i32 as ::core::ffi::c_char;
    dst.offset_from(start) as ::core::ffi::c_long as ::core::ffi::c_int
}
pub unsafe fn strnvis(
    mut dst: *mut ::core::ffi::c_char,
    mut src: *const ::core::ffi::c_char,
    mut siz: size_t,
    mut flag: ::core::ffi::c_int,
) -> ::core::ffi::c_int {
    let mut start: *mut ::core::ffi::c_char = ::core::ptr::null_mut::<::core::ffi::c_char>();
    let mut end: *mut ::core::ffi::c_char = ::core::ptr::null_mut::<::core::ffi::c_char>();
    let mut tbuf: [::core::ffi::c_char; 5] = [0; 5];
    let mut c: ::core::ffi::c_int = 0;
    let mut i: ::core::ffi::c_int = 0;
    i = 0 as ::core::ffi::c_int;
    start = dst;
    end = start.add(siz).offset(-(1 as ::core::ffi::c_int as isize));
    loop {
        c = *src as ::core::ffi::c_int;
        if !(c != 0 && dst < end) {
            break;
        }
        if (c == '\\' as i32 || flag & VIS_ALL == 0 as ::core::ffi::c_int)
            && (c as u_int <= UCHAR_MAX as u_int
                && c as u_char as ::core::ffi::c_int & !(0x7f as ::core::ffi::c_int)
                    == 0 as ::core::ffi::c_int
                && (c != '*' as i32 && c != '?' as i32 && c != '[' as i32 && c != '#' as i32
                    || flag & VIS_GLOB == 0 as ::core::ffi::c_int)
                && ::libc::isgraph(c as u_char as ::core::ffi::c_int) != 0
                || flag & VIS_SP == 0 as ::core::ffi::c_int && c == ' ' as i32
                || flag & VIS_TAB == 0 as ::core::ffi::c_int && c == '\t' as i32
                || flag & VIS_NL == 0 as ::core::ffi::c_int && c == '\n' as i32
                || flag & VIS_SAFE != 0
                    && (c == '\u{8}' as i32
                        || c == '\u{7}' as i32
                        || c == '\r' as i32
                        || ::libc::isgraph(c as u_char as ::core::ffi::c_int) != 0))
        {
            if c == '"' as i32 && flag & VIS_DQ != 0 as ::core::ffi::c_int
                || c == '\\' as i32 && flag & VIS_NOSLASH == 0 as ::core::ffi::c_int
            {
                if dst.offset(1 as ::core::ffi::c_int as isize) >= end {
                    i = 2 as ::core::ffi::c_int;
                    break;
                } else {
                    let fresh33 = dst;
                    dst = dst.offset(1);
                    *fresh33 = '\\' as i32 as ::core::ffi::c_char;
                }
            }
            i = 1 as ::core::ffi::c_int;
            let fresh34 = dst;
            dst = dst.offset(1);
            *fresh34 = c as ::core::ffi::c_char;
            src = src.offset(1);
        } else {
            src = src.offset(1);
            i = vis(
                &raw mut tbuf as *mut ::core::ffi::c_char,
                c,
                flag,
                *src as ::core::ffi::c_int,
            )
            .offset_from(&raw mut tbuf as *mut ::core::ffi::c_char)
                as ::core::ffi::c_long as ::core::ffi::c_int;
            if dst.offset(i as isize) <= end {
                memcpy(
                    dst as *mut ::core::ffi::c_void,
                    &raw mut tbuf as *mut ::core::ffi::c_char as *const ::core::ffi::c_void,
                    i as size_t,
                );
                dst = dst.offset(i as isize);
            } else {
                src = src.offset(-1);
                break;
            }
        }
    }
    if siz > 0 as size_t {
        *dst = '\0' as i32 as ::core::ffi::c_char;
    }
    if dst.offset(i as isize) > end {
        loop {
            c = *src as ::core::ffi::c_int;
            if !(c != 0) {
                break;
            }
            src = src.offset(1);
            dst = dst.offset(
                vis(
                    &raw mut tbuf as *mut ::core::ffi::c_char,
                    c,
                    flag,
                    *src as ::core::ffi::c_int,
                )
                .offset_from(&raw mut tbuf as *mut ::core::ffi::c_char)
                    as ::core::ffi::c_long as isize,
            );
        }
    }
    dst.offset_from(start) as ::core::ffi::c_long as ::core::ffi::c_int
}
/// Escape a borrowed C string into Rust-owned storage, preserving `stravis` byte rules.
pub(crate) fn stravis_cstring(src: &std::ffi::CStr, flag: ::core::ffi::c_int) -> std::ffi::CString {
    let source_len = src.to_bytes().len();
    let capacity = source_len
        .checked_mul(4)
        .and_then(|size| size.checked_add(1))
        .expect("escaped string is too large");
    let mut buffer = vec![0u8; capacity];
    let escaped_len = unsafe { strvis(buffer.as_mut_ptr().cast(), src.as_ptr(), flag) } as usize;
    buffer.truncate(escaped_len + 1);
    std::ffi::CString::from_vec_with_nul(buffer).expect("vis output contains no interior NUL")
}

#[cfg(test)]
mod owned_vis_tests {
    use super::*;

    #[test]
    fn stravis_cstring_owns_escaped_output() {
        let input = std::ffi::CString::new(&b"a\t\\\xff"[..]).unwrap();
        let escaped = stravis_cstring(input.as_c_str(), VIS_CSTYLE | VIS_TAB | VIS_OCTAL);
        drop(input);
        assert_eq!(escaped.as_bytes(), b"a\\t\\\\\\377");
    }
}
