use crate::shared::abi::*;
use crate::shared::posix_terminal::winsize;
use std::os::fd::{AsFd, OwnedFd};
/// Fork with a new pseudo-terminal using the default attributes. `name`
/// receives the NUL-terminated slave path. The parent gets the child's pid and
/// the master; the child gets `None`, with the slave as its controlling
/// terminal and standard streams. Fails without forking when the process has
/// more than one thread.
pub fn fdforkpty(name: &mut [u8; 32], ws: &winsize) -> std::io::Result<Option<(pid_t, OwnedFd)>> {
    let (master, slave) = tiri_rt::unix::open_pty(Some(ws))?;
    let path = tiri_rt::unix::terminal_name(slave.as_fd())?;
    let path = path.as_bytes();
    let length = path.len().min(name.len() - 1);
    name[..length].copy_from_slice(&path[..length]);
    name[length] = 0;
    match tiri_rt::unix::fork()? {
        Some(pid) => Ok(Some((pid, master))),
        None => {
            drop(master);
            if tiri_rt::unix::login_terminal(slave).is_err() {
                tiri_rt::unix::exit_now(1);
            }
            Ok(None)
        }
    }
}
