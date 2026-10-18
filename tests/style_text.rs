//! Compatibility observations recorded before replacing the text APIs.
use tiri::shared::colour::COLOUR_FLAG_256;
use tiri::{shared::grid::*, style::attributes::*, style::colour::*};

#[test]
fn safe_formatting_is_independent_across_threads() {
    let threads: Vec<_> = (0..8)
        .map(|n| {
            std::thread::spawn(move || {
                let text = colour_format(COLOUR_FLAG_256 | n);
                let attrs = attributes_format(GRID_ATTR_BRIGHT);
                for m in 0..256 {
                    assert_eq!(
                        colour_parse_cstr(&colour_format(COLOUR_FLAG_256 | m)),
                        Some(COLOUR_FLAG_256 | m)
                    );
                    assert_eq!(
                        attributes_parse_cstr(&attributes_format(GRID_ATTR_DIM)),
                        Some(GRID_ATTR_DIM)
                    );
                }
                assert_eq!(text.to_bytes(), format!("colour{n}").as_bytes());
                assert_eq!(attrs.to_bytes(), b"bright");
            })
        })
        .collect();
    for thread in threads {
        thread.join().unwrap();
    }
}
