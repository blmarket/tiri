//! Pi coding-agent detector.
//!
//! State comes from herdr's `pi.toml` rules under tiri's overlay (see
//! [`manifest`](super::manifest)); this module identifies the process, its
//! headless invocations, and its session.

use std::ffi::{CString, OsStr, OsString};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use super::manifest::{Bundle, Input, Rules};
use super::{AgentDetector, AgentState, Detection, SessionIdSource, is_uuid};

pub(crate) const RULES: Bundle = Bundle {
    agent: "pi",
    herdr: include_str!("../../manifests/herdr/pi.toml"),
    tiri: include_str!("../../manifests/tiri/pi.toml"),
};

/// Recognizes Pi coding-agent panes.
pub(crate) struct PiDetector {
    pub(crate) rules: Rules,
}

impl AgentDetector for PiDetector {
    fn label(&self) -> &'static str {
        "pi"
    }

    fn matches_program(&self, program: &OsStr) -> bool {
        pi_program_name(program)
    }

    fn matches_invocation(&self, arguments: &[OsString]) -> bool {
        arguments
            .iter()
            .take(2)
            .any(|argument| pi_program_name(argument) || pi_runtime_script(argument))
    }

    fn invocation_state(&self, arguments: &[OsString]) -> Option<AgentState> {
        pi_headless_invocation(arguments).then_some(AgentState::Working)
    }

    fn session_id_source(&self) -> Option<SessionIdSource> {
        Some(SessionIdSource::AgentCwdTranscript)
    }

    fn session_dir_for_cwd(&self, cwd: &Path) -> Option<PathBuf> {
        session_dir(cwd)
    }

    fn session_id_from_file_name(&self, name: &OsStr) -> Option<CString> {
        session_id_from_file_name(name)
    }

    fn detect(&self, screen: &str, title: Option<&str>) -> Detection {
        self.rules.detect(Input {
            screen,
            title: title.unwrap_or_default(),
        })
    }
}

/// Pi stores default sessions below a cwd-derived directory in its agent home.
pub(crate) fn session_dir(cwd: &Path) -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(
        Path::new(&home)
            .join(".pi/agent/sessions")
            .join(OsStr::from_bytes(project_slug(cwd).as_bytes())),
    )
}

fn project_slug(cwd: &Path) -> CString {
    let path = cwd.to_string_lossy();
    let path = path.trim_start_matches(['/', '\\']);
    let mut encoded = b"--".to_vec();
    for character in path.chars() {
        let character = if matches!(character, '/' | '\\' | ':') {
            '-'
        } else {
            character
        };
        let mut bytes = [0; 4];
        encoded.extend_from_slice(character.encode_utf8(&mut bytes).as_bytes());
    }
    encoded.extend_from_slice(b"--");
    CString::new(encoded).expect("a project slug has no NUL")
}

/// Default filenames are `<timestamp>_<session-id>.jsonl`. Accepting a bare
/// UUID stem also covers explicitly named session files without weakening UUID
/// validation.
fn session_id_from_file_name(name: &OsStr) -> Option<CString> {
    let stem = name.to_str()?.strip_suffix(".jsonl")?;
    let candidate = stem.rsplit_once('_').map_or(stem, |(_, id)| id);
    is_uuid(candidate)
        .then(|| CString::new(candidate.to_ascii_lowercase()).expect("a UUID has no NUL"))
}

fn pi_program_name(program: &OsStr) -> bool {
    let name = Path::new(program)
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or_default()
        .trim_start_matches('.')
        .to_ascii_lowercase();
    matches!(
        name.as_str(),
        "pi" | "pi.exe" | "pi.cmd" | "pi.js" | "pi.mjs"
    )
}

fn pi_runtime_script(argument: &OsStr) -> bool {
    let path = argument.to_string_lossy().replace('\\', "/").to_lowercase();
    path.ends_with("/dist/cli.js") && path.contains("/pi-coding-agent/")
}

fn pi_headless_invocation(arguments: &[OsString]) -> bool {
    arguments
        .iter()
        .skip(1)
        .any(|argument| matches!(argument.to_str(), Some("--print" | "-p")))
        || arguments
            .windows(2)
            .any(|pair| pair[0] == "--mode" && matches!(pair[1].to_str(), Some("json")))
}

#[cfg(test)]
mod tests {
    use std::ffi::{OsStr, OsString};
    use std::path::Path;

    use super::super::detect_attributed;
    use super::super::manifest::Rules;
    use super::{
        AgentState, Detection, PiDetector, RULES, pi_headless_invocation, pi_program_name,
        pi_runtime_script, project_slug, session_dir, session_id_from_file_name,
    };

    /// Classify `screen` as a pane attributed to this agent.
    fn detect(screen: &str, title: Option<&str>) -> Detection {
        let detector = PiDetector {
            rules: Rules::bundled(&RULES),
        };
        detect_attributed(&detector, screen, title)
    }

    #[test]
    fn resting_editor_reports_idle() {
        let screen = "pi v0.80.6\n────────\n \n────────\n~/work/project";
        assert_eq!(
            detect(screen, Some("π - project")),
            Detection::State(AgentState::Idle)
        );
    }

    #[test]
    fn working_indicator_outranks_static_title() {
        let screen = "previous output\n⠹ Working...\n────────\n────────\n~/work/project";
        assert_eq!(
            detect(screen, Some("π - project")),
            Detection::State(AgentState::Working)
        );
    }

    #[test]
    fn bordered_working_indicator_reports_working() {
        let screen = "running a tool\n── ⠋ Working ────────\n────────\n/work/proj";
        assert_eq!(
            detect(screen, Some("π - project")),
            Detection::State(AgentState::Working)
        );
    }

    #[test]
    fn confirmation_dialog_outranks_working_indicator() {
        let screen = "⠹ Working...\n────────\nAllow command?\n→ Yes\n  No\n↑↓ navigate  enter select  esc cancel\n────────";
        assert_eq!(
            detect(screen, Some("π - project")),
            Detection::State(AgentState::Blocked)
        );
    }

    #[test]
    fn extension_input_reports_blocked() {
        let screen = "────────\nEnter a value\n\nenter submit  esc cancel\n────────";
        assert_eq!(
            detect(screen, Some("π - project")),
            Detection::State(AgentState::Blocked)
        );
    }

    #[test]
    fn session_picker_preserves_previous_state() {
        let screen =
            "────────\nResume Session (Current Folder)\nctrl+s sort · ctrl+n named\n────────";
        assert_eq!(detect(screen, Some("π - project")), Detection::KeepPrevious);
    }

    #[test]
    fn recognizes_direct_and_runtime_wrapped_pi() {
        assert!(pi_program_name(OsStr::new("pi")));
        assert!(pi_program_name(OsStr::new("/usr/local/bin/pi.exe")));
        assert!(!pi_program_name(OsStr::new("pilot")));
        assert!(pi_runtime_script(OsStr::new(
            "/opt/node_modules/@earendil-works/pi-coding-agent/dist/cli.js"
        )));
        assert!(!pi_runtime_script(OsStr::new("/opt/other/dist/cli.js")));
    }

    #[test]
    fn recognizes_headless_invocations() {
        assert!(pi_headless_invocation(&[
            OsString::from("pi"),
            OsString::from("--print"),
            OsString::from("do work"),
        ]));
        assert!(pi_headless_invocation(&[
            OsString::from("pi"),
            OsString::from("--mode"),
            OsString::from("json"),
            OsString::from("do work"),
        ]));
        assert!(!pi_headless_invocation(&[
            OsString::from("pi"),
            OsString::from("do work"),
        ]));
    }

    #[test]
    fn cwd_maps_to_pi_session_directory() {
        assert_eq!(
            project_slug(Path::new("/home/hun/srv/pi")),
            c"--home-hun-srv-pi--"
        );
        let home = std::env::var_os("HOME").expect("HOME set in test environment");
        assert_eq!(
            session_dir(Path::new("/work/project")),
            Some(Path::new(&home).join(".pi/agent/sessions/--work-project--"))
        );
    }

    #[test]
    fn extracts_session_id_from_timestamped_file_name() {
        assert_eq!(
            session_id_from_file_name(OsStr::new(
                "2026-07-25T03-35-20-915Z_019F9757-7C53-7898-A7D1-C8C780212888.jsonl"
            ))
            .as_deref(),
            Some(c"019f9757-7c53-7898-a7d1-c8c780212888")
        );
        assert_eq!(
            session_id_from_file_name(OsStr::new("not-a-session.jsonl")),
            None
        );
    }
}
