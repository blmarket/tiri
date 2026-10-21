use std::collections::btree_map;
use tiri::options::*;
use tiri::options::{options, options_entry};
use tiri::shared::colour::{colour_palette, COLOUR_FLAG_256};
use tiri::style::colour::*;

#[test]
fn overrides_fall_back_to_defaults_and_clear_preserves_defaults() {
    let mut p = colour_palette::default();
    colour_palette_init(&mut p);
    let mut defaults = Box::new([-1; 256]);
    defaults[1] = 2;
    p.default_palette = Some(defaults);
    assert_eq!((p.fg, p.bg), (8, 8));
    assert_eq!(colour_palette_get(Some(&p), 1), 2);
    assert_eq!(colour_palette_set(Some(&mut p), 1, 3), 1);
    assert_eq!(colour_palette_get(Some(&p), 1), 3);
    assert_eq!(colour_palette_set(Some(&mut p), 1, -1), 1);
    assert_eq!(colour_palette_get(Some(&p), 1), 2);
    colour_palette_set(Some(&mut p), 1, 4);
    p.fg = 1;
    p.bg = 2;
    colour_palette_clear(Some(&mut p));
    assert_eq!((p.fg, p.bg), (8, 8));
    assert!(p.palette.is_none());
    assert_eq!(colour_palette_get(Some(&p), 1), 2);
    colour_palette_set(Some(&mut p), 1, 5);
    colour_palette_init(&mut p);
    assert!(p.palette.is_none());
}

#[test]
fn indices_are_normalized_and_invalid_indices_are_rejected() {
    let mut p = colour_palette::default();
    assert_eq!(colour_palette_set(Some(&mut p), 0, -1), 0);
    assert!(p.palette.is_none());
    for n in [0, 7, 8, 15, 255] {
        assert_eq!(colour_palette_set(Some(&mut p), n, n + 100), 1);
        assert_eq!(colour_palette_get(Some(&p), COLOUR_FLAG_256 | n), n + 100);
    }
    assert_eq!(colour_palette_get(Some(&p), 0), 100);
    assert_eq!(colour_palette_get(Some(&p), 7), 107);
    assert_eq!(colour_palette_get(Some(&p), 90), 108);
    assert_eq!(colour_palette_get(Some(&p), 97), 115);
    for n in [-1, i32::MIN, 8, 255, COLOUR_FLAG_256 | 256, i32::MAX] {
        assert_eq!(colour_palette_get(Some(&p), n), -1);
    }
    for n in [-1, 256, i32::MAX] {
        assert_eq!(colour_palette_set(Some(&mut p), n, 1), 0);
    }
    assert_eq!(colour_palette_get(None, 0), -1);
    assert_eq!(colour_palette_set(None, 0, 1), 0);
    colour_palette_clear(None);
    colour_palette_from_option(None, &OptionsScope::GlobalWindow);
}

fn pane_colours(oo: &mut options) -> &mut options_entry {
    match options_local_entry(oo, c"pane-colours") {
        btree_map::Entry::Occupied(entry) => entry.into_mut(),
        btree_map::Entry::Vacant(_) => panic!("pane-colours is set"),
    }
}

#[test]
fn option_reload_replaces_defaults_without_changing_overrides() {
    let scope = OptionsScope::GlobalWindow;
    let definition = tiri::options::options_table
        .iter()
        .find(|oe| oe.name == Some(c"pane-colours"))
        .unwrap();
    scope.with_local(|oo| {
        options_empty(oo, definition);
    });
    let mut p = colour_palette::default();
    colour_palette_from_option(Some(&mut p), &scope);
    assert!(p.default_palette.is_none());
    assert_eq!(
        scope.with_local(|oo| pane_colours(oo).value.array_mut().unwrap().set(
            definition,
            c"1",
            Some(c"red"),
            false
        )),
        Ok(())
    );
    colour_palette_from_option(Some(&mut p), &scope);
    assert_eq!(colour_palette_get(Some(&p), 1), 1);
    colour_palette_set(Some(&mut p), 1, 4);
    scope.with_local(|oo| pane_colours(oo).value.array_mut().unwrap().clear());
    assert_eq!(
        scope.with_local(|oo| pane_colours(oo).value.array_mut().unwrap().set(
            definition,
            c"255",
            Some(c"green"),
            false
        )),
        Ok(())
    );
    colour_palette_from_option(Some(&mut p), &scope);
    assert_eq!(colour_palette_get(Some(&p), 1), 4);
    colour_palette_set(Some(&mut p), 1, -1);
    assert_eq!(colour_palette_get(Some(&p), 1), -1);
    assert_eq!(colour_palette_get(Some(&p), COLOUR_FLAG_256 | 255), 2);
    scope.with_local(|oo| pane_colours(oo).value.array_mut().unwrap().clear());
    colour_palette_from_option(Some(&mut p), &scope);
    assert!(p.default_palette.is_none());
    assert_eq!(colour_palette_get(Some(&p), COLOUR_FLAG_256 | 255), -1);
}
