//! Claude Code agent detector.
//!
//! State comes from herdr's `claude.toml` rules under tiri's overlay (see
//! [`manifest`](super::manifest)); this module identifies the process and its
//! session.

use std::ffi::{CString, OsStr};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use super::manifest::{Bundle, Input, Rules};
use super::{AgentDetector, Detection, SessionEnvStamp, SessionIdSource, is_uuid};

pub(crate) const RULES: Bundle = Bundle {
    agent: "claude",
    herdr: include_str!("../../manifests/herdr/claude.toml"),
    tiri: include_str!("../../manifests/tiri/claude.toml"),
};

/// Recognizes Anthropic Claude Code panes.
pub(crate) struct ClaudeDetector {
    pub(crate) rules: Rules,
}

impl AgentDetector for ClaudeDetector {
    fn label(&self) -> &'static str {
        "claude"
    }

    fn matches_program(&self, program: &OsStr) -> bool {
        claude_program_name(program)
    }

    fn session_id_source(&self) -> Option<SessionIdSource> {
        Some(SessionIdSource::AgentCwdTranscript)
    }

    fn session_dir_for_cwd(&self, cwd: &Path) -> Option<PathBuf> {
        transcript_dir(cwd)
    }

    fn session_id_from_file_name(&self, name: &OsStr) -> Option<CString> {
        session_id_from_transcript_name(name)
    }

    fn session_env_stamp(&self) -> Option<SessionEnvStamp> {
        Some(SessionEnvStamp {
            session_id: "CLAUDE_CODE_SESSION_ID",
            owner_pid: "CLAUDE_PID",
        })
    }

    fn session_file_for_id(&self, cwd: &Path, session_id: &str) -> Option<PathBuf> {
        is_uuid(session_id)
            .then(|| transcript_dir(cwd))?
            .map(|dir| dir.join(format!("{}.jsonl", session_id.to_ascii_lowercase())))
    }

    fn detect(&self, screen: &str, title: Option<&str>) -> Detection {
        self.rules.detect(Input {
            screen,
            title: title.unwrap_or_default(),
        })
    }
}

/// Claude Code records each session's transcript at
/// `~/.claude/projects/<slug>/<session-id>.jsonl`, where `<slug>` is the
/// project's working directory with every non-alphanumeric character replaced by
/// `-`. The active session is the most recently modified transcript in that
/// directory, so attribution reads the agent's cwd and lists this directory.
///
/// The directory is shared by every agent run from the same project, and Claude
/// Code does not create a transcript until its first turn completes, so "newest"
/// alone can name a neighbour's session during that window. Candidates are
/// therefore dated against the agent's start time before being considered.
pub(crate) fn transcript_dir(cwd: &Path) -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(
        Path::new(&home)
            .join(".claude/projects")
            .join(OsStr::from_bytes(project_slug(cwd).as_bytes())),
    )
}

/// Encode a working directory the way Claude Code names its project directory:
/// every character that is not an ASCII letter or digit becomes `-`.
fn project_slug(cwd: &Path) -> CString {
    let mut slug = Vec::new();
    for character in cwd.to_string_lossy().chars() {
        let character = if character.is_ascii_alphanumeric() {
            character
        } else {
            '-'
        };
        let mut encoded = [0; 4];
        slug.extend_from_slice(character.encode_utf8(&mut encoded).as_bytes());
    }
    CString::new(slug).expect("a project slug has no NUL")
}

/// A transcript file is `<session-id>.jsonl`; the stem is the session UUID.
fn session_id_from_transcript_name(name: &OsStr) -> Option<CString> {
    let stem = name.to_str()?.strip_suffix(".jsonl")?;
    is_uuid(stem).then(|| CString::new(stem.to_ascii_lowercase()).expect("a UUID has no NUL"))
}

fn claude_program_name(program: &OsStr) -> bool {
    let name = Path::new(program)
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or_default()
        .trim_start_matches('.')
        .to_ascii_lowercase();
    matches!(
        name.as_str(),
        "claude" | "claude-code" | "claude.exe" | "claude.cmd" | "claude.js" | "claude.mjs"
    )
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;
    use std::path::Path;

    use super::super::manifest::Rules;
    use super::super::{AgentDetector, AgentState, detect_attributed};
    use super::{
        ClaudeDetector, Detection, RULES, claude_program_name, project_slug,
        session_id_from_transcript_name, transcript_dir,
    };

    /// The state the rules alone assign, before the idle fallback.
    fn rules_state(screen: &str, title: Option<&str>) -> Detection {
        let detector = ClaudeDetector {
            rules: Rules::bundled(&RULES),
        };
        detector.detect(screen, title)
    }

    /// Classify `screen` as a pane attributed to this agent.
    fn detect(screen: &str, title: Option<&str>) -> Detection {
        let detector = ClaudeDetector {
            rules: Rules::bundled(&RULES),
        };
        detect_attributed(&detector, screen, title)
    }

    #[test]
    fn title_spinner_reports_working() {
        let screen = "───────\n❯ ";
        for title in ["◐ Claude", "◑ Claude", "◒ Claude", "◓ Claude", "⠹ Claude"] {
            assert_eq!(
                detect(screen, Some(title)),
                Detection::State(AgentState::Working),
                "{title}"
            );
        }
    }

    #[test]
    fn a_spinner_glyph_alone_is_not_a_working_title() {
        let screen = "───────\n❯ ";
        assert_eq!(
            detect(screen, Some("◐Claude")),
            Detection::State(AgentState::Idle)
        );
    }

    #[test]
    fn live_prompt_box_reports_idle() {
        let screen = "some earlier output\n─────────────\n❯ ";
        assert_eq!(detect(screen, None), Detection::State(AgentState::Idle));
    }

    #[test]
    fn live_status_line_reports_working_under_a_resting_title() {
        let screen = "\
  ⎿  Running…
· Leavening… (1m 23s · ↓ 14.7k tokens)
  ⎿  Tip: Use /btw to ask a quick side question
─────────────
❯
─────────────
  h1  h1  Opus 5 (1M context)  ctx 93%";
        for title in [None, Some("✳ agentmon window 3 stopped status")] {
            assert_eq!(
                detect(screen, title),
                Detection::State(AgentState::Working),
                "{title:?}"
            );
        }
    }

    #[test]
    fn a_finished_turn_line_is_not_a_working_status_line() {
        let screen = "\
✻ Brewed for 11m 18s · done 9:06 AM
─────────────
❯
─────────────
  h1  h1  Opus 5 (1M context)  ctx 93%";
        assert_eq!(
            detect(screen, Some("✳ input_ground_timer failure")),
            Detection::State(AgentState::Idle)
        );
    }

    #[test]
    fn transcript_content_is_not_a_working_status_line() {
        for line in [
            "● Bash(cd /home/blmarket/proj/h2 && sed -n '51,90p' src/tests/test…)",
            "  Searching for 1 pattern… (ctrl+o to expand)",
            "     … +70 lines (ctrl+o to expand)",
        ] {
            let screen = format!("{line}\n─────────────\n❯ ");
            assert_eq!(
                detect(&screen, None),
                Detection::State(AgentState::Idle),
                "{line}"
            );
        }
    }

    #[test]
    fn scrollback_quoting_a_prompt_hint_does_not_mask_the_state() {
        let quoted = "\
  166 -        && !lower.contains(\"enter to select\")
  167 -        && !lower.contains(\"esc to cancel\")
  168 -        && !lower.contains(\"arrow keys to navigate\")";
        let working = format!("{quoted}\n· Leavening… (41s · ↓ 14.7k tokens)\n─────────────\n❯ ");
        assert_eq!(
            detect(&working, None),
            Detection::State(AgentState::Working)
        );
        let idle = format!("{quoted}\n✻ Brewed for 11m 18s · done 9:06 AM\n─────────────\n❯ ");
        assert_eq!(detect(&idle, None), Detection::State(AgentState::Idle));
    }

    #[test]
    fn running_background_subagents_report_working_at_the_prompt() {
        let screen = "\
✻ Waiting for 6 background agents to finish
─────────────
❯
─────────────
  /home/blmarket/proj/tiri  master  Opus 5.5  ctx 86%
  ⏵⏵ bypass permissions on · 1 shell · ← for agents
  ● main
  ◯ general-purpose  Reading e2e_status rende…  3m 42s · ↓ 93.1k tokens
  ◯ general-purpose  Adding strip_cols to inp… 3m 27s · ↓ 115.5k tokens
  ↓ 1 more";
        assert_eq!(
            detect(screen, Some("✳ Make test failures")),
            Detection::State(AgentState::Working)
        );
    }

    #[test]
    fn a_running_dynamic_workflow_reports_working_at_the_prompt() {
        let screen = "\
✻ Waiting for 1 dynamic workflow to finish
─────────────────────────────────── ultracode ─
❯
─────────────
  /home/blmarket/proj/h1/tiri  h1  Opus 5.5  ctx 87%
  ⏵⏵ bypass permissions on (shift+tab to cycle) · ← for agents

  ◯ cmdq-future-series  ▰▰▰▰▰▰▰▰▰▰▰▰▱▱▱▱▱▱▱▱  5/8 · 55m58s · ↓ 1.5m tokens";
        assert_eq!(
            detect(screen, Some("✳ Plan.md subagent commits")),
            Detection::State(AgentState::Working)
        );
    }

    #[test]
    fn a_footer_without_subagent_rows_stays_idle() {
        let screen = "\
✻ Waiting for 6 background agents to finish
─────────────
❯
─────────────
  /home/blmarket/proj/tiri  master  Opus 5.5  ctx 86%
  ⏵⏵ bypass permissions on · ← for agents
  ● main
                               new task? /clear to save 120.6k tokens";
        assert_eq!(
            detect(screen, Some("✳ Make test failures")),
            Detection::State(AgentState::Idle)
        );
    }

    #[test]
    fn a_permission_prompt_outranks_a_stale_status_line() {
        let screen = "\
✢ Slithering… (2m 12s · ↓ 8.6k tokens)
Edit file src/main.rs?
──────────────────────
❯ 1. Yes
  2. No
Do you want to proceed? · Esc to cancel";
        assert_eq!(detect(screen, None), Detection::State(AgentState::Blocked));
    }

    #[test]
    fn running_mcp_tasks_report_working_at_the_prompt() {
        let screen = "\
✻ Crunched for 2m 5s · 1 MCP task still running
──────────────────────
❯
──────────────────────
  ? for shortcuts";
        assert_eq!(
            detect(screen, Some("✳ Sync issues")),
            Detection::State(AgentState::Working)
        );
    }

    #[test]
    fn a_wrapped_mcp_task_summary_reports_working() {
        let screen = "\
✻ Crunched for 12m 30s · 2 MCP
  tasks still running
──────────────────────
❯ ";
        assert_eq!(detect(screen, None), Detection::State(AgentState::Working));
    }

    #[test]
    fn quoted_or_finished_mcp_task_summaries_stay_idle() {
        for summary in [
            "  ✻ Crunched for 2m 5s · 1 MCP task still running",
            "✻ Crunched for 2m 5s · 0 MCP tasks still running",
            "· 1 MCP task still running",
        ] {
            let screen = format!("{summary}\n──────────────────────\n❯ ");
            assert_eq!(
                detect(&screen, None),
                Detection::State(AgentState::Idle),
                "{summary}"
            );
        }
    }

    #[test]
    fn a_permission_prompt_outranks_running_mcp_tasks() {
        let screen = "\
✻ Crunched for 2m 5s · 1 MCP task still running
Edit file src/main.rs?
──────────────────────
❯ 1. Yes
  2. No
Do you want to proceed? · Esc to cancel";
        assert_eq!(detect(screen, None), Detection::State(AgentState::Blocked));
    }

    #[test]
    fn mcp_elicitation_reports_blocked() {
        for header in [
            "MCP server \"github\" requests your input",
            "MCP server “github” requests your input",
        ] {
            let screen = format!(
                "\
{header}
Repository to search
❯ Accept
  Decline
Esc to cancel"
            );
            assert_eq!(
                detect(&screen, None),
                Detection::State(AgentState::Blocked),
                "{header}"
            );
        }
    }

    #[test]
    fn a_quoted_mcp_elicitation_header_stays_idle() {
        let screen = "\
MCP server \"github\" requests your input
──────────────────────
❯ ";
        assert_eq!(detect(screen, None), Detection::State(AgentState::Idle));
    }

    #[test]
    fn confirmation_form_reports_blocked() {
        let screen = "\
Name the new branch
──────────────────────
❯ feature/rules
Enter to confirm · Esc to cancel";
        assert_eq!(detect(screen, None), Detection::State(AgentState::Blocked));
    }

    #[test]
    fn selection_form_reports_blocked() {
        let screen = "\
Do you want to make this edit?
─────────────────────────
❯ 1. Yes
  2. No
Enter to select · Esc to cancel · ↑/↓ to navigate";
        assert_eq!(detect(screen, None), Detection::State(AgentState::Blocked));
    }

    #[test]
    fn bash_permission_prompt_reports_blocked() {
        let screen = "\
Bash command
  rm -rf build/
Do you want to proceed?
❯ 1. Yes
  2. No";
        assert_eq!(detect(screen, None), Detection::State(AgentState::Blocked));
    }

    #[test]
    fn generic_permission_prompt_reports_blocked() {
        let screen = "\
Edit file src/main.rs?
──────────────────────
❯ 1. Yes
  2. No
Do you want to proceed? · Esc to cancel";
        assert_eq!(detect(screen, None), Detection::State(AgentState::Blocked));
    }

    #[test]
    fn transcript_viewer_preserves_previous_state() {
        let screen = "Showing detailed transcript · Ctrl+O to toggle";
        assert_eq!(detect(screen, None), Detection::KeepPrevious);
    }

    #[test]
    fn model_picker_is_transient() {
        let screen = "Select model\n  Sonnet\n  Opus\nEnter to set as default · Esc to cancel";
        assert_eq!(detect(screen, None), Detection::KeepPrevious);
    }

    #[test]
    fn resting_title_is_last_resort_idle() {
        assert_eq!(
            rules_state("", Some("✳ my-project")),
            Detection::State(AgentState::Idle)
        );
        assert_eq!(rules_state("", None), Detection::State(AgentState::Unknown));
    }

    #[test]
    fn recognizes_direct_and_wrapped_claude_programs() {
        assert!(claude_program_name(OsStr::new("claude")));
        assert!(claude_program_name(OsStr::new(
            "/usr/local/bin/claude-code"
        )));
        assert!(claude_program_name(OsStr::new(".claude.js")));
        assert!(!claude_program_name(OsStr::new("claude-helper")));
    }

    #[test]
    fn project_slug_replaces_non_alphanumeric_with_dash() {
        assert_eq!(
            project_slug(Path::new("/home/hun/srv/tiri")),
            c"-home-hun-srv-tiri"
        );
        assert_eq!(
            project_slug(Path::new("/home/hun/.claude")),
            c"-home-hun--claude"
        );
        assert_eq!(project_slug(Path::new("/home/hun/AAI")), c"-home-hun-AAI");
    }

    #[test]
    fn extracts_session_id_from_transcript_name() {
        assert_eq!(
            session_id_from_transcript_name(OsStr::new(
                "713EE853-A358-4DC4-A306-90F1F2F4070D.jsonl"
            ))
            .as_deref(),
            Some(c"713ee853-a358-4dc4-a306-90f1f2f4070d")
        );
        assert_eq!(
            session_id_from_transcript_name(OsStr::new("not-a-uuid.jsonl")),
            None
        );
        assert_eq!(
            session_id_from_transcript_name(OsStr::new(
                "713ee853-a358-4dc4-a306-90f1f2f4070d.json"
            )),
            None
        );
    }

    #[test]
    fn transcript_dir_is_under_the_home_projects_directory() {
        let home = std::env::var_os("HOME").expect("HOME set in test environment");
        assert_eq!(
            transcript_dir(Path::new("/proj/app")),
            Some(Path::new(&home).join(".claude/projects/-proj-app"))
        );
    }
}
