use crate::base64;
use crate::osc7501::{
    OSC_7501_QUERY_REPLY_PREFIX, Osc7501Update, ProgramStatus, parse_osc_7501,
};
use std::cell::Cell;
use std::ffi::{CStr, CString, c_char};

pub type InputEndType = u32;
pub const INPUT_END_ST: InputEndType = 0;
pub const INPUT_END_BEL: InputEndType = 1;

pub const INPUT_BUF_DEFAULT_SIZE: i32 = 1048576;
pub const INPUT_BUF_START: usize = 32;
pub const INPUT_DISCARD: i32 = 0x1;
pub const INPUT_LAST: i32 = 0x2;

const SINCE_GROUND_KEEP: usize = 4096;

thread_local! {
    static INPUT_BUFFER_SIZE: Cell<usize> = const { Cell::new(INPUT_BUF_DEFAULT_SIZE as usize) };
}

pub fn buffer_size() -> usize {
    INPUT_BUFFER_SIZE.get()
}

pub fn set_buffer_size(buffer_size: usize) {
    INPUT_BUFFER_SIZE.set(buffer_size);
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SgrColor {
    Default,
    Ansi(u8),
    Indexed(u8),
    Rgb(u8, u8, u8),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SgrAttr {
    Bright,
    Dim,
    Italics,
    Underscore,
    Underscore2,
    Underscore3,
    Underscore4,
    Underscore5,
    Blink,
    Reverse,
    Hidden,
    Strikethrough,
    Overline,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SgrOp {
    ResetAll,
    ResetKeepLink,
    SetAttr(SgrAttr),
    ClearBrightDim,
    ClearItalics,
    ClearAllUnderscore,
    ClearBlink,
    ClearReverse,
    ClearHidden,
    ClearStrikethrough,
    ClearOverline,
    Fg(SgrColor),
    Bg(SgrColor),
    Us(SgrColor),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WinOp {
    ReportTextSizePixels,
    ReportScreenSizePixels,
    ReportCellSizePixels,
    ReportTextSizeChars,
    ReportScreenSizeChars,
    PushTitle,
    PopTitle,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Osc133Command {
    PromptStart,
    Prompt { secondary: bool },
    CommandStart,
    OutputStart,
    CommandFinished { exit_status: i32 },
}

pub trait VtHost {
    fn collect_end(&mut self);
    fn print_ascii_run(&mut self, bytes: &[u8]) -> usize;
    fn print_ascii(&mut self, byte: u8);
    fn print_utf8_byte(&mut self, byte: u8) -> bool;
    fn stop_utf8(&mut self);
    fn repeat_last(&mut self, count: u32);
    fn start_ground_timer(&mut self);
    fn clear_ground_timer(&mut self);
    fn reset_cell(&mut self);
    fn save_state(&mut self);
    fn restore_state(&mut self);
    fn select_charset(&mut self, set: i32);
    fn define_charset(&mut self, bank: u8, acs: bool);
    fn apply_sgr(&mut self, op: SgrOp);
    fn bell(&mut self);
    fn backspace(&mut self);
    fn horizontal_tab(&mut self);
    fn linefeed(&mut self, cr_on_crlf: bool);
    fn carriage_return(&mut self);
    fn reverse_index(&mut self);
    fn alignment_test(&mut self);
    fn hard_reset(&mut self);
    fn set_tab_at_cursor(&mut self, enabled: bool);
    fn clear_all_tabs(&mut self);
    fn cursor_back_tab(&mut self, count: u32);
    fn cursor_up(&mut self, count: u32);
    fn cursor_down(&mut self, count: u32);
    fn cursor_right(&mut self, count: u32);
    fn cursor_left(&mut self, count: u32);
    fn cursor_move(&mut self, x: i32, y: i32);
    fn erase_chars(&mut self, count: u32);
    fn delete_chars(&mut self, count: u32);
    fn insert_chars(&mut self, count: u32);
    fn delete_lines(&mut self, count: u32);
    fn insert_lines(&mut self, count: u32);
    fn scroll_up(&mut self, count: u32);
    fn scroll_down(&mut self, count: u32);
    fn screen_height(&self) -> u32;
    fn set_scroll_region(&mut self, upper: u32, lower: u32);
    fn erase_in_display(&mut self, mode: i32, submode: i32);
    fn erase_in_line(&mut self, mode: i32);
    fn set_cursor_style(&mut self, style: u32);
    fn set_mode(&mut self, mode: i32, private: bool, enabled: bool);
    fn set_keypad_mode(&mut self, enabled: bool);
    fn set_extended_keys_mode(&mut self, param: Option<i32>);
    fn reply(&mut self, queue: bool, bytes: &[u8]);
    fn query_cursor_position(&mut self);
    fn query_mode(&mut self, mode: i32, private: bool);
    fn query_theme(&mut self);
    fn query_xda(&mut self);
    fn query_decrqss_cursor_style(&mut self);
    fn winop(&mut self, op: WinOp);
    fn dcs_tmux_passthrough(&mut self, payload: &[u8]);
    fn set_title(&mut self, title: &CStr);
    fn set_path(&mut self, path: &CStr);
    fn set_hyperlink(&mut self, uri: &CStr, id: Option<&CStr>);
    fn set_progress_bar(&mut self, state: u32, progress: i32);
    fn osc_4_query(&mut self, idx: i32, end_type: InputEndType);
    fn osc_4_set(&mut self, idx: i32, spec: &CStr) -> bool;
    fn osc_104_reset_all(&mut self);
    fn osc_104_reset_one(&mut self, idx: i32) -> bool;
    fn full_redraw(&mut self);
    fn osc_10(&mut self, spec: Option<&CStr>, end_type: InputEndType);
    fn osc_110(&mut self);
    fn osc_11(&mut self, spec: Option<&CStr>, end_type: InputEndType);
    fn osc_111(&mut self);
    fn osc_12(&mut self, spec: Option<&CStr>, end_type: InputEndType);
    fn osc_112(&mut self);
    fn allow_set_clipboard(&self) -> bool;
    fn osc_52_query(&mut self, clip: c_char, end_type: InputEndType);
    fn osc_52_set(&mut self, clip: &CStr, data: Vec<u8>);
    fn osc_133(&mut self, cmd: Osc133Command);
    fn set_program_status(&mut self, status: Option<ProgramStatus>);
    fn exit_rename(&mut self, input_buf: &[u8], input_len: usize);
}

#[derive(Default)]
pub enum InputParam {
    #[default]
    Missing,
    Number(i32),
    String(CString),
}

type HandlerCb = fn(&mut Parser, &mut dyn VtHost) -> i32;

struct InputState {
    name: &'static CStr,
    enter: Option<fn(&mut Parser, &mut dyn VtHost)>,
    exit: Option<fn(&mut Parser, &mut dyn VtHost)>,
    transitions: &'static [InputTransition],
}

struct InputTransition {
    first: i32,
    last: i32,
    handler: Option<HandlerCb>,
    state: Option<&'static InputState>,
}

struct TableEntry {
    ch: i32,
    interm: &'static CStr,
    type_0: u32,
}

const INPUT_ESC_DECALN: u32 = 0;
const INPUT_ESC_DECKPAM: u32 = 1;
const INPUT_ESC_DECKPNM: u32 = 2;
const INPUT_ESC_DECRC: u32 = 3;
const INPUT_ESC_DECSC: u32 = 4;
const INPUT_ESC_HTS: u32 = 5;
const INPUT_ESC_IND: u32 = 6;
const INPUT_ESC_NEL: u32 = 7;
const INPUT_ESC_RI: u32 = 8;
const INPUT_ESC_RIS: u32 = 9;
const INPUT_ESC_SCSG0_OFF: u32 = 10;
const INPUT_ESC_SCSG0_ON: u32 = 11;
const INPUT_ESC_SCSG1_OFF: u32 = 12;
const INPUT_ESC_SCSG1_ON: u32 = 13;
const INPUT_ESC_ST: u32 = 14;

const INPUT_CSI_CBT: u32 = 0;
const INPUT_CSI_CNL: u32 = 1;
const INPUT_CSI_CPL: u32 = 2;
const INPUT_CSI_CUB: u32 = 3;
const INPUT_CSI_CUD: u32 = 4;
const INPUT_CSI_CUF: u32 = 5;
const INPUT_CSI_CUP: u32 = 6;
const INPUT_CSI_CUU: u32 = 7;
const INPUT_CSI_DA: u32 = 8;
const INPUT_CSI_DA_TWO: u32 = 9;
const INPUT_CSI_DCH: u32 = 10;
const INPUT_CSI_DECSCUSR: u32 = 11;
const INPUT_CSI_DECSTBM: u32 = 12;
const INPUT_CSI_DL: u32 = 13;
const INPUT_CSI_DSR: u32 = 14;
const INPUT_CSI_DSR_PRIVATE: u32 = 15;
const INPUT_CSI_ECH: u32 = 16;
const INPUT_CSI_ED: u32 = 17;
const INPUT_CSI_EL: u32 = 18;
const INPUT_CSI_HPA: u32 = 19;
const INPUT_CSI_ICH: u32 = 20;
const INPUT_CSI_IL: u32 = 21;
const INPUT_CSI_MODOFF: u32 = 22;
const INPUT_CSI_MODSET: u32 = 23;
const INPUT_CSI_QUERY: u32 = 24;
const INPUT_CSI_QUERY_PRIVATE: u32 = 25;
const INPUT_CSI_RCP: u32 = 26;
const INPUT_CSI_REP: u32 = 27;
const INPUT_CSI_RM: u32 = 28;
const INPUT_CSI_RM_PRIVATE: u32 = 29;
const INPUT_CSI_SCP: u32 = 30;
const INPUT_CSI_SD: u32 = 31;
const INPUT_CSI_SGR: u32 = 32;
const INPUT_CSI_SM: u32 = 33;
const INPUT_CSI_SM_GRAPHICS: u32 = 34;
const INPUT_CSI_SM_PRIVATE: u32 = 35;
const INPUT_CSI_SU: u32 = 36;
const INPUT_CSI_TBC: u32 = 37;
const INPUT_CSI_VPA: u32 = 38;
const INPUT_CSI_WINOPS: u32 = 39;
const INPUT_CSI_XDA: u32 = 40;

static INPUT_ESC_TABLE: [TableEntry; 15] = [
    TableEntry { ch: '0' as i32, interm: c"(", type_0: INPUT_ESC_SCSG0_ON },
    TableEntry { ch: '0' as i32, interm: c")", type_0: INPUT_ESC_SCSG1_ON },
    TableEntry { ch: '7' as i32, interm: c"", type_0: INPUT_ESC_DECSC },
    TableEntry { ch: '8' as i32, interm: c"", type_0: INPUT_ESC_DECRC },
    TableEntry { ch: '8' as i32, interm: c"#", type_0: INPUT_ESC_DECALN },
    TableEntry { ch: '=' as i32, interm: c"", type_0: INPUT_ESC_DECKPAM },
    TableEntry { ch: '>' as i32, interm: c"", type_0: INPUT_ESC_DECKPNM },
    TableEntry { ch: 'B' as i32, interm: c"(", type_0: INPUT_ESC_SCSG0_OFF },
    TableEntry { ch: 'B' as i32, interm: c")", type_0: INPUT_ESC_SCSG1_OFF },
    TableEntry { ch: 'D' as i32, interm: c"", type_0: INPUT_ESC_IND },
    TableEntry { ch: 'E' as i32, interm: c"", type_0: INPUT_ESC_NEL },
    TableEntry { ch: 'H' as i32, interm: c"", type_0: INPUT_ESC_HTS },
    TableEntry { ch: 'M' as i32, interm: c"", type_0: INPUT_ESC_RI },
    TableEntry { ch: '\\' as i32, interm: c"", type_0: INPUT_ESC_ST },
    TableEntry { ch: 'c' as i32, interm: c"", type_0: INPUT_ESC_RIS },
];

static INPUT_CSI_TABLE: [TableEntry; 43] = [
    TableEntry { ch: '@' as i32, interm: c"", type_0: INPUT_CSI_ICH },
    TableEntry { ch: 'A' as i32, interm: c"", type_0: INPUT_CSI_CUU },
    TableEntry { ch: 'B' as i32, interm: c"", type_0: INPUT_CSI_CUD },
    TableEntry { ch: 'C' as i32, interm: c"", type_0: INPUT_CSI_CUF },
    TableEntry { ch: 'D' as i32, interm: c"", type_0: INPUT_CSI_CUB },
    TableEntry { ch: 'E' as i32, interm: c"", type_0: INPUT_CSI_CNL },
    TableEntry { ch: 'F' as i32, interm: c"", type_0: INPUT_CSI_CPL },
    TableEntry { ch: 'G' as i32, interm: c"", type_0: INPUT_CSI_HPA },
    TableEntry { ch: 'H' as i32, interm: c"", type_0: INPUT_CSI_CUP },
    TableEntry { ch: 'J' as i32, interm: c"", type_0: INPUT_CSI_ED },
    TableEntry { ch: 'K' as i32, interm: c"", type_0: INPUT_CSI_EL },
    TableEntry { ch: 'L' as i32, interm: c"", type_0: INPUT_CSI_IL },
    TableEntry { ch: 'M' as i32, interm: c"", type_0: INPUT_CSI_DL },
    TableEntry { ch: 'P' as i32, interm: c"", type_0: INPUT_CSI_DCH },
    TableEntry { ch: 'S' as i32, interm: c"", type_0: INPUT_CSI_SU },
    TableEntry { ch: 'S' as i32, interm: c"?", type_0: INPUT_CSI_SM_GRAPHICS },
    TableEntry { ch: 'T' as i32, interm: c"", type_0: INPUT_CSI_SD },
    TableEntry { ch: 'X' as i32, interm: c"", type_0: INPUT_CSI_ECH },
    TableEntry { ch: 'Z' as i32, interm: c"", type_0: INPUT_CSI_CBT },
    TableEntry { ch: '`' as i32, interm: c"", type_0: INPUT_CSI_HPA },
    TableEntry { ch: 'b' as i32, interm: c"", type_0: INPUT_CSI_REP },
    TableEntry { ch: 'c' as i32, interm: c"", type_0: INPUT_CSI_DA },
    TableEntry { ch: 'c' as i32, interm: c">", type_0: INPUT_CSI_DA_TWO },
    TableEntry { ch: 'd' as i32, interm: c"", type_0: INPUT_CSI_VPA },
    TableEntry { ch: 'f' as i32, interm: c"", type_0: INPUT_CSI_CUP },
    TableEntry { ch: 'g' as i32, interm: c"", type_0: INPUT_CSI_TBC },
    TableEntry { ch: 'h' as i32, interm: c"", type_0: INPUT_CSI_SM },
    TableEntry { ch: 'h' as i32, interm: c"?", type_0: INPUT_CSI_SM_PRIVATE },
    TableEntry { ch: 'l' as i32, interm: c"", type_0: INPUT_CSI_RM },
    TableEntry { ch: 'l' as i32, interm: c"?", type_0: INPUT_CSI_RM_PRIVATE },
    TableEntry { ch: 'm' as i32, interm: c"", type_0: INPUT_CSI_SGR },
    TableEntry { ch: 'm' as i32, interm: c">", type_0: INPUT_CSI_MODSET },
    TableEntry { ch: 'n' as i32, interm: c"", type_0: INPUT_CSI_DSR },
    TableEntry { ch: 'n' as i32, interm: c">", type_0: INPUT_CSI_MODOFF },
    TableEntry { ch: 'n' as i32, interm: c"?", type_0: INPUT_CSI_DSR_PRIVATE },
    TableEntry { ch: 'p' as i32, interm: c"$", type_0: INPUT_CSI_QUERY },
    TableEntry { ch: 'p' as i32, interm: c"?$", type_0: INPUT_CSI_QUERY_PRIVATE },
    TableEntry { ch: 'q' as i32, interm: c" ", type_0: INPUT_CSI_DECSCUSR },
    TableEntry { ch: 'q' as i32, interm: c">", type_0: INPUT_CSI_XDA },
    TableEntry { ch: 'r' as i32, interm: c"", type_0: INPUT_CSI_DECSTBM },
    TableEntry { ch: 's' as i32, interm: c"", type_0: INPUT_CSI_SCP },
    TableEntry { ch: 't' as i32, interm: c"", type_0: INPUT_CSI_WINOPS },
    TableEntry { ch: 'u' as i32, interm: c"", type_0: INPUT_CSI_RCP },
];

pub struct Parser {
    pub interm_buf: [u8; 4],
    pub interm_len: usize,
    pub param_buf: [u8; 64],
    pub param_len: usize,
    pub input_buf: Vec<u8>,
    pub input_len: usize,
    pub input_end: InputEndType,
    pub param_list: [InputParam; 24],
    pub param_list_len: u32,
    pub ch: i32,
    state: &'static InputState,
    pub flags: i32,
    pub since_ground: Vec<u8>,
}

impl Default for Parser {
    fn default() -> Self {
        Self::new()
    }
}

impl Parser {
    pub fn new() -> Self {
        Self {
            interm_buf: [0; 4],
            interm_len: 0,
            param_buf: [0; 64],
            param_len: 0,
            input_buf: vec![0; INPUT_BUF_START],
            input_len: 0,
            input_end: INPUT_END_ST,
            param_list: std::array::from_fn(|_| InputParam::Missing),
            param_list_len: 0,
            ch: 0,
            state: &INPUT_STATE_GROUND,
            flags: 0,
            since_ground: Vec::new(),
        }
    }

    pub fn state_name(&self) -> &'static CStr {
        self.state.name
    }

    pub fn pending(&self) -> &[u8] {
        &self.since_ground
    }

    pub fn reset(&mut self) {
        self.clear_buffers();
        self.state = &INPUT_STATE_GROUND;
        self.flags = 0;
    }

    pub fn shrink_buffer(&mut self) {
        if self.input_buf.len() > INPUT_BUF_START {
            self.input_buf.truncate(INPUT_BUF_START);
            self.input_buf.shrink_to_fit();
        }
    }

    pub fn clear_params(&mut self) {
        for param in &mut self.param_list[..self.param_list_len as usize] {
            *param = InputParam::Missing;
        }
    }

    fn clear_buffers(&mut self) {
        self.interm_buf[0] = 0;
        self.interm_len = 0;
        self.param_buf[0] = 0;
        self.param_len = 0;
        self.input_buf[0] = 0;
        self.input_len = 0;
        self.input_end = INPUT_END_ST;
        self.flags &= !INPUT_DISCARD;
    }

    pub fn parse(&mut self, host: &mut dyn VtHost, buf: &[u8]) {
        let mut state: Option<&'static InputState> = None;
        let mut transition: Option<&'static InputTransition> = None;
        let mut index = 0;
        while let Some(&byte) = buf.get(index) {
            index += 1;
            if std::ptr::eq(self.state, &INPUT_STATE_GROUND) && (0x20..0x7f).contains(&byte) {
                let printed = host.print_ascii_run(&buf[index - 1..]);
                if printed != 0 {
                    let last = buf[index - 1 + printed - 1];
                    self.flags |= INPUT_LAST;
                    self.ch = last as i32;
                    index += printed - 1;
                    continue;
                }
            }
            self.ch = byte as i32;
            if !state.is_some_and(|s| std::ptr::eq(self.state, s))
                || !transition.is_some_and(|itr| self.ch >= itr.first && self.ch <= itr.last)
            {
                transition = self
                    .state
                    .transitions
                    .iter()
                    .take_while(|itr| itr.first != -1 && itr.last != -1)
                    .find(|itr| self.ch >= itr.first && self.ch <= itr.last);
            }
            let itr = transition.expect("no transition from state");
            state = Some(self.state);
            if !itr
                .handler
                .is_some_and(|handler| std::ptr::fn_addr_eq(handler, input_print as HandlerCb))
            {
                host.collect_end();
            }
            if let Some(handler) = itr.handler
                && handler(self, host) != 0
            {
                continue;
            }
            if let Some(next_state) = itr.state {
                self.set_state(host, next_state);
            }
            if !std::ptr::eq(self.state, &INPUT_STATE_GROUND) {
                self.since_ground.push(self.ch as u8);
            }
        }
    }

    fn set_state(&mut self, host: &mut dyn VtHost, state: &'static InputState) {
        if let Some(exit) = self.state.exit {
            exit(self, host);
        }
        self.state = state;
        if let Some(enter) = self.state.enter {
            enter(self, host);
        }
    }
}

fn c_is_space(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')
}

struct IntPrefix {
    negative: bool,
    magnitude: Option<u64>,
    consumed: usize,
}

fn integer_prefix_10(bytes: &[u8]) -> IntPrefix {
    let bytes = bytes
        .iter()
        .position(|&b| b == 0)
        .map_or(bytes, |end| &bytes[..end]);
    let mut index = 0;
    while index < bytes.len() && c_is_space(bytes[index]) {
        index += 1;
    }
    let mut negative = false;
    if let Some(&sign @ (b'+' | b'-')) = bytes.get(index) {
        negative = sign == b'-';
        index += 1;
    }
    let mut magnitude = Some(0u64);
    let mut any_digit = false;
    while let Some(&byte) = bytes.get(index) {
        if !byte.is_ascii_digit() {
            break;
        }
        let digit = (byte - b'0') as u64;
        magnitude = magnitude
            .and_then(|v| v.checked_mul(10))
            .and_then(|v| v.checked_add(digit));
        any_digit = true;
        index += 1;
    }
    if !any_digit {
        return IntPrefix {
            negative: false,
            magnitude: Some(0),
            consumed: 0,
        };
    }
    IntPrefix {
        negative,
        magnitude,
        consumed: index,
    }
}

fn strtonum_range(numstr: &CStr, minval: i64, maxval: i64) -> Result<i64, ()> {
    if minval > maxval {
        return Err(());
    }
    let bytes = numstr.to_bytes();
    let prefix = integer_prefix_10(bytes);
    if prefix.consumed == 0 || prefix.consumed != bytes.len() {
        return Err(());
    }
    let value = match (prefix.negative, prefix.magnitude) {
        (true, Some(magnitude)) if magnitude <= i64::MIN.unsigned_abs() => {
            0i64.wrapping_sub_unsigned(magnitude)
        }
        (true, _) => return Err(()),
        (false, Some(magnitude)) if magnitude <= i64::MAX as u64 => magnitude as i64,
        (false, _) => return Err(()),
    };
    if value < minval || value > maxval {
        Err(())
    } else {
        Ok(value)
    }
}

fn strtol_prefix_10(bytes: &[u8]) -> (i64, usize) {
    let prefix = integer_prefix_10(bytes);
    let value = match (prefix.negative, prefix.magnitude) {
        (true, Some(magnitude)) if magnitude <= i64::MIN.unsigned_abs() => {
            0i64.wrapping_sub_unsigned(magnitude)
        }
        (true, _) => i64::MIN,
        (false, Some(magnitude)) => i64::try_from(magnitude).unwrap_or(i64::MAX),
        (false, None) => i64::MAX,
    };
    (value, prefix.consumed)
}

fn cstr_suffix(s: &CStr, offset: usize) -> &CStr {
    let bytes = s.to_bytes_with_nul();
    CStr::from_bytes_with_nul(&bytes[offset..]).expect("valid CStr suffix")
}

fn table_find<'a>(table: &'a [TableEntry], parser: &Parser) -> Option<&'a TableEntry> {
    let interm = &parser.interm_buf[..parser.interm_len];
    table
        .binary_search_by(|entry| {
            entry
                .ch
                .cmp(&parser.ch)
                .then_with(|| entry.interm.to_bytes().iter().cmp(interm))
        })
        .ok()
        .map(|index| &table[index])
}

fn split_fields(buf: &mut [u8], delim: u8) -> impl Iterator<Item = &CStr> {
    let len = CStr::from_bytes_until_nul(buf)
        .expect("field buffer is terminated")
        .count_bytes();
    buf[..len]
        .iter_mut()
        .filter(|byte| **byte == delim)
        .for_each(|byte| *byte = 0);
    buf[..=len]
        .split_inclusive(|&byte| byte == 0)
        .map(|field| CStr::from_bytes_with_nul(field).expect("field is terminated"))
}

pub fn input_split(parser: &mut Parser) -> i32 {
    parser.clear_params();
    parser.param_list_len = 0;
    if parser.param_len == 0 {
        return 0;
    }
    let mut params = parser.param_buf;
    for out in split_fields(&mut params, b';') {
        let ip = &mut parser.param_list[parser.param_list_len as usize];
        if out.is_empty() {
            *ip = InputParam::Missing;
        } else if out.to_bytes().contains(&b':') {
            *ip = InputParam::String(out.to_owned());
        } else {
            let Ok(number) = strtonum_range(out, 0, i32::MAX as i64) else {
                return -1;
            };
            *ip = InputParam::Number(number as i32);
        }
        parser.param_list_len = parser.param_list_len.wrapping_add(1);
        if parser.param_list_len as usize == parser.param_list.len() {
            return -1;
        }
    }
    0
}

pub fn input_get(parser: &Parser, validx: u32, minval: i32, defval: i32) -> i32 {
    if validx >= parser.param_list_len {
        return defval;
    }
    let retval = match &parser.param_list[validx as usize] {
        InputParam::Missing => return defval,
        InputParam::String(_) => return -1,
        InputParam::Number(value) => *value,
    };
    if retval < minval {
        return minval;
    }
    retval
}

fn input_clear(parser: &mut Parser, host: &mut dyn VtHost) {
    host.clear_ground_timer();
    parser.clear_buffers();
}

fn input_ground(parser: &mut Parser, host: &mut dyn VtHost) {
    host.clear_ground_timer();
    parser.since_ground.clear();
    parser.since_ground.shrink_to(SINCE_GROUND_KEEP);
    parser.shrink_buffer();
}

fn input_print(parser: &mut Parser, host: &mut dyn VtHost) -> i32 {
    host.print_ascii(parser.ch as u8);
    parser.flags |= INPUT_LAST;
    0
}

fn input_intermediate(parser: &mut Parser, _host: &mut dyn VtHost) -> i32 {
    if parser.interm_len == parser.interm_buf.len() - 1 {
        parser.flags |= INPUT_DISCARD;
    } else {
        let idx = parser.interm_len;
        parser.interm_len += 1;
        parser.interm_buf[idx] = parser.ch as u8;
        parser.interm_buf[parser.interm_len] = 0;
    }
    0
}

fn input_parameter(parser: &mut Parser, _host: &mut dyn VtHost) -> i32 {
    if parser.param_len == parser.param_buf.len() - 1 {
        parser.flags |= INPUT_DISCARD;
    } else {
        let idx = parser.param_len;
        parser.param_len += 1;
        parser.param_buf[idx] = parser.ch as u8;
        parser.param_buf[parser.param_len] = 0;
    }
    0
}

pub fn input_input(parser: &mut Parser, _host: &mut dyn VtHost) -> i32 {
    let mut available = parser.input_buf.len();
    while parser.input_len.wrapping_add(1) >= available {
        available = available.wrapping_mul(2);
        if available > INPUT_BUFFER_SIZE.get() {
            parser.flags |= INPUT_DISCARD;
            return 0;
        }
        parser.input_buf.resize(available, 0);
    }
    let idx = parser.input_len;
    parser.input_len += 1;
    parser.input_buf[idx] = parser.ch as u8;
    parser.input_buf[parser.input_len] = 0;
    0
}

fn input_top_bit_set(parser: &mut Parser, host: &mut dyn VtHost) -> i32 {
    parser.flags &= !INPUT_LAST;
    if host.print_utf8_byte(parser.ch as u8) {
        parser.flags |= INPUT_LAST;
    }
    0
}

fn input_c0_dispatch(parser: &mut Parser, host: &mut dyn VtHost) -> i32 {
    host.stop_utf8();
    match parser.ch {
        0 => {}
        7 => host.bell(),
        8 => host.backspace(),
        9 => host.horizontal_tab(),
        10..=12 => host.linefeed(true),
        13 => host.carriage_return(),
        14 => host.select_charset(1),
        15 => host.select_charset(0),
        _ => {}
    }
    parser.flags &= !INPUT_LAST;
    0
}

fn input_esc_dispatch(parser: &mut Parser, host: &mut dyn VtHost) -> i32 {
    if parser.flags & INPUT_DISCARD != 0 {
        return 0;
    }
    let Some(entry) = table_find(&INPUT_ESC_TABLE, parser) else {
        return 0;
    };
    match entry.type_0 {
        INPUT_ESC_RIS => {
            host.set_program_status(None);
            host.hard_reset();
        }
        INPUT_ESC_IND => host.linefeed(false),
        INPUT_ESC_NEL => {
            host.carriage_return();
            host.linefeed(false);
        }
        INPUT_ESC_HTS => host.set_tab_at_cursor(true),
        INPUT_ESC_RI => host.reverse_index(),
        INPUT_ESC_DECKPAM => host.set_keypad_mode(true),
        INPUT_ESC_DECKPNM => host.set_keypad_mode(false),
        INPUT_ESC_DECSC => host.save_state(),
        INPUT_ESC_DECRC => host.restore_state(),
        INPUT_ESC_DECALN => host.alignment_test(),
        INPUT_ESC_SCSG0_ON => host.define_charset(0, true),
        INPUT_ESC_SCSG0_OFF => host.define_charset(0, false),
        INPUT_ESC_SCSG1_ON => host.define_charset(1, true),
        INPUT_ESC_SCSG1_OFF => host.define_charset(1, false),
        _ => {}
    }
    parser.flags &= !INPUT_LAST;
    0
}

fn input_csi_dispatch(parser: &mut Parser, host: &mut dyn VtHost) -> i32 {
    if parser.flags & INPUT_DISCARD != 0 {
        return 0;
    }
    if input_split(parser) != 0 {
        return 0;
    }
    let Some(entry) = table_find(&INPUT_CSI_TABLE, parser) else {
        return 0;
    };
    match entry.type_0 {
        INPUT_CSI_CBT => {
            let n = input_get(parser, 0, 1, 1);
            if n != -1 {
                host.cursor_back_tab(n as u32);
            }
        }
        INPUT_CSI_CUB => {
            let n = input_get(parser, 0, 1, 1);
            if n != -1 {
                host.cursor_left(n as u32);
            }
        }
        INPUT_CSI_CUD => {
            let n = input_get(parser, 0, 1, 1);
            if n != -1 {
                host.cursor_down(n as u32);
            }
        }
        INPUT_CSI_CUF => {
            let n = input_get(parser, 0, 1, 1);
            if n != -1 {
                host.cursor_right(n as u32);
            }
        }
        INPUT_CSI_CUP => {
            let n = input_get(parser, 0, 1, 1);
            let m = input_get(parser, 1, 1, 1);
            if n != -1 && m != -1 {
                host.cursor_move(m - 1, n - 1);
            }
        }
        INPUT_CSI_MODSET => {
            let n = input_get(parser, 0, 0, 0);
            if n == 4 {
                let m = input_get(parser, 1, 0, 0);
                host.set_extended_keys_mode(Some(m));
            }
        }
        INPUT_CSI_MODOFF => {
            let n = input_get(parser, 0, 0, 0);
            if n == 4 {
                host.set_extended_keys_mode(None);
            }
        }
        INPUT_CSI_WINOPS => dispatch_winops(parser, host),
        INPUT_CSI_CUU => {
            let n = input_get(parser, 0, 1, 1);
            if n != -1 {
                host.cursor_up(n as u32);
            }
        }
        INPUT_CSI_CNL => {
            let n = input_get(parser, 0, 1, 1);
            if n != -1 {
                host.carriage_return();
                host.cursor_down(n as u32);
            }
        }
        INPUT_CSI_CPL => {
            let n = input_get(parser, 0, 1, 1);
            if n != -1 {
                host.carriage_return();
                host.cursor_up(n as u32);
            }
        }
        INPUT_CSI_DA => {
            if input_get(parser, 0, 0, 0) == 0 {
                host.reply(true, b"\x1B[?1;2c");
            }
        }
        INPUT_CSI_DA_TWO => {
            if input_get(parser, 0, 0, 0) == 0 {
                host.reply(true, b"\x1B[>84;0;0c");
            }
        }
        INPUT_CSI_ECH => {
            let n = input_get(parser, 0, 1, 1);
            if n != -1 {
                host.erase_chars(n as u32);
            }
        }
        INPUT_CSI_DCH => {
            let n = input_get(parser, 0, 1, 1);
            if n != -1 {
                host.delete_chars(n as u32);
            }
        }
        INPUT_CSI_DECSTBM => {
            let n = input_get(parser, 0, 1, 1);
            let m = input_get(parser, 1, 1, host.screen_height() as i32);
            if n != -1 && m != -1 {
                host.set_scroll_region((n - 1) as u32, (m - 1) as u32);
            }
        }
        INPUT_CSI_DL => {
            let n = input_get(parser, 0, 1, 1);
            if n != -1 {
                host.delete_lines(n as u32);
            }
        }
        INPUT_CSI_DSR_PRIVATE => {
            if input_get(parser, 0, 0, 0) == 996 {
                host.query_theme();
            }
        }
        INPUT_CSI_QUERY => {
            let m = input_get(parser, 0, 0, 0);
            if m > 0 {
                host.query_mode(m, false);
            }
        }
        INPUT_CSI_QUERY_PRIVATE => {
            let m = input_get(parser, 0, 0, 0);
            if m > 0 {
                host.query_mode(m, true);
            }
        }
        INPUT_CSI_DSR => match input_get(parser, 0, 0, 0) {
            5 => host.reply(true, b"\x1B[0n"),
            6 => host.query_cursor_position(),
            _ => {}
        },
        INPUT_CSI_ED => {
            let mode = input_get(parser, 0, 0, 0);
            if mode != -1 {
                let submode = input_get(parser, 1, 0, 0);
                host.erase_in_display(mode, submode);
            }
        }
        INPUT_CSI_EL => {
            let mode = input_get(parser, 0, 0, 0);
            if mode != -1 {
                host.erase_in_line(mode);
            }
        }
        INPUT_CSI_HPA => {
            let n = input_get(parser, 0, 1, 1);
            if n != -1 {
                host.cursor_move(n - 1, -1);
            }
        }
        INPUT_CSI_ICH => {
            let n = input_get(parser, 0, 1, 1);
            if n != -1 {
                host.insert_chars(n as u32);
            }
        }
        INPUT_CSI_IL => {
            let n = input_get(parser, 0, 1, 1);
            if n != -1 {
                host.insert_lines(n as u32);
            }
        }
        INPUT_CSI_REP => {
            let n = input_get(parser, 0, 1, 1);
            if n != -1 && (parser.flags & INPUT_LAST != 0) {
                host.repeat_last(n as u32);
            }
        }
        INPUT_CSI_RCP => host.restore_state(),
        INPUT_CSI_RM => dispatch_modes(parser, host, false, false),
        INPUT_CSI_RM_PRIVATE => dispatch_modes(parser, host, true, false),
        INPUT_CSI_SCP => host.save_state(),
        INPUT_CSI_SGR => dispatch_sgr(parser, host),
        INPUT_CSI_SM => dispatch_modes(parser, host, false, true),
        INPUT_CSI_SM_PRIVATE => dispatch_modes(parser, host, true, true),
        INPUT_CSI_SM_GRAPHICS => {}
        INPUT_CSI_SU => {
            let n = input_get(parser, 0, 1, 1);
            if n != -1 {
                host.scroll_up(n as u32);
            }
        }
        INPUT_CSI_SD => {
            let n = input_get(parser, 0, 1, 1);
            if n != -1 {
                host.scroll_down(n as u32);
            }
        }
        INPUT_CSI_TBC => match input_get(parser, 0, 0, 0) {
            0 => host.set_tab_at_cursor(false),
            3 => host.clear_all_tabs(),
            _ => {}
        },
        INPUT_CSI_VPA => {
            let n = input_get(parser, 0, 1, 1);
            if n != -1 {
                host.cursor_move(-1, n - 1);
            }
        }
        INPUT_CSI_DECSCUSR => {
            let n = input_get(parser, 0, 0, 0);
            if n != -1 {
                host.set_cursor_style(n as u32);
            }
        }
        INPUT_CSI_XDA => {
            if input_get(parser, 0, 0, 0) == 0 {
                host.query_xda();
            }
        }
        _ => {}
    }
    parser.flags &= !INPUT_LAST;
    0
}

fn dispatch_modes(parser: &Parser, host: &mut dyn VtHost, private: bool, enabled: bool) {
    let mut i = 0;
    while i < parser.param_list_len {
        let mode = input_get(parser, i, 0, -1);
        if mode != -1 {
            host.set_mode(mode, private, enabled);
        }
        i += 1;
    }
}

fn dispatch_winops(parser: &Parser, host: &mut dyn VtHost) {
    let mut m: u32 = 0;
    loop {
        let n = input_get(parser, m, 0, -1);
        if n == -1 {
            break;
        }
        let mut consume_extra = false;
        match n {
            1 | 2 | 5 | 6 | 7 | 11 | 13 | 20 | 21 | 24 => {}
            3 | 4 | 8 => {
                m += 1;
                if input_get(parser, m, 0, -1) == -1 {
                    break;
                }
                consume_extra = true;
            }
            9 | 10 => {
                consume_extra = true;
            }
            14 => host.winop(WinOp::ReportTextSizePixels),
            15 => host.winop(WinOp::ReportScreenSizePixels),
            16 => host.winop(WinOp::ReportCellSizePixels),
            18 => host.winop(WinOp::ReportTextSizeChars),
            19 => host.winop(WinOp::ReportScreenSizeChars),
            22 => {
                m += 1;
                match input_get(parser, m, 0, -1) {
                    -1 => break,
                    0 | 2 => host.winop(WinOp::PushTitle),
                    _ => {}
                }
            }
            23 => {
                m += 1;
                match input_get(parser, m, 0, -1) {
                    -1 => break,
                    0 | 2 => host.winop(WinOp::PopTitle),
                    _ => {}
                }
            }
            _ => {}
        }
        if consume_extra {
            m += 1;
            if input_get(parser, m, 0, -1) == -1 {
                break;
            }
        }
        m += 1;
    }
}

fn sgr_256_do(host: &mut dyn VtHost, fgbg: i32, c: i32) -> bool {
    if c == -1 || c > 255 {
        if fgbg == 38 {
            host.apply_sgr(SgrOp::Fg(SgrColor::Default));
        } else if fgbg == 48 {
            host.apply_sgr(SgrOp::Bg(SgrColor::Default));
        }
    } else if fgbg == 38 {
        host.apply_sgr(SgrOp::Fg(SgrColor::Indexed(c as u8)));
    } else if fgbg == 48 {
        host.apply_sgr(SgrOp::Bg(SgrColor::Indexed(c as u8)));
    } else if fgbg == 58 {
        host.apply_sgr(SgrOp::Us(SgrColor::Indexed(c as u8)));
    }
    true
}

fn sgr_rgb_do(host: &mut dyn VtHost, fgbg: i32, r: i32, g: i32, b: i32) -> bool {
    if r == -1 || r > 255 || g == -1 || g > 255 || b == -1 || b > 255 {
        return false;
    }
    let color = SgrColor::Rgb(r as u8, g as u8, b as u8);
    if fgbg == 38 {
        host.apply_sgr(SgrOp::Fg(color));
    } else if fgbg == 48 {
        host.apply_sgr(SgrOp::Bg(color));
    } else if fgbg == 58 {
        host.apply_sgr(SgrOp::Us(color));
    }
    true
}

pub fn dispatch_sgr_colon(parser: &Parser, host: &mut dyn VtHost, mut i: u32) {
    let s = match &parser.param_list[i as usize] {
        InputParam::String(value) => value.as_c_str(),
        _ => panic!("SGR colon parser requires a string parameter"),
    };
    let mut p = [-1i32; 8];
    let mut n: u32 = 0;
    let mut copy = s.to_bytes_with_nul().to_vec();
    for out in split_fields(&mut copy, b':') {
        if !out.is_empty() {
            let slot = n;
            n += 1;
            let parsed = strtonum_range(out, 0, i32::MAX as i64);
            p[slot as usize] = parsed.unwrap_or(0) as i32;
            if parsed.is_err() || n as usize == p.len() {
                return;
            }
        } else {
            n += 1;
            if n as usize == p.len() {
                return;
            }
        }
    }
    if n == 0 {
        return;
    }
    if p[0] == 4 {
        if n != 2 {
            return;
        }
        match p[1] {
            0 => host.apply_sgr(SgrOp::ClearAllUnderscore),
            1 => host.apply_sgr(SgrOp::SetAttr(SgrAttr::Underscore)),
            2 => host.apply_sgr(SgrOp::SetAttr(SgrAttr::Underscore2)),
            3 => host.apply_sgr(SgrOp::SetAttr(SgrAttr::Underscore3)),
            4 => host.apply_sgr(SgrOp::SetAttr(SgrAttr::Underscore4)),
            5 => host.apply_sgr(SgrOp::SetAttr(SgrAttr::Underscore5)),
            _ => {}
        }
        return;
    }
    if n < 2 || (p[0] != 38 && p[0] != 48 && p[0] != 58) {
        return;
    }
    match p[1] {
        2 => {
            if n >= 3 {
                i = if n == 5 { 2 } else { 3 };
                if n >= i + 3 {
                    sgr_rgb_do(
                        host,
                        p[0],
                        p[i as usize],
                        p[(i + 1) as usize],
                        p[(i + 2) as usize],
                    );
                }
            }
        }
        5 if n >= 3 => {
            sgr_256_do(host, p[0], p[2]);
        }
        _ => {}
    }
}

fn dispatch_sgr(parser: &Parser, host: &mut dyn VtHost) {
    if parser.param_list_len == 0 {
        host.apply_sgr(SgrOp::ResetAll);
        return;
    }
    let mut i: u32 = 0;
    while i < parser.param_list_len {
        if matches!(&parser.param_list[i as usize], InputParam::String(_)) {
            dispatch_sgr_colon(parser, host, i);
        } else {
            let n = input_get(parser, i, 0, 0);
            if n != -1 {
                if n == 38 || n == 48 || n == 58 {
                    i += 1;
                    match input_get(parser, i, 0, -1) {
                        2 => {
                            let r = input_get(parser, i + 1, 0, -1);
                            let g = input_get(parser, i + 2, 0, -1);
                            let b = input_get(parser, i + 3, 0, -1);
                            if sgr_rgb_do(host, n, r, g, b) {
                                i += 3;
                            }
                        }
                        5 => {
                            let c = input_get(parser, i + 1, 0, -1);
                            if sgr_256_do(host, n, c) {
                                i += 1;
                            }
                        }
                        _ => {}
                    }
                } else {
                    match n {
                        0 => host.apply_sgr(SgrOp::ResetKeepLink),
                        1 => host.apply_sgr(SgrOp::SetAttr(SgrAttr::Bright)),
                        2 => host.apply_sgr(SgrOp::SetAttr(SgrAttr::Dim)),
                        3 => host.apply_sgr(SgrOp::SetAttr(SgrAttr::Italics)),
                        4 => host.apply_sgr(SgrOp::SetAttr(SgrAttr::Underscore)),
                        5 | 6 => host.apply_sgr(SgrOp::SetAttr(SgrAttr::Blink)),
                        7 => host.apply_sgr(SgrOp::SetAttr(SgrAttr::Reverse)),
                        8 => host.apply_sgr(SgrOp::SetAttr(SgrAttr::Hidden)),
                        9 => host.apply_sgr(SgrOp::SetAttr(SgrAttr::Strikethrough)),
                        21 => host.apply_sgr(SgrOp::SetAttr(SgrAttr::Underscore2)),
                        22 => host.apply_sgr(SgrOp::ClearBrightDim),
                        23 => host.apply_sgr(SgrOp::ClearItalics),
                        24 => host.apply_sgr(SgrOp::ClearAllUnderscore),
                        25 => host.apply_sgr(SgrOp::ClearBlink),
                        27 => host.apply_sgr(SgrOp::ClearReverse),
                        28 => host.apply_sgr(SgrOp::ClearHidden),
                        29 => host.apply_sgr(SgrOp::ClearStrikethrough),
                        30..=37 => host.apply_sgr(SgrOp::Fg(SgrColor::Ansi((n - 30) as u8))),
                        39 => host.apply_sgr(SgrOp::Fg(SgrColor::Default)),
                        40..=47 => host.apply_sgr(SgrOp::Bg(SgrColor::Ansi((n - 40) as u8))),
                        49 => host.apply_sgr(SgrOp::Bg(SgrColor::Default)),
                        53 => host.apply_sgr(SgrOp::SetAttr(SgrAttr::Overline)),
                        55 => host.apply_sgr(SgrOp::ClearOverline),
                        59 => host.apply_sgr(SgrOp::Us(SgrColor::Default)),
                        90..=97 => host.apply_sgr(SgrOp::Fg(SgrColor::Ansi(n as u8))),
                        100..=107 => host.apply_sgr(SgrOp::Bg(SgrColor::Ansi((n - 10) as u8))),
                        _ => {}
                    }
                }
            }
        }
        i += 1;
    }
}

fn input_end_bel(parser: &mut Parser, _host: &mut dyn VtHost) -> i32 {
    parser.input_end = INPUT_END_BEL;
    0
}

fn input_enter_dcs(parser: &mut Parser, host: &mut dyn VtHost) {
    input_clear(parser, host);
    host.start_ground_timer();
    parser.flags &= !INPUT_LAST;
}

fn input_dcs_dispatch(parser: &mut Parser, host: &mut dyn VtHost) -> i32 {
    const PREFIX: &[u8] = b"tmux;";
    let len = parser.input_len;
    if parser.flags & INPUT_DISCARD != 0 {
        return 0;
    }
    if parser.interm_len == 1
        && parser.interm_buf[0] == b'$'
        && len >= 1
        && parser.input_buf[0] == b'q'
    {
        if len < 3 || parser.input_buf[1] != b' ' || parser.input_buf[2] != b'q' {
            host.reply(true, b"\x1BP0$r\x1B\\");
        } else {
            host.query_decrqss_cursor_style();
        }
        return 0;
    }
    if parser.input_buf[..len].starts_with(PREFIX) {
        host.dcs_tmux_passthrough(&parser.input_buf[PREFIX.len()..len]);
    }
    0
}

fn input_enter_osc(parser: &mut Parser, host: &mut dyn VtHost) {
    input_clear(parser, host);
    host.start_ground_timer();
    parser.flags &= !INPUT_LAST;
}

fn input_exit_osc(parser: &mut Parser, host: &mut dyn VtHost) {
    if parser.flags & INPUT_DISCARD != 0 {
        return;
    }
    let input = CStr::from_bytes_until_nul(&parser.input_buf)
        .expect("input buffer has a terminator")
        .to_owned();
    let input = input.as_c_str();
    let bytes = input.to_bytes();
    if parser.input_len < 1 || !bytes.first().is_some_and(u8::is_ascii_digit) {
        return;
    }
    let mut option: u32 = 0;
    let mut offset = 0;
    while offset < bytes.len() && bytes[offset].is_ascii_digit() {
        option = option
            .wrapping_mul(10)
            .wrapping_add(bytes[offset] as u32)
            .wrapping_sub(b'0' as u32);
        offset += 1;
    }
    match bytes.get(offset) {
        Some(b';') => offset += 1,
        Some(_) => return,
        None => {}
    }
    let p = cstr_suffix(input, offset);
    let end_type = parser.input_end;
    match option {
        0 | 2 => host.set_title(p),
        4 => dispatch_osc_4(host, p, end_type),
        7 => host.set_path(p),
        8 => dispatch_osc_8(host, p),
        9 => dispatch_osc_9(host, p),
        10 => {
            if p == c"?" {
                host.osc_10(None, end_type);
            } else {
                host.osc_10(Some(p), end_type);
            }
        }
        11 => {
            if p == c"?" {
                host.osc_11(None, end_type);
            } else {
                host.osc_11(Some(p), end_type);
            }
        }
        12 => {
            if p == c"?" {
                host.osc_12(None, end_type);
            } else {
                host.osc_12(Some(p), end_type);
            }
        }
        52 => dispatch_osc_52(host, p, end_type),
        104 => dispatch_osc_104(host, p),
        110 => {
            if p.is_empty() {
                host.osc_110();
            }
        }
        111 => {
            if p.is_empty() {
                host.osc_111();
            }
        }
        112 => {
            if p.is_empty() {
                host.osc_112();
            }
        }
        133 => dispatch_osc_133(host, p),
        7501 => dispatch_osc_7501(host, p, end_type),
        _ => {}
    }
}

fn dispatch_osc_4(host: &mut dyn VtHost, p: &CStr, end_type: InputEndType) {
    let mut s: Option<&CStr> = Some(p);
    let mut redraw = false;
    while let Some(field) = s.filter(|field| !field.is_empty()) {
        let (idx, end) = strtol_prefix_10(field.to_bytes());
        if field.to_bytes().get(end) != Some(&b';') || !(0..256).contains(&idx) {
            break;
        }
        let rest = cstr_suffix(field, end + 1);
        let (spec, next) = match rest.to_bytes().iter().position(|&byte| byte == b';') {
            Some(semi) => (&rest.to_bytes()[..semi], Some(cstr_suffix(rest, semi + 1))),
            None => (rest.to_bytes(), None),
        };
        if spec == b"?" {
            host.osc_4_query(idx as i32, end_type);
        } else {
            let spec = CString::new(spec).expect("OSC 4 colour ends before the first NUL");
            if host.osc_4_set(idx as i32, &spec) {
                redraw = true;
            }
        }
        s = next;
    }
    if redraw {
        host.full_redraw();
    }
}

fn dispatch_osc_8(host: &mut dyn VtHost, p: &CStr) {
    let bytes = p.to_bytes();
    let mut start = 0;
    let mut id: Option<CString> = None;
    let uri = loop {
        let Some(end) = bytes[start..]
            .iter()
            .position(|&byte| byte == b':' || byte == b';')
            .map(|end| start + end)
        else {
            break None;
        };
        let field = &bytes[start..end];
        if field.len() >= 4 && field.starts_with(b"id=") {
            if id.is_some() {
                break None;
            }
            id = Some(CString::new(&field[3..]).expect("OSC 8 ID ends before the first NUL"));
        }
        if bytes[end] == b';' {
            break Some(cstr_suffix(p, end + 1));
        }
        start = end + 1;
    };
    if let Some(uri) = uri {
        host.set_hyperlink(uri, id.as_deref());
    }
}

fn dispatch_osc_9(host: &mut dyn VtHost, p: &CStr) {
    let pb = p.to_bytes_with_nul();
    if pb[0] != b'4' || pb[1] != b';' || pb[2] == b'\0' {
        return;
    }
    if (b'0'..=b'4').contains(&pb[2]) {
        let state = (pb[2] - b'0') as u32;
        if pb[3] == b'\0' || (pb[3] == b';' && pb[4] == b'\0') {
            host.set_progress_bar(state, -1);
            return;
        }
        if pb[3] == b';' {
            let mut progress: i32 = 0;
            let mut i = 4;
            while pb[i].is_ascii_digit() && progress <= 100 {
                progress = progress * 10 + (pb[i] - b'0') as i32;
                i += 1;
            }
            if pb[i] == b'\0' && (0..=100).contains(&progress) {
                host.set_progress_bar(state, progress);
            }
        }
    }
}

fn dispatch_osc_52(host: &mut dyn VtHost, p: &CStr, end_type: InputEndType) {
    let allow = b"cpqs01234567";
    if !host.allow_set_clipboard() {
        return;
    }
    let bytes = p.to_bytes();
    let Some(semi) = bytes.iter().position(|&byte| byte == b';') else {
        return;
    };
    let end = cstr_suffix(p, semi + 1);
    if end.is_empty() {
        return;
    }
    let mut clip = [0u8; 13];
    let mut j = 0;
    for &byte in &bytes[..=semi] {
        if allow.contains(&byte) && !clip[..j].contains(&byte) {
            clip[j] = byte;
            j += 1;
        }
    }
    if end == c"?" {
        host.osc_52_query(clip[0] as c_char, end_type);
        return;
    }
    let Some(out) = base64::decode(end.to_bytes()) else {
        return;
    };
    let clip = CStr::from_bytes_until_nul(&clip).expect("terminated clipboard selectors");
    host.osc_52_set(clip, out);
}

fn dispatch_osc_104(host: &mut dyn VtHost, p: &CStr) {
    if p.is_empty() {
        host.osc_104_reset_all();
        return;
    }
    let mut redraw = false;
    let mut s = p;
    while !s.is_empty() {
        let (idx, end) = strtol_prefix_10(s.to_bytes());
        s = cstr_suffix(s, end);
        if (!s.is_empty() && s.to_bytes()[0] != b';') || !(0..256).contains(&idx) {
            break;
        }
        if host.osc_104_reset_one(idx as i32) {
            redraw = true;
        }
        if s.to_bytes().first() == Some(&b';') {
            s = cstr_suffix(s, 1);
        }
    }
    if redraw {
        host.full_redraw();
    }
}

pub fn osc_133_exit_status(p: &CStr) -> i32 {
    let bytes = p.to_bytes();
    if bytes.get(1) != Some(&b';') || bytes.get(2).is_none_or(|&byte| byte == b'=') {
        return 0;
    }
    let tail = &bytes[2..];
    let end = tail
        .iter()
        .position(|&byte| byte == b';')
        .unwrap_or(tail.len());
    if end == 0 {
        return 0;
    }
    let number = &tail[..end];
    if number.contains(&b'=') {
        return 0;
    }
    let copy = CString::new(number).expect("OSC 133 status token contains no NUL");
    match strtonum_range(&copy, 0, 255) {
        Ok(status) => status as i32,
        Err(()) => 255,
    }
}

fn dispatch_osc_133(host: &mut dyn VtHost, p: &CStr) {
    let bytes = p.to_bytes_with_nul();
    match bytes[0] {
        b'A' | b'N' => {
            host.set_program_status(None);
            host.osc_133(Osc133Command::PromptStart);
        }
        b'P' => {
            let secondary = bytes
                .windows(4)
                .position(|window| window == b";k=s")
                .is_some_and(|cp| bytes[cp + 4] == b';' || bytes[cp + 4] == b'\0');
            host.osc_133(Osc133Command::Prompt { secondary });
        }
        b'B' | b'I' => host.osc_133(Osc133Command::CommandStart),
        b'C' => {
            host.set_program_status(None);
            host.osc_133(Osc133Command::OutputStart);
        }
        b'D' => {
            let exit_status = osc_133_exit_status(p);
            host.set_program_status(None);
            host.osc_133(Osc133Command::CommandFinished { exit_status });
        }
        _ => {}
    }
}

fn dispatch_osc_7501(host: &mut dyn VtHost, p: &CStr, end_type: InputEndType) {
    if p == c"?" {
        let mut reply = Vec::with_capacity(OSC_7501_QUERY_REPLY_PREFIX.len() + 2);
        reply.extend_from_slice(OSC_7501_QUERY_REPLY_PREFIX);
        if end_type == INPUT_END_BEL {
            reply.push(0x07);
        } else {
            reply.extend_from_slice(b"\x1B\\");
        }
        host.reply(true, &reply);
        return;
    }
    let Some(update) = parse_osc_7501(p.to_bytes()) else {
        return;
    };
    match update {
        Osc7501Update::Clear => host.set_program_status(None),
        Osc7501Update::Status(status) => host.set_program_status(Some(status)),
        Osc7501Update::Ignore => {}
    }
}

fn input_enter_apc(parser: &mut Parser, host: &mut dyn VtHost) {
    input_clear(parser, host);
    host.start_ground_timer();
    parser.flags &= !INPUT_LAST;
}

fn input_exit_apc(parser: &mut Parser, host: &mut dyn VtHost) {
    if parser.flags & INPUT_DISCARD != 0 {
        return;
    }
    let title = CStr::from_bytes_until_nul(&parser.input_buf)
        .expect("input buffer has a terminator")
        .to_owned();
    host.set_title(&title);
}

fn input_enter_rename(parser: &mut Parser, host: &mut dyn VtHost) {
    input_clear(parser, host);
    host.start_ground_timer();
    parser.flags &= !INPUT_LAST;
}

fn input_exit_rename_state(parser: &mut Parser, host: &mut dyn VtHost) {
    if parser.flags & INPUT_DISCARD != 0 {
        return;
    }
    host.exit_rename(&parser.input_buf, parser.input_len);
}

static INPUT_STATE_GROUND: InputState = InputState {
    name: c"ground",
    enter: Some(input_ground),
    exit: None,
    transitions: &INPUT_STATE_GROUND_TABLE,
};
static INPUT_STATE_ESC_ENTER: InputState = InputState {
    name: c"esc_enter",
    enter: Some(input_clear),
    exit: None,
    transitions: &INPUT_STATE_ESC_ENTER_TABLE,
};
static INPUT_STATE_ESC_INTERMEDIATE: InputState = InputState {
    name: c"esc_intermediate",
    enter: None,
    exit: None,
    transitions: &INPUT_STATE_ESC_INTERMEDIATE_TABLE,
};
static INPUT_STATE_CSI_ENTER: InputState = InputState {
    name: c"csi_enter",
    enter: Some(input_clear),
    exit: None,
    transitions: &INPUT_STATE_CSI_ENTER_TABLE,
};
static INPUT_STATE_CSI_PARAMETER: InputState = InputState {
    name: c"csi_parameter",
    enter: None,
    exit: None,
    transitions: &INPUT_STATE_CSI_PARAMETER_TABLE,
};
static INPUT_STATE_CSI_INTERMEDIATE: InputState = InputState {
    name: c"csi_intermediate",
    enter: None,
    exit: None,
    transitions: &INPUT_STATE_CSI_INTERMEDIATE_TABLE,
};
static INPUT_STATE_CSI_IGNORE: InputState = InputState {
    name: c"csi_ignore",
    enter: None,
    exit: None,
    transitions: &INPUT_STATE_CSI_IGNORE_TABLE,
};
static INPUT_STATE_DCS_ENTER: InputState = InputState {
    name: c"dcs_enter",
    enter: Some(input_enter_dcs),
    exit: None,
    transitions: &INPUT_STATE_DCS_ENTER_TABLE,
};
static INPUT_STATE_DCS_PARAMETER: InputState = InputState {
    name: c"dcs_parameter",
    enter: None,
    exit: None,
    transitions: &INPUT_STATE_DCS_PARAMETER_TABLE,
};
static INPUT_STATE_DCS_INTERMEDIATE: InputState = InputState {
    name: c"dcs_intermediate",
    enter: None,
    exit: None,
    transitions: &INPUT_STATE_DCS_INTERMEDIATE_TABLE,
};
static INPUT_STATE_DCS_HANDLER: InputState = InputState {
    name: c"dcs_handler",
    enter: None,
    exit: None,
    transitions: &INPUT_STATE_DCS_HANDLER_TABLE,
};
static INPUT_STATE_DCS_ESCAPE: InputState = InputState {
    name: c"dcs_escape",
    enter: None,
    exit: None,
    transitions: &INPUT_STATE_DCS_ESCAPE_TABLE,
};
static INPUT_STATE_DCS_IGNORE: InputState = InputState {
    name: c"dcs_ignore",
    enter: None,
    exit: None,
    transitions: &INPUT_STATE_DCS_IGNORE_TABLE,
};
static INPUT_STATE_OSC_STRING: InputState = InputState {
    name: c"osc_string",
    enter: Some(input_enter_osc),
    exit: Some(input_exit_osc),
    transitions: &INPUT_STATE_OSC_STRING_TABLE,
};
static INPUT_STATE_APC_STRING: InputState = InputState {
    name: c"apc_string",
    enter: Some(input_enter_apc),
    exit: Some(input_exit_apc),
    transitions: &INPUT_STATE_APC_STRING_TABLE,
};
static INPUT_STATE_RENAME_STRING: InputState = InputState {
    name: c"rename_string",
    enter: Some(input_enter_rename),
    exit: Some(input_exit_rename_state),
    transitions: &INPUT_STATE_RENAME_STRING_TABLE,
};
static INPUT_STATE_CONSUME_ST: InputState = InputState {
    name: c"consume_st",
    enter: Some(input_enter_rename),
    exit: None,
    transitions: &INPUT_STATE_CONSUME_ST_TABLE,
};

static INPUT_STATE_GROUND_TABLE: [InputTransition; 10] = [
    InputTransition { first: 0x18, last: 0x18, handler: Some(input_c0_dispatch), state: Some(&INPUT_STATE_GROUND) },
    InputTransition { first: 0x1a, last: 0x1a, handler: Some(input_c0_dispatch), state: Some(&INPUT_STATE_GROUND) },
    InputTransition { first: 0x1b, last: 0x1b, handler: None, state: Some(&INPUT_STATE_ESC_ENTER) },
    InputTransition { first: 0, last: 0x17, handler: Some(input_c0_dispatch), state: None },
    InputTransition { first: 0x19, last: 0x19, handler: Some(input_c0_dispatch), state: None },
    InputTransition { first: 0x1c, last: 0x1f, handler: Some(input_c0_dispatch), state: None },
    InputTransition { first: 0x20, last: 0x7e, handler: Some(input_print), state: None },
    InputTransition { first: 0x7f, last: 0x7f, handler: None, state: None },
    InputTransition { first: 0x80, last: 0xff, handler: Some(input_top_bit_set), state: None },
    InputTransition { first: -1, last: -1, handler: None, state: None },
];

static INPUT_STATE_ESC_ENTER_TABLE: [InputTransition; 23] = [
    InputTransition { first: 0x18, last: 0x18, handler: Some(input_c0_dispatch), state: Some(&INPUT_STATE_GROUND) },
    InputTransition { first: 0x1a, last: 0x1a, handler: Some(input_c0_dispatch), state: Some(&INPUT_STATE_GROUND) },
    InputTransition { first: 0x1b, last: 0x1b, handler: None, state: Some(&INPUT_STATE_ESC_ENTER) },
    InputTransition { first: 0, last: 0x17, handler: Some(input_c0_dispatch), state: None },
    InputTransition { first: 0x19, last: 0x19, handler: Some(input_c0_dispatch), state: None },
    InputTransition { first: 0x1c, last: 0x1f, handler: Some(input_c0_dispatch), state: None },
    InputTransition { first: 0x20, last: 0x2f, handler: Some(input_intermediate), state: Some(&INPUT_STATE_ESC_INTERMEDIATE) },
    InputTransition { first: 0x30, last: 0x4f, handler: Some(input_esc_dispatch), state: Some(&INPUT_STATE_GROUND) },
    InputTransition { first: 0x50, last: 0x50, handler: None, state: Some(&INPUT_STATE_DCS_ENTER) },
    InputTransition { first: 0x51, last: 0x57, handler: Some(input_esc_dispatch), state: Some(&INPUT_STATE_GROUND) },
    InputTransition { first: 0x58, last: 0x58, handler: None, state: Some(&INPUT_STATE_CONSUME_ST) },
    InputTransition { first: 0x59, last: 0x59, handler: Some(input_esc_dispatch), state: Some(&INPUT_STATE_GROUND) },
    InputTransition { first: 0x5a, last: 0x5a, handler: Some(input_esc_dispatch), state: Some(&INPUT_STATE_GROUND) },
    InputTransition { first: 0x5b, last: 0x5b, handler: None, state: Some(&INPUT_STATE_CSI_ENTER) },
    InputTransition { first: 0x5c, last: 0x5c, handler: Some(input_esc_dispatch), state: Some(&INPUT_STATE_GROUND) },
    InputTransition { first: 0x5d, last: 0x5d, handler: None, state: Some(&INPUT_STATE_OSC_STRING) },
    InputTransition { first: 0x5e, last: 0x5e, handler: None, state: Some(&INPUT_STATE_CONSUME_ST) },
    InputTransition { first: 0x5f, last: 0x5f, handler: None, state: Some(&INPUT_STATE_APC_STRING) },
    InputTransition { first: 0x60, last: 0x6a, handler: Some(input_esc_dispatch), state: Some(&INPUT_STATE_GROUND) },
    InputTransition { first: 0x6b, last: 0x6b, handler: None, state: Some(&INPUT_STATE_RENAME_STRING) },
    InputTransition { first: 0x6c, last: 0x7e, handler: Some(input_esc_dispatch), state: Some(&INPUT_STATE_GROUND) },
    InputTransition { first: 0x7f, last: 0xff, handler: None, state: None },
    InputTransition { first: -1, last: -1, handler: None, state: None },
];

static INPUT_STATE_ESC_INTERMEDIATE_TABLE: [InputTransition; 10] = [
    InputTransition { first: 0x18, last: 0x18, handler: Some(input_c0_dispatch), state: Some(&INPUT_STATE_GROUND) },
    InputTransition { first: 0x1a, last: 0x1a, handler: Some(input_c0_dispatch), state: Some(&INPUT_STATE_GROUND) },
    InputTransition { first: 0x1b, last: 0x1b, handler: None, state: Some(&INPUT_STATE_ESC_ENTER) },
    InputTransition { first: 0, last: 0x17, handler: Some(input_c0_dispatch), state: None },
    InputTransition { first: 0x19, last: 0x19, handler: Some(input_c0_dispatch), state: None },
    InputTransition { first: 0x1c, last: 0x1f, handler: Some(input_c0_dispatch), state: None },
    InputTransition { first: 0x20, last: 0x2f, handler: Some(input_intermediate), state: None },
    InputTransition { first: 0x30, last: 0x7e, handler: Some(input_esc_dispatch), state: Some(&INPUT_STATE_GROUND) },
    InputTransition { first: 0x7f, last: 0xff, handler: None, state: None },
    InputTransition { first: -1, last: -1, handler: None, state: None },
];

static INPUT_STATE_CSI_ENTER_TABLE: [InputTransition; 14] = [
    InputTransition { first: 0x18, last: 0x18, handler: Some(input_c0_dispatch), state: Some(&INPUT_STATE_GROUND) },
    InputTransition { first: 0x1a, last: 0x1a, handler: Some(input_c0_dispatch), state: Some(&INPUT_STATE_GROUND) },
    InputTransition { first: 0x1b, last: 0x1b, handler: None, state: Some(&INPUT_STATE_ESC_ENTER) },
    InputTransition { first: 0, last: 0x17, handler: Some(input_c0_dispatch), state: None },
    InputTransition { first: 0x19, last: 0x19, handler: Some(input_c0_dispatch), state: None },
    InputTransition { first: 0x1c, last: 0x1f, handler: Some(input_c0_dispatch), state: None },
    InputTransition { first: 0x20, last: 0x2f, handler: Some(input_intermediate), state: Some(&INPUT_STATE_CSI_INTERMEDIATE) },
    InputTransition { first: 0x30, last: 0x39, handler: Some(input_parameter), state: Some(&INPUT_STATE_CSI_PARAMETER) },
    InputTransition { first: 0x3a, last: 0x3a, handler: Some(input_parameter), state: Some(&INPUT_STATE_CSI_PARAMETER) },
    InputTransition { first: 0x3b, last: 0x3b, handler: Some(input_parameter), state: Some(&INPUT_STATE_CSI_PARAMETER) },
    InputTransition { first: 0x3c, last: 0x3f, handler: Some(input_intermediate), state: Some(&INPUT_STATE_CSI_PARAMETER) },
    InputTransition { first: 0x40, last: 0x7e, handler: Some(input_csi_dispatch), state: Some(&INPUT_STATE_GROUND) },
    InputTransition { first: 0x7f, last: 0xff, handler: None, state: None },
    InputTransition { first: -1, last: -1, handler: None, state: None },
];

static INPUT_STATE_CSI_PARAMETER_TABLE: [InputTransition; 14] = [
    InputTransition { first: 0x18, last: 0x18, handler: Some(input_c0_dispatch), state: Some(&INPUT_STATE_GROUND) },
    InputTransition { first: 0x1a, last: 0x1a, handler: Some(input_c0_dispatch), state: Some(&INPUT_STATE_GROUND) },
    InputTransition { first: 0x1b, last: 0x1b, handler: None, state: Some(&INPUT_STATE_ESC_ENTER) },
    InputTransition { first: 0, last: 0x17, handler: Some(input_c0_dispatch), state: None },
    InputTransition { first: 0x19, last: 0x19, handler: Some(input_c0_dispatch), state: None },
    InputTransition { first: 0x1c, last: 0x1f, handler: Some(input_c0_dispatch), state: None },
    InputTransition { first: 0x20, last: 0x2f, handler: Some(input_intermediate), state: Some(&INPUT_STATE_CSI_INTERMEDIATE) },
    InputTransition { first: 0x30, last: 0x39, handler: Some(input_parameter), state: None },
    InputTransition { first: 0x3a, last: 0x3a, handler: Some(input_parameter), state: None },
    InputTransition { first: 0x3b, last: 0x3b, handler: Some(input_parameter), state: None },
    InputTransition { first: 0x3c, last: 0x3f, handler: None, state: Some(&INPUT_STATE_CSI_IGNORE) },
    InputTransition { first: 0x40, last: 0x7e, handler: Some(input_csi_dispatch), state: Some(&INPUT_STATE_GROUND) },
    InputTransition { first: 0x7f, last: 0xff, handler: None, state: None },
    InputTransition { first: -1, last: -1, handler: None, state: None },
];

static INPUT_STATE_CSI_INTERMEDIATE_TABLE: [InputTransition; 11] = [
    InputTransition { first: 0x18, last: 0x18, handler: Some(input_c0_dispatch), state: Some(&INPUT_STATE_GROUND) },
    InputTransition { first: 0x1a, last: 0x1a, handler: Some(input_c0_dispatch), state: Some(&INPUT_STATE_GROUND) },
    InputTransition { first: 0x1b, last: 0x1b, handler: None, state: Some(&INPUT_STATE_ESC_ENTER) },
    InputTransition { first: 0, last: 0x17, handler: Some(input_c0_dispatch), state: None },
    InputTransition { first: 0x19, last: 0x19, handler: Some(input_c0_dispatch), state: None },
    InputTransition { first: 0x1c, last: 0x1f, handler: Some(input_c0_dispatch), state: None },
    InputTransition { first: 0x20, last: 0x2f, handler: Some(input_intermediate), state: None },
    InputTransition { first: 0x30, last: 0x3f, handler: None, state: Some(&INPUT_STATE_CSI_IGNORE) },
    InputTransition { first: 0x40, last: 0x7e, handler: Some(input_csi_dispatch), state: Some(&INPUT_STATE_GROUND) },
    InputTransition { first: 0x7f, last: 0xff, handler: None, state: None },
    InputTransition { first: -1, last: -1, handler: None, state: None },
];

static INPUT_STATE_CSI_IGNORE_TABLE: [InputTransition; 10] = [
    InputTransition { first: 0x18, last: 0x18, handler: Some(input_c0_dispatch), state: Some(&INPUT_STATE_GROUND) },
    InputTransition { first: 0x1a, last: 0x1a, handler: Some(input_c0_dispatch), state: Some(&INPUT_STATE_GROUND) },
    InputTransition { first: 0x1b, last: 0x1b, handler: None, state: Some(&INPUT_STATE_ESC_ENTER) },
    InputTransition { first: 0, last: 0x17, handler: Some(input_c0_dispatch), state: None },
    InputTransition { first: 0x19, last: 0x19, handler: Some(input_c0_dispatch), state: None },
    InputTransition { first: 0x1c, last: 0x1f, handler: Some(input_c0_dispatch), state: None },
    InputTransition { first: 0x20, last: 0x3f, handler: None, state: None },
    InputTransition { first: 0x40, last: 0x7e, handler: None, state: Some(&INPUT_STATE_GROUND) },
    InputTransition { first: 0x7f, last: 0xff, handler: None, state: None },
    InputTransition { first: -1, last: -1, handler: None, state: None },
];

static INPUT_STATE_DCS_ENTER_TABLE: [InputTransition; 14] = [
    InputTransition { first: 0x18, last: 0x18, handler: Some(input_c0_dispatch), state: Some(&INPUT_STATE_GROUND) },
    InputTransition { first: 0x1a, last: 0x1a, handler: Some(input_c0_dispatch), state: Some(&INPUT_STATE_GROUND) },
    InputTransition { first: 0x1b, last: 0x1b, handler: None, state: Some(&INPUT_STATE_ESC_ENTER) },
    InputTransition { first: 0, last: 0x17, handler: None, state: None },
    InputTransition { first: 0x19, last: 0x19, handler: None, state: None },
    InputTransition { first: 0x1c, last: 0x1f, handler: None, state: None },
    InputTransition { first: 0x20, last: 0x2f, handler: Some(input_intermediate), state: Some(&INPUT_STATE_DCS_INTERMEDIATE) },
    InputTransition { first: 0x30, last: 0x39, handler: Some(input_parameter), state: Some(&INPUT_STATE_DCS_PARAMETER) },
    InputTransition { first: 0x3a, last: 0x3a, handler: None, state: Some(&INPUT_STATE_DCS_IGNORE) },
    InputTransition { first: 0x3b, last: 0x3b, handler: Some(input_parameter), state: Some(&INPUT_STATE_DCS_PARAMETER) },
    InputTransition { first: 0x3c, last: 0x3f, handler: Some(input_intermediate), state: Some(&INPUT_STATE_DCS_PARAMETER) },
    InputTransition { first: 0x40, last: 0x7e, handler: Some(input_input), state: Some(&INPUT_STATE_DCS_HANDLER) },
    InputTransition { first: 0x7f, last: 0xff, handler: None, state: None },
    InputTransition { first: -1, last: -1, handler: None, state: None },
];

static INPUT_STATE_DCS_PARAMETER_TABLE: [InputTransition; 14] = [
    InputTransition { first: 0x18, last: 0x18, handler: Some(input_c0_dispatch), state: Some(&INPUT_STATE_GROUND) },
    InputTransition { first: 0x1a, last: 0x1a, handler: Some(input_c0_dispatch), state: Some(&INPUT_STATE_GROUND) },
    InputTransition { first: 0x1b, last: 0x1b, handler: None, state: Some(&INPUT_STATE_ESC_ENTER) },
    InputTransition { first: 0, last: 0x17, handler: None, state: None },
    InputTransition { first: 0x19, last: 0x19, handler: None, state: None },
    InputTransition { first: 0x1c, last: 0x1f, handler: None, state: None },
    InputTransition { first: 0x20, last: 0x2f, handler: Some(input_intermediate), state: Some(&INPUT_STATE_DCS_INTERMEDIATE) },
    InputTransition { first: 0x30, last: 0x39, handler: Some(input_parameter), state: None },
    InputTransition { first: 0x3a, last: 0x3a, handler: None, state: Some(&INPUT_STATE_DCS_IGNORE) },
    InputTransition { first: 0x3b, last: 0x3b, handler: Some(input_parameter), state: None },
    InputTransition { first: 0x3c, last: 0x3f, handler: None, state: Some(&INPUT_STATE_DCS_IGNORE) },
    InputTransition { first: 0x40, last: 0x7e, handler: Some(input_input), state: Some(&INPUT_STATE_DCS_HANDLER) },
    InputTransition { first: 0x7f, last: 0xff, handler: None, state: None },
    InputTransition { first: -1, last: -1, handler: None, state: None },
];

static INPUT_STATE_DCS_INTERMEDIATE_TABLE: [InputTransition; 11] = [
    InputTransition { first: 0x18, last: 0x18, handler: Some(input_c0_dispatch), state: Some(&INPUT_STATE_GROUND) },
    InputTransition { first: 0x1a, last: 0x1a, handler: Some(input_c0_dispatch), state: Some(&INPUT_STATE_GROUND) },
    InputTransition { first: 0x1b, last: 0x1b, handler: None, state: Some(&INPUT_STATE_ESC_ENTER) },
    InputTransition { first: 0, last: 0x17, handler: None, state: None },
    InputTransition { first: 0x19, last: 0x19, handler: None, state: None },
    InputTransition { first: 0x1c, last: 0x1f, handler: None, state: None },
    InputTransition { first: 0x20, last: 0x2f, handler: Some(input_intermediate), state: None },
    InputTransition { first: 0x30, last: 0x3f, handler: None, state: Some(&INPUT_STATE_DCS_IGNORE) },
    InputTransition { first: 0x40, last: 0x7e, handler: Some(input_input), state: Some(&INPUT_STATE_DCS_HANDLER) },
    InputTransition { first: 0x7f, last: 0xff, handler: None, state: None },
    InputTransition { first: -1, last: -1, handler: None, state: None },
];

static INPUT_STATE_DCS_HANDLER_TABLE: [InputTransition; 4] = [
    InputTransition { first: 0, last: 0x1a, handler: Some(input_input), state: None },
    InputTransition { first: 0x1b, last: 0x1b, handler: None, state: Some(&INPUT_STATE_DCS_ESCAPE) },
    InputTransition { first: 0x1c, last: 0xff, handler: Some(input_input), state: None },
    InputTransition { first: -1, last: -1, handler: None, state: None },
];

static INPUT_STATE_DCS_ESCAPE_TABLE: [InputTransition; 4] = [
    InputTransition { first: 0, last: 0x5b, handler: Some(input_input), state: Some(&INPUT_STATE_DCS_HANDLER) },
    InputTransition { first: 0x5c, last: 0x5c, handler: Some(input_dcs_dispatch), state: Some(&INPUT_STATE_GROUND) },
    InputTransition { first: 0x5d, last: 0xff, handler: Some(input_input), state: Some(&INPUT_STATE_DCS_HANDLER) },
    InputTransition { first: -1, last: -1, handler: None, state: None },
];

static INPUT_STATE_DCS_IGNORE_TABLE: [InputTransition; 8] = [
    InputTransition { first: 0x18, last: 0x18, handler: Some(input_c0_dispatch), state: Some(&INPUT_STATE_GROUND) },
    InputTransition { first: 0x1a, last: 0x1a, handler: Some(input_c0_dispatch), state: Some(&INPUT_STATE_GROUND) },
    InputTransition { first: 0x1b, last: 0x1b, handler: None, state: Some(&INPUT_STATE_ESC_ENTER) },
    InputTransition { first: 0, last: 0x17, handler: None, state: None },
    InputTransition { first: 0x19, last: 0x19, handler: None, state: None },
    InputTransition { first: 0x1c, last: 0x1f, handler: None, state: None },
    InputTransition { first: 0x20, last: 0xff, handler: None, state: None },
    InputTransition { first: -1, last: -1, handler: None, state: None },
];

static INPUT_STATE_OSC_STRING_TABLE: [InputTransition; 10] = [
    InputTransition { first: 0x18, last: 0x18, handler: Some(input_c0_dispatch), state: Some(&INPUT_STATE_GROUND) },
    InputTransition { first: 0x1a, last: 0x1a, handler: Some(input_c0_dispatch), state: Some(&INPUT_STATE_GROUND) },
    InputTransition { first: 0x1b, last: 0x1b, handler: None, state: Some(&INPUT_STATE_ESC_ENTER) },
    InputTransition { first: 0, last: 0x6, handler: None, state: None },
    InputTransition { first: 0x7, last: 0x7, handler: Some(input_end_bel), state: Some(&INPUT_STATE_GROUND) },
    InputTransition { first: 0x8, last: 0x17, handler: None, state: None },
    InputTransition { first: 0x19, last: 0x19, handler: None, state: None },
    InputTransition { first: 0x1c, last: 0x1f, handler: None, state: None },
    InputTransition { first: 0x20, last: 0xff, handler: Some(input_input), state: None },
    InputTransition { first: -1, last: -1, handler: None, state: None },
];

static INPUT_STATE_APC_STRING_TABLE: [InputTransition; 8] = [
    InputTransition { first: 0x18, last: 0x18, handler: Some(input_c0_dispatch), state: Some(&INPUT_STATE_GROUND) },
    InputTransition { first: 0x1a, last: 0x1a, handler: Some(input_c0_dispatch), state: Some(&INPUT_STATE_GROUND) },
    InputTransition { first: 0x1b, last: 0x1b, handler: None, state: Some(&INPUT_STATE_ESC_ENTER) },
    InputTransition { first: 0, last: 0x17, handler: None, state: None },
    InputTransition { first: 0x19, last: 0x19, handler: None, state: None },
    InputTransition { first: 0x1c, last: 0x1f, handler: None, state: None },
    InputTransition { first: 0x20, last: 0xff, handler: Some(input_input), state: None },
    InputTransition { first: -1, last: -1, handler: None, state: None },
];

static INPUT_STATE_RENAME_STRING_TABLE: [InputTransition; 8] = [
    InputTransition { first: 0x18, last: 0x18, handler: Some(input_c0_dispatch), state: Some(&INPUT_STATE_GROUND) },
    InputTransition { first: 0x1a, last: 0x1a, handler: Some(input_c0_dispatch), state: Some(&INPUT_STATE_GROUND) },
    InputTransition { first: 0x1b, last: 0x1b, handler: None, state: Some(&INPUT_STATE_ESC_ENTER) },
    InputTransition { first: 0, last: 0x17, handler: None, state: None },
    InputTransition { first: 0x19, last: 0x19, handler: None, state: None },
    InputTransition { first: 0x1c, last: 0x1f, handler: None, state: None },
    InputTransition { first: 0x20, last: 0xff, handler: Some(input_input), state: None },
    InputTransition { first: -1, last: -1, handler: None, state: None },
];

static INPUT_STATE_CONSUME_ST_TABLE: [InputTransition; 8] = [
    InputTransition { first: 0x18, last: 0x18, handler: Some(input_c0_dispatch), state: Some(&INPUT_STATE_GROUND) },
    InputTransition { first: 0x1a, last: 0x1a, handler: Some(input_c0_dispatch), state: Some(&INPUT_STATE_GROUND) },
    InputTransition { first: 0x1b, last: 0x1b, handler: None, state: Some(&INPUT_STATE_ESC_ENTER) },
    InputTransition { first: 0, last: 0x17, handler: None, state: None },
    InputTransition { first: 0x19, last: 0x19, handler: None, state: None },
    InputTransition { first: 0x1c, last: 0x1f, handler: None, state: None },
    InputTransition { first: 0x20, last: 0xff, handler: None, state: None },
    InputTransition { first: -1, last: -1, handler: None, state: None },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct DummyHost {
        sgr_ops: Vec<SgrOp>,
    }

    impl VtHost for DummyHost {
        fn collect_end(&mut self) {}
        fn print_ascii_run(&mut self, _bytes: &[u8]) -> usize { 0 }
        fn print_ascii(&mut self, _byte: u8) {}
        fn print_utf8_byte(&mut self, _byte: u8) -> bool { false }
        fn stop_utf8(&mut self) {}
        fn repeat_last(&mut self, _count: u32) {}
        fn start_ground_timer(&mut self) {}
        fn clear_ground_timer(&mut self) {}
        fn reset_cell(&mut self) {}
        fn save_state(&mut self) {}
        fn restore_state(&mut self) {}
        fn select_charset(&mut self, _set: i32) {}
        fn define_charset(&mut self, _bank: u8, _acs: bool) {}
        fn apply_sgr(&mut self, op: SgrOp) {
            self.sgr_ops.push(op);
        }
        fn bell(&mut self) {}
        fn backspace(&mut self) {}
        fn horizontal_tab(&mut self) {}
        fn linefeed(&mut self, _cr_on_crlf: bool) {}
        fn carriage_return(&mut self) {}
        fn reverse_index(&mut self) {}
        fn alignment_test(&mut self) {}
        fn hard_reset(&mut self) {}
        fn set_tab_at_cursor(&mut self, _enabled: bool) {}
        fn clear_all_tabs(&mut self) {}
        fn cursor_back_tab(&mut self, _count: u32) {}
        fn cursor_up(&mut self, _count: u32) {}
        fn cursor_down(&mut self, _count: u32) {}
        fn cursor_right(&mut self, _count: u32) {}
        fn cursor_left(&mut self, _count: u32) {}
        fn cursor_move(&mut self, _x: i32, _y: i32) {}
        fn erase_chars(&mut self, _count: u32) {}
        fn delete_chars(&mut self, _count: u32) {}
        fn insert_chars(&mut self, _count: u32) {}
        fn delete_lines(&mut self, _count: u32) {}
        fn insert_lines(&mut self, _count: u32) {}
        fn scroll_up(&mut self, _count: u32) {}
        fn scroll_down(&mut self, _count: u32) {}
        fn screen_height(&self) -> u32 { 24 }
        fn set_scroll_region(&mut self, _upper: u32, _lower: u32) {}
        fn erase_in_display(&mut self, _mode: i32, _submode: i32) {}
        fn erase_in_line(&mut self, _mode: i32) {}
        fn set_cursor_style(&mut self, _style: u32) {}
        fn set_mode(&mut self, _mode: i32, _private: bool, _enabled: bool) {}
        fn set_keypad_mode(&mut self, _enabled: bool) {}
        fn set_extended_keys_mode(&mut self, _param: Option<i32>) {}
        fn reply(&mut self, _queue: bool, _bytes: &[u8]) {}
        fn query_cursor_position(&mut self) {}
        fn query_mode(&mut self, _mode: i32, _private: bool) {}
        fn query_theme(&mut self) {}
        fn query_xda(&mut self) {}
        fn query_decrqss_cursor_style(&mut self) {}
        fn winop(&mut self, _op: WinOp) {}
        fn dcs_tmux_passthrough(&mut self, _payload: &[u8]) {}
        fn set_title(&mut self, _title: &CStr) {}
        fn set_path(&mut self, _path: &CStr) {}
        fn set_hyperlink(&mut self, _uri: &CStr, _id: Option<&CStr>) {}
        fn set_progress_bar(&mut self, _state: u32, _progress: i32) {}
        fn osc_4_query(&mut self, _idx: i32, _end_type: InputEndType) {}
        fn osc_4_set(&mut self, _idx: i32, _spec: &CStr) -> bool { false }
        fn osc_104_reset_all(&mut self) {}
        fn osc_104_reset_one(&mut self, _idx: i32) -> bool { false }
        fn full_redraw(&mut self) {}
        fn osc_10(&mut self, _spec: Option<&CStr>, _end_type: InputEndType) {}
        fn osc_110(&mut self) {}
        fn osc_11(&mut self, _spec: Option<&CStr>, _end_type: InputEndType) {}
        fn osc_111(&mut self) {}
        fn osc_12(&mut self, _spec: Option<&CStr>, _end_type: InputEndType) {}
        fn osc_112(&mut self) {}
        fn allow_set_clipboard(&self) -> bool { false }
        fn osc_52_query(&mut self, _clip: c_char, _end_type: InputEndType) {}
        fn osc_52_set(&mut self, _clip: &CStr, _data: Vec<u8>) {}
        fn osc_133(&mut self, _cmd: Osc133Command) {}
        fn set_program_status(&mut self, _status: Option<ProgramStatus>) {}
        fn exit_rename(&mut self, _input_buf: &[u8], _input_len: usize) {}
    }

    #[test]
    fn colon_parameter_survives_later_numeric_error_until_next_split() {
        let mut parser = Parser::new();
        let invalid = b"38:5:196;invalid\0";
        parser.param_buf[..invalid.len()].copy_from_slice(invalid);
        parser.param_len = invalid.len() - 1;
        assert_eq!(input_split(&mut parser), -1);
        assert_eq!(parser.param_list_len, 1);
        assert_eq!(
            match &parser.param_list[0] {
                InputParam::String(value) => value.as_c_str(),
                _ => panic!("expected string parameter"),
            },
            c"38:5:196"
        );

        let next = b"48:5:25\0";
        parser.param_buf[..next.len()].copy_from_slice(next);
        parser.param_len = next.len() - 1;
        assert_eq!(input_split(&mut parser), 0);
        assert_eq!(parser.param_list_len, 1);
        assert_eq!(
            match &parser.param_list[0] {
                InputParam::String(value) => value.as_c_str(),
                _ => panic!("expected string parameter"),
            },
            c"48:5:25"
        );
        parser.clear_params();
    }

    #[test]
    fn parser_buffer_grows_preserves_bytes_and_shrinks() {
        let mut parser = Parser::new();
        let mut host = DummyHost::default();
        assert_eq!(parser.input_buf.len(), INPUT_BUF_START);
        for ch in (0..96).map(|i| if i == 17 { 0 } else { b'a' + (i % 26) }) {
            parser.ch = ch as i32;
            input_input(&mut parser, &mut host);
        }
        assert_eq!(parser.input_len, 96);
        assert_eq!(parser.input_buf[17], 0);
        assert_eq!(parser.input_buf[96], 0);
        assert_eq!(parser.input_buf.len(), 128);

        parser.shrink_buffer();
        assert_eq!(parser.input_buf.len(), INPUT_BUF_START);
    }

    #[test]
    fn sgr_colon_parses_rgb_indexed_and_underline_parameters() {
        fn parse_colon(source: &[u8]) -> Vec<SgrOp> {
            let mut parser = Parser::new();
            let mut host = DummyHost::default();
            let parameter = CStr::from_bytes_until_nul(source).unwrap();
            parser.param_list[0] = InputParam::String(parameter.to_owned());
            dispatch_sgr_colon(&parser, &mut host, 0);
            assert_eq!(
                match &parser.param_list[0] {
                    InputParam::String(value) => value.as_c_str(),
                    _ => panic!("expected string parameter"),
                },
                parameter
            );
            host.sgr_ops
        }

        assert_eq!(parse_colon(b"38:2:1:2:3\0"), vec![SgrOp::Fg(SgrColor::Rgb(1, 2, 3))]);
        assert_eq!(parse_colon(b"38:2::1:2:3\0"), vec![SgrOp::Fg(SgrColor::Rgb(1, 2, 3))]);
        assert_eq!(parse_colon(b"48:5:196\0"), vec![SgrOp::Bg(SgrColor::Indexed(196))]);
        assert_eq!(parse_colon(b"58:2:5:6:7\0"), vec![SgrOp::Us(SgrColor::Rgb(5, 6, 7))]);
        assert_eq!(parse_colon(b"4:3\0"), vec![SgrOp::SetAttr(SgrAttr::Underscore3)]);

        for source in [
            b"38:2:999999999999999:2:3\0".as_slice(),
            b"38:2:abc:2:3\0",
            b"38:2:1:2:3:4:5:6\0",
            b":::::::\0",
            b"38:2:300:2:3\0",
            b"38:2:1:2\0",
        ] {
            assert!(parse_colon(source).is_empty(), "{source:?}");
        }
        assert_eq!(parse_colon(b"38:5:196\0:2:1:2:3\0"), vec![SgrOp::Fg(SgrColor::Indexed(196))]);
    }

    #[test]
    fn osc_133_exit_status_preserves_numeric_and_parameter_syntax() {
        for (input, expected) in [
            (b"D\0".as_slice(), 0),
            (b"D;\0", 0),
            (b"D;;42\0", 0),
            (b"D;=42\0", 0),
            (b"D;42=ignored\0", 0),
            (b"D;42;k=v\0", 42),
            (b"D;0\0", 0),
            (b"D;255\0", 255),
            (b"D;+7\0", 7),
            (b"D; 7\0", 7),
            (b"D;256\0", 255),
            (b"D;-1\0", 255),
            (b"D;abc\0", 255),
            (b"D;4x\0", 255),
            (b"D;12\0;99\0", 12),
            (b"D;\xff\0", 255),
        ] {
            let input = CStr::from_bytes_until_nul(input).unwrap();
            let actual = osc_133_exit_status(input);
            assert_eq!(actual, expected, "input {input:?}");
        }
    }
}
