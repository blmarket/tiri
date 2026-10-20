//! Parse Rust syntax so comments, wrapping, visibility, and symbol aliases cannot
//! hide a Rust definition behind a foreign declaration.
use std::path::{Path, PathBuf};
use syn::{
    visit::{self, Visit},
    Attribute, Expr, ForeignItem, Item, Lit, Meta,
};

fn symbol(attrs: &[Attribute], attr: &str, fallback: &str) -> String {
    for a in attrs {
        let meta = if a.path().is_ident("unsafe") {
            a.parse_args::<Meta>().expect("unsafe attribute")
        } else {
            a.meta.clone()
        };
        if let Meta::NameValue(nv) = meta {
            if nv.path.is_ident(attr) {
                if let Expr::Lit(expr) = nv.value {
                    if let Lit::Str(value) = expr.lit {
                        return value.value();
                    }
                }
            }
        }
    }
    fallback.to_owned()
}

#[derive(Default)]
struct Symbols {
    defined: Vec<String>,
    foreign: Vec<String>,
    blocks: usize,
    exported: Vec<String>,
}
impl<'ast> Visit<'ast> for Symbols {
    fn visit_item(&mut self, item: &'ast Item) {
        match item {
            Item::Fn(f) => {
                self.defined
                    .push(symbol(&f.attrs, "export_name", &f.sig.ident.to_string()))
            }
            Item::Static(s) => {
                self.defined
                    .push(symbol(&s.attrs, "export_name", &s.ident.to_string()))
            }
            Item::ForeignMod(_) => self.blocks += 1,
            Item::Verbatim(tokens) => panic!("unparsed item: {tokens}"),
            _ => {}
        }
        let export = match item {
            Item::Fn(f) => Some((&f.attrs, f.sig.ident.to_string())),
            Item::Static(s) => Some((&s.attrs, s.ident.to_string())),
            _ => None,
        };
        if let Some((attrs, name)) = export {
            if attrs.iter().any(|a| {
                let meta = if a.path().is_ident("unsafe") {
                    a.parse_args::<Meta>().expect("unsafe attribute")
                } else {
                    a.meta.clone()
                };
                meta.path().is_ident("no_mangle") || meta.path().is_ident("export_name")
            }) {
                self.exported.push(symbol(attrs, "export_name", &name));
            }
        }
        visit::visit_item(self, item);
    }
    fn visit_foreign_item(&mut self, item: &'ast ForeignItem) {
        match item {
            ForeignItem::Fn(f) => {
                self.foreign
                    .push(symbol(&f.attrs, "link_name", &f.sig.ident.to_string()))
            }
            ForeignItem::Static(s) => {
                self.foreign
                    .push(symbol(&s.attrs, "link_name", &s.ident.to_string()))
            }
            ForeignItem::Verbatim(tokens) => panic!("unparsed foreign item: {tokens}"),
            _ => {}
        }
        visit::visit_foreign_item(self, item);
    }
}
fn scan(source: &str) -> Symbols {
    let mut symbols = Symbols::default();
    symbols.visit_file(&syn::parse_file(source).expect("parse Rust source"));
    symbols
}

fn rust_files(directory: &Path, files: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
}

fn parse(path: &Path) -> syn::File {
    let source = std::fs::read_to_string(path).unwrap();
    syn::parse_file(&source).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// Foreign declarations outside `src/ffi/` that predate the rule, relative to
/// the workspace root.
const FOREIGN_OUTSIDE_FFI: &[&str] = &["tiri-rt/src/unix.rs"];

/// The files among `files` that declare an `extern` block although they are
/// neither under `src/ffi/` nor in [`FOREIGN_OUTSIDE_FFI`].
fn misplaced_foreign_blocks(root: &Path, files: &[PathBuf]) -> Vec<String> {
    files
        .iter()
        .filter_map(|path| {
            let relative = path.strip_prefix(root).unwrap().to_str().unwrap();
            if relative.starts_with("src/ffi/") || FOREIGN_OUTSIDE_FFI.contains(&relative) {
                return None;
            }
            let mut symbols = Symbols::default();
            symbols.visit_file(&parse(path));
            (symbols.blocks != 0).then(|| relative.to_owned())
        })
        .collect()
}

#[test]
fn inventory_handles_aliases_callbacks_variadics_and_misleading_text() {
    let s = scan(
        r###"
        // extern "C" { fn ignored(); }
        const TEXT: &str = r#"extern "C" { fn also_ignored(); }"#;
        #[unsafe(export_name = "real_fn")]
        pub(crate) unsafe extern "C" fn renamed(_: i32, mut ap: ...) {}
        #[export_name = "real_global"] static mut VALUE: i32 = 0;
        mod nested {
            extern "C" {
                #[link_name = "real_fn"] pub fn alias(_: Option<unsafe extern "C" fn(i32)>, ...);
                #[link_name = "real_global"] static mut OTHER: i32;
                pub type opaque;
            }
        }
    "###,
    );
    assert_eq!(s.defined, ["real_fn", "real_global"]);
    assert_eq!(s.exported, ["real_fn", "real_global"]);
    assert_eq!(s.foreign, ["real_fn", "real_global"]);
    assert_eq!(s.blocks, 1);
    let s = scan("fn local() {} static G: i32 = 0; extern \"C\" { fn local(); static G: i32; }");
    assert_eq!(s.defined, s.foreign);
}

#[test]
fn rust_implementations_do_not_define_c_variadic_functions() {
    struct NoVariadics;
    impl<'ast> Visit<'ast> for NoVariadics {
        fn visit_item_fn(&mut self, function: &'ast syn::ItemFn) {
            assert!(
                function.sig.variadic.is_none(),
                "Rust variadic definition: {}",
                function.sig.ident
            );
            visit::visit_item_fn(self, function);
        }
    }
    let mut files = Vec::new();
    rust_files(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut files,
    );
    for path in files {
        NoVariadics.visit_file(&parse(&path));
    }
}

#[test]
fn foreign_declarations_live_under_src_ffi() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    for crate_dir in [
        "",
        "tiri-agent",
        "tiri-buffer",
        "tiri-cmdparse",
        "tiri-refbox",
        "tiri-rt",
        "tiri-sdbus",
    ] {
        rust_files(&root.join(crate_dir).join("src"), &mut files);
    }
    let misplaced = misplaced_foreign_blocks(root, &files);
    assert!(
        misplaced.is_empty(),
        "extern blocks outside src/ffi/: {misplaced:?}; move them there, \
         or add the file to FOREIGN_OUTSIDE_FFI in tests/ffi_boundary.rs"
    );
}

#[test]
fn a_foreign_declaration_shadowing_rust_outside_src_ffi_is_reported() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let fixture = "tests/fixtures/architecture/foreign-outside-ffi.rs";
    let symbols = scan(&std::fs::read_to_string(root.join(fixture)).unwrap());
    assert_eq!(symbols.defined, symbols.foreign);
    assert_eq!(
        misplaced_foreign_blocks(root, &[root.join(fixture)]),
        [fixture]
    );
}
