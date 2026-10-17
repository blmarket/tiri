//! Window storage is private to its implementation.
use crate::src::options::options;
use crate::src::shared::abi::{u_int, uint64_t};
use crate::src::shared::client::ClientWeak;
use crate::src::shared::event::Timer;
use crate::src::shared::grid::grid_cell;
use crate::src::shared::pane::{window_pane, window_panes, PaneHistory};
use crate::src::shared::window::WindowRef;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use crate::src::shared::window::window_winlinks;
#[repr(C)]
/// Window record owned through `WindowRef`; dropping the last owner closes it.
pub struct window {
    pub(super) id: u_int,
    /// Nonowning identity of the client last active in this window.
    pub(super) latest: ClientWeak,
    pub(super) name: std::ffi::CString,
    pub(super) name_event: Option<Timer>,
    pub(super) name_time: Option<Instant>,
    pub(super) alerts_timer: Option<Timer>,
    pub(super) offset_timer: Option<Timer>,
    pub(super) activity_time: SystemTime,
    pub(super) creation_time: SystemTime,
    /// Current pane identity; the pane index owns the allocation.
    pub(super) active: std::rc::Weak<std::cell::UnsafeCell<window_pane>>,
    pub(super) last_panes: PaneHistory,
    /// The pane order.
    pub(super) panes: window_panes,
    /// The arrangement of the panes and its own record of each one; every
    /// pane's rectangle derives from it and the pane order.
    pub(super) layout: super::layout::LayoutKind,
    pub(super) sx: u_int,
    pub(super) sy: u_int,
    pub(super) manual_sx: u_int,
    pub(super) manual_sy: u_int,
    pub(super) xpixel: u_int,
    pub(super) ypixel: u_int,
    pub(super) new_sx: u_int,
    pub(super) new_sy: u_int,
    pub(super) new_xpixel: u_int,
    pub(super) new_ypixel: u_int,
    pub(super) redraw_scene_generation: uint64_t,
    pub(super) menu: Option<refbox::RefBox<crate::src::shared::menu::menu_data>>,
    pub(super) menu_last_px: u_int,
    pub(super) menu_last_py: u_int,
    pub(super) sb: ::core::ffi::c_int,
    pub(super) sb_pos: ::core::ffi::c_int,
    pub(super) inside_cell: grid_cell,
    pub(super) outside_cell: grid_cell,
    pub(super) flags: ::core::ffi::c_int,
    pub(super) alerts_queued: ::core::ffi::c_int,
    pub(super) options: Option<Box<options>>,
    pub(super) winlinks: window_winlinks,
}
impl window {
    pub(super) fn invalidate_scene(&mut self) {
        self.redraw_scene_generation = self.redraw_scene_generation.wrapping_add(1);
    }
    /// Retain the active pane for the current operation.
    pub(super) fn active_pane(&self) -> Option<std::rc::Rc<std::cell::UnsafeCell<window_pane>>> {
        self.active.upgrade()
    }

    /// The caller supplies a live Rc-backed pane.
    pub(super) fn set_active(
        &mut self,
        pane: Option<&std::rc::Rc<std::cell::UnsafeCell<window_pane>>>,
    ) {
        self.active = pane.map_or_else(std::rc::Weak::new, std::rc::Rc::downgrade);
    }

    pub(super) fn new_ref() -> WindowRef {
        let value = Self {
            id: Default::default(),
            latest: Default::default(),
            name: Default::default(),
            name_event: Default::default(),
            name_time: Default::default(),
            alerts_timer: Default::default(),
            offset_timer: Default::default(),
            activity_time: UNIX_EPOCH,
            creation_time: UNIX_EPOCH,
            active: Default::default(),
            last_panes: Default::default(),
            panes: Default::default(),
            layout: Default::default(),
            sx: Default::default(),
            sy: Default::default(),
            manual_sx: Default::default(),
            manual_sy: Default::default(),
            xpixel: Default::default(),
            ypixel: Default::default(),
            new_sx: Default::default(),
            new_sy: Default::default(),
            new_xpixel: Default::default(),
            new_ypixel: Default::default(),
            redraw_scene_generation: Default::default(),
            menu: Default::default(),
            menu_last_px: Default::default(),
            menu_last_py: Default::default(),
            sb: Default::default(),
            sb_pos: Default::default(),
            inside_cell: Default::default(),
            outside_cell: Default::default(),
            flags: Default::default(),
            alerts_queued: Default::default(),
            options: Default::default(),
            winlinks: Default::default(),
        };
        WindowRef::new(value)
    }
}
