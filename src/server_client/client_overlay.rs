//! Typed overlay ownership and callback state. Dispatch takes only the active
//! callback, so closing an overlay can still retire its owner during that call.

use crate::src::popup::{PopupHandle, PopupState};
use crate::src::shared::client::{
    overlay_check_cb, overlay_draw_cb, overlay_key_cb, overlay_mode_cb, overlay_resize_cb,
};

enum OverlayData {
    Popup(refbox::RefBox<PopupState>),
}

/// One installed overlay owns its callbacks and its concrete payload together.
/// Popup is the only production payload; menus are owned by their window.
pub struct Overlay {
    pub(super) check: overlay_check_cb,
    pub(super) mode: overlay_mode_cb,
    pub(super) draw: overlay_draw_cb,
    pub(super) key: overlay_key_cb,
    pub(super) resize: overlay_resize_cb,
    data: OverlayData,
}

impl Overlay {
    pub(crate) fn popup(
        owner: refbox::RefBox<PopupState>,
        check: overlay_check_cb,
        mode: overlay_mode_cb,
        draw: overlay_draw_cb,
        key: overlay_key_cb,
        resize: overlay_resize_cb,
    ) -> Self {
        Self {
            check,
            mode,
            draw,
            key,
            resize,
            data: OverlayData::Popup(owner),
        }
    }

    pub(super) fn popup_handle(&self) -> Option<PopupHandle> {
        match &self.data {
            OverlayData::Popup(owner) => Some(PopupHandle::new(owner.downgrade())),
        }
    }
}
