//! Antigravity CLI (`agy`) agent detector.
//!
//! State comes from herdr's `antigravity.toml` rules under tiri's overlay (see
//! [`manifest`](super::manifest)); this module identifies the process, its
//! headless invocations, and its session.
//!
//! Observable difference from the other agents: `agy` keeps its conversation in
//! a sqlite database (`~/.gemini/antigravity-cli/conversations/<uuid>.db`)
//! rather than a JSONL transcript. The open database still names the session, so
//! `#{pane_agent_session_id}` is reported, but the line-oriented session scan
//! cannot read a model out of it — `#{pane_agent_model}` stays empty for agy
//! panes.

use std::ffi::{CString, OsStr, OsString};
use std::path::Path;

use super::manifest::{Bundle, Input, Rules};
use super::{AgentDetector, AgentState, Detection, SessionIdSource, is_uuid};

pub(crate) const RULES: Bundle = Bundle {
    agent: "agy",
    herdr: include_str!("../../manifests/herdr/antigravity.toml"),
    tiri: include_str!("../../manifests/tiri/agy.toml"),
};

/// Recognizes Antigravity CLI panes.
pub(crate) struct AgyDetector {
    pub(crate) rules: Rules,
}

impl AgentDetector for AgyDetector {
    fn label(&self) -> &'static str {
        "agy"
    }

    fn matches_program(&self, program: &OsStr) -> bool {
        agy_program_name(program)
    }

    fn invocation_state(&self, arguments: &[OsString]) -> Option<AgentState> {
        agy_headless_invocation(arguments).then_some(AgentState::Working)
    }

    fn session_id_source(&self) -> Option<SessionIdSource> {
        Some(SessionIdSource::ProcessTreeOpenFiles)
    }

    fn session_id_from_open_file(&self, path: &Path) -> Option<CString> {
        session_id_from_conversation_path(path)
    }

    fn detect(&self, screen: &str, title: Option<&str>) -> Detection {
        self.rules.detect(Input {
            screen,
            title: title.unwrap_or_default(),
        })
    }
}

/// `agy` holds its conversation database — and the sqlite sidecars beside it —
/// open for the life of the conversation, and names it after the conversation
/// id. Nothing is open before the first prompt, so a freshly started pane
/// reports no session until then.
fn session_id_from_conversation_path(path: &Path) -> Option<CString> {
    let name = path.file_name()?.to_str()?;
    let stem = ["-shm", "-wal", ""]
        .iter()
        .find_map(|suffix| name.strip_suffix(suffix)?.strip_suffix(".db"))?;
    let conversations = path.parent()?;
    if conversations.file_name()? != "conversations"
        || conversations.parent()?.file_name()? != "antigravity-cli"
    {
        return None;
    }
    is_uuid(stem).then(|| CString::new(stem.to_ascii_lowercase()).expect("a UUID has no NUL"))
}

fn agy_program_name(program: &OsStr) -> bool {
    let name = Path::new(program)
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or_default()
        .trim_start_matches('.')
        .to_ascii_lowercase();
    matches!(name.as_str(), "agy" | "agy.exe")
}

/// `agy -p/--print` prints one answer instead of running the TUI, so there is
/// no prompt or status line to read: the process is working for its whole
/// lifetime. `-i/--prompt-interactive` seeds a prompt and stays interactive, so
/// it is deliberately not matched here.
fn agy_headless_invocation(arguments: &[OsString]) -> bool {
    arguments
        .iter()
        .skip(1)
        .any(|argument| matches!(argument.to_str(), Some("-p" | "--print" | "--prompt")))
}

#[cfg(test)]
mod tests {
    use std::ffi::{OsStr, OsString};
    use std::path::Path;

    use super::super::detect_attributed;
    use super::super::manifest::Rules;
    use super::{
        AgentState, AgyDetector, Detection, RULES, agy_headless_invocation, agy_program_name,
        session_id_from_conversation_path,
    };

    /// Classify `screen` as a pane attributed to this agent.
    fn detect(screen: &str) -> Detection {
        let detector = AgyDetector {
            rules: Rules::bundled(&RULES),
        };
        detect_attributed(&detector, screen, None)
    }

    const RULE: &str = "────────────────────────────────────────";

    fn screen(body: &str) -> String {
        format!("Antigravity CLI 1.1.10\n\n{body}")
    }

    #[test]
    fn resting_composer_reports_idle() {
        let text = screen(&format!(
            "{RULE}\n>\n{RULE}\n? for shortcuts                    Gemini 3.6 Flash · high"
        ));
        assert_eq!(detect(&text), Detection::State(AgentState::Idle));
    }

    #[test]
    fn spinner_status_reports_working() {
        for status in [
            "⣷  Generating...",
            "   Generating...",
            "⣽  Loading...",
            "   Loading...",
            "⣟  Working...",
            "   Working...",
            "⣾  Running...",
            "   Running...",
        ] {
            let text = screen(&format!(
                "● Bash(git status) (ctrl+o to expand)\n{status}\n{RULE}\n>\n{RULE}\n\
                 esc to cancel                      Gemini 3.6 Flash · high"
            ));
            assert_eq!(detect(&text), Detection::State(AgentState::Working));
        }
    }

    #[test]
    fn a_running_background_task_reports_working_despite_the_resting_footer() {
        let text = screen(&format!(
            "{RULE}\n>\n{RULE}\n  ● [22:37:14] sleep 12 && echo finished running\n{RULE}\n\
             ? for shortcuts                Gemini 3.6 Flash · high · 1 task(s) · /tasks"
        ));
        assert_eq!(detect(&text), Detection::State(AgentState::Working));
    }

    #[test]
    fn a_model_written_progress_title_reports_working() {
        for status in [
            "⣷  Analyzing Pane Mappings...",
            "⢿  Refining The Approach...",
        ] {
            let text = screen(&format!(
                "● Bash(cargo nextest run) (ctrl+o to expand)\n{status}\n{RULE}\n>\n{RULE}\n\
                 esc to cancel                      Gemini 3.7 Flash · high"
            ));
            assert_eq!(detect(&text), Detection::State(AgentState::Working));
        }
    }

    #[test]
    fn a_finished_tool_in_the_transcript_does_not_report_working() {
        let text = screen(&format!(
            "● Bash(git status) (ctrl+o to expand)\n  Analyzing the working tree...\n\
             {RULE}\n>\n{RULE}\n? for shortcuts                    Gemini 3.6 Flash · high"
        ));
        assert_eq!(detect(&text), Detection::State(AgentState::Idle));
    }

    #[test]
    fn permission_dialog_outranks_the_status_line() {
        let text = screen(
            "⣷  Generating...\n● Bash(git status) (ctrl+o to expand)\nRequesting permission for:\n\
             git status\nDo you want to proceed?\n1. Yes\n2. Yes, and don't ask again\n\
             3. Yes, and don't ask again this session\n4. No\n\
             ↑/↓ Navigate · tab Amend · ctrl+g edit/expand command",
        );
        assert_eq!(detect(&text), Detection::State(AgentState::Blocked));
    }

    #[test]
    fn trust_dialog_reports_blocked() {
        let text = screen(
            "Do you trust the contents of this project?\n> Yes, I trust this folder\n  No, exit\n\
             ↑/↓ Navigate · enter Confirm\n                    Gemini 3.6 Flash · high",
        );
        assert_eq!(detect(&text), Detection::State(AgentState::Blocked));
    }

    #[test]
    fn a_footer_only_permission_dialog_reports_blocked() {
        let text = screen("↑/↓ Navigate · tab Amend · f full diff\nesc to cancel");
        assert_eq!(detect(&text), Detection::State(AgentState::Blocked));
    }

    #[test]
    fn shortcuts_overlay_preserves_previous_state() {
        let text = screen(
            "Shortcuts\n  ctrl+o  expand tool output\n\
             Keyboard: ↑/↓ Navigate  ←/→ Switch View  esc Close",
        );
        assert_eq!(detect(&text), Detection::KeepPrevious);
    }

    #[test]
    fn slash_menu_preserves_previous_state_despite_cancel_footer() {
        let text = screen(&format!(
            "/help   Show help\n/model  Switch model\n\
             ↑/↓ Navigate · enter Select · tab Complete\n{RULE}\n> /\n{RULE}\n\
             esc to cancel                      Gemini 3.6 Flash · high"
        ));
        assert_eq!(detect(&text), Detection::KeepPrevious);
    }

    #[test]
    fn a_screen_no_rule_explains_is_the_idle_fallback() {
        assert_eq!(
            detect("some scrolled output\n                     Gemini 3.6 Flash · high"),
            Detection::State(AgentState::Idle)
        );
        assert_eq!(
            detect("some scrolled output\n↑/↓ Navigate · enter Confirm"),
            Detection::State(AgentState::Idle)
        );
    }

    #[test]
    fn recognizes_agy_programs() {
        assert!(agy_program_name(OsStr::new("agy")));
        assert!(agy_program_name(OsStr::new(
            "/run/current-system/sw/bin/agy"
        )));
        assert!(agy_program_name(OsStr::new("AGY.EXE")));
        assert!(!agy_program_name(OsStr::new("agyness")));
    }

    #[test]
    fn recognizes_headless_invocations() {
        let args = |values: &[&str]| values.iter().map(OsString::from).collect::<Vec<_>>();

        assert!(agy_headless_invocation(&args(&["agy", "-p", "do work"])));
        assert!(agy_headless_invocation(&args(&[
            "agy", "--print", "do work"
        ])));
        assert!(!agy_headless_invocation(&args(&[
            "agy",
            "--prompt-interactive",
            "do work"
        ])));
        assert!(!agy_headless_invocation(&args(&["agy"])));
    }

    #[test]
    fn extracts_session_id_from_open_conversation_database() {
        let base = "/home/me/.gemini/antigravity-cli/conversations";
        let id = "019F9757-7C53-7898-A7D1-C8C780212888";
        for suffix in [".db", ".db-wal", ".db-shm"] {
            assert_eq!(
                session_id_from_conversation_path(&Path::new(base).join(format!("{id}{suffix}")))
                    .as_deref(),
                Some(c"019f9757-7c53-7898-a7d1-c8c780212888")
            );
        }
        assert_eq!(
            session_id_from_conversation_path(&Path::new(base).join("settings.json")),
            None
        );
        assert_eq!(
            session_id_from_conversation_path(Path::new(
                "/home/me/other/conversations/019f9757-7c53-7898-a7d1-c8c780212888.db"
            )),
            None
        );
    }
}
