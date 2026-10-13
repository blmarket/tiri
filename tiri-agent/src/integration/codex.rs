//! Codex agent detector.
//!
//! State comes from herdr's `codex.toml` rules under tiri's overlay (see
//! [`manifest`](super::manifest)); this module identifies the process, its
//! headless invocations, and its session.

use std::ffi::{CString, OsStr, OsString};
use std::path::Path;

use super::manifest::{Bundle, Input, Rules};
use super::{AgentDetector, AgentState, Detection, SessionIdSource, is_uuid};

pub(crate) const RULES: Bundle = Bundle {
    agent: "codex",
    herdr: include_str!("../../manifests/herdr/codex.toml"),
    tiri: include_str!("../../manifests/tiri/codex.toml"),
};

/// Recognizes OpenAI Codex panes.
pub(crate) struct CodexDetector {
    pub(crate) rules: Rules,
}

impl AgentDetector for CodexDetector {
    fn label(&self) -> &'static str {
        "codex"
    }

    fn matches_program(&self, program: &OsStr) -> bool {
        codex_program_name(program)
    }

    fn invocation_state(&self, arguments: &[OsString]) -> Option<AgentState> {
        codex_headless_invocation(arguments).then_some(AgentState::Working)
    }

    fn session_id_source(&self) -> Option<SessionIdSource> {
        Some(SessionIdSource::ProcessTreeOpenFiles)
    }

    fn session_id_from_open_file(&self, path: &Path) -> Option<CString> {
        session_id_from_rollout_path(path)
    }

    fn detect(&self, screen: &str, title: Option<&str>) -> Detection {
        self.rules.detect(Input {
            screen,
            title: title.unwrap_or_default(),
        })
    }
}

/// Codex keeps the active rollout open for the lifetime of a thread. Its
/// filename ends in the thread UUID, including when the thread was resumed.
fn session_id_from_rollout_path(path: &Path) -> Option<CString> {
    let name = path.file_name()?.to_str()?;
    let stem = name.strip_prefix("rollout-")?.strip_suffix(".jsonl")?;
    let session_id = stem.get(stem.len().checked_sub(36)?..)?;
    is_uuid(session_id)
        .then(|| CString::new(session_id.to_ascii_lowercase()).expect("a UUID has no NUL"))
}

fn codex_program_name(program: &OsStr) -> bool {
    let name = Path::new(program)
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or_default()
        .trim_start_matches('.')
        .to_ascii_lowercase();
    matches!(
        name.as_str(),
        "codex" | "codex.exe" | "codex.cmd" | "codex.js" | "codex.mjs" | "codex-wrapped"
    )
}

/// `codex exec` (alias `e`) and `codex review` do not run the interactive TUI:
/// they emit a transcript but no prompt or OSC lifecycle title. The process is
/// therefore working for its entire lifetime; its exit is observed separately
/// by the pane process lifecycle.
fn codex_headless_invocation(arguments: &[OsString]) -> bool {
    arguments
        .iter()
        .skip(1)
        .any(|argument| matches!(argument.to_str(), Some("exec" | "e" | "review")))
}

#[cfg(test)]
mod tests {
    use std::ffi::{OsStr, OsString};
    use std::path::Path;

    use super::super::manifest::Rules;
    use super::super::{AgentDetector, detect_attributed};
    use super::{
        AgentState, CodexDetector, Detection, RULES, codex_headless_invocation, codex_program_name,
        session_id_from_rollout_path,
    };

    /// The state the rules alone assign, before the idle fallback.
    fn rules_state(screen: &str, title: Option<&str>) -> Detection {
        let detector = CodexDetector {
            rules: Rules::bundled(&RULES),
        };
        detector.detect(screen, title)
    }

    /// Classify `screen` as a pane attributed to this agent.
    fn detect(screen: &str, title: Option<&str>) -> Detection {
        let detector = CodexDetector {
            rules: Rules::bundled(&RULES),
        };
        detect_attributed(&detector, screen, title)
    }

    #[test]
    fn current_prompt_overrides_stale_working_text() {
        let screen = "OpenAI Codex\nold output: esc to interrupt\n› ";
        assert_eq!(detect(screen, None), Detection::State(AgentState::Idle));
    }

    #[test]
    fn current_status_marker_reports_working() {
        let screen = "OpenAI Codex\n• Working (2s • esc to interrupt)\n› queued follow-up";
        assert_eq!(detect(screen, None), Detection::State(AgentState::Working));
    }

    #[test]
    fn live_confirmation_reports_blocked() {
        let screen = "OpenAI Codex\n› change files\nAllow command?";
        assert_eq!(detect(screen, None), Detection::State(AgentState::Blocked));
    }

    #[test]
    fn trust_directory_screen_reports_blocked() {
        let screen = "\
> You are in /home/me/project

  Do you trust the contents of this directory? Working with untrusted
  contents comes with higher risk of prompt injection.

› 1. Yes, continue
  2. No, quit

  Press enter to continue";
        assert_eq!(detect(screen, None), Detection::State(AgentState::Blocked));
    }

    #[test]
    fn startup_update_screen_reports_blocked() {
        let screen = "\
✨ Update available! 0.40.0 -> 0.41.0

› 1. Update now (runs `npm install -g @openai/codex`)
  2. Skip
  3. Skip until next version

  Press enter to continue";
        assert_eq!(detect(screen, None), Detection::State(AgentState::Blocked));
    }

    #[test]
    fn an_answered_question_above_the_current_prompt_is_not_blocking() {
        let screen = "OpenAI Codex\nApply the patch? [y/n] y\n• Applied.\n› ";
        assert_eq!(detect(screen, None), Detection::State(AgentState::Idle));
    }

    #[test]
    fn transcript_viewer_preserves_previous_state() {
        let screen = "›\n↑/↓ to scroll · pgup/pgdn to move · home/end to jump · q to quit · esc to edit prev";
        assert_eq!(detect(screen, None), Detection::KeepPrevious);
    }

    #[test]
    fn title_spinner_reports_working_over_idle_screen() {
        // Codex sets a braille-spinner title while working even when the screen
        // tail still shows the resting prompt. The title outranks the screen.
        let screen = "OpenAI Codex\n› ";
        assert_eq!(
            detect(screen, Some("⠹ Working")),
            Detection::State(AgentState::Working)
        );
    }

    #[test]
    fn title_action_required_reports_blocked_over_prompt() {
        let screen = "OpenAI Codex\n› ";
        assert_eq!(
            detect(screen, Some("Action Required")),
            Detection::State(AgentState::Blocked)
        );
    }

    #[test]
    fn title_outranks_transcript_viewer() {
        // A working title takes precedence even while the transcript viewer is
        // open (which would otherwise preserve the previous state).
        let screen = "›\n↑/↓ to scroll · pgup/pgdn to move · home/end to jump · q to quit · esc to edit prev";
        assert_eq!(
            detect(screen, Some("⠹ Working (12s)")),
            Detection::State(AgentState::Working)
        );
    }

    #[test]
    fn plain_title_is_a_last_resort_idle_signal() {
        // No screen evidence at all, but a non-spinner, non-approval title means
        // Codex is idle at its prompt.
        assert_eq!(
            rules_state("", Some("codex — my-project")),
            Detection::State(AgentState::Idle)
        );
        // An empty/whitespace title provides nothing.
        assert_eq!(
            rules_state("", Some("   ")),
            Detection::State(AgentState::Unknown)
        );
    }

    #[test]
    fn recognizes_direct_and_runtime_wrapped_codex_programs() {
        assert!(codex_program_name(OsStr::new("codex")));
        assert!(codex_program_name(OsStr::new("/opt/openai/bin/codex.js")));
        assert!(codex_program_name(OsStr::new(".codex-wrapped")));
        assert!(!codex_program_name(OsStr::new("my-codex-helper")));
    }

    #[test]
    fn recognizes_non_interactive_invocations() {
        let args = |values: &[&str]| values.iter().map(OsString::from).collect::<Vec<_>>();

        assert!(codex_headless_invocation(&args(&[
            "codex", "e", "--yolo", "prompt"
        ])));
        assert!(codex_headless_invocation(&args(&[
            "codex", "--yolo", "exec", "prompt"
        ])));
        assert!(codex_headless_invocation(&args(&[
            "codex",
            "review",
            "--uncommitted"
        ])));
        assert!(!codex_headless_invocation(&args(&[
            "codex", "--yolo", "prompt"
        ])));
    }

    #[test]
    fn extracts_session_id_from_open_rollout_path() {
        let path = Path::new(
            "/home/me/.codex/sessions/2026/07/17/rollout-2026-07-17T11-12-23-019f7147-8F68-7F11-8ECA-FA67874BFEFD.jsonl",
        );
        assert_eq!(
            session_id_from_rollout_path(path).as_deref(),
            Some(c"019f7147-8f68-7f11-8eca-fa67874bfefd")
        );
        assert_eq!(
            session_id_from_rollout_path(Path::new("state_5.sqlite")),
            None
        );
        assert_eq!(
            session_id_from_rollout_path(Path::new("rollout-not-a-uuid.jsonl")),
            None
        );
    }
}
