//! Check specific translated storage roles without requiring global name uniqueness.
use std::{
    fs,
    path::{Path, PathBuf},
};

fn source_files(root: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(root).expect("read source directory") {
        let path = entry.expect("read source entry").path();
        if path.is_dir() {
            source_files(&path, files);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            files.push(path);
        }
    }
}

fn tokens(source: &str) -> Vec<String> {
    let b = source.as_bytes();
    let mut result = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i..].starts_with(b"//") {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
        } else if b[i..].starts_with(b"/*") {
            i += 2;
            let mut depth = 1;
            while i < b.len() && depth != 0 {
                if b[i..].starts_with(b"/*") {
                    depth += 1;
                    i += 2;
                } else if b[i..].starts_with(b"*/") {
                    depth -= 1;
                    i += 2;
                } else {
                    i += 1;
                }
            }
        } else if b[i] == b'r' && matches!(b.get(i + 1), Some(b'#' | b'"')) {
            let start = i;
            i += 1;
            while b.get(i) == Some(&b'#') {
                i += 1;
            }
            if b.get(i) == Some(&b'"') {
                let hashes = i - start - 1;
                i += 1;
                while i < b.len() {
                    if b[i] == b'"'
                        && b.get(i + 1..i + 1 + hashes)
                            .is_some_and(|tail| tail.iter().all(|c| *c == b'#'))
                    {
                        i += 1 + hashes;
                        break;
                    }
                    i += 1;
                }
            } else {
                i = start + 2;
            }
        } else if b[i] == b'\'' {
            let start = i + 1;
            let end = if b.get(start) == Some(&b'\\') {
                match b.get(start + 1) {
                    Some(b'u') => source[start..].find('}').map(|n| start + n + 1),
                    Some(b'x') => Some(start + 4),
                    Some(_) => Some(start + 2),
                    None => None,
                }
            } else {
                source[start..].chars().next().map(|c| start + c.len_utf8())
            };
            i = match end {
                Some(end) if b.get(end) == Some(&b'\'') => end + 1,
                _ => i + 1,
            };
        } else if b[i] == b'"' {
            i += 1;
            while i < b.len() {
                if b[i] == b'\\' {
                    i += 2;
                } else if b[i] == b'"' {
                    i += 1;
                    break;
                } else {
                    i += 1;
                }
            }
        } else if b[i].is_ascii_alphabetic() || b[i] == b'_' {
            let start = i;
            i += 1;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            result.push(source[start..i].to_owned());
        } else {
            if !b[i].is_ascii_whitespace() {
                result.push((b[i] as char).to_string());
            }
            i += 1;
        }
    }
    result
}

fn grid_storage_candidates(source: &str) -> Vec<(String, &'static str)> {
    let words = tokens(source);
    let mut found = Vec::new();
    for (i, pair) in words.windows(2).enumerate() {
        if !matches!(pair[0].as_str(), "struct" | "union") {
            continue;
        }
        if words.get(i + 2).map(String::as_str) != Some("{") {
            continue;
        }
        let mut fields = Vec::new();
        let mut depth = 0;
        for j in i + 3..words.len() {
            match words[j].as_str() {
                "}" if depth == 0 => break,
                "{" | "(" | "[" | "<" => depth += 1,
                "}" | ")" | "]" | ">" => depth -= 1,
                ":" if depth == 0
                    && words[j - 1] != ":"
                    && words.get(j + 1).map(String::as_str) != Some(":") =>
                {
                    fields.push(words[j - 1].as_str());
                }
                _ => {}
            }
        }
        let role = match (pair[0].as_str(), fields.as_slice()) {
            ("union", ["offset", "data"]) => "grid_cell_entry_storage",
            ("struct", ["attr", "fg", "bg", "data"]) => "grid_cell_entry_data",
            _ => continue,
        };
        found.push((pair[1].clone(), role));
    }
    found
}

#[test]
fn grid_storage_roles_have_only_authoritative_definitions() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = vec![root.join("build.rs")];
    source_files(&root.join("src"), &mut files);
    let mut count = 0;
    for path in files {
        for (name, role) in grid_storage_candidates(&fs::read_to_string(&path).unwrap()) {
            assert_eq!(
                path.strip_prefix(root).unwrap(),
                Path::new("src/shared/grid.rs"),
                "redundant or unaudited grid-storage candidate {name} ({role})"
            );
            assert_eq!(name, role);
            count += 1;
        }
    }
    assert_eq!(
        count, 2,
        "both authoritative storage roles must be inventoried"
    );
}

#[test]
fn grid_role_guard_uses_fields_instead_of_generated_names() {
    assert_eq!(
        grid_storage_candidates(
            r#"
        /* union Ignored { offset: u32, data: Other } */
        const TEXT: &str = "struct Ignored { attr: u8, fg: u8, bg: u8, data: u8 }";
        pub(crate) union
        Renumbered { pub offset: u64, pub data: DifferentName }
        struct DifferentName { attr: u8, fg: u8, bg: u8, data: u8 }
        struct C2RustUnnamed { mask: u32, code: u32 }
    "#
        ),
        vec![
            ("Renumbered".into(), "grid_cell_entry_storage"),
            ("DifferentName".into(), "grid_cell_entry_data"),
        ]
    );
}

fn anonymous_key_enum_candidates(source: &str) -> Vec<String> {
    let words = tokens(source);
    let mut found = Vec::new();
    for (i, pair) in words.windows(2).enumerate() {
        if pair[0] != "type" || !pair[1].starts_with("C2RustUnnamed") {
            continue;
        }
        let rhs: Vec<_> = words[i + 2..].iter().take_while(|s| *s != ";").collect();
        if rhs
            .iter()
            .any(|s| matches!(s.as_str(), "c_ulong" | "key_code_enum"))
        {
            found.push(pair[1].clone());
        }
    }
    found
}

#[test]
fn key_enum_guard_handles_wrapping_and_unrelated_generated_domains() {
    assert_eq!(
        anonymous_key_enum_candidates(
            r#"
        // type C2RustUnnamedIgnored = c_ulong;
        const TEXT: &str = "type C2RustUnnamedIgnored = c_ulong;";
        pub(crate) type
            C2RustUnnamed_999 = ::core::ffi::c_ulong;
        type C2RustUnnamed_998 = crate::src::shared::key::key_code_enum;
        type C2RustUnnamed_35 = ::core::ffi::c_uint;
        struct C2RustUnnamed_38 { mask: u32, code: u32 }
        pub use crate::src::shared::key::key_code_enum as C2RustUnnamed_997;
    "#
        ),
        ["C2RustUnnamed_999", "C2RustUnnamed_998"]
    );
}
