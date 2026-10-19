use super::window_pane;
use crate::format::bytes::format_bytes;
use crate::grid::grid_default_cell;
use crate::screen_write::{
    screen_write_box, screen_write_clearcharacter, screen_write_cursormove, screen_write_nputs,
    screen_write_start, screen_write_stop,
};
use crate::shared::abi::{pid_t, size_t, ssize_t, u_int};
use crate::shared::layout::BOX_LINES_DEFAULT;
use std::{cell::UnsafeCell, rc::Rc};

pub(super) unsafe fn draw_editor_waiting(owner: &Rc<UnsafeCell<window_pane>>, pid: pid_t) {
    (*owner.get())
        .with_screen_mut(|s| {
            let mut gc = grid_default_cell;
            let mut sx: u_int = 0;
            let mut sy: u_int = 0;
            let mut box_w: u_int = 0;
            let mut box_h: u_int = 0;
            let mut x: u_int = 0;
            let mut y: u_int = 0;
            let mut text_x: u_int = 0;
            let mut textlen: size_t = 0;
            sx = s.grid().sx;
            sy = s.grid().sy;
            if sx == 0 as u_int || sy == 0 as u_int {
                return;
            }
            let text = if pid == -(1 as ::core::ffi::c_int) {
                b"WAITING FOR EDITOR".to_vec()
            } else {
                format_bytes(format_args!("WAITING FOR EDITOR (PID {})", pid))
            };
            textlen = text.len();
            box_w = textlen.wrapping_add(4 as size_t) as u_int;
            box_h = 3 as u_int;
            if sx < box_w || sy < box_h {
                return;
            }
            x = sx.wrapping_sub(box_w).wrapping_div(2 as u_int);
            y = sy.wrapping_sub(box_h).wrapping_div(2 as u_int);
            text_x = (x as size_t).wrapping_add(
                (box_w as size_t)
                    .wrapping_sub(textlen)
                    .wrapping_div(2 as size_t),
            ) as u_int;
            let mut ctx = screen_write_start(&mut *s);
            screen_write_cursormove(
                &mut ctx,
                x as ::core::ffi::c_int,
                y as ::core::ffi::c_int,
                0 as ::core::ffi::c_int,
            );
            screen_write_box(&mut ctx, box_w, box_h, BOX_LINES_DEFAULT, Some(&gc), None);
            screen_write_cursormove(
                &mut ctx,
                x.wrapping_add(1 as u_int) as ::core::ffi::c_int,
                y.wrapping_add(1 as u_int) as ::core::ffi::c_int,
                0 as ::core::ffi::c_int,
            );
            screen_write_clearcharacter(&mut ctx, box_w.wrapping_sub(2 as u_int), gc.bg as u_int);
            screen_write_cursormove(
                &mut ctx,
                text_x as ::core::ffi::c_int,
                y.wrapping_add(1 as u_int) as ::core::ffi::c_int,
                0 as ::core::ffi::c_int,
            );
            screen_write_nputs(
                &mut ctx,
                box_w.wrapping_sub(2 as u_int) as ssize_t,
                &gc,
                |out| out.write_all(&text),
            );
            screen_write_stop(ctx);
        })
        .expect("displayed pane screen");
}
