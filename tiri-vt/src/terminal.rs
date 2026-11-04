use std::ffi::CStr;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalReplyKind {
    PrimaryAttributes,
    SecondaryAttributes,
    ExtendedAttributes,
    SynchronizedOutput,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct TerminalReplyState {
    pub primary_attributes: bool,
    pub secondary_attributes: bool,
    pub extended_attributes: bool,
    pub synchronized_output: bool,
    pub supports_synchronized_output: bool,
}

impl TerminalReplyState {
    fn received(self, kind: TerminalReplyKind) -> bool {
        match kind {
            TerminalReplyKind::PrimaryAttributes => self.primary_attributes,
            TerminalReplyKind::SecondaryAttributes => self.secondary_attributes,
            TerminalReplyKind::ExtendedAttributes => self.extended_attributes,
            TerminalReplyKind::SynchronizedOutput => self.synchronized_output,
        }
    }
}

#[derive(Debug)]
pub struct TerminalReport<'a> {
    pub kind: TerminalReplyKind,
    pub features: Vec<&'static CStr>,
    pub terminal: Option<&'static CStr>,
    pub version: Option<&'a CStr>,
    pub xtermjs: Option<bool>,
}

pub trait TerminalHost {
    fn terminal_reply_state(&self) -> TerminalReplyState;
    fn terminal_report(&mut self, report: TerminalReport<'_>);
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReplyError {
    Incomplete,
    Unrecognized,
}

pub fn parse_terminal_reply(
    host: &mut dyn TerminalHost,
    kind: TerminalReplyKind,
    bytes: &[u8],
) -> Result<usize, ReplyError> {
    let state = host.terminal_reply_state();
    if state.received(kind) {
        return Err(ReplyError::Unrecognized);
    }
    let mut report = TerminalReport {
        kind,
        features: Vec::new(),
        terminal: None,
        version: None,
        xtermjs: None,
    };
    let size = match kind {
        TerminalReplyKind::PrimaryAttributes | TerminalReplyKind::SecondaryAttributes => {
            let marker = if kind == TerminalReplyKind::PrimaryAttributes {
                b'?'
            } else {
                b'>'
            };
            let params = da_params(bytes, marker)?;
            let fields: Vec<i8> = params
                .split(|&b| b == 0)
                .next()
                .unwrap_or_default()
                .split(|&b| b == b';')
                .take(32)
                .map(da_field)
                .collect();
            if kind == TerminalReplyKind::PrimaryAttributes {
                if let [61..=65, features @ ..] = fields.as_slice() {
                    for feature in features {
                        report.features.push(match feature {
                            4 => c"sixel",
                            21 => c"margins",
                            28 => c"rectfill",
                            52 => c"clipboard",
                            _ => continue,
                        });
                    }
                }
            } else {
                report.terminal = match fields.first() {
                    Some(77) => Some(c"mintty"),
                    Some(84) => Some(c"tmux"),
                    Some(85) => Some(c"rxvt-unicode"),
                    _ => None,
                };
                if params == b"0;276;0"
                    && !state.extended_attributes
                    && !state.supports_synchronized_output
                {
                    report.xtermjs = Some(true);
                }
            }
            params.len() + 4
        }
        TerminalReplyKind::SynchronizedOutput => {
            match_prefix(bytes, b"\x1b[?2026;")?;
            let status = *bytes.get(8).ok_or(ReplyError::Incomplete)?;
            if !(b'0'..=b'4').contains(&status) {
                return Err(ReplyError::Unrecognized);
            }
            match_prefix(&bytes[9..], b"$y")?;
            if matches!(status, b'1'..=b'3') {
                report.features.push(c"sync");
                report.xtermjs = Some(false);
            }
            11
        }
        TerminalReplyKind::ExtendedAttributes => {
            match_prefix(bytes, b"\x1bP>|")?;
            let mut end = 0;
            loop {
                if end == 127 {
                    return Err(ReplyError::Unrecognized);
                }
                let byte = *bytes.get(4 + end).ok_or(ReplyError::Incomplete)?;
                if end > 0 && bytes[3 + end] == 0x1b && byte == b'\\' {
                    break;
                }
                end += 1;
            }
            let text = &bytes[4..4 + end - 1];
            let text = text.split(|&b| b == 0).next().unwrap_or_default();
            let version = std::ffi::CString::new(text).unwrap();
            report.version = Some(&version);
            report.xtermjs = Some(false);
            report.terminal = [
                (b"iTerm2 ".as_slice(), c"iTerm2"),
                (b"tmux ", c"tmux"),
                (b"XTerm(", c"XTerm"),
                (b"mintty ", c"mintty"),
                (b"foot(", c"foot"),
                (b"WezTerm", c"WezTerm"),
                (b"ghostty ", c"ghostty"),
                (b"Rio ", c"Rio"),
            ]
            .into_iter()
            .find(|(prefix, _)| text.starts_with(prefix))
            .map(|(_, name)| name);
            host.terminal_report(report);
            return Ok(5 + end);
        }
    };
    host.terminal_report(report);
    Ok(size)
}

fn match_prefix(bytes: &[u8], prefix: &[u8]) -> Result<(), ReplyError> {
    for (index, expected) in prefix.iter().enumerate() {
        match bytes.get(index) {
            None => return Err(ReplyError::Incomplete),
            Some(byte) if byte != expected => return Err(ReplyError::Unrecognized),
            _ => {}
        }
    }
    Ok(())
}

fn da_field(field: &[u8]) -> i8 {
    let mut index = 0;
    while field
        .get(index)
        .is_some_and(|b| matches!(b, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r'))
    {
        index += 1;
    }
    let negative = field.get(index) == Some(&b'-');
    if matches!(field.get(index), Some(b'+' | b'-')) {
        index += 1;
    }
    let start = index;
    let mut value = Some(0u64);
    while let Some(byte) = field.get(index).filter(|b| b.is_ascii_digit()) {
        value = value
            .and_then(|v| v.checked_mul(10))
            .and_then(|v| v.checked_add((byte - b'0') as u64));
        index += 1;
    }
    if index != field.len() || (index == start && !field.is_empty()) {
        return 0;
    }
    match value {
        None => u64::MAX as i8,
        Some(value) if negative => value.wrapping_neg() as i8,
        Some(value) => value as i8,
    }
}

fn da_params(bytes: &[u8], marker: u8) -> Result<&[u8], ReplyError> {
    match_prefix(bytes, &[0x1b, b'[', marker])?;
    for index in 0..128 {
        let byte = *bytes.get(3 + index).ok_or(ReplyError::Incomplete)?;
        if byte.is_ascii_lowercase() {
            return if byte == b'c' {
                Ok(&bytes[3..3 + index])
            } else {
                Err(ReplyError::Unrecognized)
            };
        }
    }
    Err(ReplyError::Unrecognized)
}
