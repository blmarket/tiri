//! What a pane is doing, as the compact `#{pane_state_emoji}` label reports it.
//!
//! This is the non-agent half of that variable, kept beside the agent
//! detectors because the two halves share the glyph contract: every label is a
//! single codepoint two columns wide. A host builds a [`PaneProcessProbe`]
//! from its own pty descriptor and asks [`PaneClass::classify`] what to draw.

use std::ffi::{CStr, CString};
use std::os::unix::ffi::OsStrExt;

use crate::platform::{CurrentPlatform, Platform};

/// A pane's foreground-process identity, captured while the pane was at hand.
///
/// Holds only owned data (pids and the spawn command line), so a deferred
/// format callback can resolve the process-derived variables long after the
/// pane borrow ended. A pid whose process has exited simply fails its
/// `/proc` read and falls through, exactly as the live path always has.
#[derive(Clone)]
pub struct PaneProcessProbe {
    foreground: Option<libc::pid_t>,
    session_leader: Option<libc::pid_t>,
    fallback_command: Option<CString>,
}

impl PaneProcessProbe {
    /// Build a probe from what the host already knows about the pane's pty:
    /// the foreground process group and session leader it reports, and the
    /// command line the pane was spawned with as the fallback for a group
    /// whose leader has exited.
    pub fn new(
        foreground: Option<libc::pid_t>,
        session_leader: Option<libc::pid_t>,
        fallback_command: Option<CString>,
    ) -> Self {
        PaneProcessProbe {
            foreground,
            session_leader,
            fallback_command,
        }
    }

    /// The working directory of the pane's foreground process group
    /// (`#{pane_current_path}`).
    ///
    /// Mirrors tmux's `osdep_get_cwd`: prefer the foreground group, then fall
    /// back to the session leader — the group id is only a pid while the
    /// group's leader lives, and a shell pipeline whose first member exited
    /// leaves a group that names no process.
    pub fn current_path(&self) -> Option<CString> {
        [self.foreground, self.session_leader]
            .into_iter()
            .flatten()
            .find_map(|pid| CurrentPlatform::process_cwd(pid as u32))
            .and_then(|path| CString::new(path.as_os_str().as_bytes()).ok())
    }

    /// The program occupying the pane's foreground process group
    /// (`#{pane_current_command}`).
    ///
    /// Mirrors tmux's `format_cb_current_command`, which tries the foreground
    /// group's `argv[0]` and then falls back to the pane's own command line —
    /// the fallback is what answers for a leaderless group.
    pub fn current_command(&self) -> Option<CString> {
        let foreground = self
            .foreground
            .and_then(|pid| {
                CurrentPlatform::process_arguments(pid as u32)
                    .into_iter()
                    .next()
                    .or_else(|| {
                        CurrentPlatform::process_programs(pid as u32)
                            .into_iter()
                            .next()
                    })
            })
            .and_then(|program| CString::new(program.as_bytes()).ok());

        foreground
            .filter(|program| !program.is_empty())
            .or_else(|| self.fallback_command.clone())
            .map(|command| parse_window_name(&command))
            .filter(|name| !name.to_bytes().is_empty())
    }

    /// Whether the shell holding the terminal is sitting at a prompt with
    /// nothing running in front of it.
    ///
    /// The program holding the terminal is the pane's foreground group, which
    /// is the pane's own shell until something it launched takes over — and a
    /// shell started from that shell is a shell at a prompt just as much as the
    /// pane's own is, so what the test asks is what the foreground program is
    /// rather than whether it is the process the pane forked.
    ///
    /// The program has to be a shell, and a shell handed work of its own
    /// (`sh -c 'while :; do :; done'`, or a script to run) is working rather
    /// than prompting. A pane launched straight into a program
    /// (`new-window -- tail -f log`) is not a shell at all, so it never gets
    /// here.
    ///
    /// What this cannot do is ask the shell whether it is currently at its
    /// prompt: bash and dash sit in a `poll` loop there, which is
    /// indistinguishable from any other wait. Reading the invocation is what
    /// is left, and it is right for the panes that matter — an interactive
    /// shell has no work on its command line, and a shell given some never
    /// returns to a prompt.
    fn shell_at_prompt(&self) -> bool {
        self.current_command()
            .is_some_and(|command| is_shell(&command))
            && !self.runs_a_command_string()
    }

    /// Whether the foreground process was invoked with work of its own — `-c`,
    /// or a script to run — rather than started interactively.
    ///
    /// An unreadable argument vector reads as no work, leaving the shell to be
    /// treated as interactive — which is what a pane's shell usually is.
    fn runs_a_command_string(&self) -> bool {
        let Some(pid) = self.foreground else {
            return false;
        };
        let arguments = CurrentPlatform::process_arguments(pid as u32);
        invoked_with_command_string(arguments.iter().filter_map(|argument| argument.to_str()))
    }

    /// Whether the program holding the terminal is a remote login client.
    ///
    /// Such a pane is a window onto another machine: the local client sits in
    /// the same network wait whether the remote side is at a prompt or busy, so
    /// none of the local classes would say anything true about it.
    fn remote_session(&self) -> bool {
        self.current_command()
            .is_some_and(|command| is_remote_client(&command))
    }

    /// Whether the foreground command is parked in a terminal read.
    ///
    /// The foreground process *group* id is a pid only while the group's
    /// leader lives, so this answers for the leader and reads a leaderless
    /// group — a pipeline outliving its first member — as not waiting. Which
    /// is the conservative direction: it never claims a pane wants you.
    fn waiting_for_tty(&self) -> bool {
        self.foreground
            .and_then(|pid| CurrentPlatform::process_waiting_for_tty(pid as u32))
            .unwrap_or(false)
    }
}

/// Whether a shell's argument vector carries work of its own: a `-c` command
/// string, or a script operand.
///
/// Only the leading option arguments are scanned for `-c`: everything from the
/// first non-option onwards is the command string and its own arguments, which
/// may contain anything at all. `--` ends the options, and whatever follows it
/// is a script to run.
///
/// Within the options, only single-dash arguments are short-option bundles, so
/// `-lc` counts while a long option merely spelled with a `c` in it — `--norc`
/// — does not.
fn invoked_with_command_string<'a>(arguments: impl Iterator<Item = &'a str>) -> bool {
    let mut rest = arguments.skip(1).peekable();
    let mut options = Vec::new();
    while let Some(argument) = rest.peek() {
        if !argument.starts_with('-') {
            break;
        }
        let argument = rest.next().expect("the peeked option");
        if argument == "--" {
            break;
        }
        options.push(argument);
    }
    options
        .into_iter()
        .filter(|argument| !argument.starts_with("--"))
        .any(|argument| argument.contains('c'))
        || rest.next().is_some()
}

/// Whether a program name — as [`parse_window_name`] reduces it, so already
/// stripped of its path and of the leading `-` a login shell carries — is an
/// interactive shell.
///
/// Used only to decide whether a pane holding its own terminal is at a prompt.
/// An unlisted shell reads as a foreground program instead, which is the same
/// answer the pane would give while running anything else.
fn is_shell(command: &CStr) -> bool {
    const SHELLS: [&CStr; 13] = [
        c"sh", c"bash", c"zsh", c"fish", c"dash", c"ash", c"ksh", c"mksh", c"csh", c"tcsh",
        c"elvish", c"nu", c"xonsh",
    ];
    SHELLS.contains(&command)
}

/// Whether a program name — reduced by [`parse_window_name`] — is a client for
/// a shell on another machine.
///
/// `mosh` is a script that execs `mosh-client`, so the pane normally shows the
/// latter; both are listed for the moment in between.
fn is_remote_client(command: &CStr) -> bool {
    const CLIENTS: [&CStr; 7] = [
        c"ssh",
        c"autossh",
        c"mosh",
        c"mosh-client",
        c"et",
        c"telnet",
        c"rlogin",
    ];
    CLIENTS.contains(&command)
}

/// What a pane is doing, as the compact `#{pane_state_emoji}` label reports it.
///
/// This is the non-agent half of that variable: a pane running a recognized
/// agent reports the agent's lifecycle state instead. Every other pane lands in
/// exactly one of these, which is what keeps the variable from ever being
/// empty.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PaneClass {
    /// No live child: reaped with `remain-on-exit` holding the pane open, or
    /// never started.
    Dead,
    /// The terminal is held by a remote login client (`ssh`, `mosh`, ...),
    /// whose remote state cannot be seen from here.
    Remote,
    /// The pane is on its alternate screen, which is what a full-screen
    /// application switches to and an ordinary command line never does.
    Tui,
    /// The pane's shell is at its prompt, waiting for you.
    ShellPrompt,
    /// A foreground command has stopped to read from the terminal, waiting for
    /// you.
    WaitingForTty,
    /// A foreground command is working.
    Running,
}

impl PaneClass {
    /// Classify a pane from what it is running.
    ///
    /// Order matters. A dead pane is dead whatever its last screen was. A
    /// remote session comes next, since whatever screen it shows belongs to
    /// the other machine. The alternate screen then outranks the shell/command split, because a pane
    /// launched straight into a full-screen application (`new-window htop`)
    /// has that application as *both* its session leader and its foreground
    /// group, and would otherwise read as a shell sitting at a prompt.
    pub fn classify(probe: Option<&PaneProcessProbe>, alternate_on: bool, dead: bool) -> Self {
        let Some(probe) = probe.filter(|_| !dead) else {
            return PaneClass::Dead;
        };
        if probe.remote_session() {
            PaneClass::Remote
        } else if alternate_on {
            PaneClass::Tui
        } else if probe.shell_at_prompt() {
            PaneClass::ShellPrompt
        } else if probe.waiting_for_tty() {
            PaneClass::WaitingForTty
        } else {
            PaneClass::Running
        }
    }

    /// The status-bar glyph for this class.
    ///
    /// Every glyph here — and every one [`AgentState::emoji`] returns — is a
    /// single codepoint two columns wide. That is a hard requirement, not a
    /// preference: the status renderer measures per codepoint, so a glyph
    /// needing U+FE0F to reach emoji presentation would be counted as one
    /// column while the terminal drew two, and the whole status line would
    /// drift. Check any replacement against `codepoint_width` first.
    ///
    /// [`AgentState::emoji`]: crate::integration::AgentState::emoji
    pub fn emoji(self) -> &'static str {
        match self {
            PaneClass::Dead => "🛑",
            PaneClass::Remote => "📡",
            PaneClass::Tui => "🪟",
            PaneClass::ShellPrompt => "💲",
            PaneClass::WaitingForTty => "⌛",
            PaneClass::Running => "🔧",
        }
    }
}

/// Render a pane's argument vector the way tmux's `cmd_stringify_argv` does,
/// for the callers that reduce the result with [`parse_window_name`].
///
/// tmux escapes each argument; that step is skipped because it cannot change
/// what the caller sees. `parse_window_name` cuts at the first space, and it
/// does so after resolving quotes, so both a quoted and an unquoted argument
/// reduce to the same leading word either way.
pub fn stringify_argv<S: AsRef<CStr>>(argv: &[S]) -> CString {
    let mut out = Vec::new();
    for argument in argv {
        if !out.is_empty() {
            out.push(b' ');
        }
        out.extend_from_slice(argument.as_ref().to_bytes());
    }
    CString::new(out).expect("arguments have no interior NUL")
}

/// Reduce a command line to the program name tmux displays for it, following
/// tmux's `parse_window_name`: take the first quoted or whitespace-delimited
/// word, drop an `exec` prefix and any leading dashes (a login shell's `-bash`
/// is reported as `bash`), and keep only the last component of an absolute
/// path.
///
/// tmux additionally runs the result through `clean_name`, which escapes
/// non-printable bytes for display. That step is not reproduced: every name
/// reaching this function is a program name, and the trailing-byte trim below
/// already removes the control characters `clean_name` would have escaped.
pub fn parse_window_name(input: &CStr) -> CString {
    let input = input.to_bytes();
    let mut name = input.strip_prefix(b"\"").unwrap_or(input);
    if let Some(quote) = name.iter().position(|&byte| byte == b'"') {
        name = &name[..quote];
    }
    name = name.strip_prefix(b"exec ").unwrap_or(name);
    while matches!(name.first(), Some(b' ' | b'-')) {
        name = &name[1..];
    }
    if let Some(space) = name.iter().position(|&byte| byte == b' ') {
        name = &name[..space];
    }
    let trimmed = name
        .iter()
        .rposition(|byte| byte.is_ascii_graphic())
        .map_or(&[][..], |last| &name[..=last]);
    if trimmed.starts_with(b"/") {
        name = trimmed
            .rsplit(|&byte| byte == b'/')
            .next()
            .unwrap_or(trimmed);
    } else {
        name = trimmed;
    }
    CString::new(name).expect("a command name has no interior NUL")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A pane with no live child has no foreground group to compare, so the
    /// classes that describe a running process cannot apply to it.
    #[test]
    fn a_pane_with_no_process_is_dead_whatever_its_screen_says() {
        assert_eq!(PaneClass::classify(None, false, false), PaneClass::Dead);
        assert_eq!(PaneClass::classify(None, true, false), PaneClass::Dead);
    }

    #[test]
    fn a_probed_pane_is_classified_by_what_holds_its_terminal() {
        let probe = |foreground, session_leader, command: &str| PaneProcessProbe {
            foreground,
            session_leader,
            fallback_command: Some(CString::new(command).expect("command")),
        };

        assert_eq!(
            PaneClass::classify(Some(&probe(Some(0), Some(0), "bash")), false, false),
            PaneClass::ShellPrompt
        );
        assert_eq!(
            PaneClass::classify(Some(&probe(Some(0), Some(1), "zsh")), false, false),
            PaneClass::ShellPrompt
        );
        assert_eq!(
            PaneClass::classify(Some(&probe(Some(0), Some(1), "make")), false, false),
            PaneClass::Running
        );
        assert_eq!(
            PaneClass::classify(Some(&probe(Some(0), Some(0), "tail -f log")), false, false),
            PaneClass::Running
        );
        assert_eq!(
            PaneClass::classify(Some(&probe(Some(0), Some(0), "bash")), true, false),
            PaneClass::Tui
        );
        assert_eq!(
            PaneClass::classify(Some(&probe(Some(0), Some(1), "ssh sx")), false, false),
            PaneClass::Remote
        );
        assert_eq!(
            PaneClass::classify(
                Some(&probe(Some(0), Some(1), "/usr/bin/ssh sx")),
                true,
                false
            ),
            PaneClass::Remote
        );
        assert_eq!(
            PaneClass::classify(Some(&probe(Some(0), Some(1), "mosh-client")), false, false),
            PaneClass::Remote
        );
        assert_eq!(
            PaneClass::classify(Some(&probe(Some(0), Some(0), "bash")), true, true),
            PaneClass::Dead
        );
    }

    /// A shell handed work — a command string or a script — is working, not
    /// prompting, and the option scan has to stop before that work, which can
    /// hold anything.
    #[test]
    fn a_shell_given_work_is_not_at_a_prompt() {
        let invoked = |arguments: &[&str]| invoked_with_command_string(arguments.iter().copied());

        assert!(invoked(&["sh", "-c", "while :; do :; done"]));
        assert!(invoked(&["bash", "-lc", "make"]));
        assert!(!invoked(&["bash", "--norc", "-i"]));
        assert!(!invoked(&["-bash"]));
        assert!(!invoked(&["zsh"]));
        assert!(invoked(&["sh", "-i", "script.sh"]));
        assert!(invoked(&["sh", "--", "script.sh"]));
        assert!(!invoked(&["bash", "--noprofile", "--norc"]));
        assert!(invoked(&["bash", "--norc", "-c", "make"]));
    }

    #[test]
    fn a_login_shells_argv0_still_reads_as_a_shell() {
        assert!(is_shell(&parse_window_name(c"-bash")));
        assert!(is_shell(&parse_window_name(c"/usr/bin/zsh")));
        assert!(is_shell(&parse_window_name(c"fish")));
        assert!(!is_shell(&parse_window_name(c"/usr/bin/tail -f log")));
        assert!(!is_shell(&parse_window_name(c"htop")));
    }

    #[test]
    fn parse_window_name_reduces_a_command_line_to_its_program() {
        assert_eq!(parse_window_name(c"bash"), c"bash");
        assert_eq!(parse_window_name(c"/usr/bin/sleep"), c"sleep");
        assert_eq!(parse_window_name(c"sleep 30"), c"sleep");
        assert_eq!(parse_window_name(c"bin/sleep"), c"bin/sleep");
        assert_eq!(parse_window_name(c"-zsh"), c"zsh");
        assert_eq!(parse_window_name(c"exec vim file"), c"vim");
        assert_eq!(
            parse_window_name(cr#"bash -mc "echo x | sleep 30""#),
            c"bash"
        );
        assert_eq!(parse_window_name(cr#""my program" -x"#), c"my");
        assert_eq!(parse_window_name(c"sleep\r\n"), c"sleep");
        assert_eq!(parse_window_name(c""), c"");
    }

    #[test]
    fn stringify_argv_reduces_to_the_program_the_pane_was_given() {
        let argv = [c"bash", c"-mc", c"echo x | sleep 30"];
        assert_eq!(parse_window_name(&stringify_argv(&argv)), c"bash");
        let shell = [c"/bin/zsh"];
        assert_eq!(parse_window_name(&stringify_argv(&shell)), c"zsh");
    }
}
