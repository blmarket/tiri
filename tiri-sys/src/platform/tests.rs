//! The [`Platform`] contract, exercised only through the trait.
//!
//! Every test here is generic over `P: Platform` and is handed
//! [`CurrentPlatform`], so what is asserted is the contract each operating
//! system has to meet rather than the one this build happens to compile. A
//! platform added later is covered by these the moment it is named here.

use std::os::fd::AsFd;
use std::os::unix::net::UnixStream;

use super::{CurrentPlatform, Platform};

/// A descriptor that is not a terminal has no foreground process to report a
/// directory for.
fn a_non_terminal_has_no_pane_cwd<P: Platform>() {
    let (near, _far) = UnixStream::pair().expect("socket pair");
    assert_eq!(P::pane_cwd(near.as_fd()), None);
}

/// A descriptor that is not a terminal has no foreground process to name.
fn a_non_terminal_has_no_pane_name<P: Platform>() {
    let (near, _far) = UnixStream::pair().expect("socket pair");
    assert_eq!(P::pane_name(near.as_fd()), None);
}

/// An argument vector includes `argv[0]`, so a platform that reports one at
/// all reports at least that.
fn a_reported_argument_vector_has_a_program_name<P: Platform>() {
    let arguments = P::process_arguments(std::process::id());
    if arguments.is_empty() {
        return;
    }
    assert!(!arguments[0].is_empty(), "argv[0] is empty");
}

/// Nothing is reported for a process that cannot exist. Pid 0 names no
/// process on any supported platform.
fn nothing_is_reported_for_a_process_that_is_not_there<P: Platform>() {
    assert!(P::process_arguments(0).is_empty());
    assert_eq!(P::process_waiting_for_tty(0), None);
}

/// Every contract above, against the platform this build compiles for.
macro_rules! for_current_platform {
    ($($name:ident),+ $(,)?) => {
        $(
            #[test]
            fn $name() {
                super::$name::<CurrentPlatform>();
            }
        )+
    };
}

mod current {
    use super::CurrentPlatform;

    for_current_platform!(
        a_non_terminal_has_no_pane_cwd,
        a_non_terminal_has_no_pane_name,
        a_reported_argument_vector_has_a_program_name,
        nothing_is_reported_for_a_process_that_is_not_there,
    );
}
