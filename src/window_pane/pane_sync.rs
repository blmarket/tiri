//! Pane-owned synchronized output. Release state before invoking the renderer.
use super::*;
use crate::screen_write::{
    screen_write_initctx, screen_write_redraw_line, screen_write_start_pane, screen_write_stop,
};
use crate::shared::screen::MODE_SYNC;
use crate::shared::tty::tty_ctx;
use std::time::Duration;

pub(super) fn start(pane: &Rc<RefCell<window_pane>>, base: &mut screen) {
    base.mode |= MODE_SYNC;
    let observer = Rc::downgrade(pane);
    let timer = Timer::new(Duration::from_secs(1), move || {
        if let Some(pane) = observer.upgrade() {
            log_debug(format_args!(
                "screen_write_sync_callback: %{} sync timer expired",
                pane.id()
            ));
            let timer = pane.borrow_mut().sync_timer.take();
            drop(timer);
            if pane.is_synchronized() {
                unsafe {
                    let base = &mut (*pane.as_ptr()).base;
                    base.mode &= !MODE_SYNC;
                    flush_dirty(&pane, base);
                }
            }
        }
    })
    .expect("arm timer");
    let replaced = pane.borrow_mut().sync_timer.replace(timer);
    drop(replaced);
    log_debug(format_args!(
        "screen_write_start_sync: %{} started sync mode",
        pane.id()
    ));
}

pub(super) fn stop(pane: &Rc<RefCell<window_pane>>, writing: Option<&mut screen>) {
    let base = match writing {
        Some(base) => base,
        None => unsafe { &mut (*pane.as_ptr()).base },
    };
    if base.mode & MODE_SYNC == 0 {
        return;
    }
    let timer = pane.borrow_mut().sync_timer.take();
    drop(timer);
    base.mode &= !MODE_SYNC;
    flush_dirty(pane, base);
    log_debug(format_args!(
        "screen_write_stop_sync: %{} stopped sync mode",
        pane.id()
    ));
}

pub(super) fn clear_dirty(state: &mut window_pane) {
    state.sync_dirty = None;
    state.sync_dirty_size = 0;
}

pub(super) fn should_draw_rows(
    state: &mut window_pane,
    synchronized: bool,
    mut y: u32,
    mut count: u32,
    height: u32,
) -> bool {
    if state.flags & (PANE_REDRAW | crate::shared::pane::PANE_DROP) != 0 {
        return false;
    }
    if !synchronized {
        return true;
    }
    if y >= height || count == 0 {
        return false;
    }
    count = count.min(height - y);
    if state.sync_dirty.is_none() || state.sync_dirty_size != height {
        if state.sync_dirty.is_some() && state.sync_dirty_size != height {
            y = 0;
            count = height;
        }
        clear_dirty(state);
        let bytes = (height.wrapping_add(7) >> 3) as usize;
        let mut dirty = Vec::<bitstr_t>::new();
        if dirty.try_reserve_exact(bytes).is_err() {
            fatal(&std::io::Error::from_raw_os_error(libc::ENOMEM), |out| {
                out.write_all(b"bit_alloc failed")
            });
        }
        dirty.resize(bytes, 0);
        state.sync_dirty = Some(dirty.into_boxed_slice());
        state.sync_dirty_size = height;
    }
    let dirty = state.sync_dirty.as_mut().expect("allocated bitmap");
    for row in y..y + count {
        dirty[(row >> 3) as usize] |= 1 << (row & 7);
    }
    false
}

/// Draw the rows marked while synchronized; `base` is the pane's base screen.
fn flush_dirty(pane: &Rc<RefCell<window_pane>>, base: &mut screen) {
    let mut r = Vec::new();
    let mut ttyctx = tty_ctx::default();
    let mut y: u_int = 0;
    let mut sy: u_int = base.grid().sy;
    let mut lines: u_int = 0 as u_int;
    if pane.borrow().sync_dirty.is_none() {
        return;
    }
    let mut ctx = screen_write_start_pane(pane, base);
    screen_write_initctx(
        &mut ctx,
        &mut ttyctx,
        1 as ::core::ffi::c_int,
        1 as ::core::ffi::c_int,
    );
    y = 0 as u_int;
    while y < sy {
        let dirty = {
            let pane = pane.borrow();
            let dirty = pane.sync_dirty.as_ref().expect("dirty bitmap present");
            dirty[(y >> 3) as usize] & (1 << (y & 7)) != 0
        };
        if dirty {
            screen_write_redraw_line(&mut ctx, &mut ttyctx, y, &mut r);
            lines = lines.wrapping_add(1);
        }
        y = y.wrapping_add(1);
    }
    log_debug(format_args!(
        "{}: %{} had {} dirty lines",
        "screen_write_flush_dirty",
        pane.id(),
        (lines) as u32
    ));
    screen_write_stop(ctx);
    pane.clear_sync_dirty();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reactor::shutdown_runtime;

    #[test]
    fn stopping_and_destroying_sync_cancel_the_timer_without_retaining_the_pane() {
        use tiri_rt::Runtime as _;
        let runtime = tiri_rt::mio::Runtime::new().unwrap();
        unsafe {
            let pane = window_pane::new();
            {
                let mut state = pane.borrow_mut();
                state.base.grid = Some(crate::grid::grid_create(8, 2, 0));
                state.fd = None;
                state.pipe_fd = None;
            }
            let observer = Rc::downgrade(&pane);
            let pending = || pane.borrow().sync_timer.is_some();
            pane.start_sync(&mut (*pane.as_ptr()).base);
            pane.start_sync(&mut (*pane.as_ptr()).base);
            assert!(pane.is_synchronized());
            assert!(pending());
            assert_eq!(Rc::strong_count(&pane), 1, "timer only observes the pane");
            pane.stop_sync(None);
            assert!(!pane.is_synchronized());
            assert!(!pending());
            pane.stop_sync(None);
            pane.start_sync(&mut (*pane.as_ptr()).base);
            assert!(!pane.should_draw_rows(true, 0, 1, 2));
            super::window_pane_tree::insert(pane.clone());
            pane.destroy();
            assert!(!pending());
            assert!(pane.borrow().sync_dirty.is_none());
            drop(pane);
            assert!(observer.upgrade().is_none());
            shutdown_runtime(runtime);
        }
    }
}
