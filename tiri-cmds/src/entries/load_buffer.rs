use crate::cmd::cmd_get_args;
use crate::cmd::invocation::{ExecOutcome, Invocation};
use crate::shared::command::CMD_FIND_PANE;
use crate::shared::command::{CMD_AFTERHOOK, CMD_CLIENT_CANFAIL, CMD_CLIENT_TFLAG};
use crate::shared::command::{cmd, cmd_entry, cmd_entry_flag};
use std::ffi::{CStr, CString};
use std::future::Future;
use std::pin::Pin;
use tiri_core::arguments::{args_get, args_has, args_parse, args_string};
use tiri_core::file::{FileOutcome, file_read_wait};
use tiri_core::format::bytes::write_cstr;
use tiri_core::paste::paste_set_owned;
use tiri_core::reactor::evbuffer_pullup;
use tiri_core::server_client::Client as _;
use tiri_core::shared::abi::{u_char, u_int};
use tiri_core::shared::client::ClientRef;
use tiri_core::shared::errno::ErrorText;
use tiri_core::tty::tty_set_selection;

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

fn cmd_load_buffer_done(
    inv: &Invocation<'_>,
    cdata: cmd_load_buffer_data,
    mut outcome: FileOutcome,
) {
    let tc = cdata.client.as_ref();
    let path = outcome.path.as_deref();
    let error = outcome.error;
    let buffer = &mut outcome.buffer;
    let bdata: &[u8] = evbuffer_pullup(buffer, -1).map_or(&[], |bytes| bytes);
    let mut cause: Option<CString> = None;
    if error != 0 as ::core::ffi::c_int {
        inv.error(|out| {
            write!(
                out,
                "{}",
                ErrorText(&std::io::Error::from_raw_os_error(error))
            )?;
            out.write_all(b": ")?;
            write_cstr(out, path)
        });
    } else if !bdata.is_empty() {
        let owned: Box<[u8]> = bdata.into();
        if paste_set_owned(owned, cdata.name.as_deref(), Some(&mut cause))
            != 0 as ::core::ffi::c_int
        {
            inv.error(|out| write_cstr(out, (cause.as_ref().unwrap()).as_c_str()));
        } else if tc.is_some_and(|client| {
            client.attached_session().upgrade().is_some() && !client.is_dead()
        }) {
            tty_set_selection(tc.expect("selection client"), c"", bdata);
        }
    }
}

async fn cmd_load_buffer_wait<'a, 'ctx>(
    self_0: &'a cmd,
    inv: &'a mut Invocation<'ctx>,
    done: impl Future<Output = Option<FileOutcome>>,
    cdata: cmd_load_buffer_data,
) -> ExecOutcome {
    inv.suspend(self_0);
    let Some(outcome) = done.await else {
        drop(cdata);
        return std::future::pending().await;
    };
    cmd_load_buffer_done(inv, cdata, outcome);
    ExecOutcome::ok()
}

fn cmd_load_buffer_exec<'a, 'ctx>(
    self_0: &'a cmd,
    inv: &'a mut Invocation<'ctx>,
) -> Pin<Box<dyn Future<Output = ExecOutcome> + 'a>> {
    Box::pin(async move {
        let queue_client = inv.client();
        let args = cmd_get_args(self_0);
        let tc_owner = inv.target_client();
        let mut cdata = cmd_load_buffer_data {
            client: None,
            name: args_get(args, 'b' as i32 as u_char).map(CStr::to_owned),
        };
        if args_has(args, 'w' as i32 as u_char) != 0 && tc_owner.is_some() {
            cdata.client = tc_owner.clone();
        }
        let path = inv.format_single(args_string(args, 0 as u_int).unwrap_or(c""));
        let wait = file_read_wait(queue_client.as_ref(), &path, |_| None);
        drop(tc_owner);
        drop(queue_client);
        cmd_load_buffer_wait(self_0, inv, wait, cdata).await
    })
}

#[cfg(feature = "test-support")]
pub mod load_buffer_tests {
    use super::{
        CStr, CString, ClientRef, FileOutcome, cmd_load_buffer_data, cmd_load_buffer_wait,
        file_read_wait,
    };
    use crate::cmd::run::test_harness::spawn_invocation;
    use std::rc::{Rc, Weak};
    use std::time::Duration;
    use tiri_core::paste::{paste_buffer_data, paste_get_name};
    use tiri_core::reactor::Lane;
    use tiri_core::server_client::queue_test_clients as clients;
    use tiri_core::tiri_buffer::SegmentedBuf;

    fn buffer(name: &CStr) -> Option<Vec<u8>> {
        let pb = paste_get_name(name)?;
        let pb = pb.try_borrow().expect("live paste buffer");
        Some(paste_buffer_data(&pb).unwrap_or_default().to_vec())
    }

    fn append_load(
        owner: &ClientRef,
        done: impl std::future::Future<Output = Option<FileOutcome>> + 'static,
        cdata: cmd_load_buffer_data,
    ) -> Weak<()> {
        let head = spawn_invocation(Some(owner), move |cmd_ref, inv| {
            Box::pin(cmd_load_buffer_wait(cmd_ref, inv, done, cdata))
        });
        assert_eq!(Lane::drive(Some(owner)), 0);
        head
    }

    pub fn a_reported_file_is_stored_inside_the_send() {
        let owner = clients::with_queue();
        let (tx, rx) = tiri_core::tiri_rt::oneshot::channel();
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
        Lane::drive(Some(&owner));
        assert!(head.upgrade().is_none());
    }

    pub fn a_local_file_is_stored_one_runtime_turn_later() {
        use tiri_core::tiri_rt::Runtime as _;
        let mut runtime = tiri_core::tiri_rt::mio::Runtime::new().unwrap();
        {
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
            Lane::drive(Some(&owner));
            assert!(head.upgrade().is_none());
            std::fs::remove_file(path.to_str().unwrap()).unwrap();
        }
        tiri_core::reactor::shutdown_runtime(runtime);
    }

    pub fn a_dead_file_client_releases_the_target_client_and_parks() {
        let owner = clients::with_queue();
        let (tx, rx) = tiri_core::tiri_rt::oneshot::channel();
        let cdata = cmd_load_buffer_data {
            client: Some(owner.clone()),
            name: Some(c"lost".to_owned()),
        };
        let head = append_load(&owner, rx, cdata);
        drop(tx);
        assert_eq!(Rc::strong_count(&owner), 1, "the data is released");
        assert_eq!(Lane::drive(Some(&owner)), 0);
        assert!(head.upgrade().is_some(), "the command stays blocked");
        let client = Rc::downgrade(&owner);
        drop(owner);
        assert!(client.upgrade().is_none());
        assert!(head.upgrade().is_none());
        assert!(buffer(c"lost").is_none(), "the body does not run");
    }
}
