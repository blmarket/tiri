use crate::shared::client::{ClientRef, ClientWeak};
use crate::shared::key::key_event;
pub use crate::shared::target::Target;
use std::any::Any;
use std::ffi::{CStr, CString};
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

pub trait Program: Any {
    fn as_any(&self) -> &dyn Any;
    fn text(&self, flags: i32) -> CString;
    fn substitute(&self, arguments: &Vec<CString>) -> ProgramRef;
    fn read_only(&self) -> bool;
    fn starts_server(&self) -> bool;
}
pub type ProgramRef = Rc<dyn Program>;
pub const PROGRAM_ARGUMENT: i32 = 4;

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

pub struct Context {
    pub client: ClientWeak,
    pub current: Target,
    pub event: Option<key_event>,
    pub flags: i32,
    pub formats: Option<Box<crate::shared::format::format_tree>>,
}

impl Context {
    pub fn new(
        client: ClientWeak,
        current: Option<&Target>,
        event: Option<&key_event>,
        flags: i32,
    ) -> Self {
        let mut selected = Target::default();
        if let Some(current) = current.filter(|current| current.is_valid()) {
            selected.copy_from(current);
        } else {
            selected = Target::clear_state(0);
        }
        Self {
            client,
            current: selected,
            event: event.map(key_event::metadata_snapshot),
            flags,
            formats: None,
        }
    }

    pub fn subcontext(&self, current: Option<&Target>) -> Self {
        let mut next = Self::new(
            self.client.clone(),
            current.or(Some(&self.current)),
            self.event.as_ref(),
            self.flags,
        );
        if let Some(formats) = self.formats.as_deref() {
            next.add_formats(formats);
        }
        next
    }

    pub fn add_format(&mut self, key: &CStr, value: &CStr) {
        let formats = self.formats.get_or_insert_with(|| {
            crate::format::format_create(None, None, crate::shared::format::FORMAT_NONE, 0)
        });
        crate::format::format_add_cstr(formats, key, value);
    }

    pub fn add_formats(&mut self, ft: &crate::shared::format::format_tree) {
        let formats = self.formats.get_or_insert_with(|| {
            crate::format::format_create(None, None, crate::shared::format::FORMAT_NONE, 0)
        });
        crate::format::format_merge(formats, ft);
    }
}

pub struct CmdFailed;
pub type CmdResult = Result<(), CmdFailed>;

pub struct CommandEngine {
    pub parse: fn(&[u8], ParseOrigin<'_>) -> Result<ProgramRef, CString>,
    pub parse_argv: fn(&[CString]) -> Result<ProgramRef, CString>,
    pub command_names: fn() -> &'static [&'static CStr],
    pub run: fn(ProgramRef, Context) -> Pin<Box<dyn Future<Output = CmdResult>>>,
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

pub fn command_names() -> &'static [&'static CStr] {
    (engine().command_names)()
}

pub fn run(program: ProgramRef, context: Context) -> Pin<Box<dyn Future<Output = CmdResult>>> {
    (engine().run)(program, context)
}

pub mod sources {
    use super::{CStr, CString, ClientRef, PARSE_ONEGROUP, ParseOrigin, ProgramRef, engine};
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
