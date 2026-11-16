# Workspace Crate Interface & Simplification Report

Scope: all workspace crates in [Cargo.toml](file:///home/blmarket/proj/hmux/tiri/Cargo.toml) **excluding `tiri-core` internals**, while auditing every call site across `src/`, `tiri-*/`, and `tests/` (including `tiri-core`) to measure what is actually used, duplicated, or over-exposed.

---

## Executive Summary

| Crate | What It Offers | External Prod Consumers | Highest-Leverage Simplifications |
| :--- | :--- | :--- | :--- |
| **`tiri`** ([src/lib.rs](file:///home/blmarket/proj/hmux/tiri/src/lib.rs)) | Engine + default-key wiring (`initialize`, `main`) and binary entrypoint | `src/main.rs` (`main`), `tests/*` (`initialize`) | Inline `tiri::main()` into `src/main.rs`; optionally fold `default_keys.rs` into `tiri-cmds` so `tiri` needs no `[lib]` target at all. |
| **`tiri-buffer`** ([lib.rs](file:///home/blmarket/proj/hmux/tiri/tiri-buffer/src/lib.rs)) | `SegmentedBuf` byte queue + `LineEnding` (`Lf`, `Legacy`) | `tiri-core`, `tiri-cmds` | Hide unused `chunks()`; change `make_contiguous(&mut self) -> &[u8]` (never mutated externally); change `append(&mut self, Self)` to take `source` by value (or `impl Into<Self>`) and add `take_bytes()` to replace 3 overlapping byte-extraction idioms. |
| **`tiri-cmdparse`** ([lib.rs](file:///home/blmarket/proj/hmux/tiri/tiri-cmdparse/src/lib.rs)) | LALRPOP command grammar (`parse`, `Context`, `Token`, `TokenText`, `ParseCommand`, `ParseArgument`) | **1 module**: `tiri-cmds/src/cmd/parse.rs` (+ `lexer.rs`) | Replace `TokenText` wrapper with `CString` (fixes forced re-allocation of every parsed argument); hide `#[doc(hidden)] Token::EqualsEnd`, `ParseCommand::{empty,new}`, byte spans `(usize, Token, usize)`, and `lalrpop_util::ParseError`; consider moving `lexer.rs` into `tiri-cmdparse`. |
| **`tiri-cmds`** ([lib.rs](file:///home/blmarket/proj/hmux/tiri/tiri-cmds/src/lib.rs)) | `ENGINE: CommandEngine` + 89 tmux commands (+ `test_cases` feature) | `src/lib.rs` (`ENGINE`), `tests/engine_wiring.rs` (`test_cases`) | Move the 36 `test_cases` tests into `tiri-cmds` `#[cfg(test)]` and delete `feature = "test-support"` + `pub mod test_cases`; declare direct Cargo deps instead of importing via `tiri_core::{libc, refbox, tiri_buffer, tiri_rt, tiri_sys}`; prune dead `pub` items & duplicate `cmd` getters; drop `Program::as_any` & `ProgramSource` on the `actions` boundary. |
| **`tiri-refbox`** ([lib.rs](file:///home/blmarket/proj/hmux/tiri/tiri-refbox/src/lib.rs)) | Single-owner `RefBox<T>` + allocation-free empty `Weak<T>` (`Borrow`, `Ref`, `BorrowError`) | `tiri-core`, `tiri-cmds` | Remove **16 unused public items** (`coerce!`, `coerce_weak!`, `new_cyclic`, `try_borrow_mut_or_else` x2, `try_downgrade`, `try_clone`, `weak_count` x2, `is_borrowed`, `into_raw`/`from_raw`/`cast` x2, `From<T>`, `RefBox == Weak`), unnameable `pub struct RefBoxHeap`, unused `cyclic`/`cyclic_stable` features, and overlapping `Weak == RefBox` / `try_access_mut`. |
| **`tiri-rt`** ([lib.rs](file:///home/blmarket/proj/hmux/tiri/tiri-rt/src/lib.rs)) | Single-threaded `LocalWaker` runtime (`mio::*`), `oneshot`, `channel`, `stream`, and 6 root traits | `tiri-core`, `tiri-cmds` (`oneshot`, tests) | Delete dead `channel` and `stream` modules (**0 external callers**); collapse the 6 single-implementation root traits (`Runtime`, `Handle`, `AsyncAccept`, `AsyncRead`, `AsyncWrite`, `Signals`) into inherent methods on `mio::*` types (and flatten `mio::*` to crate root). |
| **`tiri-sdbus`** ([lib.rs](file:///home/blmarket/proj/hmux/tiri/tiri-sdbus/src/lib.rs)) | Blocking Linux `sd-bus` wrappers (`Bus`, `Message`, `Variant`, `CallError`) | **1 function**: `tiri-core/src/compat/systemd.rs` | Make unused `Message::{append_bool, append_u32}` private; simplify `Bus::wait` return type; or fold the 277-line module directly into `tiri-core/src/compat/systemd.rs` (or expose a single transient-scope helper). |
| **`tiri-sys`** ([lib.rs](file:///home/blmarket/proj/hmux/tiri/tiri-sys/src/lib.rs)) | Safe OS/libc wrappers, `Notification`/`Notify`, `regex`, `platform::Platform` | `tiri-core`, `tiri-cmds`, tests | Delete completely unused `notification.rs` (`Notification`, `Notify`), 6 unused `Platform` methods + `ProcessInfo`, and `RegexStorage` (replace with `CompiledRegex::compile`); drop single-impl `Platform` trait in favor of plain `platform::*` functions; make `ensure_single_threaded` & `terminal_session` `pub(crate)`. |
| **`tiri-vt`** ([lib.rs](file:///home/blmarket/proj/hmux/tiri/tiri-vt/src/lib.rs)) | VT escape/OSC parser (`Parser`, `VtHost`), `base64`, `osc7501`, terminal reply parser | `tiri-core` (`window_pane`, `tty`, `options`) | Make `mod osc7501`, `mod parser`, all 12 `Parser` fields (fixing a live `input_end` bug in `tiri-core`), 5 internal `pub fn`s, `InputParam`, and 3 `INPUT_*` constants private; replace `TerminalHost` callback trait with pure `parse_terminal_reply(state, kind, bytes) -> Result<(usize, TerminalReport), ReplyError>`; remove dead `VtHost::reset_cell` and constant `queue: bool` on `VtHost::reply`; replace thread-local `INPUT_BUFFER_SIZE` with a `Parser` field; turn `InputEndType` into an enum. |

---

## Cross-Crate Dependency Hygiene: `tiri-core` Umbrella Re-exports

In [tiri-core/src/lib.rs](file:///home/blmarket/proj/hmux/tiri/tiri-core/src/lib.rs#L13):

```rust
pub use {libc, refbox, tiri_buffer, tiri_rt, tiri_sys};
```

- **Actual consumers**: Outside `tiri-core`, **only `tiri-cmds`** uses these re-exports (`libc`: 2 files; `refbox`: 26 files; `tiri_buffer`: 5 files; `tiri_rt`: 7 files; `tiri_sys`: 8 files). Inside `tiri-core` itself, 3 files (`cfg.rs`, `wait.rs`, `window_pane/mod.rs`) also write `crate::tiri_rt::...` alongside direct `tiri_rt::...` imports.
- **Simplification**: Declare `libc`, `refbox`, `tiri-buffer`, `tiri-rt`, and `tiri-sys` directly in [tiri-cmds/Cargo.toml](file:///home/blmarket/proj/hmux/tiri/tiri-cmds/Cargo.toml) and remove `pub use {libc, refbox, tiri_buffer, tiri_rt, tiri_sys};` from `tiri-core`. That makes `tiri-cmds`'s true dependency graph explicit in Cargo and stops `tiri-core` from acting as a grab-bag prelude.

---

## 1. `tiri` (Root Package)

**Files**: [Cargo.toml](file:///home/blmarket/proj/hmux/tiri/Cargo.toml), [src/main.rs](file:///home/blmarket/proj/hmux/tiri/src/main.rs), [src/lib.rs](file:///home/blmarket/proj/hmux/tiri/src/lib.rs), [src/default_keys.rs](file:///home/blmarket/proj/hmux/tiri/src/default_keys.rs)

### What It Offers
- Binary `tiri` (`fn main() { tiri::main() }`).
- Library `tiri` exporting two functions:
  - `pub fn initialize()` — installs `&tiri_cmds::ENGINE` and `&default_keys::DEFAULT_BINDINGS` into `tiri-core`.
  - `pub fn main()` — calls `initialize()` then `tiri_core::tmux::main()`.

### Simplifications
1. **Inline `pub fn main()` into `src/main.rs`**: `tiri::main()` has a single caller ([src/main.rs](file:///home/blmarket/proj/hmux/tiri/src/main.rs#L1-L3)) and is never called in tests. Moving those two lines into `src/main.rs` leaves `tiri::initialize()` as the sole library entrypoint.
2. **Optional crate-target consolidation**: `src/lib.rs` exists only because `DEFAULT_BINDINGS` lives in `src/default_keys.rs` (295 tmux `bind` command strings) and 3 integration test files ([tests/engine_wiring.rs](file:///home/blmarket/proj/hmux/tiri/tests/engine_wiring.rs), [tests/lazy_client_queue.rs](file:///home/blmarket/proj/hmux/tiri/tests/lazy_client_queue.rs), [tests/options_storage.rs](file:///home/blmarket/proj/hmux/tiri/tests/options_storage.rs)) call `tiri::initialize()`. None of those 3 test files inspect `DEFAULT_BINDINGS` — they only require `tiri_core::actions::install(&tiri_cmds::ENGINE)`. Moving `default_keys.rs` into `tiri-cmds` (or keeping it in `src/` alongside `main.rs`) would let tests call `tiri_cmds::install()` directly and eliminate the dual `[lib]` + `[[bin]]` split in the root package.

---

## 2. `tiri-buffer`

**Files**: [tiri-buffer/src/lib.rs](file:///home/blmarket/proj/hmux/tiri/tiri-buffer/src/lib.rs), [tiri-buffer/src/segmented.rs](file:///home/blmarket/proj/hmux/tiri/tiri-buffer/src/segmented.rs)

### What It Offers
`#![forbid(unsafe_code)]` segmented byte queue `SegmentedBuf` (`Default`, `Debug`, `From<Vec<u8>>`, `io::Write`, `len`, `is_empty`, `chunks`, `extend_from_slice`, `advance`, `make_contiguous`, `copy_prefix`, `copy_slice`, `read_line`, `append`) plus `LineEnding { Lf, Legacy }`.

### Simplifications
1. **Unreached public method — `SegmentedBuf::chunks` ([segmented.rs](file:///home/blmarket/proj/hmux/tiri/tiri-buffer/src/segmented.rs#L86-L88))**:
   - **0 callers** outside `tiri-buffer` (neither production nor external tests). Only `copy_prefix`, `copy_slice`, and `tiri-buffer/tests/contracts.rs` call it. Making `chunks` private stops leaking segment boundaries as a public contract (and allows `contracts.rs` tests to move to `#[cfg(test)]` in `segmented.rs` or test observable behavior).
2. **Over-wide mutability — `make_contiguous(&mut self) -> &mut [u8]` ([segmented.rs](file:///home/blmarket/proj/hmux/tiri/tiri-buffer/src/segmented.rs#L144-L158))**:
   - Across all 15 production call sites in `tiri-core` and `tiri-cmds`, **zero callers ever mutate the returned slice**; every site immediately reads it as `&[u8]` (returning `&mut [u8]` forces `segmented.rs:144` to invalidate `self.scan = None` unconditionally even on a 1-segment buffer because the caller *might* overwrite line-ending bytes in place). Returning `&[u8]` preserves `self.scan` when `self.segments.len() <= 1` and tightens the invariant.
3. **Awkward ownership & overlapping APIs on `append` / `From<Vec<u8>>` ([segmented.rs](file:///home/blmarket/proj/hmux/tiri/tiri-buffer/src/segmented.rs#L29-L42), [L206-L220](file:///home/blmarket/proj/hmux/tiri/tiri-buffer/src/segmented.rs#L206-L220))**:
   - Every external caller of `append(&mut self, source: &mut Self)` passes an owned/temporary buffer and drops `source` on the next line; two sites ([control.rs](file:///home/blmarket/proj/hmux/tiri/tiri-core/src/control.rs#L691), [file/stream.rs](file:///home/blmarket/proj/hmux/tiri/tiri-core/src/file/stream.rs#L78)) write `buf.append(&mut SegmentedBuf::from(bytes))` just to adopt an owned `Vec<u8>` without copying.
   - Changing `pub fn append(&mut self, source: impl Into<Self>)` (taking ownership by value) lets callers pass either `other_buf` (`std::mem::take` if from a field) or `vec_bytes` directly with zero copy and no `&mut SegmentedBuf::from(...)` ceremony.
4. **Overlapping whole-buffer extraction idioms**:
   - Callers currently extract all bytes into an owned `Vec<u8>` in two different ways: `buf.make_contiguous().to_vec()` ([job.rs](file:///home/blmarket/proj/hmux/tiri/tiri-core/src/job.rs#L50), [run_shell.rs](file:///home/blmarket/proj/hmux/tiri/tiri-cmds/src/entries/run_shell.rs#L410), [events_payload.rs](file:///home/blmarket/proj/hmux/tiri/tiri-core/src/events_payload.rs#L309), [json.rs](file:///home/blmarket/proj/hmux/tiri/tiri-core/src/json.rs#L425)) vs `buf.copy_prefix(buf.len())` ([window_pane/process.rs](file:///home/blmarket/proj/hmux/tiri/tiri-core/src/window_pane/process.rs#L186)). Notice `make_contiguous().to_vec()` copies **twice** when there are multiple segments (once to coalesce into a single segment, then `.to_vec()`). Providing `into_vec(self) -> Vec<u8>` (which is zero-copy when `segments.len() == 1 && start == 0`!) replaces those double-allocating patterns cleanly.
5. **Unused `#[derive(Debug)]` on `SegmentedBuf` and `LineEnding`**: Never formatted outside `tiri-buffer` (`LineEnding::Debug` only in `tiri-buffer`'s own tests; `SegmentedBuf::Debug` nowhere at all).

---

## 3. `tiri-cmdparse`

**Files**: [tiri-cmdparse/src/lib.rs](file:///home/blmarket/proj/hmux/tiri/tiri-cmdparse/src/lib.rs), [tiri-cmdparse/src/parse_grammar.lalrpop](file:///home/blmarket/proj/hmux/tiri/tiri-cmdparse/src/parse_grammar.lalrpop)

### What It Offers
Extracted `#![forbid(unsafe_code)]` LALRPOP grammar for tmux command files/strings: `parse(context, tokens) -> Result<Vec<ParseCommand>, ParseError>`, `Context` trait, `TokenText`, `Token` (13 variants including `#[doc(hidden)] EqualsEnd`), `LexError`, `ParseArgument`, `ParseCommand`.

### Actual Consumption
Consumed **exclusively** by one module in the entire workspace: [tiri-cmds/src/cmd/parse.rs](file:///home/blmarket/proj/hmux/tiri/tiri-cmds/src/cmd/parse.rs) and its private child [tiri-cmds/src/cmd/parse/lexer.rs](file:///home/blmarket/proj/hmux/tiri/tiri-cmds/src/cmd/parse/lexer.rs).

### Simplifications

#### A. Immediate Interface Simplifications (keeping current crate boundary)
1. **Delete `pub struct TokenText(CString)` ([lib.rs](file:///home/blmarket/proj/hmux/tiri/tiri-cmdparse/src/lib.rs#L43-L56)) — use `CString` directly**:
   - `TokenText` is a trivial newtype over `std::ffi::CString` with only `from_cstring(CString)` and `as_c_str(&self) -> &CStr` — and **no `into_cstring(self) -> CString`**.
   - Because ownership cannot be extracted back out of `TokenText`, [tiri-cmds/src/cmd/parse.rs](file:///home/blmarket/proj/hmux/tiri/tiri-cmds/src/cmd/parse.rs#L200-L201) is forced to clone every parsed string argument (`text.as_c_str().to_owned()`) and drop the inner `CString`! Replacing `TokenText` with `CString` deletes a wrapper type, deletes helper shims `take_parser_token` / `text` in `tiri-cmds`, and eliminates one heap allocation per parsed token.
2. **Remove `lalrpop_util` from the public API & drop unused `(usize, Token, usize)` spans ([lib.rs](file:///home/blmarket/proj/hmux/tiri/tiri-cmdparse/src/lib.rs#L24-L39))**:
   - `pub type ParseError = lalrpop_util::ParseError<usize, Token, LexError>;` leaks `lalrpop-util` into the crate's public signature, yet **no caller ever names `ParseError` or inspects its payload** ([parse.rs](file:///home/blmarket/proj/hmux/tiri/tiri-cmds/src/cmd/parse.rs#L220) matches `Err(_)`, and `tests/grammar.rs` calls `.unwrap()` / `.is_err()`).
   - Likewise, the `usize` start/end byte offsets in `Item = Result<(usize, Token, usize), LexError>` are never read by `tiri-cmdparse` or `tiri-cmds`.
   - Changing `parse` to accept `impl IntoIterator<Item = Result<Token, LexError>>` and return `Result<Vec<ParseCommand>, ParseError>` with an opaque `#[derive(Debug)] pub struct ParseError;` (using `type Location = ();` in LALRPOP) removes dummy span plumbing from both `lexer.rs` and `tests/grammar.rs` and encapsulates LALRPOP completely.
3. **Eliminate `#[doc(hidden)] Token::EqualsEnd` ([lib.rs](file:///home/blmarket/proj/hmux/tiri/tiri-cmdparse/src/lib.rs#L74-L75))**:
   - `EqualsEnd` is an internal lookahead marker spliced by `parse()`, forced onto `pub enum Token` only because `parse_grammar.lalrpop` uses `Token` directly. Mapping public `Token` -> `pub(crate) enum GrammarToken` inside `parse()` removes `#[doc(hidden)]` and makes exhaustive `match token` in callers clean.
4. **Tighten visibility on `ParseCommand::empty` and `ParseCommand::new` ([lib.rs](file:///home/blmarket/proj/hmux/tiri/tiri-cmdparse/src/lib.rs#L98-L109))**:
   - Never called outside `parse_grammar.lalrpop`; make both `pub(crate)` (and drop unused `Clone` on `Token` once `lalrpop` uses `GrammarToken` or if LALRPOP doesn't require it).

#### B. Structural Simplification: Move `lexer.rs` into `tiri-cmdparse`
- Right now, splitting the lexer ([tiri-cmds/src/cmd/parse/lexer.rs](file:///home/blmarket/proj/hmux/tiri/tiri-cmds/src/cmd/parse/lexer.rs)) from the LALRPOP grammar ([tiri-cmdparse](file:///home/blmarket/proj/hmux/tiri/tiri-cmdparse/src/lib.rs)) forces a delicate, stateful streaming contract across the crate boundary:
  - The caller must remember to emit a synthetic trailing `Token::Newline` at EOF;
  - Lexing must stay lazy and release its `RefCell<ParseSession>` borrow before yielding each token because grammar actions mutate environment state mid-parse;
  - Diagnostics are side-channeled into `ParseSession.error` while returning an empty `LexError` marker through LALRPOP.
- `lexer.rs` is pure safe byte scanning except for two tiny host lookups (`environ_find` for `$VAR`/`$HOME` and `user_home`/`user_home_by_name` for `~`). Adding `get_environ(&self, name: &CStr) -> Option<CString>` and `home_dir(&self, user: Option<&CStr>) -> Option<CString>` (plus `advance_line`) to `tiri_cmdparse::Context` allows **all of `lexer.rs` to move into `tiri-cmdparse`**.
- Result: `Token` and `LexError` become completely private to `tiri-cmdparse`, and its entire public API shrinks to `Context`, `ParseCommand`, `ParseArgument`, `SourceMode { Bytes, File }`, and `parse(source: &[u8], mode: SourceMode, ctx: &mut dyn Context) -> Result<Vec<ParseCommand>, CString>`.

---

## 4. `tiri-cmds` (and the `tiri_core::actions` Boundary)

**Files**: [tiri-cmds/src/lib.rs](file:///home/blmarket/proj/hmux/tiri/tiri-cmds/src/lib.rs), [tiri-cmds/src/engine.rs](file:///home/blmarket/proj/hmux/tiri/tiri-cmds/src/engine.rs), [tiri-core/src/actions.rs](file:///home/blmarket/proj/hmux/tiri/tiri-core/src/actions.rs)

### What It Offers
- `pub static ENGINE: CommandEngine` — function-pointer vtable (`parse`, `parse_argv`, `command_names`, `run`) installed into `tiri_core::actions` at startup, keeping `tiri-core` free of concrete interpreter types (enforced by [tests/scripting_boundary.rs](file:///home/blmarket/proj/hmux/tiri/tests/scripting_boundary.rs)).
- `#[cfg(feature = "test-support")] pub mod test_cases` — re-exports 5 internal test modules (36 functions) called from [tests/engine_wiring.rs](file:///home/blmarket/proj/hmux/tiri/tests/engine_wiring.rs).

### Simplifications
1. **Remove `pub mod test_cases` and Cargo feature `test-support` ([tiri-cmds/Cargo.toml](file:///home/blmarket/proj/hmux/tiri/tiri-cmds/Cargo.toml#L7-L8), [tiri-cmds/src/lib.rs](file:///home/blmarket/proj/hmux/tiri/tiri-cmds/src/lib.rs#L23-L30))**:
   - Every one of the 36 functions in `test_cases::{client_queue_tests, confirm_before_tests, load_buffer_tests, run_shell_tests, wait_for_tests}` is called once in `tests/engine_wiring.rs` immediately after `tiri::initialize()`, and none of them use `DEFAULT_BINDINGS`.
   - Making those 36 functions ordinary `#[test]` unit tests inside `tiri-cmds` that call `tiri_core::actions::install(&crate::ENGINE)` (with `tiri-core = { path = "../tiri-core", features = ["test-support"] }` under `[dev-dependencies]`) reduces `tiri-cmds`'s public surface to **one item (`pub static ENGINE`)** and removes the `test-support` feature from `tiri-cmds`.
2. **Simplify `tiri_core::actions` engine contract ([tiri-core/src/actions.rs](file:///home/blmarket/proj/hmux/tiri/tiri-core/src/actions.rs))**:
   - **Delete `Program::as_any(&self) -> &dyn Any` ([actions.rs](file:///home/blmarket/proj/hmux/tiri/tiri-core/src/actions.rs#L11))**: `Program: Any` already supports native Rust trait upcasting (`Rc<dyn Program>` -> `Rc<dyn Any>` and `&dyn Program` -> `&dyn Any`, which `tiri-cmds` already uses in 3 places). The only 2 callers of `.as_any()` ([tiri-cmds/src/cmd/core.rs](file:///home/blmarket/proj/hmux/tiri/tiri-cmds/src/cmd/core.rs#L167), [L224](file:///home/blmarket/proj/hmux/tiri/tiri-cmds/src/cmd/core.rs#L224)) are in `cmd_list_copy` and `cmd_list_first`, where every caller *already statically holds `&cmd_list` / `&Rc<cmd_list>`*! Changing `cmd_list_copy(&cmd_list)` and `cmd_list_first(&cmd_list)` removes unnecessary runtime downcasts and makes `as_any` dead.
   - **Delete `pub trait ProgramSource` ([actions.rs](file:///home/blmarket/proj/hmux/tiri/tiri-core/src/actions.rs#L168-L180)) and `impl<T: Program + ?Sized> Program for Rc<T>` ([L182-L198](file:///home/blmarket/proj/hmux/tiri/tiri-core/src/actions.rs#L182-L198))**:
     - `ProgramSource` exists only so `ParseValue::Commands` (`tiri-core/src/arguments/value.rs:10`) can accept `&Rc<cmd_list>` alongside `&ProgramRef` (the other two `impl ProgramSource` sites in `hooks.rs` and `window_panes.rs` already pass `ProgramRef`!). Passing `ProgramRef` directly into `ParseValue::Commands` eliminates `ProgramSource`.
     - Blanket `impl Program for Rc<T>` is relied upon at a single call site ([tiri-cmds/src/entries/list_keys.rs](file:///home/blmarket/proj/hmux/tiri/tiri-cmds/src/entries/list_keys.rs#L135)), which can call `bd.cmdlist().text(...)` directly.
   - **Deduplicate `capitalize_error` vs `cmd_parse_error_uppercase_first`**: [tiri-core/src/actions.rs](file:///home/blmarket/proj/hmux/tiri/tiri-core/src/actions.rs#L152-L160) (`sources::capitalize_error`) and [tiri-cmds/src/cmd/parse.rs](file:///home/blmarket/proj/hmux/tiri/tiri-cmds/src/cmd/parse.rs#L78-L87) (`cmd_parse_error_uppercase_first`) have identical `&mut Option<CString>` signatures and identical behavior. Delete the copy in `tiri-cmds` and use `tiri_core::actions::sources::capitalize_error`.
   - **Trim unused `actions` exports**: Make `actions::engine()` private (0 external callers); move `PARSE_NOALIAS` and `PARSE_VERBOSE` into `tiri-cmds` (0 uses in `tiri-core`); remove unused re-exports `append_argv`, `mouse_window`, and `InvalidArgv` from `actions.rs`.
3. **Clean up internal dead & overlapping items in `tiri-cmds` (masked by `#![allow(dead_code)]`)**:
   - Delete dead items with 0 references anywhere: `DQ`/`SQ`/`NQ`/`C2RustUnnamed_38` and `cmd_list_any_have` in [cmd/core.rs](file:///home/blmarket/proj/hmux/tiri/tiri-cmds/src/cmd/core.rs#L13-L16), and `_PATH_DEV` in [cmd/find.rs](file:///home/blmarket/proj/hmux/tiri/tiri-cmds/src/cmd/find.rs#L42).
   - Remove overlapping `cmd` accessors: `cmd` fields in [shared/command.rs](file:///home/blmarket/proj/hmux/tiri/tiri-cmds/src/shared/command.rs#L66-L72) are `pub`, while [cmd/core.rs](file:///home/blmarket/proj/hmux/tiri/tiri-cmds/src/cmd/core.rs#L18-L35) also exports C-style getters `cmd_get_entry`, `cmd_get_args`, `cmd_get_source`, `cmd_get_parse_flags` (and `cmd_table()` duplicating `&catalog::CMD_TABLE`). Pick direct field access (or methods on `cmd`) and remove the C-style getters.
   - Narrow `TargetLookup` in [cmd/find.rs](file:///home/blmarket/proj/hmux/tiri/tiri-cmds/src/cmd/find.rs#L74-L91): 6 of its 7 trait methods are only called inside `find.rs`; external callers only use `find_target`.

---

## 5. `tiri-refbox` (`refbox`)

**Files**: [tiri-refbox/Cargo.toml](file:///home/blmarket/proj/hmux/tiri/tiri-refbox/Cargo.toml), [tiri-refbox/src/lib.rs](file:///home/blmarket/proj/hmux/tiri/tiri-refbox/src/lib.rs), [tiri-refbox/src/internals.rs](file:///home/blmarket/proj/hmux/tiri/tiri-refbox/src/internals.rs)

### What It Offers
Local fork of upstream `refbox 0.4.1` adding const allocation-free `Weak::new()` / `Weak::is_empty()` and shared borrows (`Ref<'_, T>` / `try_borrow()`).

### Actual Consumption
Heavily used across `tiri-core` and `tiri-cmds` for single-owner `RefBox<T>` + `Weak<T>` (`RefBox::{new, downgrade, try_borrow, try_borrow_mut, as_ptr, Default}`, `Weak::{new, Default, Clone, PartialEq, is_empty, is_alive, is, try_borrow, try_borrow_mut, as_ptr}`, `Borrow`, `Ref`, `BorrowError`). However, roughly **half of the upstream fork's API surface is completely unused** in this workspace.

### Simplifications
1. **Remove unused Cargo features `cyclic` and `cyclic_stable` + `RefBox::new_cyclic` ([Cargo.toml](file:///home/blmarket/proj/hmux/tiri/tiri-refbox/Cargo.toml#L12-L14), [lib.rs](file:///home/blmarket/proj/hmux/tiri/tiri-refbox/src/lib.rs#L248-L251), [internals.rs](file:///home/blmarket/proj/hmux/tiri/tiri-refbox/src/internals.rs#L228-L273))**:
   - Neither feature is enabled anywhere in the workspace (`tiri-core` sets self-observers after construction via `Weak::new()`). Removing them deletes `#![cfg_attr(feature = "cyclic", feature(layout_for_ptr))]`, conditional `Layout` storage in `RefBoxHeapInner`, `new_cyclic_ref_box`, and 3-way `#[cfg]` branches in `dealloc_heap`.
2. **Remove 15 additional unused public items (0 callers outside `tiri-refbox`)**:
   - Macros: `coerce!` ([lib.rs](file:///home/blmarket/proj/hmux/tiri/tiri-refbox/src/lib.rs#L111)), `coerce_weak!` ([L145](file:///home/blmarket/proj/hmux/tiri/tiri-refbox/src/lib.rs#L145)).
   - Raw-pointer / unsizing / casting APIs: `RefBox::{into_raw, from_raw, cast}` ([L361-L388](file:///home/blmarket/proj/hmux/tiri/tiri-refbox/src/lib.rs#L361-L388)) and `Weak::{into_raw, from_raw, cast}` ([L629-L657](file:///home/blmarket/proj/hmux/tiri/tiri-refbox/src/lib.rs#L629-L657)). Note also that `into_raw`/`from_raw` currently leak `pub struct RefBoxHeap<T>` from private `mod internals` without re-exporting it (`pub`-in-private type)! Removing `into_raw`/`from_raw`/`cast`/`coerce*` lets `RefBoxHeap` become `pub(crate)`.
   - Overflow-fallible / introspection methods: `RefBox::try_downgrade` ([L340](file:///home/blmarket/proj/hmux/tiri/tiri-refbox/src/lib.rs#L340)), `RefBox::weak_count` ([L349](file:///home/blmarket/proj/hmux/tiri/tiri-refbox/src/lib.rs#L349)), `Weak::try_clone` ([L579](file:///home/blmarket/proj/hmux/tiri/tiri-refbox/src/lib.rs#L579)), `Weak::weak_count` ([L592](file:///home/blmarket/proj/hmux/tiri/tiri-refbox/src/lib.rs#L592)), `Weak::is_borrowed` ([L602](file:///home/blmarket/proj/hmux/tiri/tiri-refbox/src/lib.rs#L602)).
   - Custom-error & trait impls: `RefBox::try_borrow_mut_or_else` ([L275](file:///home/blmarket/proj/hmux/tiri/tiri-refbox/src/lib.rs#L275)), `Weak::try_borrow_mut_or_else` ([L522](file:///home/blmarket/proj/hmux/tiri/tiri-refbox/src/lib.rs#L522)), `From<T> for RefBox<T>` ([L204](file:///home/blmarket/proj/hmux/tiri/tiri-refbox/src/lib.rs#L204)), `PartialEq<Weak<T>> for RefBox<T>` ([L191](file:///home/blmarket/proj/hmux/tiri/tiri-refbox/src/lib.rs#L191)).
3. **Resolve overlapping APIs**:
   - **Identity check (`Weak::is(&RefBox<T>)` vs `PartialEq<RefBox<T>> for Weak<T>`)**: `weak.is(&owner)` is used at 17 sites, whereas `weak == owner` (`PartialEq<RefBox<T>>` at [L454](file:///home/blmarket/proj/hmux/tiri/tiri-refbox/src/lib.rs#L454)) is used at only 3 sites ([server_client/api.rs](file:///home/blmarket/proj/hmux/tiri/tiri-core/src/server_client/api.rs#L423), [window_pane/input.rs](file:///home/blmarket/proj/hmux/tiri/tiri-core/src/window_pane/input.rs#L2028), [L2052](file:///home/blmarket/proj/hmux/tiri/tiri-core/src/window_pane/input.rs#L2052)) and can be misread as comparing `T` values rather than pointer identity. Migrate those 3 sites to `.is(...)` and remove cross-type `PartialEq<RefBox<T>>`.
   - **Closure borrow (`try_access_mut` vs `try_borrow_mut`)**: `RefBox::try_access_mut` (3 prod sites) and `Weak::try_access_mut` (1 prod site) are 2-line wrappers around `try_borrow_mut()` (used ~135 times). Migrating those 4 sites removes two redundant methods.

---

## 6. `tiri-rt`

**Files**: [tiri-rt/src/lib.rs](file:///home/blmarket/proj/hmux/tiri/tiri-rt/src/lib.rs), [channel.rs](file:///home/blmarket/proj/hmux/tiri/tiri-rt/src/channel.rs), [oneshot.rs](file:///home/blmarket/proj/hmux/tiri/tiri-rt/src/oneshot.rs), [stream.rs](file:///home/blmarket/proj/hmux/tiri/tiri-rt/src/stream.rs), [src/mio/*](file:///home/blmarket/proj/hmux/tiri/tiri-rt/src/mio/mod.rs)

### What It Offers
Single-threaded `LocalWaker` async runtime:
- Root traits: `Runtime`, `Handle`, `AsyncAccept`, `AsyncRead`, `AsyncWrite`, `Signals` + `Received`.
- `mio` backend: `Runtime`, `Handle`, `Task`, `Io`, `Listener`, `Signals` (+ unexported `Recv`), `Sleep`.
- Channels & adapters: `oneshot::{channel, Sender, Receiver}`, `channel::{channel, Sender, Receiver}`, `stream::{Reader, Writer}`.

### Simplifications
1. **Delete completely unused modules `tiri_rt::channel` and `tiri_rt::stream`**:
   - [tiri-rt/src/channel.rs](file:///home/blmarket/proj/hmux/tiri/tiri-rt/src/channel.rs) (189 lines, MPSC queue) has **0 callers** outside its own inline unit tests.
   - [tiri-rt/src/stream.rs](file:///home/blmarket/proj/hmux/tiri/tiri-rt/src/stream.rs) (121 lines, `Reader` / `Writer` directional wrappers + `read_chunk`) has **0 callers** outside `tiri-rt/tests/`.
   - Removing both cuts ~310 lines of dead runtime code and associated integration tests.
2. **Eliminate the 6 single-implementation root traits & flatten `mio::*` to crate root ([tiri-rt/src/lib.rs](file:///home/blmarket/proj/hmux/tiri/tiri-rt/src/lib.rs#L18-L122))**:
   - Once `stream::{Reader, Writer}` is removed, **every single trait in `tiri-rt` (`Runtime`, `Handle`, `AsyncAccept`, `AsyncRead`, `AsyncWrite`, `Signals`) has exactly one implementation (`mio::*`)**, and **zero functions anywhere in the workspace are generic over any of them**.
   - Currently, every caller has to write `use tiri_rt::{AsyncRead as _, AsyncWrite as _, Runtime as _, ...};` alongside `tiri_rt::mio::{Io, Runtime, ...}` just to call `.new()`, `.poll()`, `.spawn()`, `.accept()`, `.read()`, `.write()`, or `.recv()` on concrete `mio` types.
   - Making those methods inherent on `Runtime`, `Handle`, `Listener`, `Io`, and `Signals` and re-exporting those types directly at the `tiri_rt` root (removing the `mio` module prefix, or keeping a type alias if desired) removes 6 traits, eliminates `mio::signals::Recv` being a `pub`-in-private associated type (`async fn recv(&mut self)` works directly on inherent methods), and simplifies every caller.
3. **Remove or restrict `Runtime::handle(&self)` ([mio/runtime.rs](file:///home/blmarket/proj/hmux/tiri/tiri-rt/src/mio/runtime.rs#L334-L338))**:
   - Production code exclusively uses `Handle::current()` (4 sites in `tiri-core/src/reactor/`); `runtime.handle()` has only 1 external call site in a unit test ([tiri-core/src/server_client/mod.rs](file:///home/blmarket/proj/hmux/tiri/tiri-core/src/server_client/mod.rs#L3991), where `Handle::current()` also works since `Runtime::new()` initializes the thread-local handle).

*(Note on `tiri-core/src/reactor` consumer side: `reactor::io(BorrowedFd)` in [reactor/mod.rs](file:///home/blmarket/proj/hmux/tiri/tiri-core/src/reactor/mod.rs#L25-L28) duplicates the `fd.try_clone_to_owned()?` + `Io::new` pattern manually written in `control.rs` and `tty/control.rs` — an `Io::from_borrowed(BorrowedFd)` constructor on `tiri_rt::Io` or consistent use of `reactor::io` would unify all 5 descriptor-registration sites.)*

---

## 7. `tiri-sdbus`

**Files**: [tiri-sdbus/Cargo.toml](file:///home/blmarket/proj/hmux/tiri/tiri-sdbus/Cargo.toml), [tiri-sdbus/src/lib.rs](file:///home/blmarket/proj/hmux/tiri/tiri-sdbus/src/lib.rs)

### What It Offers
277-line Linux-only safe blocking wrapper around `libsystemd-sys::bus`: `Bus` (`default_user`, `new_method_call`, `call`, `with_match`, `process`, `wait`), `CallError`, `Variant<'a>`, `Message` (`append_str`, `append_bool`, `append_u32`, `open_container`, `close_container`, `append_property`, `read_u32`, `read_object_path`).

### Actual Consumption
Used by **a single function in the entire workspace**: `systemd_move_to_new_cgroup` in [tiri-core/src/compat/systemd.rs](file:///home/blmarket/proj/hmux/tiri/tiri-core/src/compat/systemd.rs#L92-L199), and has **0 tests** of its own.

### Simplifications
1. **Tighten unused / over-general methods (if kept as a separate crate)**:
   - Make `Message::append_bool` ([lib.rs](file:///home/blmarket/proj/hmux/tiri/tiri-sdbus/src/lib.rs#L195)) and `Message::append_u32` ([L200](file:///home/blmarket/proj/hmux/tiri/tiri-sdbus/src/lib.rs#L200)) private — they have **0 external callers** (only used inside `append_property`).
   - `Bus::wait(&self, timeout: Duration) -> io::Result<()>` ([L155](file:///home/blmarket/proj/hmux/tiri/tiri-sdbus/src/lib.rs#L155)): its `bool` return value is ignored by the sole caller.
   - `Message::open_container` ([L206](file:///home/blmarket/proj/hmux/tiri/tiri-sdbus/src/lib.rs#L206)) is only called externally with `b'a'` (`open_array(contents: &CStr)` avoids raw magic byte `'a'` at call sites while keeping `open_container` private).
2. **Crate-level simplification (recommended evaluation)**:
   - Because `tiri-core/src/compat/systemd.rs` is the *only* user and already depends on Linux `systemd` crates, `tiri-sdbus` is a "pass-through" crate built around one D-Bus call (`StartTransientUnit` + `JobRemoved` signal match). Either:
     - **Option A**: Expose a single domain function `tiri_sdbus::start_transient_scope(...)` (hiding `Bus`, `Message`, `CallError`, container signatures, and signal-match plumbing behind one safe function), OR
     - **Option B**: Fold `tiri-sdbus/src/lib.rs` into `tiri-core` (`ffi/sdbus.rs` + `compat/systemd.rs`) and remove one workspace crate. *(Note: [tests/ffi_boundary.rs](file:///home/blmarket/proj/hmux/tiri/tests/ffi_boundary.rs) only restricts `extern` blocks, not `libsystemd_sys` crate calls.)*

---

## 8. `tiri-sys`

**Files**: [tiri-sys/src/lib.rs](file:///home/blmarket/proj/hmux/tiri/tiri-sys/src/lib.rs), [notification.rs](file:///home/blmarket/proj/hmux/tiri/tiri-sys/src/notification.rs), [regex.rs](file:///home/blmarket/proj/hmux/tiri/tiri-sys/src/regex.rs), [platform/mod.rs](file:///home/blmarket/proj/hmux/tiri/tiri-sys/src/platform/mod.rs), [platform/linux.rs](file:///home/blmarket/proj/hmux/tiri/tiri-sys/src/platform/linux.rs), [platform/darwin.rs](file:///home/blmarket/proj/hmux/tiri/tiri-sys/src/platform/darwin.rs)

### What It Offers
Runtime-independent OS boundary:
- ~50 safe POSIX/libc wrapper functions & types (`set_nonblocking`, `open`, `close_on_exec_from`, `fork`, `daemonize`, sockets, termios/pty, `make_temporary`, signals, time, `glob`, `fnmatch`, user/group lookups, `Program`/`exec`/`exec_with_stdio`/`exit_now`, etc.).
- `Notification` & `Notify` trait (`eventfd` / pipe coalescing wakeups).
- `regex`: `RegexStorage`, `CompiledRegex`, `RegexMatch`.
- `platform`: `Platform` trait, `CurrentPlatform` (`Linux` / `Darwin`), `ProcessInfo`.

### Simplifications
1. **Delete `notification.rs` (`Notification` and `Notify` trait, [notification.rs](file:///home/blmarket/proj/hmux/tiri/tiri-sys/src/notification.rs#L1-L103))**:
   - **0 references anywhere in the workspace** — neither production code nor any unit/integration test uses `Notification` or `Notify`.
2. **Prune 6 dead `Platform` methods + `ProcessInfo`, and drop the `Platform` trait ([platform/mod.rs](file:///home/blmarket/proj/hmux/tiri/tiri-sys/src/platform/mod.rs#L26-L95))**:
   - Out of 10 methods on `Platform`, **6 methods and `ProcessInfo` have 0 callers outside `tiri-sys`**:
     - `ProcessInfo`
     - `Platform::process_table`
     - `Platform::process_programs`
     - `Platform::process_cwd`
     - `Platform::process_open_files`
     - `Platform::process_start_time`
     - `Platform::process_environ`
     - *(Deleting these removes ~120 lines of `/proc` parsing in [linux.rs](file:///home/blmarket/proj/hmux/tiri/tiri-sys/src/platform/linux.rs) and ~200 lines of `proc_pidinfo`/`VnodeFdInfoWithPath` FFI structs in [darwin.rs](file:///home/blmarket/proj/hmux/tiri/tiri-sys/src/platform/darwin.rs)!)*
   - Only **4 operations** are actually used externally: `pane_cwd`, `pane_name` (in [tiri-core/src/osdep.rs](file:///home/blmarket/proj/hmux/tiri/tiri-core/src/osdep.rs#L10-L17)), `process_arguments`, and `process_waiting_for_tty` (in [tiri-core/src/window_pane/process_status.rs](file:///home/blmarket/proj/hmux/tiri/tiri-core/src/window_pane/process_status.rs#L161-L188)).
   - Furthermore, no code is generic over `P: Platform`; every caller imports both `CurrentPlatform` and `Platform` just to call `CurrentPlatform::foo(...)`. Exporting 4 plain free functions (`platform::pane_cwd`, `platform::pane_name`, `platform::process_arguments`, `platform::process_waiting_for_tty`) dispatched by `#[cfg(target_os = ...)]` matches the style of the rest of `tiri-sys` (like `daemonize`, `peer_credentials`, `signal_name`) and removes `trait Platform`, `CurrentPlatform`, `pub struct Linux`, and `pub struct Darwin`.
3. **Delete `RegexStorage` in `tiri_sys::regex` — replace with `CompiledRegex::compile` ([regex.rs](file:///home/blmarket/proj/hmux/tiri/tiri-sys/src/regex.rs#L10-L29))**:
   - `RegexStorage(MaybeUninit<regex_t>)` does not reuse heap memory: `compile(&mut self, ...)` zeroes `self.0`, calls `regcomp`, and moves `regex_t` **by value** into `CompiledRegex(regex_t)`.
   - Every single caller in the workspace (all 5 sites: [expression.rs](file:///home/blmarket/proj/hmux/tiri/tiri-core/src/format/expression.rs#L603), [regsub.rs](file:///home/blmarket/proj/hmux/tiri/tiri-core/src/regsub.rs#L56), [window_copy.rs](file:///home/blmarket/proj/hmux/tiri/tiri-core/src/window_copy.rs#L5912), [L6417](file:///home/blmarket/proj/hmux/tiri/tiri-core/src/window_copy.rs#L6417), [window_pane/mod.rs](file:///home/blmarket/proj/hmux/tiri/tiri-core/src/window_pane/mod.rs#L1452)) creates a local `let mut storage = RegexStorage::default();`, compiles **once**, and drops `storage`.
   - Replacing `RegexStorage` with `CompiledRegex::compile(pattern: &CStr, flags: c_int) -> Result<Self, c_int>` deletes an entire public type and removes boilerplate at all 5 call sites.
4. **Make internal helpers private**:
   - `ensure_single_threaded` ([lib.rs](file:///home/blmarket/proj/hmux/tiri/tiri-sys/src/lib.rs#L116)) and `terminal_session` ([L355](file:///home/blmarket/proj/hmux/tiri/tiri-sys/src/lib.rs#L355)) have **0 callers outside `tiri-sys`** (used only inside `tiri-sys` itself).
   - Note also: `status_flags` ([L868](file:///home/blmarket/proj/hmux/tiri/tiri-sys/src/lib.rs#L868)) and `is_open` ([L873](file:///home/blmarket/proj/hmux/tiri/tiri-sys/src/lib.rs#L873)) are **only called in tests** (never in production code).

*(Consumer overlap note in `tiri-core`: [tiri-core/src/shared/terminal.rs](file:///home/blmarket/proj/hmux/tiri/tiri-core/src/shared/terminal.rs#L17-L38) defines 1-line pass-throughs `read_attributes`, `set_attributes`, `read_size`, `set_size` with identical signatures to `tiri_sys::{terminal_attributes, set_terminal_attributes, terminal_size, set_terminal_size}`, and files like `client.rs` and `tty/control.rs` call both in the same file.)*

---

## 9. `tiri-vt`

**Files**: [tiri-vt/src/lib.rs](file:///home/blmarket/proj/hmux/tiri/tiri-vt/src/lib.rs), [base64.rs](file:///home/blmarket/proj/hmux/tiri/tiri-vt/src/base64.rs), [osc7501.rs](file:///home/blmarket/proj/hmux/tiri/tiri-vt/src/osc7501.rs), [parser.rs](file:///home/blmarket/proj/hmux/tiri/tiri-vt/src/parser.rs), [terminal.rs](file:///home/blmarket/proj/hmux/tiri/tiri-vt/src/terminal.rs)

### What It Offers
- `Parser` + 74-method `VtHost` trait, SGR/WinOp/OSC-133/OSC-7501 types, and `buffer_size` / `set_buffer_size`.
- `parse_terminal_reply` + `TerminalHost`, `TerminalReplyKind`, `TerminalReplyState`, `TerminalReport<'a>`, `ReplyError`.
- `pub mod base64` (`encode`, `decode`), `pub mod osc7501`, `pub mod parser`.

### Simplifications
1. **Make `mod osc7501` and `mod parser` private, and hide 5 internal `pub fn`s + `InputParam` + 3 `INPUT_*` constants**:
   - Zero external code imports `tiri_vt::osc7501::*` or `tiri_vt::parser::*` (all callers use root re-exports, just like already-private `mod terminal`).
   - Inside `parser.rs`, **5 `pub fn`s have 0 external callers**: `input_split` ([L579](file:///home/blmarket/proj/hmux/tiri/tiri-vt/src/parser.rs#L579)), `input_get` ([L606](file:///home/blmarket/proj/hmux/tiri/tiri-vt/src/parser.rs#L606)), `input_input` ([L663](file:///home/blmarket/proj/hmux/tiri/tiri-vt/src/parser.rs#L663)), `dispatch_sgr_colon` ([L1053](file:///home/blmarket/proj/hmux/tiri/tiri-vt/src/parser.rs#L1053)), and `osc_133_exit_status` ([L1446](file:///home/blmarket/proj/hmux/tiri/tiri-vt/src/parser.rs#L1446)).
   - Root re-exports **`INPUT_BUF_START`**, **`INPUT_DISCARD`**, **`INPUT_LAST`**, and **`InputParam`** ([lib.rs](file:///home/blmarket/proj/hmux/tiri/tiri-vt/src/lib.rs#L7-L9)) + methods **`Parser::shrink_buffer`** and **`Parser::clear_params`** ([parser.rs](file:///home/blmarket/proj/hmux/tiri/tiri-vt/src/parser.rs#L376-L387)) also have **0 external callers** and should be private.
2. **Make all 12 fields of `Parser` private ([parser.rs](file:///home/blmarket/proj/hmux/tiri/tiri-vt/src/parser.rs#L321-L335)) — also fixes a latent bug in `tiri-core`**:
   - 11 of `Parser`'s 12 `pub` fields (`interm_buf`, `interm_len`, `param_buf`, `param_len`, `input_buf`, `input_len`, `param_list`, `param_list_len`, `ch`, `flags`, `since_ground`) have **0 accesses outside `tiri-vt`**.
   - The sole external field read is `ictx.vt.input_end` at [tiri-core/src/window_pane/input.rs](file:///home/blmarket/proj/hmux/tiri/tiri-core/src/window_pane/input.rs#L1929) inside `input_add_request`. However, `input_parse` ([input.rs](file:///home/blmarket/proj/hmux/tiri/tiri-core/src/window_pane/input.rs#L266)) swaps `ictx.vt` out with `std::mem::take(&mut ictx.vt)` before calling `vt.parse(&mut host, buf)`, so during `osc_4_query` ([input.rs](file:///home/blmarket/proj/hmux/tiri/tiri-core/src/window_pane/input.rs#L1457-L1467)), `ictx.vt` is a dummy `Parser::default()` whose `input_end` is always `INPUT_END_ST` — ignoring the `end_type` parameter already passed to `VtHost::osc_4_query`! Passing `end_type` into `input_add_request` fixes BEL-terminated OSC 4 queries and leaves **all 12 fields of `Parser` completely private**.
3. **Eliminate `pub trait TerminalHost` — make `parse_terminal_reply` a pure function ([terminal.rs](file:///home/blmarket/proj/hmux/tiri/tiri-vt/src/terminal.rs#L31-L60))**:
   - Currently `parse_terminal_reply(host: &mut dyn TerminalHost, kind, bytes) -> Result<usize, ReplyError>` calls `host.terminal_reply_state()` once at entry, allocates a `CString` for `ExtendedAttributes`, borrows it in `TerminalReport<'a>`, and calls `host.terminal_report(report)` once before returning `Ok(size)`.
   - Changing `TerminalReport` to own `pub version: Option<CString>` (dropping `'a` and redundant `pub kind: TerminalReplyKind`) lets `parse_terminal_reply` become a pure data-in / data-out function:
     ```rust
     pub fn parse_terminal_reply(
         state: TerminalReplyState,
         kind: TerminalReplyKind,
         bytes: &[u8],
     ) -> Result<(usize, TerminalReport), ReplyError>
     ```
   - This deletes `pub trait TerminalHost` from `tiri-vt`, deletes the `TerminalReplyHost` adapter struct in [tiri-core/src/tty/keys.rs](file:///home/blmarket/proj/hmux/tiri/tiri-core/src/tty/keys.rs#L2616-L2661), deletes the mock `Host` in `tiri-vt/tests/terminal.rs`, and avoids cloning `version` a second time in `record_terminal_type`.
4. **Trim `VtHost` & replace thread-local buffer-size state ([parser.rs](file:///home/blmarket/proj/hmux/tiri/tiri-vt/src/parser.rs#L17-L28), [L92-L167](file:///home/blmarket/proj/hmux/tiri/tiri-vt/src/parser.rs#L92-L167))**:
   - **Remove `VtHost::reset_cell(&mut self)` ([L101](file:///home/blmarket/proj/hmux/tiri/tiri-vt/src/parser.rs#L101))**: **0 call sites** anywhere in `tiri-vt` or `tiri-core` (`ScreenVtHost::reset_cell` is dead code).
   - **Remove `queue: bool` from `VtHost::reply(&mut self, queue: bool, bytes: &[u8])` ([L138](file:///home/blmarket/proj/hmux/tiri/tiri-vt/src/parser.rs#L138))**: Called 5 times in `parser.rs`, and `queue` is **`true` at all 5 sites** (`false` is dead code).
   - **Replace `InputEndType = u32` + `INPUT_END_ST` / `INPUT_END_BEL` ([L6-L8](file:///home/blmarket/proj/hmux/tiri/tiri-vt/src/parser.rs#L6-L8)) with `pub enum InputEnd { St, Bel }`**: Matches `SgrOp`, `WinOp`, and `Osc133Command`, removing a C-style integer typedef and 2 constants.
   - **Replace thread-local `INPUT_BUFFER_SIZE` (`buffer_size()` / `set_buffer_size()`) with per-`Parser` configuration or `VtHost::max_buffer_size(&self) -> usize`**: `tiri-vt` currently hides mutable global state in a `thread_local!` (`INPUT_BUFFER_SIZE`), making `Parser` implicitly stateful across tests on the same thread; either a field on `Parser` or a method on `VtHost` (like `allow_set_clipboard(&self)`) eliminates global state from `tiri-vt`.

---

## Prioritized Simplification Roadmap

1. **Zero-Risk Dead Surface Removal (Mechanical)**
   - **`tiri-sys`**: Delete `notification.rs` (`Notification`, `Notify`), 6 unused `Platform` methods + `ProcessInfo`, and `RegexStorage` (-> `CompiledRegex::compile`); make `ensure_single_threaded` & `terminal_session` private.
   - **`tiri-rt`**: Delete `src/channel.rs` and `src/stream.rs`.
   - **`tiri-vt`**: Make `mod parser` & `mod osc7501` private; make all 12 `Parser` fields (passing `end_type` into `input_add_request`), 5 internal `pub fn`s, `InputParam`, `INPUT_BUF_START`/`INPUT_DISCARD`/`INPUT_LAST`, and `Parser::{shrink_buffer, clear_params}` private; remove `VtHost::reset_cell` and `queue: bool` on `VtHost::reply`.
   - **`tiri-refbox`**: Remove `cyclic`/`cyclic_stable` features and the 16 unused public items / macros + `RefBoxHeap` leak.
   - **`tiri-buffer` / `tiri-sdbus` / `tiri-cmds`**: Hide `SegmentedBuf::chunks`, `Message::{append_bool, append_u32}`, `ParseCommand::{empty, new}`, and dead `pub` items in `tiri-cmds`.

2. **High-Clarity Interface Tightening (Small Call-Site Updates)**
   - **`tiri-rt`**: Collapse the 6 single-impl root traits (`Runtime`, `Handle`, `AsyncAccept`, `AsyncRead`, `AsyncWrite`, `Signals`) into inherent methods on `mio` types re-exported at the crate root.
   - **`tiri-vt`**: Replace `TerminalHost` with pure `parse_terminal_reply(state, kind, bytes) -> Result<(usize, TerminalReport), ReplyError>`; turn `InputEndType` into `enum InputEnd { St, Bel }`.
   - **`tiri-cmdparse`**: Replace `TokenText` with `CString`; hide `Token::EqualsEnd`, span tuples `(usize, Token, usize)`, and `lalrpop_util::ParseError`.
   - **`tiri-sys`**: Replace `trait Platform` + `CurrentPlatform` with 4 `platform::*` free functions.
   - **`tiri-buffer`**: Change `make_contiguous` to return `&[u8]`, `append` to take `impl Into<Self>` by value, and add `into_vec(self) -> Vec<u8>`.
   - **`tiri-core` / `tiri-cmds`**: Remove `pub use {libc, refbox, tiri_buffer, tiri_rt, tiri_sys}` from `tiri-core`; remove `Program::as_any`, `ProgramSource`, and duplicate `cmd_parse_error_uppercase_first`; move `tiri-cmds`'s 36 `test_cases` into `tiri-cmds` `#[cfg(test)]` and drop `feature = "test-support"`.

3. **Optional Crate-Boundary Consolidations**
   - Move `tiri-cmds/src/cmd/parse/lexer.rs` into `tiri-cmdparse` so `tiri-cmdparse` encapsulates lexing + grammar behind a single `parse(bytes, mode, ctx)` call.
   - Fold single-caller `tiri-sdbus` into `tiri-core/src/compat/systemd.rs` (or reduce `tiri-sdbus` to a single `start_transient_scope` entrypoint).
   - Inline `tiri::main()` into `src/main.rs` (and optionally move `default_keys.rs` so root `tiri` is binary-only).
