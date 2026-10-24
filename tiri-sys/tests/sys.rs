use std::os::fd::{AsFd, AsRawFd};

#[test]
fn close_on_exec_from_marks_without_closing() {
    let fd = tiri_sys::open(c"/dev/null", libc::O_WRONLY, 0).unwrap();
    let duplicate = unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_DUPFD, 100) };
    assert!(duplicate >= 100);
    assert_eq!(
        unsafe { libc::fcntl(duplicate, libc::F_GETFD) } & libc::FD_CLOEXEC,
        0
    );
    tiri_sys::close_on_exec_from(100);
    assert_ne!(
        unsafe { libc::fcntl(duplicate, libc::F_GETFD) } & libc::FD_CLOEXEC,
        0
    );
    assert!(tiri_sys::is_open(duplicate));
    unsafe { libc::close(duplicate) };
}

#[test]
fn exec_rejects_an_empty_argument_vector() {
    let error = tiri_sys::exec(tiri_sys::Program::Search(&[]));
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
}

#[test]
fn socket_paths_and_terminal_queries_retain_borrowed_descriptors() {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::MetadataExt;
    let path = std::env::temp_dir().join(format!("tiri-rt-path-{}", std::process::id()));
    let listener = tiri_sys::listen(&path, 8).unwrap();
    std::fs::remove_file(&path).unwrap();
    assert_eq!(
        tiri_sys::socket_path(listener.as_fd()).unwrap(),
        Some(CString::new(path.as_os_str().as_bytes()).unwrap())
    );
    let (socket, _peer) = tiri_sys::socket_pair().unwrap();
    assert_eq!(tiri_sys::socket_path(socket.as_fd()).unwrap(), None);
    assert!(tiri_sys::terminal_name(socket.as_fd()).is_err());
    assert!(tiri_sys::terminal_foreground_group(socket.as_fd()).is_err());

    let (_master, slave) = tiri_sys::open_pty(None).unwrap();
    let owned = tiri_sys::terminal_name(slave.as_fd()).unwrap();
    let named = std::fs::metadata(std::ffi::OsStr::from_bytes(owned.to_bytes())).unwrap();
    let opened = std::fs::File::from(slave.try_clone().unwrap())
        .metadata()
        .unwrap();
    assert_eq!(named.rdev(), opened.rdev());
    assert!(tiri_sys::terminal_attributes(slave.as_fd()).is_ok());
    assert_eq!(tiri_sys::terminal_name(slave.as_fd()).unwrap(), owned);
}

#[test]
fn user_and_group_lookups_round_trip_by_name() {
    let uid = tiri_sys::user_id();
    let name = tiri_sys::user_name(uid).expect("current user has a record");
    assert_eq!(tiri_sys::user_id_by_name(&name), Some(uid));
    assert_eq!(tiri_sys::user_home_by_name(&name), tiri_sys::user_home(uid));
    assert!(tiri_sys::user_id_by_name(c"tiri-no-such-user-name").is_none());
    let group = tiri_sys::group_name(0).expect("group 0 has a record");
    assert_eq!(tiri_sys::group_id_by_name(&group), Some(0));
}

#[test]
fn temporary_files_get_distinct_private_names() {
    use std::os::unix::fs::PermissionsExt;
    let directory = std::env::temp_dir();
    let template = format!("{}/tiri-rt-temporary-XXXXXX\0", directory.display());
    let mut names = Vec::new();
    for _ in 0..3 {
        let mut path = template.clone().into_bytes();
        let fd = tiri_sys::make_temporary(&mut path).unwrap();
        let name = std::ffi::CStr::from_bytes_until_nul(&path)
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        assert!(!name.ends_with("XXXXXX"));
        let metadata = std::fs::File::from(fd).metadata().unwrap();
        assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
        names.push(name);
    }
    for name in &names {
        std::fs::remove_file(name).unwrap();
    }
    names.sort();
    names.dedup();
    assert_eq!(names.len(), 3);
    let mut bad = b"no-suffix\0".to_vec();
    assert!(tiri_sys::make_temporary(&mut bad).is_err());
}
