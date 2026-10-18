//! The global option tables of this thread. Reached through `OptionsScope`.
use super::types::options;
use std::cell::RefCell;
use std::collections::BTreeMap;

thread_local! {
    static GLOBAL_OPTIONS: RefCell<options> = const {
        RefCell::new(options { tree: BTreeMap::new(), parent: None })
    };
    static GLOBAL_S_OPTIONS: RefCell<options> = const {
        RefCell::new(options { tree: BTreeMap::new(), parent: None })
    };
    static GLOBAL_W_OPTIONS: RefCell<options> = const {
        RefCell::new(options { tree: BTreeMap::new(), parent: None })
    };
}

/// Run `f` on the global server options of this thread. Borrowing the table
/// again from inside `f` panics.
pub(super) fn with_global_options<R>(f: impl FnOnce(&mut options) -> R) -> R {
    GLOBAL_OPTIONS.with(|table| f(&mut table.borrow_mut()))
}
/// Global session options of this thread. Same contract as `with_global_options`.
pub(super) fn with_global_s_options<R>(f: impl FnOnce(&mut options) -> R) -> R {
    GLOBAL_S_OPTIONS.with(|table| f(&mut table.borrow_mut()))
}
/// Global window options of this thread. Same contract as `with_global_options`.
pub(super) fn with_global_w_options<R>(f: impl FnOnce(&mut options) -> R) -> R {
    GLOBAL_W_OPTIONS.with(|table| f(&mut table.borrow_mut()))
}
