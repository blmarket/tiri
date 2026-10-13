//! Read-only plugin access stays inside the pane owner. Observations retain
//! allocation identity weakly and reject logical destruction and ID reuse.
use super::{window_pane, window_pane_find_by_id, window_pane_upgrade, WindowPane};
use crate::src::grid::{grid_peek_line, grid_string_cells_bytes};
use crate::src::plugin::PaneActivity;
use crate::src::shared::display::{
    SCREEN_CURSOR_BAR, SCREEN_CURSOR_BLOCK, SCREEN_CURSOR_UNDERLINE,
};
use crate::src::shared::grid::{
    grid, GRID_LINE_WRAPPED, GRID_STRING_EMPTY_CELLS, GRID_STRING_TRIM_SPACES,
};
use crate::src::shared::pane::PANE_STATUSREADY;
use crate::src::shared::screen::{
    screen, MODE_CURSOR, MODE_CURSOR_BLINKING, MODE_CURSOR_BLINKING_SET,
};
use tiri_agent::observability::v1::{
    PaneId, PaneObservability, PaneProcess, ScreenSource, ScreenTail,
};
use tiri_agent::pane_class::{stringify_argv, PaneProcessProbe};
use std::cell::UnsafeCell;
use std::ffi::CString;
use std::io;
use std::os::fd::AsFd;
use std::os::unix::ffi::OsStringExt;
use std::path::PathBuf;
use std::rc::{Rc, Weak};

struct PaneView {
    id: PaneId,
    pane: Weak<UnsafeCell<window_pane>>,
}

impl PaneView {
    fn read<T>(&self, read: impl FnOnce(&window_pane) -> T) -> io::Result<T> {
        unsafe {
            let Some(owner) = window_pane_upgrade(&self.pane) else {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    "pane was destroyed",
                ));
            };
            let current = window_pane_find_by_id(self.id.0);
            let live = current
                .as_ref()
                .is_some_and(|current| Rc::ptr_eq(current, &owner));
            if let Some(current) = current {
                current.release(c"plugin observation check");
            }
            let result = if live {
                Ok(read(&*owner.get()))
            } else {
                Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    "pane is no longer registered",
                ))
            };
            owner.release(c"plugin observation");
            result
        }
    }
}

pub(crate) unsafe fn resolve(id: PaneId) -> Option<Rc<dyn PaneObservability>> {
    unsafe {
        let owner = window_pane_find_by_id(id.0)?;
        let pane = Rc::downgrade(&owner);
        owner.release(c"plugin resolve");
        Some(Rc::new(PaneView { id, pane }))
    }
}

fn read_pane<T>(id: PaneId, read: impl FnOnce(&window_pane) -> T) -> Option<T> {
    unsafe {
        let owner = window_pane_find_by_id(id.0)?;
        let result = read(&*owner.get());
        owner.release(c"plugin pane snapshot");
        Some(result)
    }
}

pub(crate) unsafe fn activity(id: PaneId) -> Option<PaneActivity> {
    read_pane(id, |pane| {
        let (foreground, session_leader) = process_groups(pane);
        PaneActivity {
            foreground,
            session_leader,
            command: if pane.argv.is_empty() {
                pane.shell.clone()
            } else {
                Some(stringify_argv(&pane.argv))
            },
            alternate_on: pane.base.saved_grid.is_some(),
            exited: pane.fd.is_none() && pane.flags & PANE_STATUSREADY != 0,
        }
    })
}

pub(crate) unsafe fn cwd(id: PaneId) -> Option<PathBuf> {
    let info = activity(id)?;
    let path =
        PaneProcessProbe::new(info.foreground, info.session_leader, info.command).current_path()?;
    let path = PathBuf::from(std::ffi::OsString::from_vec(path.into_bytes()));
    path.is_absolute().then_some(path)
}

fn process_groups(pane: &window_pane) -> (Option<i32>, Option<i32>) {
    let Some(fd) = pane.fd.as_ref() else {
        return (None, None);
    };
    (
        tiri_rt::unix::terminal_foreground_group(fd.as_fd())
            .ok()
            .filter(|&pid| pid > 0),
        tiri_rt::unix::terminal_session(fd.as_fd())
            .ok()
            .filter(|&pid| pid > 0),
    )
}

impl PaneObservability for PaneView {
    fn process(&self) -> io::Result<PaneProcess> {
        self.read(|pane| PaneProcess {
            child_pid: (pane.pid > 0).then_some(pane.pid as u32),
            exited: pane.fd.is_none(),
        })
    }

    fn output_revision(&self) -> io::Result<u64> {
        self.read(|pane| pane.output_generation)
    }

    fn screen(&self, source: ScreenSource, lines: usize) -> io::Result<ScreenTail> {
        self.read(|pane| ScreenTail {
            revision: pane.output_generation,
            text: pane
                .base
                .grid
                .as_deref()
                .map_or_else(CString::default, |grid| screen_text(grid, source, lines)),
            cursor_visible: pane.base.mode & MODE_CURSOR != 0,
            cursor_shape: cursor_shape(&pane.base),
        })
    }

    fn scrollback_rows(&self) -> io::Result<usize> {
        self.read(|pane| {
            pane.base
                .grid
                .as_deref()
                .map_or(0, |grid| grid.hsize as usize)
        })
    }

    fn title(&self) -> io::Result<Option<CString>> {
        self.read(|pane| (!pane.base.title.is_empty()).then(|| pane.base.title.clone()))
    }
}

fn cursor_shape(screen: &screen) -> u8 {
    let blinking = screen.mode & MODE_CURSOR_BLINKING != 0;
    match screen.cstyle {
        SCREEN_CURSOR_BLOCK => {
            if blinking {
                1
            } else {
                2
            }
        }
        SCREEN_CURSOR_UNDERLINE => {
            if blinking {
                3
            } else {
                4
            }
        }
        SCREEN_CURSOR_BAR => {
            if blinking {
                5
            } else {
                6
            }
        }
        _ if screen.mode & MODE_CURSOR_BLINKING_SET != 0 => {
            if blinking {
                1
            } else {
                2
            }
        }
        _ => 0,
    }
}

fn row_text(grid: &grid, row: u32, trim: bool) -> Vec<u8> {
    let flags = GRID_STRING_EMPTY_CELLS | if trim { GRID_STRING_TRIM_SPACES } else { 0 };
    unsafe { grid_string_cells_bytes(grid, 0, row, grid.sx, None, flags, None) }
}

fn wrapped(grid: &grid, row: u32) -> bool {
    unsafe {
        grid_peek_line(grid, row).is_some_and(|line| line.flags as i32 & GRID_LINE_WRAPPED != 0)
    }
}

fn screen_text(grid: &grid, source: ScreenSource, lines: usize) -> CString {
    if lines == 0 {
        return CString::default();
    }
    let floor = if matches!(source, ScreenSource::Visible) {
        grid.hsize
    } else {
        0
    };
    let mut end = grid.hsize.saturating_add(grid.sy);
    while end > floor && row_text(grid, end - 1, true).is_empty() {
        end -= 1;
    }
    if end == floor {
        return CString::default();
    }
    let unwrapped = matches!(source, ScreenSource::RecentUnwrapped);
    let start = if unwrapped {
        let mut start = end;
        let mut count = 0;
        while start > floor && count < lines {
            start -= 1;
            if start == floor || !wrapped(grid, start - 1) {
                count += 1;
            }
        }
        start
    } else {
        end.saturating_sub(u32::try_from(lines).unwrap_or(u32::MAX))
            .max(floor)
    };
    let mut text = Vec::new();
    for row in start..end {
        let continues = unwrapped && row + 1 < end && wrapped(grid, row);
        text.extend_from_slice(&row_text(grid, row, !continues));
        if !continues && row + 1 < end {
            text.push(b'\n');
        }
    }
    CString::new(text).expect("grid text contains no NUL")
}

#[cfg(test)]
mod tests;
