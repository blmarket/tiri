pub mod base64;
mod osc7501;
mod parser;

pub use osc7501::{ProgramState, ProgramStatus};
pub use parser::{
    INPUT_BUF_DEFAULT_SIZE, INPUT_END_BEL, INPUT_END_ST, InputEndType, Osc133Command, Parser,
    SgrAttr, SgrColor, SgrOp, VtHost, WinOp, buffer_size, set_buffer_size,
};

mod terminal;
pub use terminal::{
    ReplyError, TerminalHost, TerminalReplyKind, TerminalReplyState, TerminalReport,
    parse_terminal_reply,
};
