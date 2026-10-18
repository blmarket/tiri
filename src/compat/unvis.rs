use crate::shared::abi::ssize_t;
use crate::shared::abi::*;
pub const UNVIS_VALID: ::core::ffi::c_int = 1 as ::core::ffi::c_int;
pub const UNVIS_VALIDPUSH: ::core::ffi::c_int = 2 as ::core::ffi::c_int;
pub const UNVIS_NOCHAR: ::core::ffi::c_int = 3 as ::core::ffi::c_int;
pub const UNVIS_SYNBAD: ::core::ffi::c_int = -(1 as ::core::ffi::c_int);
pub const UNVIS_END: ::core::ffi::c_int = 1 as ::core::ffi::c_int;
pub const S_GROUND: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
pub const S_START: ::core::ffi::c_int = 1;
pub const S_META: ::core::ffi::c_int = 2;
pub const S_META1: ::core::ffi::c_int = 3;
pub const S_CTRL: ::core::ffi::c_int = 4;
pub const S_OCTAL2: ::core::ffi::c_int = 5;
pub const S_OCTAL3: ::core::ffi::c_int = 6;
pub unsafe fn unvis(
    mut cp: *mut ::core::ffi::c_char,
    mut c: ::core::ffi::c_char,
    mut astate: *mut ::core::ffi::c_int,
    mut flag: ::core::ffi::c_int,
) -> ::core::ffi::c_int {
    if flag & UNVIS_END != 0 {
        if *astate == S_OCTAL2 || *astate == S_OCTAL3 {
            *astate = S_GROUND;
            return 1 as ::core::ffi::c_int;
        }
        return if *astate == S_GROUND {
            UNVIS_NOCHAR
        } else {
            UNVIS_SYNBAD
        };
    }
    match *astate {
        S_GROUND => {
            *cp = 0 as ::core::ffi::c_char;
            if c as ::core::ffi::c_int == '\\' as i32 {
                *astate = S_START;
                return 0 as ::core::ffi::c_int;
            }
            *cp = c;
            1 as ::core::ffi::c_int
        }
        S_START => {
            match c as ::core::ffi::c_int {
                92 => {
                    *cp = c;
                    *astate = S_GROUND;
                    return 1 as ::core::ffi::c_int;
                }
                48..=55 => {
                    *cp = (c as ::core::ffi::c_int - '0' as i32) as ::core::ffi::c_char;
                    *astate = S_OCTAL2;
                    return 0 as ::core::ffi::c_int;
                }
                77 => {
                    *cp = 0o200 as ::core::ffi::c_int as ::core::ffi::c_char;
                    *astate = S_META;
                    return 0 as ::core::ffi::c_int;
                }
                94 => {
                    *astate = S_CTRL;
                    return 0 as ::core::ffi::c_int;
                }
                110 => {
                    *cp = '\n' as i32 as ::core::ffi::c_char;
                    *astate = S_GROUND;
                    return 1 as ::core::ffi::c_int;
                }
                114 => {
                    *cp = '\r' as i32 as ::core::ffi::c_char;
                    *astate = S_GROUND;
                    return 1 as ::core::ffi::c_int;
                }
                98 => {
                    *cp = '\u{8}' as i32 as ::core::ffi::c_char;
                    *astate = S_GROUND;
                    return 1 as ::core::ffi::c_int;
                }
                97 => {
                    *cp = '\u{7}' as i32 as ::core::ffi::c_char;
                    *astate = S_GROUND;
                    return 1 as ::core::ffi::c_int;
                }
                118 => {
                    *cp = '\u{b}' as i32 as ::core::ffi::c_char;
                    *astate = S_GROUND;
                    return 1 as ::core::ffi::c_int;
                }
                116 => {
                    *cp = '\t' as i32 as ::core::ffi::c_char;
                    *astate = S_GROUND;
                    return 1 as ::core::ffi::c_int;
                }
                102 => {
                    *cp = '\u{c}' as i32 as ::core::ffi::c_char;
                    *astate = S_GROUND;
                    return 1 as ::core::ffi::c_int;
                }
                115 => {
                    *cp = ' ' as i32 as ::core::ffi::c_char;
                    *astate = S_GROUND;
                    return 1 as ::core::ffi::c_int;
                }
                69 => {
                    *cp = '\u{1b}' as i32 as ::core::ffi::c_char;
                    *astate = S_GROUND;
                    return 1 as ::core::ffi::c_int;
                }
                10 => {
                    *astate = S_GROUND;
                    return 3 as ::core::ffi::c_int;
                }
                36 => {
                    *astate = S_GROUND;
                    return 3 as ::core::ffi::c_int;
                }
                _ => {}
            }
            *astate = S_GROUND;
            -(1 as ::core::ffi::c_int)
        }
        S_META => {
            if c as ::core::ffi::c_int == '-' as i32 {
                *astate = S_META1;
            } else if c as ::core::ffi::c_int == '^' as i32 {
                *astate = S_CTRL;
            } else {
                *astate = S_GROUND;
                return -(1 as ::core::ffi::c_int);
            }
            0 as ::core::ffi::c_int
        }
        S_META1 => {
            *astate = S_GROUND;
            *cp = (*cp as ::core::ffi::c_int | c as ::core::ffi::c_int) as ::core::ffi::c_char;
            1 as ::core::ffi::c_int
        }
        S_CTRL => {
            if c as ::core::ffi::c_int == '?' as i32 {
                *cp = (*cp as ::core::ffi::c_int | 0o177 as ::core::ffi::c_int)
                    as ::core::ffi::c_char;
            } else {
                *cp = (*cp as ::core::ffi::c_int
                    | c as ::core::ffi::c_int & 0o37 as ::core::ffi::c_int)
                    as ::core::ffi::c_char;
            }
            *astate = S_GROUND;
            1 as ::core::ffi::c_int
        }
        S_OCTAL2 => {
            if c as u_char as ::core::ffi::c_int >= '0' as i32
                && c as u_char as ::core::ffi::c_int <= '7' as i32
            {
                *cp = (((*cp as ::core::ffi::c_int) << 3 as ::core::ffi::c_int)
                    + (c as ::core::ffi::c_int - '0' as i32))
                    as ::core::ffi::c_char;
                *astate = S_OCTAL3;
                return 0 as ::core::ffi::c_int;
            }
            *astate = S_GROUND;
            2 as ::core::ffi::c_int
        }
        S_OCTAL3 => {
            *astate = S_GROUND;
            if c as u_char as ::core::ffi::c_int >= '0' as i32
                && c as u_char as ::core::ffi::c_int <= '7' as i32
            {
                *cp = (((*cp as ::core::ffi::c_int) << 3 as ::core::ffi::c_int)
                    + (c as ::core::ffi::c_int - '0' as i32))
                    as ::core::ffi::c_char;
                return 1 as ::core::ffi::c_int;
            }
            2 as ::core::ffi::c_int
        }
        _ => {
            *astate = S_GROUND;
            -(1 as ::core::ffi::c_int)
        }
    }
}
pub unsafe fn strunvis(
    mut dst: *mut ::core::ffi::c_char,
    mut src: *const ::core::ffi::c_char,
) -> ::core::ffi::c_int {
    let mut c: ::core::ffi::c_char = 0;
    let mut start: *mut ::core::ffi::c_char = dst;
    let mut state: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    loop {
        let fresh0 = src;
        src = src.offset(1);
        c = *fresh0;
        if !(c != 0) {
            break;
        }
        loop {
            match unvis(dst, c, &raw mut state, 0 as ::core::ffi::c_int) {
                UNVIS_VALID => {
                    dst = dst.offset(1);
                    break;
                }
                UNVIS_VALIDPUSH => {
                    dst = dst.offset(1);
                }
                0 | UNVIS_NOCHAR => {
                    break;
                }
                _ => {
                    *dst = '\0' as i32 as ::core::ffi::c_char;
                    return -(1 as ::core::ffi::c_int);
                }
            }
        }
    }
    if unvis(dst, c, &raw mut state, UNVIS_END) == UNVIS_VALID {
        dst = dst.offset(1);
    }
    *dst = '\0' as i32 as ::core::ffi::c_char;
    dst.offset_from(start) as ::core::ffi::c_long as ::core::ffi::c_int
}
