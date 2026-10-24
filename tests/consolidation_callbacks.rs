//! Function assignments check argument types and return types.
use tiri::shared::prompt::*;

/// Invokes each callback through its declared `Option` slot type.
fn invoke_assigned(free: prompt_free_cb, callback: prompt_input_cb) {
    free.expect("free callback assigned")();
    assert_eq!(
        callback.expect("input callback assigned")(None, PROMPT_KEY_CLOSE),
        PROMPT_CLOSE
    );
}

#[test]
fn local_prompt_callbacks_are_owned() {
    let free: prompt_free_cb = Some(Box::new(|| {}));
    let callback: prompt_input_cb = Some(Box::new(|text, key| {
        assert!(text.is_none());
        assert_eq!(key, PROMPT_KEY_CLOSE);
        PROMPT_CLOSE
    }));
    invoke_assigned(free, callback);
    let free: prompt_free_cb = None;
    let input: prompt_input_cb = None;
    assert!(free.is_none() && input.is_none());
}
