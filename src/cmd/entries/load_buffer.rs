use crate::src::arguments::{args_get, args_has, args_string};
use crate::src::cmd::cmd_get_args_mut;
use crate::src::cmd::queue::{cmdq_continue, cmdq_error, cmdq_get_client, cmdq_get_target_client};
use crate::src::file::file_read_with_cmdq_wait;
use crate::src::format::bytes::nullable_cstr;
use crate::src::format::bytes::write_cstr;
use crate::src::format::format_single_from_target_cstring;
use crate::src::paste::paste_set_owned;
use crate::src::reactor::{evbuffer_get_length, evbuffer_pullup};
use libc::strerror;

use crate::src::server_client::Client as _;
use crate::src::shared::abi::*;
use crate::src::shared::arguments::{args, args_parse};
use crate::src::shared::client::ClientRef;
use crate::src::shared::command::*;
use crate::src::shared::command::{cmd, cmd_entry, cmd_entry_flag, cmdq_item};
use crate::src::shared::command::{CMD_AFTERHOOK, CMD_CLIENT_CANFAIL, CMD_CLIENT_TFLAG};
use crate::src::tty::tty_set_selection;
use std::cell::UnsafeCell;
use std::ffi::{CStr, CString};
use std::rc::Weak;
use tiri_buffer::SegmentedBuf;

#[repr(C)]
pub struct cmd_load_buffer_data {
    pub client: Option<ClientRef>,
    pub item: Weak<UnsafeCell<cmdq_item>>,
    pub name: Option<CString>,
}

impl cmd_load_buffer_data {
    unsafe fn release_client(&mut self) {
        if let Some(client) = self.client.take() {
            (client).release();
        }
    }
}

impl Drop for cmd_load_buffer_data {
    fn drop(&mut self) {
        unsafe { self.release_client() }
    }
}
pub static cmd_load_buffer_entry: cmd_entry = {
    cmd_entry {
        name: c"load-buffer",
        alias: Some(c"loadb"),
        args: args_parse {
            template: c"b:t:w",
            lower: 1 as ::core::ffi::c_int,
            upper: 1 as ::core::ffi::c_int,
            cb: None,
        },
        usage: c"[-b buffer-name] [-t target-client] path",
        source: cmd_entry_flag {
            flag: 0,
            type_0: CMD_FIND_PANE,
            flags: 0,
        },
        target: cmd_entry_flag {
            flag: 0,
            type_0: CMD_FIND_PANE,
            flags: 0,
        },
        flags: CMD_AFTERHOOK | CMD_CLIENT_TFLAG | CMD_CLIENT_CANFAIL,
        exec: Some(cmd_load_buffer_exec),
    }
};
unsafe fn cmd_load_buffer_done(
    cdata: &mut cmd_load_buffer_data,
    path: Option<&CStr>,
    mut error: ::core::ffi::c_int,
    mut closed: ::core::ffi::c_int,
    mut buffer: &mut SegmentedBuf,
) {
    // Progress notifications do not need contiguous storage. Coalesce only
    // once, after the complete file has arrived.
    if closed == 0 {
        return;
    }
    let tc = cdata.client.as_ref();
    let item_owner = cdata.item.upgrade();
    let mut bdata: *mut ::core::ffi::c_void = evbuffer_pullup(buffer, -1)
        .map_or(std::ptr::null_mut(), |bytes| bytes.as_mut_ptr())
        as *mut ::core::ffi::c_void;
    let mut bsize: size_t = evbuffer_get_length(&*(buffer));
    let mut cause: Option<CString> = None;
    if error != 0 as ::core::ffi::c_int {
        if let Some(item_owner) = item_owner.as_ref() {
            cmdq_error(item_owner, |out| {
                write_cstr(out, nullable_cstr(strerror(error)))?;
                out.write_all(b": ")?;
                write_cstr(out, path)
            });
        }
    } else if bsize != 0 as size_t {
        let owned: Box<[u8]> = std::slice::from_raw_parts(bdata.cast::<u8>(), bsize).into();
        if paste_set_owned(owned, cdata.name.as_deref(), Some(&mut cause))
            != 0 as ::core::ffi::c_int
        {
            if let Some(item_owner) = item_owner.as_ref() {
                cmdq_error(item_owner, |out| {
                    write_cstr(out, (cause.as_ref().unwrap()).as_c_str())
                });
            }
        } else if tc.is_some_and(|client| {
            client.attached_session().upgrade().is_some() && !client.is_dead()
        }) {
            tty_set_selection(
                tc.expect("selection client"),
                c"",
                std::slice::from_raw_parts(bdata.cast(), bsize),
            );
        }
    }
    cdata.release_client();
    if let Some(item_owner) = item_owner.as_ref() {
        cmdq_continue(item_owner);
    }
}
unsafe fn cmd_load_buffer_exec(
    mut self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
) -> cmd_retval {
    let item = item_handle.get();
    let queue_client = cmdq_get_client((item).as_ref());
    let mut args: *mut args =
        cmd_get_args_mut(self_0.get_mut_unchecked()).map_or(std::ptr::null_mut(), |args| args);
    let tc_owner = cmdq_get_target_client((item).as_ref());
    let mut tc: Option<ClientRef> = tc_owner.clone();
    let mut cdata = Box::new(cmd_load_buffer_data {
        client: None,
        item: std::rc::Rc::downgrade(item_handle),
        name: None,
    });
    let mut bufname: *const ::core::ffi::c_char =
        args_get(&*(args), 'b' as i32 as u_char).map_or(std::ptr::null(), |value| value.as_ptr());
    if !bufname.is_null() {
        cdata.name = Some(CStr::from_ptr(bufname).to_owned());
    }
    if args_has(args, 'w' as i32 as u_char) != 0 && !tc.is_none() {
        cdata.client = tc_owner.clone();
    }
    let path = format_single_from_target_cstring(
        item_handle,
        args_string(&mut *(args), 0 as u_int).map_or(std::ptr::null(), |value| value.as_ptr()),
    );
    file_read_with_cmdq_wait(
        queue_client.as_ref(),
        &path,
        Some(Box::new(move |event| unsafe {
            cmd_load_buffer_done(
                &mut cdata,
                event.path,
                event.error,
                event.closed as ::core::ffi::c_int,
                event.buffer.expect("read callback buffer"),
            )
        })),
        item_handle,
        None,
    );
    CMD_RETURN_WAIT
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiri_buffer::{Buf, BufMut, Buffer, SegmentedBuf};

    #[test]
    fn file_progress_keeps_segments_until_completion() {
        let mut data = cmd_load_buffer_data {
            client: None,
            item: Weak::new(),
            name: None,
        };
        let mut buffer = SegmentedBuf::default();
        buffer.put(SegmentedBuf::from(vec![1; 4096]));
        let first = buffer.chunk().as_ptr();
        for count in 2..=16 {
            buffer.put(SegmentedBuf::from(vec![2; 4096]));
            unsafe {
                cmd_load_buffer_done(&mut data, Some(c"input"), 0, 0, &mut buffer);
            }
            assert_eq!(buffer.chunks().count(), count);
            assert_eq!(buffer.chunk().as_ptr(), first);
            assert_eq!(buffer.remaining(), count * 4096);
        }
    }

    #[test]
    fn expired_wait_ignores_completion_error() {
        let mut data = cmd_load_buffer_data {
            client: None,
            item: Weak::new(),
            name: None,
        };
        let mut buffer = SegmentedBuf::default();
        unsafe { cmd_load_buffer_done(&mut data, Some(c"input"), 5, 1, &mut buffer) };
    }
}
