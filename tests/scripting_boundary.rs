use std::path::Path;
use syn::visit::{self, Visit};

struct CoreTypes<'a>(&'a Path);

impl CoreTypes<'_> {
    fn check(&self, name: &str) {
        assert!(
            ![
                "cmd",
                "cmd_list",
                "cmd_entry",
                "cmdq_item",
                "cmdq_list",
                "cmdq_state",
                "cmd_parse_input",
            ]
            .contains(&name),
            "{} contains an interpreter type: {name}",
            self.0.display(),
        );
    }
}

impl<'ast> Visit<'ast> for CoreTypes<'_> {
    fn visit_type_path(&mut self, ty: &'ast syn::TypePath) {
        for segment in &ty.path.segments {
            self.check(&segment.ident.to_string());
        }
        visit::visit_type_path(self, ty);
    }

    fn visit_item_struct(&mut self, item: &'ast syn::ItemStruct) {
        self.check(&item.ident.to_string());
        visit::visit_item_struct(self, item);
    }

    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        let name = item.sig.ident.to_string();
        assert!(
            !name.starts_with("cmdq_") && !name.starts_with("cmd_parse_"),
            "{} contains interpreter execution: {name}",
            self.0.display(),
        );
        visit::visit_item_fn(self, item);
    }
}

fn inspect_core(directory: &Path) {
    for entry in std::fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            inspect_core(&path);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            let source = std::fs::read_to_string(&path).unwrap();
            CoreTypes(&path).visit_file(&syn::parse_file(&source).unwrap());
        }
    }
}

#[test]
fn core_interfaces_do_not_use_concrete_interpreter_types() {
    inspect_core(&Path::new(env!("CARGO_MANIFEST_DIR")).join("tiri-core/src"));
}

#[test]
fn core_has_no_dependency_on_the_interpreter_or_its_grammar() {
    let manifest = include_str!("../tiri-core/Cargo.toml");
    for dependency in ["tiri", "tiri-cmds", "tiri-cmdparse"] {
        assert!(
            !manifest
                .lines()
                .any(|line| line.trim_start().starts_with(&format!("{dependency} ="))),
            "core depends on {dependency}",
        );
    }
}
