use std::ffi::{CString, OsStr, OsString};
use std::path::Path;

use super::manifest::{Input, Rules};
use super::{AgentDetector, AgentState, Detection, SessionIdSource, is_uuid};

pub(crate) struct JetskiDetector {
    pub(crate) rules: Rules,
}

impl AgentDetector for JetskiDetector {
    fn label(&self) -> &'static str {
        "jetski"
    }

    fn matches_program(&self, program: &OsStr) -> bool {
        jetski_program_name(program)
    }

    fn invocation_state(&self, arguments: &[OsString]) -> Option<AgentState> {
        jetski_headless_invocation(arguments).then_some(AgentState::Working)
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

fn session_id_from_conversation_path(path: &Path) -> Option<CString> {
    let name = path.file_name()?.to_str()?;
    let stem = ["-shm", "-wal", ""]
        .iter()
        .find_map(|suffix| name.strip_suffix(suffix)?.strip_suffix(".db"))?;
    let conversations = path.parent()?;
    if conversations.file_name()? != "conversations"
        || conversations.parent()?.file_name()? != "jetski"
    {
        return None;
    }
    is_uuid(stem).then(|| CString::new(stem.to_ascii_lowercase()).expect("a UUID has no NUL"))
}

fn jetski_program_name(program: &OsStr) -> bool {
    let name = Path::new(program)
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or_default()
        .trim_start_matches('.')
        .to_ascii_lowercase();
    if matches!(name.as_str(), "jetski" | "jetski.exe") {
        return true;
    }
    if matches!(name.as_str(), "cli" | "cli_internal") {
        let path = program.to_string_lossy().replace('\\', "/").to_lowercase();
        return path.contains("/jetski/") || path.contains("/jetski-devs/");
    }
    false
}

fn jetski_headless_invocation(arguments: &[OsString]) -> bool {
    arguments
        .iter()
        .skip(1)
        .any(|argument| matches!(argument.to_str(), Some("-p" | "--print" | "--prompt")))
}

#[cfg(test)]
mod tests {
    use std::ffi::{OsStr, OsString};
    use std::path::Path;

    use super::super::agy::RULES;
    use super::super::detect_attributed;
    use super::super::manifest::Rules;
    use super::{
        AgentState, Detection, JetskiDetector, jetski_headless_invocation, jetski_program_name,
        session_id_from_conversation_path,
    };

    fn detect(screen: &str) -> Detection {
        let detector = JetskiDetector {
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
        let text = screen(&format!(
            "● Bash(git status) (ctrl+o to expand)\n⣷  Generating...\n{RULE}\n>\n{RULE}\n\
             esc to cancel                      Gemini 3.6 Flash · high"
        ));
        assert_eq!(detect(&text), Detection::State(AgentState::Working));
    }

    #[test]
    fn permission_dialog_reports_blocked() {
        let text = screen(
            "⣷  Generating...\n● Bash(git status) (ctrl+o to expand)\nRequesting permission for:\n\
             git status\nDo you want to proceed?\n1. Yes\n4. No\n\
             ↑/↓ Navigate · tab Amend · ctrl+g edit/expand command",
        );
        assert_eq!(detect(&text), Detection::State(AgentState::Blocked));
    }

    #[test]
    fn recognizes_jetski_programs() {
        assert!(jetski_program_name(OsStr::new("jetski")));
        assert!(jetski_program_name(OsStr::new("JETSKI.EXE")));
        assert!(jetski_program_name(OsStr::new(
            "/google/bin/releases/jetski-devs/tools/cli"
        )));
        assert!(jetski_program_name(OsStr::new(
            "/var/tmp/sar.cli_internal.123/cli_internal_impl.runfiles/google3/third_party/jetski/cmd/cli/cli"
        )));
        assert!(!jetski_program_name(OsStr::new("agy")));
        assert!(!jetski_program_name(OsStr::new("cli")));
        assert!(!jetski_program_name(OsStr::new(
            "/usr/local/bin/other/cli"
        )));
    }

    #[test]
    fn recognizes_headless_invocations() {
        let args = |values: &[&str]| values.iter().map(OsString::from).collect::<Vec<_>>();

        assert!(jetski_headless_invocation(&args(&[
            "jetski", "-p", "do work"
        ])));
        assert!(jetski_headless_invocation(&args(&[
            "jetski", "--print", "do work"
        ])));
        assert!(jetski_headless_invocation(&args(&[
            "jetski", "--prompt", "do work"
        ])));
        assert!(!jetski_headless_invocation(&args(&[
            "jetski",
            "--prompt-interactive",
            "do work"
        ])));
        assert!(!jetski_headless_invocation(&args(&["jetski"])));
    }

    #[test]
    fn extracts_session_id_from_open_conversation_database() {
        let base = "/home/me/.gemini/jetski/conversations";
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
                "/home/me/.gemini/antigravity-cli/conversations/019f9757-7c53-7898-a7d1-c8c780212888.db"
            )),
            None
        );
    }
}
