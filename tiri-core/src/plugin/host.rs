use super::{Host, PaneId};
use crate::shared::pane::{window_pane, window_pane_tree};
use crate::window_pane::WindowPane;
use std::cell::RefCell;
use std::io;
use std::os::unix::ffi::OsStringExt;
use std::path::PathBuf;
use std::rc::Rc;

#[derive(Clone, Copy)]
pub(super) struct ServerHost;

impl Host for ServerHost {
    fn pane_ids(&self) -> io::Result<Vec<PaneId>> {
        Ok(window_pane_tree::snapshot()
            .into_iter()
            .map(|pane| PaneId(pane.id()))
            .collect())
    }

    fn pane_cwd(&self, pane: PaneId) -> Option<PathBuf> {
        let pane = <Rc<RefCell<window_pane>>>::find_by_id(pane.0)?;
        let path = pane.current_path()?;
        let path = PathBuf::from(std::ffi::OsString::from_vec(path.into_bytes()));
        path.is_absolute().then_some(path)
    }
}

pub(super) fn invalidate(id: PaneId) {
    let Some(pane) = <Rc<RefCell<window_pane>>>::find_by_id(id.0) else {
        return;
    };
    let window = pane.window_observer().upgrade();
    drop(pane);
    if let Some(window) = window {
        crate::server_fn::server_status_window(&window);
        drop(window);
    }
}
