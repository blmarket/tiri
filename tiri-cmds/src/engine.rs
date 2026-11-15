use crate::shared::command::cmd_parse_input;
use std::ffi::CStr;
use tiri_core::actions::{CommandEngine, ProgramRef};

pub static ENGINE: CommandEngine = CommandEngine {
    parse: |bytes, origin| {
        let mut pi = cmd_parse_input {
            flags: origin.flags,
            file: origin.file.map(CStr::to_owned),
            line: origin.line,
            format: None,
            c: origin
                .client
                .map_or_else(std::rc::Weak::new, std::rc::Rc::downgrade),
            fs: Default::default(),
        };
        let parsed = if origin.file.is_some() {
            crate::cmd::parse::cmd_parse_from_file(bytes, Some(&mut pi))
        } else {
            crate::cmd::parse::cmd_parse_from_buffer(bytes, Some(&mut pi))
        };
        parsed.map(|program| program as ProgramRef)
    },
    parse_argv: |argv| {
        crate::cmd::parse::cmd_parse_from_argv(argv).map(|program| program as ProgramRef)
    },
    command_names: || &crate::catalog::CMD_NAMES,
    run: crate::cmd::run::run,
};
