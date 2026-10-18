//! Authoritative terminal capability-code values.

use super::abi::u_int;
use super::colour::colour_palette;
use super::grid::grid_cell;
use super::pane::window_pane;
use crate::shared::client::ClientRef;
pub type tty_code_code = ::core::ffi::c_uint;
pub const TTYC_XT: tty_code_code = 235;
pub const TTYC_VPA: tty_code_code = 234;
pub const TTYC_U8: tty_code_code = 233;
pub const TTYC_TSL: tty_code_code = 232;
pub const TTYC_TC: tty_code_code = 231;
pub const TTYC_SYNC: tty_code_code = 230;
pub const TTYC_SWD: tty_code_code = 229;
pub const TTYC_SS: tty_code_code = 228;
pub const TTYC_SPB: tty_code_code = 226;
pub const TTYC_SMXX: tty_code_code = 225;
pub const TTYC_SMULX: tty_code_code = 224;
pub const TTYC_SMUL: tty_code_code = 223;
pub const TTYC_SMSO: tty_code_code = 222;
pub const TTYC_SMOL: tty_code_code = 221;
pub const TTYC_SMKX: tty_code_code = 220;
pub const TTYC_SMCUP: tty_code_code = 219;
pub const TTYC_SMACS: tty_code_code = 218;
pub const TTYC_SITM: tty_code_code = 217;
pub const TTYC_SGR0: tty_code_code = 216;
pub const TTYC_SETULC1: tty_code_code = 215;
pub const TTYC_SETULC: tty_code_code = 214;
pub const TTYC_SETRGBF: tty_code_code = 213;
pub const TTYC_SETRGBB: tty_code_code = 212;
pub const TTYC_SETAL: tty_code_code = 211;
pub const TTYC_SETAF: tty_code_code = 210;
pub const TTYC_SETAB: tty_code_code = 209;
pub const TTYC_SE: tty_code_code = 208;
pub const TTYC_RMKX: tty_code_code = 207;
pub const TTYC_RMCUP: tty_code_code = 206;
pub const TTYC_RMACS: tty_code_code = 205;
pub const TTYC_RIN: tty_code_code = 204;
pub const TTYC_RI: tty_code_code = 203;
pub const TTYC_RGB: tty_code_code = 202;
pub const TTYC_REV: tty_code_code = 201;
pub const TTYC_RECT: tty_code_code = 200;
pub const TTYC_OL: tty_code_code = 198;
pub const TTYC_NOBR: tty_code_code = 197;
pub const TTYC_MS: tty_code_code = 196;
pub const TTYC_KUP7: tty_code_code = 195;
pub const TTYC_KUP6: tty_code_code = 194;
pub const TTYC_KUP5: tty_code_code = 193;
pub const TTYC_KUP4: tty_code_code = 192;
pub const TTYC_KUP3: tty_code_code = 191;
pub const TTYC_KUP2: tty_code_code = 190;
pub const TTYC_KRIT7: tty_code_code = 189;
pub const TTYC_KRIT6: tty_code_code = 188;
pub const TTYC_KRIT5: tty_code_code = 187;
pub const TTYC_KRIT4: tty_code_code = 186;
pub const TTYC_KRIT3: tty_code_code = 185;
pub const TTYC_KRIT2: tty_code_code = 184;
pub const TTYC_KRI: tty_code_code = 183;
pub const TTYC_KPRV7: tty_code_code = 182;
pub const TTYC_KPRV6: tty_code_code = 181;
pub const TTYC_KPRV5: tty_code_code = 180;
pub const TTYC_KPRV4: tty_code_code = 179;
pub const TTYC_KPRV3: tty_code_code = 178;
pub const TTYC_KPRV2: tty_code_code = 177;
pub const TTYC_KPP: tty_code_code = 176;
pub const TTYC_KNXT7: tty_code_code = 175;
pub const TTYC_KNXT6: tty_code_code = 174;
pub const TTYC_KNXT5: tty_code_code = 173;
pub const TTYC_KNXT4: tty_code_code = 172;
pub const TTYC_KNXT3: tty_code_code = 171;
pub const TTYC_KNXT2: tty_code_code = 170;
pub const TTYC_KNP: tty_code_code = 169;
pub const TTYC_KMOUS: tty_code_code = 168;
pub const TTYC_KLFT7: tty_code_code = 167;
pub const TTYC_KLFT6: tty_code_code = 166;
pub const TTYC_KLFT5: tty_code_code = 165;
pub const TTYC_KLFT4: tty_code_code = 164;
pub const TTYC_KLFT3: tty_code_code = 163;
pub const TTYC_KLFT2: tty_code_code = 162;
pub const TTYC_KIND: tty_code_code = 161;
pub const TTYC_KICH1: tty_code_code = 160;
pub const TTYC_KIC7: tty_code_code = 159;
pub const TTYC_KIC6: tty_code_code = 158;
pub const TTYC_KIC5: tty_code_code = 157;
pub const TTYC_KIC4: tty_code_code = 156;
pub const TTYC_KIC3: tty_code_code = 155;
pub const TTYC_KIC2: tty_code_code = 154;
pub const TTYC_KHOME: tty_code_code = 153;
pub const TTYC_KHOM7: tty_code_code = 152;
pub const TTYC_KHOM6: tty_code_code = 151;
pub const TTYC_KHOM5: tty_code_code = 150;
pub const TTYC_KHOM4: tty_code_code = 149;
pub const TTYC_KHOM3: tty_code_code = 148;
pub const TTYC_KHOM2: tty_code_code = 147;
pub const TTYC_KF9: tty_code_code = 146;
pub const TTYC_KF8: tty_code_code = 145;
pub const TTYC_KF7: tty_code_code = 144;
pub const TTYC_KF63: tty_code_code = 143;
pub const TTYC_KF62: tty_code_code = 142;
pub const TTYC_KF61: tty_code_code = 141;
pub const TTYC_KF60: tty_code_code = 140;
pub const TTYC_KF6: tty_code_code = 139;
pub const TTYC_KF59: tty_code_code = 138;
pub const TTYC_KF58: tty_code_code = 137;
pub const TTYC_KF57: tty_code_code = 136;
pub const TTYC_KF56: tty_code_code = 135;
pub const TTYC_KF55: tty_code_code = 134;
pub const TTYC_KF54: tty_code_code = 133;
pub const TTYC_KF53: tty_code_code = 132;
pub const TTYC_KF52: tty_code_code = 131;
pub const TTYC_KF51: tty_code_code = 130;
pub const TTYC_KF50: tty_code_code = 129;
pub const TTYC_KF5: tty_code_code = 128;
pub const TTYC_KF49: tty_code_code = 127;
pub const TTYC_KF48: tty_code_code = 126;
pub const TTYC_KF47: tty_code_code = 125;
pub const TTYC_KF46: tty_code_code = 124;
pub const TTYC_KF45: tty_code_code = 123;
pub const TTYC_KF44: tty_code_code = 122;
pub const TTYC_KF43: tty_code_code = 121;
pub const TTYC_KF42: tty_code_code = 120;
pub const TTYC_KF41: tty_code_code = 119;
pub const TTYC_KF40: tty_code_code = 118;
pub const TTYC_KF4: tty_code_code = 117;
pub const TTYC_KF39: tty_code_code = 116;
pub const TTYC_KF38: tty_code_code = 115;
pub const TTYC_KF37: tty_code_code = 114;
pub const TTYC_KF36: tty_code_code = 113;
pub const TTYC_KF35: tty_code_code = 112;
pub const TTYC_KF34: tty_code_code = 111;
pub const TTYC_KF33: tty_code_code = 110;
pub const TTYC_KF32: tty_code_code = 109;
pub const TTYC_KF31: tty_code_code = 108;
pub const TTYC_KF30: tty_code_code = 107;
pub const TTYC_KF3: tty_code_code = 106;
pub const TTYC_KF29: tty_code_code = 105;
pub const TTYC_KF28: tty_code_code = 104;
pub const TTYC_KF27: tty_code_code = 103;
pub const TTYC_KF26: tty_code_code = 102;
pub const TTYC_KF25: tty_code_code = 101;
pub const TTYC_KF24: tty_code_code = 100;
pub const TTYC_KF23: tty_code_code = 99;
pub const TTYC_KF22: tty_code_code = 98;
pub const TTYC_KF21: tty_code_code = 97;
pub const TTYC_KF20: tty_code_code = 96;
pub const TTYC_KF2: tty_code_code = 95;
pub const TTYC_KF19: tty_code_code = 94;
pub const TTYC_KF18: tty_code_code = 93;
pub const TTYC_KF17: tty_code_code = 92;
pub const TTYC_KF16: tty_code_code = 91;
pub const TTYC_KF15: tty_code_code = 90;
pub const TTYC_KF14: tty_code_code = 89;
pub const TTYC_KF13: tty_code_code = 88;
pub const TTYC_KF12: tty_code_code = 87;
pub const TTYC_KF11: tty_code_code = 86;
pub const TTYC_KF10: tty_code_code = 85;
pub const TTYC_KF1: tty_code_code = 84;
pub const TTYC_KEND7: tty_code_code = 83;
pub const TTYC_KEND6: tty_code_code = 82;
pub const TTYC_KEND5: tty_code_code = 81;
pub const TTYC_KEND4: tty_code_code = 80;
pub const TTYC_KEND3: tty_code_code = 79;
pub const TTYC_KEND2: tty_code_code = 78;
pub const TTYC_KEND: tty_code_code = 77;
pub const TTYC_KDN7: tty_code_code = 76;
pub const TTYC_KDN6: tty_code_code = 75;
pub const TTYC_KDN5: tty_code_code = 74;
pub const TTYC_KDN4: tty_code_code = 73;
pub const TTYC_KDN3: tty_code_code = 72;
pub const TTYC_KDN2: tty_code_code = 71;
pub const TTYC_KDCH1: tty_code_code = 70;
pub const TTYC_KDC7: tty_code_code = 69;
pub const TTYC_KDC6: tty_code_code = 68;
pub const TTYC_KDC5: tty_code_code = 67;
pub const TTYC_KDC4: tty_code_code = 66;
pub const TTYC_KDC3: tty_code_code = 65;
pub const TTYC_KDC2: tty_code_code = 64;
pub const TTYC_KCUU1: tty_code_code = 63;
pub const TTYC_KCUF1: tty_code_code = 62;
pub const TTYC_KCUD1: tty_code_code = 61;
pub const TTYC_KCUB1: tty_code_code = 60;
pub const TTYC_KCBT: tty_code_code = 59;
pub const TTYC_INVIS: tty_code_code = 58;
pub const TTYC_INDN: tty_code_code = 57;
pub const TTYC_IND: tty_code_code = 56;
pub const TTYC_IL1: tty_code_code = 55;
pub const TTYC_IL: tty_code_code = 54;
pub const TTYC_ICH1: tty_code_code = 53;
pub const TTYC_ICH: tty_code_code = 52;
pub const TTYC_HPA: tty_code_code = 51;
pub const TTYC_HOME: tty_code_code = 50;
pub const TTYC_HLS: tty_code_code = 49;
pub const TTYC_FSL: tty_code_code = 48;
pub const TTYC_ENMG: tty_code_code = 47;
pub const TTYC_ENFCS: tty_code_code = 46;
pub const TTYC_ENESC: tty_code_code = 45;
pub const TTYC_ENEKS: tty_code_code = 44;
pub const TTYC_ENBP: tty_code_code = 43;
pub const TTYC_ENACS: tty_code_code = 42;
pub const TTYC_EL1: tty_code_code = 41;
pub const TTYC_EL: tty_code_code = 40;
pub const TTYC_ED: tty_code_code = 39;
pub const TTYC_ECH: tty_code_code = 38;
pub const TTYC_E3: tty_code_code = 37;
pub const TTYC_DSMG: tty_code_code = 36;
pub const TTYC_DSFCS: tty_code_code = 35;
pub const TTYC_DSESC: tty_code_code = 34;
pub const TTYC_DSEKS: tty_code_code = 33;
pub const TTYC_DSBP: tty_code_code = 32;
pub const TTYC_DL1: tty_code_code = 31;
pub const TTYC_DL: tty_code_code = 30;
pub const TTYC_DIM: tty_code_code = 29;
pub const TTYC_DCH1: tty_code_code = 28;
pub const TTYC_DCH: tty_code_code = 27;
pub const TTYC_CVVIS: tty_code_code = 26;
pub const TTYC_CUU1: tty_code_code = 25;
pub const TTYC_CUU: tty_code_code = 24;
pub const TTYC_CUP: tty_code_code = 23;
pub const TTYC_CUF1: tty_code_code = 22;
pub const TTYC_CUF: tty_code_code = 21;
pub const TTYC_CUD1: tty_code_code = 20;
pub const TTYC_CUD: tty_code_code = 19;
pub const TTYC_CUB1: tty_code_code = 18;
pub const TTYC_CUB: tty_code_code = 17;
pub const TTYC_CSR: tty_code_code = 16;
pub const TTYC_CS: tty_code_code = 15;
pub const TTYC_CR: tty_code_code = 14;
pub const TTYC_COLORS: tty_code_code = 13;
pub const TTYC_CNORM: tty_code_code = 12;
pub const TTYC_CMG: tty_code_code = 11;
pub const TTYC_CLMG: tty_code_code = 10;
pub const TTYC_CLEAR: tty_code_code = 9;
pub const TTYC_CIVIS: tty_code_code = 8;
pub const TTYC_BOLD: tty_code_code = 7;
pub const TTYC_BLINK: tty_code_code = 6;
pub const TTYC_BIDI: tty_code_code = 5;
pub const TTYC_BEL: tty_code_code = 4;
pub const TTYC_BCE: tty_code_code = 3;
pub const TTYC_AX: tty_code_code = 2;
pub const TTYC_AM: tty_code_code = 1;
pub const TTYC_ACSC: tty_code_code = 0;

pub const TTY_CTX_WINDOW_BIGGER: ::core::ffi::c_int = 0x4 as ::core::ffi::c_int;
pub const TTY_CTX_WRAPPED: ::core::ffi::c_int = 0x1 as ::core::ffi::c_int;
pub const TTY_CTX_INVISIBLE_PANES: ::core::ffi::c_int = 0x2 as ::core::ffi::c_int;
pub const TTY_CTX_SYNC: ::core::ffi::c_int = 0x8 as ::core::ffi::c_int;
pub const TTY_CTX_OVERLAY_SYNC: ::core::ffi::c_int = 0x10 as ::core::ffi::c_int;
pub const TTY_CTX_CELL_INVALIDATE: ::core::ffi::c_int = 0x20 as ::core::ffi::c_int;
pub const TTY_CTX_PANE_OBSCURED: ::core::ffi::c_int = 0x40 as ::core::ffi::c_int;

#[cfg(test)]
mod tests {
    use super::*;
    use ::core::mem::{align_of, size_of};

    #[test]
    fn tty_code_domain_matches_translated_c_baseline() {
        assert_eq!(size_of::<tty_code_code>(), 4);
        assert_eq!(align_of::<tty_code_code>(), 4);
        assert_eq!(TTYC_ACSC, 0);
        assert_eq!(TTYC_CLEAR, 9);
        assert_eq!(TTYC_XT, 235);
    }
}

/// Cached window coordinates and the size visible through a terminal.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct tty_window_view {
    pub bigger: bool,
    pub ox: u_int,
    pub oy: u_int,
    pub sx: u_int,
    pub sy: u_int,
}

/// Command metadata; screen and cell borrows are supplied only at dispatch.
#[derive(Default)]
pub struct tty_ctx<'a> {
    pub redraw_cb: tty_ctx_redraw_cb,
    pub set_client_cb: tty_ctx_set_client_cb,
    pub flags: ::core::ffi::c_int,
    pub data: tty_command_data<'a>,
    pub ocx: u_int,
    pub ocy: u_int,
    pub orupper: u_int,
    pub orlower: u_int,
    pub xoff: ::core::ffi::c_int,
    pub yoff: ::core::ffi::c_int,
    pub rxoff: ::core::ffi::c_int,
    pub ryoff: ::core::ffi::c_int,
    pub sx: u_int,
    pub sy: u_int,
    pub bg: u_int,
    pub style_ctx: tty_style_ctx,
    pub wox: u_int,
    pub woy: u_int,
    pub wsx: u_int,
    pub wsy: u_int,
}

#[derive(Clone, Default)]
#[repr(C)]
pub struct tty_style_ctx {
    pub defaults: grid_cell,
    pub palette: PaletteSource,
    pub dim: u_int,
    pub hyperlinks: Option<crate::hyperlinks::HyperlinksRef>,
}

#[derive(Clone, Default)]
pub enum PaletteSource {
    #[default]
    None,
    Pane(std::rc::Weak<std::cell::UnsafeCell<window_pane>>),
    Popup(refbox::Weak<colour_palette>),
    Snapshot(Box<colour_palette>),
}

impl PaletteSource {
    /// The callback must not reenter the pane, dispatch model callbacks, or
    /// retain any pointer/reference into the palette after returning.
    pub unsafe fn with_palette<R>(&self, read: impl FnOnce(Option<&colour_palette>) -> R) -> R {
        use crate::window::WindowPane;
        match self {
            Self::None => read(None),
            Self::Pane(observer) => {
                let owner = observer.upgrade();
                let palette = owner.as_ref().map(|pane| pane.borrow_palette());
                read(palette)
            }
            Self::Popup(palette) => match palette.try_borrow_mut() {
                Ok(borrowed) => read(Some(&borrowed)),
                Err(refbox::BorrowError::Dropped) => read(None),
                Err(refbox::BorrowError::Borrowed) => panic!("popup palette already borrowed"),
            },
            Self::Snapshot(palette) => read(Some(palette)),
        }
    }
}

/// Payload borrowed for the synchronous terminal command dispatch.
#[derive(Copy, Clone)]
pub enum tty_command_data<'a> {
    Count(u_int),
    Bytes(&'a [u8]),
    Selection {
        clip: &'a std::ffi::CStr,
        data: &'a [u8],
    },
}

impl Default for tty_command_data<'_> {
    fn default() -> Self {
        Self::Count(0)
    }
}

impl<'a> tty_command_data<'a> {
    pub fn count(self) -> u_int {
        match self {
            Self::Count(count) => count,
            _ => panic!("terminal command requires a count"),
        }
    }

    pub fn bytes(self) -> &'a [u8] {
        match self {
            Self::Bytes(bytes) => bytes,
            _ => panic!("terminal command requires text bytes"),
        }
    }

    pub fn selection(self) -> (&'a std::ffi::CStr, &'a [u8]) {
        match self {
            Self::Selection { clip, data } => (clip, data),
            _ => panic!("terminal command requires a selection"),
        }
    }
}

pub type tty_ctx_set_client_cb = Option<Box<dyn FnMut(&mut tty_ctx, &ClientRef) -> i32>>;

pub type tty_ctx_redraw_cb = Option<Box<dyn Fn(&tty_ctx)>>;
