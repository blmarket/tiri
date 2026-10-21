use crate::arguments::{args_get, args_has, args_parse, args_string};
use crate::cmd::cmd_get_args;
use crate::cmd::queue::{cmdq_error, cmdq_get_client, cmdq_get_target_client, cmdq_resumed};
use crate::file::{file_read_wait, FileOutcome};
use crate::format::bytes::write_cstr;
use crate::format::format_single_from_target_cstring;
use crate::paste::paste_set_owned;
use crate::reactor::{evbuffer_get_length, evbuffer_pullup};
use crate::shared::errno::ErrorText;

use crate::server_client::Client as _;
use crate::shared::abi::*;
use crate::shared::client::ClientRef;
use crate::shared::command::*;
use crate::shared::command::{cmd, cmd_entry, cmd_entry_flag, cmdq_item};
use crate::shared::command::{CMD_AFTERHOOK, CMD_CLIENT_CANFAIL, CMD_CLIENT_TFLAG};
use crate::tty::tty_set_selection;
use std::ffi::{CStr, CString};
use std::rc::Weak;

#[repr(C)]
pub struct cmd_load_buffer_data {
    pub client: Option<ClientRef>,
    pub name: Option<CString>,
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
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
    mut cdata: cmd_load_buffer_data,
    mut outcome: FileOutcome,
) {
    let tc = cdata.client.as_ref();
    let path = outcome.path.as_deref();
    let error = outcome.error;
    let buffer = &mut outcome.buffer;
    let mut bdata: *mut ::core::ffi::c_void = evbuffer_pullup(buffer, -1)
        .map_or(std::ptr::null_mut(), |bytes| bytes.as_mut_ptr())
        as *mut ::core::ffi::c_void;
    let mut bsize: size_t = evbuffer_get_length(&*(buffer));
    let mut cause: Option<CString> = None;
    if error != 0 as ::core::ffi::c_int {
        cmdq_error(item_handle, |out| {
            write!(
                out,
                "{}",
                ErrorText(&std::io::Error::from_raw_os_error(error))
            )?;
            out.write_all(b": ")?;
            write_cstr(out, path)
        });
    } else if bsize != 0 as size_t {
        let owned: Box<[u8]> = std::slice::from_raw_parts(bdata.cast::<u8>(), bsize).into();
        if paste_set_owned(owned, cdata.name.as_deref(), Some(&mut cause))
            != 0 as ::core::ffi::c_int
        {
            cmdq_error(item_handle, |out| {
                write_cstr(out, (cause.as_ref().unwrap()).as_c_str())
            });
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
}
/// Store the file once it is read. A file whose client died never reports
/// (tmux's file_fire_done_cb): release the data, and the client with it, then
/// park.
async fn cmd_load_buffer_complete(
    done: impl std::future::Future<Output = Option<FileOutcome>>,
    cdata: cmd_load_buffer_data,
    item: Weak<std::cell::RefCell<cmdq_item>>,
) {
    let Some(outcome) = done.await else {
        drop(cdata);
        return std::future::pending().await;
    };
    unsafe { cmd_load_buffer_done(&cmdq_resumed(&item), cdata, outcome) };
}
unsafe fn cmd_load_buffer_exec(
    self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
) -> CmdOutcome {
    let item = item_handle.borrow();
    let queue_client = cmdq_get_client(Some(&*item));
    let cmd_ref = self_0.try_borrow().expect("live command");
    let args = cmd_get_args(&cmd_ref);
    let tc_owner = cmdq_get_target_client(Some(&*item));
    let mut tc: Option<ClientRef> = tc_owner.clone();
    let mut cdata = cmd_load_buffer_data {
        client: None,
        name: None,
    };
    cdata.name = args_get(args, 'b' as i32 as u_char).map(CStr::to_owned);
    if args_has(args, 'w' as i32 as u_char) != 0 && !tc.is_none() {
        cdata.client = tc_owner.clone();
    }
    let path = format_single_from_target_cstring(
        item_handle,
        args_string(args, 0 as u_int).unwrap_or(c""),
    );
    let wait = file_read_wait(queue_client.as_ref(), &path, |_| None);
    CmdOutcome::Suspend(Box::pin(cmd_load_buffer_complete(
        wait,
        cdata,
        std::rc::Rc::downgrade(item_handle),
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmd::queue::{cmdq_append, cmdq_get_callback_owned, cmdq_next};
    use crate::paste::{paste_buffer_data, paste_get_name};
    use crate::server_client::queue_test_clients as clients;
    use std::rc::Rc;
    use std::time::Duration;
    use tiri_buffer::SegmentedBuf;

    fn buffer(name: &CStr) -> Option<Vec<u8>> {
        let pb = paste_get_name(name)?;
        let pb = pb.borrow();
        Some(paste_buffer_data(&pb).unwrap_or_default().to_vec())
    }

    /// Queue a head on `owner` that suspends until `done` reports the file.
    unsafe fn append_load(
        owner: &ClientRef,
        done: impl std::future::Future<Output = Option<FileOutcome>> + 'static,
        cdata: cmd_load_buffer_data,
    ) -> Weak<std::cell::RefCell<cmdq_item>> {
        let head = cmdq_append(
            Some(owner),
            cmdq_get_callback_owned(
                c"load",
                Some(Box::new(move |item| {
                    CmdOutcome::Suspend(Box::pin(cmd_load_buffer_complete(
                        done,
                        cdata,
                        Rc::downgrade(item),
                    )))
                })),
            ),
        );
        assert_eq!(cmdq_next(Some(owner)), 0);
        head
    }

    #[test]
    fn a_reported_file_is_stored_inside_the_send() {
        unsafe {
            let owner = clients::with_queue();
            let (tx, rx) = tiri_rt::oneshot::channel();
            let cdata = cmd_load_buffer_data {
                client: None,
                name: Some(c"loaded".to_owned()),
            };
            let head = append_load(&owner, rx, cdata);
            assert!(buffer(c"loaded").is_none());
            let outcome = FileOutcome {
                path: Some(c"input".to_owned()),
                error: 0,
                buffer: SegmentedBuf::from(b"hello".to_vec()),
            };
            assert!(tx.send(outcome).is_ok());
            assert_eq!(buffer(c"loaded").as_deref(), Some(&b"hello"[..]));
            cmdq_next(Some(&owner));
            assert!(head.upgrade().is_none());
        }
    }

    #[test]
    fn a_local_file_is_stored_one_runtime_turn_later() {
        use tiri_rt::Runtime as _;
        let mut runtime = tiri_rt::mio::Runtime::new().unwrap();
        unsafe {
            let path = std::env::temp_dir().join(format!("tiri-load-{}", std::process::id()));
            std::fs::write(&path, b"local").unwrap();
            let path = CString::new(path.into_os_string().into_encoded_bytes()).unwrap();
            let owner = clients::with_queue();
            let cdata = cmd_load_buffer_data {
                client: None,
                name: Some(c"local".to_owned()),
            };
            let head = append_load(&owner, file_read_wait(None, &path, |_| None), cdata);
            assert!(buffer(c"local").is_none(), "not in the dispatching pass");
            runtime.poll(Some(Duration::ZERO)).unwrap();
            assert_eq!(buffer(c"local").as_deref(), Some(&b"local"[..]));
            cmdq_next(Some(&owner));
            assert!(head.upgrade().is_none());
            std::fs::remove_file(path.to_str().unwrap()).unwrap();
        }
        crate::reactor::shutdown_runtime(runtime);
    }

    #[test]
    fn a_dead_file_client_releases_the_target_client_and_parks() {
        unsafe {
            let owner = clients::with_queue();
            let (tx, rx) = tiri_rt::oneshot::channel();
            let cdata = cmd_load_buffer_data {
                client: Some(owner.clone()),
                name: Some(c"lost".to_owned()),
            };
            let head = append_load(&owner, rx, cdata);
            drop(tx);
            assert_eq!(Rc::strong_count(&owner), 1, "the data is released");
            assert_eq!(cmdq_next(Some(&owner)), 0);
            assert!(head.upgrade().is_some(), "the command stays blocked");
            let client = Rc::downgrade(&owner);
            drop(owner);
            assert!(client.upgrade().is_none());
            assert!(head.upgrade().is_none());
            assert!(buffer(c"lost").is_none(), "the body does not run");
        }
    }
}
