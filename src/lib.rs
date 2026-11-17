mod cli;
mod client;
mod default_keys;
mod getopt;

pub fn initialize() {
    tiri_core::actions::install(&tiri_cmds::ENGINE);
    tiri_core::bindings::set_default_bindings(&default_keys::DEFAULT_BINDINGS);
}

pub fn main() {
    initialize();
    cli::run();
}
