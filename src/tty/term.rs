use super::model::*;
use crate::compat::strtonum::strtonum;
use crate::compat::unvis::strunvis;
use crate::compat::vis::strnvis;
use crate::ffi::ncurses::{
    cur_term, del_curterm, setupterm, tigetflag, tigetnum, tigetstr, tiparm_s,
};
use crate::format::bytes::{cstr_suffix, format_message_with};
use crate::log::{fatalx, log_bytes, log_cstr, log_debug};
use crate::options::OptionsScope;
use crate::server_client::Client as _;
use crate::shared::abi::*;
use crate::shared::client::{ClientRef, ClientWeak};
use crate::shared::limits::INT_MAX;
use crate::shared::posix_io::STDIN_FILENO;
use crate::shared::tty::*;
use crate::shared::vis::{VIS_CSTYLE, VIS_NL, VIS_OCTAL, VIS_TAB};
use crate::tty::features::{tty_apply_features, tty_parse_client_features};
use libc::{fnmatch, strcmp, strlen};
use std::ffi::{CStr, CString};

pub(super) const TTYCODE_FLAG: tty_code_type = 3;
pub(super) const TTYCODE_NUMBER: tty_code_type = 2;
pub(super) const TTYCODE_STRING: tty_code_type = 1;

#[derive(Copy, Clone)]
#[repr(C)]
pub(super) struct tty_term_code_entry {
    pub type_0: tty_code_type,
    pub name: &'static ::std::ffi::CStr,
}
pub(super) const OK: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
#[derive(Clone)]
struct TerminalRegistration {
    client: ClientWeak,
    identity: std::rc::Weak<()>,
}

static mut tty_terms: Vec<TerminalRegistration> = Vec::new();

pub(crate) struct TerminalDescription {
    pub name: CString,
    pub client_name: Option<CString>,
    pub flags: i32,
    pub codes: Vec<CString>,
}

pub(crate) unsafe fn tty_term_descriptions(
    target: Option<&ClientRef>,
) -> impl Iterator<Item = TerminalDescription> {
    let target = target.map(std::rc::Rc::downgrade);
    tty_terms
        .clone()
        .into_iter()
        .filter_map(move |registration| {
            if target
                .as_ref()
                .is_some_and(|target| !target.ptr_eq(&registration.client))
            {
                return None;
            }
            let owner = registration.client.upgrade()?;
            let client_name = owner.name();
            let terminal = owner.borrow_terminal();
            let term = terminal.term.as_deref()?;
            if !registration
                .identity
                .ptr_eq(&std::rc::Rc::downgrade(&term.identity))
            {
                return None;
            }
            Some(TerminalDescription {
                name: term.name.clone(),
                client_name,
                flags: term.flags,
                codes: (0..tty_term_ncodes())
                    .map(|code| tty_term_describe(term, code))
                    .collect(),
            })
        })
}

static mut tty_term_codes: [tty_term_code_entry; 236] = [
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"acsc",
    },
    tty_term_code_entry {
        type_0: TTYCODE_FLAG,
        name: c"am",
    },
    tty_term_code_entry {
        type_0: TTYCODE_FLAG,
        name: c"AX",
    },
    tty_term_code_entry {
        type_0: TTYCODE_FLAG,
        name: c"bce",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"bel",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"Bidi",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"blink",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"bold",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"civis",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"clear",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"Clmg",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"Cmg",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"cnorm",
    },
    tty_term_code_entry {
        type_0: TTYCODE_NUMBER,
        name: c"colors",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"Cr",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"Cs",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"csr",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"cub",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"cub1",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"cud",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"cud1",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"cuf",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"cuf1",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"cup",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"cuu",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"cuu1",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"cvvis",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"dch",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"dch1",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"dim",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"dl",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"dl1",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"Dsbp",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"Dseks",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"Dsesc",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"Dsfcs",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"Dsmg",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"E3",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"ech",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"ed",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"el",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"el1",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"enacs",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"Enbp",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"Eneks",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"Enesc",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"Enfcs",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"Enmg",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"fsl",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"Hls",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"home",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"hpa",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"ich",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"ich1",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"il",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"il1",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"ind",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"indn",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"invis",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kcbt",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kcub1",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kcud1",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kcuf1",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kcuu1",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kDC",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kDC3",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kDC4",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kDC5",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kDC6",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kDC7",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kdch1",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kDN",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kDN3",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kDN4",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kDN5",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kDN6",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kDN7",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kend",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kEND",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kEND3",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kEND4",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kEND5",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kEND6",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kEND7",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kf1",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kf10",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kf11",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kf12",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kf13",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kf14",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kf15",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kf16",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kf17",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kf18",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kf19",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kf2",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kf20",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kf21",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kf22",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kf23",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kf24",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kf25",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kf26",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kf27",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kf28",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kf29",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kf3",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kf30",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kf31",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kf32",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kf33",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kf34",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kf35",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kf36",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kf37",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kf38",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kf39",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kf4",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kf40",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kf41",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kf42",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kf43",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kf44",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kf45",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kf46",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kf47",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kf48",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kf49",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kf5",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kf50",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kf51",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kf52",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kf53",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kf54",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kf55",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kf56",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kf57",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kf58",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kf59",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kf6",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kf60",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kf61",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kf62",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kf63",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kf7",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kf8",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kf9",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kHOM",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kHOM3",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kHOM4",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kHOM5",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kHOM6",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kHOM7",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"khome",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kIC",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kIC3",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kIC4",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kIC5",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kIC6",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kIC7",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kich1",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kind",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kLFT",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kLFT3",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kLFT4",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kLFT5",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kLFT6",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kLFT7",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kmous",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"knp",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kNXT",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kNXT3",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kNXT4",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kNXT5",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kNXT6",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kNXT7",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kpp",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kPRV",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kPRV3",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kPRV4",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kPRV5",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kPRV6",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kPRV7",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kri",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kRIT",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kRIT3",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kRIT4",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kRIT5",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kRIT6",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kRIT7",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kUP",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kUP3",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kUP4",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kUP5",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kUP6",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"kUP7",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"Ms",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"Nobr",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"ol",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"op",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"Rect",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"rev",
    },
    tty_term_code_entry {
        type_0: TTYCODE_FLAG,
        name: c"RGB",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"ri",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"rin",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"rmacs",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"rmcup",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"rmkx",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"Se",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"setab",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"setaf",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"setal",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"setrgbb",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"setrgbf",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"Setulc",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"Setulc1",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"sgr0",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"sitm",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"smacs",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"smcup",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"smkx",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"Smol",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"smso",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"smul",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"Smulx",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"smxx",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"Spb",
    },
    tty_term_code_entry {
        type_0: TTYCODE_FLAG,
        name: c"Sxl",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"Ss",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"Swd",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"Sync",
    },
    tty_term_code_entry {
        type_0: TTYCODE_FLAG,
        name: c"Tc",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"tsl",
    },
    tty_term_code_entry {
        type_0: TTYCODE_NUMBER,
        name: c"U8",
    },
    tty_term_code_entry {
        type_0: TTYCODE_STRING,
        name: c"vpa",
    },
    tty_term_code_entry {
        type_0: TTYCODE_FLAG,
        name: c"XT",
    },
];
pub(super) fn tty_term_ncodes() -> u_int {
    (::core::mem::size_of::<[tty_term_code_entry; 236]>() as usize)
        .wrapping_div(::core::mem::size_of::<tty_term_code_entry>() as usize) as u_int
}
fn tty_term_strip(s: &CStr) -> CString {
    let bytes = s.to_bytes();
    if !bytes.contains(&b'$') {
        return s.to_owned();
    }
    let mut stripped = Vec::with_capacity(8191);
    let mut offset = 0;
    while offset < bytes.len() {
        if bytes[offset] == b'$' && bytes.get(offset + 1) == Some(&b'<') {
            while offset < bytes.len() && bytes[offset] != b'>' {
                offset += 1;
            }
            if offset < bytes.len() {
                offset += 1;
            }
            if offset == bytes.len() {
                break;
            }
        }
        stripped.push(bytes[offset]);
        if stripped.len() == 8191 {
            break;
        }
        offset += 1;
    }
    CString::new(stripped).expect("terminal capability C string contains no NUL")
}
unsafe fn tty_term_override_next(
    mut s: *const ::core::ffi::c_char,
    mut offset: *mut size_t,
) -> Option<CString> {
    let mut value = Vec::new();
    let mut n: size_t = 0 as size_t;
    let mut at: size_t = *offset;
    if *s.add(at) as ::core::ffi::c_int == '\0' as i32 {
        return None;
    }
    while *s.add(at) as ::core::ffi::c_int != '\0' as i32 {
        if *s.add(at) as ::core::ffi::c_int == ':' as i32 {
            if !(*s.add(at.wrapping_add(1 as size_t)) as ::core::ffi::c_int == ':' as i32) {
                break;
            }
            n = n.wrapping_add(1);
            value.push(b':');
            at = at.wrapping_add(2 as size_t);
        } else {
            n = n.wrapping_add(1);
            value.push(*s.add(at) as u8);
            at = at.wrapping_add(1);
        }
        if n == (::core::mem::size_of::<[::core::ffi::c_char; 8192]>() as usize)
            .wrapping_sub(1_usize)
        {
            return None;
        }
    }
    if *s.add(at) as ::core::ffi::c_int != '\0' as i32 {
        *offset = at.wrapping_add(1 as size_t);
    } else {
        *offset = at;
    }
    Some(CString::new(value).expect("override tokens are read up to the NUL"))
}
unsafe fn tty_term_override_value(source: &CStr) -> CString {
    let mut decoded = source.to_bytes_with_nul().to_vec();
    if strunvis(decoded.as_mut_ptr().cast(), source.as_ptr()) == -1 {
        return source.to_owned();
    }
    decoded.truncate(strlen(decoded.as_ptr().cast()) + 1);
    CString::from_vec_with_nul(decoded).expect("strunvis terminates its output")
}
pub(super) unsafe fn tty_term_apply(
    mut term: *mut tty_term,
    mut capabilities: *const ::core::ffi::c_char,
    mut quiet: ::core::ffi::c_int,
    flags: ::core::ffi::c_int,
) {
    let mut offset: size_t = 0 as size_t;
    let mut i: u_int = 0;
    let mut n: ::core::ffi::c_int = 0;
    while let Some(token) = tty_term_override_next(capabilities, &raw mut offset) {
        if token.is_empty() {
            continue;
        }
        let (s, value) = if let Some(at) = token.as_bytes().iter().position(|&b| b == b'=') {
            let value = CStr::from_bytes_with_nul(&token.as_bytes_with_nul()[at + 1..])
                .expect("a suffix of a C string is NUL-terminated");
            (
                &token.as_bytes()[..at],
                Some(tty_term_override_value(value)),
            )
        } else if let Some(s) = token.as_bytes().strip_suffix(b"@") {
            (s, None)
        } else {
            (token.as_bytes(), Some(CString::default()))
        };
        if quiet == 0 {
            match &value {
                None => log_debug(format_args!(
                    "{} override: {}@",
                    log_cstr(&(*term).name),
                    log_bytes(s)
                )),
                Some(value) if value.is_empty() => log_debug(format_args!(
                    "{} override: {}",
                    log_cstr(&(*term).name),
                    log_bytes(s)
                )),
                Some(value) => log_debug(format_args!(
                    "{} override: {}={}",
                    log_cstr(&(*term).name),
                    log_bytes(s),
                    log_cstr(value)
                )),
            }
        }
        i = 0 as u_int;
        while i < tty_term_ncodes() {
            let ent = &tty_term_codes[i as usize];
            if s == ent.name.to_bytes()
                && !(flags & TERM_NOREPLACE != 0
                    && !matches!((&(*term).codes)[i as usize], tty_code::None))
            {
                match &value {
                    None => {
                        (&mut (*term).codes)[i as usize] = tty_code::None;
                    }
                    Some(value) => match ent.type_0 as ::core::ffi::c_uint {
                        1 => {
                            (&mut (*term).codes)[i as usize] = tty_code::String(value.clone());
                        }
                        2 => {
                            let parsed = strtonum(
                                value,
                                0 as ::core::ffi::c_longlong,
                                INT_MAX as ::core::ffi::c_longlong,
                            );
                            n = parsed.unwrap_or(0) as ::core::ffi::c_int;
                            if parsed.is_ok() {
                                (&mut (*term).codes)[i as usize] = tty_code::Number(n);
                            }
                        }
                        3 => {
                            (&mut (*term).codes)[i as usize] = tty_code::Flag(1);
                        }
                        _ => {}
                    },
                }
            }
            i = i.wrapping_add(1);
        }
    }
}
/// Copy out a global array option's string values; the loops that use them
/// apply terminal changes which may read options again.
unsafe fn global_array_strings(name: &CStr) -> Vec<CString> {
    OptionsScope::GlobalServer
        .with_entry(name, |entry| {
            entry
                .value
                .array()
                .into_iter()
                .flat_map(|array| array.iter())
                .filter_map(|item| item.value.string_ptr().map(CStr::to_owned))
                .collect()
        })
        .unwrap_or_default()
}
pub(super) unsafe fn tty_term_apply_overrides(mut term: *mut tty_term) {
    let values = global_array_strings(c"terminal-overrides");
    for value in &values {
        let mut offset: size_t = 0;
        let first = tty_term_override_next(value.as_ptr(), &raw mut offset);
        if first.as_ref().is_some_and(|first| {
            fnmatch(
                first.as_ptr(),
                ((*term).name).as_ptr().cast_mut(),
                0 as ::core::ffi::c_int,
            ) == 0
        }) {
            tty_term_apply(
                term,
                cstr_suffix(value, offset).as_ptr(),
                0 as ::core::ffi::c_int,
                0,
            );
        }
    }
    log_debug(format_args!(
        "SIXEL flag is {}",
        ((*term).flags & TERM_SIXEL != 0) as ::core::ffi::c_int
    ));
    if tty_term_has(&*term, TTYC_SETRGBF) != 0 && tty_term_has(&*term, TTYC_SETRGBB) != 0 {
        (*term).flags |= TERM_RGBCOLOURS;
    } else {
        (*term).flags &= !TERM_RGBCOLOURS;
    }
    log_debug(format_args!(
        "RGBCOLOURS flag is {}",
        ((*term).flags & TERM_RGBCOLOURS != 0) as ::core::ffi::c_int
    ));
    if tty_term_has(&*term, TTYC_CMG) != 0 && tty_term_has(&*term, TTYC_CLMG) != 0 {
        (*term).flags |= TERM_DECSLRM;
    } else {
        (*term).flags &= !TERM_DECSLRM;
    }
    log_debug(format_args!(
        "DECSLRM flag is {}",
        ((*term).flags & TERM_DECSLRM != 0) as ::core::ffi::c_int
    ));
    if tty_term_has(&*term, TTYC_RECT) != 0 {
        (*term).flags |= TERM_DECFRA;
    } else {
        (*term).flags &= !TERM_DECFRA;
    }
    log_debug(format_args!(
        "DECFRA flag is {}",
        ((*term).flags & TERM_DECFRA != 0) as ::core::ffi::c_int
    ));
    if tty_term_flag(&*term, TTYC_AM) == 0 {
        (*term).flags |= TERM_NOAM;
    } else {
        (*term).flags &= !TERM_NOAM;
    }
    log_debug(format_args!(
        "NOAM flag is {}",
        ((*term).flags & TERM_NOAM != 0) as ::core::ffi::c_int
    ));
    (*term).acs.fill([0; 2]);
    let acs = if tty_term_has(&*term, TTYC_ACSC) != 0 {
        tty_term_string(&*(term), TTYC_ACSC)
    } else {
        c"a#j+k+l+m+n+o-p-q-r-s-t+u+v+w+x|y<z>~."
    };
    for pair in acs.to_bytes().as_chunks::<2>().0 {
        (*term).acs[pair[0] as usize][0] = pair[1];
    }
    tty_term_validate(term);
}
unsafe fn tty_term_validate(mut term: *mut tty_term) {
    if !matches!((&(*term).codes)[TTYC_MS as usize], tty_code::String(_)) {
        return;
    }
    let ms = tty_term_string_ss(&*term, TTYC_MS, c"c", c"?");
    if !ms.as_bytes().is_empty() {
        (*term).flags &= !TERM_INVALIDMS;
        return;
    }
    log_debug(format_args!("removing invalid Ms capability"));
    (*term).flags |= TERM_INVALIDMS;
    (&mut (*term).codes)[TTYC_MS as usize] = tty_code::None;
}
pub(super) unsafe fn tty_term_create(
    client_owner: &ClientRef,
    name: &CStr,
    caps: &[CString],
) -> Result<Box<tty_term>, CString> {
    let mut term: *mut tty_term = ::core::ptr::null_mut::<tty_term>();
    let mut i: u_int = 0;
    log_debug(format_args!("adding term {}", log_cstr(name)));
    let mut owner = Box::new(tty_term {
        name: name.to_owned(),
        applied_features: 0,
        acs: [[0; 2]; 256],
        codes: vec![tty_code::default(); tty_term_ncodes() as usize].into_boxed_slice(),
        flags: 0,
        identity: std::rc::Rc::new(()),
        registered: true,
    });
    tty_terms.insert(
        0,
        TerminalRegistration {
            client: std::rc::Rc::downgrade(client_owner),
            identity: std::rc::Rc::downgrade(&owner.identity),
        },
    );
    term = &raw mut *owner;
    for cap in caps {
        // Capabilities come from the client as "name=value"; ignore anything else.
        let Some(namelen) = cap.to_bytes().iter().position(|&b| b == b'=') else {
            continue;
        };
        if namelen == 0 {
            continue;
        }
        let cap_name = &cap.to_bytes()[..namelen];
        let value = cstr_suffix(cap, namelen + 1);
        for j in 0..tty_term_ncodes() as usize {
            let ent = &tty_term_codes[j];
            if ent.name.to_bytes() != cap_name {
                continue;
            }
            (&mut (*term).codes)[j] = tty_code::None;
            match ent.type_0 as ::core::ffi::c_uint {
                1 => {
                    (&mut (*term).codes)[j] = tty_code::String(tty_term_strip(value));
                }
                2 => match strtonum(
                    value,
                    0 as ::core::ffi::c_longlong,
                    INT_MAX as ::core::ffi::c_longlong,
                ) {
                    Ok(n) => (&mut (*term).codes)[j] = tty_code::Number(n as ::core::ffi::c_int),
                    Err(errstr) => log_debug(format_args!(
                        "{}: {}",
                        crate::log::log_bytes(ent.name.to_bytes()),
                        log_cstr(errstr)
                    )),
                },
                3 => {
                    (&mut (*term).codes)[j] = tty_code::Flag(
                        (value.to_bytes().first() == Some(&b'1')) as ::core::ffi::c_int,
                    );
                }
                _ => {}
            }
        }
    }
    let values = global_array_strings(c"terminal-features");
    for value in &values {
        let mut offset: size_t = 0;
        let first = tty_term_override_next(value.as_ptr(), &raw mut offset);
        if first.as_ref().is_some_and(|first| {
            fnmatch(
                first.as_ptr(),
                ((*term).name).as_ptr().cast_mut(),
                0 as ::core::ffi::c_int,
            ) == 0
        }) {
            tty_parse_client_features(client_owner, cstr_suffix(value, offset), c":");
        }
    }
    del_curterm(cur_term);
    let colour_term = client_owner.with_environment(|env| {
        env.and_then(|env| env.find(c"COLORTERM"))
            .map(|entry| entry.value.clone())
    });
    if let Some(value) = colour_term {
        let name = client_owner.name();
        log_debug(format_args!(
            "{} COLORTERM={}",
            log_cstr(name.as_deref().unwrap_or(c"(null)")),
            log_cstr(value.as_deref().unwrap_or(c"(null)"))
        ));
        if let Some(value) = value {
            if value.to_bytes().eq_ignore_ascii_case(b"truecolor")
                || value.to_bytes().eq_ignore_ascii_case(b"24bit")
            {
                client_owner.parse_terminal_features(c"RGB", c",");
            } else if value.to_bytes().windows(3).any(|part| part == b"256") {
                client_owner.parse_terminal_features(c"256", c",");
            }
        }
    }
    tty_term_apply_overrides(term);
    let error = if tty_term_has(&*term, TTYC_CLEAR) == 0 {
        Some(c"terminal does not support clear".to_owned())
    } else if tty_term_has(&*term, TTYC_CUP) == 0 {
        Some(c"terminal does not support cup".to_owned())
    } else {
        if tty_term_flag(&*term, TTYC_XT) != 0
            || tty_term_string(&*term, TTYC_CLEAR)
                .to_bytes()
                .starts_with(b"\x1B[")
        {
            (*term).flags |= TERM_VT100LIKE;
            tty_parse_client_features(client_owner, c"bpaste,focus,title", c",");
        }
        if (tty_term_flag(&*term, TTYC_TC) != 0 || tty_term_has(&*term, TTYC_RGB) != 0)
            && (tty_term_has(&*term, TTYC_SETRGBF) == 0 || tty_term_has(&*term, TTYC_SETRGBB) == 0)
        {
            tty_parse_client_features(client_owner, c"RGB", c",");
        }
        let applied = tty_apply_features(term, client_owner.terminal_feature_mask());
        if applied.enable_utf8 {
            client_owner.update_flags(crate::shared::client::CLIENT_UTF8 as u64, 0);
        }
        if applied.changed {
            tty_term_apply_overrides(term);
        }
        i = 0 as u_int;
        while i < tty_term_ncodes() {
            log_debug(format_args!(
                "{}{}",
                log_cstr(name),
                log_cstr(&tty_term_describe(&*term, i as tty_code_code))
            ));
            i = i.wrapping_add(1);
        }
        return Ok(owner);
    };
    drop(owner);
    Err(error.expect("unsupported terminal has an error message"))
}

impl Drop for tty_term {
    fn drop(&mut self) {
        if !self.registered {
            return;
        }
        unsafe {
            log_debug(format_args!(
                "removing term {}",
                crate::log::log_bytes(self.name.as_bytes())
            ));
            let identity = std::rc::Rc::downgrade(&self.identity);
            tty_terms.retain(|entry| !entry.identity.ptr_eq(&identity));
        }
    }
}

pub(crate) unsafe fn tty_term_read_list(name: &CStr) -> Result<Vec<CString>, CString> {
    let fd: ::core::ffi::c_int = STDIN_FILENO;
    let mut error: ::core::ffi::c_int = 0;
    let mut n: ::core::ffi::c_int = 0;
    let mut i: u_int = 0;
    let mut s: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    let mut number = CString::default();
    let mut caps = Vec::new();
    if setupterm(name.as_ptr().cast_mut(), fd, &raw mut error) != OK {
        let (message, with_name): (&[u8], bool) = match error {
            1 => (b"can't use hardcopy terminal: ", true),
            0 => (b"missing or unsuitable terminal: ", true),
            -1 => (b"can't find terminfo database", false),
            _ => (b"unknown error", false),
        };
        let mut cause = message.to_vec();
        if with_name {
            cause.extend_from_slice(name.to_bytes());
        }
        return Err(CString::new(cause).expect("C strings contain no interior NUL"));
    }
    let terminal = cur_term;
    let mut current_block_23: u64;
    i = 0 as u_int;
    while i < tty_term_ncodes() {
        let ent = &tty_term_codes[i as usize];
        match ent.type_0 as ::core::ffi::c_uint {
            0 => {
                current_block_23 = 1856101646708284338;
            }
            1 => {
                s = tigetstr(ent.name.as_ptr().cast_mut());
                if s.is_null()
                    || std::ptr::eq(s, -(1 as ::core::ffi::c_int) as *mut ::core::ffi::c_char)
                {
                    current_block_23 = 1856101646708284338;
                } else {
                    current_block_23 = 14763689060501151050;
                }
            }
            2 => {
                n = tigetnum(ent.name.as_ptr().cast_mut());
                if n == -(1 as ::core::ffi::c_int) || n == -(2 as ::core::ffi::c_int) {
                    current_block_23 = 1856101646708284338;
                } else {
                    number = format_message_with(|out| write!(out, "{}", n));
                    s = number.as_ptr();
                    current_block_23 = 14763689060501151050;
                }
            }
            3 => {
                n = tigetflag(ent.name.as_ptr().cast_mut());
                if n == -(1 as ::core::ffi::c_int) {
                    current_block_23 = 1856101646708284338;
                } else {
                    if n != 0 {
                        s = c"1".as_ptr();
                    } else {
                        s = c"0".as_ptr();
                    }
                    current_block_23 = 14763689060501151050;
                }
            }
            _ => {
                fatalx(|out| out.write_all(b"unknown capability type"));
            }
        }
        if current_block_23 == 14763689060501151050 {
            let name = ent.name.to_bytes();
            let value = CStr::from_ptr(s).to_bytes();
            let mut cap = Vec::with_capacity(name.len() + 1 + value.len());
            cap.extend_from_slice(name);
            cap.push(b'=');
            cap.extend_from_slice(value);
            caps.push(CString::new(cap).expect("C strings contain no interior NUL"));
        }
        i = i.wrapping_add(1);
    }
    del_curterm(terminal);
    Ok(caps)
}

/// Read-only legacy projection; ownership stays with the TTY.
pub(super) fn tty_term_owner_ptr(owner: &Option<Box<tty_term>>) -> Option<&tty_term> {
    owner.as_deref()
}

pub(super) fn tty_term_has(term: &tty_term, mut code: tty_code_code) -> ::core::ffi::c_int {
    (!matches!((&term.codes)[code as usize], tty_code::None)) as ::core::ffi::c_int
}
pub(super) unsafe fn tty_term_has_name(term: &tty_term, name: &CStr) -> ::core::ffi::c_int {
    let mut i: u_int = 0;
    i = 0 as u_int;
    while i < tty_term_ncodes() {
        if strcmp(tty_term_codes[i as usize].name.as_ptr(), name.as_ptr())
            == 0 as ::core::ffi::c_int
        {
            return tty_term_has(term, i as tty_code_code);
        }
        i = i.wrapping_add(1);
    }
    0 as ::core::ffi::c_int
}
pub(super) fn tty_term_string(term: &tty_term, code: tty_code_code) -> &std::ffi::CStr {
    match &term.codes[code as usize] {
        tty_code::None => c"",
        tty_code::String(value) => value.as_c_str(),
        _ => fatalx(|out| write!(out, "not a string: {}", (code) as i32)),
    }
}
unsafe fn tty_term_expanded(code: tty_code_code, s: *const ::core::ffi::c_char) -> CString {
    if s.is_null() {
        log_debug(format_args!(
            "could not expand {}",
            log_bytes(tty_term_codes[code as usize].name.to_bytes())
        ));
        return CString::default();
    }
    CStr::from_ptr(s).to_owned()
}
pub(super) unsafe fn tty_term_string_i(
    term: &tty_term,
    mut code: tty_code_code,
    mut a: ::core::ffi::c_int,
) -> std::ffi::CString {
    let mut x: *const ::core::ffi::c_char = tty_term_string(term, code).as_ptr();
    let mut s: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    s = tiparm_s(1 as ::core::ffi::c_int, 0 as ::core::ffi::c_int, x, a);
    tty_term_expanded(code, s)
}
pub(super) unsafe fn tty_term_string_ii(
    term: &tty_term,
    mut code: tty_code_code,
    mut a: ::core::ffi::c_int,
    mut b: ::core::ffi::c_int,
) -> std::ffi::CString {
    let mut x: *const ::core::ffi::c_char = tty_term_string(term, code).as_ptr();
    let mut s: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    s = tiparm_s(2 as ::core::ffi::c_int, 0 as ::core::ffi::c_int, x, a, b);
    tty_term_expanded(code, s)
}
pub(super) unsafe fn tty_term_string_iii(
    term: &tty_term,
    mut code: tty_code_code,
    mut a: ::core::ffi::c_int,
    mut b: ::core::ffi::c_int,
    mut c: ::core::ffi::c_int,
) -> std::ffi::CString {
    let mut x: *const ::core::ffi::c_char = tty_term_string(term, code).as_ptr();
    let mut s: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    s = tiparm_s(3 as ::core::ffi::c_int, 0 as ::core::ffi::c_int, x, a, b, c);
    tty_term_expanded(code, s)
}
pub(super) unsafe fn tty_term_string_s(
    term: &tty_term,
    mut code: tty_code_code,
    a: &CStr,
) -> std::ffi::CString {
    let mut x: *const ::core::ffi::c_char = tty_term_string(term, code).as_ptr();
    let mut s: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    s = tiparm_s(
        1 as ::core::ffi::c_int,
        1 as ::core::ffi::c_int,
        x,
        a.as_ptr(),
    );
    tty_term_expanded(code, s)
}
pub(super) unsafe fn tty_term_string_ss(
    term: &tty_term,
    mut code: tty_code_code,
    a: &CStr,
    b: &CStr,
) -> std::ffi::CString {
    let mut x: *const ::core::ffi::c_char = tty_term_string(term, code).as_ptr();
    let mut s: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    s = tiparm_s(
        2 as ::core::ffi::c_int,
        3 as ::core::ffi::c_int,
        x,
        a.as_ptr(),
        b.as_ptr(),
    );
    tty_term_expanded(code, s)
}
pub(super) fn tty_term_number(term: &tty_term, code: tty_code_code) -> ::core::ffi::c_int {
    match &(&term.codes)[code as usize] {
        tty_code::None => 0,
        tty_code::Number(value) => *value,
        _ => fatalx(|out| write!(out, "not a number: {}", (code) as i32)),
    }
}
pub(super) fn tty_term_flag(term: &tty_term, code: tty_code_code) -> ::core::ffi::c_int {
    match &(&term.codes)[code as usize] {
        tty_code::None => 0,
        tty_code::Flag(value) => *value,
        _ => fatalx(|out| write!(out, "not a flag: {}", (code) as i32)),
    }
}
pub(super) unsafe fn tty_term_describe(term: &tty_term, code: tty_code_code) -> CString {
    let mut escaped: [::core::ffi::c_char; 128] = [0; 128];
    match &(&term.codes)[code as usize] {
        tty_code::None => format_message_with(|out| {
            write!(out, "{:4}: ", { code })?;
            out.write_all(tty_term_codes[code as usize].name.to_bytes())?;
            out.write_all(b": [missing]")
        }),
        tty_code::String(value) => {
            strnvis(
                &raw mut escaped as *mut ::core::ffi::c_char,
                value.as_ptr(),
                ::core::mem::size_of::<[::core::ffi::c_char; 128]>() as size_t,
                VIS_OCTAL | VIS_CSTYLE | VIS_TAB | VIS_NL,
            );
            format_message_with(|out| {
                write!(out, "{:4}: ", { code })?;
                out.write_all(tty_term_codes[code as usize].name.to_bytes())?;
                out.write_all(b": (string) ")?;
                out.write_all(
                    CStr::from_bytes_until_nul(&escaped.map(|c| c as u8))
                        .expect("strnvis terminates its output")
                        .to_bytes(),
                )
            })
        }
        tty_code::Number(value) => format_message_with(|out| {
            write!(out, "{:4}: ", { code })?;
            out.write_all(tty_term_codes[code as usize].name.to_bytes())?;
            write!(out, ": (number) {}", { *value })
        }),
        tty_code::Flag(value) => format_message_with(|out| {
            write!(out, "{:4}: ", { code })?;
            out.write_all(tty_term_codes[code as usize].name.to_bytes())?;
            out.write_all(b": (flag) ")?;
            out.write_all(if *value != 0 { b"true" } else { b"false" })
        }),
    }
}

#[cfg(test)]
mod term_string_owner_tests {
    use super::*;

    #[test]
    fn override_tokens_keep_escaping_limits_and_independent_storage() {
        unsafe {
            let source = c":clear=left::right:bel=\xff";
            let mut offset = 0;
            let empty = tty_term_override_next(source.as_ptr(), &mut offset).unwrap();
            let first = tty_term_override_next(source.as_ptr(), &mut offset).unwrap();
            let second = tty_term_override_next(source.as_ptr(), &mut offset).unwrap();
            assert!(tty_term_override_next(source.as_ptr(), &mut offset).is_none());
            assert_eq!(empty.as_c_str(), c"");
            assert_eq!(first.as_c_str(), c"clear=left:right");
            assert_eq!(second.as_c_str(), c"bel=\xff");

            for (len, accepted) in [(8190, true), (8191, false), (8192, false)] {
                let source = CString::new(format!("x:{}:next", "a".repeat(len))).unwrap();
                let mut offset = 2;
                let token = tty_term_override_next(source.as_ptr(), &mut offset);
                assert_eq!(token.is_some(), accepted);
                if let Some(token) = token {
                    assert_eq!(token.as_bytes().len(), len);
                    assert_eq!(offset, len + 3);
                } else {
                    assert_eq!(offset, 2);
                }
            }
        }
    }

    #[test]
    fn strip_preserves_bytes_and_delay_path_limit() {
        assert_eq!(tty_term_strip(c"ab$<5>cd").as_bytes(), b"abcd");
        let high = CString::new(b"a\xff$<10>b".as_slice()).unwrap();
        assert_eq!(tty_term_strip(high.as_c_str()).as_bytes(), b"a\xffb");

        let mut long = b"$<1>".to_vec();
        long.extend(std::iter::repeat_n(b'x', 9000));
        let long = CString::new(long).unwrap();
        let stripped = tty_term_strip(long.as_c_str());
        assert_eq!(stripped.as_bytes().len(), 8191);
        assert!(stripped.as_bytes().iter().all(|byte| *byte == b'x'));
    }
}
