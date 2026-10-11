#![forbid(unsafe_code)]

use crate::src::reactor::{evbuffer_add_formatted, evbuffer_new, evbuffer_pullup};
use crate::src::shared::abi::int64_t;
use crate::src::shared::json::{json_node, json_node_type, JsonValue};
use hmux_buffer::BufMut;
use hmux_buffer::SegmentedBuf;
use std::ffi::{CStr, CString};

pub const NODE_ARRAY: json_node_type = 4;
pub const NODE_OBJECT: json_node_type = 3;
pub const NODE_BOOLEAN: json_node_type = 2;
pub const NODE_NUMBER: json_node_type = 1;
pub const NODE_STRING: json_node_type = 0;
pub const ERROR_CTX_LEN: usize = 8;
pub const PARSE_DEPTH_MAX: usize = 200;

pub struct json_parse_ctx<'input, 'cause> {
    pub input: &'input CStr,
    pub cause: Option<&'cause mut Option<CString>>,
    pub depth: usize,
}

impl<'input> json_parse_ctx<'input, '_> {
    fn text(&self, token: &json_token) -> &'input [u8] {
        &self.input.to_bytes()[token.offset..token.offset + token.len]
    }

    fn error(&mut self, reason: &CStr, token: &json_token) {
        json_error(
            self.cause.as_deref_mut(),
            reason,
            &self.input.to_bytes()[token.offset..],
        );
    }
}

#[derive(Copy, Clone)]
pub struct json_token {
    pub type_0: json_token_type,
    pub offset: usize,
    pub len: usize,
}
pub type json_token_type = u32;
pub const TOK_EOF: json_token_type = 8;
pub const TOK_VALUE: json_token_type = 7;
pub const TOK_QUOTE: json_token_type = 6;
pub const TOK_COLON: json_token_type = 5;
pub const TOK_COMMA: json_token_type = 4;
pub const TOK_CLOSEARRAY: json_token_type = 3;
pub const TOK_OPENARRAY: json_token_type = 2;
pub const TOK_CLOSEOBJECT: json_token_type = 1;
pub const TOK_OPENOBJECT: json_token_type = 0;

/// Parse tmux's JSON subset. The returned tree owns its strings and children;
/// neither the input nor the optional diagnostic output is retained.
pub fn json_parse(input: &CStr, mut cause: Option<&mut Option<CString>>) -> Option<Box<json_node>> {
    if input.to_bytes().is_empty() {
        json_error(cause, c"empty input", &[]);
        return None;
    }
    let tokens = json_tokenize_input(input, cause.as_deref_mut())?;
    let mut pctx = json_parse_ctx {
        input,
        cause,
        depth: 0,
    };
    json_parse_tokens(&tokens, &mut pctx)
}

pub fn json_find<'a>(node: &'a json_node, key: &CStr) -> Option<&'a json_node> {
    let JsonValue::Object(fields) = &node.value else {
        return None;
    };
    fields.entries.get(key.to_bytes()).map(Box::as_ref)
}

pub fn json_array_members(node: &json_node) -> Option<&[json_node]> {
    let JsonValue::Array(members) = &node.value else {
        return None;
    };
    Some(&members.members)
}

pub fn json_get_number(node: &json_node) -> Option<int64_t> {
    let JsonValue::Number(number) = node.value else {
        return None;
    };
    Some(number)
}

pub fn json_get_object(node: &json_node) -> Option<&json_node> {
    matches!(node.value, JsonValue::Object(_)).then_some(node)
}

fn json_key_error(key: &CStr, message: &[u8]) -> CString {
    let mut bytes = b"key \"".to_vec();
    bytes.extend_from_slice(key.to_bytes());
    bytes.extend_from_slice(b"\" ");
    bytes.extend_from_slice(message);
    CString::new(bytes).expect("JSON diagnostic contains no NUL")
}

fn json_require<'a>(node: &'a json_node, key: &CStr) -> Result<&'a json_node, CString> {
    json_find(node, key).ok_or_else(|| json_key_error(key, b"not found"))
}

pub fn json_find_string<'a>(node: &'a json_node, key: &CStr) -> Result<&'a CStr, CString> {
    let JsonValue::String(string) = &json_require(node, key)?.value else {
        return Err(json_key_error(key, b"expected a string"));
    };
    Ok(string)
}

pub fn json_find_number(node: &json_node, key: &CStr) -> Result<int64_t, CString> {
    json_get_number(json_require(node, key)?)
        .ok_or_else(|| json_key_error(key, b"expected a number"))
}

pub fn json_find_boolean(node: &json_node, key: &CStr) -> Result<i32, CString> {
    let JsonValue::Boolean(boolean) = json_require(node, key)?.value else {
        return Err(json_key_error(key, b"expected a boolean"));
    };
    Ok(boolean)
}

pub fn json_find_object<'a>(node: &'a json_node, key: &CStr) -> Result<&'a json_node, CString> {
    json_get_object(json_require(node, key)?)
        .ok_or_else(|| json_key_error(key, b"expected an object"))
}

pub fn json_find_array<'a>(
    node: &'a json_node,
    key: &CStr,
) -> Result<&'a [json_node], CString> {
    json_array_members(json_require(node, key)?)
        .ok_or_else(|| json_key_error(key, b"expected an array"))
}

fn json_error(cause: Option<&mut Option<CString>>, reason: &CStr, loc: &[u8]) {
    let Some(cause) = cause else { return };
    if loc.is_empty() {
        *cause = Some(reason.to_owned());
        return;
    }
    let mut message = reason.to_bytes().to_vec();
    message.extend_from_slice(b": ");
    let len = loc.len().min(ERROR_CTX_LEN);
    message.extend_from_slice(&loc[..len]);
    if loc.len() > len {
        message.extend_from_slice(b"...");
    }
    *cause = Some(CString::new(message).expect("JSON diagnostic contains no NUL"));
}

fn json_tokenize_input(
    input: &CStr,
    cause: Option<&mut Option<CString>>,
) -> Option<Vec<json_token>> {
    let input = input.to_bytes();
    let mut tokens = Vec::with_capacity(1024);
    let mut offset = 0;
    let mut last = 0;
    let mut in_string = false;
    while let Some(&byte) = input.get(offset) {
        last = offset;
        let type_0 = if in_string && byte != b'"' {
            TOK_VALUE
        } else {
            match byte {
                b' ' | b'\t' | b'\n' | b'\r' => {
                    offset += 1;
                    continue;
                }
                b'{' => TOK_OPENOBJECT,
                b'}' => TOK_CLOSEOBJECT,
                b'[' => TOK_OPENARRAY,
                b']' => TOK_CLOSEARRAY,
                b'"' => TOK_QUOTE,
                b':' => TOK_COLON,
                b',' => TOK_COMMA,
                _ => TOK_VALUE,
            }
        };
        let len = if type_0 == TOK_VALUE {
            let Some(len) = json_tokenize_value(&tokens, &input[offset..]) else {
                json_error(cause, c"tokenization error", &input[offset..]);
                return None;
            };
            len
        } else {
            1
        };
        tokens.push(json_token {
            type_0,
            offset,
            len,
        });
        if type_0 == TOK_QUOTE {
            in_string = !in_string;
        }
        offset += len;
    }
    // tmux points EOF at the start of the last scanned token/whitespace.
    tokens.push(json_token {
        type_0: TOK_EOF,
        offset: last,
        len: 0,
    });
    Some(tokens)
}

fn json_tokenize_value(tokens: &[json_token], input: &[u8]) -> Option<usize> {
    match tokens.last()?.type_0 {
        TOK_QUOTE => {
            let mut scan = 0;
            loop {
                match *input.get(scan)? {
                    b'"' => return Some(scan),
                    0..=0x1f => return None,
                    b'\\' => {
                        scan += 1;
                        match *input.get(scan)? {
                            b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't' => scan += 1,
                            b'u' => {
                                if !input
                                    .get(scan + 1..scan + 5)?
                                    .iter()
                                    .all(u8::is_ascii_hexdigit)
                                {
                                    return None;
                                }
                                scan += 5;
                            }
                            _ => return None,
                        }
                    }
                    _ => scan += 1,
                }
            }
        }
        TOK_COLON => input.iter().enumerate().skip(1).find_map(|(index, byte)| {
            // C isspace also includes vertical tab, which Rust's ASCII
            // whitespace predicate deliberately omits.
            (matches!(byte, b']' | b'}' | b',' | 0x0b) || byte.is_ascii_whitespace())
                .then_some(index)
        }),
        _ => None,
    }
}

fn json_create_node(key: Option<&CStr>, value: JsonValue) -> Box<json_node> {
    Box::new(json_node {
        key: key.map(CStr::to_owned),
        value,
    })
}

fn json_parse_tokens(
    mut tokens: &[json_token],
    pctx: &mut json_parse_ctx<'_, '_>,
) -> Option<Box<json_node>> {
    // Tokenization is complete. Recursive parsers advance this borrowed slice,
    // leaving the EOF token in place on incomplete input.
    if tokens[0].type_0 != TOK_OPENOBJECT {
        pctx.error(c"expected object", &tokens[0]);
        return None;
    }
    let node = json_parse_object(&mut tokens, pctx, None)?;
    if tokens[0].type_0 != TOK_EOF {
        pctx.error(c"unexpected trailing data", &tokens[0]);
        return None;
    }
    Some(node)
}

fn json_parse_key(
    tokens: &mut &[json_token],
    pctx: &mut json_parse_ctx<'_, '_>,
) -> Option<CString> {
    let start = tokens[0];
    if tokens[0].type_0 == TOK_QUOTE {
        *tokens = &tokens[1..];
        if tokens[0].type_0 == TOK_VALUE {
            let bytes = pctx.text(&tokens[0]);
            *tokens = &tokens[1..];
            if tokens[0].type_0 == TOK_QUOTE {
                *tokens = &tokens[1..];
                return Some(CString::new(bytes).expect("JSON key contains no NUL"));
            }
        }
    }
    pctx.error(c"invalid key", &start);
    None
}

fn json_parse_object(
    tokens: &mut &[json_token],
    pctx: &mut json_parse_ctx<'_, '_>,
    key: Option<&CStr>,
) -> Option<Box<json_node>> {
    if tokens[0].type_0 != TOK_OPENOBJECT {
        return None;
    }
    pctx.depth += 1;
    if pctx.depth > PARSE_DEPTH_MAX {
        pctx.error(c"parse depth exceeded", &tokens[0]);
        return None;
    }
    *tokens = &tokens[1..];
    let mut object = json_create_node(key, JsonValue::Object(Default::default()));
    loop {
        if tokens[0].type_0 == TOK_CLOSEOBJECT {
            *tokens = &tokens[1..];
            pctx.depth -= 1;
            return Some(object);
        }
        let Some(key) = json_parse_key(tokens, pctx) else {
            break;
        };
        if json_find(&object, &key).is_some() {
            pctx.error(c"duplicate key", &tokens[0]);
            break;
        }
        if tokens[0].type_0 != TOK_COLON {
            pctx.error(c"missing colon", &tokens[0]);
            break;
        }
        *tokens = &tokens[1..];
        let field = match tokens[0].type_0 {
            TOK_QUOTE => json_parse_string(tokens, pctx, Some(&key)),
            TOK_VALUE => {
                let bytes = pctx.text(&tokens[0]);
                if bytes[0].is_ascii_digit()
                    || (bytes[0] == b'-' && bytes.get(1).is_some_and(u8::is_ascii_digit))
                {
                    json_parse_number(tokens, pctx, Some(&key))
                } else {
                    json_parse_boolean(tokens, pctx, Some(&key))
                }
            }
            TOK_OPENOBJECT => json_parse_object(tokens, pctx, Some(&key)),
            TOK_OPENARRAY => json_parse_array(tokens, pctx, Some(&key)),
            _ => {
                pctx.error(c"unexpected value when parsing object", &tokens[0]);
                break;
            }
        };
        let field = field?;
        let JsonValue::Object(fields) = &mut object.value else {
            unreachable!()
        };
        fields.entries.insert(key.to_bytes().to_vec(), field);
        if tokens[0].type_0 == TOK_COMMA {
            if tokens[1].type_0 == TOK_CLOSEOBJECT {
                pctx.error(c"invalid object", &tokens[0]);
                break;
            }
            *tokens = &tokens[1..];
        } else if tokens[0].type_0 != TOK_CLOSEOBJECT {
            pctx.error(c"invalid object", &tokens[0]);
            break;
        }
    }
    None
}

fn json_parse_array(
    tokens: &mut &[json_token],
    pctx: &mut json_parse_ctx<'_, '_>,
    key: Option<&CStr>,
) -> Option<Box<json_node>> {
    if tokens[0].type_0 != TOK_OPENARRAY {
        return None;
    }
    *tokens = &tokens[1..];
    let mut array = json_create_node(key, JsonValue::Array(Default::default()));
    loop {
        if tokens[0].type_0 == TOK_CLOSEARRAY {
            *tokens = &tokens[1..];
            return Some(array);
        }
        if tokens[0].type_0 != TOK_OPENOBJECT {
            pctx.error(c"invalid array member", &tokens[0]);
            break;
        }
        let member = json_parse_object(tokens, pctx, None)?;
        let JsonValue::Array(members) = &mut array.value else {
            unreachable!()
        };
        members.members.push(*member);
        if tokens[0].type_0 == TOK_COMMA {
            if tokens[1].type_0 == TOK_CLOSEARRAY {
                pctx.error(c"invalid array", &tokens[0]);
                break;
            }
            *tokens = &tokens[1..];
        } else if tokens[0].type_0 != TOK_CLOSEARRAY {
            pctx.error(c"invalid array", &tokens[0]);
            break;
        }
    }
    None
}

fn json_parse_string(
    tokens: &mut &[json_token],
    pctx: &mut json_parse_ctx<'_, '_>,
    key: Option<&CStr>,
) -> Option<Box<json_node>> {
    let start = tokens[0];
    if tokens[0].type_0 == TOK_QUOTE {
        *tokens = &tokens[1..];
        if tokens[0].type_0 == TOK_VALUE {
            let bytes = pctx.text(&tokens[0]);
            *tokens = &tokens[1..];
            if tokens[0].type_0 == TOK_QUOTE {
                *tokens = &tokens[1..];
                let string = CString::new(bytes).expect("JSON token contains no NUL");
                return Some(json_create_node(key, JsonValue::String(string)));
            }
        }
    }
    pctx.error(c"invalid string", &start);
    None
}

fn json_parse_number(
    tokens: &mut &[json_token],
    pctx: &mut json_parse_ctx<'_, '_>,
    key: Option<&CStr>,
) -> Option<Box<json_node>> {
    let bytes = pctx.text(&tokens[0]);
    if !(bytes.starts_with(b"0") && bytes.len() != 1
        || bytes.starts_with(b"-0") && bytes.len() != 2)
    {
        if let Some(number) = std::str::from_utf8(bytes)
            .ok()
            .and_then(|text| text.parse::<i64>().ok())
        {
            *tokens = &tokens[1..];
            return Some(json_create_node(key, JsonValue::Number(number)));
        }
    }
    pctx.error(c"invalid number", &tokens[0]);
    None
}

fn json_parse_boolean(
    tokens: &mut &[json_token],
    pctx: &mut json_parse_ctx<'_, '_>,
    key: Option<&CStr>,
) -> Option<Box<json_node>> {
    let boolean = match pctx.text(&tokens[0]) {
        b"true" => 1,
        b"false" => 0,
        _ => {
            pctx.error(c"invalid boolean", &tokens[0]);
            return None;
        }
    };
    *tokens = &tokens[1..];
    Some(json_create_node(key, JsonValue::Boolean(boolean)))
}
fn json_string_append(buffer: &mut SegmentedBuf, node: &json_node) {
    match &node.value {
        JsonValue::String(string) => {
            buffer.put_slice(b"\"");
            buffer.put_slice(string.to_bytes());
            buffer.put_slice(b"\"");
        }
        JsonValue::Number(number) => {
            evbuffer_add_formatted(buffer, |out| write!(out, "{number}"));
        }
        JsonValue::Boolean(boolean) => {
            buffer.put_slice(if *boolean != 0 { b"true" } else { b"false" })
        }
        JsonValue::Object(fields) => {
            buffer.put_slice(b"{");
            for (index, (key, field)) in fields.entries.iter().enumerate() {
                if index != 0 {
                    buffer.put_slice(b",");
                }
                buffer.put_slice(b"\"");
                buffer.put_slice(key);
                buffer.put_slice(b"\":");
                json_string_append(buffer, field);
            }
            buffer.put_slice(b"}");
        }
        JsonValue::Array(members) => {
            buffer.put_slice(b"[");
            for (index, member) in members.members.iter().enumerate() {
                if index != 0 {
                    buffer.put_slice(b",");
                }
                json_string_append(buffer, member);
            }
            buffer.put_slice(b"]");
        }
    }
}

pub fn json_to_string(node: &json_node) -> CString {
    let mut buffer = evbuffer_new();
    json_string_append(&mut buffer, node);
    let bytes = evbuffer_pullup(&mut buffer, -1)
        .unwrap_or_default()
        .to_vec();
    CString::new(bytes).expect("serialized JSON contains no NUL")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostics_preserve_bytes_and_context_truncation() {
        let mut cause = None;
        json_error(Some(&mut cause), c"\xff", b"abcdefghZ");
        assert_eq!(cause.take().unwrap().to_bytes(), b"\xff: abcdefgh...");
        json_error(Some(&mut cause), c"\xff", b"ab");
        assert_eq!(cause.take().unwrap().to_bytes(), b"\xff: ab");
    }
}
