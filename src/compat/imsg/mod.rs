mod message;

use crate::shared::abi::{pid_t, uint32_t};
use libc::{EBADMSG, EINVAL, ERANGE};
use message::Outgoing;
pub use message::{
    imsg, imsg_hdr, msgtype, MSG_COMMAND, MSG_DETACH, MSG_DETACHKILL, MSG_EXEC, MSG_EXIT,
    MSG_EXITED, MSG_EXITING, MSG_FLAGS, MSG_IDENTIFY_CLIENTPID, MSG_IDENTIFY_CWD,
    MSG_IDENTIFY_DONE, MSG_IDENTIFY_ENVIRON, MSG_IDENTIFY_FEATURES, MSG_IDENTIFY_FLAGS,
    MSG_IDENTIFY_LONGFLAGS, MSG_IDENTIFY_STDIN, MSG_IDENTIFY_STDOUT, MSG_IDENTIFY_TERM,
    MSG_IDENTIFY_TERMINFO, MSG_IDENTIFY_TTYNAME, MSG_LOCK, MSG_READ, MSG_READY, MSG_READ_CANCEL,
    MSG_READ_DONE, MSG_READ_OPEN, MSG_RESIZE, MSG_SHELL, MSG_SHUTDOWN, MSG_STDERR, MSG_STDIN,
    MSG_STDOUT, MSG_SUSPEND, MSG_UNLOCK, MSG_VERSION, MSG_WAKEUP, MSG_WRITE, MSG_WRITE_CLOSE,
    MSG_WRITE_DONE, MSG_WRITE_OPEN, MSG_WRITE_READY,
};
pub(crate) use message::{imsgbuf, msg_command, IMSG_HEADER_SIZE, MAX_IMSGSIZE, PROTOCOL_VERSION};
use std::os::fd::{AsRawFd, OwnedFd, RawFd};

const IMSG_FD_MARK: u32 = 0x80000000;

pub(crate) fn imsgbuf_init(buffer: &mut imsgbuf, fd: OwnedFd) {
    *buffer = imsgbuf {
        fd: Some(fd),
        pid: std::process::id() as pid_t,
        ..Default::default()
    };
}

pub(crate) fn imsgbuf_clear(buffer: &mut imsgbuf) {
    *buffer = imsgbuf::default();
}

pub(crate) fn imsgbuf_queuelen(buffer: &imsgbuf) -> uint32_t {
    buffer.outgoing.len() as uint32_t
}

/// Preserve read order, including an FD received before its complete header.
/// A read may include an earlier unmarked message before the marked message.
pub(crate) fn imsgbuf_receive(
    buffer: &mut imsgbuf,
    bytes: &[u8],
    fd: Option<OwnedFd>,
) -> Result<(), i32> {
    if buffer.fd.is_none() {
        return Err(EINVAL);
    }
    buffer.input.extend_from_slice(bytes);
    buffer.input_fds.extend(fd);
    let mut offset = 0;
    while let Some(header) = decode_imsg_hdr(&buffer.input[offset..]) {
        let len = (header.len & !IMSG_FD_MARK) as usize;
        if !(IMSG_HEADER_SIZE..=MAX_IMSGSIZE as usize).contains(&len) {
            return Err(ERANGE);
        }
        if buffer.input.len() - offset < len {
            break;
        }
        let fd = if header.len & IMSG_FD_MARK != 0 {
            Some(buffer.input_fds.pop_front().ok_or(EBADMSG)?)
        } else {
            None
        };
        buffer.incoming.push_back(imsg {
            hdr: imsg_hdr {
                len: len as u32,
                ..header
            },
            data: buffer.input[offset + IMSG_HEADER_SIZE..offset + len].to_vec(),
            fd,
        });
        offset += len;
    }
    buffer.input.drain(..offset);
    if buffer.input_fds.len() > 1 || (buffer.input.is_empty() && !buffer.input_fds.is_empty()) {
        return Err(EBADMSG);
    }
    Ok(())
}

pub(crate) fn imsgbuf_get(buffer: &mut imsgbuf) -> Option<imsg> {
    buffer.incoming.pop_front()
}

/// Snapshot a bounded write, stopping before a second message carrying an FD.
/// The descriptor remains owned by the queue until written or explicitly cleared.
pub(crate) fn imsgbuf_output(buffer: &imsgbuf) -> (Vec<u8>, Option<RawFd>) {
    let mut bytes = Vec::new();
    let fd = buffer
        .outgoing
        .front()
        .and_then(|message| message.fd.as_ref())
        .map(AsRawFd::as_raw_fd);
    for message in &buffer.outgoing {
        if !bytes.is_empty() && message.fd.is_some() {
            break;
        }
        let data = &message.bytes[message.offset..];
        let count = data.len().min(65536 - bytes.len());
        bytes.extend_from_slice(&data[..count]);
        if bytes.len() == 65536 {
            break;
        }
    }
    (bytes, fd)
}

/// A positive write transfers the attached FD even when its bytes are partial.
pub(crate) fn imsgbuf_written(buffer: &mut imsgbuf, mut count: usize) {
    if count == 0 {
        return;
    }
    if let Some(message) = buffer.outgoing.front_mut() {
        drop(message.fd.take());
    }
    while count != 0 {
        let message = buffer.outgoing.front_mut().expect("queued output");
        let written = count.min(message.bytes.len() - message.offset);
        message.offset += written;
        count -= written;
        if message.offset == message.bytes.len() {
            buffer.outgoing.pop_front();
        }
    }
}

pub(crate) fn imsg_get_fd(message: &mut imsg) -> Option<OwnedFd> {
    message.fd.take()
}

pub(crate) fn imsg_compose(
    buffer: &mut imsgbuf,
    type_0: msgtype,
    id: uint32_t,
    pid: pid_t,
    fd: Option<OwnedFd>,
    data: &[u8],
) -> Result<(), i32> {
    if buffer.fd.is_none() {
        return Err(EINVAL);
    }
    let len = data.len().checked_add(IMSG_HEADER_SIZE).ok_or(ERANGE)?;
    if len > MAX_IMSGSIZE as usize {
        return Err(ERANGE);
    }
    let header = imsg_hdr {
        type_0,
        len: len as u32 | if fd.is_some() { IMSG_FD_MARK } else { 0 },
        peerid: id,
        pid: if pid == 0 {
            buffer.pid as u32
        } else {
            pid as u32
        },
    };
    let mut bytes = Vec::with_capacity(len);
    bytes.extend_from_slice(&encode_imsg_hdr(header));
    bytes.extend_from_slice(data);
    buffer.outgoing.push_back(Outgoing {
        bytes,
        offset: 0,
        fd,
    });
    Ok(())
}

fn decode_imsg_hdr(bytes: &[u8]) -> Option<imsg_hdr> {
    if bytes.len() < IMSG_HEADER_SIZE {
        return None;
    }
    let read_u32 = |offset: usize| -> Option<u32> {
        let bytes: [u8; 4] = bytes.get(offset..offset + 4)?.try_into().ok()?;
        Some(u32::from_ne_bytes(bytes))
    };
    Some(imsg_hdr {
        type_0: read_u32(0)?,
        len: read_u32(4)?,
        peerid: read_u32(8)?,
        pid: read_u32(12)?,
    })
}

fn encode_imsg_hdr(hdr: imsg_hdr) -> [u8; IMSG_HEADER_SIZE] {
    let mut bytes = [0; IMSG_HEADER_SIZE];
    bytes[0..4].copy_from_slice(&hdr.type_0.to_ne_bytes());
    bytes[4..8].copy_from_slice(&hdr.len.to_ne_bytes());
    bytes[8..12].copy_from_slice(&hdr.peerid.to_ne_bytes());
    bytes[12..16].copy_from_slice(&hdr.pid.to_ne_bytes());
    bytes
}
