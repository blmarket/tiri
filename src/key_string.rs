use crate::src::ffi::libc::wctomb;
use crate::src::format::bytes::cstr_suffix;
use crate::src::shared::abi::*;
use crate::src::shared::control_character::{
    C0_ASC, C0_BEL, C0_BS, C0_CAN, C0_CR, C0_DC1, C0_DC2, C0_DC3, C0_DC4, C0_DLE, C0_EM, C0_ENQ,
    C0_EOT, C0_ESC, C0_ETB, C0_ETX, C0_FF, C0_FS, C0_GS, C0_HT, C0_LF, C0_NAK, C0_NUL, C0_RS,
    C0_SI, C0_SO, C0_SOH, C0_STX, C0_SUB, C0_SYN, C0_US, C0_VT,
};
use crate::src::shared::grid::*;
use crate::src::shared::key::*;
use crate::src::shared::utf8::wchar_t;
use crate::src::shared::utf8::*;
use crate::src::text::utf8::{
    utf8_append, utf8_from_data, utf8_fromcstr_vec, utf8_open, utf8_to_data,
};
use libc::{sscanf, strcasecmp};
use std::ffi::{CStr, CString};

#[derive(Copy, Clone)]
#[repr(C)]
pub struct C2RustUnnamed_1 {
    pub string: &'static CStr,
    pub key: key_code,
}
#[inline]
unsafe fn tolower(mut __c: ::core::ffi::c_int) -> ::core::ffi::c_int {
    if __c >= -(128 as ::core::ffi::c_int) && __c < 256 as ::core::ffi::c_int {
        ::libc::tolower(__c as ::core::ffi::c_int) as ::core::ffi::c_int
    } else {
        __c
    }
}
pub const MB_LEN_MAX: ::core::ffi::c_int = 16 as ::core::ffi::c_int;
static key_string_table: [C2RustUnnamed_1; 1379] = [
    C2RustUnnamed_1 {
        string: c"F1",
        key: KEYC_F1 as ::core::ffi::c_ulong as key_code | KEYC_IMPLIED_META,
    },
    C2RustUnnamed_1 {
        string: c"F2",
        key: KEYC_F2 as ::core::ffi::c_ulong as key_code | KEYC_IMPLIED_META,
    },
    C2RustUnnamed_1 {
        string: c"F3",
        key: KEYC_F3 as ::core::ffi::c_ulong as key_code | KEYC_IMPLIED_META,
    },
    C2RustUnnamed_1 {
        string: c"F4",
        key: KEYC_F4 as ::core::ffi::c_ulong as key_code | KEYC_IMPLIED_META,
    },
    C2RustUnnamed_1 {
        string: c"F5",
        key: KEYC_F5 as ::core::ffi::c_ulong as key_code | KEYC_IMPLIED_META,
    },
    C2RustUnnamed_1 {
        string: c"F6",
        key: KEYC_F6 as ::core::ffi::c_ulong as key_code | KEYC_IMPLIED_META,
    },
    C2RustUnnamed_1 {
        string: c"F7",
        key: KEYC_F7 as ::core::ffi::c_ulong as key_code | KEYC_IMPLIED_META,
    },
    C2RustUnnamed_1 {
        string: c"F8",
        key: KEYC_F8 as ::core::ffi::c_ulong as key_code | KEYC_IMPLIED_META,
    },
    C2RustUnnamed_1 {
        string: c"F9",
        key: KEYC_F9 as ::core::ffi::c_ulong as key_code | KEYC_IMPLIED_META,
    },
    C2RustUnnamed_1 {
        string: c"F10",
        key: KEYC_F10 as ::core::ffi::c_ulong as key_code | KEYC_IMPLIED_META,
    },
    C2RustUnnamed_1 {
        string: c"F11",
        key: KEYC_F11 as ::core::ffi::c_ulong as key_code | KEYC_IMPLIED_META,
    },
    C2RustUnnamed_1 {
        string: c"F12",
        key: KEYC_F12 as ::core::ffi::c_ulong as key_code | KEYC_IMPLIED_META,
    },
    C2RustUnnamed_1 {
        string: c"IC",
        key: KEYC_IC as ::core::ffi::c_ulong as key_code | KEYC_IMPLIED_META,
    },
    C2RustUnnamed_1 {
        string: c"Insert",
        key: KEYC_IC as ::core::ffi::c_ulong as key_code | KEYC_IMPLIED_META,
    },
    C2RustUnnamed_1 {
        string: c"DC",
        key: KEYC_DC as ::core::ffi::c_ulong as key_code | KEYC_IMPLIED_META,
    },
    C2RustUnnamed_1 {
        string: c"Delete",
        key: KEYC_DC as ::core::ffi::c_ulong as key_code | KEYC_IMPLIED_META,
    },
    C2RustUnnamed_1 {
        string: c"Home",
        key: KEYC_HOME as ::core::ffi::c_ulong as key_code | KEYC_IMPLIED_META,
    },
    C2RustUnnamed_1 {
        string: c"End",
        key: KEYC_END as ::core::ffi::c_ulong as key_code | KEYC_IMPLIED_META,
    },
    C2RustUnnamed_1 {
        string: c"NPage",
        key: KEYC_NPAGE as ::core::ffi::c_ulong as key_code | KEYC_IMPLIED_META,
    },
    C2RustUnnamed_1 {
        string: c"PageDown",
        key: KEYC_NPAGE as ::core::ffi::c_ulong as key_code | KEYC_IMPLIED_META,
    },
    C2RustUnnamed_1 {
        string: c"PgDn",
        key: KEYC_NPAGE as ::core::ffi::c_ulong as key_code | KEYC_IMPLIED_META,
    },
    C2RustUnnamed_1 {
        string: c"PPage",
        key: KEYC_PPAGE as ::core::ffi::c_ulong as key_code | KEYC_IMPLIED_META,
    },
    C2RustUnnamed_1 {
        string: c"PageUp",
        key: KEYC_PPAGE as ::core::ffi::c_ulong as key_code | KEYC_IMPLIED_META,
    },
    C2RustUnnamed_1 {
        string: c"PgUp",
        key: KEYC_PPAGE as ::core::ffi::c_ulong as key_code | KEYC_IMPLIED_META,
    },
    C2RustUnnamed_1 {
        string: c"BTab",
        key: KEYC_BTAB as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"Space",
        key: ' ' as i32 as key_code,
    },
    C2RustUnnamed_1 {
        string: c"BSpace",
        key: KEYC_BSPACE as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"[NUL]",
        key: C0_NUL as ::core::ffi::c_int as key_code,
    },
    C2RustUnnamed_1 {
        string: c"[SOH]",
        key: C0_SOH as ::core::ffi::c_int as key_code,
    },
    C2RustUnnamed_1 {
        string: c"[STX]",
        key: C0_STX as ::core::ffi::c_int as key_code,
    },
    C2RustUnnamed_1 {
        string: c"[ETX]",
        key: C0_ETX as ::core::ffi::c_int as key_code,
    },
    C2RustUnnamed_1 {
        string: c"[EOT]",
        key: C0_EOT as ::core::ffi::c_int as key_code,
    },
    C2RustUnnamed_1 {
        string: c"[ENQ]",
        key: C0_ENQ as ::core::ffi::c_int as key_code,
    },
    C2RustUnnamed_1 {
        string: c"[ASC]",
        key: C0_ASC as ::core::ffi::c_int as key_code,
    },
    C2RustUnnamed_1 {
        string: c"[BEL]",
        key: C0_BEL as ::core::ffi::c_int as key_code,
    },
    C2RustUnnamed_1 {
        string: c"[BS]",
        key: C0_BS as ::core::ffi::c_int as key_code,
    },
    C2RustUnnamed_1 {
        string: c"Tab",
        key: C0_HT as ::core::ffi::c_int as key_code,
    },
    C2RustUnnamed_1 {
        string: c"[LF]",
        key: C0_LF as ::core::ffi::c_int as key_code,
    },
    C2RustUnnamed_1 {
        string: c"[VT]",
        key: C0_VT as ::core::ffi::c_int as key_code,
    },
    C2RustUnnamed_1 {
        string: c"[FF]",
        key: C0_FF as ::core::ffi::c_int as key_code,
    },
    C2RustUnnamed_1 {
        string: c"Enter",
        key: C0_CR as ::core::ffi::c_int as key_code,
    },
    C2RustUnnamed_1 {
        string: c"[SO]",
        key: C0_SO as ::core::ffi::c_int as key_code,
    },
    C2RustUnnamed_1 {
        string: c"[SI]",
        key: C0_SI as ::core::ffi::c_int as key_code,
    },
    C2RustUnnamed_1 {
        string: c"[DLE]",
        key: C0_DLE as ::core::ffi::c_int as key_code,
    },
    C2RustUnnamed_1 {
        string: c"[DC1]",
        key: C0_DC1 as ::core::ffi::c_int as key_code,
    },
    C2RustUnnamed_1 {
        string: c"[DC2]",
        key: C0_DC2 as ::core::ffi::c_int as key_code,
    },
    C2RustUnnamed_1 {
        string: c"[DC3]",
        key: C0_DC3 as ::core::ffi::c_int as key_code,
    },
    C2RustUnnamed_1 {
        string: c"[DC4]",
        key: C0_DC4 as ::core::ffi::c_int as key_code,
    },
    C2RustUnnamed_1 {
        string: c"[NAK]",
        key: C0_NAK as ::core::ffi::c_int as key_code,
    },
    C2RustUnnamed_1 {
        string: c"[SYN]",
        key: C0_SYN as ::core::ffi::c_int as key_code,
    },
    C2RustUnnamed_1 {
        string: c"[ETB]",
        key: C0_ETB as ::core::ffi::c_int as key_code,
    },
    C2RustUnnamed_1 {
        string: c"[CAN]",
        key: C0_CAN as ::core::ffi::c_int as key_code,
    },
    C2RustUnnamed_1 {
        string: c"[EM]",
        key: C0_EM as ::core::ffi::c_int as key_code,
    },
    C2RustUnnamed_1 {
        string: c"[SUB]",
        key: C0_SUB as ::core::ffi::c_int as key_code,
    },
    C2RustUnnamed_1 {
        string: c"Escape",
        key: C0_ESC as ::core::ffi::c_int as key_code,
    },
    C2RustUnnamed_1 {
        string: c"[FS]",
        key: C0_FS as ::core::ffi::c_int as key_code,
    },
    C2RustUnnamed_1 {
        string: c"[GS]",
        key: C0_GS as ::core::ffi::c_int as key_code,
    },
    C2RustUnnamed_1 {
        string: c"[RS]",
        key: C0_RS as ::core::ffi::c_int as key_code,
    },
    C2RustUnnamed_1 {
        string: c"[US]",
        key: C0_US as ::core::ffi::c_int as key_code,
    },
    C2RustUnnamed_1 {
        string: c"Up",
        key: KEYC_UP as ::core::ffi::c_ulong as key_code | KEYC_CURSOR | KEYC_IMPLIED_META,
    },
    C2RustUnnamed_1 {
        string: c"Down",
        key: KEYC_DOWN as ::core::ffi::c_ulong as key_code | KEYC_CURSOR | KEYC_IMPLIED_META,
    },
    C2RustUnnamed_1 {
        string: c"Left",
        key: KEYC_LEFT as ::core::ffi::c_ulong as key_code | KEYC_CURSOR | KEYC_IMPLIED_META,
    },
    C2RustUnnamed_1 {
        string: c"Right",
        key: KEYC_RIGHT as ::core::ffi::c_ulong as key_code | KEYC_CURSOR | KEYC_IMPLIED_META,
    },
    C2RustUnnamed_1 {
        string: c"KP/",
        key: KEYC_KP_SLASH as ::core::ffi::c_ulong as key_code | KEYC_KEYPAD,
    },
    C2RustUnnamed_1 {
        string: c"KP*",
        key: KEYC_KP_STAR as ::core::ffi::c_ulong as key_code | KEYC_KEYPAD,
    },
    C2RustUnnamed_1 {
        string: c"KP-",
        key: KEYC_KP_MINUS as ::core::ffi::c_ulong as key_code | KEYC_KEYPAD,
    },
    C2RustUnnamed_1 {
        string: c"KP7",
        key: KEYC_KP_SEVEN as ::core::ffi::c_ulong as key_code | KEYC_KEYPAD,
    },
    C2RustUnnamed_1 {
        string: c"KP8",
        key: KEYC_KP_EIGHT as ::core::ffi::c_ulong as key_code | KEYC_KEYPAD,
    },
    C2RustUnnamed_1 {
        string: c"KP9",
        key: KEYC_KP_NINE as ::core::ffi::c_ulong as key_code | KEYC_KEYPAD,
    },
    C2RustUnnamed_1 {
        string: c"KP+",
        key: KEYC_KP_PLUS as ::core::ffi::c_ulong as key_code | KEYC_KEYPAD,
    },
    C2RustUnnamed_1 {
        string: c"KP4",
        key: KEYC_KP_FOUR as ::core::ffi::c_ulong as key_code | KEYC_KEYPAD,
    },
    C2RustUnnamed_1 {
        string: c"KP5",
        key: KEYC_KP_FIVE as ::core::ffi::c_ulong as key_code | KEYC_KEYPAD,
    },
    C2RustUnnamed_1 {
        string: c"KP6",
        key: KEYC_KP_SIX as ::core::ffi::c_ulong as key_code | KEYC_KEYPAD,
    },
    C2RustUnnamed_1 {
        string: c"KP1",
        key: KEYC_KP_ONE as ::core::ffi::c_ulong as key_code | KEYC_KEYPAD,
    },
    C2RustUnnamed_1 {
        string: c"KP2",
        key: KEYC_KP_TWO as ::core::ffi::c_ulong as key_code | KEYC_KEYPAD,
    },
    C2RustUnnamed_1 {
        string: c"KP3",
        key: KEYC_KP_THREE as ::core::ffi::c_ulong as key_code | KEYC_KEYPAD,
    },
    C2RustUnnamed_1 {
        string: c"KPEnter",
        key: KEYC_KP_ENTER as ::core::ffi::c_ulong as key_code | KEYC_KEYPAD,
    },
    C2RustUnnamed_1 {
        string: c"KP0",
        key: KEYC_KP_ZERO as ::core::ffi::c_ulong as key_code | KEYC_KEYPAD,
    },
    C2RustUnnamed_1 {
        string: c"KP.",
        key: KEYC_KP_PERIOD as ::core::ffi::c_ulong as key_code | KEYC_KEYPAD,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown1Pane",
        key: KEYC_MOUSEDOWN1_PANE as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown1Status",
        key: KEYC_MOUSEDOWN1_STATUS as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown1StatusLeft",
        key: KEYC_MOUSEDOWN1_STATUS_LEFT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown1StatusRight",
        key: KEYC_MOUSEDOWN1_STATUS_RIGHT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown1StatusDefault",
        key: KEYC_MOUSEDOWN1_STATUS_DEFAULT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown1ScrollbarUp",
        key: KEYC_MOUSEDOWN1_SCROLLBAR_UP as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown1ScrollbarSlider",
        key: KEYC_MOUSEDOWN1_SCROLLBAR_SLIDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown1ScrollbarDown",
        key: KEYC_MOUSEDOWN1_SCROLLBAR_DOWN as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown1Empty",
        key: KEYC_MOUSEDOWN1_EMPTY as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown1Border",
        key: KEYC_MOUSEDOWN1_BORDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown1Control0",
        key: KEYC_MOUSEDOWN1_CONTROL0 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown1Control1",
        key: KEYC_MOUSEDOWN1_CONTROL1 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown1Control2",
        key: KEYC_MOUSEDOWN1_CONTROL2 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown1Control3",
        key: KEYC_MOUSEDOWN1_CONTROL3 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown1Control4",
        key: KEYC_MOUSEDOWN1_CONTROL4 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown1Control5",
        key: KEYC_MOUSEDOWN1_CONTROL5 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown1Control6",
        key: KEYC_MOUSEDOWN1_CONTROL6 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown1Control7",
        key: KEYC_MOUSEDOWN1_CONTROL7 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown1Control8",
        key: KEYC_MOUSEDOWN1_CONTROL8 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown1Control9",
        key: KEYC_MOUSEDOWN1_CONTROL9 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown2Pane",
        key: KEYC_MOUSEDOWN2_PANE as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown2Status",
        key: KEYC_MOUSEDOWN2_STATUS as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown2StatusLeft",
        key: KEYC_MOUSEDOWN2_STATUS_LEFT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown2StatusRight",
        key: KEYC_MOUSEDOWN2_STATUS_RIGHT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown2StatusDefault",
        key: KEYC_MOUSEDOWN2_STATUS_DEFAULT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown2ScrollbarUp",
        key: KEYC_MOUSEDOWN2_SCROLLBAR_UP as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown2ScrollbarSlider",
        key: KEYC_MOUSEDOWN2_SCROLLBAR_SLIDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown2ScrollbarDown",
        key: KEYC_MOUSEDOWN2_SCROLLBAR_DOWN as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown2Empty",
        key: KEYC_MOUSEDOWN2_EMPTY as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown2Border",
        key: KEYC_MOUSEDOWN2_BORDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown2Control0",
        key: KEYC_MOUSEDOWN2_CONTROL0 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown2Control1",
        key: KEYC_MOUSEDOWN2_CONTROL1 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown2Control2",
        key: KEYC_MOUSEDOWN2_CONTROL2 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown2Control3",
        key: KEYC_MOUSEDOWN2_CONTROL3 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown2Control4",
        key: KEYC_MOUSEDOWN2_CONTROL4 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown2Control5",
        key: KEYC_MOUSEDOWN2_CONTROL5 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown2Control6",
        key: KEYC_MOUSEDOWN2_CONTROL6 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown2Control7",
        key: KEYC_MOUSEDOWN2_CONTROL7 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown2Control8",
        key: KEYC_MOUSEDOWN2_CONTROL8 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown2Control9",
        key: KEYC_MOUSEDOWN2_CONTROL9 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown3Pane",
        key: KEYC_MOUSEDOWN3_PANE as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown3Status",
        key: KEYC_MOUSEDOWN3_STATUS as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown3StatusLeft",
        key: KEYC_MOUSEDOWN3_STATUS_LEFT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown3StatusRight",
        key: KEYC_MOUSEDOWN3_STATUS_RIGHT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown3StatusDefault",
        key: KEYC_MOUSEDOWN3_STATUS_DEFAULT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown3ScrollbarUp",
        key: KEYC_MOUSEDOWN3_SCROLLBAR_UP as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown3ScrollbarSlider",
        key: KEYC_MOUSEDOWN3_SCROLLBAR_SLIDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown3ScrollbarDown",
        key: KEYC_MOUSEDOWN3_SCROLLBAR_DOWN as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown3Empty",
        key: KEYC_MOUSEDOWN3_EMPTY as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown3Border",
        key: KEYC_MOUSEDOWN3_BORDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown3Control0",
        key: KEYC_MOUSEDOWN3_CONTROL0 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown3Control1",
        key: KEYC_MOUSEDOWN3_CONTROL1 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown3Control2",
        key: KEYC_MOUSEDOWN3_CONTROL2 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown3Control3",
        key: KEYC_MOUSEDOWN3_CONTROL3 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown3Control4",
        key: KEYC_MOUSEDOWN3_CONTROL4 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown3Control5",
        key: KEYC_MOUSEDOWN3_CONTROL5 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown3Control6",
        key: KEYC_MOUSEDOWN3_CONTROL6 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown3Control7",
        key: KEYC_MOUSEDOWN3_CONTROL7 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown3Control8",
        key: KEYC_MOUSEDOWN3_CONTROL8 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown3Control9",
        key: KEYC_MOUSEDOWN3_CONTROL9 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown6Pane",
        key: KEYC_MOUSEDOWN6_PANE as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown6Status",
        key: KEYC_MOUSEDOWN6_STATUS as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown6StatusLeft",
        key: KEYC_MOUSEDOWN6_STATUS_LEFT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown6StatusRight",
        key: KEYC_MOUSEDOWN6_STATUS_RIGHT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown6StatusDefault",
        key: KEYC_MOUSEDOWN6_STATUS_DEFAULT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown6ScrollbarUp",
        key: KEYC_MOUSEDOWN6_SCROLLBAR_UP as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown6ScrollbarSlider",
        key: KEYC_MOUSEDOWN6_SCROLLBAR_SLIDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown6ScrollbarDown",
        key: KEYC_MOUSEDOWN6_SCROLLBAR_DOWN as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown6Empty",
        key: KEYC_MOUSEDOWN6_EMPTY as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown6Border",
        key: KEYC_MOUSEDOWN6_BORDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown6Control0",
        key: KEYC_MOUSEDOWN6_CONTROL0 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown6Control1",
        key: KEYC_MOUSEDOWN6_CONTROL1 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown6Control2",
        key: KEYC_MOUSEDOWN6_CONTROL2 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown6Control3",
        key: KEYC_MOUSEDOWN6_CONTROL3 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown6Control4",
        key: KEYC_MOUSEDOWN6_CONTROL4 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown6Control5",
        key: KEYC_MOUSEDOWN6_CONTROL5 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown6Control6",
        key: KEYC_MOUSEDOWN6_CONTROL6 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown6Control7",
        key: KEYC_MOUSEDOWN6_CONTROL7 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown6Control8",
        key: KEYC_MOUSEDOWN6_CONTROL8 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown6Control9",
        key: KEYC_MOUSEDOWN6_CONTROL9 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown7Pane",
        key: KEYC_MOUSEDOWN7_PANE as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown7Status",
        key: KEYC_MOUSEDOWN7_STATUS as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown7StatusLeft",
        key: KEYC_MOUSEDOWN7_STATUS_LEFT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown7StatusRight",
        key: KEYC_MOUSEDOWN7_STATUS_RIGHT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown7StatusDefault",
        key: KEYC_MOUSEDOWN7_STATUS_DEFAULT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown7ScrollbarUp",
        key: KEYC_MOUSEDOWN7_SCROLLBAR_UP as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown7ScrollbarSlider",
        key: KEYC_MOUSEDOWN7_SCROLLBAR_SLIDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown7ScrollbarDown",
        key: KEYC_MOUSEDOWN7_SCROLLBAR_DOWN as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown7Empty",
        key: KEYC_MOUSEDOWN7_EMPTY as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown7Border",
        key: KEYC_MOUSEDOWN7_BORDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown7Control0",
        key: KEYC_MOUSEDOWN7_CONTROL0 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown7Control1",
        key: KEYC_MOUSEDOWN7_CONTROL1 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown7Control2",
        key: KEYC_MOUSEDOWN7_CONTROL2 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown7Control3",
        key: KEYC_MOUSEDOWN7_CONTROL3 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown7Control4",
        key: KEYC_MOUSEDOWN7_CONTROL4 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown7Control5",
        key: KEYC_MOUSEDOWN7_CONTROL5 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown7Control6",
        key: KEYC_MOUSEDOWN7_CONTROL6 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown7Control7",
        key: KEYC_MOUSEDOWN7_CONTROL7 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown7Control8",
        key: KEYC_MOUSEDOWN7_CONTROL8 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown7Control9",
        key: KEYC_MOUSEDOWN7_CONTROL9 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown8Pane",
        key: KEYC_MOUSEDOWN8_PANE as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown8Status",
        key: KEYC_MOUSEDOWN8_STATUS as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown8StatusLeft",
        key: KEYC_MOUSEDOWN8_STATUS_LEFT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown8StatusRight",
        key: KEYC_MOUSEDOWN8_STATUS_RIGHT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown8StatusDefault",
        key: KEYC_MOUSEDOWN8_STATUS_DEFAULT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown8ScrollbarUp",
        key: KEYC_MOUSEDOWN8_SCROLLBAR_UP as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown8ScrollbarSlider",
        key: KEYC_MOUSEDOWN8_SCROLLBAR_SLIDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown8ScrollbarDown",
        key: KEYC_MOUSEDOWN8_SCROLLBAR_DOWN as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown8Empty",
        key: KEYC_MOUSEDOWN8_EMPTY as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown8Border",
        key: KEYC_MOUSEDOWN8_BORDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown8Control0",
        key: KEYC_MOUSEDOWN8_CONTROL0 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown8Control1",
        key: KEYC_MOUSEDOWN8_CONTROL1 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown8Control2",
        key: KEYC_MOUSEDOWN8_CONTROL2 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown8Control3",
        key: KEYC_MOUSEDOWN8_CONTROL3 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown8Control4",
        key: KEYC_MOUSEDOWN8_CONTROL4 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown8Control5",
        key: KEYC_MOUSEDOWN8_CONTROL5 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown8Control6",
        key: KEYC_MOUSEDOWN8_CONTROL6 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown8Control7",
        key: KEYC_MOUSEDOWN8_CONTROL7 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown8Control8",
        key: KEYC_MOUSEDOWN8_CONTROL8 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown8Control9",
        key: KEYC_MOUSEDOWN8_CONTROL9 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown9Pane",
        key: KEYC_MOUSEDOWN9_PANE as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown9Status",
        key: KEYC_MOUSEDOWN9_STATUS as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown9StatusLeft",
        key: KEYC_MOUSEDOWN9_STATUS_LEFT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown9StatusRight",
        key: KEYC_MOUSEDOWN9_STATUS_RIGHT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown9StatusDefault",
        key: KEYC_MOUSEDOWN9_STATUS_DEFAULT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown9ScrollbarUp",
        key: KEYC_MOUSEDOWN9_SCROLLBAR_UP as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown9ScrollbarSlider",
        key: KEYC_MOUSEDOWN9_SCROLLBAR_SLIDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown9ScrollbarDown",
        key: KEYC_MOUSEDOWN9_SCROLLBAR_DOWN as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown9Empty",
        key: KEYC_MOUSEDOWN9_EMPTY as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown9Border",
        key: KEYC_MOUSEDOWN9_BORDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown9Control0",
        key: KEYC_MOUSEDOWN9_CONTROL0 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown9Control1",
        key: KEYC_MOUSEDOWN9_CONTROL1 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown9Control2",
        key: KEYC_MOUSEDOWN9_CONTROL2 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown9Control3",
        key: KEYC_MOUSEDOWN9_CONTROL3 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown9Control4",
        key: KEYC_MOUSEDOWN9_CONTROL4 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown9Control5",
        key: KEYC_MOUSEDOWN9_CONTROL5 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown9Control6",
        key: KEYC_MOUSEDOWN9_CONTROL6 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown9Control7",
        key: KEYC_MOUSEDOWN9_CONTROL7 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown9Control8",
        key: KEYC_MOUSEDOWN9_CONTROL8 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown9Control9",
        key: KEYC_MOUSEDOWN9_CONTROL9 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown10Pane",
        key: KEYC_MOUSEDOWN10_PANE as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown10Status",
        key: KEYC_MOUSEDOWN10_STATUS as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown10StatusLeft",
        key: KEYC_MOUSEDOWN10_STATUS_LEFT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown10StatusRight",
        key: KEYC_MOUSEDOWN10_STATUS_RIGHT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown10StatusDefault",
        key: KEYC_MOUSEDOWN10_STATUS_DEFAULT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown10ScrollbarUp",
        key: KEYC_MOUSEDOWN10_SCROLLBAR_UP as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown10ScrollbarSlider",
        key: KEYC_MOUSEDOWN10_SCROLLBAR_SLIDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown10ScrollbarDown",
        key: KEYC_MOUSEDOWN10_SCROLLBAR_DOWN as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown10Empty",
        key: KEYC_MOUSEDOWN10_EMPTY as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown10Border",
        key: KEYC_MOUSEDOWN10_BORDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown10Control0",
        key: KEYC_MOUSEDOWN10_CONTROL0 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown10Control1",
        key: KEYC_MOUSEDOWN10_CONTROL1 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown10Control2",
        key: KEYC_MOUSEDOWN10_CONTROL2 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown10Control3",
        key: KEYC_MOUSEDOWN10_CONTROL3 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown10Control4",
        key: KEYC_MOUSEDOWN10_CONTROL4 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown10Control5",
        key: KEYC_MOUSEDOWN10_CONTROL5 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown10Control6",
        key: KEYC_MOUSEDOWN10_CONTROL6 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown10Control7",
        key: KEYC_MOUSEDOWN10_CONTROL7 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown10Control8",
        key: KEYC_MOUSEDOWN10_CONTROL8 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown10Control9",
        key: KEYC_MOUSEDOWN10_CONTROL9 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown11Pane",
        key: KEYC_MOUSEDOWN11_PANE as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown11Status",
        key: KEYC_MOUSEDOWN11_STATUS as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown11StatusLeft",
        key: KEYC_MOUSEDOWN11_STATUS_LEFT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown11StatusRight",
        key: KEYC_MOUSEDOWN11_STATUS_RIGHT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown11StatusDefault",
        key: KEYC_MOUSEDOWN11_STATUS_DEFAULT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown11ScrollbarUp",
        key: KEYC_MOUSEDOWN11_SCROLLBAR_UP as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown11ScrollbarSlider",
        key: KEYC_MOUSEDOWN11_SCROLLBAR_SLIDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown11ScrollbarDown",
        key: KEYC_MOUSEDOWN11_SCROLLBAR_DOWN as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown11Empty",
        key: KEYC_MOUSEDOWN11_EMPTY as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown11Border",
        key: KEYC_MOUSEDOWN11_BORDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown11Control0",
        key: KEYC_MOUSEDOWN11_CONTROL0 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown11Control1",
        key: KEYC_MOUSEDOWN11_CONTROL1 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown11Control2",
        key: KEYC_MOUSEDOWN11_CONTROL2 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown11Control3",
        key: KEYC_MOUSEDOWN11_CONTROL3 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown11Control4",
        key: KEYC_MOUSEDOWN11_CONTROL4 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown11Control5",
        key: KEYC_MOUSEDOWN11_CONTROL5 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown11Control6",
        key: KEYC_MOUSEDOWN11_CONTROL6 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown11Control7",
        key: KEYC_MOUSEDOWN11_CONTROL7 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown11Control8",
        key: KEYC_MOUSEDOWN11_CONTROL8 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDown11Control9",
        key: KEYC_MOUSEDOWN11_CONTROL9 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp1Pane",
        key: KEYC_MOUSEUP1_PANE as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp1Status",
        key: KEYC_MOUSEUP1_STATUS as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp1StatusLeft",
        key: KEYC_MOUSEUP1_STATUS_LEFT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp1StatusRight",
        key: KEYC_MOUSEUP1_STATUS_RIGHT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp1StatusDefault",
        key: KEYC_MOUSEUP1_STATUS_DEFAULT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp1ScrollbarUp",
        key: KEYC_MOUSEUP1_SCROLLBAR_UP as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp1ScrollbarSlider",
        key: KEYC_MOUSEUP1_SCROLLBAR_SLIDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp1ScrollbarDown",
        key: KEYC_MOUSEUP1_SCROLLBAR_DOWN as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp1Empty",
        key: KEYC_MOUSEUP1_EMPTY as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp1Border",
        key: KEYC_MOUSEUP1_BORDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp1Control0",
        key: KEYC_MOUSEUP1_CONTROL0 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp1Control1",
        key: KEYC_MOUSEUP1_CONTROL1 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp1Control2",
        key: KEYC_MOUSEUP1_CONTROL2 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp1Control3",
        key: KEYC_MOUSEUP1_CONTROL3 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp1Control4",
        key: KEYC_MOUSEUP1_CONTROL4 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp1Control5",
        key: KEYC_MOUSEUP1_CONTROL5 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp1Control6",
        key: KEYC_MOUSEUP1_CONTROL6 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp1Control7",
        key: KEYC_MOUSEUP1_CONTROL7 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp1Control8",
        key: KEYC_MOUSEUP1_CONTROL8 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp1Control9",
        key: KEYC_MOUSEUP1_CONTROL9 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp2Pane",
        key: KEYC_MOUSEUP2_PANE as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp2Status",
        key: KEYC_MOUSEUP2_STATUS as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp2StatusLeft",
        key: KEYC_MOUSEUP2_STATUS_LEFT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp2StatusRight",
        key: KEYC_MOUSEUP2_STATUS_RIGHT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp2StatusDefault",
        key: KEYC_MOUSEUP2_STATUS_DEFAULT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp2ScrollbarUp",
        key: KEYC_MOUSEUP2_SCROLLBAR_UP as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp2ScrollbarSlider",
        key: KEYC_MOUSEUP2_SCROLLBAR_SLIDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp2ScrollbarDown",
        key: KEYC_MOUSEUP2_SCROLLBAR_DOWN as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp2Empty",
        key: KEYC_MOUSEUP2_EMPTY as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp2Border",
        key: KEYC_MOUSEUP2_BORDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp2Control0",
        key: KEYC_MOUSEUP2_CONTROL0 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp2Control1",
        key: KEYC_MOUSEUP2_CONTROL1 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp2Control2",
        key: KEYC_MOUSEUP2_CONTROL2 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp2Control3",
        key: KEYC_MOUSEUP2_CONTROL3 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp2Control4",
        key: KEYC_MOUSEUP2_CONTROL4 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp2Control5",
        key: KEYC_MOUSEUP2_CONTROL5 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp2Control6",
        key: KEYC_MOUSEUP2_CONTROL6 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp2Control7",
        key: KEYC_MOUSEUP2_CONTROL7 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp2Control8",
        key: KEYC_MOUSEUP2_CONTROL8 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp2Control9",
        key: KEYC_MOUSEUP2_CONTROL9 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp3Pane",
        key: KEYC_MOUSEUP3_PANE as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp3Status",
        key: KEYC_MOUSEUP3_STATUS as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp3StatusLeft",
        key: KEYC_MOUSEUP3_STATUS_LEFT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp3StatusRight",
        key: KEYC_MOUSEUP3_STATUS_RIGHT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp3StatusDefault",
        key: KEYC_MOUSEUP3_STATUS_DEFAULT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp3ScrollbarUp",
        key: KEYC_MOUSEUP3_SCROLLBAR_UP as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp3ScrollbarSlider",
        key: KEYC_MOUSEUP3_SCROLLBAR_SLIDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp3ScrollbarDown",
        key: KEYC_MOUSEUP3_SCROLLBAR_DOWN as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp3Empty",
        key: KEYC_MOUSEUP3_EMPTY as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp3Border",
        key: KEYC_MOUSEUP3_BORDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp3Control0",
        key: KEYC_MOUSEUP3_CONTROL0 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp3Control1",
        key: KEYC_MOUSEUP3_CONTROL1 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp3Control2",
        key: KEYC_MOUSEUP3_CONTROL2 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp3Control3",
        key: KEYC_MOUSEUP3_CONTROL3 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp3Control4",
        key: KEYC_MOUSEUP3_CONTROL4 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp3Control5",
        key: KEYC_MOUSEUP3_CONTROL5 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp3Control6",
        key: KEYC_MOUSEUP3_CONTROL6 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp3Control7",
        key: KEYC_MOUSEUP3_CONTROL7 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp3Control8",
        key: KEYC_MOUSEUP3_CONTROL8 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp3Control9",
        key: KEYC_MOUSEUP3_CONTROL9 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp6Pane",
        key: KEYC_MOUSEUP6_PANE as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp6Status",
        key: KEYC_MOUSEUP6_STATUS as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp6StatusLeft",
        key: KEYC_MOUSEUP6_STATUS_LEFT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp6StatusRight",
        key: KEYC_MOUSEUP6_STATUS_RIGHT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp6StatusDefault",
        key: KEYC_MOUSEUP6_STATUS_DEFAULT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp6ScrollbarUp",
        key: KEYC_MOUSEUP6_SCROLLBAR_UP as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp6ScrollbarSlider",
        key: KEYC_MOUSEUP6_SCROLLBAR_SLIDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp6ScrollbarDown",
        key: KEYC_MOUSEUP6_SCROLLBAR_DOWN as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp6Empty",
        key: KEYC_MOUSEUP6_EMPTY as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp6Border",
        key: KEYC_MOUSEUP6_BORDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp6Control0",
        key: KEYC_MOUSEUP6_CONTROL0 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp6Control1",
        key: KEYC_MOUSEUP6_CONTROL1 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp6Control2",
        key: KEYC_MOUSEUP6_CONTROL2 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp6Control3",
        key: KEYC_MOUSEUP6_CONTROL3 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp6Control4",
        key: KEYC_MOUSEUP6_CONTROL4 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp6Control5",
        key: KEYC_MOUSEUP6_CONTROL5 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp6Control6",
        key: KEYC_MOUSEUP6_CONTROL6 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp6Control7",
        key: KEYC_MOUSEUP6_CONTROL7 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp6Control8",
        key: KEYC_MOUSEUP6_CONTROL8 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp6Control9",
        key: KEYC_MOUSEUP6_CONTROL9 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp7Pane",
        key: KEYC_MOUSEUP7_PANE as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp7Status",
        key: KEYC_MOUSEUP7_STATUS as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp7StatusLeft",
        key: KEYC_MOUSEUP7_STATUS_LEFT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp7StatusRight",
        key: KEYC_MOUSEUP7_STATUS_RIGHT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp7StatusDefault",
        key: KEYC_MOUSEUP7_STATUS_DEFAULT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp7ScrollbarUp",
        key: KEYC_MOUSEUP7_SCROLLBAR_UP as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp7ScrollbarSlider",
        key: KEYC_MOUSEUP7_SCROLLBAR_SLIDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp7ScrollbarDown",
        key: KEYC_MOUSEUP7_SCROLLBAR_DOWN as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp7Empty",
        key: KEYC_MOUSEUP7_EMPTY as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp7Border",
        key: KEYC_MOUSEUP7_BORDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp7Control0",
        key: KEYC_MOUSEUP7_CONTROL0 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp7Control1",
        key: KEYC_MOUSEUP7_CONTROL1 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp7Control2",
        key: KEYC_MOUSEUP7_CONTROL2 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp7Control3",
        key: KEYC_MOUSEUP7_CONTROL3 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp7Control4",
        key: KEYC_MOUSEUP7_CONTROL4 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp7Control5",
        key: KEYC_MOUSEUP7_CONTROL5 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp7Control6",
        key: KEYC_MOUSEUP7_CONTROL6 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp7Control7",
        key: KEYC_MOUSEUP7_CONTROL7 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp7Control8",
        key: KEYC_MOUSEUP7_CONTROL8 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp7Control9",
        key: KEYC_MOUSEUP7_CONTROL9 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp8Pane",
        key: KEYC_MOUSEUP8_PANE as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp8Status",
        key: KEYC_MOUSEUP8_STATUS as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp8StatusLeft",
        key: KEYC_MOUSEUP8_STATUS_LEFT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp8StatusRight",
        key: KEYC_MOUSEUP8_STATUS_RIGHT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp8StatusDefault",
        key: KEYC_MOUSEUP8_STATUS_DEFAULT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp8ScrollbarUp",
        key: KEYC_MOUSEUP8_SCROLLBAR_UP as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp8ScrollbarSlider",
        key: KEYC_MOUSEUP8_SCROLLBAR_SLIDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp8ScrollbarDown",
        key: KEYC_MOUSEUP8_SCROLLBAR_DOWN as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp8Empty",
        key: KEYC_MOUSEUP8_EMPTY as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp8Border",
        key: KEYC_MOUSEUP8_BORDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp8Control0",
        key: KEYC_MOUSEUP8_CONTROL0 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp8Control1",
        key: KEYC_MOUSEUP8_CONTROL1 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp8Control2",
        key: KEYC_MOUSEUP8_CONTROL2 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp8Control3",
        key: KEYC_MOUSEUP8_CONTROL3 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp8Control4",
        key: KEYC_MOUSEUP8_CONTROL4 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp8Control5",
        key: KEYC_MOUSEUP8_CONTROL5 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp8Control6",
        key: KEYC_MOUSEUP8_CONTROL6 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp8Control7",
        key: KEYC_MOUSEUP8_CONTROL7 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp8Control8",
        key: KEYC_MOUSEUP8_CONTROL8 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp8Control9",
        key: KEYC_MOUSEUP8_CONTROL9 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp9Pane",
        key: KEYC_MOUSEUP9_PANE as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp9Status",
        key: KEYC_MOUSEUP9_STATUS as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp9StatusLeft",
        key: KEYC_MOUSEUP9_STATUS_LEFT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp9StatusRight",
        key: KEYC_MOUSEUP9_STATUS_RIGHT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp9StatusDefault",
        key: KEYC_MOUSEUP9_STATUS_DEFAULT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp9ScrollbarUp",
        key: KEYC_MOUSEUP9_SCROLLBAR_UP as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp9ScrollbarSlider",
        key: KEYC_MOUSEUP9_SCROLLBAR_SLIDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp9ScrollbarDown",
        key: KEYC_MOUSEUP9_SCROLLBAR_DOWN as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp9Empty",
        key: KEYC_MOUSEUP9_EMPTY as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp9Border",
        key: KEYC_MOUSEUP9_BORDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp9Control0",
        key: KEYC_MOUSEUP9_CONTROL0 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp9Control1",
        key: KEYC_MOUSEUP9_CONTROL1 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp9Control2",
        key: KEYC_MOUSEUP9_CONTROL2 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp9Control3",
        key: KEYC_MOUSEUP9_CONTROL3 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp9Control4",
        key: KEYC_MOUSEUP9_CONTROL4 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp9Control5",
        key: KEYC_MOUSEUP9_CONTROL5 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp9Control6",
        key: KEYC_MOUSEUP9_CONTROL6 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp9Control7",
        key: KEYC_MOUSEUP9_CONTROL7 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp9Control8",
        key: KEYC_MOUSEUP9_CONTROL8 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp9Control9",
        key: KEYC_MOUSEUP9_CONTROL9 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp10Pane",
        key: KEYC_MOUSEUP10_PANE as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp10Status",
        key: KEYC_MOUSEUP10_STATUS as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp10StatusLeft",
        key: KEYC_MOUSEUP10_STATUS_LEFT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp10StatusRight",
        key: KEYC_MOUSEUP10_STATUS_RIGHT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp10StatusDefault",
        key: KEYC_MOUSEUP10_STATUS_DEFAULT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp10ScrollbarUp",
        key: KEYC_MOUSEUP10_SCROLLBAR_UP as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp10ScrollbarSlider",
        key: KEYC_MOUSEUP10_SCROLLBAR_SLIDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp10ScrollbarDown",
        key: KEYC_MOUSEUP10_SCROLLBAR_DOWN as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp10Empty",
        key: KEYC_MOUSEUP10_EMPTY as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp10Border",
        key: KEYC_MOUSEUP10_BORDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp10Control0",
        key: KEYC_MOUSEUP10_CONTROL0 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp10Control1",
        key: KEYC_MOUSEUP10_CONTROL1 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp10Control2",
        key: KEYC_MOUSEUP10_CONTROL2 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp10Control3",
        key: KEYC_MOUSEUP10_CONTROL3 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp10Control4",
        key: KEYC_MOUSEUP10_CONTROL4 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp10Control5",
        key: KEYC_MOUSEUP10_CONTROL5 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp10Control6",
        key: KEYC_MOUSEUP10_CONTROL6 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp10Control7",
        key: KEYC_MOUSEUP10_CONTROL7 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp10Control8",
        key: KEYC_MOUSEUP10_CONTROL8 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp10Control9",
        key: KEYC_MOUSEUP10_CONTROL9 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp11Pane",
        key: KEYC_MOUSEUP11_PANE as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp11Status",
        key: KEYC_MOUSEUP11_STATUS as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp11StatusLeft",
        key: KEYC_MOUSEUP11_STATUS_LEFT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp11StatusRight",
        key: KEYC_MOUSEUP11_STATUS_RIGHT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp11StatusDefault",
        key: KEYC_MOUSEUP11_STATUS_DEFAULT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp11ScrollbarUp",
        key: KEYC_MOUSEUP11_SCROLLBAR_UP as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp11ScrollbarSlider",
        key: KEYC_MOUSEUP11_SCROLLBAR_SLIDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp11ScrollbarDown",
        key: KEYC_MOUSEUP11_SCROLLBAR_DOWN as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp11Empty",
        key: KEYC_MOUSEUP11_EMPTY as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp11Border",
        key: KEYC_MOUSEUP11_BORDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp11Control0",
        key: KEYC_MOUSEUP11_CONTROL0 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp11Control1",
        key: KEYC_MOUSEUP11_CONTROL1 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp11Control2",
        key: KEYC_MOUSEUP11_CONTROL2 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp11Control3",
        key: KEYC_MOUSEUP11_CONTROL3 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp11Control4",
        key: KEYC_MOUSEUP11_CONTROL4 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp11Control5",
        key: KEYC_MOUSEUP11_CONTROL5 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp11Control6",
        key: KEYC_MOUSEUP11_CONTROL6 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp11Control7",
        key: KEYC_MOUSEUP11_CONTROL7 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp11Control8",
        key: KEYC_MOUSEUP11_CONTROL8 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseUp11Control9",
        key: KEYC_MOUSEUP11_CONTROL9 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag1Pane",
        key: KEYC_MOUSEDRAG1_PANE as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag1Status",
        key: KEYC_MOUSEDRAG1_STATUS as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag1StatusLeft",
        key: KEYC_MOUSEDRAG1_STATUS_LEFT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag1StatusRight",
        key: KEYC_MOUSEDRAG1_STATUS_RIGHT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag1StatusDefault",
        key: KEYC_MOUSEDRAG1_STATUS_DEFAULT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag1ScrollbarUp",
        key: KEYC_MOUSEDRAG1_SCROLLBAR_UP as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag1ScrollbarSlider",
        key: KEYC_MOUSEDRAG1_SCROLLBAR_SLIDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag1ScrollbarDown",
        key: KEYC_MOUSEDRAG1_SCROLLBAR_DOWN as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag1Empty",
        key: KEYC_MOUSEDRAG1_EMPTY as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag1Border",
        key: KEYC_MOUSEDRAG1_BORDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag1Control0",
        key: KEYC_MOUSEDRAG1_CONTROL0 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag1Control1",
        key: KEYC_MOUSEDRAG1_CONTROL1 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag1Control2",
        key: KEYC_MOUSEDRAG1_CONTROL2 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag1Control3",
        key: KEYC_MOUSEDRAG1_CONTROL3 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag1Control4",
        key: KEYC_MOUSEDRAG1_CONTROL4 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag1Control5",
        key: KEYC_MOUSEDRAG1_CONTROL5 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag1Control6",
        key: KEYC_MOUSEDRAG1_CONTROL6 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag1Control7",
        key: KEYC_MOUSEDRAG1_CONTROL7 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag1Control8",
        key: KEYC_MOUSEDRAG1_CONTROL8 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag1Control9",
        key: KEYC_MOUSEDRAG1_CONTROL9 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag2Pane",
        key: KEYC_MOUSEDRAG2_PANE as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag2Status",
        key: KEYC_MOUSEDRAG2_STATUS as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag2StatusLeft",
        key: KEYC_MOUSEDRAG2_STATUS_LEFT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag2StatusRight",
        key: KEYC_MOUSEDRAG2_STATUS_RIGHT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag2StatusDefault",
        key: KEYC_MOUSEDRAG2_STATUS_DEFAULT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag2ScrollbarUp",
        key: KEYC_MOUSEDRAG2_SCROLLBAR_UP as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag2ScrollbarSlider",
        key: KEYC_MOUSEDRAG2_SCROLLBAR_SLIDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag2ScrollbarDown",
        key: KEYC_MOUSEDRAG2_SCROLLBAR_DOWN as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag2Empty",
        key: KEYC_MOUSEDRAG2_EMPTY as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag2Border",
        key: KEYC_MOUSEDRAG2_BORDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag2Control0",
        key: KEYC_MOUSEDRAG2_CONTROL0 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag2Control1",
        key: KEYC_MOUSEDRAG2_CONTROL1 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag2Control2",
        key: KEYC_MOUSEDRAG2_CONTROL2 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag2Control3",
        key: KEYC_MOUSEDRAG2_CONTROL3 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag2Control4",
        key: KEYC_MOUSEDRAG2_CONTROL4 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag2Control5",
        key: KEYC_MOUSEDRAG2_CONTROL5 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag2Control6",
        key: KEYC_MOUSEDRAG2_CONTROL6 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag2Control7",
        key: KEYC_MOUSEDRAG2_CONTROL7 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag2Control8",
        key: KEYC_MOUSEDRAG2_CONTROL8 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag2Control9",
        key: KEYC_MOUSEDRAG2_CONTROL9 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag3Pane",
        key: KEYC_MOUSEDRAG3_PANE as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag3Status",
        key: KEYC_MOUSEDRAG3_STATUS as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag3StatusLeft",
        key: KEYC_MOUSEDRAG3_STATUS_LEFT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag3StatusRight",
        key: KEYC_MOUSEDRAG3_STATUS_RIGHT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag3StatusDefault",
        key: KEYC_MOUSEDRAG3_STATUS_DEFAULT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag3ScrollbarUp",
        key: KEYC_MOUSEDRAG3_SCROLLBAR_UP as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag3ScrollbarSlider",
        key: KEYC_MOUSEDRAG3_SCROLLBAR_SLIDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag3ScrollbarDown",
        key: KEYC_MOUSEDRAG3_SCROLLBAR_DOWN as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag3Empty",
        key: KEYC_MOUSEDRAG3_EMPTY as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag3Border",
        key: KEYC_MOUSEDRAG3_BORDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag3Control0",
        key: KEYC_MOUSEDRAG3_CONTROL0 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag3Control1",
        key: KEYC_MOUSEDRAG3_CONTROL1 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag3Control2",
        key: KEYC_MOUSEDRAG3_CONTROL2 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag3Control3",
        key: KEYC_MOUSEDRAG3_CONTROL3 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag3Control4",
        key: KEYC_MOUSEDRAG3_CONTROL4 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag3Control5",
        key: KEYC_MOUSEDRAG3_CONTROL5 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag3Control6",
        key: KEYC_MOUSEDRAG3_CONTROL6 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag3Control7",
        key: KEYC_MOUSEDRAG3_CONTROL7 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag3Control8",
        key: KEYC_MOUSEDRAG3_CONTROL8 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag3Control9",
        key: KEYC_MOUSEDRAG3_CONTROL9 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag6Pane",
        key: KEYC_MOUSEDRAG6_PANE as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag6Status",
        key: KEYC_MOUSEDRAG6_STATUS as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag6StatusLeft",
        key: KEYC_MOUSEDRAG6_STATUS_LEFT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag6StatusRight",
        key: KEYC_MOUSEDRAG6_STATUS_RIGHT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag6StatusDefault",
        key: KEYC_MOUSEDRAG6_STATUS_DEFAULT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag6ScrollbarUp",
        key: KEYC_MOUSEDRAG6_SCROLLBAR_UP as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag6ScrollbarSlider",
        key: KEYC_MOUSEDRAG6_SCROLLBAR_SLIDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag6ScrollbarDown",
        key: KEYC_MOUSEDRAG6_SCROLLBAR_DOWN as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag6Empty",
        key: KEYC_MOUSEDRAG6_EMPTY as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag6Border",
        key: KEYC_MOUSEDRAG6_BORDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag6Control0",
        key: KEYC_MOUSEDRAG6_CONTROL0 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag6Control1",
        key: KEYC_MOUSEDRAG6_CONTROL1 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag6Control2",
        key: KEYC_MOUSEDRAG6_CONTROL2 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag6Control3",
        key: KEYC_MOUSEDRAG6_CONTROL3 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag6Control4",
        key: KEYC_MOUSEDRAG6_CONTROL4 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag6Control5",
        key: KEYC_MOUSEDRAG6_CONTROL5 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag6Control6",
        key: KEYC_MOUSEDRAG6_CONTROL6 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag6Control7",
        key: KEYC_MOUSEDRAG6_CONTROL7 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag6Control8",
        key: KEYC_MOUSEDRAG6_CONTROL8 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag6Control9",
        key: KEYC_MOUSEDRAG6_CONTROL9 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag7Pane",
        key: KEYC_MOUSEDRAG7_PANE as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag7Status",
        key: KEYC_MOUSEDRAG7_STATUS as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag7StatusLeft",
        key: KEYC_MOUSEDRAG7_STATUS_LEFT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag7StatusRight",
        key: KEYC_MOUSEDRAG7_STATUS_RIGHT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag7StatusDefault",
        key: KEYC_MOUSEDRAG7_STATUS_DEFAULT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag7ScrollbarUp",
        key: KEYC_MOUSEDRAG7_SCROLLBAR_UP as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag7ScrollbarSlider",
        key: KEYC_MOUSEDRAG7_SCROLLBAR_SLIDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag7ScrollbarDown",
        key: KEYC_MOUSEDRAG7_SCROLLBAR_DOWN as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag7Empty",
        key: KEYC_MOUSEDRAG7_EMPTY as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag7Border",
        key: KEYC_MOUSEDRAG7_BORDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag7Control0",
        key: KEYC_MOUSEDRAG7_CONTROL0 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag7Control1",
        key: KEYC_MOUSEDRAG7_CONTROL1 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag7Control2",
        key: KEYC_MOUSEDRAG7_CONTROL2 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag7Control3",
        key: KEYC_MOUSEDRAG7_CONTROL3 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag7Control4",
        key: KEYC_MOUSEDRAG7_CONTROL4 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag7Control5",
        key: KEYC_MOUSEDRAG7_CONTROL5 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag7Control6",
        key: KEYC_MOUSEDRAG7_CONTROL6 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag7Control7",
        key: KEYC_MOUSEDRAG7_CONTROL7 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag7Control8",
        key: KEYC_MOUSEDRAG7_CONTROL8 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag7Control9",
        key: KEYC_MOUSEDRAG7_CONTROL9 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag8Pane",
        key: KEYC_MOUSEDRAG8_PANE as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag8Status",
        key: KEYC_MOUSEDRAG8_STATUS as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag8StatusLeft",
        key: KEYC_MOUSEDRAG8_STATUS_LEFT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag8StatusRight",
        key: KEYC_MOUSEDRAG8_STATUS_RIGHT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag8StatusDefault",
        key: KEYC_MOUSEDRAG8_STATUS_DEFAULT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag8ScrollbarUp",
        key: KEYC_MOUSEDRAG8_SCROLLBAR_UP as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag8ScrollbarSlider",
        key: KEYC_MOUSEDRAG8_SCROLLBAR_SLIDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag8ScrollbarDown",
        key: KEYC_MOUSEDRAG8_SCROLLBAR_DOWN as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag8Empty",
        key: KEYC_MOUSEDRAG8_EMPTY as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag8Border",
        key: KEYC_MOUSEDRAG8_BORDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag8Control0",
        key: KEYC_MOUSEDRAG8_CONTROL0 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag8Control1",
        key: KEYC_MOUSEDRAG8_CONTROL1 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag8Control2",
        key: KEYC_MOUSEDRAG8_CONTROL2 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag8Control3",
        key: KEYC_MOUSEDRAG8_CONTROL3 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag8Control4",
        key: KEYC_MOUSEDRAG8_CONTROL4 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag8Control5",
        key: KEYC_MOUSEDRAG8_CONTROL5 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag8Control6",
        key: KEYC_MOUSEDRAG8_CONTROL6 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag8Control7",
        key: KEYC_MOUSEDRAG8_CONTROL7 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag8Control8",
        key: KEYC_MOUSEDRAG8_CONTROL8 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag8Control9",
        key: KEYC_MOUSEDRAG8_CONTROL9 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag9Pane",
        key: KEYC_MOUSEDRAG9_PANE as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag9Status",
        key: KEYC_MOUSEDRAG9_STATUS as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag9StatusLeft",
        key: KEYC_MOUSEDRAG9_STATUS_LEFT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag9StatusRight",
        key: KEYC_MOUSEDRAG9_STATUS_RIGHT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag9StatusDefault",
        key: KEYC_MOUSEDRAG9_STATUS_DEFAULT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag9ScrollbarUp",
        key: KEYC_MOUSEDRAG9_SCROLLBAR_UP as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag9ScrollbarSlider",
        key: KEYC_MOUSEDRAG9_SCROLLBAR_SLIDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag9ScrollbarDown",
        key: KEYC_MOUSEDRAG9_SCROLLBAR_DOWN as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag9Empty",
        key: KEYC_MOUSEDRAG9_EMPTY as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag9Border",
        key: KEYC_MOUSEDRAG9_BORDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag9Control0",
        key: KEYC_MOUSEDRAG9_CONTROL0 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag9Control1",
        key: KEYC_MOUSEDRAG9_CONTROL1 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag9Control2",
        key: KEYC_MOUSEDRAG9_CONTROL2 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag9Control3",
        key: KEYC_MOUSEDRAG9_CONTROL3 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag9Control4",
        key: KEYC_MOUSEDRAG9_CONTROL4 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag9Control5",
        key: KEYC_MOUSEDRAG9_CONTROL5 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag9Control6",
        key: KEYC_MOUSEDRAG9_CONTROL6 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag9Control7",
        key: KEYC_MOUSEDRAG9_CONTROL7 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag9Control8",
        key: KEYC_MOUSEDRAG9_CONTROL8 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag9Control9",
        key: KEYC_MOUSEDRAG9_CONTROL9 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag10Pane",
        key: KEYC_MOUSEDRAG10_PANE as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag10Status",
        key: KEYC_MOUSEDRAG10_STATUS as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag10StatusLeft",
        key: KEYC_MOUSEDRAG10_STATUS_LEFT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag10StatusRight",
        key: KEYC_MOUSEDRAG10_STATUS_RIGHT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag10StatusDefault",
        key: KEYC_MOUSEDRAG10_STATUS_DEFAULT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag10ScrollbarUp",
        key: KEYC_MOUSEDRAG10_SCROLLBAR_UP as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag10ScrollbarSlider",
        key: KEYC_MOUSEDRAG10_SCROLLBAR_SLIDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag10ScrollbarDown",
        key: KEYC_MOUSEDRAG10_SCROLLBAR_DOWN as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag10Empty",
        key: KEYC_MOUSEDRAG10_EMPTY as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag10Border",
        key: KEYC_MOUSEDRAG10_BORDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag10Control0",
        key: KEYC_MOUSEDRAG10_CONTROL0 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag10Control1",
        key: KEYC_MOUSEDRAG10_CONTROL1 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag10Control2",
        key: KEYC_MOUSEDRAG10_CONTROL2 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag10Control3",
        key: KEYC_MOUSEDRAG10_CONTROL3 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag10Control4",
        key: KEYC_MOUSEDRAG10_CONTROL4 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag10Control5",
        key: KEYC_MOUSEDRAG10_CONTROL5 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag10Control6",
        key: KEYC_MOUSEDRAG10_CONTROL6 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag10Control7",
        key: KEYC_MOUSEDRAG10_CONTROL7 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag10Control8",
        key: KEYC_MOUSEDRAG10_CONTROL8 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag10Control9",
        key: KEYC_MOUSEDRAG10_CONTROL9 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag11Pane",
        key: KEYC_MOUSEDRAG11_PANE as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag11Status",
        key: KEYC_MOUSEDRAG11_STATUS as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag11StatusLeft",
        key: KEYC_MOUSEDRAG11_STATUS_LEFT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag11StatusRight",
        key: KEYC_MOUSEDRAG11_STATUS_RIGHT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag11StatusDefault",
        key: KEYC_MOUSEDRAG11_STATUS_DEFAULT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag11ScrollbarUp",
        key: KEYC_MOUSEDRAG11_SCROLLBAR_UP as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag11ScrollbarSlider",
        key: KEYC_MOUSEDRAG11_SCROLLBAR_SLIDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag11ScrollbarDown",
        key: KEYC_MOUSEDRAG11_SCROLLBAR_DOWN as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag11Empty",
        key: KEYC_MOUSEDRAG11_EMPTY as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag11Border",
        key: KEYC_MOUSEDRAG11_BORDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag11Control0",
        key: KEYC_MOUSEDRAG11_CONTROL0 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag11Control1",
        key: KEYC_MOUSEDRAG11_CONTROL1 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag11Control2",
        key: KEYC_MOUSEDRAG11_CONTROL2 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag11Control3",
        key: KEYC_MOUSEDRAG11_CONTROL3 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag11Control4",
        key: KEYC_MOUSEDRAG11_CONTROL4 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag11Control5",
        key: KEYC_MOUSEDRAG11_CONTROL5 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag11Control6",
        key: KEYC_MOUSEDRAG11_CONTROL6 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag11Control7",
        key: KEYC_MOUSEDRAG11_CONTROL7 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag11Control8",
        key: KEYC_MOUSEDRAG11_CONTROL8 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDrag11Control9",
        key: KEYC_MOUSEDRAG11_CONTROL9 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd1Pane",
        key: KEYC_MOUSEDRAGEND1_PANE as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd1Status",
        key: KEYC_MOUSEDRAGEND1_STATUS as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd1StatusLeft",
        key: KEYC_MOUSEDRAGEND1_STATUS_LEFT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd1StatusRight",
        key: KEYC_MOUSEDRAGEND1_STATUS_RIGHT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd1StatusDefault",
        key: KEYC_MOUSEDRAGEND1_STATUS_DEFAULT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd1ScrollbarUp",
        key: KEYC_MOUSEDRAGEND1_SCROLLBAR_UP as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd1ScrollbarSlider",
        key: KEYC_MOUSEDRAGEND1_SCROLLBAR_SLIDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd1ScrollbarDown",
        key: KEYC_MOUSEDRAGEND1_SCROLLBAR_DOWN as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd1Empty",
        key: KEYC_MOUSEDRAGEND1_EMPTY as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd1Border",
        key: KEYC_MOUSEDRAGEND1_BORDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd1Control0",
        key: KEYC_MOUSEDRAGEND1_CONTROL0 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd1Control1",
        key: KEYC_MOUSEDRAGEND1_CONTROL1 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd1Control2",
        key: KEYC_MOUSEDRAGEND1_CONTROL2 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd1Control3",
        key: KEYC_MOUSEDRAGEND1_CONTROL3 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd1Control4",
        key: KEYC_MOUSEDRAGEND1_CONTROL4 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd1Control5",
        key: KEYC_MOUSEDRAGEND1_CONTROL5 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd1Control6",
        key: KEYC_MOUSEDRAGEND1_CONTROL6 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd1Control7",
        key: KEYC_MOUSEDRAGEND1_CONTROL7 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd1Control8",
        key: KEYC_MOUSEDRAGEND1_CONTROL8 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd1Control9",
        key: KEYC_MOUSEDRAGEND1_CONTROL9 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd2Pane",
        key: KEYC_MOUSEDRAGEND2_PANE as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd2Status",
        key: KEYC_MOUSEDRAGEND2_STATUS as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd2StatusLeft",
        key: KEYC_MOUSEDRAGEND2_STATUS_LEFT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd2StatusRight",
        key: KEYC_MOUSEDRAGEND2_STATUS_RIGHT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd2StatusDefault",
        key: KEYC_MOUSEDRAGEND2_STATUS_DEFAULT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd2ScrollbarUp",
        key: KEYC_MOUSEDRAGEND2_SCROLLBAR_UP as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd2ScrollbarSlider",
        key: KEYC_MOUSEDRAGEND2_SCROLLBAR_SLIDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd2ScrollbarDown",
        key: KEYC_MOUSEDRAGEND2_SCROLLBAR_DOWN as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd2Empty",
        key: KEYC_MOUSEDRAGEND2_EMPTY as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd2Border",
        key: KEYC_MOUSEDRAGEND2_BORDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd2Control0",
        key: KEYC_MOUSEDRAGEND2_CONTROL0 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd2Control1",
        key: KEYC_MOUSEDRAGEND2_CONTROL1 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd2Control2",
        key: KEYC_MOUSEDRAGEND2_CONTROL2 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd2Control3",
        key: KEYC_MOUSEDRAGEND2_CONTROL3 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd2Control4",
        key: KEYC_MOUSEDRAGEND2_CONTROL4 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd2Control5",
        key: KEYC_MOUSEDRAGEND2_CONTROL5 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd2Control6",
        key: KEYC_MOUSEDRAGEND2_CONTROL6 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd2Control7",
        key: KEYC_MOUSEDRAGEND2_CONTROL7 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd2Control8",
        key: KEYC_MOUSEDRAGEND2_CONTROL8 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd2Control9",
        key: KEYC_MOUSEDRAGEND2_CONTROL9 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd3Pane",
        key: KEYC_MOUSEDRAGEND3_PANE as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd3Status",
        key: KEYC_MOUSEDRAGEND3_STATUS as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd3StatusLeft",
        key: KEYC_MOUSEDRAGEND3_STATUS_LEFT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd3StatusRight",
        key: KEYC_MOUSEDRAGEND3_STATUS_RIGHT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd3StatusDefault",
        key: KEYC_MOUSEDRAGEND3_STATUS_DEFAULT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd3ScrollbarUp",
        key: KEYC_MOUSEDRAGEND3_SCROLLBAR_UP as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd3ScrollbarSlider",
        key: KEYC_MOUSEDRAGEND3_SCROLLBAR_SLIDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd3ScrollbarDown",
        key: KEYC_MOUSEDRAGEND3_SCROLLBAR_DOWN as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd3Empty",
        key: KEYC_MOUSEDRAGEND3_EMPTY as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd3Border",
        key: KEYC_MOUSEDRAGEND3_BORDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd3Control0",
        key: KEYC_MOUSEDRAGEND3_CONTROL0 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd3Control1",
        key: KEYC_MOUSEDRAGEND3_CONTROL1 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd3Control2",
        key: KEYC_MOUSEDRAGEND3_CONTROL2 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd3Control3",
        key: KEYC_MOUSEDRAGEND3_CONTROL3 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd3Control4",
        key: KEYC_MOUSEDRAGEND3_CONTROL4 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd3Control5",
        key: KEYC_MOUSEDRAGEND3_CONTROL5 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd3Control6",
        key: KEYC_MOUSEDRAGEND3_CONTROL6 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd3Control7",
        key: KEYC_MOUSEDRAGEND3_CONTROL7 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd3Control8",
        key: KEYC_MOUSEDRAGEND3_CONTROL8 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd3Control9",
        key: KEYC_MOUSEDRAGEND3_CONTROL9 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd6Pane",
        key: KEYC_MOUSEDRAGEND6_PANE as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd6Status",
        key: KEYC_MOUSEDRAGEND6_STATUS as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd6StatusLeft",
        key: KEYC_MOUSEDRAGEND6_STATUS_LEFT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd6StatusRight",
        key: KEYC_MOUSEDRAGEND6_STATUS_RIGHT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd6StatusDefault",
        key: KEYC_MOUSEDRAGEND6_STATUS_DEFAULT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd6ScrollbarUp",
        key: KEYC_MOUSEDRAGEND6_SCROLLBAR_UP as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd6ScrollbarSlider",
        key: KEYC_MOUSEDRAGEND6_SCROLLBAR_SLIDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd6ScrollbarDown",
        key: KEYC_MOUSEDRAGEND6_SCROLLBAR_DOWN as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd6Empty",
        key: KEYC_MOUSEDRAGEND6_EMPTY as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd6Border",
        key: KEYC_MOUSEDRAGEND6_BORDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd6Control0",
        key: KEYC_MOUSEDRAGEND6_CONTROL0 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd6Control1",
        key: KEYC_MOUSEDRAGEND6_CONTROL1 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd6Control2",
        key: KEYC_MOUSEDRAGEND6_CONTROL2 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd6Control3",
        key: KEYC_MOUSEDRAGEND6_CONTROL3 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd6Control4",
        key: KEYC_MOUSEDRAGEND6_CONTROL4 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd6Control5",
        key: KEYC_MOUSEDRAGEND6_CONTROL5 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd6Control6",
        key: KEYC_MOUSEDRAGEND6_CONTROL6 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd6Control7",
        key: KEYC_MOUSEDRAGEND6_CONTROL7 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd6Control8",
        key: KEYC_MOUSEDRAGEND6_CONTROL8 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd6Control9",
        key: KEYC_MOUSEDRAGEND6_CONTROL9 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd7Pane",
        key: KEYC_MOUSEDRAGEND7_PANE as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd7Status",
        key: KEYC_MOUSEDRAGEND7_STATUS as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd7StatusLeft",
        key: KEYC_MOUSEDRAGEND7_STATUS_LEFT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd7StatusRight",
        key: KEYC_MOUSEDRAGEND7_STATUS_RIGHT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd7StatusDefault",
        key: KEYC_MOUSEDRAGEND7_STATUS_DEFAULT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd7ScrollbarUp",
        key: KEYC_MOUSEDRAGEND7_SCROLLBAR_UP as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd7ScrollbarSlider",
        key: KEYC_MOUSEDRAGEND7_SCROLLBAR_SLIDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd7ScrollbarDown",
        key: KEYC_MOUSEDRAGEND7_SCROLLBAR_DOWN as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd7Empty",
        key: KEYC_MOUSEDRAGEND7_EMPTY as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd7Border",
        key: KEYC_MOUSEDRAGEND7_BORDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd7Control0",
        key: KEYC_MOUSEDRAGEND7_CONTROL0 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd7Control1",
        key: KEYC_MOUSEDRAGEND7_CONTROL1 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd7Control2",
        key: KEYC_MOUSEDRAGEND7_CONTROL2 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd7Control3",
        key: KEYC_MOUSEDRAGEND7_CONTROL3 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd7Control4",
        key: KEYC_MOUSEDRAGEND7_CONTROL4 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd7Control5",
        key: KEYC_MOUSEDRAGEND7_CONTROL5 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd7Control6",
        key: KEYC_MOUSEDRAGEND7_CONTROL6 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd7Control7",
        key: KEYC_MOUSEDRAGEND7_CONTROL7 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd7Control8",
        key: KEYC_MOUSEDRAGEND7_CONTROL8 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd7Control9",
        key: KEYC_MOUSEDRAGEND7_CONTROL9 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd8Pane",
        key: KEYC_MOUSEDRAGEND8_PANE as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd8Status",
        key: KEYC_MOUSEDRAGEND8_STATUS as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd8StatusLeft",
        key: KEYC_MOUSEDRAGEND8_STATUS_LEFT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd8StatusRight",
        key: KEYC_MOUSEDRAGEND8_STATUS_RIGHT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd8StatusDefault",
        key: KEYC_MOUSEDRAGEND8_STATUS_DEFAULT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd8ScrollbarUp",
        key: KEYC_MOUSEDRAGEND8_SCROLLBAR_UP as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd8ScrollbarSlider",
        key: KEYC_MOUSEDRAGEND8_SCROLLBAR_SLIDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd8ScrollbarDown",
        key: KEYC_MOUSEDRAGEND8_SCROLLBAR_DOWN as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd8Empty",
        key: KEYC_MOUSEDRAGEND8_EMPTY as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd8Border",
        key: KEYC_MOUSEDRAGEND8_BORDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd8Control0",
        key: KEYC_MOUSEDRAGEND8_CONTROL0 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd8Control1",
        key: KEYC_MOUSEDRAGEND8_CONTROL1 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd8Control2",
        key: KEYC_MOUSEDRAGEND8_CONTROL2 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd8Control3",
        key: KEYC_MOUSEDRAGEND8_CONTROL3 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd8Control4",
        key: KEYC_MOUSEDRAGEND8_CONTROL4 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd8Control5",
        key: KEYC_MOUSEDRAGEND8_CONTROL5 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd8Control6",
        key: KEYC_MOUSEDRAGEND8_CONTROL6 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd8Control7",
        key: KEYC_MOUSEDRAGEND8_CONTROL7 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd8Control8",
        key: KEYC_MOUSEDRAGEND8_CONTROL8 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd8Control9",
        key: KEYC_MOUSEDRAGEND8_CONTROL9 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd9Pane",
        key: KEYC_MOUSEDRAGEND9_PANE as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd9Status",
        key: KEYC_MOUSEDRAGEND9_STATUS as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd9StatusLeft",
        key: KEYC_MOUSEDRAGEND9_STATUS_LEFT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd9StatusRight",
        key: KEYC_MOUSEDRAGEND9_STATUS_RIGHT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd9StatusDefault",
        key: KEYC_MOUSEDRAGEND9_STATUS_DEFAULT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd9ScrollbarUp",
        key: KEYC_MOUSEDRAGEND9_SCROLLBAR_UP as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd9ScrollbarSlider",
        key: KEYC_MOUSEDRAGEND9_SCROLLBAR_SLIDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd9ScrollbarDown",
        key: KEYC_MOUSEDRAGEND9_SCROLLBAR_DOWN as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd9Empty",
        key: KEYC_MOUSEDRAGEND9_EMPTY as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd9Border",
        key: KEYC_MOUSEDRAGEND9_BORDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd9Control0",
        key: KEYC_MOUSEDRAGEND9_CONTROL0 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd9Control1",
        key: KEYC_MOUSEDRAGEND9_CONTROL1 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd9Control2",
        key: KEYC_MOUSEDRAGEND9_CONTROL2 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd9Control3",
        key: KEYC_MOUSEDRAGEND9_CONTROL3 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd9Control4",
        key: KEYC_MOUSEDRAGEND9_CONTROL4 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd9Control5",
        key: KEYC_MOUSEDRAGEND9_CONTROL5 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd9Control6",
        key: KEYC_MOUSEDRAGEND9_CONTROL6 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd9Control7",
        key: KEYC_MOUSEDRAGEND9_CONTROL7 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd9Control8",
        key: KEYC_MOUSEDRAGEND9_CONTROL8 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd9Control9",
        key: KEYC_MOUSEDRAGEND9_CONTROL9 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd10Pane",
        key: KEYC_MOUSEDRAGEND10_PANE as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd10Status",
        key: KEYC_MOUSEDRAGEND10_STATUS as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd10StatusLeft",
        key: KEYC_MOUSEDRAGEND10_STATUS_LEFT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd10StatusRight",
        key: KEYC_MOUSEDRAGEND10_STATUS_RIGHT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd10StatusDefault",
        key: KEYC_MOUSEDRAGEND10_STATUS_DEFAULT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd10ScrollbarUp",
        key: KEYC_MOUSEDRAGEND10_SCROLLBAR_UP as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd10ScrollbarSlider",
        key: KEYC_MOUSEDRAGEND10_SCROLLBAR_SLIDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd10ScrollbarDown",
        key: KEYC_MOUSEDRAGEND10_SCROLLBAR_DOWN as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd10Empty",
        key: KEYC_MOUSEDRAGEND10_EMPTY as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd10Border",
        key: KEYC_MOUSEDRAGEND10_BORDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd10Control0",
        key: KEYC_MOUSEDRAGEND10_CONTROL0 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd10Control1",
        key: KEYC_MOUSEDRAGEND10_CONTROL1 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd10Control2",
        key: KEYC_MOUSEDRAGEND10_CONTROL2 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd10Control3",
        key: KEYC_MOUSEDRAGEND10_CONTROL3 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd10Control4",
        key: KEYC_MOUSEDRAGEND10_CONTROL4 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd10Control5",
        key: KEYC_MOUSEDRAGEND10_CONTROL5 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd10Control6",
        key: KEYC_MOUSEDRAGEND10_CONTROL6 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd10Control7",
        key: KEYC_MOUSEDRAGEND10_CONTROL7 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd10Control8",
        key: KEYC_MOUSEDRAGEND10_CONTROL8 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd10Control9",
        key: KEYC_MOUSEDRAGEND10_CONTROL9 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd11Pane",
        key: KEYC_MOUSEDRAGEND11_PANE as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd11Status",
        key: KEYC_MOUSEDRAGEND11_STATUS as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd11StatusLeft",
        key: KEYC_MOUSEDRAGEND11_STATUS_LEFT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd11StatusRight",
        key: KEYC_MOUSEDRAGEND11_STATUS_RIGHT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd11StatusDefault",
        key: KEYC_MOUSEDRAGEND11_STATUS_DEFAULT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd11ScrollbarUp",
        key: KEYC_MOUSEDRAGEND11_SCROLLBAR_UP as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd11ScrollbarSlider",
        key: KEYC_MOUSEDRAGEND11_SCROLLBAR_SLIDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd11ScrollbarDown",
        key: KEYC_MOUSEDRAGEND11_SCROLLBAR_DOWN as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd11Empty",
        key: KEYC_MOUSEDRAGEND11_EMPTY as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd11Border",
        key: KEYC_MOUSEDRAGEND11_BORDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd11Control0",
        key: KEYC_MOUSEDRAGEND11_CONTROL0 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd11Control1",
        key: KEYC_MOUSEDRAGEND11_CONTROL1 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd11Control2",
        key: KEYC_MOUSEDRAGEND11_CONTROL2 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd11Control3",
        key: KEYC_MOUSEDRAGEND11_CONTROL3 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd11Control4",
        key: KEYC_MOUSEDRAGEND11_CONTROL4 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd11Control5",
        key: KEYC_MOUSEDRAGEND11_CONTROL5 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd11Control6",
        key: KEYC_MOUSEDRAGEND11_CONTROL6 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd11Control7",
        key: KEYC_MOUSEDRAGEND11_CONTROL7 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd11Control8",
        key: KEYC_MOUSEDRAGEND11_CONTROL8 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"MouseDragEnd11Control9",
        key: KEYC_MOUSEDRAGEND11_CONTROL9 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"WheelUpPane",
        key: KEYC_WHEELUP_PANE as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"WheelUpStatus",
        key: KEYC_WHEELUP_STATUS as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"WheelUpStatusLeft",
        key: KEYC_WHEELUP_STATUS_LEFT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"WheelUpStatusRight",
        key: KEYC_WHEELUP_STATUS_RIGHT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"WheelUpStatusDefault",
        key: KEYC_WHEELUP_STATUS_DEFAULT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"WheelUpScrollbarUp",
        key: KEYC_WHEELUP_SCROLLBAR_UP as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"WheelUpScrollbarSlider",
        key: KEYC_WHEELUP_SCROLLBAR_SLIDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"WheelUpScrollbarDown",
        key: KEYC_WHEELUP_SCROLLBAR_DOWN as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"WheelUpEmpty",
        key: KEYC_WHEELUP_EMPTY as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"WheelUpBorder",
        key: KEYC_WHEELUP_BORDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"WheelUpControl0",
        key: KEYC_WHEELUP_CONTROL0 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"WheelUpControl1",
        key: KEYC_WHEELUP_CONTROL1 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"WheelUpControl2",
        key: KEYC_WHEELUP_CONTROL2 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"WheelUpControl3",
        key: KEYC_WHEELUP_CONTROL3 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"WheelUpControl4",
        key: KEYC_WHEELUP_CONTROL4 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"WheelUpControl5",
        key: KEYC_WHEELUP_CONTROL5 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"WheelUpControl6",
        key: KEYC_WHEELUP_CONTROL6 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"WheelUpControl7",
        key: KEYC_WHEELUP_CONTROL7 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"WheelUpControl8",
        key: KEYC_WHEELUP_CONTROL8 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"WheelUpControl9",
        key: KEYC_WHEELUP_CONTROL9 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"WheelDownPane",
        key: KEYC_WHEELDOWN_PANE as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"WheelDownStatus",
        key: KEYC_WHEELDOWN_STATUS as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"WheelDownStatusLeft",
        key: KEYC_WHEELDOWN_STATUS_LEFT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"WheelDownStatusRight",
        key: KEYC_WHEELDOWN_STATUS_RIGHT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"WheelDownStatusDefault",
        key: KEYC_WHEELDOWN_STATUS_DEFAULT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"WheelDownScrollbarUp",
        key: KEYC_WHEELDOWN_SCROLLBAR_UP as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"WheelDownScrollbarSlider",
        key: KEYC_WHEELDOWN_SCROLLBAR_SLIDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"WheelDownScrollbarDown",
        key: KEYC_WHEELDOWN_SCROLLBAR_DOWN as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"WheelDownEmpty",
        key: KEYC_WHEELDOWN_EMPTY as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"WheelDownBorder",
        key: KEYC_WHEELDOWN_BORDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"WheelDownControl0",
        key: KEYC_WHEELDOWN_CONTROL0 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"WheelDownControl1",
        key: KEYC_WHEELDOWN_CONTROL1 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"WheelDownControl2",
        key: KEYC_WHEELDOWN_CONTROL2 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"WheelDownControl3",
        key: KEYC_WHEELDOWN_CONTROL3 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"WheelDownControl4",
        key: KEYC_WHEELDOWN_CONTROL4 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"WheelDownControl5",
        key: KEYC_WHEELDOWN_CONTROL5 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"WheelDownControl6",
        key: KEYC_WHEELDOWN_CONTROL6 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"WheelDownControl7",
        key: KEYC_WHEELDOWN_CONTROL7 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"WheelDownControl8",
        key: KEYC_WHEELDOWN_CONTROL8 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"WheelDownControl9",
        key: KEYC_WHEELDOWN_CONTROL9 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick1Pane",
        key: KEYC_SECONDCLICK1_PANE as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick1Status",
        key: KEYC_SECONDCLICK1_STATUS as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick1StatusLeft",
        key: KEYC_SECONDCLICK1_STATUS_LEFT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick1StatusRight",
        key: KEYC_SECONDCLICK1_STATUS_RIGHT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick1StatusDefault",
        key: KEYC_SECONDCLICK1_STATUS_DEFAULT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick1ScrollbarUp",
        key: KEYC_SECONDCLICK1_SCROLLBAR_UP as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick1ScrollbarSlider",
        key: KEYC_SECONDCLICK1_SCROLLBAR_SLIDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick1ScrollbarDown",
        key: KEYC_SECONDCLICK1_SCROLLBAR_DOWN as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick1Empty",
        key: KEYC_SECONDCLICK1_EMPTY as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick1Border",
        key: KEYC_SECONDCLICK1_BORDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick1Control0",
        key: KEYC_SECONDCLICK1_CONTROL0 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick1Control1",
        key: KEYC_SECONDCLICK1_CONTROL1 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick1Control2",
        key: KEYC_SECONDCLICK1_CONTROL2 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick1Control3",
        key: KEYC_SECONDCLICK1_CONTROL3 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick1Control4",
        key: KEYC_SECONDCLICK1_CONTROL4 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick1Control5",
        key: KEYC_SECONDCLICK1_CONTROL5 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick1Control6",
        key: KEYC_SECONDCLICK1_CONTROL6 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick1Control7",
        key: KEYC_SECONDCLICK1_CONTROL7 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick1Control8",
        key: KEYC_SECONDCLICK1_CONTROL8 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick1Control9",
        key: KEYC_SECONDCLICK1_CONTROL9 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick2Pane",
        key: KEYC_SECONDCLICK2_PANE as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick2Status",
        key: KEYC_SECONDCLICK2_STATUS as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick2StatusLeft",
        key: KEYC_SECONDCLICK2_STATUS_LEFT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick2StatusRight",
        key: KEYC_SECONDCLICK2_STATUS_RIGHT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick2StatusDefault",
        key: KEYC_SECONDCLICK2_STATUS_DEFAULT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick2ScrollbarUp",
        key: KEYC_SECONDCLICK2_SCROLLBAR_UP as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick2ScrollbarSlider",
        key: KEYC_SECONDCLICK2_SCROLLBAR_SLIDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick2ScrollbarDown",
        key: KEYC_SECONDCLICK2_SCROLLBAR_DOWN as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick2Empty",
        key: KEYC_SECONDCLICK2_EMPTY as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick2Border",
        key: KEYC_SECONDCLICK2_BORDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick2Control0",
        key: KEYC_SECONDCLICK2_CONTROL0 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick2Control1",
        key: KEYC_SECONDCLICK2_CONTROL1 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick2Control2",
        key: KEYC_SECONDCLICK2_CONTROL2 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick2Control3",
        key: KEYC_SECONDCLICK2_CONTROL3 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick2Control4",
        key: KEYC_SECONDCLICK2_CONTROL4 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick2Control5",
        key: KEYC_SECONDCLICK2_CONTROL5 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick2Control6",
        key: KEYC_SECONDCLICK2_CONTROL6 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick2Control7",
        key: KEYC_SECONDCLICK2_CONTROL7 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick2Control8",
        key: KEYC_SECONDCLICK2_CONTROL8 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick2Control9",
        key: KEYC_SECONDCLICK2_CONTROL9 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick3Pane",
        key: KEYC_SECONDCLICK3_PANE as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick3Status",
        key: KEYC_SECONDCLICK3_STATUS as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick3StatusLeft",
        key: KEYC_SECONDCLICK3_STATUS_LEFT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick3StatusRight",
        key: KEYC_SECONDCLICK3_STATUS_RIGHT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick3StatusDefault",
        key: KEYC_SECONDCLICK3_STATUS_DEFAULT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick3ScrollbarUp",
        key: KEYC_SECONDCLICK3_SCROLLBAR_UP as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick3ScrollbarSlider",
        key: KEYC_SECONDCLICK3_SCROLLBAR_SLIDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick3ScrollbarDown",
        key: KEYC_SECONDCLICK3_SCROLLBAR_DOWN as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick3Empty",
        key: KEYC_SECONDCLICK3_EMPTY as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick3Border",
        key: KEYC_SECONDCLICK3_BORDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick3Control0",
        key: KEYC_SECONDCLICK3_CONTROL0 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick3Control1",
        key: KEYC_SECONDCLICK3_CONTROL1 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick3Control2",
        key: KEYC_SECONDCLICK3_CONTROL2 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick3Control3",
        key: KEYC_SECONDCLICK3_CONTROL3 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick3Control4",
        key: KEYC_SECONDCLICK3_CONTROL4 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick3Control5",
        key: KEYC_SECONDCLICK3_CONTROL5 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick3Control6",
        key: KEYC_SECONDCLICK3_CONTROL6 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick3Control7",
        key: KEYC_SECONDCLICK3_CONTROL7 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick3Control8",
        key: KEYC_SECONDCLICK3_CONTROL8 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick3Control9",
        key: KEYC_SECONDCLICK3_CONTROL9 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick6Pane",
        key: KEYC_SECONDCLICK6_PANE as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick6Status",
        key: KEYC_SECONDCLICK6_STATUS as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick6StatusLeft",
        key: KEYC_SECONDCLICK6_STATUS_LEFT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick6StatusRight",
        key: KEYC_SECONDCLICK6_STATUS_RIGHT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick6StatusDefault",
        key: KEYC_SECONDCLICK6_STATUS_DEFAULT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick6ScrollbarUp",
        key: KEYC_SECONDCLICK6_SCROLLBAR_UP as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick6ScrollbarSlider",
        key: KEYC_SECONDCLICK6_SCROLLBAR_SLIDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick6ScrollbarDown",
        key: KEYC_SECONDCLICK6_SCROLLBAR_DOWN as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick6Empty",
        key: KEYC_SECONDCLICK6_EMPTY as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick6Border",
        key: KEYC_SECONDCLICK6_BORDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick6Control0",
        key: KEYC_SECONDCLICK6_CONTROL0 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick6Control1",
        key: KEYC_SECONDCLICK6_CONTROL1 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick6Control2",
        key: KEYC_SECONDCLICK6_CONTROL2 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick6Control3",
        key: KEYC_SECONDCLICK6_CONTROL3 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick6Control4",
        key: KEYC_SECONDCLICK6_CONTROL4 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick6Control5",
        key: KEYC_SECONDCLICK6_CONTROL5 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick6Control6",
        key: KEYC_SECONDCLICK6_CONTROL6 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick6Control7",
        key: KEYC_SECONDCLICK6_CONTROL7 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick6Control8",
        key: KEYC_SECONDCLICK6_CONTROL8 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick6Control9",
        key: KEYC_SECONDCLICK6_CONTROL9 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick7Pane",
        key: KEYC_SECONDCLICK7_PANE as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick7Status",
        key: KEYC_SECONDCLICK7_STATUS as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick7StatusLeft",
        key: KEYC_SECONDCLICK7_STATUS_LEFT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick7StatusRight",
        key: KEYC_SECONDCLICK7_STATUS_RIGHT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick7StatusDefault",
        key: KEYC_SECONDCLICK7_STATUS_DEFAULT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick7ScrollbarUp",
        key: KEYC_SECONDCLICK7_SCROLLBAR_UP as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick7ScrollbarSlider",
        key: KEYC_SECONDCLICK7_SCROLLBAR_SLIDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick7ScrollbarDown",
        key: KEYC_SECONDCLICK7_SCROLLBAR_DOWN as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick7Empty",
        key: KEYC_SECONDCLICK7_EMPTY as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick7Border",
        key: KEYC_SECONDCLICK7_BORDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick7Control0",
        key: KEYC_SECONDCLICK7_CONTROL0 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick7Control1",
        key: KEYC_SECONDCLICK7_CONTROL1 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick7Control2",
        key: KEYC_SECONDCLICK7_CONTROL2 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick7Control3",
        key: KEYC_SECONDCLICK7_CONTROL3 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick7Control4",
        key: KEYC_SECONDCLICK7_CONTROL4 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick7Control5",
        key: KEYC_SECONDCLICK7_CONTROL5 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick7Control6",
        key: KEYC_SECONDCLICK7_CONTROL6 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick7Control7",
        key: KEYC_SECONDCLICK7_CONTROL7 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick7Control8",
        key: KEYC_SECONDCLICK7_CONTROL8 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick7Control9",
        key: KEYC_SECONDCLICK7_CONTROL9 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick8Pane",
        key: KEYC_SECONDCLICK8_PANE as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick8Status",
        key: KEYC_SECONDCLICK8_STATUS as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick8StatusLeft",
        key: KEYC_SECONDCLICK8_STATUS_LEFT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick8StatusRight",
        key: KEYC_SECONDCLICK8_STATUS_RIGHT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick8StatusDefault",
        key: KEYC_SECONDCLICK8_STATUS_DEFAULT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick8ScrollbarUp",
        key: KEYC_SECONDCLICK8_SCROLLBAR_UP as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick8ScrollbarSlider",
        key: KEYC_SECONDCLICK8_SCROLLBAR_SLIDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick8ScrollbarDown",
        key: KEYC_SECONDCLICK8_SCROLLBAR_DOWN as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick8Empty",
        key: KEYC_SECONDCLICK8_EMPTY as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick8Border",
        key: KEYC_SECONDCLICK8_BORDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick8Control0",
        key: KEYC_SECONDCLICK8_CONTROL0 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick8Control1",
        key: KEYC_SECONDCLICK8_CONTROL1 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick8Control2",
        key: KEYC_SECONDCLICK8_CONTROL2 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick8Control3",
        key: KEYC_SECONDCLICK8_CONTROL3 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick8Control4",
        key: KEYC_SECONDCLICK8_CONTROL4 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick8Control5",
        key: KEYC_SECONDCLICK8_CONTROL5 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick8Control6",
        key: KEYC_SECONDCLICK8_CONTROL6 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick8Control7",
        key: KEYC_SECONDCLICK8_CONTROL7 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick8Control8",
        key: KEYC_SECONDCLICK8_CONTROL8 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick8Control9",
        key: KEYC_SECONDCLICK8_CONTROL9 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick9Pane",
        key: KEYC_SECONDCLICK9_PANE as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick9Status",
        key: KEYC_SECONDCLICK9_STATUS as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick9StatusLeft",
        key: KEYC_SECONDCLICK9_STATUS_LEFT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick9StatusRight",
        key: KEYC_SECONDCLICK9_STATUS_RIGHT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick9StatusDefault",
        key: KEYC_SECONDCLICK9_STATUS_DEFAULT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick9ScrollbarUp",
        key: KEYC_SECONDCLICK9_SCROLLBAR_UP as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick9ScrollbarSlider",
        key: KEYC_SECONDCLICK9_SCROLLBAR_SLIDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick9ScrollbarDown",
        key: KEYC_SECONDCLICK9_SCROLLBAR_DOWN as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick9Empty",
        key: KEYC_SECONDCLICK9_EMPTY as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick9Border",
        key: KEYC_SECONDCLICK9_BORDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick9Control0",
        key: KEYC_SECONDCLICK9_CONTROL0 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick9Control1",
        key: KEYC_SECONDCLICK9_CONTROL1 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick9Control2",
        key: KEYC_SECONDCLICK9_CONTROL2 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick9Control3",
        key: KEYC_SECONDCLICK9_CONTROL3 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick9Control4",
        key: KEYC_SECONDCLICK9_CONTROL4 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick9Control5",
        key: KEYC_SECONDCLICK9_CONTROL5 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick9Control6",
        key: KEYC_SECONDCLICK9_CONTROL6 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick9Control7",
        key: KEYC_SECONDCLICK9_CONTROL7 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick9Control8",
        key: KEYC_SECONDCLICK9_CONTROL8 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick9Control9",
        key: KEYC_SECONDCLICK9_CONTROL9 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick10Pane",
        key: KEYC_SECONDCLICK10_PANE as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick10Status",
        key: KEYC_SECONDCLICK10_STATUS as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick10StatusLeft",
        key: KEYC_SECONDCLICK10_STATUS_LEFT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick10StatusRight",
        key: KEYC_SECONDCLICK10_STATUS_RIGHT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick10StatusDefault",
        key: KEYC_SECONDCLICK10_STATUS_DEFAULT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick10ScrollbarUp",
        key: KEYC_SECONDCLICK10_SCROLLBAR_UP as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick10ScrollbarSlider",
        key: KEYC_SECONDCLICK10_SCROLLBAR_SLIDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick10ScrollbarDown",
        key: KEYC_SECONDCLICK10_SCROLLBAR_DOWN as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick10Empty",
        key: KEYC_SECONDCLICK10_EMPTY as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick10Border",
        key: KEYC_SECONDCLICK10_BORDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick10Control0",
        key: KEYC_SECONDCLICK10_CONTROL0 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick10Control1",
        key: KEYC_SECONDCLICK10_CONTROL1 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick10Control2",
        key: KEYC_SECONDCLICK10_CONTROL2 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick10Control3",
        key: KEYC_SECONDCLICK10_CONTROL3 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick10Control4",
        key: KEYC_SECONDCLICK10_CONTROL4 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick10Control5",
        key: KEYC_SECONDCLICK10_CONTROL5 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick10Control6",
        key: KEYC_SECONDCLICK10_CONTROL6 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick10Control7",
        key: KEYC_SECONDCLICK10_CONTROL7 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick10Control8",
        key: KEYC_SECONDCLICK10_CONTROL8 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick10Control9",
        key: KEYC_SECONDCLICK10_CONTROL9 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick11Pane",
        key: KEYC_SECONDCLICK11_PANE as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick11Status",
        key: KEYC_SECONDCLICK11_STATUS as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick11StatusLeft",
        key: KEYC_SECONDCLICK11_STATUS_LEFT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick11StatusRight",
        key: KEYC_SECONDCLICK11_STATUS_RIGHT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick11StatusDefault",
        key: KEYC_SECONDCLICK11_STATUS_DEFAULT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick11ScrollbarUp",
        key: KEYC_SECONDCLICK11_SCROLLBAR_UP as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick11ScrollbarSlider",
        key: KEYC_SECONDCLICK11_SCROLLBAR_SLIDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick11ScrollbarDown",
        key: KEYC_SECONDCLICK11_SCROLLBAR_DOWN as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick11Empty",
        key: KEYC_SECONDCLICK11_EMPTY as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick11Border",
        key: KEYC_SECONDCLICK11_BORDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick11Control0",
        key: KEYC_SECONDCLICK11_CONTROL0 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick11Control1",
        key: KEYC_SECONDCLICK11_CONTROL1 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick11Control2",
        key: KEYC_SECONDCLICK11_CONTROL2 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick11Control3",
        key: KEYC_SECONDCLICK11_CONTROL3 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick11Control4",
        key: KEYC_SECONDCLICK11_CONTROL4 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick11Control5",
        key: KEYC_SECONDCLICK11_CONTROL5 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick11Control6",
        key: KEYC_SECONDCLICK11_CONTROL6 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick11Control7",
        key: KEYC_SECONDCLICK11_CONTROL7 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick11Control8",
        key: KEYC_SECONDCLICK11_CONTROL8 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"SecondClick11Control9",
        key: KEYC_SECONDCLICK11_CONTROL9 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick1Pane",
        key: KEYC_DOUBLECLICK1_PANE as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick1Status",
        key: KEYC_DOUBLECLICK1_STATUS as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick1StatusLeft",
        key: KEYC_DOUBLECLICK1_STATUS_LEFT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick1StatusRight",
        key: KEYC_DOUBLECLICK1_STATUS_RIGHT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick1StatusDefault",
        key: KEYC_DOUBLECLICK1_STATUS_DEFAULT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick1ScrollbarUp",
        key: KEYC_DOUBLECLICK1_SCROLLBAR_UP as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick1ScrollbarSlider",
        key: KEYC_DOUBLECLICK1_SCROLLBAR_SLIDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick1ScrollbarDown",
        key: KEYC_DOUBLECLICK1_SCROLLBAR_DOWN as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick1Empty",
        key: KEYC_DOUBLECLICK1_EMPTY as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick1Border",
        key: KEYC_DOUBLECLICK1_BORDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick1Control0",
        key: KEYC_DOUBLECLICK1_CONTROL0 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick1Control1",
        key: KEYC_DOUBLECLICK1_CONTROL1 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick1Control2",
        key: KEYC_DOUBLECLICK1_CONTROL2 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick1Control3",
        key: KEYC_DOUBLECLICK1_CONTROL3 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick1Control4",
        key: KEYC_DOUBLECLICK1_CONTROL4 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick1Control5",
        key: KEYC_DOUBLECLICK1_CONTROL5 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick1Control6",
        key: KEYC_DOUBLECLICK1_CONTROL6 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick1Control7",
        key: KEYC_DOUBLECLICK1_CONTROL7 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick1Control8",
        key: KEYC_DOUBLECLICK1_CONTROL8 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick1Control9",
        key: KEYC_DOUBLECLICK1_CONTROL9 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick2Pane",
        key: KEYC_DOUBLECLICK2_PANE as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick2Status",
        key: KEYC_DOUBLECLICK2_STATUS as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick2StatusLeft",
        key: KEYC_DOUBLECLICK2_STATUS_LEFT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick2StatusRight",
        key: KEYC_DOUBLECLICK2_STATUS_RIGHT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick2StatusDefault",
        key: KEYC_DOUBLECLICK2_STATUS_DEFAULT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick2ScrollbarUp",
        key: KEYC_DOUBLECLICK2_SCROLLBAR_UP as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick2ScrollbarSlider",
        key: KEYC_DOUBLECLICK2_SCROLLBAR_SLIDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick2ScrollbarDown",
        key: KEYC_DOUBLECLICK2_SCROLLBAR_DOWN as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick2Empty",
        key: KEYC_DOUBLECLICK2_EMPTY as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick2Border",
        key: KEYC_DOUBLECLICK2_BORDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick2Control0",
        key: KEYC_DOUBLECLICK2_CONTROL0 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick2Control1",
        key: KEYC_DOUBLECLICK2_CONTROL1 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick2Control2",
        key: KEYC_DOUBLECLICK2_CONTROL2 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick2Control3",
        key: KEYC_DOUBLECLICK2_CONTROL3 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick2Control4",
        key: KEYC_DOUBLECLICK2_CONTROL4 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick2Control5",
        key: KEYC_DOUBLECLICK2_CONTROL5 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick2Control6",
        key: KEYC_DOUBLECLICK2_CONTROL6 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick2Control7",
        key: KEYC_DOUBLECLICK2_CONTROL7 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick2Control8",
        key: KEYC_DOUBLECLICK2_CONTROL8 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick2Control9",
        key: KEYC_DOUBLECLICK2_CONTROL9 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick3Pane",
        key: KEYC_DOUBLECLICK3_PANE as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick3Status",
        key: KEYC_DOUBLECLICK3_STATUS as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick3StatusLeft",
        key: KEYC_DOUBLECLICK3_STATUS_LEFT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick3StatusRight",
        key: KEYC_DOUBLECLICK3_STATUS_RIGHT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick3StatusDefault",
        key: KEYC_DOUBLECLICK3_STATUS_DEFAULT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick3ScrollbarUp",
        key: KEYC_DOUBLECLICK3_SCROLLBAR_UP as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick3ScrollbarSlider",
        key: KEYC_DOUBLECLICK3_SCROLLBAR_SLIDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick3ScrollbarDown",
        key: KEYC_DOUBLECLICK3_SCROLLBAR_DOWN as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick3Empty",
        key: KEYC_DOUBLECLICK3_EMPTY as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick3Border",
        key: KEYC_DOUBLECLICK3_BORDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick3Control0",
        key: KEYC_DOUBLECLICK3_CONTROL0 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick3Control1",
        key: KEYC_DOUBLECLICK3_CONTROL1 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick3Control2",
        key: KEYC_DOUBLECLICK3_CONTROL2 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick3Control3",
        key: KEYC_DOUBLECLICK3_CONTROL3 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick3Control4",
        key: KEYC_DOUBLECLICK3_CONTROL4 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick3Control5",
        key: KEYC_DOUBLECLICK3_CONTROL5 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick3Control6",
        key: KEYC_DOUBLECLICK3_CONTROL6 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick3Control7",
        key: KEYC_DOUBLECLICK3_CONTROL7 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick3Control8",
        key: KEYC_DOUBLECLICK3_CONTROL8 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick3Control9",
        key: KEYC_DOUBLECLICK3_CONTROL9 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick6Pane",
        key: KEYC_DOUBLECLICK6_PANE as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick6Status",
        key: KEYC_DOUBLECLICK6_STATUS as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick6StatusLeft",
        key: KEYC_DOUBLECLICK6_STATUS_LEFT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick6StatusRight",
        key: KEYC_DOUBLECLICK6_STATUS_RIGHT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick6StatusDefault",
        key: KEYC_DOUBLECLICK6_STATUS_DEFAULT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick6ScrollbarUp",
        key: KEYC_DOUBLECLICK6_SCROLLBAR_UP as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick6ScrollbarSlider",
        key: KEYC_DOUBLECLICK6_SCROLLBAR_SLIDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick6ScrollbarDown",
        key: KEYC_DOUBLECLICK6_SCROLLBAR_DOWN as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick6Empty",
        key: KEYC_DOUBLECLICK6_EMPTY as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick6Border",
        key: KEYC_DOUBLECLICK6_BORDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick6Control0",
        key: KEYC_DOUBLECLICK6_CONTROL0 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick6Control1",
        key: KEYC_DOUBLECLICK6_CONTROL1 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick6Control2",
        key: KEYC_DOUBLECLICK6_CONTROL2 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick6Control3",
        key: KEYC_DOUBLECLICK6_CONTROL3 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick6Control4",
        key: KEYC_DOUBLECLICK6_CONTROL4 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick6Control5",
        key: KEYC_DOUBLECLICK6_CONTROL5 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick6Control6",
        key: KEYC_DOUBLECLICK6_CONTROL6 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick6Control7",
        key: KEYC_DOUBLECLICK6_CONTROL7 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick6Control8",
        key: KEYC_DOUBLECLICK6_CONTROL8 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick6Control9",
        key: KEYC_DOUBLECLICK6_CONTROL9 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick7Pane",
        key: KEYC_DOUBLECLICK7_PANE as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick7Status",
        key: KEYC_DOUBLECLICK7_STATUS as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick7StatusLeft",
        key: KEYC_DOUBLECLICK7_STATUS_LEFT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick7StatusRight",
        key: KEYC_DOUBLECLICK7_STATUS_RIGHT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick7StatusDefault",
        key: KEYC_DOUBLECLICK7_STATUS_DEFAULT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick7ScrollbarUp",
        key: KEYC_DOUBLECLICK7_SCROLLBAR_UP as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick7ScrollbarSlider",
        key: KEYC_DOUBLECLICK7_SCROLLBAR_SLIDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick7ScrollbarDown",
        key: KEYC_DOUBLECLICK7_SCROLLBAR_DOWN as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick7Empty",
        key: KEYC_DOUBLECLICK7_EMPTY as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick7Border",
        key: KEYC_DOUBLECLICK7_BORDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick7Control0",
        key: KEYC_DOUBLECLICK7_CONTROL0 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick7Control1",
        key: KEYC_DOUBLECLICK7_CONTROL1 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick7Control2",
        key: KEYC_DOUBLECLICK7_CONTROL2 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick7Control3",
        key: KEYC_DOUBLECLICK7_CONTROL3 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick7Control4",
        key: KEYC_DOUBLECLICK7_CONTROL4 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick7Control5",
        key: KEYC_DOUBLECLICK7_CONTROL5 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick7Control6",
        key: KEYC_DOUBLECLICK7_CONTROL6 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick7Control7",
        key: KEYC_DOUBLECLICK7_CONTROL7 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick7Control8",
        key: KEYC_DOUBLECLICK7_CONTROL8 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick7Control9",
        key: KEYC_DOUBLECLICK7_CONTROL9 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick8Pane",
        key: KEYC_DOUBLECLICK8_PANE as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick8Status",
        key: KEYC_DOUBLECLICK8_STATUS as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick8StatusLeft",
        key: KEYC_DOUBLECLICK8_STATUS_LEFT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick8StatusRight",
        key: KEYC_DOUBLECLICK8_STATUS_RIGHT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick8StatusDefault",
        key: KEYC_DOUBLECLICK8_STATUS_DEFAULT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick8ScrollbarUp",
        key: KEYC_DOUBLECLICK8_SCROLLBAR_UP as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick8ScrollbarSlider",
        key: KEYC_DOUBLECLICK8_SCROLLBAR_SLIDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick8ScrollbarDown",
        key: KEYC_DOUBLECLICK8_SCROLLBAR_DOWN as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick8Empty",
        key: KEYC_DOUBLECLICK8_EMPTY as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick8Border",
        key: KEYC_DOUBLECLICK8_BORDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick8Control0",
        key: KEYC_DOUBLECLICK8_CONTROL0 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick8Control1",
        key: KEYC_DOUBLECLICK8_CONTROL1 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick8Control2",
        key: KEYC_DOUBLECLICK8_CONTROL2 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick8Control3",
        key: KEYC_DOUBLECLICK8_CONTROL3 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick8Control4",
        key: KEYC_DOUBLECLICK8_CONTROL4 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick8Control5",
        key: KEYC_DOUBLECLICK8_CONTROL5 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick8Control6",
        key: KEYC_DOUBLECLICK8_CONTROL6 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick8Control7",
        key: KEYC_DOUBLECLICK8_CONTROL7 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick8Control8",
        key: KEYC_DOUBLECLICK8_CONTROL8 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick8Control9",
        key: KEYC_DOUBLECLICK8_CONTROL9 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick9Pane",
        key: KEYC_DOUBLECLICK9_PANE as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick9Status",
        key: KEYC_DOUBLECLICK9_STATUS as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick9StatusLeft",
        key: KEYC_DOUBLECLICK9_STATUS_LEFT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick9StatusRight",
        key: KEYC_DOUBLECLICK9_STATUS_RIGHT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick9StatusDefault",
        key: KEYC_DOUBLECLICK9_STATUS_DEFAULT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick9ScrollbarUp",
        key: KEYC_DOUBLECLICK9_SCROLLBAR_UP as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick9ScrollbarSlider",
        key: KEYC_DOUBLECLICK9_SCROLLBAR_SLIDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick9ScrollbarDown",
        key: KEYC_DOUBLECLICK9_SCROLLBAR_DOWN as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick9Empty",
        key: KEYC_DOUBLECLICK9_EMPTY as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick9Border",
        key: KEYC_DOUBLECLICK9_BORDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick9Control0",
        key: KEYC_DOUBLECLICK9_CONTROL0 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick9Control1",
        key: KEYC_DOUBLECLICK9_CONTROL1 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick9Control2",
        key: KEYC_DOUBLECLICK9_CONTROL2 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick9Control3",
        key: KEYC_DOUBLECLICK9_CONTROL3 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick9Control4",
        key: KEYC_DOUBLECLICK9_CONTROL4 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick9Control5",
        key: KEYC_DOUBLECLICK9_CONTROL5 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick9Control6",
        key: KEYC_DOUBLECLICK9_CONTROL6 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick9Control7",
        key: KEYC_DOUBLECLICK9_CONTROL7 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick9Control8",
        key: KEYC_DOUBLECLICK9_CONTROL8 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick9Control9",
        key: KEYC_DOUBLECLICK9_CONTROL9 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick10Pane",
        key: KEYC_DOUBLECLICK10_PANE as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick10Status",
        key: KEYC_DOUBLECLICK10_STATUS as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick10StatusLeft",
        key: KEYC_DOUBLECLICK10_STATUS_LEFT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick10StatusRight",
        key: KEYC_DOUBLECLICK10_STATUS_RIGHT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick10StatusDefault",
        key: KEYC_DOUBLECLICK10_STATUS_DEFAULT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick10ScrollbarUp",
        key: KEYC_DOUBLECLICK10_SCROLLBAR_UP as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick10ScrollbarSlider",
        key: KEYC_DOUBLECLICK10_SCROLLBAR_SLIDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick10ScrollbarDown",
        key: KEYC_DOUBLECLICK10_SCROLLBAR_DOWN as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick10Empty",
        key: KEYC_DOUBLECLICK10_EMPTY as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick10Border",
        key: KEYC_DOUBLECLICK10_BORDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick10Control0",
        key: KEYC_DOUBLECLICK10_CONTROL0 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick10Control1",
        key: KEYC_DOUBLECLICK10_CONTROL1 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick10Control2",
        key: KEYC_DOUBLECLICK10_CONTROL2 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick10Control3",
        key: KEYC_DOUBLECLICK10_CONTROL3 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick10Control4",
        key: KEYC_DOUBLECLICK10_CONTROL4 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick10Control5",
        key: KEYC_DOUBLECLICK10_CONTROL5 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick10Control6",
        key: KEYC_DOUBLECLICK10_CONTROL6 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick10Control7",
        key: KEYC_DOUBLECLICK10_CONTROL7 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick10Control8",
        key: KEYC_DOUBLECLICK10_CONTROL8 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick10Control9",
        key: KEYC_DOUBLECLICK10_CONTROL9 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick11Pane",
        key: KEYC_DOUBLECLICK11_PANE as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick11Status",
        key: KEYC_DOUBLECLICK11_STATUS as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick11StatusLeft",
        key: KEYC_DOUBLECLICK11_STATUS_LEFT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick11StatusRight",
        key: KEYC_DOUBLECLICK11_STATUS_RIGHT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick11StatusDefault",
        key: KEYC_DOUBLECLICK11_STATUS_DEFAULT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick11ScrollbarUp",
        key: KEYC_DOUBLECLICK11_SCROLLBAR_UP as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick11ScrollbarSlider",
        key: KEYC_DOUBLECLICK11_SCROLLBAR_SLIDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick11ScrollbarDown",
        key: KEYC_DOUBLECLICK11_SCROLLBAR_DOWN as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick11Empty",
        key: KEYC_DOUBLECLICK11_EMPTY as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick11Border",
        key: KEYC_DOUBLECLICK11_BORDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick11Control0",
        key: KEYC_DOUBLECLICK11_CONTROL0 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick11Control1",
        key: KEYC_DOUBLECLICK11_CONTROL1 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick11Control2",
        key: KEYC_DOUBLECLICK11_CONTROL2 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick11Control3",
        key: KEYC_DOUBLECLICK11_CONTROL3 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick11Control4",
        key: KEYC_DOUBLECLICK11_CONTROL4 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick11Control5",
        key: KEYC_DOUBLECLICK11_CONTROL5 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick11Control6",
        key: KEYC_DOUBLECLICK11_CONTROL6 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick11Control7",
        key: KEYC_DOUBLECLICK11_CONTROL7 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick11Control8",
        key: KEYC_DOUBLECLICK11_CONTROL8 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"DoubleClick11Control9",
        key: KEYC_DOUBLECLICK11_CONTROL9 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick1Pane",
        key: KEYC_TRIPLECLICK1_PANE as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick1Status",
        key: KEYC_TRIPLECLICK1_STATUS as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick1StatusLeft",
        key: KEYC_TRIPLECLICK1_STATUS_LEFT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick1StatusRight",
        key: KEYC_TRIPLECLICK1_STATUS_RIGHT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick1StatusDefault",
        key: KEYC_TRIPLECLICK1_STATUS_DEFAULT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick1ScrollbarUp",
        key: KEYC_TRIPLECLICK1_SCROLLBAR_UP as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick1ScrollbarSlider",
        key: KEYC_TRIPLECLICK1_SCROLLBAR_SLIDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick1ScrollbarDown",
        key: KEYC_TRIPLECLICK1_SCROLLBAR_DOWN as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick1Empty",
        key: KEYC_TRIPLECLICK1_EMPTY as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick1Border",
        key: KEYC_TRIPLECLICK1_BORDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick1Control0",
        key: KEYC_TRIPLECLICK1_CONTROL0 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick1Control1",
        key: KEYC_TRIPLECLICK1_CONTROL1 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick1Control2",
        key: KEYC_TRIPLECLICK1_CONTROL2 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick1Control3",
        key: KEYC_TRIPLECLICK1_CONTROL3 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick1Control4",
        key: KEYC_TRIPLECLICK1_CONTROL4 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick1Control5",
        key: KEYC_TRIPLECLICK1_CONTROL5 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick1Control6",
        key: KEYC_TRIPLECLICK1_CONTROL6 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick1Control7",
        key: KEYC_TRIPLECLICK1_CONTROL7 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick1Control8",
        key: KEYC_TRIPLECLICK1_CONTROL8 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick1Control9",
        key: KEYC_TRIPLECLICK1_CONTROL9 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick2Pane",
        key: KEYC_TRIPLECLICK2_PANE as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick2Status",
        key: KEYC_TRIPLECLICK2_STATUS as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick2StatusLeft",
        key: KEYC_TRIPLECLICK2_STATUS_LEFT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick2StatusRight",
        key: KEYC_TRIPLECLICK2_STATUS_RIGHT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick2StatusDefault",
        key: KEYC_TRIPLECLICK2_STATUS_DEFAULT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick2ScrollbarUp",
        key: KEYC_TRIPLECLICK2_SCROLLBAR_UP as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick2ScrollbarSlider",
        key: KEYC_TRIPLECLICK2_SCROLLBAR_SLIDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick2ScrollbarDown",
        key: KEYC_TRIPLECLICK2_SCROLLBAR_DOWN as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick2Empty",
        key: KEYC_TRIPLECLICK2_EMPTY as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick2Border",
        key: KEYC_TRIPLECLICK2_BORDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick2Control0",
        key: KEYC_TRIPLECLICK2_CONTROL0 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick2Control1",
        key: KEYC_TRIPLECLICK2_CONTROL1 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick2Control2",
        key: KEYC_TRIPLECLICK2_CONTROL2 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick2Control3",
        key: KEYC_TRIPLECLICK2_CONTROL3 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick2Control4",
        key: KEYC_TRIPLECLICK2_CONTROL4 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick2Control5",
        key: KEYC_TRIPLECLICK2_CONTROL5 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick2Control6",
        key: KEYC_TRIPLECLICK2_CONTROL6 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick2Control7",
        key: KEYC_TRIPLECLICK2_CONTROL7 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick2Control8",
        key: KEYC_TRIPLECLICK2_CONTROL8 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick2Control9",
        key: KEYC_TRIPLECLICK2_CONTROL9 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick3Pane",
        key: KEYC_TRIPLECLICK3_PANE as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick3Status",
        key: KEYC_TRIPLECLICK3_STATUS as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick3StatusLeft",
        key: KEYC_TRIPLECLICK3_STATUS_LEFT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick3StatusRight",
        key: KEYC_TRIPLECLICK3_STATUS_RIGHT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick3StatusDefault",
        key: KEYC_TRIPLECLICK3_STATUS_DEFAULT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick3ScrollbarUp",
        key: KEYC_TRIPLECLICK3_SCROLLBAR_UP as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick3ScrollbarSlider",
        key: KEYC_TRIPLECLICK3_SCROLLBAR_SLIDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick3ScrollbarDown",
        key: KEYC_TRIPLECLICK3_SCROLLBAR_DOWN as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick3Empty",
        key: KEYC_TRIPLECLICK3_EMPTY as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick3Border",
        key: KEYC_TRIPLECLICK3_BORDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick3Control0",
        key: KEYC_TRIPLECLICK3_CONTROL0 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick3Control1",
        key: KEYC_TRIPLECLICK3_CONTROL1 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick3Control2",
        key: KEYC_TRIPLECLICK3_CONTROL2 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick3Control3",
        key: KEYC_TRIPLECLICK3_CONTROL3 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick3Control4",
        key: KEYC_TRIPLECLICK3_CONTROL4 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick3Control5",
        key: KEYC_TRIPLECLICK3_CONTROL5 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick3Control6",
        key: KEYC_TRIPLECLICK3_CONTROL6 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick3Control7",
        key: KEYC_TRIPLECLICK3_CONTROL7 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick3Control8",
        key: KEYC_TRIPLECLICK3_CONTROL8 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick3Control9",
        key: KEYC_TRIPLECLICK3_CONTROL9 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick6Pane",
        key: KEYC_TRIPLECLICK6_PANE as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick6Status",
        key: KEYC_TRIPLECLICK6_STATUS as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick6StatusLeft",
        key: KEYC_TRIPLECLICK6_STATUS_LEFT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick6StatusRight",
        key: KEYC_TRIPLECLICK6_STATUS_RIGHT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick6StatusDefault",
        key: KEYC_TRIPLECLICK6_STATUS_DEFAULT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick6ScrollbarUp",
        key: KEYC_TRIPLECLICK6_SCROLLBAR_UP as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick6ScrollbarSlider",
        key: KEYC_TRIPLECLICK6_SCROLLBAR_SLIDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick6ScrollbarDown",
        key: KEYC_TRIPLECLICK6_SCROLLBAR_DOWN as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick6Empty",
        key: KEYC_TRIPLECLICK6_EMPTY as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick6Border",
        key: KEYC_TRIPLECLICK6_BORDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick6Control0",
        key: KEYC_TRIPLECLICK6_CONTROL0 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick6Control1",
        key: KEYC_TRIPLECLICK6_CONTROL1 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick6Control2",
        key: KEYC_TRIPLECLICK6_CONTROL2 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick6Control3",
        key: KEYC_TRIPLECLICK6_CONTROL3 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick6Control4",
        key: KEYC_TRIPLECLICK6_CONTROL4 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick6Control5",
        key: KEYC_TRIPLECLICK6_CONTROL5 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick6Control6",
        key: KEYC_TRIPLECLICK6_CONTROL6 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick6Control7",
        key: KEYC_TRIPLECLICK6_CONTROL7 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick6Control8",
        key: KEYC_TRIPLECLICK6_CONTROL8 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick6Control9",
        key: KEYC_TRIPLECLICK6_CONTROL9 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick7Pane",
        key: KEYC_TRIPLECLICK7_PANE as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick7Status",
        key: KEYC_TRIPLECLICK7_STATUS as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick7StatusLeft",
        key: KEYC_TRIPLECLICK7_STATUS_LEFT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick7StatusRight",
        key: KEYC_TRIPLECLICK7_STATUS_RIGHT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick7StatusDefault",
        key: KEYC_TRIPLECLICK7_STATUS_DEFAULT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick7ScrollbarUp",
        key: KEYC_TRIPLECLICK7_SCROLLBAR_UP as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick7ScrollbarSlider",
        key: KEYC_TRIPLECLICK7_SCROLLBAR_SLIDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick7ScrollbarDown",
        key: KEYC_TRIPLECLICK7_SCROLLBAR_DOWN as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick7Empty",
        key: KEYC_TRIPLECLICK7_EMPTY as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick7Border",
        key: KEYC_TRIPLECLICK7_BORDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick7Control0",
        key: KEYC_TRIPLECLICK7_CONTROL0 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick7Control1",
        key: KEYC_TRIPLECLICK7_CONTROL1 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick7Control2",
        key: KEYC_TRIPLECLICK7_CONTROL2 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick7Control3",
        key: KEYC_TRIPLECLICK7_CONTROL3 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick7Control4",
        key: KEYC_TRIPLECLICK7_CONTROL4 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick7Control5",
        key: KEYC_TRIPLECLICK7_CONTROL5 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick7Control6",
        key: KEYC_TRIPLECLICK7_CONTROL6 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick7Control7",
        key: KEYC_TRIPLECLICK7_CONTROL7 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick7Control8",
        key: KEYC_TRIPLECLICK7_CONTROL8 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick7Control9",
        key: KEYC_TRIPLECLICK7_CONTROL9 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick8Pane",
        key: KEYC_TRIPLECLICK8_PANE as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick8Status",
        key: KEYC_TRIPLECLICK8_STATUS as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick8StatusLeft",
        key: KEYC_TRIPLECLICK8_STATUS_LEFT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick8StatusRight",
        key: KEYC_TRIPLECLICK8_STATUS_RIGHT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick8StatusDefault",
        key: KEYC_TRIPLECLICK8_STATUS_DEFAULT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick8ScrollbarUp",
        key: KEYC_TRIPLECLICK8_SCROLLBAR_UP as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick8ScrollbarSlider",
        key: KEYC_TRIPLECLICK8_SCROLLBAR_SLIDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick8ScrollbarDown",
        key: KEYC_TRIPLECLICK8_SCROLLBAR_DOWN as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick8Empty",
        key: KEYC_TRIPLECLICK8_EMPTY as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick8Border",
        key: KEYC_TRIPLECLICK8_BORDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick8Control0",
        key: KEYC_TRIPLECLICK8_CONTROL0 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick8Control1",
        key: KEYC_TRIPLECLICK8_CONTROL1 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick8Control2",
        key: KEYC_TRIPLECLICK8_CONTROL2 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick8Control3",
        key: KEYC_TRIPLECLICK8_CONTROL3 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick8Control4",
        key: KEYC_TRIPLECLICK8_CONTROL4 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick8Control5",
        key: KEYC_TRIPLECLICK8_CONTROL5 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick8Control6",
        key: KEYC_TRIPLECLICK8_CONTROL6 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick8Control7",
        key: KEYC_TRIPLECLICK8_CONTROL7 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick8Control8",
        key: KEYC_TRIPLECLICK8_CONTROL8 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick8Control9",
        key: KEYC_TRIPLECLICK8_CONTROL9 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick9Pane",
        key: KEYC_TRIPLECLICK9_PANE as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick9Status",
        key: KEYC_TRIPLECLICK9_STATUS as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick9StatusLeft",
        key: KEYC_TRIPLECLICK9_STATUS_LEFT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick9StatusRight",
        key: KEYC_TRIPLECLICK9_STATUS_RIGHT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick9StatusDefault",
        key: KEYC_TRIPLECLICK9_STATUS_DEFAULT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick9ScrollbarUp",
        key: KEYC_TRIPLECLICK9_SCROLLBAR_UP as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick9ScrollbarSlider",
        key: KEYC_TRIPLECLICK9_SCROLLBAR_SLIDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick9ScrollbarDown",
        key: KEYC_TRIPLECLICK9_SCROLLBAR_DOWN as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick9Empty",
        key: KEYC_TRIPLECLICK9_EMPTY as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick9Border",
        key: KEYC_TRIPLECLICK9_BORDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick9Control0",
        key: KEYC_TRIPLECLICK9_CONTROL0 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick9Control1",
        key: KEYC_TRIPLECLICK9_CONTROL1 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick9Control2",
        key: KEYC_TRIPLECLICK9_CONTROL2 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick9Control3",
        key: KEYC_TRIPLECLICK9_CONTROL3 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick9Control4",
        key: KEYC_TRIPLECLICK9_CONTROL4 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick9Control5",
        key: KEYC_TRIPLECLICK9_CONTROL5 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick9Control6",
        key: KEYC_TRIPLECLICK9_CONTROL6 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick9Control7",
        key: KEYC_TRIPLECLICK9_CONTROL7 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick9Control8",
        key: KEYC_TRIPLECLICK9_CONTROL8 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick9Control9",
        key: KEYC_TRIPLECLICK9_CONTROL9 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick10Pane",
        key: KEYC_TRIPLECLICK10_PANE as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick10Status",
        key: KEYC_TRIPLECLICK10_STATUS as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick10StatusLeft",
        key: KEYC_TRIPLECLICK10_STATUS_LEFT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick10StatusRight",
        key: KEYC_TRIPLECLICK10_STATUS_RIGHT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick10StatusDefault",
        key: KEYC_TRIPLECLICK10_STATUS_DEFAULT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick10ScrollbarUp",
        key: KEYC_TRIPLECLICK10_SCROLLBAR_UP as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick10ScrollbarSlider",
        key: KEYC_TRIPLECLICK10_SCROLLBAR_SLIDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick10ScrollbarDown",
        key: KEYC_TRIPLECLICK10_SCROLLBAR_DOWN as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick10Empty",
        key: KEYC_TRIPLECLICK10_EMPTY as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick10Border",
        key: KEYC_TRIPLECLICK10_BORDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick10Control0",
        key: KEYC_TRIPLECLICK10_CONTROL0 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick10Control1",
        key: KEYC_TRIPLECLICK10_CONTROL1 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick10Control2",
        key: KEYC_TRIPLECLICK10_CONTROL2 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick10Control3",
        key: KEYC_TRIPLECLICK10_CONTROL3 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick10Control4",
        key: KEYC_TRIPLECLICK10_CONTROL4 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick10Control5",
        key: KEYC_TRIPLECLICK10_CONTROL5 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick10Control6",
        key: KEYC_TRIPLECLICK10_CONTROL6 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick10Control7",
        key: KEYC_TRIPLECLICK10_CONTROL7 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick10Control8",
        key: KEYC_TRIPLECLICK10_CONTROL8 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick10Control9",
        key: KEYC_TRIPLECLICK10_CONTROL9 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick11Pane",
        key: KEYC_TRIPLECLICK11_PANE as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick11Status",
        key: KEYC_TRIPLECLICK11_STATUS as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick11StatusLeft",
        key: KEYC_TRIPLECLICK11_STATUS_LEFT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick11StatusRight",
        key: KEYC_TRIPLECLICK11_STATUS_RIGHT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick11StatusDefault",
        key: KEYC_TRIPLECLICK11_STATUS_DEFAULT as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick11ScrollbarUp",
        key: KEYC_TRIPLECLICK11_SCROLLBAR_UP as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick11ScrollbarSlider",
        key: KEYC_TRIPLECLICK11_SCROLLBAR_SLIDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick11ScrollbarDown",
        key: KEYC_TRIPLECLICK11_SCROLLBAR_DOWN as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick11Empty",
        key: KEYC_TRIPLECLICK11_EMPTY as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick11Border",
        key: KEYC_TRIPLECLICK11_BORDER as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick11Control0",
        key: KEYC_TRIPLECLICK11_CONTROL0 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick11Control1",
        key: KEYC_TRIPLECLICK11_CONTROL1 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick11Control2",
        key: KEYC_TRIPLECLICK11_CONTROL2 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick11Control3",
        key: KEYC_TRIPLECLICK11_CONTROL3 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick11Control4",
        key: KEYC_TRIPLECLICK11_CONTROL4 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick11Control5",
        key: KEYC_TRIPLECLICK11_CONTROL5 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick11Control6",
        key: KEYC_TRIPLECLICK11_CONTROL6 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick11Control7",
        key: KEYC_TRIPLECLICK11_CONTROL7 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick11Control8",
        key: KEYC_TRIPLECLICK11_CONTROL8 as ::core::ffi::c_ulong as key_code,
    },
    C2RustUnnamed_1 {
        string: c"TripleClick11Control9",
        key: KEYC_TRIPLECLICK11_CONTROL9 as ::core::ffi::c_ulong as key_code,
    },
];

/// Search the static key names using libc's case-insensitive comparison.
fn key_string_search_table(input: &CStr) -> key_code {
    let mut user: u_int = 0;

    unsafe {
        for entry in &key_string_table {
            if strcasecmp(input.as_ptr(), entry.string.as_ptr()) == 0 as ::core::ffi::c_int {
                return entry.key;
            }
        }
        if sscanf(input.as_ptr(), c"User%u".as_ptr(), &raw mut user) == 1 as ::core::ffi::c_int
            && user <= KEYC_NUSER as u_int
        {
            return (KEYC_USER as ::core::ffi::c_ulong).wrapping_add(user as ::core::ffi::c_ulong)
                as key_code;
        }
    }

    KEYC_UNKNOWN as ::core::ffi::c_ulong as key_code
}

fn key_string_get_modifiers(input: &[u8]) -> Option<(key_code, usize)> {
    let mut modifiers = 0 as key_code;
    let mut offset = 0;

    while input.get(offset + 1) == Some(&b'-') {
        match input[offset] {
            b'C' | b'c' => modifiers |= KEYC_CTRL,
            b'M' | b'm' => modifiers |= KEYC_META,
            b'S' | b's' => modifiers |= KEYC_SHIFT,
            _ => return None,
        }
        offset += 2;
    }

    Some((modifiers, offset))
}

fn key_string_lowercase_byte(byte: u8) -> key_code {
    unsafe { tolower(byte as ::core::ffi::c_int) as key_code }
}

fn key_string_parse_numeric(input: &CStr) -> Option<key_code> {
    let mut value: u_int = 0;
    let parsed = unsafe {
        sscanf(
            cstr_suffix(input, 2).as_ptr(),
            c"%x".as_ptr(),
            &raw mut value,
        )
    };
    if parsed != 1 as ::core::ffi::c_int {
        return None;
    }
    if value < 32 as u_int {
        return Some(value as key_code);
    }

    let mut multibyte = [0u8; 17];
    let length = unsafe { wctomb(multibyte.as_mut_ptr().cast(), value as wchar_t) };
    if length <= 0 as ::core::ffi::c_int || length > MB_LEN_MAX {
        return None;
    }
    multibyte[length as usize] = 0;

    let decoded =
        utf8_fromcstr_vec(CStr::from_bytes_until_nul(&multibyte).expect("terminated above"));
    let mut codepoint: utf8_char = 0;
    let valid = decoded[0].size as ::core::ffi::c_int != 0 as ::core::ffi::c_int
        && decoded[1].size as ::core::ffi::c_int == 0 as ::core::ffi::c_int
        && utf8_from_data(&decoded[0], &mut codepoint) as ::core::ffi::c_uint
            == UTF8_DONE as ::core::ffi::c_int as ::core::ffi::c_uint;

    valid.then_some(codepoint as key_code)
}

/// Parse the key-name bytes while retaining a `CStr` only where the legacy
/// locale-sensitive libc/table operations require a NUL-terminated view.
fn key_string_lookup_string_bytes(input: &CStr) -> key_code {
    let bytes = input.to_bytes();
    if unsafe { strcasecmp(input.as_ptr(), c"None".as_ptr()) == 0 as ::core::ffi::c_int } {
        return KEYC_NONE as ::core::ffi::c_ulong as key_code;
    }
    if unsafe { strcasecmp(input.as_ptr(), c"Any".as_ptr()) == 0 as ::core::ffi::c_int } {
        return KEYC_ANY as ::core::ffi::c_ulong as key_code;
    }
    if bytes.starts_with(b"0x") {
        return key_string_parse_numeric(input).unwrap_or(KEYC_UNKNOWN);
    }

    let mut offset = 0;
    let mut modifiers = 0 as key_code;
    if bytes.first() == Some(&b'^') && bytes.len() > 1 {
        if bytes.len() == 2 {
            return key_string_lowercase_byte(bytes[1]) | KEYC_CTRL;
        }
        modifiers |= KEYC_CTRL;
        offset = 1;
    }

    let (prefix_modifiers, consumed) = match key_string_get_modifiers(&bytes[offset..]) {
        Some(result) => result,
        None => return KEYC_UNKNOWN as ::core::ffi::c_ulong as key_code,
    };
    modifiers |= prefix_modifiers;
    offset += consumed;

    let remaining = &bytes[offset..];
    if remaining.is_empty() {
        return KEYC_UNKNOWN as ::core::ffi::c_ulong as key_code;
    }
    if remaining.len() == 1 && remaining[0] <= 127 {
        let key = remaining[0] as key_code;
        if key < 32 as key_code {
            return KEYC_UNKNOWN as ::core::ffi::c_ulong as key_code;
        }
        return key | modifiers;
    }

    let remaining_cstr = cstr_suffix(input, offset);
    let mut data = utf8_data {
        data: [0; 32],
        have: 0,
        size: 0,
        width: 0,
    };
    let mut codepoint: utf8_char = 0;
    let mut more = utf8_open(&mut data, remaining[0]);
    if more as ::core::ffi::c_uint == UTF8_MORE as ::core::ffi::c_int as ::core::ffi::c_uint {
        if remaining.len() != data.size as usize {
            return KEYC_UNKNOWN as ::core::ffi::c_ulong as key_code;
        }
        for &byte in &remaining[1..] {
            more = utf8_append(&mut data, byte);
        }
        if more as ::core::ffi::c_uint != UTF8_DONE as ::core::ffi::c_int as ::core::ffi::c_uint {
            return KEYC_UNKNOWN as ::core::ffi::c_ulong as key_code;
        }
        if utf8_from_data(&data, &mut codepoint) as ::core::ffi::c_uint
            != UTF8_DONE as ::core::ffi::c_int as ::core::ffi::c_uint
        {
            return KEYC_UNKNOWN as ::core::ffi::c_ulong as key_code;
        }
        return codepoint as key_code | modifiers;
    }

    let mut key = key_string_search_table(remaining_cstr);
    if key == KEYC_UNKNOWN as ::core::ffi::c_ulong as key_code {
        return KEYC_UNKNOWN as ::core::ffi::c_ulong as key_code;
    }
    if !(modifiers as ::core::ffi::c_ulonglong) & KEYC_META != 0 {
        key &= !KEYC_IMPLIED_META;
    }
    key | modifiers
}

/// Parse a NUL-terminated key name without retaining the input pointer.
pub fn key_string_parse_cstr(input: &CStr) -> Option<key_code> {
    if input.to_bytes().is_empty() {
        return None;
    }
    let key = key_string_lookup_string_bytes(input);
    (key != KEYC_UNKNOWN).then_some(key)
}

/// Format a canonical key name into an owned NUL-terminated byte string.
///
/// The result is independent of all later formatting calls.
pub fn key_string_format(key: key_code, with_flags: bool) -> CString {
    CString::new(key_string_format_bytes(key, with_flags))
        .expect("key names contain no embedded NUL")
}

fn key_string_named_name(key: key_code) -> Option<&'static [u8]> {
    match key {
        KEYC_NONE => Some(b"None"),
        KEYC_UNKNOWN => Some(b"Unknown"),
        KEYC_ANY => Some(b"Any"),
        KEYC_FOCUS_IN => Some(b"FocusIn"),
        KEYC_FOCUS_OUT => Some(b"FocusOut"),
        KEYC_PASTE_START => Some(b"PasteStart"),
        KEYC_PASTE_END => Some(b"PasteEnd"),
        KEYC_REPORT_DARK_THEME => Some(b"ReportDarkTheme"),
        KEYC_REPORT_LIGHT_THEME => Some(b"ReportLightTheme"),
        KEYC_MOUSE => Some(b"Mouse"),
        KEYC_DRAGGING => Some(b"Dragging"),
        KEYC_MOUSEMOVE_PANE => Some(b"MouseMovePane"),
        KEYC_MOUSEMOVE_STATUS => Some(b"MouseMoveStatus"),
        KEYC_MOUSEMOVE_STATUS_LEFT => Some(b"MouseMoveStatusLeft"),
        KEYC_MOUSEMOVE_STATUS_RIGHT => Some(b"MouseMoveStatusRight"),
        KEYC_MOUSEMOVE_BORDER => Some(b"MouseMoveBorder"),
        _ => None,
    }
}

fn key_string_table_name(key: key_code) -> Option<Vec<u8>> {
    for entry in &key_string_table {
        if entry.key & KEYC_MASK_KEY == key {
            return Some(entry.string.to_bytes().to_vec());
        }
    }
    None
}

fn key_string_format_bytes(saved: key_code, with_flags: bool) -> Vec<u8> {
    let mut output = Vec::new();

    if saved & KEYC_LITERAL != 0 {
        let literal = (saved & 0xff) as u8;
        if literal != 0 {
            output.push(literal);
        }
    } else {
        if saved & KEYC_CTRL != 0 {
            output.extend_from_slice(b"C-");
        }
        if saved & KEYC_META != 0 {
            output.extend_from_slice(b"M-");
        }
        if saved & KEYC_SHIFT != 0 {
            output.extend_from_slice(b"S-");
        }

        let key = saved & KEYC_MASK_KEY;
        if let Some(name) = key_string_named_name(key) {
            output.extend_from_slice(name);
        } else if key & KEYC_MASK_TYPE == (KEYC_TYPE_USER as key_code) << 32 {
            output.extend_from_slice(format!("User{}", key.wrapping_sub(KEYC_USER)).as_bytes());
        } else if let Some(name) = key_string_table_name(key) {
            output.extend_from_slice(&name);
        } else if key & KEYC_MASK_TYPE == 0 && key > 0x7f {
            let mut data = utf8_data {
                data: [0; 32],
                have: 0,
                size: 0,
                width: 0,
            };
            utf8_to_data(key as utf8_char, &mut data);
            output.extend_from_slice(&data.data[..data.size as usize]);
        } else if key > 255 {
            output.clear();
            output.extend_from_slice(format!("Invalid#{saved:x}").as_bytes());
        } else if key > 32 && key <= 126 {
            output.push(key as u8);
        } else if key == 127 {
            output.extend_from_slice(b"C-?");
        } else if key >= 128 {
            output.extend_from_slice(format!(r"\{:o}", key).as_bytes());
        }
    }

    if let Some(nul) = output.iter().position(|&byte| byte == 0) {
        output.truncate(nul);
    }

    if with_flags && saved & KEYC_MASK_FLAGS != 0 {
        output.push(b'[');
        if saved & KEYC_LITERAL != 0 {
            output.push(b'L');
        }
        if saved & KEYC_KEYPAD != 0 {
            output.push(b'K');
        }
        if saved & KEYC_CURSOR != 0 {
            output.push(b'C');
        }
        if saved & KEYC_IMPLIED_META != 0 {
            output.push(b'I');
        }
        if saved & KEYC_BUILD_MODIFIERS != 0 {
            output.push(b'B');
        }
        if saved & KEYC_SENT != 0 {
            output.push(b'S');
        }
        output.push(b']');
    }
    output
}
