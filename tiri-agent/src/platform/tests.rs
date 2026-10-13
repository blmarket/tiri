//! The [`Platform`] and [`OutputWakeup`] contracts, exercised only through
//! the traits.
//!
//! Every test here is generic over `P: Platform` and is handed
//! [`CurrentPlatform`], so what is asserted is the contract each operating
//! system has to meet rather than the one this build happens to compile. A
//! platform added later is covered by these the moment it is named here.
//!
//! Two methods are deliberately absent. [`Platform::fork_pty`] forks the
//! process, and a child returning into the test harness would run the suite a
//! second time; [`Platform::close_fds_from`] closes every descriptor at or
//! above its argument, which in a test process includes the harness's own.
//! Both are documented `unsafe` with exactly that caller obligation, so
//! neither is a contract a test process can hold up its end of.

use std::os::fd::{AsFd, AsRawFd as _};
use std::os::unix::net::UnixStream;
use std::path::Path;

use super::{CurrentPlatform, OutputWakeup, Platform, ProcessInfo};

/// Whether `fd` has something to read without blocking.
fn is_readable(fd: &impl AsFd) -> bool {
    let mut pollfd = libc::pollfd {
        fd: fd.as_fd().as_raw_fd(),
        events: libc::POLLIN,
        revents: 0,
    };
    unsafe { libc::poll(&mut pollfd, 1, 0) == 1 }
}

/// A wakeup is handed out already signalled, so a consumer that polls before
/// anything has happened still takes one turn and reads the initial state.
fn wakeup_starts_signalled<P: Platform>() {
    let wakeup = P::new_output_wakeup().expect("create wakeup");
    assert!(is_readable(&wakeup), "a new wakeup is signalled");
}

/// Repeated wakes may coalesce, and one `clear` takes the lot: after any
/// number of wakes a single clear leaves the descriptor quiet.
fn wakes_coalesce_and_one_clear_takes_them_all<P: Platform>() {
    let wakeup = P::new_output_wakeup().expect("create wakeup");

    for _ in 0..4 {
        wakeup.wake().expect("wake");
    }
    wakeup.clear().expect("clear");
    assert!(!is_readable(&wakeup), "one clear took every pending wake");
}

/// Clearing a wakeup that is already clear is allowed and leaves it clear.
fn clearing_a_clear_wakeup_does_nothing<P: Platform>() {
    let wakeup = P::new_output_wakeup().expect("create wakeup");
    wakeup.clear().expect("clear the initial signal");
    assert!(!is_readable(&wakeup));

    wakeup.clear().expect("clear an already-clear wakeup");
    assert!(!is_readable(&wakeup), "still clear");
}

/// A wakeup goes on signalling after it has been cleared, which is what lets
/// one live for the life of a pane rather than being replaced per wake.
fn a_cleared_wakeup_can_be_woken_again<P: Platform>() {
    let wakeup = P::new_output_wakeup().expect("create wakeup");
    wakeup.clear().expect("clear");
    assert!(!is_readable(&wakeup));

    wakeup.wake().expect("wake after clear");
    assert!(is_readable(&wakeup), "signalled again");

    wakeup.clear().expect("clear again");
    assert!(!is_readable(&wakeup));
}

/// Two wakeups are independent: one going off says nothing about the other,
/// which is what lets a server hold one per pane.
fn wakeups_are_independent<P: Platform>() {
    let first = P::new_output_wakeup().expect("first wakeup");
    let second = P::new_output_wakeup().expect("second wakeup");
    first.clear().expect("clear first");
    second.clear().expect("clear second");

    first.wake().expect("wake first");
    assert!(is_readable(&first));
    assert!(!is_readable(&second), "the other is untouched");
}

/// The far end of a connected Unix socket pair is this process, so the uid the
/// platform reports for it is this process's own.
fn peer_uid_of_our_own_socket_is_our_uid<P: Platform>() {
    let (near, _far) = UnixStream::pair().expect("socket pair");
    let reported = P::peer_uid(near.as_fd());
    assert_eq!(reported, Some(unsafe { libc::geteuid() }));
}

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

    // The kernel reports resolved paths; macOS's temporary directory lives
    // under the /tmp -> /private/tmp symlink.
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

/// A wakeup is shared across threads, so the contract requires it to be `Send`
/// and `Sync`; this is checked at compile time rather than run time.
fn a_wakeup_crosses_threads<P: Platform>()
where
    P::OutputWakeup: Send + Sync,
{
    fn require<T: Send + Sync>() {}
    require::<P::OutputWakeup>();
}

/// A wakeup owns its descriptor for as long as it lives, and gives it back
/// when dropped: a descriptor opened afterwards may reuse the number, so what
/// matters is that dropping does not leave it open.
fn a_dropped_wakeup_gives_its_descriptor_back<P: Platform>() {
    let wakeup = P::new_output_wakeup().expect("create wakeup");
    let raw = wakeup.as_fd().as_raw_fd();
    drop(wakeup);

    let still_open = unsafe { libc::fcntl(raw, libc::F_GETFD) } != -1;
    assert!(!still_open, "the descriptor outlived the wakeup");
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
        wakeup_starts_signalled,
        wakes_coalesce_and_one_clear_takes_them_all,
        clearing_a_clear_wakeup_does_nothing,
        a_cleared_wakeup_can_be_woken_again,
        wakeups_are_independent,
        peer_uid_of_our_own_socket_is_our_uid,
        a_non_terminal_has_no_pane_cwd,
        a_process_table_contains_this_process,
        a_reported_process_cwd_is_this_directory,
        reported_open_files_include_one_we_hold,
        reported_program_names_are_never_empty,
        a_reported_argument_vector_has_a_program_name,
        nothing_is_reported_for_a_process_that_is_not_there,
        a_wakeup_crosses_threads,
        a_dropped_wakeup_gives_its_descriptor_back,
    );
}
