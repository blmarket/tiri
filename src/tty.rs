use crate::src::reactor::Timer;
use hmux_rt::{AsyncRead as _, AsyncWrite as _};
// Copy a terminal field and release its component borrow before a nested output
// operation. Writes evaluate their source before borrowing the destination.
macro_rules! terminal_value {
    ($owner:expr, $field:ident $(.$member:ident)*) => {{
        let terminal = $owner.borrow_terminal();
        terminal.$field$(.$member)*
    }};
}
macro_rules! terminal_set {
    ($owner:expr, $field:ident $(.$member:ident)*, $operator:tt, $value:expr) => {{
        let value = $value;
        let mut terminal = $owner.borrow_terminal_mut();
        terminal.$field$(.$member)* $operator value;
    }};
}
pub(crate) use terminal_set;
pub(crate) use terminal_value;

mod input;
pub use input::TerminalInput;
mod output;
pub use output::*;

use crate::src::ffi::libc::{abs, getpid, strcmp, strncmp, time};
use crate::src::ffi::resolv::__b64_ntop;
use crate::src::format::bytes::{format_cstring, xformat};
use crate::src::grid::{grid_cells_equal, grid_default_cell};
use crate::src::hyperlinks::hyperlinks_get;
use crate::src::log::{fatalx, log_cstr, log_cstr_n, log_debug, log_get_level};
use crate::src::options::{options_get_number, options_get_string};
use crate::src::reactor;
use crate::src::reactor::{evbuffer_add, evbuffer_drain, evbuffer_get_length, evbuffer_new};
use crate::src::screen::screen_mode_display;
use crate::src::server::clients;
use crate::src::server_client::Client as _;
use crate::src::server_fn::server_redraw_client;
use crate::src::session::Session;
use crate::src::shared::abi::*;
use crate::src::shared::client::{ClientRef, ClientWeak};
use crate::src::shared::client::{
    CLIENT_ALLREDRAWFLAGS, CLIENT_REDRAWSTATUS, CLIENT_REDRAWWINDOW, CLIENT_SUSPENDED,
    CLIENT_TERMINAL, CLIENT_UTF8,
};
use crate::src::shared::colour::*;
use crate::src::shared::colour::{COLOUR_FLAG_256, COLOUR_FLAG_RGB, COLOUR_FLAG_THEME};
use crate::src::shared::display::*;
use crate::src::shared::grid::*;
use crate::src::shared::limits::UINT_MAX;
use crate::src::shared::pane::window_pane;
use crate::src::shared::posix_io::{O_CREAT, O_TRUNC, O_WRONLY};
use crate::src::shared::posix_terminal::{winsize, ICRNL, ONLCR, OPOST, TCSANOW, VMIN, VTIME};
use crate::src::shared::screen::{
    screen, ScreenMode, ALL_MODES, ALL_MOUSE_MODES, CURSOR_MODES, MODE_CURSOR,
    MODE_CURSOR_BLINKING, MODE_CURSOR_BLINKING_SET, MODE_CURSOR_VERY_VISIBLE, MODE_MOUSE_ALL,
    MODE_MOUSE_BUTTON, MODE_MOUSE_STANDARD,
};
use crate::src::shared::terminal::*;
use crate::src::shared::tty::*;
pub use crate::src::shared::tty::{tty, tty_ctx, tty_ctx_set_client_cb, tty_style_ctx, tty_term};
use crate::src::shared::tty::{
    TERM_256COLOURS, TERM_DECFRA, TERM_DECSLRM, TERM_NOAM, TERM_RGBCOLOURS, TERM_VT100LIKE,
    TTY_ALL_REQUEST_FLAGS, TTY_BLOCK, TTY_BLOCK_INTERVAL, TTY_CTX_CELL_INVALIDATE,
    TTY_CTX_INVISIBLE_PANES, TTY_CTX_PANE_OBSCURED, TTY_CTX_SYNC, TTY_CTX_WINDOW_BIGGER,
    TTY_CTX_WRAPPED, TTY_FREEZE, TTY_HAVEDA, TTY_HAVEDA2, TTY_HAVESYNC, TTY_HAVEXDA, TTY_NOBLOCK,
    TTY_NOCURSOR, TTY_OPENED, TTY_OSC52QUERY, TTY_QUERY_TIMEOUT, TTY_REQUEST_LIMIT, TTY_STARTED,
    TTY_SYNCING, TTY_TIMER, TTY_WAITBG, TTY_WAITFG, TTY_WINSIZEQUERY,
};
use crate::src::shared::utf8::UTF8_SIZE;
use crate::src::shared::window::WindowRef;
use crate::src::status::status_line_size;
use crate::src::style::colour::{
    colour_256to16, colour_dim, colour_find_rgb, colour_force_rgb, colour_palette_get,
    colour_split_rgb,
};
use crate::src::text::utf8::utf8_set;
use crate::src::tmux::{global_options, setblocking};
use crate::src::tty_acs::{tty_acs_get, tty_acs_needed, tty_acs_reverse_get};
use crate::src::tty_draw::tty_draw_line;
use crate::src::tty_features::tty_apply_features;
use crate::src::tty_keys::{tty_keys_build, tty_keys_free, tty_keys_next};
use crate::src::tty_term::tty_term_owner_ptr;
use crate::src::tty_term::{
    tty_term_apply_overrides, tty_term_create, tty_term_flag, tty_term_has, tty_term_number,
    tty_term_string, tty_term_string_i, tty_term_string_ii, tty_term_string_iii, tty_term_string_s,
    tty_term_string_ss,
};
use crate::src::window::{Window, WindowPane};
use std::time::Duration;

use std::ffi::CStr;

/// Output retains its client holder and cannot run callbacks or replace
/// terminfo. The existing terminfo owner therefore stays alive after this
/// immediate component borrow has ended.
pub(crate) unsafe fn terminal_term(owner: &ClientRef) -> &tty_term {
    let terminal = owner.borrow_terminal();
    let term = tty_term_owner_ptr(&terminal.term).expect("client has a terminal");
    std::ptr::NonNull::from(term).as_ref()
}

pub const TIOCGWINSZ: ::core::ffi::c_int = 0x5413 as ::core::ffi::c_int;

pub const F_SETFD: ::core::ffi::c_int = 2 as ::core::ffi::c_int;
pub const FD_CLOEXEC: ::core::ffi::c_int = 1 as ::core::ffi::c_int;

pub const IGNBRK: ::core::ffi::c_int = 0o1 as ::core::ffi::c_int;
pub const ISTRIP: ::core::ffi::c_int = 0o40 as ::core::ffi::c_int;
pub const INLCR: ::core::ffi::c_int = 0o100 as ::core::ffi::c_int;
pub const IGNCR: ::core::ffi::c_int = 0o200 as ::core::ffi::c_int;

pub const IXON: ::core::ffi::c_int = 0o2000 as ::core::ffi::c_int;
pub const IXOFF: ::core::ffi::c_int = 0o10000 as ::core::ffi::c_int;
pub const IMAXBEL: ::core::ffi::c_int = 0o20000 as ::core::ffi::c_int;

pub const OCRNL: ::core::ffi::c_int = 0o10 as ::core::ffi::c_int;
pub const ONLRET: ::core::ffi::c_int = 0o40 as ::core::ffi::c_int;
pub const ISIG: ::core::ffi::c_int = 0o1 as ::core::ffi::c_int;
pub const ICANON: ::core::ffi::c_int = 0o2 as ::core::ffi::c_int;
pub const ECHO: ::core::ffi::c_int = 0o10 as ::core::ffi::c_int;
pub const ECHOE: ::core::ffi::c_int = 0o20 as ::core::ffi::c_int;
pub const ECHONL: ::core::ffi::c_int = 0o100 as ::core::ffi::c_int;
pub const ECHOCTL: ::core::ffi::c_int = 0o1000 as ::core::ffi::c_int;
pub const ECHOPRT: ::core::ffi::c_int = 0o2000 as ::core::ffi::c_int;
pub const ECHOKE: ::core::ffi::c_int = 0o4000 as ::core::ffi::c_int;
pub const IEXTEN: ::core::ffi::c_int = 0o100000 as ::core::ffi::c_int;

pub const TCOFLUSH: ::core::ffi::c_int = 1 as ::core::ffi::c_int;

static mut tty_log: Option<(hmux_rt::mio::Runtime, hmux_rt::mio::Io)> = None;
static mut tty_default_style_ctx: tty_style_ctx = {
    tty_style_ctx {
        defaults: grid_default_cell,
        palette: crate::src::shared::tty::PaletteSource::None,
        dim: 0 as u_int,
        hyperlinks: None,
    }
};
pub unsafe fn tty_create_log() {
    let mut name: [u8; 64] = [0; 64];
    xformat(
        &mut name,
        format_args!("tmux-out-{}.log", getpid() as ::core::ffi::c_long),
    );
    use hmux_rt::Runtime as _;
    tty_log = (|| -> std::io::Result<_> {
        let file = hmux_rt::unix::open(
            CStr::from_bytes_until_nul(&name).expect("xformat output is terminated"),
            O_WRONLY | O_CREAT | O_TRUNC,
            0o644,
        )?;
        let runtime = hmux_rt::mio::Runtime::new()?;
        let source = runtime.enter(|| hmux_rt::mio::Io::new(file))?;
        Ok((runtime, source))
    })()
    .ok();
}
pub unsafe fn tty_init(owner: &ClientRef) -> ::core::ffi::c_int {
    owner.initialize_terminal()
}

// Reset only after descriptor validation. This helper may run under a mapped
// Client component borrow: it never queries Client or invokes callbacks.
pub(crate) unsafe fn tty_initialize_component(
    terminal: &mut tty,
    fd: i32,
    observer: ClientWeak,
) -> i32 {
    if fd < 0
        || hmux_rt::unix::terminal_attributes(std::os::fd::BorrowedFd::borrow_raw(fd)).is_err()
    {
        return -1;
    }
    tty_keys_free(terminal);
    *terminal = tty::empty();
    terminal.client = observer;
    terminal.cstyle = SCREEN_CURSOR_DEFAULT;
    terminal.ccolour = -1;
    terminal.bg = -1;
    terminal.fg = terminal.bg;
    terminal.mouse_last_pane = -1;
    if crate::src::shared::terminal::read_attributes(fd, &mut terminal.tio) != 0 {
        return -1;
    }
    0
}
pub unsafe fn tty_resize(owner: &ClientRef) {
    let size = owner.query_terminal_size();
    let name = owner.name();
    {
        let terminal = owner;

        let (sx, sy, xpixel, ypixel) = if let Some(size) = size {
            let sx = u32::from(size.ws_col);
            let sy = u32::from(size.ws_row);
            let xpixel = u32::from(size.ws_xpixel).checked_div(sx).unwrap_or(0);
            let ypixel = u32::from(size.ws_ypixel).checked_div(sy).unwrap_or(0);
            if (xpixel == 0 || ypixel == 0)
                && terminal.borrow_terminal_mut().out.is_some()
                && terminal_value!(terminal, flags) & TTY_WINSIZEQUERY == 0
                && terminal
                    .borrow_terminal()
                    .term
                    .as_ref()
                    .is_some_and(|term| term.flags & TERM_VT100LIKE != 0)
            {
                tty_puts(terminal, c"\x1b[18t\x1b[14t");
                terminal_set!(terminal, flags, |=, TTY_WINSIZEQUERY);
            }
            (
                if sx == 0 { 80 } else { sx },
                if sy == 0 { 24 } else { sy },
                xpixel,
                ypixel,
            )
        } else {
            (80, 24, 0, 0)
        };
        log_debug(format_args!(
            "tty_resize: {} now {}x{} ({}x{})",
            log_cstr(name.as_deref().unwrap_or(c"(null)")),
            sx,
            sy,
            xpixel,
            ypixel
        ));
        tty_set_size(&mut *terminal.borrow_terminal_mut(), sx, sy, xpixel, ypixel);
        tty_invalidate(terminal);
    };
}

pub unsafe fn tty_set_size(
    mut tty: *mut tty,
    mut sx: u_int,
    mut sy: u_int,
    mut xpixel: u_int,
    mut ypixel: u_int,
) {
    (*tty).sx = sx;
    (*tty).sy = sy;
    (*tty).xpixel = xpixel;
    (*tty).ypixel = ypixel;
}
unsafe fn tty_read_callback(owner: &ClientRef, result: std::io::Result<Vec<u8>>) {
    let name = owner.name();
    let bytes = match result {
        Ok(bytes) if !bytes.is_empty() => bytes,
        result => {
            if let Err(error) = result {
                log_debug(format_args!(
                    "{}: read error: {}",
                    log_cstr(name.as_deref().unwrap_or(c"(null)")),
                    error
                ));
            } else {
                log_debug(format_args!(
                    "{}: read closed",
                    log_cstr(name.as_deref().unwrap_or(c"(null)"))
                ));
            }
            drop(owner.borrow_terminal_mut().read_task.take());
            owner.lost();
            return;
        }
    };
    let nread = bytes.len();
    let size = owner.append_terminal_input(bytes);
    log_debug(format_args!(
        "{}: read {} bytes (already {})",
        log_cstr(name.as_deref().unwrap_or(c"(null)")),
        nread,
        size
    ));
    while tty_keys_next(owner) != 0 {}
}
unsafe fn tty_timer_callback(owner: &ClientRef) {
    let name = owner.name();
    let discarded = owner.borrow_terminal().discarded;
    log_debug(format_args!(
        "{}: {} discarded",
        log_cstr(name.as_deref().unwrap_or(c"(null)")),
        discarded
    ));
    owner.update_flags(CLIENT_ALLREDRAWFLAGS as u64, 0);
    (|terminal: &ClientRef| {
        terminal.record_terminal_discard(discarded);
        if terminal_value!(terminal, discarded)
            < 1_u32.wrapping_add(
                terminal_value!(terminal, sx).wrapping_mul(terminal_value!(terminal, sy)) / 8,
            ) as usize
        {
            terminal_set!(terminal, flags, &=, !TTY_BLOCK);
            tty_invalidate(terminal);
            return;
        }
        terminal_set!(terminal, discarded, =, 0);
        let timeout = Duration::from_micros(TTY_BLOCK_INTERVAL as u64);
        terminal.borrow_terminal_mut().timer = Some(
            Timer::new(
                timeout,
                tty_client_timer_callback(terminal, tty_timer_callback),
            )
            .expect("arm timer"),
        );
    })(owner);
}

unsafe fn tty_block_maybe(terminal: &ClientRef) -> bool {
    let size = evbuffer_get_length(
        terminal
            .borrow_terminal_mut()
            .out
            .as_deref()
            .expect("open TTY buffer"),
    );
    if size == 0 {
        terminal_set!(terminal, flags, &=, !TTY_NOBLOCK);
    } else if terminal_value!(terminal, flags) & TTY_NOBLOCK != 0 {
        return false;
    }
    if size
        < 1_u32.wrapping_add(
            terminal_value!(terminal, sx)
                .wrapping_mul(terminal_value!(terminal, sy))
                .wrapping_mul(8),
        ) as usize
    {
        return false;
    }
    if terminal_value!(terminal, flags) & TTY_BLOCK != 0 {
        return true;
    }
    terminal_set!(terminal, flags, |=, TTY_BLOCK);
    log_debug(format_args!(
        "{}: can't keep up, {} discarded",
        log_cstr(terminal.name().as_deref().unwrap_or(c"(null)")),
        size
    ));
    evbuffer_drain(
        terminal
            .borrow_terminal_mut()
            .out
            .as_deref_mut()
            .expect("open TTY buffer"),
        size,
    );
    terminal.record_terminal_discard(size);
    terminal_set!(terminal, discarded, =, 0);
    let timeout = Duration::from_micros(TTY_BLOCK_INTERVAL as u64);
    terminal.borrow_terminal_mut().timer = Some(
        Timer::new(
            timeout,
            tty_client_timer_callback(terminal, tty_timer_callback),
        )
        .expect("arm timer"),
    );
    true
}

unsafe fn tty_write_callback(owner: &ClientRef, result: std::io::Result<usize>) {
    (|terminal: &ClientRef| {
        let size = evbuffer_get_length(
            terminal
                .borrow_terminal_mut()
                .out
                .as_deref()
                .expect("open TTY buffer"),
        );
        let Ok(written) = result else { return };
        evbuffer_drain(
            terminal
                .borrow_terminal_mut()
                .out
                .as_deref_mut()
                .expect("open TTY buffer"),
            written,
        );
        log_debug(format_args!(
            "{}: wrote {} bytes (of {})",
            log_cstr(terminal.name().as_deref().unwrap_or(c"(null)")),
            written,
            size
        ));
        if let Some(left) = terminal.acknowledge_terminal_redraw(written) {
            log_debug(format_args!(
                "{}: waiting for redraw, {} bytes left",
                log_cstr(terminal.name().as_deref().unwrap_or(c"(null)")),
                left
            ));
        } else if tty_block_maybe(terminal) {
            return;
        }
        if evbuffer_get_length(
            terminal
                .borrow_terminal_mut()
                .out
                .as_deref()
                .expect("open TTY buffer"),
        ) != 0
        {
            tty_start_write(&mut *terminal.borrow_terminal_mut());
        }
    })(owner);
}

fn tty_start_read(terminal: &mut tty) {
    let fd = terminal.io_fd.expect("open TTY descriptor");
    let observer = terminal.client.clone();
    crate::src::reactor::task_start(&mut terminal.read_task, move || {
        // SAFETY: the client owns this TTY descriptor until terminal cleanup.
        let source = reactor::io(unsafe { std::os::fd::BorrowedFd::borrow_raw(fd) })?;
        Ok(async move {
            loop {
                let mut bytes = vec![0; 65536];
                let result = source.read(&mut bytes).await.map(|received| {
                    bytes.truncate(received.bytes);
                    bytes
                });
                let Some(owner) = observer.upgrade() else {
                    return;
                };
                unsafe { tty_read_callback(&owner, result) };
                drop(owner);
                // Input handling can close the terminal and cancel this task.
                reactor::yield_now().await;
            }
        })
    })
    .expect("start TTY input");
}

pub(crate) fn tty_start_write(terminal: &mut tty) {
    if terminal.write_task.is_some() {
        return;
    }
    let fd = terminal.io_fd.expect("open TTY descriptor");
    let observer = terminal.client.clone();
    crate::src::reactor::task_start(&mut terminal.write_task, move || {
        // SAFETY: the client owns this TTY descriptor until terminal cleanup.
        let source = reactor::io(unsafe { std::os::fd::BorrowedFd::borrow_raw(fd) })?;
        Ok(async move {
            // Include output queued before this task runs. Capturing only the
            // first enqueue leaves an artificial backlog that can trigger TTY
            // blocking and discard the rest of a redraw on small terminals.
            let bytes = {
                let Some(owner) = observer.upgrade() else {
                    return;
                };
                let terminal = unsafe { owner.borrow_terminal() };
                reactor::buffer_prefix(terminal.out.as_deref().expect("open TTY buffer"), 65536)
            };
            let buffers = [std::io::IoSlice::new(&bytes)];
            let result = source.write(&buffers, None).await;
            if let Some(owner) = observer.upgrade() {
                // Complete this write before dispatch: the callback can
                // schedule another write if bytes remain in the buffer.
                unsafe {
                    drop(owner.borrow_terminal_mut().write_task.take());
                    tty_write_callback(&owner, result);
                }
            }
        })
    })
    .expect("start TTY output");
}

pub(crate) fn tty_client_timer_callback(
    owner: &ClientRef,
    callback: unsafe fn(&ClientRef),
) -> impl FnMut() {
    let observer = std::rc::Rc::downgrade(owner);
    move || {
        if let Some(owner) = observer.upgrade() {
            unsafe { callback(&owner) };
        }
    }
}

pub unsafe fn tty_open(owner: &ClientRef) -> Result<(), std::ffi::CString> {
    let (name, capabilities) = owner.terminal_description_source();
    let mut caps: Vec<_> = capabilities
        .iter()
        .map(|cap| cap.as_ptr().cast_mut())
        .collect();
    let term = match tty_term_create(
        owner,
        name.as_deref()
            .map_or(std::ptr::null_mut(), |name| name.as_ptr().cast_mut()),
        caps.as_mut_ptr(),
        caps.len() as u_int,
    ) {
        Ok(term) => term,
        Err(error) => {
            tty_close(owner);
            return Err(error);
        }
    };
    {
        let terminal = owner;

        terminal_set!(terminal, term, =, Some(term));
        terminal_set!(terminal, flags, |=, TTY_OPENED);
        terminal_set!(terminal, flags, &=, !(TTY_NOCURSOR | TTY_FREEZE | TTY_BLOCK | TTY_TIMER));
        let fd = terminal.terminal_fd();
        terminal_set!(terminal, io_fd, =, Some(fd));
        terminal_set!(terminal, in_0, =, Some(Box::new(TerminalInput::default())));
        terminal_set!(terminal, out, =, Some(evbuffer_new()));
    };
    tty_start_tty(owner);
    tty_keys_build(&mut *owner.borrow_terminal_mut());
    Ok(())
}
unsafe fn tty_start_timer_callback(owner: &ClientRef) {
    let name = owner.name();
    log_debug(format_args!(
        "{}: start timer fired",
        log_cstr(name.as_deref().unwrap_or(c"(null)"))
    ));
    let no_attributes =
        owner.borrow_terminal().flags & (TTY_HAVEDA | TTY_HAVEDA2 | TTY_HAVEXDA) == 0;
    if no_attributes {
        tty_update_features(owner);
    }
    let mut terminal = owner.borrow_terminal_mut();
    terminal.flags |= TTY_ALL_REQUEST_FLAGS;
    terminal.flags &= !(TTY_WAITBG | TTY_WAITFG);
}
unsafe fn tty_start_start_timer(tty: &ClientRef) {
    let tv = Duration::from_secs(TTY_QUERY_TIMEOUT as u64);
    log_debug(format_args!(
        "{}: start timer started",
        log_cstr(tty.name().as_deref().unwrap_or(c"(null)"))
    ));
    drop(tty.borrow_terminal_mut().start_timer.take());
    tty.borrow_terminal_mut().start_timer = Some(
        Timer::new(tv, tty_client_timer_callback(tty, tty_start_timer_callback))
            .expect("arm timer"),
    );
}
pub unsafe fn tty_start_tty(owner: &ClientRef) {
    {
        let tty = owner;

        let fd = tty.terminal_fd();

        let mut tio: termios = termios {
            c_iflag: 0,
            c_oflag: 0,
            c_cflag: 0,
            c_lflag: 0,
            c_line: 0,
            c_cc: [0; 32],
            c2rust_unnamed: termios_input_speed { __ispeed: 0 },
            c2rust_unnamed_0: termios_output_speed { __ospeed: 0 },
        };
        let mut i: u_int = 0;
        setblocking(fd, 0 as ::core::ffi::c_int);
        tty_start_read(&mut *tty.borrow_terminal_mut());
        tio = terminal_value!(tty, tio);
        tio.c_iflag &= !(IXON | IXOFF | ICRNL | INLCR | IGNCR | IMAXBEL | ISTRIP) as tcflag_t;
        tio.c_iflag |= IGNBRK as tcflag_t;
        tio.c_oflag &= !(OPOST | ONLCR | OCRNL | ONLRET) as tcflag_t;
        tio.c_lflag &=
            !(IEXTEN | ICANON | ECHO | ECHOE | ECHONL | ECHOCTL | ECHOPRT | ECHOKE | ISIG)
                as tcflag_t;
        tio.c_cc[VMIN as usize] = 1 as cc_t;
        tio.c_cc[VTIME as usize] = 0 as cc_t;
        if crate::src::shared::terminal::set_attributes(fd, TCSANOW, &tio)
            == 0 as ::core::ffi::c_int
        {
            let _ =
                hmux_rt::unix::discard_terminal(std::os::fd::BorrowedFd::borrow_raw(fd), TCOFLUSH);
        }
        if options_get_number(global_options, c"clear-on-attach") != 0 {
            tty_putcode(tty, TTYC_SMCUP);
            tty_putcode(tty, TTYC_CLEAR);
        } else {
            tty_putcode_ii(
                tty,
                TTYC_CSR,
                0 as ::core::ffi::c_int,
                terminal_value!(tty, sy).wrapping_sub(1 as u_int) as ::core::ffi::c_int,
            );
            tty_putcode_ii(
                tty,
                TTYC_CUP,
                0 as ::core::ffi::c_int,
                terminal_value!(tty, sy).wrapping_sub(1 as u_int) as ::core::ffi::c_int,
            );
            if tty_term_has(terminal_term(tty), TTYC_INDN) != 0 {
                tty_putcode_i(
                    tty,
                    TTYC_INDN,
                    terminal_value!(tty, sy).wrapping_add(1 as u_int) as ::core::ffi::c_int,
                );
            } else if tty_term_has(terminal_term(tty), TTYC_IND) != 0 {
                i = 0 as u_int;
                while i < terminal_value!(tty, sy).wrapping_add(1 as u_int) {
                    tty_putcode(tty, TTYC_IND);
                    i = i.wrapping_add(1);
                }
            } else {
                tty_putcode(tty, TTYC_CLEAR);
            }
        }
        tty_putcode(tty, TTYC_SMKX);
        if {
            let utf8 = tty.flags() & CLIENT_UTF8 as u64 != 0;
            tty_acs_needed(Some(tty.borrow_terminal()), utf8)
        } != 0
        {
            log_debug(format_args!(
                "{}: using capabilities for ACS",
                log_cstr(tty.name().as_deref().unwrap_or(c"(null)"))
            ));
            tty_putcode(tty, TTYC_ENACS);
        } else {
            log_debug(format_args!(
                "{}: using UTF-8 for ACS",
                log_cstr(tty.name().as_deref().unwrap_or(c"(null)"))
            ));
        }
        tty_putcode(tty, TTYC_CNORM);
        if tty_term_has(terminal_term(tty), TTYC_KMOUS) != 0 {
            tty_puts(tty, c"\x1B[?1000l\x1B[?1002l\x1B[?1003l");
            tty_puts(tty, c"\x1B[?1006l\x1B[?1005l");
        }
        if tty_term_has(terminal_term(tty), TTYC_ENBP) != 0 {
            tty_putcode(tty, TTYC_ENBP);
        }
        if terminal_term(tty).flags & TERM_VT100LIKE != 0 {
            tty_puts(tty, c"\x1B[?2031h\x1B[?996n");
        }
        tty_start_start_timer(tty);
        terminal_set!(tty, flags, |=, TTY_STARTED);
        tty_invalidate(tty);
        if terminal_value!(tty, ccolour) != -(1 as ::core::ffi::c_int) {
            tty_force_cursor_colour(tty, -(1 as ::core::ffi::c_int));
        }
        terminal_set!(tty, mouse_drag_flag, =, 0 as ::core::ffi::c_int);
    };
    let retired = owner.borrow_terminal_mut().mouse_drag_update.take();
    drop(retired);
    let retired = owner.borrow_terminal_mut().mouse_drag_release.take();
    drop(retired);
}
pub unsafe fn tty_send_requests(tty: &ClientRef) {
    if !terminal_value!(tty, flags) & TTY_STARTED != 0 {
        return;
    }
    if terminal_term(tty).flags & TERM_VT100LIKE != 0 {
        if !terminal_value!(tty, flags) & TTY_HAVEDA != 0 {
            tty_puts(tty, c"\x1B[c");
        }
        if !terminal_value!(tty, flags) & TTY_HAVEDA2 != 0 {
            tty_puts(tty, c"\x1B[>c");
        }
        if !terminal_value!(tty, flags) & TTY_HAVEXDA != 0 {
            tty_puts(tty, c"\x1B[>q");
        }
        if !terminal_value!(tty, flags) & TTY_HAVESYNC != 0 {
            tty_puts(tty, c"\x1B[?2026$p");
        }
        tty_puts(tty, c"\x1B]10;?\x1B\\\x1B]11;?\x1B\\");
        terminal_set!(tty, flags, |=, TTY_WAITBG | TTY_WAITFG);
    } else {
        terminal_set!(tty, flags, |=, TTY_ALL_REQUEST_FLAGS);
    }
    terminal_set!(tty, last_requests, =, time(::core::ptr::null_mut::<time_t>()));
}
pub unsafe fn tty_repeat_requests(tty: &ClientRef, mut force: ::core::ffi::c_int) {
    let mut t: time_t = time(::core::ptr::null_mut::<time_t>());
    let mut n: u_int = (t - terminal_value!(tty, last_requests)) as u_int;
    if !terminal_value!(tty, flags) & TTY_STARTED != 0 {
        return;
    }
    if force == 0 && n <= TTY_REQUEST_LIMIT as u_int {
        log_debug(format_args!(
            "{}: not repeating requests ({} seconds)",
            log_cstr(tty.name().as_deref().unwrap_or(c"(null)")),
            (n) as u32
        ));
        return;
    }
    log_debug(format_args!(
        "{}: {}repeating requests ({} seconds)",
        log_cstr(tty.name().as_deref().unwrap_or(c"(null)")),
        log_cstr(if force != 0 { c"(force) " } else { c"" }),
        (n) as u32
    ));
    terminal_set!(tty, last_requests, =, t);
    if terminal_term(tty).flags & TERM_VT100LIKE != 0 {
        tty_puts(tty, c"\x1B]10;?\x1B\\\x1B]11;?\x1B\\");
        terminal_set!(tty, flags, |=, TTY_WAITBG | TTY_WAITFG);
    }
    tty_start_start_timer(tty);
}
pub unsafe fn tty_stop_tty(owner: &ClientRef) {
    (|tty: &ClientRef| {
        let fd = tty.terminal_fd();

        if terminal_value!(tty, flags) & TTY_STARTED == 0 {
            return;
        }

        let mut ws: winsize = winsize {
            ws_row: 0,
            ws_col: 0,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        terminal_set!(tty, flags, &=, !TTY_STARTED);
        drop(tty.borrow_terminal_mut().start_timer.take());
        drop(tty.borrow_terminal_mut().clipboard_timer.take());
        drop(tty.borrow_terminal_mut().timer.take());
        terminal_set!(tty, flags, &=, !TTY_BLOCK);
        drop(tty.borrow_terminal_mut().read_task.take());
        drop(tty.borrow_terminal_mut().write_task.take());
        if crate::src::shared::terminal::read_size(fd, &mut ws) == -(1 as ::core::ffi::c_int) {
            return;
        }
        let saved_termios = terminal_value!(tty, tio);
        if crate::src::shared::terminal::set_attributes(fd, TCSANOW, &saved_termios)
            == -(1 as ::core::ffi::c_int)
        {
            return;
        }
        tty_raw(
            fd,
            CStr::from_ptr(
                tty_term_string_ii(
                    terminal_term(tty),
                    TTYC_CSR,
                    0 as ::core::ffi::c_int,
                    ws.ws_row as ::core::ffi::c_int - 1 as ::core::ffi::c_int,
                )
                .as_ptr(),
            ),
        );
        if {
            let utf8 = tty.flags() & CLIENT_UTF8 as u64 != 0;
            tty_acs_needed(Some(tty.borrow_terminal()), utf8)
        } != 0
        {
            tty_raw(
                fd,
                CStr::from_ptr(tty_term_string(terminal_term(tty), TTYC_RMACS).as_ptr()),
            );
        }
        tty_raw(
            fd,
            CStr::from_ptr(tty_term_string(terminal_term(tty), TTYC_SGR0).as_ptr()),
        );
        tty_raw(
            fd,
            CStr::from_ptr(tty_term_string(terminal_term(tty), TTYC_RMKX).as_ptr()),
        );
        if options_get_number(global_options, c"clear-on-attach") != 0 {
            tty_raw(
                fd,
                CStr::from_ptr(tty_term_string(terminal_term(tty), TTYC_CLEAR).as_ptr()),
            );
        }
        if terminal_value!(tty, cstyle) as ::core::ffi::c_uint
            != SCREEN_CURSOR_DEFAULT as ::core::ffi::c_int as ::core::ffi::c_uint
        {
            if tty_term_has(terminal_term(tty), TTYC_SE) != 0 {
                tty_raw(
                    fd,
                    CStr::from_ptr(tty_term_string(terminal_term(tty), TTYC_SE).as_ptr()),
                );
            } else if tty_term_has(terminal_term(tty), TTYC_SS) != 0 {
                tty_raw(
                    fd,
                    CStr::from_ptr(
                        tty_term_string_i(terminal_term(tty), TTYC_SS, 0 as ::core::ffi::c_int)
                            .as_ptr(),
                    ),
                );
            }
        }
        if terminal_value!(tty, ccolour) != -(1 as ::core::ffi::c_int) {
            tty_raw(
                fd,
                CStr::from_ptr(tty_term_string(terminal_term(tty), TTYC_CR).as_ptr()),
            );
        }
        tty_raw(
            fd,
            CStr::from_ptr(tty_term_string(terminal_term(tty), TTYC_CNORM).as_ptr()),
        );
        if tty_term_has(terminal_term(tty), TTYC_KMOUS) != 0 {
            tty_raw(fd, c"\x1B[?1000l\x1B[?1002l\x1B[?1003l");
            tty_raw(fd, c"\x1B[?1006l\x1B[?1005l");
        }
        if tty_term_has(terminal_term(tty), TTYC_DSBP) != 0 {
            tty_raw(
                fd,
                CStr::from_ptr(tty_term_string(terminal_term(tty), TTYC_DSBP).as_ptr()),
            );
        }
        tty_raw(
            fd,
            CStr::from_ptr(tty_term_string(terminal_term(tty), TTYC_DSESC).as_ptr()),
        );
        tty_raw(
            fd,
            CStr::from_ptr(tty_term_string(terminal_term(tty), TTYC_DSFCS).as_ptr()),
        );
        tty_raw(
            fd,
            CStr::from_ptr(tty_term_string(terminal_term(tty), TTYC_DSEKS).as_ptr()),
        );
        if terminal_term(tty).flags & TERM_DECSLRM != 0 {
            tty_raw(
                fd,
                CStr::from_ptr(tty_term_string(terminal_term(tty), TTYC_DSMG).as_ptr()),
            );
        }
        if options_get_number(global_options, c"clear-on-attach") != 0 {
            tty_raw(
                fd,
                CStr::from_ptr(tty_term_string(terminal_term(tty), TTYC_RMCUP).as_ptr()),
            );
        } else {
            tty_raw(
                fd,
                CStr::from_ptr(tty_term_string(terminal_term(tty), TTYC_CLEAR).as_ptr()),
            );
        }
        if terminal_term(tty).flags & TERM_VT100LIKE != 0 {
            tty_raw(fd, c"\x1B[?2031l");
        }
        setblocking(fd, 1 as ::core::ffi::c_int);
    })(owner);
}
pub unsafe fn tty_close(owner: &ClientRef) {
    {
        let mut terminal = owner.borrow_terminal_mut();
        drop(terminal.key_timer.take());
    }
    tty_stop_tty(owner);
    let term = {
        let mut terminal = owner.borrow_terminal_mut();
        if terminal.flags & TTY_OPENED == 0 {
            return;
        }
        drop(terminal.read_task.take());
        drop(terminal.write_task.take());
        terminal.io_fd.take();
        terminal.in_0 = None;
        terminal.out = None;
        terminal.term.take()
    };
    if let Some(term) = term {
        drop(term);
    }
    let mut terminal = owner.borrow_terminal_mut();
    tty_keys_free(&mut *terminal);
    terminal.flags &= !TTY_OPENED;
}
pub unsafe fn tty_free(owner: &ClientRef) {
    tty_close(owner);
    owner.borrow_terminal_mut().r.clear();
}
pub unsafe fn tty_update_features(owner: &ClientRef) {
    let features = owner.terminal_feature_mask();
    let applied = {
        let mut terminal = owner.borrow_terminal_mut();
        tty_apply_features(
            terminal.term.as_deref_mut().expect("open terminal"),
            features,
        )
    };
    if applied.enable_utf8 {
        owner.update_flags(CLIENT_UTF8 as u64, 0);
    }
    {
        let tty = owner;

        if applied.changed {
            tty_term_apply_overrides(
                tty.borrow_terminal_mut()
                    .term
                    .as_deref_mut()
                    .expect("open terminal"),
            );
        }
        if terminal_term(tty).flags & TERM_DECSLRM != 0 {
            tty_putcode(tty, TTYC_ENMG);
        }
        if options_get_number(global_options, c"extended-keys") != 0 {
            tty_puts(tty, tty_term_string(terminal_term(tty), TTYC_ENEKS));
        }
        if options_get_number(global_options, c"focus-events") != 0 {
            tty_puts(tty, tty_term_string(terminal_term(tty), TTYC_ENFCS));
        }
        tty_puts(tty, tty_term_string(terminal_term(tty), TTYC_ENESC));
    };
    server_redraw_client(owner);
    tty_invalidate(owner);
}
pub(crate) unsafe fn tty_raw(fd: i32, text: &CStr) {
    use hmux_rt::Runtime as _;
    use std::future::{poll_fn, Future};
    use std::pin::pin;
    use std::task::Poll;
    if fd < 0 {
        return;
    }
    let bytes = text.to_bytes();
    let borrowed = std::os::fd::BorrowedFd::borrow_raw(fd);
    let Ok(was_nonblocking) = hmux_rt::unix::set_nonblocking(borrowed, true) else {
        return;
    };
    // Teardown is synchronous and may run inside an application callback. Use
    // an independent driver so it never reenters the application's runtime.
    let _ = (|| -> std::io::Result<()> {
        let mut runtime = hmux_rt::mio::Runtime::new()?;
        let (source, stop) = runtime.enter(|| -> std::io::Result<_> {
            let source = hmux_rt::mio::Io::new(borrowed.try_clone_to_owned()?)?;
            let stop =
                hmux_rt::mio::Sleep::new(std::time::Instant::now() + Duration::from_micros(500));
            Ok((source, stop))
        })?;
        runtime.block_on(async {
            let mut stop = pin!(stop);
            let send = async {
                let mut left = bytes;
                while !left.is_empty() {
                    let count = source.write(&[std::io::IoSlice::new(left)], None).await?;
                    if count == 0 {
                        return Err(std::io::ErrorKind::WriteZero.into());
                    }
                    left = &left[count..];
                }
                Ok(())
            };
            let mut send = pin!(send);
            poll_fn(|cx| {
                if let Poll::Ready(result) = send.as_mut().poll(cx) {
                    return Poll::Ready(result);
                }
                if stop.as_mut().poll(cx).is_ready() {
                    return Poll::Ready(Ok(()));
                }
                Poll::Pending
            })
            .await
        })?
    })();
    let _ = hmux_rt::unix::set_nonblocking(borrowed, was_nonblocking);
}

/// The part of the terminal that shows `window`: the window's size, cut to the
/// terminal. A larger terminal shows the outside fill beyond it, as tmux does.
unsafe fn tty_window_frame(owner: &ClientRef, window: &WindowRef) -> (u32, u32) {
    let (wx, wy) = window.size();
    let (tx, ty) = owner.terminal_size();
    (tx.min(wx), ty.wrapping_sub(status_line_size(owner)).min(wy))
}
pub unsafe fn tty_window_bigger(owner: &ClientRef) -> ::core::ffi::c_int {
    let session = owner.attached_session().upgrade().expect("live session");
    let link = session.current_winlink();
    let window = link
        .get_unchecked()
        .window_handle()
        .cloned()
        .expect("current window");
    let (sx, sy) = window.logical_size();
    let (fx, fy) = tty_window_frame(owner, &window);
    (fx < sx || fy < sy) as i32
}
pub fn tty_window_offset(tty: &tty) -> tty_window_view {
    tty_window_view {
        bigger: tty.oflag != 0,
        ox: tty.oox,
        oy: tty.ooy,
        sx: tty.osx,
        sy: tty.osy,
    }
}
/// Where a view `view` cells long rests along one axis of a window `extent`
/// cells long. It rests on the stop nearest `previous` that shows the active
/// pane's `cell` (start, length) completely, so cursor movement inside a
/// visible pane never moves it. A view too short for the cell stays within
/// the cell and moves as little as it can to keep the cursor visible.
fn tty_follow_cell(
    stops: &[u32],
    (start, length): (u32, u32),
    view: u32,
    extent: u32,
    previous: u32,
    cursor: Option<u32>,
) -> u32 {
    let last = extent.saturating_sub(view);
    let end = start.saturating_add(length);
    if let Some(stop) = stops
        .iter()
        .copied()
        .filter(|&stop| stop <= start.min(last) && end <= stop.saturating_add(view))
        .min_by_key(|&stop| (stop.abs_diff(previous), stop))
    {
        return stop;
    }
    let first = start.min(last);
    let final_offset = end.saturating_sub(view).clamp(first, last);
    let mut offset = previous.clamp(first, final_offset);
    if let Some(cursor) = cursor {
        if cursor < offset {
            offset = cursor;
        } else if cursor >= offset.saturating_add(view) {
            offset = cursor.saturating_add(1).saturating_sub(view);
        }
    }
    offset.clamp(first, final_offset)
}
unsafe fn tty_window_offset1(owner: &ClientRef) -> tty_window_view {
    let session = owner.attached_session().upgrade().expect("live session");
    let link = session.current_winlink();
    let window = link
        .get_unchecked()
        .window_handle()
        .cloned()
        .expect("current window");
    let (sx, sy) = window.logical_size();
    let (fx, fy) = tty_window_frame(owner, &window);
    if fx >= sx && fy >= sy {
        owner.reset_pan(None);
        return tty_window_view {
            bigger: false,
            ox: 0,
            oy: 0,
            sx,
            sy,
        };
    }
    let mut view = tty_window_view {
        bigger: true,
        ox: 0,
        oy: 0,
        sx: fx,
        sy: fy,
    };
    if owner.apply_pan(&window, &mut view) {
        return view;
    }
    owner.reset_pan(None);
    // Rest on a pane's first column, or the top row, that shows the active
    // pane completely, and never so far right that the view passes the last
    // pane's right border while panes are left out on the left.
    let previous = tty_window_offset(owner.borrow_terminal());
    let cells = window.pane_cells();
    let strip = cells
        .iter()
        .map(|(_, cell)| (cell.xoff as u32).saturating_add(cell.sx))
        .max()
        .unwrap_or(0);
    let content = strip.saturating_add(1).min(sx).max(view.sx);
    let active = window.active_pane().and_then(|active| {
        cells
            .iter()
            .find(|(pane, _)| std::rc::Rc::ptr_eq(pane, &active))
            .map(|&(_, cell)| (active.visible_cursor_in_window(), cell))
    });
    if let Some((cursor, cell)) = active {
        let stops = cells
            .iter()
            .map(|(_, cell)| cell.xoff as u32)
            .collect::<Vec<_>>();
        view.ox = tty_follow_cell(
            &stops,
            (cell.xoff as u32, cell.sx),
            view.sx,
            content,
            previous.ox,
            cursor.map(|(x, _)| x),
        );
        view.oy = tty_follow_cell(
            &[cell.yoff as u32],
            (cell.yoff as u32, cell.sy),
            view.sy,
            sy,
            previous.oy,
            cursor.map(|(_, y)| y),
        );
    }
    // The terminal clips only when the view leaves part of a pane out. A
    // view at the origin that shows every pane draws them where they are.
    view.bigger = view.ox != 0 || view.oy != 0 || strip > view.sx || sy > view.sy;
    view
}
pub unsafe fn tty_update_window_offset(w_owner: &WindowRef) {
    let mut c: Option<ClientRef> = None;
    let mut registry_c_owner = clients.first();
    c = registry_c_owner.clone();
    while !c.is_none() {
        if !c
            .as_ref()
            .expect("live client")
            .attached_session()
            .upgrade()
            .is_none()
            && c.as_ref()
                .expect("live client")
                .attached_session()
                .upgrade()
                .expect("live session")
                .current_winlink()
                .is_alive()
            && (c
                .as_ref()
                .expect("live client")
                .attached_session()
                .upgrade()
                .expect("live session")
                .current_winlink())
            .get_unchecked()
            .window_handle()
            .is_some_and(|owner| WindowRef::ptr_eq(owner, w_owner))
        {
            tty_update_client_offset(&c.clone().expect("live client"));
        }
        registry_c_owner =
            clients.next(registry_c_owner.as_ref().expect("current registry client"));
        c = registry_c_owner.clone();
    }
}
pub unsafe fn tty_update_client_offset(owner: &ClientRef) {
    if owner.flags() & CLIENT_TERMINAL as u64 == 0 {
        return;
    }
    let view = tty_window_offset1(owner);
    let old = {
        let mut terminal = owner.borrow_terminal_mut();
        terminal.oflag = view.bigger as i32;
        tty_window_offset(terminal)
    };
    if view.ox == old.ox && view.oy == old.oy && view.sx == old.sx && view.sy == old.sy {
        return;
    }
    log_debug(format_args!(
        "tty_update_client_offset: {} offset has changed ({},{} {}x{} -> {},{} {}x{})",
        log_cstr(owner.name().as_deref().unwrap_or(c"(null)")),
        old.ox,
        old.oy,
        old.sx,
        old.sy,
        view.ox,
        view.oy,
        view.sx,
        view.sy,
    ));
    {
        let mut terminal = owner.borrow_terminal_mut();
        terminal.oox = view.ox;
        terminal.ooy = view.oy;
        terminal.osx = view.sx;
        terminal.osy = view.sy;
    }
    owner.update_flags((CLIENT_REDRAWWINDOW | CLIENT_REDRAWSTATUS) as u64, 0);
}
fn tty_large_region(ctx: &tty_ctx) -> ::core::ffi::c_int {
    (ctx.orlower.wrapping_sub(ctx.orupper) >= ctx.sy.wrapping_div(2 as u_int)) as ::core::ffi::c_int
}
pub unsafe fn tty_fake_bce(tty: &tty, gc: &grid_cell, mut bg: u_int) -> ::core::ffi::c_int {
    if tty_term_flag(
        tty_term_owner_ptr(&tty.term).expect("client has a terminal"),
        TTYC_BCE,
    ) != 0
    {
        return 0 as ::core::ffi::c_int;
    }
    if !(bg == 8 as u_int || bg == 9 as u_int)
        || !(gc.bg == 8 as ::core::ffi::c_int || gc.bg == 9 as ::core::ffi::c_int)
    {
        return 1 as ::core::ffi::c_int;
    }
    0 as ::core::ffi::c_int
}
unsafe fn tty_redraw_region(owner: &ClientRef, ctx: &tty_ctx, s: &screen) {
    let name = owner.name();
    let mut i: u_int = 0;
    if tty_large_region(ctx) != 0 || ctx.flags & TTY_CTX_PANE_OBSCURED != 0 {
        log_debug(format_args!(
            "{}: {} large region redraw",
            "tty_redraw_region",
            log_cstr(name.as_deref().unwrap_or(c"(null)"))
        ));
        ctx.redraw_cb.as_ref().expect("non-null redraw callback")(ctx);
        return;
    }
    log_debug(format_args!(
        "{}: {} small region redraw ({}-{})",
        "tty_redraw_region",
        log_cstr(name.as_deref().unwrap_or(c"(null)")),
        { ctx.orupper },
        { ctx.orlower }
    ));
    i = ctx.orupper;
    while i <= ctx.orlower {
        tty_draw_pane(owner, ctx, s, i);
        i = i.wrapping_add(1);
    }
}
fn tty_is_visible(
    ctx: &tty_ctx,
    mut px: u_int,
    mut py: u_int,
    mut nx: u_int,
    mut ny: u_int,
) -> ::core::ffi::c_int {
    let mut xoff: u_int = (ctx.rxoff as u_int).wrapping_add(px);
    let mut yoff: u_int = (ctx.ryoff as u_int).wrapping_add(py);
    if !ctx.flags & TTY_CTX_WINDOW_BIGGER != 0 {
        return 1 as ::core::ffi::c_int;
    }
    if xoff.wrapping_add(nx) <= ctx.wox
        || xoff >= ctx.wox.wrapping_add(ctx.wsx)
        || yoff.wrapping_add(ny) <= ctx.woy
        || yoff >= ctx.woy.wrapping_add(ctx.wsy)
    {
        return 0 as ::core::ffi::c_int;
    }
    1 as ::core::ffi::c_int
}
#[derive(Debug, PartialEq, Eq)]
struct tty_clamped_line {
    skip: u_int,
    x: u_int,
    y: u_int,
    width: u_int,
}

fn tty_clamp_line(ctx: &tty_ctx, px: u_int, py: u_int, nx: u_int) -> Option<tty_clamped_line> {
    let xoff = (ctx.rxoff as u_int).wrapping_add(px) as i32;
    if tty_is_visible(ctx, px, py, nx, 1) == 0 {
        return None;
    }
    let y = (ctx.yoff as u_int).wrapping_add(py).wrapping_sub(ctx.woy);
    let (skip, x, width) = if xoff >= ctx.wox as i32
        && (xoff as u_int).wrapping_add(nx) <= ctx.wox.wrapping_add(ctx.wsx)
    {
        (
            0,
            (ctx.xoff as u_int).wrapping_add(px).wrapping_sub(ctx.wox),
            nx,
        )
    } else if xoff < ctx.wox as i32
        && (xoff as u_int).wrapping_add(nx) > ctx.wox.wrapping_add(ctx.wsx)
    {
        (ctx.wox, 0, ctx.wsx)
    } else if xoff < ctx.wox as i32 {
        let skip = ctx.wox.wrapping_sub((ctx.xoff as u_int).wrapping_add(px));
        (skip, 0, nx.wrapping_sub(skip))
    } else {
        let x = (ctx.xoff as u_int).wrapping_add(px).wrapping_sub(ctx.wox);
        (0, x, ctx.wsx.wrapping_sub(x))
    };
    if width > nx {
        unsafe {
            fatalx(|out| write!(out, "tty_clamp_line: x too big, {} > {}", width, nx));
        }
    }
    Some(tty_clamped_line { skip, x, y, width })
}
unsafe fn tty_clear_line(
    owner: &ClientRef,
    defaults: &grid_cell,
    py: u_int,
    px: u_int,
    nx: u_int,
    bg: u_int,
) {
    let name = owner.name();
    log_debug(format_args!(
        "tty_clear_line: {}, {} at {},{}",
        log_cstr(name.as_deref().unwrap_or(c"(null)")),
        nx,
        px,
        py
    ));
    if nx == 0 {
        return;
    }
    let complete = (|tty: &ClientRef| {
        if tty_fake_bce(tty.borrow_terminal(), defaults, bg) == 0 {
            if px.wrapping_add(nx) >= terminal_value!(tty, sx)
                && tty_term_has(terminal_term(tty), TTYC_EL) != 0
            {
                tty_cursor(tty, px, py);
                tty_putcode(tty, TTYC_EL);
                return true;
            }
            if px == 0 as u_int && tty_term_has(terminal_term(tty), TTYC_EL1) != 0 {
                tty_cursor(tty, px.wrapping_add(nx).wrapping_sub(1 as u_int), py);
                tty_putcode(tty, TTYC_EL1);
                return true;
            }
            if tty_term_has(terminal_term(tty), TTYC_ECH) != 0 {
                tty_cursor(tty, px, py);
                tty_putcode_i(tty, TTYC_ECH, nx as ::core::ffi::c_int);
                return true;
            }
        }

        false
    })(owner);
    if complete {
        return;
    }
    if nx != 0 {
        tty_cursor(owner, px, py);
        tty_repeat_space(owner, nx);
    }
}
unsafe fn tty_clear_pane_line(
    owner: &ClientRef,
    ctx: &tty_ctx,
    py: u_int,
    px: u_int,
    nx: u_int,
    bg: u_int,
) {
    let name = owner.name();
    log_debug(format_args!(
        "tty_clear_pane_line: {}, {} at {},{}",
        log_cstr(name.as_deref().unwrap_or(c"(null)")),
        nx,
        px,
        py
    ));
    if let Some(line) = tty_clamp_line(ctx, px, py, nx) {
        if line.width != 0 {
            tty_clear_line(
                owner,
                &ctx.style_ctx.defaults,
                line.y,
                line.x,
                line.width,
                bg,
            );
        }
    }
}
#[derive(Debug, PartialEq, Eq)]
struct tty_clamped_area {
    x: u_int,
    y: u_int,
    width: u_int,
    height: u_int,
}

fn tty_clamp_area(
    ctx: &tty_ctx,
    px: u_int,
    py: u_int,
    nx: u_int,
    ny: u_int,
) -> Option<tty_clamped_area> {
    let xoff = (ctx.rxoff as u_int).wrapping_add(px);
    let yoff = (ctx.ryoff as u_int).wrapping_add(py);
    if tty_is_visible(ctx, px, py, nx, ny) == 0 {
        return None;
    }
    let (x, width) = if xoff >= ctx.wox && xoff.wrapping_add(nx) <= ctx.wox.wrapping_add(ctx.wsx) {
        (
            (ctx.xoff as u_int).wrapping_add(px).wrapping_sub(ctx.wox),
            nx,
        )
    } else if xoff < ctx.wox && xoff.wrapping_add(nx) > ctx.wox.wrapping_add(ctx.wsx) {
        (0, ctx.wsx)
    } else if xoff < ctx.wox {
        let skip = ctx.wox.wrapping_sub((ctx.xoff as u_int).wrapping_add(px));
        (0, nx.wrapping_sub(skip))
    } else {
        let x = (ctx.xoff as u_int).wrapping_add(px).wrapping_sub(ctx.wox);
        (x, ctx.wsx.wrapping_sub(x))
    };
    if width > nx {
        unsafe {
            fatalx(|out| write!(out, "tty_clamp_area: x too big, {} > {}", width, nx));
        }
    }
    let (y, height) = if yoff >= ctx.woy && yoff.wrapping_add(ny) <= ctx.woy.wrapping_add(ctx.wsy) {
        (
            (ctx.yoff as u_int).wrapping_add(py).wrapping_sub(ctx.woy),
            ny,
        )
    } else if yoff < ctx.woy && yoff.wrapping_add(ny) > ctx.woy.wrapping_add(ctx.wsy) {
        (0, ctx.wsy)
    } else if yoff < ctx.woy {
        let skip = ctx.woy.wrapping_sub((ctx.yoff as u_int).wrapping_add(py));
        (0, ny.wrapping_sub(skip))
    } else {
        let y = (ctx.yoff as u_int).wrapping_add(py).wrapping_sub(ctx.woy);
        (y, ctx.wsy.wrapping_sub(y))
    };
    if height > ny {
        unsafe {
            fatalx(|out| write!(out, "tty_clamp_area: y too big, {} > {}", height, ny));
        }
    }
    Some(tty_clamped_area {
        x,
        y,
        width,
        height,
    })
}
unsafe fn tty_clear_area(
    owner: &ClientRef,
    ctx: &tty_ctx,
    py: u_int,
    ny: u_int,
    px: u_int,
    nx: u_int,
    bg: u_int,
) {
    let name = owner.name();
    log_debug(format_args!(
        "tty_clear_area: {}, {},{} at {},{}",
        log_cstr(name.as_deref().unwrap_or(c"(null)")),
        nx,
        ny,
        px,
        py
    ));
    if nx == 0 || ny == 0 {
        return;
    }
    let defaults = &ctx.style_ctx.defaults;
    let complete = (|tty: &ClientRef| {
        let mut tmp: [u8; 64] = [0; 64];
        if tty_fake_bce(tty.borrow_terminal(), defaults, bg) == 0 {
            if px == 0 as u_int
                && px.wrapping_add(nx) >= terminal_value!(tty, sx)
                && py.wrapping_add(ny) >= terminal_value!(tty, sy)
                && tty_term_has(terminal_term(tty), TTYC_ED) != 0
            {
                tty_cursor(tty, 0 as u_int, py);
                tty_putcode(tty, TTYC_ED);
                return true;
            }
            if terminal_term(tty).flags & TERM_DECFRA != 0
                && !(bg == 8 as u_int || bg == 9 as u_int)
            {
                xformat(
                    &mut tmp,
                    format_args!(
                        "\x1B[32;{};{};{};{}$x",
                        { py.wrapping_add(1 as u_int) },
                        { px.wrapping_add(1 as u_int) },
                        { py.wrapping_add(ny) },
                        { px.wrapping_add(nx) }
                    ),
                );
                tty_puts(
                    tty,
                    std::ffi::CStr::from_bytes_until_nul(&tmp)
                        .expect("xformat output is terminated"),
                );
                return true;
            }
            if px == 0 as u_int
                && px.wrapping_add(nx) >= terminal_value!(tty, sx)
                && ny > 2 as u_int
                && tty_term_has(terminal_term(tty), TTYC_CSR) != 0
                && tty_term_has(terminal_term(tty), TTYC_INDN) != 0
            {
                tty_region(tty, py, py.wrapping_add(ny).wrapping_sub(1 as u_int));
                tty_margin_off(tty);
                tty_putcode_i(tty, TTYC_INDN, ny as ::core::ffi::c_int);
                return true;
            }
            if nx > 2 as u_int
                && ny > 2 as u_int
                && tty_term_has(terminal_term(tty), TTYC_CSR) != 0
                && terminal_term(tty).flags & TERM_DECSLRM != 0
                && tty_term_has(terminal_term(tty), TTYC_INDN) != 0
            {
                tty_region(tty, py, py.wrapping_add(ny).wrapping_sub(1 as u_int));
                tty_margin(tty, px, px.wrapping_add(nx).wrapping_sub(1 as u_int));
                tty_putcode_i(tty, TTYC_INDN, ny as ::core::ffi::c_int);
                return true;
            }
        }

        false
    })(owner);
    if complete {
        return;
    }
    let mut yy = py;
    while yy < py.wrapping_add(ny) {
        tty_clear_line(owner, defaults, yy, px, nx, bg);
        yy = yy.wrapping_add(1);
    }
}
unsafe fn tty_clear_pane_area(
    owner: &ClientRef,
    ctx: &tty_ctx,
    mut py: u_int,
    mut ny: u_int,
    mut px: u_int,
    mut nx: u_int,
    mut bg: u_int,
) {
    if let Some(area) = tty_clamp_area(ctx, px, py, nx, ny) {
        tty_clear_area(owner, ctx, area.y, area.height, area.x, area.width, bg);
    }
}
unsafe fn tty_draw_pane(owner: &ClientRef, ctx: &tty_ctx, s: &screen, py: u_int) {
    let name = owner.name();
    log_debug(format_args!(
        "tty_draw_pane: {} {}",
        log_cstr(name.as_deref().unwrap_or(c"(null)")),
        py
    ));
    let line = if ctx.flags & TTY_CTX_WINDOW_BIGGER == 0 {
        tty_clamped_line {
            skip: 0,
            x: ctx.xoff as u_int,
            y: (ctx.yoff as u_int).wrapping_add(py),
            width: ctx.sx,
        }
    } else {
        let Some(line) = tty_clamp_line(ctx, 0, py, ctx.sx) else {
            return;
        };
        line
    };
    if line.width != 0 {
        tty_draw_line(
            owner,
            s,
            line.skip,
            py,
            line.width,
            line.x,
            line.y,
            Some(&ctx.style_ctx),
        );
    }
}
pub unsafe fn tty_cmd_redrawline(owner: &ClientRef, ctx: &tty_ctx, s: &screen) {
    if let Some(line) = tty_clamp_line(ctx, ctx.ocx, ctx.ocy, ctx.data.count()) {
        if line.width != 0 {
            tty_draw_line(
                owner,
                s,
                ctx.ocx.wrapping_add(line.skip),
                ctx.ocy,
                line.width,
                line.x,
                line.y,
                Some(&ctx.style_ctx),
            );
        }
    }
}
pub unsafe fn tty_check_codeset(utf8: bool, gc: &grid_cell) -> grid_cell {
    if gc.data.size as ::core::ffi::c_int == 1 as ::core::ffi::c_int
        && (gc.data.data[0] as ::core::ffi::c_int) < 0x7f as ::core::ffi::c_int
    {
        return *gc;
    }
    if gc.flags as ::core::ffi::c_int & GRID_FLAG_TAB != 0 {
        return *gc;
    }

    if utf8 {
        return *gc;
    }
    let mut new = *gc;
    if let Some(ch) = CStr::from_bytes_until_nul(&gc.data.data)
        .ok()
        .and_then(|text| tty_acs_reverse_get(text, gc.data.size as usize))
    {
        utf8_set(&mut new.data, ch);
        new.attr = (new.attr as ::core::ffi::c_int | GRID_ATTR_CHARSET) as u_short;
        return new;
    }
    new.data.size = gc.data.width;
    if new.data.size as ::core::ffi::c_int > UTF8_SIZE {
        new.data.size = UTF8_SIZE as u_char;
    }
    new.data.data[..new.data.size as usize].fill(b'_');
    new
}
unsafe fn tty_client_ready(ctx: &tty_ctx, owner: &ClientRef) -> bool {
    if owner.attached_session().upgrade().is_none() {
        return false;
    }
    let flags = owner.flags();
    let terminal = owner.borrow_terminal();
    if terminal.term.is_none() || flags & CLIENT_SUSPENDED as u64 != 0 {
        return false;
    }
    ctx.flags & TTY_CTX_INVISIBLE_PANES != 0
        || (flags & CLIENT_REDRAWWINDOW as u64 == 0 && terminal.flags & TTY_FREEZE == 0)
}

pub unsafe fn tty_write(mut cmdfn: impl FnMut(&ClientRef, &tty_ctx), ctx: &mut tty_ctx) {
    let Some(mut set_client_cb) = ctx.set_client_cb.take() else {
        return;
    };
    let mut cursor = clients.first();
    while let Some(owner) = cursor {
        if tty_client_ready(ctx, &owner) {
            let state = set_client_cb(ctx, &owner);
            if state == -1 {
                break;
            }
            if state != 0 {
                cmdfn(&owner, ctx);
            }
        }
        cursor = clients.next(&owner);
    }
    if ctx.set_client_cb.is_none() {
        ctx.set_client_cb = Some(set_client_cb);
    }
}

pub unsafe fn tty_cmd_insertcharacter(owner: &ClientRef, ctx: &tty_ctx, s: &screen) {
    let complete = (|tty: &ClientRef| {
        if ctx.flags & TTY_CTX_WINDOW_BIGGER != 0
            || !(ctx.xoff == 0 as ::core::ffi::c_int && ctx.sx >= terminal_value!(tty, sx))
            || tty_fake_bce(tty.borrow_terminal(), &ctx.style_ctx.defaults, ctx.bg) != 0
            || tty_term_has(terminal_term(tty), TTYC_ICH) == 0
                && tty_term_has(terminal_term(tty), TTYC_ICH1) == 0
        {
            return false;
        }
        tty_default_attributes(tty, ctx.bg, Some(&ctx.style_ctx));
        tty_cursor_pane(tty, ctx, ctx.ocx, ctx.ocy);
        tty_emulate_repeat(tty, TTYC_ICH, TTYC_ICH1, ctx.data.count());

        true
    })(owner);
    if !complete {
        tty_draw_pane(owner, ctx, s, ctx.ocy);
    }
}
pub unsafe fn tty_cmd_deletecharacter(owner: &ClientRef, ctx: &tty_ctx, s: &screen) {
    let complete = (|tty: &ClientRef| {
        if ctx.flags & TTY_CTX_WINDOW_BIGGER != 0
            || !(ctx.xoff == 0 as ::core::ffi::c_int && ctx.sx >= terminal_value!(tty, sx))
            || tty_fake_bce(tty.borrow_terminal(), &ctx.style_ctx.defaults, ctx.bg) != 0
            || tty_term_has(terminal_term(tty), TTYC_DCH) == 0
                && tty_term_has(terminal_term(tty), TTYC_DCH1) == 0
        {
            return false;
        }
        tty_default_attributes(tty, ctx.bg, Some(&ctx.style_ctx));
        tty_cursor_pane(tty, ctx, ctx.ocx, ctx.ocy);
        tty_emulate_repeat(tty, TTYC_DCH, TTYC_DCH1, ctx.data.count());

        true
    })(owner);
    if !complete {
        tty_draw_pane(owner, ctx, s, ctx.ocy);
    }
}
pub unsafe fn tty_cmd_clearcharacter(owner: &ClientRef, ctx: &tty_ctx) {
    {
        let tty = owner;

        tty_default_attributes(tty, ctx.bg, Some(&ctx.style_ctx));
    };
    tty_clear_pane_line(owner, ctx, ctx.ocy, ctx.ocx, ctx.data.count(), ctx.bg);
}
pub unsafe fn tty_cmd_insertline(owner: &ClientRef, ctx: &tty_ctx, s: &screen) {
    let complete = (|tty: &ClientRef| {
        if ctx.flags & TTY_CTX_WINDOW_BIGGER != 0
            || !(ctx.xoff == 0 as ::core::ffi::c_int && ctx.sx >= terminal_value!(tty, sx))
            || tty_fake_bce(tty.borrow_terminal(), &ctx.style_ctx.defaults, ctx.bg) != 0
            || tty_term_has(terminal_term(tty), TTYC_CSR) == 0
            || tty_term_has(terminal_term(tty), TTYC_IL1) == 0
            || ctx.sx == 1 as u_int
            || ctx.sy == 1 as u_int
        {
            return false;
        }
        tty_default_attributes(tty, ctx.bg, Some(&ctx.style_ctx));
        tty_region_pane(tty, ctx, ctx.orupper, ctx.orlower);
        tty_margin_off(tty);
        tty_cursor_pane(tty, ctx, ctx.ocx, ctx.ocy);
        tty_emulate_repeat(tty, TTYC_IL, TTYC_IL1, ctx.data.count());
        terminal_set!(tty, cy, =, UINT_MAX as u_int);
        terminal_set!(tty, cx, =, terminal_value!(tty, cy));

        true
    })(owner);
    if !complete {
        tty_redraw_region(owner, ctx, s);
    }
}
pub unsafe fn tty_cmd_deleteline(owner: &ClientRef, ctx: &tty_ctx, s: &screen) {
    let complete = (|tty: &ClientRef| {
        if ctx.flags & TTY_CTX_WINDOW_BIGGER != 0
            || !(ctx.xoff == 0 as ::core::ffi::c_int && ctx.sx >= terminal_value!(tty, sx))
            || tty_fake_bce(tty.borrow_terminal(), &ctx.style_ctx.defaults, ctx.bg) != 0
            || tty_term_has(terminal_term(tty), TTYC_CSR) == 0
            || tty_term_has(terminal_term(tty), TTYC_DL1) == 0
            || ctx.sx == 1 as u_int
            || ctx.sy == 1 as u_int
        {
            return false;
        }
        tty_default_attributes(tty, ctx.bg, Some(&ctx.style_ctx));
        tty_region_pane(tty, ctx, ctx.orupper, ctx.orlower);
        tty_margin_off(tty);
        tty_cursor_pane(tty, ctx, ctx.ocx, ctx.ocy);
        tty_emulate_repeat(tty, TTYC_DL, TTYC_DL1, ctx.data.count());
        terminal_set!(tty, cy, =, UINT_MAX as u_int);
        terminal_set!(tty, cx, =, terminal_value!(tty, cy));

        true
    })(owner);
    if !complete {
        tty_redraw_region(owner, ctx, s);
    }
}
pub unsafe fn tty_cmd_reverseindex(owner: &ClientRef, ctx: &tty_ctx, s: &screen) {
    let complete = (|tty: &ClientRef| {
        if ctx.ocy != ctx.orupper {
            return true;
        }
        if ctx.flags & TTY_CTX_WINDOW_BIGGER != 0
            || !(ctx.xoff == 0 as ::core::ffi::c_int && ctx.sx >= terminal_value!(tty, sx))
                && terminal_term(tty).flags & TERM_DECSLRM == 0
            || tty_fake_bce(tty.borrow_terminal(), &ctx.style_ctx.defaults, 8 as u_int) != 0
            || tty_term_has(terminal_term(tty), TTYC_CSR) == 0
            || tty_term_has(terminal_term(tty), TTYC_RI) == 0
                && tty_term_has(terminal_term(tty), TTYC_RIN) == 0
            || ctx.sx == 1 as u_int
            || ctx.sy == 1 as u_int
        {
            return false;
        }
        tty_default_attributes(tty, ctx.bg, Some(&ctx.style_ctx));
        tty_region_pane(tty, ctx, ctx.orupper, ctx.orlower);
        tty_margin_pane(tty, ctx);
        tty_cursor_pane(tty, ctx, ctx.ocx, ctx.orupper);
        if tty_term_has(terminal_term(tty), TTYC_RI) != 0 {
            tty_putcode(tty, TTYC_RI);
        } else {
            tty_putcode_i(tty, TTYC_RIN, 1 as ::core::ffi::c_int);
        };

        true
    })(owner);
    if !complete {
        tty_redraw_region(owner, ctx, s);
    }
}
pub unsafe fn tty_cmd_scrollup(owner: &ClientRef, ctx: &tty_ctx, s: &screen) {
    let complete = (|tty: &ClientRef| {
        let mut i: u_int = 0;
        if ctx.flags & TTY_CTX_WINDOW_BIGGER != 0
            || !(ctx.xoff == 0 as ::core::ffi::c_int && ctx.sx >= terminal_value!(tty, sx))
                && terminal_term(tty).flags & TERM_DECSLRM == 0
            || tty_fake_bce(tty.borrow_terminal(), &ctx.style_ctx.defaults, 8 as u_int) != 0
            || tty_term_has(terminal_term(tty), TTYC_CSR) == 0
            || ctx.sx == 1 as u_int
            || ctx.sy == 1 as u_int
        {
            return false;
        }
        tty_default_attributes(tty, ctx.bg, Some(&ctx.style_ctx));
        tty_region_pane(tty, ctx, ctx.orupper, ctx.orlower);
        tty_margin_pane(tty, ctx);
        if ctx.data.count() == 1 as u_int || tty_term_has(terminal_term(tty), TTYC_INDN) == 0 {
            if terminal_term(tty).flags & TERM_DECSLRM == 0 {
                tty_cursor(tty, 0 as u_int, terminal_value!(tty, rlower));
            } else {
                tty_cursor(
                    tty,
                    terminal_value!(tty, rright),
                    terminal_value!(tty, rlower),
                );
            }
            i = 0 as u_int;
            while i < ctx.data.count() {
                tty_putc(tty, '\n' as i32 as u_char);
                i = i.wrapping_add(1);
            }
        } else {
            if terminal_value!(tty, cy) == UINT_MAX {
                tty_cursor(tty, 0 as u_int, 0 as u_int);
            } else {
                tty_cursor(tty, 0 as u_int, terminal_value!(tty, cy));
            }
            tty_putcode_i(tty, TTYC_INDN, ctx.data.count() as ::core::ffi::c_int);
        };

        true
    })(owner);
    if !complete {
        tty_redraw_region(owner, ctx, s);
    }
}
pub unsafe fn tty_cmd_scrolldown(owner: &ClientRef, ctx: &tty_ctx, s: &screen) {
    let complete = (|tty: &ClientRef| {
        let mut i: u_int = 0;
        if ctx.flags & TTY_CTX_WINDOW_BIGGER != 0
            || !(ctx.xoff == 0 as ::core::ffi::c_int && ctx.sx >= terminal_value!(tty, sx))
                && terminal_term(tty).flags & TERM_DECSLRM == 0
            || tty_fake_bce(tty.borrow_terminal(), &ctx.style_ctx.defaults, 8 as u_int) != 0
            || tty_term_has(terminal_term(tty), TTYC_CSR) == 0
            || tty_term_has(terminal_term(tty), TTYC_RI) == 0
                && tty_term_has(terminal_term(tty), TTYC_RIN) == 0
            || ctx.sx == 1 as u_int
            || ctx.sy == 1 as u_int
        {
            return false;
        }
        tty_default_attributes(tty, ctx.bg, Some(&ctx.style_ctx));
        tty_region_pane(tty, ctx, ctx.orupper, ctx.orlower);
        tty_margin_pane(tty, ctx);
        tty_cursor_pane(tty, ctx, ctx.ocx, ctx.orupper);
        if tty_term_has(terminal_term(tty), TTYC_RIN) != 0 {
            tty_putcode_i(tty, TTYC_RIN, ctx.data.count() as ::core::ffi::c_int);
        } else {
            i = 0 as u_int;
            while i < ctx.data.count() {
                tty_putcode(tty, TTYC_RI);
                i = i.wrapping_add(1);
            }
        };

        true
    })(owner);
    if !complete {
        tty_redraw_region(owner, ctx, s);
    }
}
pub unsafe fn tty_cmd_clearendofscreen(owner: &ClientRef, ctx: &tty_ctx) {
    let mut px: u_int = 0;
    let mut py: u_int = 0;
    let mut nx: u_int = 0;
    let mut ny: u_int = 0;
    {
        let tty = owner;

        tty_default_attributes(tty, ctx.bg, Some(&ctx.style_ctx));
        tty_region_pane(tty, ctx, 0 as u_int, ctx.sy.wrapping_sub(1 as u_int));
        tty_margin_off(tty);
    };
    px = 0 as u_int;
    nx = ctx.sx;
    py = ctx.ocy.wrapping_add(1 as u_int);
    ny = ctx.sy.wrapping_sub(ctx.ocy).wrapping_sub(1 as u_int);
    tty_clear_pane_area(owner, ctx, py, ny, px, nx, ctx.bg);
    px = ctx.ocx;
    nx = ctx.sx.wrapping_sub(ctx.ocx);
    py = ctx.ocy;
    tty_clear_pane_line(owner, ctx, py, px, nx, ctx.bg);
}
pub unsafe fn tty_cmd_clearstartofscreen(owner: &ClientRef, ctx: &tty_ctx) {
    let mut px: u_int = 0;
    let mut py: u_int = 0;
    let mut nx: u_int = 0;
    let mut ny: u_int = 0;
    {
        let tty = owner;

        tty_default_attributes(tty, ctx.bg, Some(&ctx.style_ctx));
        tty_region_pane(tty, ctx, 0 as u_int, ctx.sy.wrapping_sub(1 as u_int));
        tty_margin_off(tty);
    };
    px = 0 as u_int;
    nx = ctx.sx;
    py = 0 as u_int;
    ny = ctx.ocy;
    tty_clear_pane_area(owner, ctx, py, ny, px, nx, ctx.bg);
    px = 0 as u_int;
    nx = ctx.ocx.wrapping_add(1 as u_int);
    py = ctx.ocy;
    tty_clear_pane_line(owner, ctx, py, px, nx, ctx.bg);
}
pub unsafe fn tty_cmd_clearscreen(owner: &ClientRef, ctx: &tty_ctx) {
    let mut px: u_int = 0;
    let mut py: u_int = 0;
    let mut nx: u_int = 0;
    let mut ny: u_int = 0;
    {
        let tty = owner;

        tty_default_attributes(tty, ctx.bg, Some(&ctx.style_ctx));
        tty_region_pane(tty, ctx, 0 as u_int, ctx.sy.wrapping_sub(1 as u_int));
        tty_margin_off(tty);
    };
    px = 0 as u_int;
    nx = ctx.sx;
    py = 0 as u_int;
    ny = ctx.sy;
    tty_clear_pane_area(owner, ctx, py, ny, px, nx, ctx.bg);
}
pub unsafe fn tty_cmd_alignmenttest(owner: &ClientRef, ctx: &tty_ctx) {
    let complete = (|tty: &ClientRef| {
        let mut i: u_int = 0;
        let mut j: u_int = 0;
        if ctx.flags & TTY_CTX_WINDOW_BIGGER != 0 {
            return false;
        }
        tty_attributes(tty, &grid_default_cell, Some(&ctx.style_ctx));
        tty_region_pane(tty, ctx, 0 as u_int, ctx.sy.wrapping_sub(1 as u_int));
        tty_margin_off(tty);
        j = 0 as u_int;
        while j < ctx.sy {
            tty_cursor_pane(tty, ctx, 0 as u_int, j);
            i = 0 as u_int;
            while i < ctx.sx {
                tty_putc(tty, 'E' as i32 as u_char);
                i = i.wrapping_add(1);
            }
            j = j.wrapping_add(1);
        }

        true
    })(owner);
    if !complete {
        ctx.redraw_cb.as_ref().expect("non-null redraw callback")(ctx);
    }
}
pub unsafe fn tty_cmd_cell(owner: &ClientRef, ctx: &tty_ctx, gc: &grid_cell) {
    let px = (ctx.xoff as u_int)
        .wrapping_add(ctx.ocx)
        .wrapping_sub(ctx.wox);
    if tty_is_visible(ctx, ctx.ocx, ctx.ocy, 1, 1) == 0 {
        return;
    }
    {
        let terminal = owner;

        if px > terminal_value!(terminal, sx).wrapping_sub(1)
            && ctx.ocy == ctx.orlower
            && ctx.xoff == 0
            && ctx.sx >= terminal_value!(terminal, sx)
        {
            tty_region_pane(terminal, ctx, ctx.orupper, ctx.orlower);
        }
        tty_margin_off(terminal);
        if ctx.flags & TTY_CTX_CELL_INVALIDATE != 0 {
            tty_invalidate(terminal);
        }
        tty_cursor_pane_unless_wrap(terminal, ctx, ctx.ocx, ctx.ocy);
    };
    tty_cell(owner, gc, Some(&ctx.style_ctx));
    if ctx.flags & TTY_CTX_CELL_INVALIDATE != 0 {
        tty_invalidate(owner);
    }
}

pub unsafe fn tty_cmd_cells(owner: &ClientRef, ctx: &tty_ctx, s: &screen, gc: &grid_cell) {
    let data = ctx.data.bytes();
    let n = data.len();
    if tty_is_visible(ctx, ctx.ocx, ctx.ocy, n as u_int, 1) == 0 {
        return;
    }
    if ctx.flags & TTY_CTX_WINDOW_BIGGER != 0
        && ((ctx.xoff as u_int).wrapping_add(ctx.ocx) < ctx.wox
            || ((ctx.xoff as u_int).wrapping_add(ctx.ocx) as size_t).wrapping_add(n)
                > ctx.wox.wrapping_add(ctx.wsx) as size_t)
    {
        let draw_line = {
            let tty = owner.borrow_terminal();
            ctx.flags & TTY_CTX_WRAPPED == 0
                || !(ctx.xoff == 0 && ctx.sx >= tty.sx)
                || tty
                    .term
                    .as_ref()
                    .is_some_and(|term| term.flags & TERM_NOAM != 0)
                || (ctx.xoff as u_int).wrapping_add(ctx.ocx) != 0
                || (ctx.yoff as u_int).wrapping_add(ctx.ocy) != tty.cy.wrapping_add(1)
                || tty.cx < tty.sx
                || tty.cy == tty.rlower
        };
        if draw_line {
            tty_draw_pane(owner, ctx, s, ctx.ocy);
        } else {
            ctx.redraw_cb.as_ref().expect("non-null redraw callback")(ctx);
        }
        return;
    }
    {
        let terminal = owner;

        tty_margin_off(terminal);
        tty_cursor_pane_unless_wrap(terminal, ctx, ctx.ocx, ctx.ocy);
        tty_attributes(terminal, gc, Some(&ctx.style_ctx));
        if n != 0 {
            tty_putn(terminal, data, n as u_int);
        }
    };
}

pub unsafe fn tty_cmd_setselection(owner: &ClientRef, ctx: &tty_ctx) {
    let (clip, data) = ctx.data.selection();
    tty_set_selection(owner, clip, data);
}

pub unsafe fn tty_set_selection(owner: &ClientRef, clip: &CStr, data: &[u8]) {
    let sequence = {
        let mut terminal = owner.borrow_terminal_mut();
        tty_selection_sequence(terminal, clip, data)
    };
    if let Some(sequence) = sequence.filter(|sequence| !sequence.is_empty()) {
        owner.write_terminal(sequence.to_bytes());
    }
}

// Capability expansion reads only the terminal component. The owned sequence
// must outlive this borrow so output accounting can borrow Client afresh.
unsafe fn tty_selection_sequence(
    terminal: &mut tty,
    clip: &CStr,
    data: &[u8],
) -> Option<std::ffi::CString> {
    if terminal.flags & TTY_STARTED == 0 {
        return None;
    }
    let term = tty_term_owner_ptr(&terminal.term).expect("client has a terminal");
    if tty_term_has(term, TTYC_MS) == 0 {
        return None;
    }
    let size = 4usize
        .wrapping_mul(data.len().wrapping_add(2) / 3)
        .wrapping_add(1);
    let mut encoded = vec![0; size];
    __b64_ntop(data.as_ptr(), data.len(), encoded.as_mut_ptr().cast(), size);
    terminal.flags |= TTY_NOBLOCK;
    Some(tty_term_string_ss(
        term,
        TTYC_MS,
        clip,
        CStr::from_ptr(encoded.as_ptr().cast()),
    ))
}
pub unsafe fn tty_cmd_rawstring(owner: &ClientRef, ctx: &tty_ctx) {
    {
        let terminal = owner;

        terminal_set!(terminal, flags, |=, TTY_NOBLOCK);
        tty_add(terminal, ctx.data.bytes());
        tty_invalidate(terminal);
    };
}
pub unsafe fn tty_cmd_syncstart(owner: &ClientRef, ctx: &tty_ctx) {
    if ctx.flags & TTY_CTX_SYNC != 0 {
        tty_sync_start(owner);
    }
}

pub unsafe fn tty_cell(owner: &ClientRef, gc: &grid_cell, style_ctx: Option<&tty_style_ctx>) {
    {
        let terminal = owner.borrow_terminal();
        if terminal
            .term
            .as_ref()
            .is_some_and(|term| term.flags & TERM_NOAM != 0)
            && terminal.cy == terminal.sy.wrapping_sub(1)
            && terminal.cx == terminal.sx.wrapping_sub(1)
        {
            return;
        }
    }
    if gc.flags as i32 & GRID_FLAG_PADDING != 0 {
        return;
    }
    (|terminal: &ClientRef| {
        let converted = tty_check_codeset(terminal.flags() & CLIENT_UTF8 as u64 != 0, gc);
        tty_attributes(terminal, &converted, style_ctx);
        if converted.data.size == 1 {
            let byte = converted.data.data[0];
            if byte < 0x20 || byte == 0x7f {
                return;
            }
            tty_putc(terminal, byte);
        } else {
            tty_putn(
                terminal,
                &converted.data.data[..converted.data.size as usize],
                converted.data.width as u_int,
            );
        }
    })(owner);
}

pub unsafe fn tty_default_colours(
    pane: &std::rc::Rc<std::cell::UnsafeCell<window_pane>>,
) -> (grid_cell, u_int) {
    pane.default_colours()
}

unsafe fn tty_clipboard_query_callback(owner: &ClientRef) {
    owner.borrow_terminal_mut().flags &= !TTY_OSC52QUERY;
}
pub unsafe fn tty_clipboard_query(owner: &ClientRef) {
    let query = {
        let tty = owner.borrow_terminal();
        if tty.flags & TTY_STARTED == 0 || tty.flags & TTY_OSC52QUERY != 0 {
            return;
        }
        tty_term_string_ss(
            tty.term.as_deref().expect("open terminal"),
            TTYC_MS,
            c"",
            c"?",
        )
    };
    if !query.is_empty() {
        owner.write_terminal(query.to_bytes());
    }
    let timeout = Duration::from_secs(TTY_QUERY_TIMEOUT as u64);
    let mut tty = owner.borrow_terminal_mut();
    tty.flags |= TTY_OSC52QUERY;
    tty.clipboard_timer = Some(
        Timer::new(
            timeout,
            tty_client_timer_callback(owner, tty_clipboard_query_callback),
        )
        .expect("arm timer"),
    );
}

#[cfg(test)]
mod clipping_tests {
    use super::*;

    #[test]
    fn the_view_rests_on_the_nearest_pane_start_that_shows_the_pane() {
        // Halves of an 80-column window start at 0, 40, 80 and 120.
        let stops = [0, 40, 80, 120];
        let follow = |cell, previous| tty_follow_cell(&stops, cell, 80, 200, previous, None);
        // A shown pane leaves the view alone, wherever its cursor is.
        assert_eq!(follow((80, 39), 80), 80);
        assert_eq!(follow((80, 39), 40), 40);
        assert_eq!(
            tty_follow_cell(&stops, (80, 39), 80, 200, 80, Some(118)),
            80
        );
        // From the left it becomes the last pane shown; from the right, the first.
        assert_eq!(follow((80, 39), 0), 40);
        assert_eq!(follow((40, 39), 120), 40);
        // A pan between starts returns to the nearer one, the left on a tie.
        assert_eq!(follow((40, 39), 25), 40);
        assert_eq!(follow((40, 39), 20), 0);
        // A view wider than the window stays within the extent.
        assert_eq!(tty_follow_cell(&stops, (120, 39), 120, 200, 0, None), 40);
    }

    #[test]
    fn a_view_shorter_than_the_pane_keeps_the_cursor_in_sight() {
        // A full pane at columns 40 to 119, seen through 40 columns.
        let follow =
            |previous, cursor| tty_follow_cell(&[0, 40, 121], (40, 80), 40, 201, previous, cursor);
        assert_eq!(follow(0, None), 40);
        assert_eq!(follow(121, None), 80);
        assert_eq!(follow(40, Some(111)), 72);
        assert_eq!(follow(72, Some(51)), 51);
        assert_eq!(follow(51, Some(70)), 51);
        // Rows have one stop at the top; a short view follows the cursor.
        assert_eq!(tty_follow_cell(&[0], (0, 24), 24, 24, 5, Some(23)), 0);
        assert_eq!(tty_follow_cell(&[0], (0, 24), 10, 24, 0, Some(23)), 14);
        assert_eq!(tty_follow_cell(&[0], (0, 24), 10, 24, 14, Some(3)), 3);
    }

    #[test]
    fn clipping_matches_tmux_at_viewport_edges_and_for_empty_regions() {
        let ctx = tty_ctx {
            flags: TTY_CTX_WINDOW_BIGGER,
            xoff: 8,
            yoff: 18,
            rxoff: 8,
            ryoff: 18,
            wox: 10,
            woy: 20,
            wsx: 30,
            wsy: 10,
            ..Default::default()
        };
        // Expected geometry from the pinned tmux tty_clamp_line/area functions.
        // The source skip when both sides are clipped deliberately follows tmux.
        for (input, line, area) in [
            ((3, 2, 5, 2), Some((0, 1, 0, 5)), Some((1, 0, 5, 2))),
            ((0, 2, 5, 3), Some((2, 0, 0, 3)), Some((0, 0, 3, 3))),
            ((30, 2, 6, 5), Some((0, 28, 0, 2)), Some((28, 0, 2, 5))),
            ((0, 2, 40, 20), Some((10, 0, 0, 30)), Some((0, 0, 30, 10))),
            ((0, 2, 2, 1), None, None),
            ((32, 2, 2, 1), None, None),
            ((3, 0, 5, 2), None, None),
            ((3, 12, 5, 1), None, None),
            ((3, 3, 0, 0), Some((0, 1, 1, 0)), Some((1, 1, 0, 0))),
            ((0, 0, 5, 5), None, Some((0, 0, 3, 3))),
            ((30, 10, 6, 5), Some((0, 28, 8, 2)), Some((28, 8, 2, 2))),
            ((0, 0, 40, 20), None, Some((0, 0, 30, 10))),
        ] {
            let (px, py, nx, ny) = input;
            assert_eq!(
                tty_clamp_line(&ctx, px, py, nx).map(|r| (r.skip, r.x, r.y, r.width)),
                line,
                "line {input:?}"
            );
            assert_eq!(
                tty_clamp_area(&ctx, px, py, nx, ny).map(|r| (r.x, r.y, r.width, r.height)),
                area,
                "area {input:?}"
            );
        }
    }
}
