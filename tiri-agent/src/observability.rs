//! Versioned, read-only runtime observability contracts.
//!
//! These traits are deliberately independent of
//! [`NonblockingTmuxServer`](crate::tmux::NonblockingTmuxServer), keeping pane
//! inspection separate
//! from the wire-protocol contract.

/// The first version of the pane observability contract.
///
/// This module is versioned so future contracts can be added without changing
/// the public traits defined here.
pub mod v1 {
    use std::ffi::CString;
    use std::io;
    use std::rc::Rc;

    /// Stable server-local identity of a pane.
    #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
    pub struct PaneId(pub u32);

    /// Cheap process information associated with a pane.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct PaneProcess {
        /// PID originally spawned on the pane's PTY, if the pane has a process.
        pub child_pid: Option<u32>,
        /// Whether that process has exited.
        pub exited: bool,
    }

    /// A plain-text terminal tail and the output revision it represents.
    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct ScreenTail {
        /// Monotonic pane output revision corresponding to `text`.
        pub revision: u64,
        /// Up to the requested number of rows from the live bottom of the
        /// terminal buffer, independent of any client's scroll position.
        pub text: CString,
        /// Whether the terminal's hardware cursor is currently visible
        /// (DEC private mode 25, DECTCEM).
        pub cursor_visible: bool,
        /// Last DECSCUSR parameter selected by the application: `0` for the
        /// terminal default, then blinking/steady block, underline, and bar as
        /// `1..=6`.
        pub cursor_shape: u8,
    }

    /// Which slice of a pane's terminal buffer to read.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum ScreenSource {
        /// The live viewport only — what an attached client currently sees,
        /// with scrollback history excluded.
        Visible,
        /// The viewport plus recent scrollback history, rendered as displayed.
        /// This is what lets a consumer read back beyond the current screen.
        Recent,
        /// Like [`Recent`](Self::Recent), but rows split only by a right-margin
        /// soft wrap are rejoined into their logical lines.
        RecentUnwrapped,
    }

    /// Server-level access to observable panes.
    ///
    /// Resolving a pane returns a cheap, cacheable handle. Implementations must
    /// not retain a global server-state lock through calls on that handle.
    ///
    /// Observability is a single-threaded contract: implementations and the
    /// handles they return are owned by the server's thread, and consumers run
    /// on that thread (the server loop ticks them between its other work).
    pub trait ServerObservability {
        /// List the panes currently known to the server.
        fn pane_ids(&self) -> io::Result<Vec<PaneId>>;

        /// Resolve a current pane identifier to its observation handle.
        ///
        /// Returns `None` if the pane disappeared before it could be resolved.
        fn resolve_pane(&self, id: PaneId) -> io::Result<Option<Rc<dyn PaneObservability>>>;
    }

    /// Read-only observability for one resolved pane.
    ///
    /// `process` and `output_revision` are intended to be cheap. Consumers can
    /// inspect the foreground process and compare revisions before requesting
    /// the more expensive terminal formatting performed by `last_lines`.
    ///
    /// Like [`ServerObservability`], this contract is single-threaded: a
    /// resolved handle stays on the server's thread.
    pub trait PaneObservability {
        /// Return the pane's child-process lifecycle information.
        fn process(&self) -> io::Result<PaneProcess>;

        /// Return the current monotonic output revision without formatting the
        /// terminal contents.
        fn output_revision(&self) -> io::Result<u64>;

        /// Format up to `lines` rows from the bottom of the requested
        /// [`ScreenSource`] — the visible viewport, recent scrollback, or
        /// recent scrollback with soft wraps rejoined.
        fn screen(&self, source: ScreenSource, lines: usize) -> io::Result<ScreenTail>;

        /// The number of scrollback (history) rows above the visible viewport,
        /// i.e. how far [`ScreenSource::Recent`] can read back.
        fn scrollback_rows(&self) -> io::Result<usize>;

        /// Format up to `lines` rows from the live bottom of the terminal,
        /// including recent scrollback. Convenience shim over
        /// [`screen`](Self::screen) with [`ScreenSource::Recent`].
        fn last_lines(&self, lines: usize) -> io::Result<ScreenTail> {
            self.screen(ScreenSource::Recent, lines)
        }

        /// The pane's terminal title as set by escape sequences (OSC 0/2), or
        /// `None` when no title has been set.
        ///
        /// Some agents (e.g. Codex) report live status — working, idle, or an
        /// approval request — in the window title rather than only on screen, so
        /// this is a distinct detection signal from [`last_lines`](Self::last_lines).
        fn title(&self) -> io::Result<Option<CString>>;
    }

    #[cfg(test)]
    mod tests {
        use std::cell::RefCell;
        use std::ffi::CString;
        use std::io;

        use super::{
            PaneObservability, PaneProcess, ScreenSource, ScreenTail, ServerObservability,
        };

        #[test]
        fn public_traits_are_object_safe() {
            fn accepts_server(_: Option<&dyn ServerObservability>) {}
            fn accepts_pane(_: Option<&dyn PaneObservability>) {}

            accepts_server(None);
            accepts_pane(None);
        }

        /// A pane that records the [`ScreenSource`] and row count it was asked
        /// for, so what the trait's own default method does with them can be
        /// read back.
        #[derive(Default)]
        struct RecordingPane {
            asked: RefCell<Vec<(ScreenSource, usize)>>,
        }

        impl PaneObservability for RecordingPane {
            fn process(&self) -> io::Result<PaneProcess> {
                Ok(PaneProcess {
                    child_pid: None,
                    exited: false,
                })
            }

            fn output_revision(&self) -> io::Result<u64> {
                Ok(1)
            }

            fn screen(&self, source: ScreenSource, lines: usize) -> io::Result<ScreenTail> {
                self.asked.borrow_mut().push((source, lines));
                Ok(ScreenTail {
                    revision: 1,
                    text: CString::new(format!("{source:?}/{lines}")).expect("screen tail"),
                    cursor_visible: true,
                    cursor_shape: 0,
                })
            }

            fn scrollback_rows(&self) -> io::Result<usize> {
                Ok(0)
            }

            fn title(&self) -> io::Result<Option<CString>> {
                Ok(None)
            }
        }

        /// `last_lines` is a shim the trait provides rather than something an
        /// implementor writes, so every server gets the same answer for it:
        /// `screen` over [`ScreenSource::Recent`], with the row count passed
        /// straight through.
        #[test]
        fn last_lines_is_screen_over_recent_scrollback() {
            let pane = RecordingPane::default();

            let through_shim = pane.last_lines(12).expect("last_lines");
            let directly = pane.screen(ScreenSource::Recent, 12).expect("screen");

            assert_eq!(through_shim, directly);
            assert_eq!(
                *pane.asked.borrow(),
                vec![(ScreenSource::Recent, 12), (ScreenSource::Recent, 12)],
                "the shim asked for exactly what a direct call does"
            );
        }

        /// The shim is reachable through a trait object too, which is how a
        /// server hands a resolved pane to a detector.
        #[test]
        fn last_lines_works_through_a_trait_object() {
            let pane = RecordingPane::default();
            let erased: &dyn PaneObservability = &pane;

            assert_eq!(
                erased.last_lines(3).expect("last_lines").text.as_c_str(),
                c"Recent/3"
            );
        }
    }
}
