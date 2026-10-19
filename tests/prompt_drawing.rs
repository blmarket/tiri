use std::ffi::CStr;
use tiri::environ::environ_create;
use tiri::grid::{grid_default_cell, grid_get_cell, grid_string_cells_bytes};
use tiri::options::options_table;
use tiri::options::OptionsScope;
use tiri::prompt::{prompt_create, prompt_draw, prompt_free};
use tiri::screen::screen_init;
use tiri::screen_write::{screen_write_start, screen_write_stop};
use tiri::shared::display::{SCREEN_CURSOR_BLOCK, SCREEN_CURSOR_UNDERLINE};
use tiri::shared::grid::{GRID_ATTR_UNDERSCORE, GRID_STRING_TRIM_SPACES};
use tiri::shared::prompt::*;
use tiri::shared::screen::{screen, MODE_CURSOR, MODE_CURSOR_BLINKING};
use tiri::shared::screen_write::screen_write_ctx;
use tiri::tmux::global_environ;

#[test]
fn drawing_preserves_tmux_alignment_clipping_completion_and_cursor_style() {
    unsafe {
        assert!(!libc::setlocale(libc::LC_CTYPE, c"C.UTF-8".as_ptr()).is_null());
        let saved = std::ptr::replace(&raw mut global_environ, None);
        let environment = environ_create();
        global_environ = Some(environment);
        let definition = options_table
            .iter()
            .find(|entry| entry.name == Some(c"extended-keys"))
            .unwrap();
        OptionsScope::GlobalServer.reset_default(definition);

        for (label, input, index, width, style, flags, completion, expected, cursor) in [
            (c"L:", c"abc", 3, 10, c"align=left", 0, None, "   L:abc", 8),
            (
                c"L:",
                c"abc",
                3,
                10,
                c"align=centre",
                0,
                None,
                "     L:abc",
                10,
            ),
            (
                c"L:",
                c"abc",
                3,
                10,
                c"align=right",
                0,
                None,
                "        L:abc",
                13,
            ),
            (
                c"L:",
                c"abcdefghijk",
                11,
                8,
                c"align=left",
                0,
                None,
                "   L:ghijk",
                10,
            ),
            (
                c"L:",
                c"abcdefghijk",
                2,
                8,
                c"align=left",
                0,
                None,
                "   L:abcdef",
                7,
            ),
            (c"LONG", c"abc", 3, 2, c"align=left", 0, None, "   LO", 5),
            (c"L:", c"abc", 3, 0, c"align=left", 0, None, "", 0),
            (
                c"L:",
                c"é漢Z",
                3,
                10,
                c"align=left",
                0,
                None,
                "   L:é漢Z",
                9,
            ),
            (
                c"L:",
                c"abc",
                1,
                10,
                c"align=left",
                PROMPT_QUOTENEXT,
                None,
                "   L:a^bc",
                6,
            ),
            (
                c"L:",
                c"ab",
                2,
                10,
                c"align=left",
                0,
                Some(c" done"),
                "   L:ab done",
                7,
            ),
            (
                c"L:",
                c"abc",
                3,
                10,
                c"align=left",
                PROMPT_COMMANDMODE,
                None,
                "        L:abc",
                13,
            ),
            (
                c"L:",
                c"abc",
                3,
                10,
                c"invalid-style",
                0,
                None,
                "   L:abc",
                8,
            ),
            (
                c"#{prompt_flags}|#{prompt_type}:",
                c"abc",
                3,
                40,
                c"align=left",
                0,
                None,
                "   NOFORMAT|command:abc",
                23,
            ),
            (
                c"#{prompt_flags}|#{prompt_type}:",
                c"abc",
                3,
                40,
                c"align=left",
                PROMPT_EDITARROWS,
                None,
                "   NOFORMAT,EDITARROWS|command:abc",
                34,
            ),
        ] {
            let prompt = prompt_create(prompt_create_data {
                prompt: label,
                input: Some(input),
                flags: flags | PROMPT_NOFORMAT,
                style: grid_default_cell,
                command_style: grid_default_cell,
                style_str: style.to_owned(),
                command_style_str: c"align=right".to_owned(),
                message_format: c"#{message}".to_owned(),
                cstyle: SCREEN_CURSOR_UNDERLINE,
                cmode: MODE_CURSOR,
                ccolour: 1,
                command_cstyle: SCREEN_CURSOR_BLOCK,
                command_cmode: MODE_CURSOR | MODE_CURSOR_BLINKING,
                command_ccolour: 2,
                ..Default::default()
            });
            prompt
                .try_borrow_mut()
                .expect("live unborrowed prompt")
                .index = index;
            prompt
                .try_borrow_mut()
                .expect("live unborrowed prompt")
                .completion
                .display = completion.map(CStr::to_owned);
            let mut s = screen::empty();
            screen_init(&mut s, 80, 1, 0);
            let mut ctx = screen_write_start(&mut s);
            let cursor_x = prompt_draw(
                &prompt.try_borrow_mut().expect("live unborrowed prompt"),
                &mut ctx,
                prompt_draw_data {
                    area_x: 3,
                    area_width: width,
                    prompt_line: 0,
                },
            );
            screen_write_stop(ctx);
            let text =
                grid_string_cells_bytes(s.grid(), 0, 0, 80, None, GRID_STRING_TRIM_SPACES, None);
            assert_eq!(
                text,
                expected.as_bytes(),
                "{input:?}, {style:?}, flags {flags}, width {width}"
            );
            assert_eq!(
                cursor_x, cursor,
                "{input:?}, {style:?}, flags {flags}, width {width}"
            );
            let appearance = if flags & PROMPT_COMMANDMODE != 0 {
                (SCREEN_CURSOR_BLOCK, MODE_CURSOR | MODE_CURSOR_BLINKING, 2)
            } else {
                (SCREEN_CURSOR_UNDERLINE, MODE_CURSOR, 1)
            };
            assert_eq!(
                (s.default_cstyle, s.default_mode, s.default_ccolour),
                appearance
            );
            if completion.is_some() {
                let mut cell = grid_default_cell;
                grid_get_cell(s.grid(), 8, 0, &mut cell);
                assert_ne!(cell.attr as i32 & GRID_ATTR_UNDERSCORE, 0);
            }
            drop(s);
            prompt_free(&prompt.downgrade());
        }
        global_environ = saved;
    }
}
