//! Typed overlay ownership and callback state. Dispatch takes only the active
//! callback, so closing an overlay can still retire its owner during that call.

use crate::popup::{PopupEvent, PopupHandle};
use crate::shared::client::{overlay_check_cb, overlay_draw_cb, overlay_mode_cb};

enum OverlayData {
    Popup(PopupHandle),
}

/// An installed overlay owns its callbacks and observes its popup future.
/// Menus are owned by their window.
pub struct Overlay {
    pub(super) check: overlay_check_cb,
    pub(super) mode: overlay_mode_cb,
    pub(super) draw: overlay_draw_cb,
    data: OverlayData,
    pub(super) events: tiri_rt::channel::Sender<PopupEvent>,
}

impl Overlay {
    pub(crate) fn popup(
        handle: PopupHandle,
        events: tiri_rt::channel::Sender<PopupEvent>,
        check: overlay_check_cb,
        mode: overlay_mode_cb,
        draw: overlay_draw_cb,
    ) -> Self {
        Self {
            check,
            mode,
            draw,
            data: OverlayData::Popup(handle),
            events,
        }
    }

    pub(super) fn popup_handle(&self) -> Option<PopupHandle> {
        match &self.data {
            OverlayData::Popup(handle) => Some(handle.clone()),
        }
    }
}

impl Drop for Overlay {
    fn drop(&mut self) {
        let _ = self.events.send(PopupEvent::Close);
    }
}
