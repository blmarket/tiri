//! Function assignments check argument types, return types, and the C ABI.
use std::ffi::{c_int, c_void};
use tiri::shared::{abi::__compar_fn_t, prompt::*};

unsafe extern "C" fn compare(left: *const c_void, right: *const c_void) -> c_int {
    assert!(left.is_null());
    assert!(right.is_null());
    -1
}

/// Invokes each callback through its declared `Option` slot type.
fn invoke_assigned(free: prompt_free_cb, callback: prompt_input_cb, comparator: __compar_fn_t) {
    unsafe {
        free.expect("free callback assigned")();
        assert_eq!(
            callback.expect("input callback assigned")(None, PROMPT_KEY_CLOSE),
            PROMPT_CLOSE
        );
        assert_eq!(
            comparator.expect("comparator assigned")(std::ptr::null(), std::ptr::null()),
            -1
        );
    }
}

#[test]
fn local_prompt_callbacks_are_owned_and_comparators_keep_the_c_abi() {
    let free: prompt_free_cb = Some(Box::new(|| {}));
    let callback: prompt_input_cb = Some(Box::new(|text, key| {
        assert!(text.is_none());
        assert_eq!(key, PROMPT_KEY_CLOSE);
        PROMPT_CLOSE
    }));
    let comparator: __compar_fn_t = Some(compare);
    invoke_assigned(free, callback, comparator);
    let free: prompt_free_cb = None;
    let input: prompt_input_cb = None;
    let comparator: __compar_fn_t = None;
    assert!(free.is_none() && input.is_none() && comparator.is_none());
}
