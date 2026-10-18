use crate::ffi::libc::warnx;
use crate::shared::abi::*;
use libc::{getenv, strchr, strlen, strncmp};

#[derive(Copy, Clone)]
#[repr(C)]
pub struct option {
    pub name: *const ::core::ffi::c_char,
    pub has_arg: ::core::ffi::c_int,
    pub flag: *mut ::core::ffi::c_int,
    pub val: ::core::ffi::c_int,
}
pub const no_argument: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
pub const required_argument: ::core::ffi::c_int = 1 as ::core::ffi::c_int;
pub const optional_argument: ::core::ffi::c_int = 2 as ::core::ffi::c_int;
pub static mut BSDopterr: ::core::ffi::c_int = 1 as ::core::ffi::c_int;
pub static mut BSDoptind: ::core::ffi::c_int = 1 as ::core::ffi::c_int;
pub static mut BSDoptopt: ::core::ffi::c_int = '?' as i32;
pub static mut BSDoptreset: ::core::ffi::c_int = 0;
pub static mut BSDoptarg: *mut ::core::ffi::c_char =
    ::core::ptr::null::<::core::ffi::c_char>() as *mut ::core::ffi::c_char;
pub const FLAG_PERMUTE: ::core::ffi::c_int = 0x1 as ::core::ffi::c_int;
pub const FLAG_ALLARGS: ::core::ffi::c_int = 0x2 as ::core::ffi::c_int;
pub const FLAG_LONGONLY: ::core::ffi::c_int = 0x4 as ::core::ffi::c_int;
pub const EMSG: *mut ::core::ffi::c_char = c"".as_ptr() as *mut ::core::ffi::c_char;
static mut place: *mut ::core::ffi::c_char = EMSG;
static mut nonopt_start: ::core::ffi::c_int = -(1 as ::core::ffi::c_int);
static mut nonopt_end: ::core::ffi::c_int = -(1 as ::core::ffi::c_int);
static mut recargchar: [::core::ffi::c_char; 34] = unsafe {
    ::core::mem::transmute::<[u8; 34], [::core::ffi::c_char; 34]>(
        *b"option requires an argument -- %c\0",
    )
};
static mut recargstring: [::core::ffi::c_char; 34] = unsafe {
    ::core::mem::transmute::<[u8; 34], [::core::ffi::c_char; 34]>(
        *b"option requires an argument -- %s\0",
    )
};
static mut ambig: [::core::ffi::c_char; 25] = unsafe {
    ::core::mem::transmute::<[u8; 25], [::core::ffi::c_char; 25]>(*b"ambiguous option -- %.*s\0")
};
static mut noarg: [::core::ffi::c_char; 40] = unsafe {
    ::core::mem::transmute::<[u8; 40], [::core::ffi::c_char; 40]>(
        *b"option doesn't take an argument -- %.*s\0",
    )
};
static mut illoptchar: [::core::ffi::c_char; 21] = unsafe {
    ::core::mem::transmute::<[u8; 21], [::core::ffi::c_char; 21]>(*b"unknown option -- %c\0")
};
static mut illoptstring: [::core::ffi::c_char; 21] = unsafe {
    ::core::mem::transmute::<[u8; 21], [::core::ffi::c_char; 21]>(*b"unknown option -- %s\0")
};
unsafe fn gcd(mut a: ::core::ffi::c_int, mut b: ::core::ffi::c_int) -> ::core::ffi::c_int {
    let mut c: ::core::ffi::c_int = 0;
    c = a % b;
    while c != 0 as ::core::ffi::c_int {
        a = b;
        b = c;
        c = a % b;
    }
    b
}
unsafe fn permute_args(
    mut panonopt_start: ::core::ffi::c_int,
    mut panonopt_end: ::core::ffi::c_int,
    mut opt_end: ::core::ffi::c_int,
    mut nargv: *const *mut ::core::ffi::c_char,
) {
    let mut cstart: ::core::ffi::c_int = 0;
    let mut cyclelen: ::core::ffi::c_int = 0;
    let mut i: ::core::ffi::c_int = 0;
    let mut j: ::core::ffi::c_int = 0;
    let mut ncycle: ::core::ffi::c_int = 0;
    let mut nnonopts: ::core::ffi::c_int = 0;
    let mut nopts: ::core::ffi::c_int = 0;
    let mut pos: ::core::ffi::c_int = 0;
    let mut swap: *mut ::core::ffi::c_char = ::core::ptr::null_mut::<::core::ffi::c_char>();
    nnonopts = panonopt_end - panonopt_start;
    nopts = opt_end - panonopt_end;
    ncycle = gcd(nnonopts, nopts);
    cyclelen = (opt_end - panonopt_start) / ncycle;
    i = 0 as ::core::ffi::c_int;
    while i < ncycle {
        cstart = panonopt_end + i;
        pos = cstart;
        j = 0 as ::core::ffi::c_int;
        while j < cyclelen {
            if pos >= panonopt_end {
                pos -= nnonopts;
            } else {
                pos += nopts;
            }
            swap = *nargv.offset(pos as isize);
            let fresh3 = &mut *(nargv as *mut *mut ::core::ffi::c_char).offset(pos as isize);
            *fresh3 = *nargv.offset(cstart as isize);
            let fresh4 = &mut *(nargv as *mut *mut ::core::ffi::c_char).offset(cstart as isize);
            *fresh4 = swap;
            j += 1;
        }
        i += 1;
    }
}
unsafe fn parse_long_options(
    mut nargv: *const *mut ::core::ffi::c_char,
    mut options: *const ::core::ffi::c_char,
    mut long_options: *const option,
    mut idx: *mut ::core::ffi::c_int,
    mut short_too: ::core::ffi::c_int,
) -> ::core::ffi::c_int {
    let mut current_argv: *mut ::core::ffi::c_char = ::core::ptr::null_mut::<::core::ffi::c_char>();
    let mut has_equal: *mut ::core::ffi::c_char = ::core::ptr::null_mut::<::core::ffi::c_char>();
    let mut current_argv_len: size_t = 0;
    let mut i: ::core::ffi::c_int = 0;
    let mut match_0: ::core::ffi::c_int = 0;
    current_argv = place;
    match_0 = -(1 as ::core::ffi::c_int);
    BSDoptind += 1;
    has_equal = strchr(current_argv, '=' as i32);
    if !has_equal.is_null() {
        current_argv_len = has_equal.offset_from(current_argv) as ::core::ffi::c_long as size_t;
        has_equal = has_equal.offset(1);
    } else {
        current_argv_len = strlen(current_argv);
    }
    i = 0 as ::core::ffi::c_int;
    while !(*long_options.offset(i as isize)).name.is_null() {
        if !(strncmp(
            current_argv,
            (*long_options.offset(i as isize)).name,
            current_argv_len,
        ) != 0)
        {
            if strlen((*long_options.offset(i as isize)).name) == current_argv_len {
                match_0 = i;
                break;
            } else if !(short_too != 0 && current_argv_len == 1 as size_t) {
                if match_0 == -(1 as ::core::ffi::c_int) {
                    match_0 = i;
                } else {
                    if BSDopterr != 0 && *options as ::core::ffi::c_int != ':' as i32 {
                        warnx(
                            &raw const ambig as *const ::core::ffi::c_char,
                            current_argv_len as ::core::ffi::c_int,
                            current_argv,
                        );
                    }
                    BSDoptopt = 0 as ::core::ffi::c_int;
                    return '?' as i32;
                }
            }
        }
        i += 1;
    }
    if match_0 != -(1 as ::core::ffi::c_int) {
        if (*long_options.offset(match_0 as isize)).has_arg == no_argument && !has_equal.is_null() {
            if BSDopterr != 0 && *options as ::core::ffi::c_int != ':' as i32 {
                warnx(
                    &raw const noarg as *const ::core::ffi::c_char,
                    current_argv_len as ::core::ffi::c_int,
                    current_argv,
                );
            }
            if (*long_options.offset(match_0 as isize)).flag.is_null() {
                BSDoptopt = (*long_options.offset(match_0 as isize)).val;
            } else {
                BSDoptopt = 0 as ::core::ffi::c_int;
            }
            return if *options as ::core::ffi::c_int == ':' as i32 {
                ':' as i32
            } else {
                '?' as i32
            };
        }
        if (*long_options.offset(match_0 as isize)).has_arg == required_argument
            || (*long_options.offset(match_0 as isize)).has_arg == optional_argument
        {
            if !has_equal.is_null() {
                BSDoptarg = has_equal;
            } else if (*long_options.offset(match_0 as isize)).has_arg == required_argument {
                let fresh2 = BSDoptind;
                BSDoptind += 1;
                BSDoptarg = *nargv.offset(fresh2 as isize);
            }
        }
        if (*long_options.offset(match_0 as isize)).has_arg == required_argument
            && BSDoptarg.is_null()
        {
            if BSDopterr != 0 && *options as ::core::ffi::c_int != ':' as i32 {
                warnx(
                    &raw const recargstring as *const ::core::ffi::c_char,
                    current_argv,
                );
            }
            if (*long_options.offset(match_0 as isize)).flag.is_null() {
                BSDoptopt = (*long_options.offset(match_0 as isize)).val;
            } else {
                BSDoptopt = 0 as ::core::ffi::c_int;
            }
            BSDoptind -= 1;
            return if *options as ::core::ffi::c_int == ':' as i32 {
                ':' as i32
            } else {
                '?' as i32
            };
        }
    } else {
        if short_too != 0 {
            BSDoptind -= 1;
            return -(1 as ::core::ffi::c_int);
        }
        if BSDopterr != 0 && *options as ::core::ffi::c_int != ':' as i32 {
            warnx(
                &raw const illoptstring as *const ::core::ffi::c_char,
                current_argv,
            );
        }
        BSDoptopt = 0 as ::core::ffi::c_int;
        return '?' as i32;
    }
    if !idx.is_null() {
        *idx = match_0;
    }
    if !(*long_options.offset(match_0 as isize)).flag.is_null() {
        *(*long_options.offset(match_0 as isize)).flag =
            (*long_options.offset(match_0 as isize)).val;
        0 as ::core::ffi::c_int
    } else {
        (*long_options.offset(match_0 as isize)).val
    }
}
unsafe fn getopt_internal(
    mut nargc: ::core::ffi::c_int,
    mut nargv: *const *mut ::core::ffi::c_char,
    mut options: *const ::core::ffi::c_char,
    mut long_options: *const option,
    mut idx: *mut ::core::ffi::c_int,
    mut flags: ::core::ffi::c_int,
) -> ::core::ffi::c_int {
    let mut oli: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    let mut optchar: ::core::ffi::c_int = 0;
    let mut short_too: ::core::ffi::c_int = 0;
    static mut posixly_correct: ::core::ffi::c_int = -(1 as ::core::ffi::c_int);
    if options.is_null() {
        return -(1 as ::core::ffi::c_int);
    }
    if BSDoptind == 0 as ::core::ffi::c_int {
        BSDoptreset = 1 as ::core::ffi::c_int;
        BSDoptind = BSDoptreset;
    }
    if posixly_correct == -(1 as ::core::ffi::c_int) || BSDoptreset != 0 {
        posixly_correct = (getenv(c"POSIXLY_CORRECT".as_ptr()) != NULL as *mut ::core::ffi::c_char)
            as ::core::ffi::c_int;
    }
    if *options as ::core::ffi::c_int == '-' as i32 {
        flags |= FLAG_ALLARGS;
    } else if posixly_correct != 0 || *options as ::core::ffi::c_int == '+' as i32 {
        flags &= !FLAG_PERMUTE;
    }
    if *options as ::core::ffi::c_int == '+' as i32 || *options as ::core::ffi::c_int == '-' as i32
    {
        options = options.offset(1);
    }
    BSDoptarg = ::core::ptr::null_mut::<::core::ffi::c_char>();
    if BSDoptreset != 0 {
        nonopt_end = -(1 as ::core::ffi::c_int);
        nonopt_start = nonopt_end;
    }
    while BSDoptreset != 0 || *place == 0 {
        BSDoptreset = 0 as ::core::ffi::c_int;
        if BSDoptind >= nargc {
            place = EMSG;
            if nonopt_end != -(1 as ::core::ffi::c_int) {
                permute_args(nonopt_start, nonopt_end, BSDoptind, nargv);
                BSDoptind -= nonopt_end - nonopt_start;
            } else if nonopt_start != -(1 as ::core::ffi::c_int) {
                BSDoptind = nonopt_start;
            }
            nonopt_end = -(1 as ::core::ffi::c_int);
            nonopt_start = nonopt_end;
            return -(1 as ::core::ffi::c_int);
        }
        place = *nargv.offset(BSDoptind as isize);
        if *place as ::core::ffi::c_int != '-' as i32
            || *place.offset(1 as ::core::ffi::c_int as isize) as ::core::ffi::c_int == '\0' as i32
                && strchr(options, '-' as i32).is_null()
        {
            place = EMSG;
            if flags & FLAG_ALLARGS != 0 {
                let fresh0 = BSDoptind;
                BSDoptind += 1;
                BSDoptarg = *nargv.offset(fresh0 as isize);
                return 1 as ::core::ffi::c_int;
            }
            if flags & FLAG_PERMUTE == 0 {
                return -(1 as ::core::ffi::c_int);
            }
            if nonopt_start == -(1 as ::core::ffi::c_int) {
                nonopt_start = BSDoptind;
            } else if nonopt_end != -(1 as ::core::ffi::c_int) {
                permute_args(nonopt_start, nonopt_end, BSDoptind, nargv);
                nonopt_start = BSDoptind - (nonopt_end - nonopt_start);
                nonopt_end = -(1 as ::core::ffi::c_int);
            }
            BSDoptind += 1;
        } else {
            if nonopt_start != -(1 as ::core::ffi::c_int)
                && nonopt_end == -(1 as ::core::ffi::c_int)
            {
                nonopt_end = BSDoptind;
            }
            if *place.offset(1 as ::core::ffi::c_int as isize) as ::core::ffi::c_int != '\0' as i32
                && {
                    place = place.offset(1);
                    *place as ::core::ffi::c_int == '-' as i32
                }
                && *place.offset(1 as ::core::ffi::c_int as isize) as ::core::ffi::c_int
                    == '\0' as i32
            {
                BSDoptind += 1;
                place = EMSG;
                if nonopt_end != -(1 as ::core::ffi::c_int) {
                    permute_args(nonopt_start, nonopt_end, BSDoptind, nargv);
                    BSDoptind -= nonopt_end - nonopt_start;
                }
                nonopt_end = -(1 as ::core::ffi::c_int);
                nonopt_start = nonopt_end;
                return -(1 as ::core::ffi::c_int);
            }
            break;
        }
    }
    if !long_options.is_null()
        && place != *nargv.offset(BSDoptind as isize)
        && (*place as ::core::ffi::c_int == '-' as i32 || flags & FLAG_LONGONLY != 0)
    {
        short_too = 0 as ::core::ffi::c_int;
        if *place as ::core::ffi::c_int == '-' as i32 {
            place = place.offset(1);
        } else if *place as ::core::ffi::c_int != ':' as i32
            && !strchr(options, *place as ::core::ffi::c_int).is_null()
        {
            short_too = 1 as ::core::ffi::c_int;
        }
        optchar = parse_long_options(nargv, options, long_options, idx, short_too);
        if optchar != -(1 as ::core::ffi::c_int) {
            place = EMSG;
            return optchar;
        }
    }
    let fresh1 = place;
    place = place.offset(1);
    optchar = *fresh1 as ::core::ffi::c_int;
    if optchar == ':' as i32
        || optchar == '-' as i32 && *place as ::core::ffi::c_int != '\0' as i32
        || {
            oli = strchr(options, optchar);
            oli.is_null()
        }
    {
        if optchar == '-' as i32 && *place as ::core::ffi::c_int == '\0' as i32 {
            return -(1 as ::core::ffi::c_int);
        }
        if *place == 0 {
            BSDoptind += 1;
        }
        if BSDopterr != 0 && *options as ::core::ffi::c_int != ':' as i32 {
            warnx(&raw const illoptchar as *const ::core::ffi::c_char, optchar);
        }
        BSDoptopt = optchar;
        return '?' as i32;
    }
    if !long_options.is_null()
        && optchar == 'W' as i32
        && *oli.offset(1 as ::core::ffi::c_int as isize) as ::core::ffi::c_int == ';' as i32
    {
        if !(*place != 0) {
            BSDoptind += 1;
            if BSDoptind >= nargc {
                place = EMSG;
                if BSDopterr != 0 && *options as ::core::ffi::c_int != ':' as i32 {
                    warnx(&raw const recargchar as *const ::core::ffi::c_char, optchar);
                }
                BSDoptopt = optchar;
                return if *options as ::core::ffi::c_int == ':' as i32 {
                    ':' as i32
                } else {
                    '?' as i32
                };
            } else {
                place = *nargv.offset(BSDoptind as isize);
            }
        }
        optchar = parse_long_options(nargv, options, long_options, idx, 0 as ::core::ffi::c_int);
        place = EMSG;
        return optchar;
    }
    oli = oli.offset(1);
    if *oli as ::core::ffi::c_int != ':' as i32 {
        if *place == 0 {
            BSDoptind += 1;
        }
    } else {
        BSDoptarg = ::core::ptr::null_mut::<::core::ffi::c_char>();
        if *place != 0 {
            BSDoptarg = place;
        } else if *oli.offset(1 as ::core::ffi::c_int as isize) as ::core::ffi::c_int != ':' as i32
        {
            BSDoptind += 1;
            if BSDoptind >= nargc {
                place = EMSG;
                if BSDopterr != 0 && *options as ::core::ffi::c_int != ':' as i32 {
                    warnx(&raw const recargchar as *const ::core::ffi::c_char, optchar);
                }
                BSDoptopt = optchar;
                return if *options as ::core::ffi::c_int == ':' as i32 {
                    ':' as i32
                } else {
                    '?' as i32
                };
            } else {
                BSDoptarg = *nargv.offset(BSDoptind as isize);
            }
        }
        place = EMSG;
        BSDoptind += 1;
    }
    optchar
}
pub unsafe fn BSDgetopt(
    mut nargc: ::core::ffi::c_int,
    mut nargv: *const *mut ::core::ffi::c_char,
    mut options: *const ::core::ffi::c_char,
) -> ::core::ffi::c_int {
    getopt_internal(
        nargc,
        nargv,
        options,
        ::core::ptr::null::<option>(),
        ::core::ptr::null_mut::<::core::ffi::c_int>(),
        0 as ::core::ffi::c_int,
    )
}
