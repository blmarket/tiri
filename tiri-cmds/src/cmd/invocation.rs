use crate::cmd::{cmd_get_args, cmd_get_entry, cmd_get_source};
use crate::shared::command::{
    CMD_AFTERHOOK, CMDQ_STATE_NOHOOKS, cmd, cmd_entry_flag,
};
use crate::cmd::find::TargetLookup as _;
use std::cell::Cell;
use std::ffi::{CStr, CString};
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use tiri_core::actions::{CmdFailed, CmdResult, Context, ProgramRef, Target};
use tiri_core::arguments::{
    args_count, args_flag_values, args_flags, args_get, args_print_cstring, args_string,
};
use tiri_core::control::control_write_guard;
use tiri_core::events_payload::{
    event_payload_create, event_payload_set_string, event_payload_set_target,
};
use tiri_core::format::bytes::{format_message_with, write_cstr};
use tiri_core::format::{format_create_from_state, format_single_from_state_cstring};
use tiri_core::hooks::{HookCommandContext, hooks_fire_command, set_running_hook_context};
use tiri_core::reactor::LaneTarget;
use tiri_core::server_client::Client as _;
use tiri_core::shared::abi::{time_t, u_char, u_int, uint64_t};
use tiri_core::shared::client::{CLIENT_CONTROL, ClientRef, ClientWeak};
use tiri_core::shared::format::{CommandFormat, format_tree};
use tiri_core::shared::key::{KEYC_NONE, key_event};
use tiri_core::tiri_buffer::SegmentedBuf;

pub type Tail =
    Box<dyn for<'c> FnOnce(&'c mut Context) -> Pin<Box<dyn Future<Output = ()> + 'c>>>;

pub struct ExecOutcome {
    pub result: CmdResult,
    pub tail: Option<Tail>,
}

impl ExecOutcome {
    pub fn ok() -> Self {
        Self {
            result: Ok(()),
            tail: None,
        }
    }

    pub fn err() -> Self {
        Self {
            result: Err(CmdFailed),
            tail: None,
        }
    }

    pub fn tail<F>(f: F) -> Self
    where
        F: for<'c> FnOnce(&'c mut Context) -> Pin<Box<dyn Future<Output = ()> + 'c>> + 'static,
    {
        Self {
            result: Ok(()),
            tail: Some(Box::new(f)),
        }
    }

    pub fn commands(cmdlist: Rc<crate::shared::command::cmd_list>) -> Self {
        if cmdlist.groups.iter().all(Vec::is_empty) {
            drop(cmdlist);
            return Self::tail(|ctx| {
                Box::pin(async move {
                    LaneTarget::for_client(ctx.client.upgrade().as_ref()).step();
                })
            });
        }
        Self::tail(move |ctx| {
            Box::pin(async move {
                let _ = crate::cmd::run::run_list(&cmdlist, ctx).await;
            })
        })
    }
}

thread_local! {
    static NEXT_INVOCATION_ID: Cell<usize> = const { Cell::new(1) };
}

fn next_invocation_id() -> usize {
    NEXT_INVOCATION_ID.with(|cell| {
        let id = cell.get();
        cell.set(id.wrapping_add(1));
        id
    })
}

pub struct Invocation<'ctx> {
    context: &'ctx mut Context,
    id: usize,
    lane: LaneTarget,
    action_name: &'static CStr,
    source_loc: Option<(CString, u32)>,
    lent_client: Option<ClientWeak>,
    target_client: ClientWeak,
    source: Target,
    target: Target,
    guard_stamp: (time_t, u32),
    control_flags: i32,
    hooks: Vec<(ProgramRef, Context)>,
    suspended: &'ctx Cell<bool>,
}

impl<'ctx> Invocation<'ctx> {
    pub(crate) fn new(
        context: &'ctx mut Context,
        lane: LaneTarget,
        cmd_ref: &cmd,
        guard_stamp: (time_t, u32),
        control_flags: i32,
        suspended: &'ctx Cell<bool>,
    ) -> Self {
        let (file, line) = cmd_get_source(cmd_ref);
        Self {
            context,
            id: next_invocation_id(),
            lane,
            action_name: cmd_get_entry(cmd_ref).name,
            source_loc: file.map(|f| (f.to_owned(), line)),
            lent_client: None,
            target_client: std::rc::Weak::new(),
            source: Target::default(),
            target: Target::default(),
            guard_stamp,
            control_flags,
            hooks: Vec::new(),
            suspended,
        }
    }

    pub fn context(&self) -> &Context {
        self.context
    }

    pub fn context_mut(&mut self) -> &mut Context {
        self.context
    }

    pub fn client_weak(&self) -> ClientWeak {
        self.lent_client
            .clone()
            .unwrap_or_else(|| self.context.client.clone())
    }

    pub fn client(&self) -> Option<ClientRef> {
        self.client_weak().upgrade()
    }

    pub fn owning_client(&self) -> ClientWeak {
        self.context.client.clone()
    }

    pub fn target_client(&self) -> Option<ClientRef> {
        self.target_client.upgrade()
    }

    pub fn source(&self) -> &Target {
        &self.source
    }

    pub fn target(&self) -> &Target {
        &self.target
    }

    pub fn event(&self) -> key_event {
        self.context.event.as_ref().map_or_else(
            || key_event {
                client: std::rc::Weak::new(),
                key: KEYC_NONE,
                m: Default::default(),
                bytes: None,
            },
            key_event::metadata_snapshot,
        )
    }

    pub fn flags(&self) -> i32 {
        self.context.flags
    }

    pub fn format(&self) -> CommandFormat {
        let m = self.context.event.as_ref().map(|e| e.m).unwrap_or_default();
        CommandFormat::new(
            self.client_weak(),
            Some(self.action_name),
            m,
            self.context.formats.as_deref(),
        )
    }

    pub fn format_from_target(&self) -> Box<format_tree> {
        let cf = self.format();
        let tc = self.target_client();
        format_create_from_state(Some(&cf), tc.as_ref(), &self.target)
    }

    pub fn format_single(&self, fmt: &CStr) -> CString {
        let cf = self.format();
        let tc = self.target_client();
        format_single_from_state_cstring(Some(&cf), fmt, tc.as_ref(), &self.target)
    }

    pub fn print(&self, write: impl FnOnce(&mut dyn std::io::Write) -> std::io::Result<()>) {
        let client = self.client();
        ClientRef::print_line(client.as_ref(), write);
    }

    pub fn print_data(&self, evb: &mut SegmentedBuf) {
        let client = self.client();
        ClientRef::print_to(client.as_ref(), true, evb);
    }

    pub fn error(&self, write: impl FnOnce(&mut dyn std::io::Write) -> std::io::Result<()>) {
        let client = self.client();
        let source_ref = if client.is_none() {
            self.source_loc
                .as_ref()
                .map(|(file, line)| (file.as_c_str(), *line))
        } else {
            None
        };
        ClientRef::report_error(client.as_ref(), source_ref, write);
    }

    pub fn insert_hook(
        &mut self,
        current: Option<&Target>,
        cmd_ref: &cmd,
        write: impl FnOnce(&mut dyn std::io::Write) -> std::io::Result<()>,
    ) {
        if self.context.flags & CMDQ_STATE_NOHOOKS != 0 {
            return;
        }
        let args_0 = cmd_get_args(cmd_ref);
        let name = format_message_with(write);
        let mut ep = event_payload_create();
        if let Some(current) = current {
            event_payload_set_target(&mut ep, current);
        }
        let arguments = args_print_cstring(args_0);
        event_payload_set_string(&mut ep, c"arguments", |out| write_cstr(out, &*arguments));
        let mut i: u_int = 0;
        while i < args_count(args_0) {
            let key = format_message_with(|out| write!(out, "argument_{}", i));
            event_payload_set_string(&mut ep, &key, |out| write_cstr(out, args_string(args_0, i)));
            i = i.wrapping_add(1);
        }
        for flag in args_flags(args_0) {
            let value = args_get(args_0, flag as u_char);
            let key = format_message_with(|out| {
                out.write_all(b"flag_")?;
                out.write_all(&[flag])
            });
            if value.is_none() {
                event_payload_set_string(&mut ep, &key, |out| out.write_all(b"1"));
            } else {
                event_payload_set_string(&mut ep, &key, |out| write_cstr(out, value));
            }
            i = 0;
            for av in args_flag_values(args_0, flag as u_char) {
                let key = format_message_with(|out| {
                    out.write_all(b"flag_")?;
                    out.write_all(&[flag])?;
                    write!(out, "_{}", i)
                });
                event_payload_set_string(&mut ep, &key, |out| write_cstr(out, av.as_string()));
                i = i.wrapping_add(1);
            }
        }
        let tasks = hooks_fire_command(self.id, &self.hook_context(), &name, ep);
        self.hooks.extend(tasks);
    }

    pub fn suspend(&mut self, cmd_ref: &cmd) {
        debug_assert!(!self.suspended.get(), "command suspended twice");
        let entry = cmd_get_entry(cmd_ref);
        if entry.flags & CMD_AFTERHOOK != 0 {
            let execution_client = self.client();
            if let Some(fs) = self.resolve_hook_target(execution_client.as_ref()) {
                self.insert_hook(Some(&fs), cmd_ref, |out| {
                    out.write_all(b"after-")?;
                    write_cstr(out, entry.name)
                });
            }
        }
        self.lent_client = None;
        self.guard(c"end");
        self.suspended.set(true);
        if matches!(self.lane, LaneTarget::Global) {
            set_running_hook_context(None);
        }
    }

    pub fn is_suspended(&self) -> bool {
        self.suspended.get()
    }

    pub(crate) fn set_target_client(&mut self, tc: ClientWeak) {
        self.target_client = tc;
    }

    pub(crate) fn lend_client(&mut self, lent: ClientWeak) {
        self.lent_client = Some(lent);
    }

    pub(crate) fn unlend_client(&mut self) {
        self.lent_client = None;
    }

    pub(crate) fn resolve_source(&mut self, cmd_ref: &cmd, flag: &cmd_entry_flag) -> CmdResult {
        let (fs, res) = self.find_flag(cmd_ref, flag);
        self.source = fs;
        res
    }

    pub(crate) fn resolve_target(&mut self, cmd_ref: &cmd, flag: &cmd_entry_flag) -> CmdResult {
        let (fs, res) = self.find_flag(cmd_ref, flag);
        self.target = fs;
        res
    }

    fn find_flag(&self, cmd_ref: &cmd, flag: &cmd_entry_flag) -> (Target, CmdResult) {
        if flag.flag == 0 {
            let tc = self.target_client();
            let fs = Target::from_client(tc.as_ref(), 0).unwrap_or_else(|| Target::clear_state(0));
            return (fs, Ok(()));
        }
        let mut fs = Target::default();
        if fs.find_target(
            Some(self),
            args_get(cmd_get_args(cmd_ref), flag.flag),
            flag.type_0,
            flag.flags,
        ) != 0
        {
            return (Target::clear_state(0), Err(CmdFailed));
        }
        (fs, Ok(()))
    }

    pub(crate) fn resolve_hook_target(&self, fallback_client: Option<&ClientRef>) -> Option<Target> {
        if self.target.is_valid() {
            Some(self.target.clone())
        } else if self.context.current.is_valid() {
            Some(self.context.current.clone())
        } else {
            Target::from_client(fallback_client, 0)
        }
    }

    pub(crate) fn hook_context(&self) -> HookCommandContext {
        HookCommandContext {
            flags: self.context.flags,
            event: self.event(),
            format: self.format(),
            source: self.source_loc.clone(),
        }
    }

    pub(crate) fn guard(&self, guard: &CStr) {
        let Some(c) = self.client() else {
            return;
        };
        if c.flags() & CLIENT_CONTROL as uint64_t != 0 {
            let (time, number) = self.guard_stamp;
            control_write_guard(&c, guard, time as ::core::ffi::c_long, number, self.control_flags);
        }
    }

    pub(crate) fn take_hooks(&mut self) -> Vec<(ProgramRef, Context)> {
        std::mem::take(&mut self.hooks)
    }
}
