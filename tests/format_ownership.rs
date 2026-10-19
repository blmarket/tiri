//! Source probe for the format-tree ownership migration: format trees are
//! single-owned boxes, expansion borrows them through `format_expand_state`,
//! and the retired constructors, copy helper and sort static stay gone.
use std::path::{Path, PathBuf};
use syn::{
    visit::{self, Visit},
    Type,
};

const POINTEE: &[&str] = &["format_tree", "format_expand_state", "format_modifier"];
const RETIRED: &[&str] = &[
    "format_copy_state",
    "format_create_with_client",
    "format_create_owned",
    "format_create_box",
];

#[derive(Default)]
struct Audit {
    violations: Vec<String>,
}

impl<'ast> Visit<'ast> for Audit {
    fn visit_type_ptr(&mut self, ty: &'ast syn::TypePtr) {
        if let Type::Path(path) = &*ty.elem {
            if let Some(segment) = path.path.segments.last() {
                let name = segment.ident.to_string();
                if POINTEE.contains(&name.as_str()) {
                    self.violations.push(format!("raw pointer to {name}"));
                }
            }
        }
        visit::visit_type_ptr(self, ty);
    }

    fn visit_ident(&mut self, ident: &'ast syn::Ident) {
        let name = ident.to_string();
        if RETIRED.contains(&name.as_str()) {
            self.violations.push(format!("retired {name}"));
        }
    }

    fn visit_item_static(&mut self, item: &'ast syn::ItemStatic) {
        if matches!(item.mutability, syn::StaticMutability::Mut(_)) && item.ident == "sort_crit" {
            self.violations.push("static mut sort_crit".to_owned());
        }
        visit::visit_item_static(self, item);
    }
}

fn audit(source: &str) -> Vec<String> {
    let mut audit = Audit::default();
    audit.visit_file(&syn::parse_file(source).unwrap());
    audit.violations
}

fn rust_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_sources(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn rejects_format_pointers_and_retired_helpers() {
    for source in [
        "struct S { ft: *mut format_tree }",
        "fn f(es: *const format_expand_state) {}",
        "fn f() { let fm: *mut format_modifier = std::ptr::null_mut(); }",
        "fn f() { format_copy_state(&mut a, &b, 0); }",
        "use crate::format::format_create_with_client;",
        "fn format_create_owned() {}",
        "static mut sort_crit: sort_criteria = X;",
    ] {
        assert!(!audit(source).is_empty(), "{source}");
    }
    for source in [
        "struct S<'a> { ft: &'a mut format_tree }",
        "fn f(sort_crit: &sort_criteria) {}",
        "fn f() { let p: *mut c_char = std::ptr::null_mut(); }",
    ] {
        assert_eq!(audit(source), Vec::<String>::new(), "{source}");
    }
}

#[test]
fn sources_do_not_reintroduce_format_pointers() {
    let mut paths = Vec::new();
    rust_sources(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut paths,
    );
    assert!(!paths.is_empty());
    for path in paths {
        let source = std::fs::read_to_string(&path).unwrap();
        assert_eq!(audit(&source), Vec::<String>::new(), "{}", path.display());
    }
}
