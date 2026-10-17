//! Queue ownership must remain visible at Rust API and storage boundaries.
use std::path::Path;
use syn::visit::{self, Visit};

#[derive(Default)]
struct ItemType(bool);
impl<'ast> Visit<'ast> for ItemType {
    fn visit_type_path(&mut self, ty: &'ast syn::TypePath) {
        self.0 |= ty
            .path
            .segments
            .last()
            .is_some_and(|part| part.ident == "cmdq_item");
        visit::visit_type_path(self, ty);
    }
}

#[derive(Default)]
struct RawItem(bool);
impl<'ast> Visit<'ast> for RawItem {
    fn visit_type_ptr(&mut self, ty: &'ast syn::TypePtr) {
        let mut item = ItemType::default();
        item.visit_type(&ty.elem);
        self.0 |= item.0;
        visit::visit_type_ptr(self, ty);
    }
    fn visit_type_path(&mut self, ty: &'ast syn::TypePath) {
        if ty
            .path
            .segments
            .last()
            .is_some_and(|part| part.ident == "NonNull")
        {
            let mut item = ItemType::default();
            item.visit_type_path(ty);
            self.0 |= item.0;
        }
        visit::visit_type_path(self, ty);
    }
}

struct Interfaces<'a>(&'a Path);
impl Interfaces<'_> {
    fn check(&self, ty: &syn::Type) {
        let mut raw = RawItem::default();
        raw.visit_type(ty);
        assert!(!raw.0, "raw queue item in interface: {}", self.0.display());
    }
}
impl<'ast> Visit<'ast> for Interfaces<'_> {
    fn visit_signature(&mut self, sig: &'ast syn::Signature) {
        for arg in &sig.inputs {
            if let syn::FnArg::Typed(arg) = arg {
                self.check(&arg.ty);
            }
        }
        if let syn::ReturnType::Type(_, ty) = &sig.output {
            self.check(ty);
        }
    }
    fn visit_field(&mut self, field: &'ast syn::Field) {
        self.check(&field.ty);
    }
    fn visit_item_static(&mut self, item: &'ast syn::ItemStatic) {
        self.check(&item.ty);
    }
    fn visit_item_type(&mut self, item: &'ast syn::ItemType) {
        self.check(&item.ty);
        if let syn::Type::Path(ty) = &*item.ty {
            let name = &ty.path.segments.last().unwrap().ident;
            let mut queue_item = ItemType::default();
            queue_item.visit_type(&item.ty);
            assert!(
                !(queue_item.0 && (name == "Rc" || name == "Weak")),
                "queue item Rc/Weak alias: {}",
                self.0.display()
            );
        }
    }
}

fn inspect(path: &Path) {
    if path.is_dir() {
        for entry in std::fs::read_dir(path).unwrap() {
            inspect(&entry.unwrap().path());
        }
    } else if path.extension().is_some_and(|ext| ext == "rs") {
        let source = std::fs::read_to_string(path).unwrap();
        Interfaces(path).visit_file(&syn::parse_file(&source).unwrap());
    }
}

#[test]
fn queue_item_interfaces_express_ownership_without_raw_transfers() {
    inspect(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"));
    let queue = include_str!("../src/cmd/queue.rs");
    assert!(!queue.contains("Rc::into_raw"));
    assert!(!queue.contains("Rc::from_raw"));
}

#[test]
fn audit_catches_nested_callbacks_and_output_pointers() {
    for ty in [
        "Option<unsafe fn(*mut cmdq_item)>",
        "Option<Box<dyn FnOnce(std::ptr::NonNull<cmdq_item>)>>",
        "*mut *mut cmdq_item",
        "Option<(*mut cmdq_item, bool)>",
    ] {
        let mut raw = RawItem::default();
        raw.visit_type(&syn::parse_str(ty).unwrap());
        assert!(raw.0, "missed {ty}");
    }
}
