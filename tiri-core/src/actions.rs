use crate::shared::client::{ClientRef, ClientWeak};
use crate::shared::key::key_event;
pub use crate::shared::target::Target;
use std::any::Any;
use std::cell::{Ref, RefCell};
use std::ffi::{CStr, CString};
use std::rc::{Rc, Weak};

pub trait Program: Any {
    fn as_any(&self) -> &dyn Any;
    fn text(&self, flags: i32) -> CString;
    fn substitute(&self, arguments: &Vec<CString>) -> ProgramRef;
    fn read_only(&self) -> bool;
    fn starts_server(&self) -> bool;
}
pub type ProgramRef = Rc<dyn Program>;
pub const PROGRAM_ARGUMENT: i32 = 4;

/// What a command or queue callback did. `Suspend` hands the queue the item's
/// continuation; the item blocks its queue until the continuation completes,
/// and dropping the queue drops the continuation, which cancels the wait. A
/// continuation's own result is not an outcome: it never removes a group.
#[must_use = "return the outcome from the operation"]
pub enum TaskOutcome {
    Normal,
    Error,
    Stop,
    Suspend(std::pin::Pin<Box<dyn std::future::Future<Output = ()>>>),
}
pub struct ExecutionState {
    pub flags: ::core::ffi::c_int,
    pub formats: std::cell::RefCell<Option<Box<crate::shared::format::format_tree>>>,
    pub event: key_event,
    pub current: std::cell::RefCell<Target>,
}
impl ExecutionState {
    pub fn current_snapshot(&self) -> Target {
        self.current.borrow().clone()
    }
    pub fn add_format(&self, key: &CStr, value: &CStr) {
        let mut formats = self.formats.borrow_mut();
        let formats = formats.get_or_insert_with(|| {
            crate::format::format_create(None, None, crate::shared::format::FORMAT_NONE, 0)
        });
        crate::format::format_add_cstr(formats, key, value);
    }
    pub fn add_formats(&self, ft: &crate::shared::format::format_tree) {
        let mut formats = self.formats.borrow_mut();
        let formats = formats.get_or_insert_with(|| {
            crate::format::format_create(None, None, crate::shared::format::FORMAT_NONE, 0)
        });
        crate::format::format_merge(formats, ft);
    }
}

#[derive(Default)]
pub struct Context {
    pub name: Option<CString>,
    pub action_name: Option<&'static CStr>,
    pub client: ClientWeak,
    pub target_client: ClientWeak,
    pub number: u32,
    pub time: crate::shared::abi::time_t,
    pub state: Option<Rc<ExecutionState>>,
    pub source: Target,
    pub target: Target,
}

pub trait Operation: Any {
    fn context(&self) -> Ref<'_, Context>;
}

#[derive(Clone)]
pub struct ContextRef(Rc<dyn Operation>);
impl ContextRef {
    pub fn new<T: Operation>(owner: Rc<T>) -> Self {
        Self(owner)
    }
    pub fn borrow(&self) -> Ref<'_, Context> {
        self.0.context()
    }
    pub fn downgrade(&self) -> ContextWeak {
        ContextWeak(Some(Rc::downgrade(&self.0)))
    }
    pub fn native<T: Operation>(&self) -> Rc<T> {
        let owner: Rc<dyn Any> = self.0.clone();
        owner
            .downcast::<T>()
            .unwrap_or_else(|_| panic!("operation belongs to a different runtime"))
    }
}

#[derive(Clone, Default)]
pub struct ContextWeak(Option<Weak<dyn Operation>>);
impl ContextWeak {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn upgrade(&self) -> Option<ContextRef> {
        self.0.as_ref()?.upgrade().map(ContextRef)
    }
    pub fn address(&self) -> usize {
        self.0
            .as_ref()
            .map_or(0, |owner| owner.as_ptr().cast::<()>().addr())
    }
    pub fn ptr_eq(&self, other: &Self) -> bool {
        match (&self.0, &other.0) {
            (Some(left), Some(right)) => Weak::ptr_eq(left, right),
            (None, None) => true,
            _ => false,
        }
    }
}

pub trait WorkQueue: Any {
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

pub type Callback = Option<Box<dyn FnOnce(&ContextRef) -> TaskOutcome>>;

pub trait ContextSource {
    fn context_ref(&self) -> ContextRef;
    fn borrow(&self) -> Ref<'_, Context>;
    fn downgrade(&self) -> ContextWeak {
        self.context_ref().downgrade()
    }
}
impl ContextSource for ContextRef {
    fn context_ref(&self) -> ContextRef {
        self.clone()
    }
    fn borrow(&self) -> Ref<'_, Context> {
        self.0.context()
    }
}
impl<T: Operation> ContextSource for Rc<T> {
    fn context_ref(&self) -> ContextRef {
        ContextRef::new(self.clone())
    }
    fn borrow(&self) -> Ref<'_, Context> {
        (**self).context()
    }
}

pub struct Completion {
    client: ClientWeak,
    done: crate::tiri_rt::oneshot::Sender<()>,
}
impl Completion {
    pub fn new(client: ClientWeak) -> (Self, crate::tiri_rt::oneshot::Receiver<()>) {
        let (done, receiver) = crate::tiri_rt::oneshot::channel();
        (Self { client, done }, receiver)
    }
    pub fn client(&self) -> Option<ClientRef> {
        self.client.upgrade()
    }
    pub fn is_cancelled(&self) -> bool {
        self.done.is_closed()
    }
    pub fn resume(self) {
        let _ = self.done.send(());
    }
    pub fn finish(self, status: i32) {
        use crate::server_client::Client as _;
        if let Some(client) = self.client() {
            if client.attached_session().upgrade().is_none() {
                client.set_return_value(status);
            }
        }
        self.resume();
    }
    #[cfg(any(test, feature = "test-support"))]
    pub fn detached(client: ClientWeak) -> (Self, crate::tiri_rt::oneshot::Receiver<()>) {
        Self::new(client)
    }
}

#[derive(Default)]
pub struct ParseOrigin<'a> {
    pub file: Option<&'a CStr>,
    pub line: u32,
    pub client: Option<&'a ClientRef>,
    pub flags: i32,
}

pub const PARSE_QUIET: i32 = 0x1;
pub const PARSE_ONLY: i32 = 0x2;
pub const PARSE_NOALIAS: i32 = 0x4;
pub const PARSE_VERBOSE: i32 = 0x8;
pub const PARSE_ONEGROUP: i32 = 0x10;

pub struct CommandEngine {
    pub queue_new: fn() -> Box<dyn WorkQueue>,
    pub queue_append: fn(Option<&ClientRef>, ContextRef) -> ContextWeak,
    pub queue_defer: fn(&ContextRef, ContextRef) -> TaskOutcome,
    pub queue_program: fn(&ProgramRef, Option<&Rc<ExecutionState>>) -> ContextRef,
    pub queue_callback: fn(&'static CStr, Callback) -> ContextRef,
    pub queue_error: fn(&CStr) -> ContextRef,
    pub queue_drive: fn(Option<&ClientRef>) -> u32,
    pub queue_running: fn() -> ContextWeak,
    pub queue_wait: fn(&ContextRef) -> (Completion, TaskOutcome),
    pub queue_cancel: fn(),
    pub report:
        fn(&ContextRef, &mut dyn FnMut(&mut dyn std::io::Write) -> std::io::Result<()>, bool),
    pub guard: fn(&ContextRef, &'static CStr, i32),
    pub parse: fn(&[u8], ParseOrigin<'_>) -> Result<ProgramRef, CString>,
    pub parse_argv: fn(&[CString]) -> Result<ProgramRef, CString>,
    pub command_names: fn() -> Vec<&'static CStr>,
}

static ENGINE: std::sync::OnceLock<&'static CommandEngine> = std::sync::OnceLock::new();
pub fn install(engine: &'static CommandEngine) {
    let installed = ENGINE.get_or_init(|| engine);
    assert!(
        std::ptr::eq(*installed, engine),
        "command engine already installed"
    );
}
pub fn engine() -> &'static CommandEngine {
    ENGINE.get().expect("command engine is installed")
}

pub fn command_names() -> Vec<&'static CStr> {
    (engine().command_names)()
}

pub mod sources {
    use super::{
        CStr, CString, ClientRef, ExecutionState, PARSE_ONEGROUP, ParseOrigin, ProgramRef, Rc,
        engine, queue,
    };
    pub fn compile(source: &CStr) -> Result<ProgramRef, CString> {
        (engine().parse)(
            source.to_bytes(),
            ParseOrigin {
                flags: PARSE_ONEGROUP,
                ..Default::default()
            },
        )
    }
    pub fn compile_file(
        bytes: &[u8],
        file: &CStr,
        client: Option<&ClientRef>,
        flags: i32,
    ) -> Result<ProgramRef, CString> {
        (engine().parse)(
            bytes,
            ParseOrigin {
                file: Some(file),
                line: 1,
                client,
                flags,
            },
        )
    }
    pub fn compile_argv(argv: &[CString]) -> Result<ProgramRef, CString> {
        (engine().parse_argv)(argv)
    }
    pub fn submit(
        source: &CStr,
        client: Option<&ClientRef>,
        state: Option<&Rc<ExecutionState>>,
    ) -> Result<(), CString> {
        let program = compile(source)?;
        queue::enqueue(client, queue::program_task(&program, state));
        Ok(())
    }
    pub fn capitalize_error(error: &mut Option<CString>) {
        if let Some(error) = error {
            let mut bytes = error.as_bytes().to_vec();
            if let Some(first) = bytes.first_mut() {
                first.make_ascii_uppercase();
            }
            *error = CString::new(bytes).expect("C string bytes");
        }
    }
}

pub mod queue {
    pub fn merge_formats(
        item: &dyn super::ContextSource,
        formats: &mut crate::shared::format::format_tree,
    ) {
        let (name, state) = {
            let item = item.borrow();
            (item.action_name, execution_state(&item))
        };
        if let Some(name) = name {
            crate::format::format_add(formats, c"command", |out| out.write_all(name.to_bytes()));
        }
        let source = state.formats.borrow();
        if let Some(source) = source.as_deref() {
            crate::format::format_merge(formats, source);
        }
    }

    pub use super::Completion;
    use super::{
        CStr, Callback, ClientRef, Context, ContextRef, ContextSource, ContextWeak, ENGINE,
        ExecutionState, ProgramRef, Rc, RefCell, Target, TaskOutcome, Weak, WorkQueue, engine,
        key_event,
    };
    pub fn new_queue() -> Option<Box<dyn WorkQueue>> {
        ENGINE.get().map(|engine| (engine.queue_new)())
    }
    pub fn enqueue(client: Option<&ClientRef>, item: ContextRef) -> ContextWeak {
        (engine().queue_append)(client, item)
    }
    pub fn defer(parent: &dyn ContextSource, item: ContextRef) -> TaskOutcome {
        (engine().queue_defer)(&parent.context_ref(), item)
    }
    pub fn program_task(program: &ProgramRef, state: Option<&Rc<ExecutionState>>) -> ContextRef {
        (engine().queue_program)(program, state)
    }
    pub fn callback(name: &'static CStr, callback: Callback) -> ContextRef {
        (engine().queue_callback)(name, callback)
    }
    pub fn error_task(error: &CStr) -> ContextRef {
        (engine().queue_error)(error)
    }
    pub fn drive(client: Option<&ClientRef>) -> u32 {
        (engine().queue_drive)(client)
    }
    pub fn running() -> ContextWeak {
        (engine().queue_running)()
    }
    pub fn wait(item: &dyn ContextSource) -> (Completion, TaskOutcome) {
        (engine().queue_wait)(&item.context_ref())
    }
    pub fn cancel_background() {
        (engine().queue_cancel)();
    }
    pub fn print(
        item: &dyn ContextSource,
        write: impl FnOnce(&mut dyn std::io::Write) -> std::io::Result<()>,
    ) {
        let mut write = Some(write);
        (engine().report)(
            &item.context_ref(),
            &mut |output| write.take().expect("output is written once")(output),
            false,
        );
    }
    pub fn report_error(
        item: &dyn ContextSource,
        write: impl FnOnce(&mut dyn std::io::Write) -> std::io::Result<()>,
    ) {
        let mut write = Some(write);
        (engine().report)(
            &item.context_ref(),
            &mut |output| write.take().expect("output is written once")(output),
            true,
        );
    }
    pub fn guard(item: &dyn ContextSource, prefix: &'static CStr, flags: i32) {
        (engine().guard)(&item.context_ref(), prefix, flags);
    }
    pub fn client(item: Option<&Context>) -> Option<ClientRef> {
        item.and_then(|item| item.client.upgrade())
    }
    pub fn target_client(item: Option<&Context>) -> Option<ClientRef> {
        item.and_then(|item| item.target_client.upgrade())
    }
    pub fn target(item: &Context) -> &Target {
        &item.target
    }
    pub fn source(item: &Context) -> &Target {
        &item.source
    }
    pub fn execution_state(item: &Context) -> Rc<ExecutionState> {
        item.state.as_ref().expect("action state").clone()
    }
    pub fn event(item: &Context) -> key_event {
        execution_state(item).event.metadata_snapshot()
    }
    pub fn new_state(
        current: Option<&Target>,
        event: Option<&key_event>,
        flags: i32,
    ) -> Rc<ExecutionState> {
        let mut selected = Target::default();
        if let Some(current) = current.filter(|current| current.is_valid()) {
            selected.copy_from(current);
        } else {
            selected = Target::clear_state(0);
        }
        let event = event.map_or_else(
            || key_event {
                client: Weak::new(),
                key: crate::shared::key::KEYC_NONE as _,
                m: Default::default(),
                bytes: None,
            },
            key_event::metadata_snapshot,
        );
        Rc::new(ExecutionState {
            flags,
            formats: RefCell::new(None),
            event,
            current: RefCell::new(selected),
        })
    }
}

pub use crate::text::action_helpers::{
    append_argv, log_argv, mouse_at, mouse_pane, mouse_window, pack_argv, replace_template,
    stringify_argv,
};

pub trait ProgramSource {
    fn program_ref(&self) -> ProgramRef;
}
impl<T: Program> ProgramSource for Rc<T> {
    fn program_ref(&self) -> ProgramRef {
        self.clone()
    }
}
impl ProgramSource for ProgramRef {
    fn program_ref(&self) -> ProgramRef {
        self.clone()
    }
}

impl<T: Program + ?Sized> Program for Rc<T> {
    fn as_any(&self) -> &dyn Any {
        (**self).as_any()
    }
    fn text(&self, flags: i32) -> CString {
        (**self).text(flags)
    }
    fn substitute(&self, arguments: &Vec<CString>) -> ProgramRef {
        (**self).substitute(arguments)
    }
    fn read_only(&self) -> bool {
        (**self).read_only()
    }
    fn starts_server(&self) -> bool {
        (**self).starts_server()
    }
}

pub use crate::compat::imsg::{unpack_argv, InvalidArgv};
pub const REPEAT_STATE: i32 = 1;
pub const CONTROL_REPLY: i32 = 2;
pub const NO_HOOKS: i32 = 4;
