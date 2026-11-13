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
    /// Copy the weak target handles and release the borrow before callbacks run.
    pub fn current_snapshot(&self) -> Target {
        self.current.borrow().clone()
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
}

pub trait WorkQueue: Any {
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

pub type Callback = Option<Box<dyn FnOnce(&ContextRef) -> TaskOutcome>>;

pub trait PreparedAction {
    fn materialize(&mut self, arguments: &Vec<CString>) -> Result<ProgramRef, Option<CString>>;
}

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

use crate::shared::key::{key_event as KeyEvent, KeyBindingCommand};

pub struct Runtime {
    pub queue_new: fn() -> Box<dyn WorkQueue>,
    pub queue_append: fn(Option<&ClientRef>, ContextRef) -> ContextWeak,
    pub queue_insert: fn(&ContextRef, ContextRef) -> ContextWeak,
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
    pub parse_source: fn(&CStr) -> Result<ProgramRef, CString>,
    pub parse_argv: fn(&[CString]) -> Result<ProgramRef, CString>,
    pub command_names: fn() -> Vec<&'static CStr>,
    pub has_command: fn(&ContextRef) -> bool,
    pub prepare: fn(&ContextRef, u32, Option<&'static CStr>, i32, i32) -> Box<dyn PreparedAction>,
    pub config_finished: fn() -> bool,
    pub config_client: fn() -> Option<ClientRef>,
    pub config_forget: fn(&ClientRef),
    pub config_start: fn(),
    pub config_files: fn() -> &'static [CString],
    pub config_set_files: fn(Vec<CString>),
    pub config_report_missing: fn(),
    pub hooks_build: fn(),
    pub hook_add: fn(&CStr),
    pub hook_is_event: fn(&CStr) -> i32,
    pub bindings_init: fn(),
    pub binding_dispatch: fn(
        &KeyBindingCommand,
        Option<&ClientRef>,
        Option<&ContextRef>,
        Option<&KeyEvent>,
        &Target,
    ) -> ContextWeak,
    pub client_lost: fn(&ClientRef),
    pub flush: fn(),
    pub target_resolve: fn(&mut Target, Option<&ContextRef>, Option<&CStr>, u32, i32) -> i32,
}

static RUNTIME: std::sync::OnceLock<&'static Runtime> = std::sync::OnceLock::new();
pub fn install(runtime: &'static Runtime) {
    let installed = RUNTIME.get_or_init(|| runtime);
    assert!(
        std::ptr::eq(*installed, runtime),
        "action runtime already installed"
    );
}
pub fn runtime() -> &'static Runtime {
    RUNTIME.get().expect("action runtime is installed")
}

pub fn client_lost(client: &ClientRef) {
    (runtime().client_lost)(client);
}
pub fn flush() {
    (runtime().flush)();
}
pub fn build_hooks() {
    (runtime().hooks_build)();
}
pub fn initialize_bindings() {
    (runtime().bindings_init)();
}
pub fn command_names() -> Vec<&'static CStr> {
    (runtime().command_names)()
}

pub mod sources {
    use super::{CStr, CString, ClientRef, ExecutionState, ProgramRef, Rc, queue, runtime};
    pub fn compile(source: &CStr) -> Result<ProgramRef, CString> {
        (runtime().parse_source)(source)
    }
    pub fn compile_argv(argv: &[CString]) -> Result<ProgramRef, CString> {
        (runtime().parse_argv)(argv)
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

pub mod config {
    use super::{CString, ClientRef, RUNTIME, runtime};
    pub fn configuration_finished() -> bool {
        RUNTIME
            .get()
            .is_some_and(|runtime| (runtime.config_finished)())
    }
    pub fn configuration_client() -> Option<ClientRef> {
        RUNTIME.get().and_then(|runtime| (runtime.config_client)())
    }
    pub fn forget_configuration_client(client: &ClientRef) {
        if let Some(runtime) = RUNTIME.get() {
            (runtime.config_forget)(client);
        }
    }
    pub fn start_configuration() {
        (runtime().config_start)();
    }
    pub fn configuration_files() -> &'static [CString] {
        RUNTIME
            .get()
            .map_or(&[], |runtime| (runtime.config_files)())
    }
    pub fn set_configuration_files(files: Vec<CString>) {
        (runtime().config_set_files)(files);
    }
    pub fn report_missing_configuration() {
        (runtime().config_report_missing)();
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
        CStr, Callback, ClientRef, Context, ContextRef, ContextSource, ContextWeak, ExecutionState,
        ProgramRef, RUNTIME, Rc, RefCell, Target, TaskOutcome, Weak, WorkQueue, key_event, runtime,
    };
    pub fn new_queue() -> Option<Box<dyn WorkQueue>> {
        RUNTIME.get().map(|runtime| (runtime.queue_new)())
    }
    pub fn enqueue(client: Option<&ClientRef>, item: ContextRef) -> ContextWeak {
        (runtime().queue_append)(client, item)
    }
    pub fn insert_after(anchor: &dyn ContextSource, item: ContextRef) -> ContextWeak {
        (runtime().queue_insert)(&anchor.context_ref(), item)
    }
    pub fn program_task(program: &ProgramRef, state: Option<&Rc<ExecutionState>>) -> ContextRef {
        (runtime().queue_program)(program, state)
    }
    pub fn callback(name: &'static CStr, callback: Callback) -> ContextRef {
        (runtime().queue_callback)(name, callback)
    }
    pub fn error_task(error: &CStr) -> ContextRef {
        (runtime().queue_error)(error)
    }
    pub fn drive(client: Option<&ClientRef>) -> u32 {
        (runtime().queue_drive)(client)
    }
    pub fn running() -> ContextWeak {
        (runtime().queue_running)()
    }
    pub fn wait(item: &dyn ContextSource) -> (Completion, TaskOutcome) {
        (runtime().queue_wait)(&item.context_ref())
    }
    pub fn cancel_background() {
        (runtime().queue_cancel)();
    }
    pub fn print(
        item: &dyn ContextSource,
        write: impl FnOnce(&mut dyn std::io::Write) -> std::io::Result<()>,
    ) {
        let mut write = Some(write);
        (runtime().report)(
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
        (runtime().report)(
            &item.context_ref(),
            &mut |output| write.take().expect("output is written once")(output),
            true,
        );
    }
    pub fn guard(item: &dyn ContextSource, prefix: &'static CStr, flags: i32) {
        (runtime().guard)(&item.context_ref(), prefix, flags);
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

pub fn dispatch_binding(
    binding: KeyBindingCommand,
    item: Option<&dyn ContextSource>,
    client: Option<&ClientRef>,
    event: Option<&key_event>,
    target: &Target,
) -> ContextWeak {
    (runtime().binding_dispatch)(
        &binding,
        client,
        item.map(|item| item.context_ref()).as_ref(),
        event,
        target,
    )
}

pub use crate::compat::imsg::{unpack_argv, InvalidArgv};
pub const CONTROL_REPLY: i32 = 2;

pub fn register_hook_event(name: &CStr) {
    (runtime().hook_add)(name);
}
pub fn is_hook_event(name: &CStr) -> i32 {
    (runtime().hook_is_event)(name)
}

pub fn resolve_target(
    target: &mut Target,
    item: Option<&ContextRef>,
    text: Option<&CStr>,
    kind: u32,
    flags: i32,
) -> i32 {
    (runtime().target_resolve)(target, item, text, kind, flags)
}
