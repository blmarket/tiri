use super::*;
use crate::options::OptionsScope;

struct Fake {
    name: &'static str,
    variables: &'static [Variable],
}

impl Plugin for Fake {
    fn name(&self) -> &'static str {
        self.name
    }
    fn variables(&self) -> &'static [Variable] {
        self.variables
    }
    fn interval(&self) -> Duration {
        Duration::from_millis(200)
    }
    fn refresh(&mut self, _: &dyn Host, _: &mut PaneValues) -> io::Result<()> {
        Ok(())
    }
}

fn add(
    registry: &mut Registry,
    name: &'static str,
    variables: &'static [Variable],
) -> io::Result<()> {
    let mut prepared = Some(Slot::new(Box::new(Fake { name, variables }))?);
    registry.insert(&mut prepared)
}

const VARIABLES: &[Variable] = &[
    Variable::with_default(c"test_state", c"none"),
    Variable::new(c"test_model"),
];
const OTHER: &[Variable] = &[Variable::new(c"other")];
const CONFLICT: &[Variable] = &[Variable::new(c"new_name"), Variable::new(c"test_state")];
const BUILTIN: &[Variable] = &[Variable::new(c"pane_id")];
const OPTION: &[Variable] = &[Variable::new(c"prefix")];
const DUPLICATE: &[Variable] = &[Variable::new(c"repeated"), Variable::new(c"repeated")];
const INVALID: &[&[Variable]] = &[
    &[Variable::new(c"@user")],
    &[Variable::new(c"bad:name")],
    &[Variable::new(c"")],
    &[Variable::new(c"1name")],
];

#[test]
fn registration_owns_names_and_rejects_collisions_atomically() {
    let mut registry = Registry::default();
    add(&mut registry, "first", VARIABLES).unwrap();
    for (name, variables) in [
        ("first", OTHER),
        ("second", CONFLICT),
        ("builtin", BUILTIN),
        ("option", OPTION),
        ("duplicate", DUPLICATE),
    ] {
        assert_eq!(
            add(&mut registry, name, variables).unwrap_err().kind(),
            io::ErrorKind::AlreadyExists
        );
    }
    assert_eq!(registry.slots.len(), 1);
    assert_eq!(registry.names.len(), 2);
    assert_eq!(registry.find(Some(PaneId(1)), c"new_name"), None);
    for variables in INVALID {
        assert_eq!(
            add(&mut registry, "invalid", variables).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
    }
}

#[test]
fn snapshots_are_per_pane_and_remove_disappeared_values() {
    let mut registry = Registry::default();
    add(&mut registry, "fake", VARIABLES).unwrap();
    assert_eq!(
        registry.find(Some(PaneId(1)), c"test_state").as_deref(),
        Some(c"none")
    );
    assert_eq!(registry.find(None, c"test_state").as_deref(), Some(c""));
    assert_eq!(registry.find(None, c"unknown"), None);
    let mut values = PaneValues::new(VARIABLES);
    assert!(values
        .set(PaneId(1), c"git_branch", c"other plugin".into())
        .is_err());
    values
        .set(PaneId(1), c"test_state", c"working".into())
        .unwrap();
    values
        .set(PaneId(2), c"test_state", c"idle".into())
        .unwrap();
    assert_eq!(
        registry
            .publish(0, values)
            .iter()
            .map(|p| p.0)
            .collect::<Vec<_>>(),
        [1, 2]
    );
    assert_eq!(
        registry.find(Some(PaneId(1)), c"test_state").as_deref(),
        Some(c"working")
    );
    assert_eq!(
        registry.find(Some(PaneId(2)), c"test_state").as_deref(),
        Some(c"idle")
    );
    let mut values = PaneValues::new(VARIABLES);
    values
        .set(PaneId(1), c"test_state", c"working".into())
        .unwrap();
    assert_eq!(
        registry
            .publish(0, values)
            .iter()
            .map(|p| p.0)
            .collect::<Vec<_>>(),
        [2]
    );
    assert_eq!(
        registry.find(Some(PaneId(2)), c"test_state").as_deref(),
        Some(c"none")
    );
    let mut values = PaneValues::new(VARIABLES);
    values
        .set(PaneId(1), c"test_state", c"working".into())
        .unwrap();
    assert!(registry.publish(0, values).is_empty());
}

#[test]
fn selection_matches_tiri_environment_semantics() {
    assert_eq!(enabled_names(None), ["agent", "git"]);
    assert_eq!(
        enabled_names(Some(OsStr::new(" Agent , Git "))),
        ["agent", "git"]
    );
    assert_eq!(enabled_names(Some(OsStr::new("all"))), ["all"]);
    assert!(enabled_names(Some(OsStr::new(""))).is_empty());
    assert!(enabled_names(Some(OsStr::new("none"))).is_empty());
    assert!(enabled_names(Some(OsStr::new("git,none"))).is_empty());
}

#[test]
fn publishing_does_not_override_context_entries_or_user_options() {
    use crate::format::{format_add_cstr, format_create, format_expand_cstring};
    use crate::options::options_set_string;
    {
        shutdown();
        OptionsScope::GlobalSession.with_local(|oo| {
            options_set_string(oo, c"@test_state", 0, |out| out.write_all(b"user"));
        });
        REGISTRY.with(|registry| add(&mut registry.borrow_mut(), "fake", VARIABLES).unwrap());
        let mut tree = format_create(None, None, 0, 0);
        format_add_cstr(&mut tree, c"test_state", c"context");
        let expanded = format_expand_cstring(&mut tree, c"#{test_state}:#{@test_state}");
        assert_eq!(expanded, c"context:user");
        let mut enumerated = Vec::new();
        for (key, value) in each(None) {
            enumerated.push((key, value));
        }
        assert_eq!(enumerated.len(), 2);
        drop(tree);
        shutdown();
    }
}

#[test]
fn failed_registration_drops_plugin_outside_registry_borrows() {
    struct Reenter;
    impl Drop for Reenter {
        fn drop(&mut self) {
            let _ = find(None, c"test_state");
        }
    }
    impl Plugin for Reenter {
        fn name(&self) -> &'static str {
            "conflict"
        }
        fn variables(&self) -> &'static [Variable] {
            BUILTIN
        }
        fn interval(&self) -> Duration {
            Duration::from_secs(1)
        }
        fn refresh(&mut self, _: &dyn Host, _: &mut PaneValues) -> io::Result<()> {
            Ok(())
        }
    }
    assert!(register(Box::new(Reenter)).is_err());
}

#[test]
fn refresh_reads_previous_snapshot_and_errors_do_not_publish_partial_values() {
    use std::cell::Cell;
    use std::rc::Rc;
    use tiri_rt::Runtime as _;

    struct Refresh {
        calls: Rc<Cell<usize>>,
        dropped: Rc<Cell<bool>>,
    }
    impl Drop for Refresh {
        fn drop(&mut self) {
            assert_eq!(find(None, c"test_state"), None);
            self.dropped.set(true);
        }
    }
    impl Plugin for Refresh {
        fn name(&self) -> &'static str {
            "refresh"
        }
        fn variables(&self) -> &'static [Variable] {
            VARIABLES
        }
        fn interval(&self) -> Duration {
            Duration::from_secs(60)
        }
        fn refresh(&mut self, _: &dyn Host, values: &mut PaneValues) -> io::Result<()> {
            let call = self.calls.get();
            self.calls.set(call + 1);
            assert_eq!(
                find(Some(PaneId(99)), c"test_state").as_deref(),
                Some(if call == 0 { c"none" } else { c"committed" })
            );
            values.set(
                PaneId(99),
                c"test_state",
                if call == 0 { c"committed" } else { c"partial" }.into(),
            )?;
            if call == 0 {
                Ok(())
            } else {
                Err(io::Error::other("refresh failed"))
            }
        }
    }
    shutdown();
    let runtime = tiri_rt::mio::Runtime::new().unwrap();
    let calls = Rc::new(Cell::new(0));
    let dropped = Rc::new(Cell::new(false));
    register(Box::new(Refresh {
        calls: calls.clone(),
        dropped: dropped.clone(),
    }))
    .unwrap();
    tick();
    assert_eq!(
        find(Some(PaneId(99)), c"test_state").as_deref(),
        Some(c"committed")
    );
    REGISTRY.with(|registry| registry.borrow_mut().slots[0].due = Instant::now());
    tick();
    assert_eq!(calls.get(), 2);
    assert_eq!(
        find(Some(PaneId(99)), c"test_state").as_deref(),
        Some(c"committed")
    );
    shutdown();
    assert!(dropped.get());
    REGISTRY.with(|registry| assert!(registry.borrow().timer.is_none()));
    crate::reactor::shutdown_runtime(runtime);
    assert_eq!(calls.get(), 2);
}
