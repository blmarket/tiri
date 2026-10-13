# tiri-refbox

Local fork of [refbox 0.4.1](https://github.com/yvesdum/refbox), upstream commit
`eb6c83f147a6328b72b1f01cb5ffab2658a76d92`. Original source and unit tests are
retained under MIT OR Apache-2.0; see LICENSE-MIT and LICENSE-APACHE.

Use it in place of refbox without changing imports:

```toml
refbox = { package = "tiri-refbox", path = "tiri-refbox" }
```

```rust
use refbox::{RefBox, Weak};

let mut observer: Weak<String> = Weak::new();
assert!(!observer.is_alive());
let owner = RefBox::new(String::from("hello"));
observer = owner.downgrade();
assert_eq!(&*observer.try_borrow_mut().unwrap(), "hello");
observer = Weak::default();
assert!(observer.is_empty());
```

`Weak::new()` is const and allocation-free; `Default` has no `T: Default` bound.
Empty handles have zero weak count, are neither alive nor borrowed, and borrowing
returns `BorrowError::Dropped`. Cloning and dropping them require no allocation
or reference counting. Two empty handles compare equal. `is_empty()` distinguishes
an absent observer from an expired observer, which still retains its identity.
Existing nonempty behavior and the `cyclic` / `cyclic_stable` features are preserved.

The representation follows [std::rc::Weak](https://doc.rust-lang.org/src/alloc/rc.rs.html):
a `NonNull` pointer with address `usize::MAX` reserved for emptiness. The control
block's u32 counter guarantees alignment of at least four, so this address cannot
be an allocated control block. Sized handles remain one pointer wide, including
`Option<Weak<T>>`. Empty pointers must be checked before any dereference or field
projection, including when coerced to trait objects or slices.

As with std, empty `as_ptr()` returns a dangling sentinel, **not null**. Use
`is_empty()` or the checked borrowing API. `into_raw()` / `from_raw()` preserve
empty handles; unlike std, these existing refbox APIs use control-block pointers,
not payload pointers. Never pass null to `from_raw()`.

Validation:

```sh
cargo test -p tiri-refbox
cargo test -p tiri-refbox --features cyclic_stable
cargo test -p tiri-refbox --features cyclic # nightly
```
