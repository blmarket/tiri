pub use tiri_cmds::{arguments, cfg, cmd, hooks, key_bindings};
pub use tiri_core::{
    actions, alerts, bindings, bracketed_paste, client, compat, control, control_notify, environ,
    events, events_payload, ffi, file, format, format_draw, fuzzy, grid, hyperlinks, input,
    input_keys, job, json, key_string, libc, log, menu, mode_tree, monitor, names, options, osdep,
    paste, plugin, prompt, prompt_history, r#proc, reactor, refbox, regsub, resize, screen,
    screen_redraw, screen_write, server, server_acl, server_client, server_fn, session,
    session_group, sort, spawn, status, style, text, tiri_buffer, tiri_rt, tiri_sys, tmux, tty,
    window, window_border, window_buffer, window_client, window_clock, window_copy,
    window_customize, window_pane, window_panes, window_switch, window_tree, window_visible,
    winlink,
};

pub fn initialize() {
    tiri_cmds::initialize();
}

pub fn main() {
    initialize();
    tiri_core::tmux::main();
}

pub mod shared {
    pub use tiri_cmds::shared::{
        abi, account, alerts, borders, client, colour, command, control, control_character,
        display, environment, errno, event, events, format, grid, hyperlinks, input, job, json,
        key, layout, limits, menu, mouse, pane, paste, posix_io, posix_terminal, process, prompt,
        rc, redraw, screen, screen_write, server_acl, session, signal, socket, sort, spawn, status,
        stdio, style, terminal, time, tree, tty, utf8, vis, window,
    };
}
