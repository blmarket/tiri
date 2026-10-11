//! Style evaluation over scoped option access, without holding a component
//! borrow across format callbacks.

use crate::src::format::{format_create, format_expand_cstring};
use crate::src::grid::grid_default_cell;
use crate::src::options::{options_get_only_mut, options_is_string, OptionsScope};
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
    enum Source {
        Local,
        Inherited(OptionsScope),
    }

    let mut source = None;
    let mut resolved = None;
    let mut parent = None;
    access(&mut |root| {
        if let Some(entry) = options_get_only_mut(root, key) {
            source = Some(Source::Local);
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
            source = Some(Source::Inherited(owner));
        }
    }
    let parsed = match resolved {
        None => None,
        Some(Ok(style)) => Some(style),
        Some(Err((value, colour, expand))) => {
            let text = if let Some(context) = context.filter(|_| expand) {
                format_expand_cstring(context, value.as_ptr())
            } else {
                value
            };
            let mut parsed = None;
            let mut parse = |entry: &mut options_entry| {
                let failed = if colour {
                    style_parse_colour(&mut entry.style, &grid_default_cell, &text)
                } else {
                    style_parse(&mut entry.style, &grid_default_cell, &text)
                };
                if failed == 0 {
                    parsed = Some(entry.style);
                }
            };
            match source.expect("resolved style has an owner") {
                Source::Local => access(&mut |root| {
                    parse(options_get_only_mut(root, key).expect("style entry remains live"));
                }),
                Source::Inherited(owner) => {
                    owner
                        .with_entry(key, parse)
                        .expect("style entry remains live");
                }
            }
            parsed
        }
    };
    parsed
}

unsafe fn style_snapshot(
    entry: &mut options_entry,
) -> Option<Result<style, (CString, bool, bool)>> {
    if options_is_string(entry) == 0 {
        return None;
    }
    if entry.cached != 0 {
        return Some(Ok(entry.style));
    }
    let value = entry.value.string_ptr().expect("string option").to_owned();
    let colour = entry
        .tableentry_ptr()
        .is_some_and(|table| table.flags & OPTIONS_TABLE_IS_COLOUR != 0);
    let expand = value.as_bytes().windows(2).any(|bytes| bytes == b"#{");
    // Publish the default and cache state before a format callback can
    // recursively observe this option, just as the old API did.
    style_set(&mut entry.style, &grid_default_cell);
    entry.cached = (!expand) as i32;
    Some(Err((value, colour, expand)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::src::format::format_add_owned_cb;
    use crate::src::options::{options_create, options_set_string};
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
            assert_eq!(entry.cached, 0);
            assert_eq!(
                entry.style.gc.fg, grid_default_cell.fg,
                "cache resets before evaluating dynamic style text"
            );
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
            options_get_only_mut(&mut globals, c"@style")
                .unwrap()
                .style
                .gc
                .fg = 321;
            let mut parent = options_create(None);
            global_w_options = &mut *parent;
            options_set_string(&mut *parent, c"@style", 0, |out| {
                out.write_all(b"fg=#{zz_scoped_style}")
            });
            options_get_only_mut(&mut parent, c"@style")
                .unwrap()
                .style
                .gc
                .fg = 123;
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
            assert_eq!(
                options_get_only_mut(&mut parent, c"@style")
                    .unwrap()
                    .style
                    .gc
                    .fg,
                1,
                "write parsed cache back to the originally resolved parent"
            );
            assert_eq!(
                options_get_only_mut(&mut globals, c"@style")
                    .unwrap()
                    .style
                    .gc
                    .fg,
                321,
                "reparenting cannot redirect the in-flight cache update"
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
                options_get_only_mut(&mut child, c"@style").unwrap().cached,
                1
            );
            drop(context);
            EVALUATING.with(|slot| slot.set((std::ptr::null_mut(), std::ptr::null_mut(), false)));
            (global_options, global_s_options, global_w_options) = saved;
        }
    }
}
