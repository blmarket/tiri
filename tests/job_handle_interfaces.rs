//! Jobs have one RefBox owner; raw job observers must not return in APIs or storage.
use std::path::Path;
use syn::visit::{self, Visit};

#[derive(Default)]
struct ContainsJob(bool);
impl<'ast> Visit<'ast> for ContainsJob {
    fn visit_type_path(&mut self, ty: &'ast syn::TypePath) {
        self.0 |= ty
            .path
            .segments
            .last()
            .is_some_and(|part| part.ident == "job");
        visit::visit_type_path(self, ty);
    }
}

#[derive(Default)]
struct RawJob(bool);
impl<'ast> Visit<'ast> for RawJob {
    fn visit_type_ptr(&mut self, ty: &'ast syn::TypePtr) {
        let mut job = ContainsJob::default();
        job.visit_type(&ty.elem);
        self.0 |= job.0;
        visit::visit_type_ptr(self, ty);
    }
    fn visit_type_path(&mut self, ty: &'ast syn::TypePath) {
        if ty
            .path
            .segments
            .last()
            .is_some_and(|part| part.ident == "NonNull")
        {
            let mut job = ContainsJob::default();
            job.visit_type_path(ty);
            self.0 |= job.0;
        }
        visit::visit_type_path(self, ty);
    }
}

fn inspect(path: &Path) {
    if path.is_dir() {
        for entry in std::fs::read_dir(path).unwrap() {
            inspect(&entry.unwrap().path());
        }
    } else if path.extension().is_some_and(|ext| ext == "rs") {
        let source = std::fs::read_to_string(path).unwrap();
        let mut raw = RawJob::default();
        raw.visit_file(&syn::parse_file(&source).unwrap());
        assert!(!raw.0, "raw job observer in {}", path.display());
    }
}

#[test]
fn jobs_have_no_raw_pointer_storage_arguments_returns_or_casts() {
    for directory in ["src", "tiri-core/src", "tiri-cmds/src"] {
        inspect(&Path::new(env!("CARGO_MANIFEST_DIR")).join(directory));
    }
}

#[test]
fn audit_recognizes_nested_job_pointers() {
    for ty in [
        "*mut job",
        "*const job",
        "Option<fn(*mut job)>",
        "std::ptr::NonNull<job>",
    ] {
        let mut raw = RawJob::default();
        raw.visit_type(&syn::parse_str(ty).unwrap());
        assert!(raw.0, "missed {ty}");
    }
}
