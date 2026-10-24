use std::io::{IoSlice, Read, Write};
use std::os::fd::{AsFd, AsRawFd};
use std::os::unix::net::UnixStream;
use std::time::{Duration, Instant};
use tiri_rt::{AsyncRead, AsyncWrite, Handle, Runtime, mio, unix};

#[test]
fn borrowed_driver_waits_for_io_and_timers_without_retaining_its_future() {
    let mut runtime = mio::Runtime::new().unwrap();
    let (reader, writer) = unix::socket_pair().unwrap();
    unix::set_nonblocking(reader.as_fd(), true).unwrap();
    unix::set_nonblocking(writer.as_fd(), true).unwrap();
    let reader = mio::Io::new(reader).unwrap();
    let writer = mio::Io::new(writer).unwrap();
    let _task = runtime
        .handle()
        .spawn(async move {
            mio::Sleep::new(Instant::now() + Duration::from_millis(5))
                .await
                .unwrap();
            writer
                .write(&[IoSlice::new(b"borrowed")], None)
                .await
                .unwrap();
        })
        .unwrap();
    let mut bytes = [0; 8];
    assert_eq!(
        runtime
            .block_on(reader.read(&mut bytes))
            .unwrap()
            .unwrap()
            .bytes,
        8
    );
    assert_eq!(&bytes, b"borrowed");
    assert_eq!(
        runtime
            .block_on(reader.read(&mut bytes))
            .unwrap()
            .unwrap()
            .bytes,
        0
    );
    runtime.poll(Some(Duration::ZERO)).unwrap();
}

#[test]
fn shutdown_write_preserves_incoming_data() {
    let (socket, peer) = unix::socket_pair().unwrap();
    assert!(!unix::set_nonblocking(socket.as_fd(), true).unwrap());
    let mut peer = UnixStream::from(peer);
    let mut runtime = mio::Runtime::new().unwrap();
    let source = mio::Io::new(socket.as_fd().try_clone_to_owned().unwrap()).unwrap();
    runtime
        .block_on(source.write(&[IoSlice::new(b"input")], None))
        .unwrap()
        .unwrap();
    unix::shutdown_write(socket.as_fd()).unwrap();
    let mut sent = Vec::new();
    peer.read_to_end(&mut sent).unwrap();
    assert_eq!(sent, b"input");
    peer.write_all(b"reply").unwrap();
    let mut reply = [0; 5];
    assert_eq!(
        runtime
            .block_on(source.read(&mut reply))
            .unwrap()
            .unwrap()
            .bytes,
        5
    );
    assert_eq!(&reply, b"reply");
    drop(source);
}

#[test]
fn close_on_exec_from_marks_without_closing() {
    let fd = unix::open(c"/dev/null", libc::O_WRONLY, 0).unwrap();
    let duplicate = unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_DUPFD, 100) };
    assert!(duplicate >= 100);
    assert_eq!(
        unsafe { libc::fcntl(duplicate, libc::F_GETFD) } & libc::FD_CLOEXEC,
        0
    );
    unix::close_on_exec_from(100);
    assert_ne!(
        unsafe { libc::fcntl(duplicate, libc::F_GETFD) } & libc::FD_CLOEXEC,
        0
    );
    assert!(unix::is_open(duplicate));
    unsafe { libc::close(duplicate) };
}

#[test]
fn exec_rejects_an_empty_argument_vector() {
    let error = unix::exec(unix::Program::Search(&[]));
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
}

#[test]
fn socket_paths_and_terminal_queries_retain_borrowed_descriptors() {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::MetadataExt;
    let path = std::env::temp_dir().join(format!("tiri-rt-path-{}", std::process::id()));
    let listener = unix::listen(&path, 8).unwrap();
    std::fs::remove_file(&path).unwrap();
    assert_eq!(
        unix::socket_path(listener.as_fd()).unwrap(),
        Some(CString::new(path.as_os_str().as_bytes()).unwrap())
    );
    let (socket, _peer) = unix::socket_pair().unwrap();
    assert_eq!(unix::socket_path(socket.as_fd()).unwrap(), None);
    assert!(unix::terminal_name(socket.as_fd()).is_err());
    assert!(unix::terminal_foreground_group(socket.as_fd()).is_err());

    let (_master, slave) = unix::open_pty(None).unwrap();
    let owned = unix::terminal_name(slave.as_fd()).unwrap();
    let named = std::fs::metadata(std::ffi::OsStr::from_bytes(owned.to_bytes())).unwrap();
    let opened = std::fs::File::from(slave.try_clone().unwrap())
        .metadata()
        .unwrap();
    assert_eq!(named.rdev(), opened.rdev());
    assert!(unix::terminal_attributes(slave.as_fd()).is_ok());
    assert_eq!(unix::terminal_name(slave.as_fd()).unwrap(), owned);
}

#[test]
fn user_and_group_lookups_round_trip_by_name() {
    let uid = unix::user_id();
    let name = unix::user_name(uid).expect("current user has a record");
    assert_eq!(unix::user_id_by_name(&name), Some(uid));
    assert_eq!(unix::user_home_by_name(&name), unix::user_home(uid));
    assert!(unix::user_id_by_name(c"tiri-no-such-user-name").is_none());
    let group = unix::group_name(0).expect("group 0 has a record");
    assert_eq!(unix::group_id_by_name(&group), Some(0));
}

#[test]
fn temporary_files_get_distinct_private_names() {
    use std::os::unix::fs::PermissionsExt;
    let directory = std::env::temp_dir();
    let template = format!("{}/tiri-rt-temporary-XXXXXX\0", directory.display());
    let mut names = Vec::new();
    for _ in 0..3 {
        let mut path = template.clone().into_bytes();
        let fd = unix::make_temporary(&mut path).unwrap();
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
    assert!(unix::make_temporary(&mut bad).is_err());
}
