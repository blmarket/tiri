//! Style evaluation over scoped option access, without holding a component
//! borrow across format callbacks.

use crate::src::format::{format_create, format_expand_cstring};
use crate::src::grid::grid_default_cell;
use crate::src::options::{options, options_entry, OPTIONS_TABLE_IS_COLOUR};
use crate::src::options::{options_is_string, options_local_entry};
use crate::src::shared::format::{format_tree, FORMAT_NOJOBS};
use crate::src::shared::grid::grid_cell;
use crate::src::shared::style::style;
use crate::src::style::{style_parse, style_parse_colour, style_set};
use std::collections::btree_map;
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
        if let btree_map::Entry::Occupied(mut entry) = options_local_entry(root, key) {
            resolved = style_snapshot(entry.get_mut());
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
