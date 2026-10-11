//! A per-client overlay: a small box drawn over the client's window. It is
//! either a key hint, listing the bindings of the client's key table as soon
//! as the table starts waiting for a follow-up key, or a notice that hides
//! itself after a delay.
//! The box is drawn as overlay spans in the client's redraw scene, so panes
//! never draw over it.

use super::{server_client_timer, ClientRef};
use crate::src::grid::grid_default_cell;
use crate::src::key_string::key_string_format;
use crate::src::screen::screen_init;
use crate::src::screen_write::{
    screen_write_box, screen_write_cursormove, screen_write_nputs, screen_write_start,
    screen_write_stop,
};
use crate::src::server_client::Client as _;
use crate::src::shared::abi::u_int;
use crate::src::shared::client::{CLIENT_CONTROL, CLIENT_REDRAWWINDOW, CLIENT_UNATTACHEDFLAGS};
use crate::src::shared::colour::COLOUR_FLAG_256;
use crate::src::shared::event::Timer;
use crate::src::shared::grid::{grid_cell, GRID_ATTR_BRIGHT};
use crate::src::shared::screen::screen;
use crate::src::shared::screen_write::screen_write_ctx;
use std::ffi::CStr;
use std::time::Duration;

/// The only key table that shows a hint; tmux's own tables keep tmux behavior.
const KEY_HINT_TABLE: &[u8] = b"strip";
const KEY_HINT_NOTE_WIDTH: usize = 24;
const KEY_HINT_MAX_ROWS: usize = 12;
/// Rounded box lines.
const OVERLAY_BOX_LINES: i32 = 4;
const OVERLAY_BORDER_FG: i32 = 244 | COLOUR_FLAG_256;

pub struct Overlay {
    screen: Box<screen>,
    /// Hides a notice when it fires; a key hint has none and lasts as long as
    /// its key table.
    timer: Option<Timer>,
}

impl Overlay {
    pub(crate) fn screen(&self) -> &screen {
        &self.screen
    }

    /// Stop the timer before releasing the screen.
    fn free(self) {
        drop(self.timer);
        drop(self.screen);
    }
}

/// Show the hint for the client's new key table, if it has one, and otherwise
/// hide the hint for the old table. A notice stays until its timer fires.
pub(super) unsafe fn overlay_key_table_changed(owner: &ClientRef) {
    if let Some(hint) = key_hint_build(owner) {
        overlay_replace(owner, hint);
    } else if (*owner.get())
        .overlay
        .as_ref()
        .is_some_and(|overlay| overlay.timer.is_none())
    {
        overlay_hide(owner);
    }
}

/// Show `text` in a box titled `title` for `duration`, replacing any overlay.
pub(super) unsafe fn overlay_show_notice(
    owner: &ClientRef,
    title: &CStr,
    text: &CStr,
    duration: Duration,
) {
    if !overlay_shown_on(owner) {
        return;
    }
    let (sx, sy) = owner.terminal_size();
    let width = text.to_bytes().len().max(title.to_bytes().len()) + 4;
    let Some(mut notice) = overlay_build(width, 3, sx, sy, title, |ctx| {
        let mut gc = grid_default_cell;
        gc.attr |= GRID_ATTR_BRIGHT as u16;
        overlay_put(ctx, &gc, 2, 1, text.to_bytes());
    }) else {
        return;
    };
    notice.timer = Some(server_client_timer(owner, duration, overlay_hide));
    overlay_replace(owner, notice);
}

/// Drop the overlay with the client.
pub(super) unsafe fn overlay_free(owner: &ClientRef) {
    if let Some(overlay) = (*owner.get()).overlay.take() {
        overlay.free();
    }
}

unsafe fn overlay_hide(owner: &ClientRef) {
    let Some(overlay) = (*owner.get()).overlay.take() else {
        return;
    };
    overlay.free();
    overlay_changed(owner);
}

unsafe fn overlay_replace(owner: &ClientRef, overlay: Overlay) {
    if let Some(old) = (*owner.get()).overlay.replace(overlay) {
        old.free();
    }
    overlay_changed(owner);
}

unsafe fn overlay_changed(owner: &ClientRef) {
    drop(owner.take_redraw_scene());
    (*owner.get()).flags |= CLIENT_REDRAWWINDOW as u64;
}

/// Whether the client draws a window an overlay can cover.
unsafe fn overlay_shown_on(owner: &ClientRef) -> bool {
    let c = owner.get();
    (*c).flags & (CLIENT_CONTROL as u64 | CLIENT_UNATTACHEDFLAGS as u64) == 0
        && (*c).session.upgrade().is_some()
}

/// Box a `width` by `height` screen titled `title` and let `fill` write its
/// inside, or return None when it does not fit, with a line to spare, in `sx`
/// by `sy`.
unsafe fn overlay_build(
    width: usize,
    height: usize,
    sx: u_int,
    sy: u_int,
    title: &CStr,
    fill: impl FnOnce(&mut screen_write_ctx),
) -> Option<Overlay> {
    let (width, height) = (u_int::try_from(width).ok()?, u_int::try_from(height).ok()?);
    if width > sx || height.saturating_add(1) > sy {
        return None;
    }
    let mut screen = Box::new(screen::default());
    screen_init(&mut screen, width, height, 0);
    let mut ctx = screen_write_ctx::default();
    screen_write_start(&mut ctx, &mut *screen);
    let mut border = grid_default_cell;
    border.fg = OVERLAY_BORDER_FG;
    screen_write_cursormove(&mut ctx, 0, 0, 0);
    screen_write_box(
        &mut ctx,
        width,
        height,
        OVERLAY_BOX_LINES,
        Some(&border),
        Some(title),
    );
    fill(&mut ctx);
    screen_write_stop(&mut ctx);
    Some(Overlay {
        screen,
        timer: None,
    })
}

/// Lay out the noted bindings of the client's key table in columns and box
/// them, or return None when the table shows no hint, nothing is noted or the
/// box does not fit.
unsafe fn key_hint_build(owner: &ClientRef) -> Option<Overlay> {
    if !overlay_shown_on(owner) {
        return None;
    }
    let table = (*owner.get()).keytable.clone()?;
    let table = table.borrow();
    if table.name.as_bytes() != KEY_HINT_TABLE {
        return None;
    }
    // Digit keys whose note ends in the digit ("Select window 3") share a row.
    let mut entries: Vec<(String, String)> = Vec::new();
    let mut digits: Option<(String, char, char)> = None;
    for bd in table.key_bindings.iter() {
        let Some(note) = bd.note.as_ref() else {
            continue;
        };
        let key = key_string_format(bd.key, false)
            .to_string_lossy()
            .into_owned();
        let note = note.to_string_lossy().into_owned();
        let digit = key
            .chars()
            .next()
            .filter(|ch| key.len() == 1 && ch.is_ascii_digit());
        if let Some(digit) = digit.filter(|_| note.ends_with(key.as_str())) {
            let stem = note[..note.len() - 1].trim_end().to_owned();
            match digits.as_mut() {
                Some((group, _, last)) if *group == stem => *last = digit,
                _ => {
                    if let Some((group, first, last)) = digits.replace((stem, digit, digit)) {
                        entries.push((format!("{first}-{last}"), group));
                    }
                }
            }
            continue;
        }
        entries.push((key, note));
    }
    if let Some((stem, first, last)) = digits {
        entries.push((format!("{first}-{last}"), stem));
    }
    for (_, note) in entries.iter_mut() {
        if note.chars().count() > KEY_HINT_NOTE_WIDTH {
            *note = note.chars().take(KEY_HINT_NOTE_WIDTH - 1).collect();
            note.push('…');
        }
    }
    if entries.is_empty() {
        return None;
    }
    let key_width = entries.iter().map(|(k, _)| k.len()).max()?;
    let note_width = entries.iter().map(|(_, n)| n.chars().count()).max()?;
    let cell_width = key_width + 1 + note_width;
    let columns = entries.len().div_ceil(KEY_HINT_MAX_ROWS);
    let rows = entries.len().div_ceil(columns);
    let inner = columns * cell_width + (columns - 1) * 2;
    let (sx, sy) = owner.terminal_size();
    overlay_build(inner + 4, rows + 2, sx, sy, c" C-a ", |ctx| {
        let mut key = grid_default_cell;
        key.attr |= GRID_ATTR_BRIGHT as u16;
        key.fg = 4;
        let mut note = grid_default_cell;
        note.fg = 250 | COLOUR_FLAG_256;
        for (i, (key_name, note_text)) in entries.iter().enumerate() {
            let (column, row) = (i / rows, i % rows);
            let x = 2 + column * (cell_width + 2);
            overlay_put(ctx, &key, x, row + 1, key_name.as_bytes());
            let x = x + key_width + 1;
            overlay_put(ctx, &note, x, row + 1, note_text.as_bytes());
        }
    })
}

unsafe fn overlay_put(ctx: &mut screen_write_ctx, gc: &grid_cell, x: usize, y: usize, text: &[u8]) {
    screen_write_cursormove(ctx, x as i32, y as i32, 0);
    screen_write_nputs(ctx, -1, gc, |out| out.write_all(text));
}
