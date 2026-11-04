use std::ffi::{CStr, CString};
use tiri_vt::{
    ReplyError, TerminalHost, TerminalReplyKind as Kind, TerminalReplyState, TerminalReport,
    parse_terminal_reply,
};

#[derive(Default)]
struct Host {
    state: TerminalReplyState,
    features: Vec<&'static CStr>,
    terminal: Option<&'static CStr>,
    version: Option<CString>,
    xtermjs: bool,
    reports: usize,
}

impl TerminalHost for Host {
    fn terminal_reply_state(&self) -> TerminalReplyState {
        self.state
    }

    fn terminal_report(&mut self, report: TerminalReport<'_>) {
        self.reports += 1;
        self.features.extend(report.features);
        self.terminal = report.terminal.or(self.terminal);
        if let Some(version) = report.version {
            self.version = Some(version.to_owned());
        }
        if let Some(xtermjs) = report.xtermjs {
            self.xtermjs = xtermjs;
        }
        match report.kind {
            Kind::PrimaryAttributes => self.state.primary_attributes = true,
            Kind::SecondaryAttributes => self.state.secondary_attributes = true,
            Kind::ExtendedAttributes => self.state.extended_attributes = true,
            Kind::SynchronizedOutput => {
                self.state.synchronized_output = true;
                self.state.supports_synchronized_output = self.features.contains(&c"sync");
            }
        }
    }
}

#[test]
fn fragmented_replies_do_not_publish_until_complete() {
    for (kind, bytes) in [
        (Kind::PrimaryAttributes, b"\x1b[?64;4;21;28;52c".as_slice()),
        (Kind::SecondaryAttributes, b"\x1b[>84;0;0c"),
        (Kind::ExtendedAttributes, b"\x1bP>|WezTerm 2024\x1b\\"),
        (Kind::SynchronizedOutput, b"\x1b[?2026;2$y"),
    ] {
        let mut host = Host::default();
        for end in 0..bytes.len() {
            assert_eq!(
                parse_terminal_reply(&mut host, kind, &bytes[..end]),
                Err(ReplyError::Incomplete)
            );
            assert_eq!(host.reports, 0);
        }
        let mut input = bytes.to_vec();
        input.extend_from_slice(b"following input");
        assert_eq!(
            parse_terminal_reply(&mut host, kind, &input),
            Ok(bytes.len())
        );
        assert_eq!(host.reports, 1);
        assert_eq!(
            parse_terminal_reply(&mut host, kind, bytes),
            Err(ReplyError::Unrecognized)
        );
        assert_eq!(host.reports, 1);
    }
}

#[test]
fn identifies_terminals_and_primary_features() {
    let mut host = Host::default();
    parse_terminal_reply(&mut host, Kind::PrimaryAttributes, b"\x1b[?64;4;21;28;52c").unwrap();
    assert_eq!(
        host.features,
        [c"sixel", c"margins", c"rectfill", c"clipboard"]
    );
    parse_terminal_reply(&mut host, Kind::SecondaryAttributes, b"\x1b[>84;0;0c").unwrap();
    assert_eq!(host.terminal, Some(c"tmux"));
    parse_terminal_reply(
        &mut host,
        Kind::ExtendedAttributes,
        b"\x1bP>|foot(1.16.2)\x1b\\",
    )
    .unwrap();
    assert_eq!(host.terminal, Some(c"foot"));
    assert_eq!(host.version.as_deref(), Some(c"foot(1.16.2)"));
}

#[test]
fn later_evidence_refines_xtermjs_in_either_order() {
    let da = (Kind::SecondaryAttributes, b"\x1b[>0;276;0c".as_slice());
    for other in [
        (
            Kind::ExtendedAttributes,
            b"\x1bP>|WezTerm 2024\x1b\\".as_slice(),
        ),
        (Kind::SynchronizedOutput, b"\x1b[?2026;2$y"),
    ] {
        let mut host = Host::default();
        parse_terminal_reply(&mut host, da.0, da.1).unwrap();
        assert!(host.xtermjs);
        parse_terminal_reply(&mut host, other.0, other.1).unwrap();
        assert!(!host.xtermjs);
        let mut host = Host::default();
        parse_terminal_reply(&mut host, other.0, other.1).unwrap();
        parse_terminal_reply(&mut host, da.0, da.1).unwrap();
        assert!(!host.xtermjs);
    }
    let mut host = Host::default();
    parse_terminal_reply(&mut host, da.0, da.1).unwrap();
    parse_terminal_reply(&mut host, Kind::SynchronizedOutput, b"\x1b[?2026;0$y").unwrap();
    assert!(host.xtermjs);
}

#[test]
fn malformed_and_oversized_replies_do_not_publish() {
    for (kind, bytes) in [
        (Kind::PrimaryAttributes, b"\x1b[?64;4x".as_slice()),
        (Kind::SecondaryAttributes, b"\x1b[>85x"),
        (Kind::SynchronizedOutput, b"\x1b[?2026;5$y"),
        (Kind::ExtendedAttributes, b"\x1bP?"),
    ] {
        let mut host = Host::default();
        assert_eq!(
            parse_terminal_reply(&mut host, kind, bytes),
            Err(ReplyError::Unrecognized)
        );
        assert_eq!(host.reports, 0);
    }
    for (kind, prefix) in [
        (Kind::PrimaryAttributes, b"\x1b[?".as_slice()),
        (Kind::ExtendedAttributes, b"\x1bP>|"),
    ] {
        let mut host = Host::default();
        let mut bytes = prefix.to_vec();
        bytes.resize(131, b'1');
        assert_eq!(
            parse_terminal_reply(&mut host, kind, &bytes),
            Err(ReplyError::Unrecognized)
        );
        assert_eq!(host.reports, 0);
    }
}
