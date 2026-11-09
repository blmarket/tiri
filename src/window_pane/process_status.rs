use super::{window_pane, window_pane_tree, WindowPane};
use crate::cmd::cmd_stringify_argv_cstring;
use crate::names::parse_window_name_cstring;
use crate::osdep::osdep_get_name_cstring;
use crate::reactor::Timer;
use std::cell::RefCell;
use std::os::fd::{AsFd, AsRawFd};
use std::time::{Duration, Instant};

const POLL_INTERVAL: Duration = Duration::from_millis(200);
const RECHECK_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Default)]
pub(super) struct ProcessStatus {
    key: Option<(libc::pid_t, libc::c_int, Option<libc::pid_t>, bool)>,
    checked: Option<Instant>,
    emoji: Option<&'static str>,
    shell_running: Option<bool>,
    report_foreground: Option<libc::pid_t>,
}

thread_local! {
    static TIMER: RefCell<Option<Timer>> = const { RefCell::new(None) };
}

pub(crate) fn start() {
    TIMER.set(Some(
        Timer::new(POLL_INTERVAL, tick).expect("process status timer"),
    ));
}

pub(crate) fn shutdown() {
    TIMER.set(None);
}

fn tick() {
    TIMER.set(None);
    for pane in window_pane_tree::snapshot() {
        let changed = {
            let mut pane = pane.borrow_mut();
            let reported = pane.process_status.borrow().report_foreground;
            let expired = pane.base.program_status.is_some()
                && reported.is_some()
                && foreground_group(&pane).is_some_and(|foreground| Some(foreground) != reported);
            if expired {
                pane.base.program_status = None;
                pane.process_status.borrow_mut().report_foreground = None;
            }
            refresh(&pane, expired) || expired
        };
        if changed {
            if let Some(window) = pane.window_observer().upgrade() {
                crate::server_fn::server_status_window(&window);
            }
        }
    }
    start();
}

fn foreground_group(pane: &window_pane) -> Option<libc::pid_t> {
    pane.fd.as_ref().and_then(|fd| {
        tiri_sys::terminal_foreground_group(fd.as_fd())
            .ok()
            .filter(|&pid| pid > 0)
    })
}

pub(super) fn program_status_event(pane: &window_pane) {
    pane.process_status.borrow_mut().report_foreground = pane
        .base
        .program_status
        .as_ref()
        .and_then(|_| foreground_group(pane));
    refresh(pane, true);
}

pub(super) fn shell_event(pane: &window_pane, running: Option<bool>) -> bool {
    if let Some(running) = running {
        pane.process_status.borrow_mut().shell_running = Some(running);
    }
    refresh(pane, true)
}

pub(super) fn emoji(pane: &window_pane) -> &'static str {
    if pane.fd.is_none() {
        return "🛑";
    }
    if pane.process_status.borrow().emoji.is_none() {
        refresh(pane, true);
    }
    pane.process_status.borrow().emoji.unwrap_or("🛑")
}

fn refresh(pane: &window_pane, force: bool) -> bool {
    if pane.base.program_status.is_some() {
        return false;
    }
    let now = Instant::now();
    let foreground = foreground_group(pane);
    let key = (
        pane.pid,
        pane.fd.as_ref().map_or(-1, |fd| fd.as_fd().as_raw_fd()),
        foreground,
        pane.base.saved_grid.is_some(),
    );
    {
        let cache = pane.process_status.borrow();
        if !force
            && cache.key == Some(key)
            && (pane.fd.is_none()
                || cache
                    .checked
                    .is_some_and(|checked| now.duration_since(checked) < RECHECK_INTERVAL))
        {
            return false;
        }
    }
    let emoji = classify(pane, foreground);
    let mut cache = pane.process_status.borrow_mut();
    let changed = cache.emoji != Some(emoji);
    cache.key = Some(key);
    cache.checked = Some(now);
    cache.emoji = Some(emoji);
    changed
}

fn classify(pane: &window_pane, foreground: Option<libc::pid_t>) -> &'static str {
    use tiri_sys::platform::{CurrentPlatform, Platform};

    let Some(fd) = pane.fd.as_ref() else {
        return "🛑";
    };
    let command = osdep_get_name_cstring(fd.as_fd())
        .or_else(|| cmd_stringify_argv_cstring(&pane.argv).filter(|command| !command.is_empty()))
        .or_else(|| pane.shell.clone())
        .map(|command| parse_window_name_cstring(&command));
    let command = command.as_deref().unwrap_or(c"");
    if [
        c"ssh",
        c"autossh",
        c"mosh",
        c"mosh-client",
        c"et",
        c"telnet",
        c"rlogin",
    ]
    .contains(&command)
    {
        return "📡";
    }
    if pane.base.saved_grid.is_some() {
        return "🪟";
    }
    if [
        c"sh", c"bash", c"zsh", c"fish", c"dash", c"ash", c"ksh", c"mksh", c"csh", c"tcsh",
        c"elvish", c"nu", c"xonsh",
    ]
    .contains(&command)
    {
        let arguments = foreground
            .map(|pid| CurrentPlatform::process_arguments(pid as u32))
            .unwrap_or_default();
        let mut arguments = arguments
            .iter()
            .filter_map(|arg| arg.to_str())
            .skip(1)
            .peekable();
        let mut command_string = false;
        while let Some(argument) = arguments.peek() {
            if !argument.starts_with('-') {
                break;
            }
            let argument = arguments.next().unwrap();
            if argument == "--" {
                break;
            }
            command_string |= !argument.starts_with("--") && argument.contains('c');
        }
        if !command_string && arguments.next().is_none() {
            return if pane.process_status.borrow().shell_running == Some(true) {
                "🔧"
            } else {
                "💲"
            };
        }
    }
    if foreground
        .and_then(|pid| CurrentPlatform::process_waiting_for_tty(pid as u32))
        .unwrap_or(false)
    {
        "⌛"
    } else {
        "🔧"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiri_vt::{ProgramState, ProgramStatus};

    #[test]
    fn unchanged_process_is_cached_until_recheck_and_only_emoji_changes_invalidate() {
        let owner = window_pane::new();
        let mut pane = owner.borrow_mut();
        pane.fd = Some(super::super::pty::PanePty::new(
            std::fs::File::open("/dev/null").unwrap().into(),
        ));
        pane.shell = Some(c"/bin/sh".to_owned());
        assert!(refresh(&pane, false));
        assert_eq!(emoji(&pane), "💲");
        let checked = pane.process_status.borrow().checked;
        pane.shell = Some(c"ssh".to_owned());
        assert!(!refresh(&pane, false));
        assert_eq!(pane.process_status.borrow().checked, checked);
        assert_eq!(emoji(&pane), "💲");
        pane.process_status.borrow_mut().checked = Some(Instant::now() - RECHECK_INTERVAL);
        assert!(refresh(&pane, false));
        assert_eq!(emoji(&pane), "📡");
        pane.process_status.borrow_mut().checked = Some(Instant::now() - RECHECK_INTERVAL);
        assert!(!refresh(&pane, false));
    }

    #[test]
    fn reports_skip_process_checks_and_shell_events_force_refresh() {
        let owner = window_pane::new();
        let mut pane = owner.borrow_mut();
        pane.fd = Some(super::super::pty::PanePty::new(
            std::fs::File::open("/dev/null").unwrap().into(),
        ));
        pane.shell = Some(c"/bin/sh".to_owned());
        pane.base.program_status = Some(ProgramStatus::new(ProgramState::Working, None));
        assert!(!refresh(&pane, false));
        assert!(pane.process_status.borrow().checked.is_none());
        pane.base.program_status = None;
        assert!(shell_event(&pane, Some(false)));
        assert_eq!(emoji(&pane), "💲");
        assert!(shell_event(&pane, Some(true)));
        assert_eq!(emoji(&pane), "🔧");
        assert!(!shell_event(&pane, Some(true)));
        assert!(shell_event(&pane, Some(false)));
        assert_eq!(emoji(&pane), "💲");
        pane.fd = None;
        assert!(refresh(&pane, false));
        let checked = pane.process_status.borrow().checked;
        assert!(!refresh(&pane, false));
        assert_eq!(pane.process_status.borrow().checked, checked);
    }
}
