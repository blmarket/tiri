mod default_keys;

pub fn initialize() {
    tiri_core::actions::install(&tiri_cmds::ENGINE);
    tiri_core::bindings::set_default_bindings(&default_keys::DEFAULT_BINDINGS);
}

pub fn main() {
    initialize();
    tiri_core::tmux::main();
}
