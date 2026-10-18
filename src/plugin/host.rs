//! The small adapter between plugin observations and pane ownership.
use super::{Host, PaneActivity, PaneId};
use crate::shared::pane::{window_pane, window_pane_tree};
use crate::window_pane::{observability, WindowPane};
use std::cell::UnsafeCell;
use std::io;
use std::path::PathBuf;
use std::rc::Rc;
use tiri_agent::observability::v1::{PaneObservability, ServerObservability};

#[derive(Clone, Copy)]
pub(super) struct ServerHost;

impl ServerObservability for ServerHost {
    fn pane_ids(&self) -> io::Result<Vec<PaneId>> {
        unsafe {
            Ok(window_pane_tree::snapshot()
                .into_iter()
                .map(|pane| PaneId(pane.id()))
                .collect())
        }
    }

    fn resolve_pane(&self, id: PaneId) -> io::Result<Option<Rc<dyn PaneObservability>>> {
        Ok(observability::resolve(id))
    }
}

impl Host for ServerHost {
    fn pane_cwd(&self, pane: PaneId) -> Option<PathBuf> {
        unsafe { observability::cwd(pane) }
    }

    fn pane_activity(&self, pane: PaneId) -> Option<PaneActivity> {
        observability::activity(pane)
    }
}

pub(super) fn invalidate(id: PaneId) {
    unsafe {
        let Some(pane) = <Rc<UnsafeCell<window_pane>>>::find_by_id(id.0) else {
            return;
        };
        let window = pane.window_observer().upgrade();
        drop(pane);
        if let Some(window) = window {
            crate::server_fn::server_status_window(&window);
            drop(window);
        }
    }
}
