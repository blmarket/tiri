//! OpenCode agent detector.
//!
//! State comes from herdr's `opencode.toml` rules under tiri's overlay (see
//! [`manifest`](super::manifest)); this module identifies the process and its
//! headless invocations.

use std::ffi::{OsStr, OsString};

use super::manifest::{Bundle, Input, Rules};
use super::{AgentDetector, AgentState, Detection, SessionIdSource};

pub(crate) const RULES: Bundle = Bundle {
    agent: "opencode",
    herdr: include_str!("../../manifests/herdr/opencode.toml"),
    tiri: include_str!("../../manifests/tiri/opencode.toml"),
};

/// Recognizes OpenCode panes.
pub(crate) struct OpencodeDetector {
    pub(crate) rules: Rules,
}

impl AgentDetector for OpencodeDetector {
    fn label(&self) -> &'static str {
        "opencode"
    }

    fn matches_program(&self, program: &OsStr) -> bool {
        opencode_program_name(program)
    }

    fn invocation_state(&self, arguments: &[OsString]) -> Option<AgentState> {
        opencode_headless_invocation(arguments).then_some(AgentState::Working)
    }

    fn session_id_source(&self) -> Option<SessionIdSource> {
        None
    }

    fn detect(&self, screen: &str, title: Option<&str>) -> Detection {
        self.rules.detect(Input {
            screen,
            title: title.unwrap_or_default(),
        })
    }
}

fn opencode_program_name(program: &OsStr) -> bool {
    let name = program
        .to_string_lossy()
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default()
        .trim_start_matches('.')
        .to_ascii_lowercase();
    matches!(
        name.as_str(),
        "opencode"
            | "opencode.exe"
            | "opencode.cmd"
            | "opencode.js"
            | "opencode.mjs"
            | "opencode-wrapped"
            | "opencode-wrapp"
    )
}

fn opencode_headless_invocation(arguments: &[OsString]) -> bool {
    let mut takes_value = false;
    for argument in arguments.iter().skip(1) {
        let Some(argument) = argument.to_str() else {
            continue;
        };

        if takes_value {
            takes_value = false;
            continue;
        }

        if argument == "--" {
            return false;
        }

        if argument.starts_with('-') {
            takes_value = matches!(
                argument,
                "--agent"
                    | "-a"
                    | "--attach"
                    | "--config"
                    | "--dir"
                    | "--format"
                    | "--hostname"
                    | "--log-level"
                    | "--model"
                    | "-m"
                    | "--port"
                    | "--prompt"
                    | "--session"
                    | "-s"
            );
            continue;
        }

        return argument == "run";
    }
    false
}

#[cfg(test)]
mod tests {
    use std::ffi::{OsStr, OsString};

    use super::super::detect_attributed;
    use super::super::manifest::Rules;
    use super::{
        AgentState, Detection, OpencodeDetector, RULES, opencode_headless_invocation,
        opencode_program_name,
    };

    /// Classify `screen` as a pane attributed to this agent.
    fn detect(screen: &str) -> Detection {
        let detector = OpencodeDetector {
            rules: Rules::bundled(&RULES),
        };
        detect_attributed(&detector, screen, None)
    }

    #[test]
    fn recognizes_direct_and_wrapped_opencode_programs() {
        assert!(opencode_program_name(OsStr::new("opencode")));
        assert!(opencode_program_name(OsStr::new(
            "/run/current-system/sw/bin/opencode"
        )));
        assert!(opencode_program_name(OsStr::new(".opencode-wrapp")));
        assert!(opencode_program_name(OsStr::new(
            r"C:\\Users\\me\\opencode.cmd"
        )));
        assert!(!opencode_program_name(OsStr::new("opencoder")));
        assert!(!opencode_program_name(OsStr::new("opencode2")));
    }

    #[test]
    fn recognizes_the_noninteractive_run_command() {
        assert!(opencode_headless_invocation(&[
            OsString::from("opencode"),
            OsString::from("--yolo"),
            OsString::from("run"),
            OsString::from("fix the bug"),
        ]));
        assert!(!opencode_headless_invocation(&[
            OsString::from("opencode"),
            OsString::from("--prompt"),
            OsString::from("run"),
        ]));
        assert!(!opencode_headless_invocation(&[
            OsString::from("opencode"),
            OsString::from("--yolo"),
        ]));
    }

    #[test]
    fn permission_prompt_reports_blocked() {
        assert_eq!(
            detect("△ Permission required\nShell command\nenter once  esc dismiss"),
            Detection::State(AgentState::Blocked)
        );
        assert_eq!(
            detect("↑↓ select   enter submit   esc dismiss"),
            Detection::State(AgentState::Blocked)
        );
        // Transcript text quoting the prompt is not the prompt.
        assert_eq!(
            detect("old output: permission required"),
            Detection::State(AgentState::Idle)
        );
    }

    #[test]
    fn interrupt_hints_report_working() {
        for screen in [
            "esc to interrupt",
            "ctrl+c to interrupt",
            "OpenCode · esc interrupt",
            "esc again to interrupt",
        ] {
            assert_eq!(
                detect(screen),
                Detection::State(AgentState::Working),
                "screen: {screen}"
            );
        }
    }

    #[test]
    fn progress_bar_reports_working() {
        assert_eq!(
            detect("building ■⬝■■ now"),
            Detection::State(AgentState::Working)
        );
    }

    #[test]
    fn footer_reports_idle() {
        let screen = "done\nctrl+p commands  • OpenCode 1.18.23";
        assert_eq!(detect(screen), Detection::State(AgentState::Idle));
    }

    #[test]
    fn a_screen_no_rule_explains_is_the_idle_fallback() {
        assert_eq!(
            detect("OpenCode is ready"),
            Detection::State(AgentState::Idle)
        );
    }
}
