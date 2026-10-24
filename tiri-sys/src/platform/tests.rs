//! The [`Platform`] contract, exercised only through the trait.
//!
//! Every test here is generic over `P: Platform` and is handed
//! [`CurrentPlatform`], so what is asserted is the contract each operating
//! system has to meet rather than the one this build happens to compile. A
//! platform added later is covered by these the moment it is named here.

use std::os::fd::AsFd;
use std::os::unix::net::UnixStream;
use std::path::Path;

use super::{CurrentPlatform, Platform, ProcessInfo};

/// A descriptor that is not a terminal has no foreground process to report a
/// directory for.
fn a_non_terminal_has_no_pane_cwd<P: Platform>() {
    let (near, _far) = UnixStream::pair().expect("socket pair");
    assert_eq!(P::pane_cwd(near.as_fd()), None);
}

/// A platform that reports a process table reports this process in it, with a
/// parent that is not itself.
fn a_process_table_contains_this_process<P: Platform>() {
    let Some(table) = P::process_table() else {
        return;
    };
    let mine = std::process::id();
    let entry = table
        .iter()
        .copied()
        .find(|process: &ProcessInfo| process.pid == mine)
        .expect("this process is in the table");
    assert_ne!(entry.ppid, entry.pid, "a process is not its own parent");
}

/// A platform that reports a working directory reports an absolute one, and
/// for this process it is the directory this process is actually in.
fn a_reported_process_cwd_is_this_directory<P: Platform>() {
    let Some(cwd) = P::process_cwd(std::process::id()) else {
        return;
    };
    assert!(cwd.is_absolute(), "{cwd:?} is not absolute");
    let here = std::env::current_dir().expect("current directory");
    assert_eq!(cwd, here);
}

/// A platform that can inspect a process reports a file it is holding open.
///
/// Not every entry is a filesystem path — a descriptor on a pipe, a socket or
/// an anonymous inode is reported under the pseudo-path the kernel names it
/// by — so the contract is that a real open file is among them, not that
/// every entry is one.
fn reported_open_files_include_one_we_hold<P: Platform>() {
    let mut path = std::env::temp_dir();
    path.push(format!("tiri-platform-contract-{}", std::process::id()));
    let file = std::fs::File::create(&path).expect("create a file to hold");

    let resolved = std::fs::canonicalize(&path).expect("resolve the held file");
    let open = P::process_open_files(std::process::id());
    if !open.is_empty() {
        assert!(open.contains(&resolved), "open files: {open:?}");
    }

    drop(file);
    let _ = std::fs::remove_file(&path);
}

/// Program names are most specific first, and none of them is empty. A
/// platform that cannot inspect a process says so with an empty list.
fn reported_program_names_are_never_empty<P: Platform>() {
    for name in P::process_programs(std::process::id()) {
        assert!(!name.is_empty(), "an empty program name");
        assert!(
            !Path::new(&name).to_string_lossy().is_empty(),
            "an unnameable program"
        );
    }
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
    assert_eq!(P::process_cwd(0), None);
    assert!(P::process_open_files(0).is_empty());
    assert!(P::process_programs(0).is_empty());
    assert!(P::process_arguments(0).is_empty());
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
        a_process_table_contains_this_process,
        a_reported_process_cwd_is_this_directory,
        reported_open_files_include_one_we_hold,
        reported_program_names_are_never_empty,
        a_reported_argument_vector_has_a_program_name,
        nothing_is_reported_for_a_process_that_is_not_there,
    );
}
