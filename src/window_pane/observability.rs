//! Read-only plugin access stays inside the pane owner. Observations retain
//! allocation identity weakly and reject logical destruction and ID reuse.
use super::{window_pane, window_pane_find_by_id, window_pane_upgrade};
use crate::src::grid::grid_string_cells_bytes;
use crate::src::plugin::PaneActivity;
use crate::src::shared::display::{
    SCREEN_CURSOR_BAR, SCREEN_CURSOR_BLOCK, SCREEN_CURSOR_UNDERLINE,
};
use crate::src::shared::grid::{grid, GRID_STRING_EMPTY_CELLS, GRID_STRING_TRIM_SPACES};
use crate::src::shared::screen::{
    screen, MODE_CURSOR, MODE_CURSOR_BLINKING, MODE_CURSOR_BLINKING_SET,
};
use std::cell::UnsafeCell;
use std::ffi::CString;
use std::io;
use std::os::fd::AsFd;
use std::os::unix::ffi::OsStringExt;
use std::path::PathBuf;
use std::rc::{Rc, Weak};
use tiri_agent::observability::v1::{PaneId, PaneObservability, PaneProcess, ScreenTail};
use tiri_agent::pane_class::{stringify_argv, PaneProcessProbe};

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
            let live = window_pane_find_by_id(self.id.0)
                .is_some_and(|current| Rc::ptr_eq(&current, &owner));
            if live {
                Ok(read(&*owner.get()))
            } else {
                Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    "pane is no longer registered",
                ))
            }
        }
    }
}

pub(crate) unsafe fn resolve(id: PaneId) -> Option<Rc<dyn PaneObservability>> {
    unsafe {
        let owner = window_pane_find_by_id(id.0)?;
        Some(Rc::new(PaneView {
            id,
            pane: Rc::downgrade(&owner),
        }))
    }
}

fn read_pane<T>(id: PaneId, read: impl FnOnce(&window_pane) -> T) -> Option<T> {
    unsafe {
        let owner = window_pane_find_by_id(id.0)?;
        Some(read(&*owner.get()))
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
            exited: pane.fd.is_none() && pane.exit_status.is_some(),
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

    fn screen(&self, lines: usize) -> io::Result<ScreenTail> {
        self.read(|pane| ScreenTail {
            revision: pane.output_generation,
            text: pane
                .base
                .grid
                .as_deref()
                .map_or_else(CString::default, |grid| screen_text(grid, lines)),
            cursor_visible: pane.base.mode & MODE_CURSOR != 0,
            cursor_shape: cursor_shape(&pane.base),
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

fn screen_text(grid: &grid, lines: usize) -> CString {
    if lines == 0 {
        return CString::default();
    }
    let floor = grid.hsize;
    let mut end = grid.hsize.saturating_add(grid.sy);
    while end > floor && row_text(grid, end - 1, true).is_empty() {
        end -= 1;
    }
    if end == floor {
        return CString::default();
    }
    let start = end
        .saturating_sub(u32::try_from(lines).unwrap_or(u32::MAX))
        .max(floor);
    let mut text = Vec::new();
    for row in start..end {
        text.extend_from_slice(&row_text(grid, row, true));
        if row + 1 < end {
            text.push(b'\n');
        }
    }
    CString::new(text).expect("grid text contains no NUL")
}

#[cfg(test)]
mod tests;
