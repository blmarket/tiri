use crate::base64;

const OSC_7501_MAX_BODY_LEN: usize = 4087;
const OSC_7501_MAX_KEY_LEN: usize = 16;
const OSC_7501_MAX_NAME_LEN: usize = 32;
const OSC_7501_MAX_ID_LEN: usize = 128;
const OSC_7501_MAX_ID_SEGMENTS: usize = 8;
const OSC_7501_MAX_TITLE_ENCODED_LEN: usize = 256;
const OSC_7501_MAX_TITLE_DECODED_LEN: usize = 192;
const OSC_7501_MAX_MSG_ENCODED_LEN: usize = 2732;
const OSC_7501_MAX_MSG_DECODED_LEN: usize = 2048;

pub(crate) const OSC_7501_QUERY_REPLY_PREFIX: &[u8] =
    b"\x1B]7501;?:states=idle,working,done,blocked,error:kinds=permission,question,auth";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProgramState {
    Idle,
    Working,
    Done,
    Blocked,
    Error,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProgramStatus {
    state: ProgramState,
    app: Option<String>,
}

impl ProgramStatus {
    pub fn new(state: ProgramState, app: Option<String>) -> Self {
        Self { state, app }
    }

    pub fn state(&self) -> ProgramState {
        self.state
    }

    pub fn app(&self) -> Option<&str> {
        self.app.as_deref()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Osc7501Update {
    Clear,
    Status(ProgramStatus),
    Ignore,
}

fn osc_7501_is_name_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'+' | b'-')
}

fn osc_7501_is_value_byte(b: u8) -> bool {
    osc_7501_is_name_byte(b) || matches!(b, b',' | b'/' | b'=')
}

fn osc_7501_is_name(bytes: &[u8]) -> bool {
    !bytes.is_empty()
        && bytes.len() <= OSC_7501_MAX_NAME_LEN
        && bytes.iter().copied().all(osc_7501_is_name_byte)
}

fn osc_7501_valid_id(bytes: &[u8]) -> bool {
    if bytes.is_empty() || bytes.len() > OSC_7501_MAX_ID_LEN {
        return false;
    }
    let mut count = 0;
    for segment in bytes.split(|&b| b == b'/') {
        count += 1;
        if count > OSC_7501_MAX_ID_SEGMENTS || !osc_7501_is_name(segment) {
            return false;
        }
    }
    true
}

fn osc_7501_valid_base64_text(encoded: &[u8], max_decoded_len: usize) -> bool {
    if encoded.len() > max_decoded_len.div_ceil(3) * 4 {
        return false;
    }
    let decoded = if encoded.contains(&b'=') {
        base64::decode(encoded)
    } else {
        if encoded.len() % 4 == 1 {
            return false;
        }
        let mut padded = Vec::with_capacity(encoded.len() + 2);
        padded.extend_from_slice(encoded);
        padded.resize(encoded.len().next_multiple_of(4), b'=');
        base64::decode(&padded)
    };
    let Some(decoded) = decoded else {
        return false;
    };
    if decoded.len() > max_decoded_len {
        return false;
    }
    let Ok(text) = std::str::from_utf8(&decoded) else {
        return false;
    };
    !text.chars().any(char::is_control)
}

fn osc_7501_last_value<'a>(bytes: &'a [u8], target: &[u8]) -> Option<&'a [u8]> {
    let mut result = None;
    for pair in bytes.split(|&b| b == b':') {
        let Some(eq) = pair.iter().position(|&b| b == b'=') else {
            continue;
        };
        let key = pair[..eq].trim_ascii();
        let val = pair[eq + 1..].trim_ascii();
        if !val.iter().copied().all(osc_7501_is_value_byte) {
            continue;
        }
        if key == target {
            result = Some(val);
        }
    }
    result
}

pub(crate) fn parse_osc_7501(bytes: &[u8]) -> Option<Osc7501Update> {
    if bytes.is_empty() || bytes.len() > OSC_7501_MAX_BODY_LEN {
        return None;
    }
    for pair in bytes.split(|&b| b == b':') {
        let Some(eq) = pair.iter().position(|&b| b == b'=') else {
            continue;
        };
        let key = pair[..eq].trim_ascii();
        let val = pair[eq + 1..].trim_ascii();
        if key.len() > OSC_7501_MAX_KEY_LEN {
            return None;
        }
        match key {
            b"id" if !osc_7501_valid_id(val) => return None,
            b"app" if val.len() > OSC_7501_MAX_NAME_LEN => return None,
            b"title"
                if val.len() > OSC_7501_MAX_TITLE_ENCODED_LEN
                    || (!val.is_empty()
                        && !osc_7501_valid_base64_text(val, OSC_7501_MAX_TITLE_DECODED_LEN)) =>
            {
                return None;
            }
            b"msg"
                if val.len() > OSC_7501_MAX_MSG_ENCODED_LEN
                    || (!val.is_empty()
                        && !osc_7501_valid_base64_text(val, OSC_7501_MAX_MSG_DECODED_LEN)) =>
            {
                return None;
            }
            _ => {}
        }
    }
    let state_bytes = osc_7501_last_value(bytes, b"state")?;
    let id = osc_7501_last_value(bytes, b"id").filter(|v| !v.is_empty());
    if state_bytes == b"clear" {
        return Some(if id.is_none() {
            Osc7501Update::Clear
        } else {
            Osc7501Update::Ignore
        });
    }
    let state = match state_bytes {
        b"idle" => ProgramState::Idle,
        b"working" => ProgramState::Working,
        b"done" => ProgramState::Done,
        b"blocked" => ProgramState::Blocked,
        b"error" => ProgramState::Error,
        _ => return None,
    };
    if id.is_some() {
        return Some(Osc7501Update::Ignore);
    }
    let app = osc_7501_last_value(bytes, b"app")
        .filter(|v| osc_7501_is_name(v))
        .and_then(|v| std::str::from_utf8(v).ok())
        .map(str::to_owned);
    Some(Osc7501Update::Status(ProgramStatus::new(state, app)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_osc_7501_states_and_clear() {
        assert_eq!(
            parse_osc_7501(b"state=working:app=antigravity"),
            Some(Osc7501Update::Status(ProgramStatus::new(
                ProgramState::Working,
                Some("antigravity".into()),
            )))
        );
        assert_eq!(
            parse_osc_7501(b"state=blocked:kind=permission:app=jetski:title=aGk:msg=dGVzdA=="),
            Some(Osc7501Update::Status(ProgramStatus::new(
                ProgramState::Blocked,
                Some("jetski".into()),
            )))
        );
        assert_eq!(
            parse_osc_7501(b"state=done:app=bad/app"),
            Some(Osc7501Update::Status(ProgramStatus::new(
                ProgramState::Done,
                None,
            )))
        );
        assert_eq!(parse_osc_7501(b"state=clear"), Some(Osc7501Update::Clear));
        assert_eq!(
            parse_osc_7501(b"state=working:id=sub/1:app=claude-code"),
            Some(Osc7501Update::Ignore)
        );
        assert_eq!(
            parse_osc_7501(b"state=clear:id=sub/1"),
            Some(Osc7501Update::Ignore)
        );
    }

    #[test]
    fn rejects_invalid_osc_7501_payloads() {
        assert_eq!(parse_osc_7501(b"state=unknown"), None);
        assert_eq!(parse_osc_7501(b"state=working:title=!!!"), None);
        assert_eq!(parse_osc_7501(b"state=working:id=a//b"), None);
        assert_eq!(parse_osc_7501(b"state=working:app=way_too_long_application_identifier_value"), None);
    }
}
