//! Syntax checks complement the compiler-backed field inventory in tools/.
//! They deliberately do not claim to type-resolve arbitrary Rust expressions.
use std::collections::HashSet;
use std::path::Path;
use syn::{
    visit::{self, Visit},
    GenericArgument, Item, Type,
};

fn model_name(name: &str) -> bool {
    matches!(name, "session" | "window" | "window_pane" | "client")
}

fn direct_model(ty: &Type, aliases: &HashSet<String>) -> bool {
    match ty {
        Type::Path(path) => path.path.segments.last().is_some_and(|s| {
            model_name(&s.ident.to_string()) || aliases.contains(&s.ident.to_string())
        }),
        Type::Paren(ty) => direct_model(&ty.elem, aliases),
        Type::Group(ty) => direct_model(&ty.elem, aliases),
        _ => false,
    }
}

fn model_cell(ty: &Type, aliases: &HashSet<String>) -> bool {
    let Type::Path(path) = ty else { return false };
    let Some(last) = path.path.segments.last() else {
        return false;
    };
    if last.ident != "UnsafeCell" {
        return false;
    }
    let syn::PathArguments::AngleBracketed(args) = &last.arguments else {
        return false;
    };
    args.args
        .iter()
        .any(|arg| matches!(arg, GenericArgument::Type(ty) if direct_model(ty, aliases)))
}

#[derive(Default)]
struct Audit {
    aliases: HashSet<String>,
    consumer: bool,
    findings: Vec<String>,
}

impl<'ast> Visit<'ast> for Audit {
    fn visit_item(&mut self, item: &'ast Item) {
        if let Item::Mod(module) = item {
            if module.attrs.iter().any(|attr| {
                attr.path().is_ident("cfg")
                    && attr
                        .parse_args::<syn::Path>()
                        .is_ok_and(|p| p.is_ident("test"))
            }) && self.consumer
            {
                return;
            }
        }
        visit::visit_item(self, item);
    }
    fn visit_use_rename(&mut self, rename: &'ast syn::UseRename) {
        if model_name(&rename.ident.to_string()) {
            self.aliases.insert(rename.rename.to_string());
        }
        visit::visit_use_rename(self, rename);
    }
    fn visit_item_type(&mut self, alias: &'ast syn::ItemType) {
        if direct_model(&alias.ty, &self.aliases) {
            self.aliases.insert(alias.ident.to_string());
        }
        visit::visit_item_type(self, alias);
    }
    fn visit_type_reference(&mut self, ty: &'ast syn::TypeReference) {
        if self.consumer && direct_model(&ty.elem, &self.aliases) {
            self.findings
                .push("borrowed model passed around the trait".into());
        }
        visit::visit_type_reference(self, ty);
    }
    fn visit_type_ptr(&mut self, ty: &'ast syn::TypePtr) {
        if self.consumer && direct_model(&ty.elem, &self.aliases) {
            self.findings.push("raw model pointer".into());
        }
        visit::visit_type_ptr(self, ty);
    }
    fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
        let payload_map = matches!(&*call.receiver, syn::Expr::Field(field)
            if matches!(&*field.base, syn::Expr::Path(base) if base.path.is_ident("ep"))
                && matches!(&field.member, syn::Member::Named(name) if name == "items"));
        if self.consumer && call.method == "get" && call.args.is_empty() && !payload_map {
            self.findings
                .push("storage get() in migrated consumer".into());
        }
        if call.method == "cast" {
            if let Some(args) = &call.turbofish {
                for arg in &args.args {
                    if let GenericArgument::Type(ty) = arg {
                        if model_cell(ty, &self.aliases) || direct_model(ty, &self.aliases) {
                            self.findings.push("model representation cast".into());
                        }
                    }
                }
            }
        }
        visit::visit_expr_method_call(self, call);
    }
    fn visit_expr_cast(&mut self, cast: &'ast syn::ExprCast) {
        if let Type::Ptr(ty) = &*cast.ty {
            struct PointerSource(bool);
            impl<'ast> Visit<'ast> for PointerSource {
                fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
                    self.0 |= call.method == "as_ptr";
                    visit::visit_expr_method_call(self, call);
                }
                fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
                    if let syn::Expr::Path(path) = &*call.func {
                        self.0 |= path
                            .path
                            .segments
                            .last()
                            .is_some_and(|s| s.ident == "as_ptr");
                    }
                    visit::visit_expr_call(self, call);
                }
            }
            let mut source = PointerSource(false);
            source.visit_expr(&cast.expr);
            if model_cell(&ty.elem, &self.aliases)
                || (direct_model(&ty.elem, &self.aliases) && source.0)
            {
                self.findings.push("model representation cast".into());
            }
        }
        visit::visit_expr_cast(self, cast);
    }
    fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
        if self.consumer {
            if let syn::Expr::Path(path) = &*call.func {
                if path
                    .path
                    .segments
                    .last()
                    .is_some_and(|s| s.ident == "as_ptr")
                {
                    self.findings
                        .push("storage pointer helper in migrated consumer".into());
                }
            }
        }
        visit::visit_expr_call(self, call);
    }
}

fn audit(source: &str, consumer: bool) -> Vec<String> {
    let syntax = syn::parse_file(source).expect("Rust source");
    let mut aliases = Audit::default();
    aliases.visit_file(&syntax);
    let mut audit = Audit {
        aliases: aliases.aliases,
        consumer,
        ..Default::default()
    };
    audit.visit_file(&syntax);
    audit.findings
}

#[test]
fn rejects_projection_references_pointer_helpers_and_representation_casts() {
    for source in [
        "fn f(s: &Rc<UnsafeCell<session>>) { let s = s.get(); unsafe { use_it(&*s); } }",
        "fn f(s: &session) { helper(s); }",
        "fn f(s: *mut window) { unsafe { (*s).id = 1; } }",
        "fn f(s: &Rc<UnsafeCell<client>>) { let p = rc::as_ptr(s); }",
        "fn f(window: &WindowRef) { let model = window.get(); }",
        "fn f(window: &WindowWeak) { let model = window.upgrade().unwrap().get(); }",
        "fn f(window: &WindowRef) { let model = rc::as_ptr(window); }",
        "fn f(session: &SessionRef) { let model = session.get(); }",
        "fn f(session: &SessionWeak) { let model = session.upgrade().unwrap().get(); }",
        "fn f(p: *mut window_pane) { let _ = p.cast::<UnsafeCell<window_pane>>(); }",
        "fn f(p: *mut u8) { let _ = p.cast::<Record>(); } use x::client as Record;",
        "fn f(p: *mut u8) { let _ = p as *mut UnsafeCell<session>; }",
    ] {
        assert!(!audit(source, true).is_empty(), "missed {source}");
    }
    for source in [
        "fn f(s: &Rc<UnsafeCell<session>>) { let _ = Rc::as_ptr(s) as *mut session; }",
        "fn f(s: &Weak<UnsafeCell<window>>) { let _ = s.as_ptr() as usize as *const window; }",
        "type Record = client; fn f(p: *mut u8) { let _ = p.cast::<Record>(); }",
        "fn f(window: &WindowRef) { let _ = Rc::as_ptr(window) as *mut window; }",
        "fn f(window: &WindowWeak) { let _ = window.as_ptr() as usize as *const window; }",
    ] {
        assert!(
            !audit(source, false).is_empty(),
            "missed representation cast: {source}"
        );
    }
    assert!(audit(
        "fn f(s: &Rc<UnsafeCell<session>>) { s.name(); s.with_winlinks(|links| lookup(links)); }",
        true
    )
    .is_empty());
}

#[test]
fn migrated_consumers_do_not_project_model_storage() {
    for path in [
        "src/alerts.rs",
        "src/control_notify.rs",
        "src/events_payload.rs",
        "src/names.rs",
        "src/resize.rs",
        "src/status.rs",
        "src/tty/acs.rs",
        "src/tty/features.rs",
        "src/tty/term.rs",
        "src/tty/output.rs",
        "src/tty/input.rs",
        "src/tty/draw.rs",
        "src/tty/keys.rs",
        "src/tty/control.rs",
        "src/screen_redraw.rs",
        "src/screen_write.rs",
        "src/shared/screen_write.rs",
        "src/window_border.rs",
        "src/window_visible.rs",
        "src/options/store.rs",
        "src/options/select.rs",
        "src/options/from_string.rs",
        "src/options/changes.rs",
        "src/format/jobs.rs",
    ] {
        let source =
            std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(path)).unwrap();
        assert_eq!(audit(&source, true), Vec::<String>::new(), "{path}");
    }
}

#[test]
fn format_callbacks_do_not_retain_cache_or_entry_pointers() {
    let source = std::fs::read_to_string("src/format/jobs.rs").unwrap();
    let syntax = syn::parse_file(&source).unwrap();
    struct CachePointers;
    impl<'ast> Visit<'ast> for CachePointers {
        fn visit_type_ptr(&mut self, ty: &'ast syn::TypePtr) {
            if let Type::Path(path) = &*ty.elem {
                assert!(
                    !path.path.segments.last().is_some_and(|segment| matches!(
                        segment.ident.to_string().as_str(),
                        "format_job" | "format_job_tree"
                    )),
                    "format callbacks must reacquire cache entries after Client borrows end"
                );
            }
            visit::visit_type_ptr(self, ty);
        }
    }
    CachePointers.visit_file(&syntax);
}

#[test]
fn screen_mode_snapshot_owns_only_copied_scalar_state() {
    let source = std::fs::read_to_string("src/shared/screen.rs").unwrap();
    let syntax = syn::parse_file(&source).unwrap();
    let snapshot = syntax
        .items
        .iter()
        .find_map(|item| match item {
            Item::Struct(item) if item.ident == "ScreenMode" => Some(item),
            _ => None,
        })
        .expect("owned screen mode snapshot");
    for field in &snapshot.fields {
        let Type::Path(path) = &field.ty else {
            panic!("borrowed screen mode field")
        };
        assert!(matches!(
            path.path
                .segments
                .last()
                .unwrap()
                .ident
                .to_string()
                .as_str(),
            "u_int" | "c_int" | "screen_cursor_style"
        ));
    }
}

#[test]
fn migrated_terminal_helpers_do_not_reach_through_client_storage() {
    let syntax = syn::parse_file(&std::fs::read_to_string("src/tty/control.rs").unwrap()).unwrap();
    for item in &syntax.items {
        let Item::Fn(function) = item else { continue };
        let mut check = Audit {
            consumer: true,
            ..Default::default()
        };
        check.visit_item_fn(function);
        assert!(
            check.findings.is_empty(),
            "{}: {:?}",
            function.sig.ident,
            check.findings
        );
    }
    for (path, expected) in [
        (
            "src/tty/control.rs",
            vec![
                "tty_init",
                "tty_initialize_component",
                "tty_window_bigger",
                "tty_window_offset1",
                "tty_update_client_offset",
                "tty_update_window_offset",
                "tty_set_selection",
                "tty_selection_sequence",
            ],
        ),
        (
            "src/tty/keys.rs",
            vec![
                "tty_keys_next1",
                "tty_keys_extended_key",
                "tty_keys_mouse",
                "tty_keys_colours",
                "tty_keys_update_focus",
                "tty_keys_clipboard",
                "tty_keys_palette",
            ],
        ),
        (
            "src/window_pane/input.rs",
            vec![
                "input_request_reply",
                "input_request_matches",
                "input_complete_request",
                "input_free_request",
            ],
        ),
    ] {
        let source =
            std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(path)).unwrap();
        let syntax = syn::parse_file(&source).unwrap();
        let mut checked = 0;
        for item in syntax.items {
            let Item::Fn(function) = item else { continue };
            if !expected.contains(&function.sig.ident.to_string().as_str()) {
                continue;
            }
            let mut check = Audit {
                consumer: true,
                ..Default::default()
            };
            check.visit_item_fn(&function);
            assert!(
                check.findings.is_empty(),
                "{path}::{}: {:?}",
                function.sig.ident,
                check.findings
            );
            checked += 1;
        }
        assert_eq!(
            checked,
            expected.len(),
            "{path}: migrated helpers must remain checked"
        );
    }
}

#[test]
fn migrated_name_and_window_notification_helpers_use_holders() {
    for (path, expected) in [
        (
            "src/window_border.rs",
            &["window_render_fill_cell", "window_get_fill_cell"][..],
        ),
        (
            "src/format/callbacks.rs",
            &[
                "format_cb_window_layout",
                "format_cb_pane_at_bottom",
                "format_cb_window_linked_sessions",
                "format_cb_window_name",
            ][..],
        ),
        (
            "src/format/expression.rs",
            &["format_add_window_neighbour"][..],
        ),
        ("src/window_pane/input.rs", &["input_exit_rename"][..]),
        (
            "src/window_tree.rs",
            &["window_tree_pull_item", "window_tree_border_cell"][..],
        ),
        (
            "src/window_panes.rs",
            &[
                "window_panes_pane_geometry",
                "window_panes_scaled_geometry",
                "window_panes_get_geometry",
                "window_panes_get_border_cell",
                "window_panes_draw_borders",
                "window_panes_draw_format",
            ][..],
        ),
        ("src/spawn.rs", &["initialize_spawned_window"][..]),
        (
            "src/cmd/entries/join_pane.rs",
            &["cmd_join_pane_finish"][..],
        ),
        ("src/session/mod.rs", &["session_is_linked"][..]),
        ("src/monitor.rs", &["monitor_check_window"][..]),
        (
            "src/cmd/entries/select_pane.rs",
            &["cmd_select_pane_redraw"][..],
        ),
        (
            "src/screen_redraw.rs",
            &[
                "redraw_set_context",
                "redraw_reset_cell",
                "redraw_build_cells",
                "redraw_get_default_border_style",
                "redraw_make_scene",
                "redraw_get_scene",
                "redraw_invalidate_all_scenes",
            ][..],
        ),
        (
            "src/server_fn.rs",
            &[
                "server_redraw_window",
                "server_redraw_window_borders",
                "server_status_window",
            ][..],
        ),
    ] {
        let source =
            std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(path)).unwrap();
        let syntax = syn::parse_file(&source).unwrap();
        let mut checked = 0;
        for item in &syntax.items {
            let Item::Fn(function) = item else { continue };
            if expected.contains(&function.sig.ident.to_string().as_str()) {
                let mut check = Audit {
                    consumer: true,
                    ..Default::default()
                };
                check.visit_item_fn(function);
                assert!(
                    check.findings.is_empty(),
                    "{path} {}: {:?}",
                    function.sig.ident,
                    check.findings
                );
                checked += 1;
            }
        }
        assert_eq!(
            checked,
            expected.len(),
            "{path}: migrated helpers must remain checked"
        );
    }
}

#[test]
fn notification_helpers_do_not_project_client_storage() {
    let source =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/control.rs"))
            .unwrap();
    let syntax = syn::parse_file(&source).unwrap();
    let expected = [
        "control_write_line",
        "control_flush_deferred",
        "control_write",
        "control_write_guard",
        "control_notify_write",
        "control_discard_pane_output",
    ];
    let mut checked = 0;
    for item in syntax.items {
        let Item::Fn(function) = item else { continue };
        if expected.contains(&function.sig.ident.to_string().as_str()) {
            let mut check = Audit {
                consumer: true,
                ..Default::default()
            };
            check.visit_item_fn(&function);
            assert!(
                check.findings.is_empty(),
                "{}: {:?}",
                function.sig.ident,
                check.findings
            );
            checked += 1;
        }
    }
    assert_eq!(
        checked,
        expected.len(),
        "migrated helpers must remain checked"
    );
}

#[test]
fn no_model_representation_casts_anywhere_in_application() {
    fn check(directory: &Path) {
        for entry in std::fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                check(&path);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                let source = std::fs::read_to_string(&path).unwrap();
                assert!(audit(&source, false).is_empty(), "{}", path.display());
            }
        }
    }
    check(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"));
}

#[test]
fn model_traits_do_not_return_raw_components_or_whole_models() {
    for path in [
        "src/session/api.rs",
        "src/window/api.rs",
        "src/window_pane/api.rs",
        "src/server_client/api.rs",
    ] {
        let source =
            std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(path)).unwrap();
        let syntax = syn::parse_file(&source).unwrap();
        for item in syntax.items {
            let Item::Trait(interface) = item else {
                continue;
            };
            for item in interface.items {
                let syn::TraitItem::Fn(method) = item else {
                    continue;
                };
                struct Output;
                impl<'ast> Visit<'ast> for Output {
                    fn visit_type_ptr(&mut self, _: &'ast syn::TypePtr) {
                        panic!("raw component in model trait result");
                    }
                    fn visit_type_reference(&mut self, ty: &'ast syn::TypeReference) {
                        assert!(
                            !direct_model(&ty.elem, &HashSet::new()),
                            "whole model borrow"
                        );
                        visit::visit_type_reference(self, ty);
                    }
                }
                Output.visit_return_type(&method.sig.output);
            }
        }
    }
}

#[test]
fn client_fields_remain_private_to_the_implementation_module() {
    let source = std::fs::read_to_string("src/server_client/model.rs").unwrap();
    let syntax = syn::parse_file(&source).unwrap();
    let model = syntax
        .items
        .iter()
        .find_map(|item| match item {
            Item::Struct(model) if model.ident == "client" => Some(model),
            _ => None,
        })
        .unwrap();
    for field in &model.fields {
        assert!(
            matches!(&field.vis, syn::Visibility::Restricted(vis)
            if vis.path.is_ident("super")),
            "Client field visibility widened: {:?}",
            field.ident
        );
    }
    let syntax =
        syn::parse_file(&std::fs::read_to_string("src/server_client/mod.rs").unwrap()).unwrap();
    struct ClientStorage;
    impl<'ast> Visit<'ast> for ClientStorage {
        fn visit_type_reference(&mut self, ty: &'ast syn::TypeReference) {
            assert!(
                !matches!(&*ty.elem, Type::Path(path) if path.path.is_ident("client")),
                "exported helper must not accept or return a whole Client borrow"
            );
            visit::visit_type_reference(self, ty);
        }
        fn visit_type_ptr(&mut self, ty: &'ast syn::TypePtr) {
            assert!(
                !matches!(&*ty.elem, Type::Path(path) if path.path.is_ident("client")),
                "exported helper must not expose raw Client storage"
            );
            visit::visit_type_ptr(self, ty);
        }
    }
    for item in &syntax.items {
        let Item::Fn(function) = item else { continue };
        if !matches!(function.vis, syn::Visibility::Inherited) {
            ClientStorage.visit_signature(&function.sig);
        }
    }
}

#[test]
fn queued_items_do_not_retain_raw_queue_component_pointers() {
    let source = std::fs::read_to_string("src/cmd/queue.rs").unwrap();
    let syntax = syn::parse_file(&source).unwrap();
    struct QueuePointers;
    impl<'ast> Visit<'ast> for QueuePointers {
        fn visit_type_ptr(&mut self, ty: &'ast syn::TypePtr) {
            if let Type::Path(path) = &*ty.elem {
                assert!(
                    !path
                        .path
                        .segments
                        .last()
                        .is_some_and(|s| s.ident == "cmdq_list"),
                    "queue pointers must not outlive the Client component borrow"
                );
            }
            visit::visit_type_ptr(self, ty);
        }
    }
    QueuePointers.visit_file(&syntax);
    assert!(syntax.items.iter().any(|item| matches!(item,
        Item::Enum(target) if target.ident == "QueueTarget")));
}

#[test]
fn session_state_visibility_cannot_widen_silently() {
    let source = std::fs::read_to_string("src/session/model.rs").unwrap();
    let syntax = syn::parse_file(&source).unwrap();
    let model = syntax
        .items
        .iter()
        .find_map(|item| match item {
            Item::Struct(model) if model.ident == "session" => Some(model),
            _ => None,
        })
        .unwrap();
    for field in &model.fields {
        let name = field.ident.as_ref().unwrap();
        assert!(
            matches!(&field.vis, syn::Visibility::Restricted(vis)
            if vis.path.is_ident("super")),
            "Session::{name} visibility widened"
        );
    }
    let syntax = syn::parse_file(&std::fs::read_to_string("src/session/mod.rs").unwrap()).unwrap();
    for item in &syntax.items {
        let Item::Fn(function) = item else { continue };
        if matches!(function.vis, syn::Visibility::Inherited) {
            continue;
        }
        struct References;
        impl<'ast> Visit<'ast> for References {
            fn visit_type_reference(&mut self, ty: &'ast syn::TypeReference) {
                if let Type::Path(path) = &*ty.elem {
                    assert!(
                        !path.path.is_ident("session"),
                        "public helper bypasses Session"
                    );
                }
                visit::visit_type_reference(self, ty);
            }
            fn visit_type_ptr(&mut self, ty: &'ast syn::TypePtr) {
                if let Type::Path(path) = &*ty.elem {
                    assert!(
                        !path.path.is_ident("session"),
                        "public helper exposes Session pointer"
                    );
                }
                visit::visit_type_ptr(self, ty);
            }
        }
        References.visit_signature(&function.sig);
    }
}

#[test]
fn window_state_is_private_and_pane_implementation_is_a_sibling_module() {
    let source = std::fs::read_to_string("src/window/model.rs").unwrap();
    let syntax = syn::parse_file(&source).unwrap();
    let model = syntax
        .items
        .iter()
        .find_map(|item| match item {
            Item::Struct(model) if model.ident == "window" => Some(model),
            _ => None,
        })
        .unwrap();
    for field in &model.fields {
        assert!(
            matches!(&field.vis, syn::Visibility::Restricted(vis)
            if vis.path.is_ident("super")),
            "Window field visibility widened: {:?}",
            field.ident
        );
    }
    let syntax = syn::parse_file(&std::fs::read_to_string("src/window/mod.rs").unwrap()).unwrap();
    for item in &syntax.items {
        if let Item::Mod(module) = item {
            assert!(
                !module.ident.to_string().contains("pane"),
                "Pane implementation must not inherit access to Window's private state"
            );
        }
    }
    for item in &syntax.items {
        let Item::Fn(function) = item else { continue };
        let name = function.sig.ident.to_string();
        let order_edit = name.starts_with("window_pane_list_");
        let whole_model_query = matches!(
            name.as_str(),
            "windows_find"
                | "windows_next"
                | "window_winlinks_first"
                | "window_winlinks_next"
                | "window_has_pane"
                | "window_pane_first"
                | "window_pane_last"
                | "window_pane_stack_first"
                | "window_pane_stack_next"
                | "window_get_pane_lines"
        );
        if order_edit || whole_model_query || name == "window_replace_name" {
            assert!(
                matches!(function.vis, syn::Visibility::Inherited),
                "{name} must remain private; consumers use Window operations/component borrows"
            );
        }
        assert!(
            !matches!(
                name.as_str(),
                "window_pane_swap_order" | "window_pane_z_swap_order"
            ),
            "whole-model swap helpers must not be reintroduced"
        );
    }
    assert!(Path::new("src/window_pane/mod.rs").exists());
    assert!(Path::new("src/window_pane/api.rs").exists());
    assert!(Path::new("src/winlink.rs").exists());
}

fn rust_sources(directory: &Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files.extend(rust_sources(&path));
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            files.push(path);
        }
    }
    files
}

#[test]
fn four_model_fields_and_inherent_helpers_remain_owner_private() {
    for (model, path) in [
        ("session", "src/session/model.rs"),
        ("window", "src/window/model.rs"),
        ("window_pane", "src/window_pane/model.rs"),
        ("client", "src/server_client/model.rs"),
    ] {
        let syntax = syn::parse_file(&std::fs::read_to_string(path).unwrap()).unwrap();
        let owner_private = |visibility: &syn::Visibility| {
            matches!(visibility, syn::Visibility::Inherited)
                || matches!(visibility,
            syn::Visibility::Restricted(vis) if vis.path.is_ident("super"))
        };
        for item in &syntax.items {
            if let Item::Struct(record) = item {
                if record.ident == model {
                    for field in &record.fields {
                        assert!(
                            matches!(&field.vis, syn::Visibility::Restricted(vis)
                            if vis.path.is_ident("super")),
                            "{path}: {:?}",
                            field.ident
                        );
                    }
                }
            }
            if let Item::Impl(implementation) = item {
                if implementation.trait_.is_none()
                    && matches!(&*implementation.self_ty, Type::Path(ty)
                        if ty.path.is_ident(model))
                {
                    for item in &implementation.items {
                        if let syn::ImplItem::Fn(method) = item {
                            assert!(owner_private(&method.vis), "{path}: {}", method.sig.ident);
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn external_client_types_use_holders_without_exposing_model_storage() {
    struct ClientStorage<'a>(&'a Path);
    impl<'ast> Visit<'ast> for ClientStorage<'_> {
        fn visit_type_path(&mut self, ty: &'ast syn::TypePath) {
            assert!(
                !ty.path
                    .segments
                    .last()
                    .is_some_and(|part| part.ident == "client"),
                "{}: external types must use ClientRef/ClientWeak or component guards",
                self.0.display()
            );
            visit::visit_type_path(self, ty);
        }
    }
    for path in rust_sources(Path::new("src")) {
        if path.starts_with("src/server_client") || path == Path::new("src/shared/client.rs") {
            continue;
        }
        let syntax = syn::parse_file(&std::fs::read_to_string(&path).unwrap()).unwrap();
        ClientStorage(&path).visit_file(&syntax);
    }
}

#[test]
fn terminal_registry_results_do_not_retain_component_pointers() {
    struct Owned;
    impl<'ast> Visit<'ast> for Owned {
        fn visit_type_ptr(&mut self, _: &'ast syn::TypePtr) {
            panic!("terminal observation must reacquire components through its Client holder");
        }
        fn visit_type_reference(&mut self, _: &'ast syn::TypeReference) {
            panic!("terminal observation must own its result");
        }
        fn visit_type_path(&mut self, ty: &'ast syn::TypePath) {
            assert!(!ty.path.segments.iter().any(|part| part.ident == "NonNull"));
            visit::visit_type_path(self, ty);
        }
    }
    let syntax = syn::parse_file(&std::fs::read_to_string("src/tty/term.rs").unwrap()).unwrap();
    let mut checked = 0;
    for item in &syntax.items {
        if let Item::Struct(item) = item {
            if matches!(
                item.ident.to_string().as_str(),
                "TerminalRegistration" | "TerminalDescription"
            ) {
                Owned.visit_fields(&item.fields);
                checked += 1;
            }
        }
    }
    assert_eq!(checked, 2);
}

#[test]
fn option_inheritance_retains_owner_identity_without_component_pointers() {
    fn path_name(ty: &Type, expected: &str) -> bool {
        matches!(ty, Type::Path(path) if path.path.segments.last().is_some_and(|s| s.ident == expected))
    }
    fn only_argument<'a>(ty: &'a Type, expected: &str) -> Option<&'a Type> {
        let Type::Path(path) = ty else { return None };
        let last = path.path.segments.last()?;
        if last.ident != expected {
            return None;
        }
        let syn::PathArguments::AngleBracketed(arguments) = &last.arguments else {
            return None;
        };
        if arguments.args.len() != 1 {
            return None;
        }
        match arguments.args.first()? {
            GenericArgument::Type(inner) => Some(inner),
            _ => None,
        }
    }
    fn model_observer(ty: &Type, model: &str, alias: &str) -> bool {
        if path_name(ty, alias) {
            return true;
        }
        let Some(cell) = only_argument(ty, "Weak") else {
            return false;
        };
        ["UnsafeCell", "RefCell"].iter().any(|storage| {
            only_argument(cell, storage).is_some_and(|inner| path_name(inner, model))
        })
    }
    fn optional_scope(ty: &Type) -> bool {
        only_argument(ty, "Option").is_some_and(|inner| path_name(inner, "OptionsScope"))
    }

    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let scope =
        syn::parse_file(&std::fs::read_to_string(root.join("src/options/scope.rs")).unwrap())
            .unwrap();
    let identity = scope
        .items
        .iter()
        .find_map(|item| match item {
            Item::Enum(item) if item.ident == "OptionsScope" => Some(item),
            _ => None,
        })
        .expect("persistent option scope identity");
    assert_eq!(identity.variants.len(), 6);
    for variant in &identity.variants {
        let model = match variant.ident.to_string().as_str() {
            "GlobalServer" | "GlobalSession" | "GlobalWindow" => {
                assert!(matches!(variant.fields, syn::Fields::Unit));
                continue;
            }
            "Session" => ("session", "SessionWeak"),
            "Window" => ("window", "WindowWeak"),
            "Pane" => ("window_pane", "PaneWeak"),
            other => panic!("option scope must name a known owner: {other}"),
        };
        let syn::Fields::Unnamed(fields) = &variant.fields else {
            panic!("model option scope must carry one Weak owner");
        };
        assert_eq!(fields.unnamed.len(), 1);
        assert!(
            model_observer(&fields.unnamed[0].ty, model.0, model.1),
            "{} scope cannot store a component reference, pointer or strong owner",
            variant.ident,
        );
    }
    let mut audit = Audit {
        consumer: true,
        ..Default::default()
    };
    audit.visit_file(&scope);
    assert!(audit.findings.is_empty(), "{:?}", audit.findings);

    let shared =
        syn::parse_file(&std::fs::read_to_string(root.join("src/options/types.rs")).unwrap())
            .unwrap();
    let table = shared
        .items
        .iter()
        .find_map(|item| match item {
            Item::Struct(item) if item.ident == "options" => Some(item),
            _ => None,
        })
        .expect("option table");
    let parent = table
        .fields
        .iter()
        .find(|field| field.ident.as_ref().is_some_and(|name| name == "parent"))
        .expect("inheritance parent");
    assert!(
        optional_scope(&parent.ty),
        "option parent must retain only its scope identity"
    );

    let functions =
        syn::parse_file(&std::fs::read_to_string(root.join("src/options/store.rs")).unwrap())
            .unwrap();
    let constructors = ["options_create", "options_create_owned"];
    let mut checked = 0;
    for item in &functions.items {
        let Item::Fn(function) = item else { continue };
        assert_ne!(
            function.sig.ident, "options_get",
            "inherited entries cannot be returned as unguarded observers"
        );
        if !constructors.contains(&function.sig.ident.to_string().as_str()) {
            continue;
        }
        let parent = function.sig.inputs.iter().find_map(|argument| match argument {
            syn::FnArg::Typed(argument) if matches!(&*argument.pat, syn::Pat::Ident(name) if name.ident == "parent") => Some(&*argument.ty),
            _ => None,
        }).expect("typed inheritance argument");
        assert!(
            optional_scope(parent),
            "{} must accept an owner identity",
            function.sig.ident
        );
        checked += 1;
    }
    assert_eq!(checked, constructors.len());
}
