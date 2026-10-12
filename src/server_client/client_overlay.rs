//! Typed overlay ownership and callback state. Dispatch takes only the active
//! callback, so closing an overlay can still retire its owner during that call.

use crate::src::popup::{PopupHandle, PopupState};
use crate::src::shared::client::{
    overlay_check_cb, overlay_draw_cb, overlay_free_cb, overlay_key_cb, overlay_mode_cb,
    overlay_resize_cb,
};

enum OverlayData {
    Popup(refbox::RefBox<PopupState>),
    #[cfg(test)]
    CallbackOnly,
}

/// One installed overlay owns its callbacks and its concrete payload together.
/// Popup is the only production payload; menus are owned by their window.
pub struct Overlay {
    pub(super) check: overlay_check_cb,
    pub(super) mode: overlay_mode_cb,
    pub(super) draw: overlay_draw_cb,
    pub(super) key: overlay_key_cb,
    pub(super) free: overlay_free_cb,
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
            free: None,
            resize,
            data: OverlayData::Popup(owner),
        }
    }

    #[cfg(test)]
    pub(crate) fn callbacks(
        check: overlay_check_cb,
        mode: overlay_mode_cb,
        draw: overlay_draw_cb,
        key: overlay_key_cb,
        free: overlay_free_cb,
        resize: overlay_resize_cb,
    ) -> Self {
        Self {
            check,
            mode,
            draw,
            key,
            free,
            resize,
            data: OverlayData::CallbackOnly,
        }
    }

    pub(super) fn popup_handle(&self) -> Option<PopupHandle> {
        match &self.data {
            OverlayData::Popup(owner) => Some(PopupHandle::new(owner.downgrade())),
            #[cfg(test)]
            OverlayData::CallbackOnly => None,
        }
    }

    /// Keep the explicit free callback before callback/payload destruction.
    /// Removing the overlay from the client before this call allows cleanup to
    /// install another overlay without borrowing or overwriting this owner.
    pub(super) fn free(self, client: &crate::src::shared::client::ClientRef) {
        let Self {
            check,
            mode,
            draw,
            key,
            free,
            resize,
            data,
        } = self;
        if let Some(free) = free {
            free(client);
        }
        drop((check, mode, draw, key, resize));
        drop(data);
    }
}
