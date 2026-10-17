//! Style evaluation over scoped option access, without holding a component
//! borrow across format callbacks.

use crate::src::format::{format_create, format_expand_cstring};
use crate::src::grid::grid_default_cell;
use crate::src::options::{options_get_only_mut, options_is_string};
use crate::src::shared::format::{format_tree, FORMAT_NOJOBS};
use crate::src::shared::grid::grid_cell;
use crate::src::shared::options::{options, options_entry, OPTIONS_TABLE_IS_COLOUR};
use crate::src::shared::style::style;
use crate::src::style::{style_parse, style_parse_colour, style_set};
use std::ffi::{CStr, CString};

/// Apply the same style/cache semantics as `style_apply`, using a model's scoped
/// component method. The accessor must synchronously call its visitor once.
///
/// Format callbacks may inspect or shadow an inherited option. They must keep
/// the resolved entry alive during evaluation, as required by the existing
/// option-style evaluator. No entry pointer escapes either visit; the resolved
/// owner identifies an inherited entry even if a callback adds an override or
/// changes the receiver's parent.
pub unsafe fn style_apply_with_options(
    cell: &mut grid_cell,
    key: &CStr,
    context: Option<&mut format_tree>,
    mut access: impl FnMut(&mut dyn FnMut(&mut options)),
) {
    *cell = grid_default_cell;
    let mut owned_context = None;
    let context = match context {
        Some(context) => context,
        None => owned_context.insert(format_create(None, None, 0, FORMAT_NOJOBS)),
    };
    let parsed = style_resolve_with_options(key, Some(context), access)
        .unwrap_or(crate::src::style::parsing::style_default);
    if parsed.gc.fg != 8 {
        cell.fg = parsed.gc.fg;
    }
    if parsed.gc.bg != 8 {
        cell.bg = parsed.gc.bg;
    }
    if parsed.gc.us != 8 {
        cell.us = parsed.gc.us;
    }
    cell.attr |= parsed.gc.attr;
    if let Some(context) = owned_context {
        drop(context);
    }
}

/// Resolve a style to a value, releasing option access before format evaluation.
pub unsafe fn style_resolve_with_options(
    key: &CStr,
    context: Option<&mut format_tree>,
    mut access: impl FnMut(&mut dyn FnMut(&mut options)),
) -> Option<style> {
    let mut resolved = None;
    let mut parent = None;
    access(&mut |root| {
        if let Some(entry) = options_get_only_mut(root, key) {
            resolved = style_snapshot(entry);
        } else {
            parent = root.parent.clone();
        }
    });
    if let Some(parent) = parent {
        if let Some(owner) = parent.resolve(key, false) {
            resolved = owner
                .with_entry(key, |entry| style_snapshot(entry))
                .expect("resolved style entry remains live");
        }
    }
    match resolved? {
        Ok(parsed) => parsed,
        Err((value, colour)) => {
            let text = if let Some(context) = context {
                format_expand_cstring(context, value.as_ptr())
            } else {
                value
            };
            style_parse_text(&text, colour)
        }
    }
}

/// Literal values are parsed and cached in place; values containing `#{` are
/// returned for expansion once option access is released.
unsafe fn style_snapshot(
    entry: &mut options_entry,
) -> Option<Result<Option<style>, (CString, bool)>> {
    if options_is_string(entry) == 0 {
        return None;
    }
    if let Some(style) = entry.style {
        return Some(Ok(Some(style)));
    }
    let value = entry.value.string_ptr().expect("string option");
    let colour = entry
        .tableentry_ptr()
        .is_some_and(|table| table.flags & OPTIONS_TABLE_IS_COLOUR != 0);
    if value.to_bytes().windows(2).any(|bytes| bytes == b"#{") {
        return Some(Err((value.to_owned(), colour)));
    }
    let parsed = style_parse_text(value, colour);
    entry.style = parsed;
    Some(Ok(parsed))
}

unsafe fn style_parse_text(text: &CStr, colour: bool) -> Option<style> {
    let mut sy = style::default();
    style_set(&mut sy, &grid_default_cell);
    let failed = if colour {
        style_parse_colour(&mut sy, &grid_default_cell, text)
    } else {
        style_parse(&mut sy, &grid_default_cell, text)
    };
    (failed == 0).then_some(sy)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::src::format::format_add_owned_cb;
    use crate::src::options::{options_create, options_set_string, OptionsScope};
    use crate::src::tmux::{global_options, global_s_options, global_w_options};
    use std::cell::{Cell, RefCell};
    use std::ptr::NonNull;

    thread_local! {
        static MODEL_BORROW: RefCell<()> = const { RefCell::new(()) };
        static EVALUATING: Cell<(*mut options, *mut options, bool)> = const {
            Cell::new((std::ptr::null_mut(), std::ptr::null_mut(), false))
        };
    }

    fn shadow_during_expansion(_: NonNull<format_tree>) -> Option<CString> {
        MODEL_BORROW.with(|model| {
            let _reentrant = model.borrow_mut();
        });
        EVALUATING.with(|slot| unsafe {
            let (parent, child, _) = slot.get();
            let entry = options_get_only_mut(&mut *parent, c"@style").unwrap();
            assert!(entry.style.is_none(), "dynamic style text is never cached");
            options_set_string(child, c"@style", 0, |out| out.write_all(b"fg=blue"));
            (*child).parent = Some(OptionsScope::GlobalSession);
            slot.set((parent, child, true));
        });
        Some(c"red".to_owned())
    }

    #[test]
    fn expansion_can_shadow_and_reparent_after_observing_reset_cache() {
        unsafe {
            let saved = (global_options, global_s_options, global_w_options);
            let mut globals = options_create(None);
            global_options = &mut *globals;
            global_s_options = &mut *globals;
            options_set_string(&mut *globals, c"@style", 0, |out| {
                out.write_all(b"fg=green")
            });
            let mut parent = options_create(None);
            global_w_options = &mut *parent;
            options_set_string(&mut *parent, c"@style", 0, |out| {
                out.write_all(b"fg=#{zz_scoped_style}")
            });
            let mut child = options_create(Some(OptionsScope::GlobalWindow));
            let child_pointer = &mut *child as *mut options;
            EVALUATING.with(|slot| slot.set((&mut *parent, child_pointer, false)));
            let mut context = format_create(None, None, 0, FORMAT_NOJOBS);
            format_add_owned_cb(&mut *context, c"zz_scoped_style", shadow_during_expansion);
            let mut cell = grid_default_cell;
            style_apply_with_options(&mut cell, c"@style", Some(&mut context), |visit| {
                MODEL_BORROW.with(|model| {
                    let _guard = model.borrow_mut();
                    visit(&mut *child_pointer);
                });
            });
            assert!(EVALUATING.with(|slot| slot.get().2));
            assert_eq!(cell.fg, 1, "the in-flight inherited style resolves red");
            assert!(
                options_get_only_mut(&mut parent, c"@style")
                    .unwrap()
                    .style
                    .is_none(),
                "the expanded result is not cached on the resolved parent"
            );
            assert!(
                options_get_only_mut(&mut globals, c"@style")
                    .unwrap()
                    .style
                    .is_none(),
                "reparenting during expansion does not touch the new parent"
            );
            style_apply_with_options(&mut cell, c"@style", Some(&mut context), |visit| {
                MODEL_BORROW.with(|model| {
                    let _guard = model.borrow_mut();
                    visit(&mut *child_pointer);
                });
            });
            assert_eq!(
                cell.fg, 4,
                "the next evaluation uses the new local override"
            );
            assert_eq!(
                options_get_only_mut(&mut child, c"@style")
                    .unwrap()
                    .style
                    .map(|style| style.gc.fg),
                Some(4)
            );
            drop(context);
            EVALUATING.with(|slot| slot.set((std::ptr::null_mut(), std::ptr::null_mut(), false)));
            (global_options, global_s_options, global_w_options) = saved;
        }
    }
}
