pub use tiri_cmds::{arguments, cfg, cmd, hooks, key_bindings};
pub use tiri_core::*;

pub fn initialize() {
    tiri_cmds::initialize();
}

pub fn main() {
    initialize();
    tiri_core::tmux::main();
}

pub mod shared {
    pub use tiri_cmds::shared::*;
}
