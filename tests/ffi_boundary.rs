//! Parse Rust syntax so comments, wrapping, visibility, and symbol aliases cannot
//! hide a Rust definition behind a foreign declaration.
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
    fn check(directory: &std::path::Path) {
        for entry in std::fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                check(&path);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                let source = std::fs::read_to_string(&path).unwrap();
                let syntax = syn::parse_file(&source)
                    .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
                NoVariadics.visit_file(&syntax);
            }
        }
    }
    check(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"));
}
