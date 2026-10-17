//! Prompt history owns strings and supplies independent snapshots to readers.

use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use tiri::src::format::bytes::write_cstr;
use tiri::src::options::{options_create, options_default, options_set_number, options_set_string};
use tiri::src::options_table::options_table;
use tiri::src::prompt_history::{
    prompt_add_history, prompt_down_history, prompt_history_clear, prompt_history_get,
    prompt_history_size, prompt_load_history, prompt_save_history, prompt_up_history,
};
use tiri::src::shared::prompt::{PROMPT_TYPE_COMMAND, PROMPT_TYPE_SEARCH};
use tiri::src::tmux::global_options;

#[test]
fn history_owns_entries_and_preserves_order_pruning_and_navigation() {
    unsafe {
        let previous_options = global_options;
        let mut options_owner = options_create(None);
        let options = &raw mut *options_owner;
        for name in [c"prompt-history-limit", c"history-file"] {
            let definition = options_table
                .iter()
                .find(|entry| entry.name == Some(name))
                .unwrap();
            options_default(options, definition);
        }
        global_options = options;

        prompt_history_clear(PROMPT_TYPE_COMMAND);
        prompt_history_clear(PROMPT_TYPE_SEARCH);
        assert_eq!(prompt_history_size(PROMPT_TYPE_COMMAND), 0);
        assert!(prompt_history_get(PROMPT_TYPE_COMMAND, 0).is_none());
        assert!(prompt_history_get(255, 0).is_none());

        options_set_number(options, c"prompt-history-limit", 200);
        prompt_add_history(c"\xff-first", PROMPT_TYPE_COMMAND);
        let first = prompt_history_get(PROMPT_TYPE_COMMAND, 0).unwrap();
        for n in 0..128 {
            let text = CString::new(format!("entry-{n}")).unwrap();
            prompt_add_history(&text, PROMPT_TYPE_COMMAND);
        }
        assert_eq!(&prompt_history_get(PROMPT_TYPE_COMMAND, 0).unwrap(), &first);
        assert_eq!(first.as_c_str().to_bytes(), b"\xff-first");
        assert_eq!(prompt_history_size(PROMPT_TYPE_COMMAND), 129);
        prompt_add_history(c"entry-127", PROMPT_TYPE_COMMAND);
        assert_eq!(prompt_history_size(PROMPT_TYPE_COMMAND), 129);

        prompt_history_clear(PROMPT_TYPE_COMMAND);
        assert_eq!(first.to_bytes(), b"\xff-first");
        drop(first);
        options_set_number(options, c"prompt-history-limit", 3);
        for text in [c"one", c"two", c"three"] {
            prompt_add_history(text, PROMPT_TYPE_COMMAND);
        }
        let middle = prompt_history_get(PROMPT_TYPE_COMMAND, 1).unwrap();
        prompt_add_history(c"four", PROMPT_TYPE_COMMAND);
        assert_eq!(prompt_history_size(PROMPT_TYPE_COMMAND), 3);
        assert_eq!(
            &prompt_history_get(PROMPT_TYPE_COMMAND, 0).unwrap(),
            &middle
        );
        assert_eq!(middle.as_c_str(), c"two");
        assert_eq!(
            prompt_history_get(PROMPT_TYPE_COMMAND, 2)
                .unwrap()
                .as_c_str(),
            c"four"
        );

        options_set_number(options, c"prompt-history-limit", 2);
        prompt_add_history(c"four", PROMPT_TYPE_COMMAND);
        assert_eq!(prompt_history_size(PROMPT_TYPE_COMMAND), 2);
        assert_eq!(
            prompt_history_get(PROMPT_TYPE_COMMAND, 0)
                .unwrap()
                .as_c_str(),
            c"three"
        );

        let mut indexes = [0_u32; 2];
        assert_eq!(
            prompt_up_history(&mut indexes, PROMPT_TYPE_COMMAND)
                .unwrap()
                .as_c_str(),
            c"four"
        );
        assert_eq!(
            prompt_up_history(&mut indexes, PROMPT_TYPE_COMMAND)
                .unwrap()
                .as_c_str(),
            c"three"
        );
        assert!(prompt_up_history(&mut indexes, PROMPT_TYPE_COMMAND).is_none());
        assert_eq!(
            prompt_down_history(&mut indexes, PROMPT_TYPE_COMMAND)
                .as_deref()
                .unwrap_or(c""),
            c"four"
        );
        assert_eq!(
            prompt_down_history(&mut indexes, PROMPT_TYPE_COMMAND)
                .as_deref()
                .unwrap_or(c""),
            c""
        );

        prompt_add_history(c"search-one", PROMPT_TYPE_SEARCH);
        prompt_add_history(c"search-two", PROMPT_TYPE_SEARCH);
        assert_eq!(
            prompt_up_history(&mut indexes, PROMPT_TYPE_SEARCH).as_deref(),
            Some(c"search-two")
        );
        assert_eq!(
            prompt_up_history(&mut indexes, PROMPT_TYPE_COMMAND).as_deref(),
            Some(c"four")
        );
        assert_eq!(indexes, [1, 1]);
        assert!(prompt_down_history(&mut indexes, PROMPT_TYPE_SEARCH).is_none());
        assert_eq!(indexes, [1, 0]);
        assert!(prompt_up_history(&mut indexes, 255).is_none());
        assert!(prompt_down_history(&mut indexes, 255).is_none());
        assert_eq!(indexes, [1, 0]);
        prompt_history_clear(PROMPT_TYPE_SEARCH);

        let oldest = prompt_history_get(PROMPT_TYPE_COMMAND, 0).unwrap();
        prompt_add_history(&oldest, PROMPT_TYPE_COMMAND);
        assert_eq!(oldest.as_c_str(), c"three");
        assert_eq!(
            prompt_history_get(PROMPT_TYPE_COMMAND, 0)
                .unwrap()
                .as_c_str(),
            c"four"
        );
        assert_eq!(
            prompt_history_get(PROMPT_TYPE_COMMAND, 1)
                .unwrap()
                .as_c_str(),
            c"three"
        );

        let path = std::env::temp_dir().join(format!("tiri-prompt-history-{}", std::process::id()));
        let path_string = CString::new(path.as_os_str().as_bytes()).unwrap();
        options_set_string(options, c"history-file", 0, |out| {
            write_cstr(out, &*path_string)
        });
        prompt_save_history();
        assert_eq!(
            std::fs::read(&path).unwrap(),
            b"command:four\ncommand:three\n"
        );
        prompt_history_clear(PROMPT_TYPE_COMMAND);
        prompt_load_history();
        assert_eq!(prompt_history_size(PROMPT_TYPE_COMMAND), 2);
        assert_eq!(
            prompt_history_get(PROMPT_TYPE_COMMAND, 0)
                .unwrap()
                .as_c_str(),
            c"four"
        );
        assert_eq!(
            prompt_history_get(PROMPT_TYPE_COMMAND, 1)
                .unwrap()
                .as_c_str(),
            c"three"
        );
        prompt_history_clear(PROMPT_TYPE_COMMAND);
        std::fs::write(&path, b"command:one\0ignored\ncommand:two").unwrap();
        prompt_load_history();
        assert_eq!(prompt_history_size(PROMPT_TYPE_COMMAND), 2);
        assert_eq!(
            prompt_history_get(PROMPT_TYPE_COMMAND, 0)
                .unwrap()
                .as_c_str(),
            c"one"
        );
        assert_eq!(
            prompt_history_get(PROMPT_TYPE_COMMAND, 1)
                .unwrap()
                .as_c_str(),
            c"two"
        );

        prompt_history_clear(PROMPT_TYPE_COMMAND);
        prompt_history_clear(PROMPT_TYPE_SEARCH);
        options_set_number(options, c"prompt-history-limit", 10);
        std::fs::write(
            &path,
            b"plain\nunknown:value\n:empty-prefix\ncommand:\nsearch:a:b\ncommand:first:second\n",
        )
        .unwrap();
        prompt_load_history();
        let command_history = [
            b"plain".as_slice(),
            b"unknown:value".as_slice(),
            b":empty-prefix".as_slice(),
            b"".as_slice(),
            b"first:second".as_slice(),
        ];
        assert_eq!(
            prompt_history_size(PROMPT_TYPE_COMMAND),
            command_history.len() as u32
        );
        for (index, expected) in command_history.into_iter().enumerate() {
            assert_eq!(
                prompt_history_get(PROMPT_TYPE_COMMAND, index as u32)
                    .unwrap()
                    .as_c_str()
                    .to_bytes(),
                expected
            );
        }
        assert_eq!(prompt_history_size(PROMPT_TYPE_SEARCH), 1);
        assert_eq!(
            prompt_history_get(PROMPT_TYPE_SEARCH, 0)
                .unwrap()
                .as_c_str()
                .to_bytes(),
            b"a:b"
        );
        let mut stale = [u32::MAX, 0];
        assert!(prompt_up_history(&mut stale, PROMPT_TYPE_COMMAND).is_none());
        assert_eq!(stale, [u32::MAX, 0]);
        assert!(prompt_down_history(&mut stale, PROMPT_TYPE_COMMAND).is_none());
        assert_eq!(stale, [u32::MAX - 1, 0]);
        std::fs::remove_file(path).unwrap();

        options_set_number(options, c"prompt-history-limit", 0);
        prompt_add_history(c"ignored", PROMPT_TYPE_COMMAND);
        assert_eq!(prompt_history_size(PROMPT_TYPE_COMMAND), 0);
        prompt_add_history(c"search", PROMPT_TYPE_SEARCH);
        assert_eq!(prompt_history_size(PROMPT_TYPE_SEARCH), 0);

        prompt_history_clear(PROMPT_TYPE_COMMAND);
        prompt_history_clear(PROMPT_TYPE_SEARCH);
        global_options = previous_options;
        drop(options_owner);
    }
}
