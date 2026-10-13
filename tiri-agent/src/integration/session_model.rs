//! Incremental model discovery from agent session files.
//!
//! Every supported agent appends JSONL records to its session file, and each
//! records the model serving the conversation as a `"model":"…"` member —
//! Claude and Pi on assistant messages, Codex on per-turn `turn_context`
//! records. The value can change mid-session (a `/model` switch), and Codex
//! names it far from the end of the file, so a fixed-size tail read is not
//! enough. [`ModelScan`] instead scans the file *incrementally*: session files
//! are append-only, so each poll reads only the newly appended bytes and keeps
//! the last well-formed model value seen so far.
//!
//! The byte-level search is deliberately not a JSON parser. A `"model":"…"`
//! occurrence embedded in a JSON *string* (say, a tool result quoting this very
//! documentation) is escaped as `\"model\":\"…\"` on disk, so searching for the
//! unescaped byte pattern only matches structural members. This remains a
//! heuristic: an agent could log an unrelated structural `model` member, which
//! is why values are validated as plausible model identifiers before use.

use std::ffi::CString;
use std::path::{Path, PathBuf};

use super::ProcessSource;

/// Bytes read per `read_span` call while catching up on appended content.
const SCAN_CHUNK: usize = 64 * 1024;

/// Bytes of the previous chunk kept in front of the next one, so a `"model"`
/// member split across a chunk boundary is still seen whole. Longer than any
/// valid occurrence (needle plus a maximum-length value).
const CARRY: usize = 256;

/// The serialized member introducing a model value.
const NEEDLE: &[u8] = b"\"model\":\"";

/// Incremental scanner over one append-only session file.
pub(crate) struct ModelScan {
    path: PathBuf,
    /// Next file offset to read; everything before it has been scanned.
    offset: u64,
    /// Tail of the previously scanned bytes, re-prepended to the next chunk.
    carry: Vec<u8>,
}

impl ModelScan {
    pub(crate) fn new(path: PathBuf) -> Self {
        Self {
            path,
            offset: 0,
            carry: Vec::new(),
        }
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    /// Bytes scanned so far. Because [`advance`](Self::advance) reads to end of
    /// file, comparing this across polls tells whether the file grew.
    pub(crate) fn offset(&self) -> u64 {
        self.offset
    }

    /// Scan bytes appended since the last call and return the newest model
    /// they name, or `None` when nothing new names one. The first call reads
    /// the whole file, so attaching to a long-running session still finds the
    /// model it last recorded.
    pub(crate) fn advance(&mut self, source: &dyn ProcessSource) -> Option<CString> {
        let mut newest = None;
        while let Some(chunk) = source.read_span(&self.path, self.offset, SCAN_CHUNK) {
            if chunk.is_empty() {
                break;
            }
            self.offset += chunk.len() as u64;
            let short_read = chunk.len() < SCAN_CHUNK;
            let mut window = std::mem::take(&mut self.carry);
            window.extend_from_slice(&chunk);
            if let Some(model) = last_model_in(&window) {
                newest = Some(model);
            }
            let keep = window.len().min(CARRY);
            window.drain(..window.len() - keep);
            self.carry = window;
            if short_read {
                break;
            }
        }
        newest
    }
}

/// The last plausible model value named in `window`. An occurrence whose
/// closing quote lies beyond the window (split across chunks) is skipped here
/// and picked up whole on the next chunk via the carry.
fn last_model_in(window: &[u8]) -> Option<CString> {
    let mut end = window.len();
    while let Some(at) = window[..end]
        .windows(NEEDLE.len())
        .rposition(|bytes| bytes == NEEDLE)
    {
        let value_start = at + NEEDLE.len();
        if let Some(len) = window[value_start..].iter().position(|&byte| byte == b'"') {
            let value = &window[value_start..value_start + len];
            if is_model_identifier(value) {
                return CString::new(value).ok();
            }
        }
        end = at;
    }
    None
}

/// Whether `value` is shaped like a model identifier. This rejects the empty
/// string, placeholder values such as Claude's `<synthetic>`, and anything
/// containing escapes or free text.
fn is_model_identifier(value: &[u8]) -> bool {
    (1..=64).contains(&value.len())
        && value
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'/'))
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::ffi::OsString;
    use std::path::{Path, PathBuf};

    use super::super::ProcessSource;
    use super::{ModelScan, SCAN_CHUNK, last_model_in};

    /// A [`ProcessSource`] serving only file contents, mutable between polls so
    /// tests can append to a "session file".
    struct FileSource {
        files: RefCell<HashMap<PathBuf, Vec<u8>>>,
    }

    impl FileSource {
        fn new(path: &str, content: &[u8]) -> Self {
            Self {
                files: RefCell::new(HashMap::from([(PathBuf::from(path), content.to_vec())])),
            }
        }

        fn append(&self, path: &str, content: &[u8]) {
            self.files
                .borrow_mut()
                .get_mut(Path::new(path))
                .unwrap()
                .extend_from_slice(content);
        }
    }

    impl ProcessSource for FileSource {
        fn process_table(&self) -> Option<Vec<(u32, u32)>> {
            None
        }

        fn programs(&self, _pid: u32) -> Vec<OsString> {
            Vec::new()
        }

        fn read_span(&self, path: &Path, offset: u64, max_len: usize) -> Option<Vec<u8>> {
            let files = self.files.borrow();
            let content = files.get(path)?;
            let start = (offset as usize).min(content.len());
            let end = (start + max_len).min(content.len());
            Some(content[start..end].to_vec())
        }
    }

    #[test]
    fn finds_the_last_model_in_a_transcript() {
        let content =
            br#"{"type":"assistant","message":{"model":"claude-opus-5","role":"assistant"}}
{"type":"assistant","message":{"model":"claude-fable-5","role":"assistant"}}
"#;
        let source = FileSource::new("/s.jsonl", content);
        let mut scan = ModelScan::new(PathBuf::from("/s.jsonl"));
        assert_eq!(scan.advance(&source).as_deref(), Some(c"claude-fable-5"));
        // Nothing appended: no new model.
        assert_eq!(scan.advance(&source), None);
    }

    #[test]
    fn appended_model_switch_is_picked_up_incrementally() {
        let source = FileSource::new(
            "/s.jsonl",
            br#"{"type":"turn_context","payload":{"model":"gpt-5-codex"}}"#,
        );
        let mut scan = ModelScan::new(PathBuf::from("/s.jsonl"));
        assert_eq!(scan.advance(&source).as_deref(), Some(c"gpt-5-codex"));

        source.append(
            "/s.jsonl",
            br#"
{"type":"turn_context","payload":{"model":"gpt-5.1-codex"}}"#,
        );
        assert_eq!(scan.advance(&source).as_deref(), Some(c"gpt-5.1-codex"));
    }

    #[test]
    fn occurrence_split_across_chunks_is_found_via_the_carry() {
        // Place the only occurrence so the chunk boundary falls inside it.
        let mut content = vec![b' '; SCAN_CHUNK - 4];
        content.extend_from_slice(br#""model":"claude-opus-5""#);
        let source = FileSource::new("/s.jsonl", &content);
        let mut scan = ModelScan::new(PathBuf::from("/s.jsonl"));
        assert_eq!(scan.advance(&source).as_deref(), Some(c"claude-opus-5"));
    }

    #[test]
    fn escaped_and_placeholder_occurrences_are_ignored() {
        // A tool result quoting a model member is escaped on disk; a synthetic
        // placeholder is not a model. Neither may shadow the real value.
        let content = br#"{"message":{"model":"claude-opus-5"}}
{"toolResult":"saw \"model\":\"gpt-oops\" in a log"}
{"message":{"model":"<synthetic>"}}
"#;
        assert_eq!(last_model_in(content).as_deref(), Some(c"claude-opus-5"));
        assert_eq!(last_model_in(br#""model":"""#), None);
    }
}
