# Plan: structural window close, Drop-only free

Companion to `task-events.md`. Goal: fire `window-closed` from one
deterministic place without tracking the last `Rc<window>` drop, delete the
`window_add_ref` / `window_remove_ref` plumbing, and keep hooks, control
notifications and alerts working unchanged.

## Principle

Separate two things that the current code conflates:

| Concern | Trigger | Needs strong ref? | Observable? |
|---|---|---|---|
| **Close** (event, index removal, pane/timer/option teardown) | last winlink detaches | yes, and the detaching winlink still holds it | yes |
| **Free** (memory) | last `Rc` drops | no | no |

Events, hooks and control notifications only need the window to be intact
*during dispatch*. The caller that fires the event already holds a strong ref,
so the payload's `Rc` clones are borrows in spirit. Nothing needs to own the
window after dispatch. Verified consumers:

- `control_notify.rs` does not subscribe to `window-closed` at all.
- `hooks.rs` snapshots formats (`hook_window` and friends) at insert time and
  keeps a weak `cmd_find_state`; `hooks_insert` already falls back to
  `cmd_find_from_nothing` when the target is dead. The conformance test
  `tests/core/conformance_window_closed_hooks.rs` confirms hook commands target
  the successor window, not the closed one.
- No sink retains a `WindowRef` beyond dispatch (the only long-lived strong
  holders in the tree are `winlink.window_owner`, `alerts_list` and the
  in-flight `event_payload`).

So the "close callbacks may retain the window to postpone destruction" branch
in `window_prepare_release` is dead and can go.

## Current state (what changes)

- `src/window/mod.rs:354-375` fires the event when
  `Rc::strong_count == 1` and the lifecycle is `Live`. Every transient upgrade
  (146 call sites across 52 files) therefore has to go through
  `window_remove_ref`, and a transient holder moves the firing point to
  wherever it happens to release (e.g. inside a format callback).
- Both detach sites already go through `winlink_release_window`
  (`src/winlink.rs:208`): `winlink_remove` and `winlink_set_window`. That is
  the structural close point.
- `alerts_list` (`src/alerts.rs:12`) is the only non-structural long-lived
  strong holder.
- `impl Drop for window` only asserts lifecycle is `Unowned | Destroyed`.

## Target design

```text
winlink_release_window(wl)
  window.remove_winlink(wl)
  if window.winlinks.is_empty():
      window_close(&window)          # single trigger
  wl.window_owner.take()             # plain Rc drop, no semantics

window_close(&WindowRef)
  assert lifecycle == Live
  lifecycle = Closing
  events_fire_window("window-closed", &window)   # still fully intact
  windows_remove(index)                          # id lookups now fail
  window_destroy_panes / timers / options        # today's window_destroy
  lifecycle = Closed

impl Drop for window
  assert lifecycle is Unowned | Closed            # unchanged in spirit
```

Rc clone / drop of `WindowRef` no longer carries lifecycle meaning anywhere.

## Steps

Each step should compile and pass tests on its own.

### 1. Add `window_close`, switch the trigger

- In `src/window/mod.rs`, rename `window_destroy` to the teardown half of
  `window_close`, and replace the `strong_count == 1` condition in
  `window_prepare_release` with `winlinks.is_empty()`.
- Rename lifecycle states to `Live | Closing | Closed` (keep `Unowned`).
- Make `winlink_release_window` call `window_close` when the list empties,
  then drop the field. Keep the field published through the event, as today.
- Keep `window_remove_ref` for now as a thin `drop(owner)` so the tree still
  compiles. Its strong-count logic is gone after this step.
- Run `conformance_window_closed_hooks` and the `window/mod.rs` teardown
  tests. Expected: identical firing points in every path, because at a winlink
  detach with no transient holder `strong_count == 1` and
  `winlinks.is_empty()` are the same condition.

### 2. Make the alerts queue a weak observer

- `alerts_list: VecDeque<WindowWeak>`; `alerts_callback` upgrades and skips
  dead entries. `queue_alerts` / `finish_alerts` keep the `alerts_queued` flag
  as the membership guard.
- Drop the "transfers one release duty to the alert queue" wording in
  `src/window/api.rs:121` and the `strong_count` assertions in
  `src/window/alerts.rs` tests.
- After this, the winlink is literally the sole owner, so free time equals
  close time except for stack-scoped transient upgrades.

### 3. Make event firing borrow

- `events_fire_window(name, &WindowRef)` and `events_fire_pane` / `_winlink`
  stop taking ownership. Callers stop writing `Rc::clone(...)` at the call
  site.
- `event_payload` may keep `Rc` clones internally (simplest, no lifetime
  parameter needed) but `event_payload_free_target` and the
  `event_payload_item` Drop stop calling `window_remove_ref`; plain drops.

### 4. Delete the ref plumbing

- Remove `window_add_ref`, `window_remove_ref`, `window_prepare_release`.
- Mechanically replace the 146 `window_remove_ref(x, c"...")` calls with
  nothing (let the binding fall out of scope) or `drop(x)` where ordering
  matters. Review the `from` strings only where they documented a reason to
  hold the ref past a call that can unlink (see audit below).
- Update the `WindowRef` doc comment in `src/shared/window.rs:4` and the
  `window` struct comment in `src/window/model.rs`.

### 5. Audit: code that can observe a Closed window

New situation: a transient `WindowRef` can now point at a window that is
already closed (no winlinks, no panes, not in the index). Today that cannot
happen because teardown waits for every holder. The lifecycle enum already
models this state; the audit is about *uses after an unlink*. Sites to read:

- `src/cmd/entries/join_pane.rs`, `break_pane.rs`, `swap_pane.rs`,
  `split_window.rs`, `respawn_pane.rs`: hold a source window, move or kill
  panes, then redraw or notify. Make sure nothing touches `panes` or the
  layout after the call that can remove the last winlink.
- `src/window_pane/lifecycle.rs` (`server_kill_pane`, `server_destroy_pane`):
  killing the last pane kills the window; check the tail of each function.
- `src/server_fn.rs` (`server_kill_window`, `server_link_window`,
  `server_unlink_window`): link before unlink for moves is already the tmux
  order; verify.
- `src/session/mod.rs` `session_destroy` loop and `src/session/spawn.rs`
  error path (`winlink_remove` after a failed spawn).
- Iteration helpers `windows.first()` / `next_window()` in `alerts.rs`,
  `server.rs`, `options.rs`, `resize.rs`, `screen_redraw.rs`,
  `server_client/mod.rs`: closing a window mid-iteration must not break the
  cursor. `windows_remove` already tolerates a missing id; confirm `next`
  works from a removed element.

Where a site does use the window after a possible close, either reorder so
the use happens first, or guard on lifecycle.

### 6. Windows that are never linked

Both creators (`src/session/spawn.rs:76`, `src/cmd/entries/break_pane.rs:173`)
link immediately after `window_create`. If a future path can fail between
create and link, it must call `window_close` itself; the Drop assertion keeps
that honest. No change needed now, document it on `window_create`.

## Tests

- Existing: `conformance_window_closed_hooks` (all eight paths), window
  teardown tests, alerts tests, control-notify tests.
- New unit tests in `src/window/mod.rs`:
  - closing fires exactly once even when extra `Rc` clones exist at detach.
  - a transient clone held across the close sees `Closed` and empty panes,
    and dropping it afterwards does not fire again.
  - `window_find_by_id` fails immediately after close.
- New alerts test: an alert queued for a window that closes before the task
  runs is skipped without panicking.

## Non-goals

- Deferring event dispatch through the command queue the way tmux does. hmux
  fires synchronously today and this plan keeps that.
- Touching `src/compat/`.
- Changing session or pane lifecycle. Sessions already close explicitly
  (`session_destroy`) and panes are owned by their window; they are the model
  this plan brings windows in line with.
