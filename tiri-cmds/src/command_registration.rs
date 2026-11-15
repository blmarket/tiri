use crate::cmd::{cmd_find, cmd_table};

#[test]
fn scripting_catalog_resolves_command_names_and_aliases() {
    assert_eq!(cmd_table().len(), 89);
    for entry in cmd_table() {
        assert!(std::ptr::eq(cmd_find(entry.name).unwrap(), *entry));
        if let Some(alias) = entry.alias {
            assert!(std::ptr::eq(cmd_find(alias).unwrap(), *entry));
        }
    }
    assert!(std::ptr::eq(
        cmd_find(c"display").unwrap(),
        &crate::entries::display_message::cmd_display_message_entry,
    ));
    assert!(
        cmd_find(c"list-")
            .err()
            .unwrap()
            .as_bytes()
            .starts_with(b"ambiguous command:")
    );
    assert_eq!(
        cmd_find(c"unknown-registration-command")
            .err()
            .unwrap()
            .as_c_str(),
        c"unknown command: unknown-registration-command",
    );
}
