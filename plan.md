# Plan: command-queue suspension → `std::future::Future`

Goal: replace `CMDQ_FIRED` / `CMDQ_WAITING`, `CMD_RETURN_WAIT` and
`cmdq_continue` with Future-based suspension. A waiting command owns its
continuation as a future; resuming is a one-shot send; cancellation is
dropping the future.

## Repo facts that shape the plan

- Git toplevel is `/srv/hmux`; `tiri/` is a subtree. Conformance tests live in
  `/srv/hmux/tests/core/` (`queue.rs`, `command_queue.rs`,
  `queue/{run_shell,if_shell,source_file,prompt_substitution,insertion_order,nested_suppression,item_identity,chain_error,command_error_hook,global_queue,bind_time,control_granularity}.rs`,
  `conformance_uncovered_{wait_for_*,run_shell_*,source_file_*,load_buffer_*,save_buffer_*}.rs`,
  `run_shell_bg.rs`, `queue_stdin.rs`). Run from `/srv/hmux`:
  `nix develop --command make test-core TEST_ARGS="-E 'test(/queue/)'"`. Lib
  unit tests need the supervisor:
  `nix develop --command make test-modules MODULE=queue` (or `MODULE=file`).
- `tiri-rt` has `Task` (drop = cancel, `tiri-rt/src/mio/runtime.rs:193-205`),
  `Sleep` (`waits.rs`), LocalWaker-only wakes (`runtime.rs:470-525`),
  `reactor::Timer` / `task_start` / `yield_now`
  (`src/reactor/{timers,tasks,mod}.rs`). **No one-shot/notify primitive exists
  anywhere**; `src/file/stream.rs:18,59-70` and `src/reactor/streams.rs:23`
  store an `Option<LocalWaker>` and wake after releasing borrows. That is the
  local idiom.
- `cmdq_set_cancel_callback` / `cmdq_item.cancel_data`
  (`src/cmd/queue.rs:83,183-190,230-238`) have **no production users**, only
  `lifecycle_tests` at `queue.rs:1228,1248`. `file.rs:127` sets
  `client_file.cancel_data`, a different field, and all production callers pass
  `None` (source_file.rs:179, load_buffer.rs:169, window_pane/mod.rs:1646); the
  only non-None is `completion_tests.rs:18`.
- `tests/cmdq_handle_interfaces.rs` audits that no `*mut cmdq_item` /
  `NonNull<cmdq_item>` appears in any signature/field/static and that no
  `Rc`/`Weak` alias of the item type is introduced. New types below respect
  that.

## 1. Inventory of wait sites

| # | Site (exec → returns WAIT) | Waits for | How it resumes today | State / cleanup it owns | Value on resume | Ordering-sensitive behaviour |
|---|---|---|---|---|---|---|
| 1 | `wait_for.rs:475-508` `cmd_wait_for_wait` | channel signal | `cmd_wait_for_signal` 454-474 → `wait_items_continue_all` 121-126; `-w` wake 432-453; `cmd_wait_for_flush` 555-577 | `wait_channel.waiters: Vec<wait_item{Weak item}>`; `prune_expired` 202-205 drops dead Weaks; channel removed when empty/woken | none | `-l`/`-w` resolve the waiter's **client name** by upgrading the item (180-194) |
| 2 | `wait_for.rs:509-530` `cmd_wait_for_lock` | unlock | `cmd_wait_for_unlock` 531-554 continues first locker **and keeps `locked=1`** (lock handoff); `-w`; flush (sets `locked=0`) | `wait_channel.lockers` | none | FIFO handoff |
| 3 | `wait_for.rs:322-366` `cmd_wait_for_event` (`-E`) | hook event via `events_add_sink` | sink cb 261-312: verbose-print payload with `cmdq_print(item)`, evaluate `-F` filter with `format_create_with_client(client, Some(item))`, then continue; `-Ew` 381-411; flush | `wait_event_items: Vec<wait_event_item>`; `Drop` removes the sink (313-317) | none | printing + filter need the **item** (for formats) while still waiting; filter reject keeps waiting (multi-shot) |
| 4 | `confirm_before.rs:63-144` | prompt answer | prompt input cb 145-186: on confirm `cmdq_get_command(cmdlist, item.state)` + `cmdq_insert_after(item)` (165-173); sets `client.set_return_value(retcode)` if no session; `cmdq_continue` | `cmd_confirm_before_data` owned by the prompt closure; freed by `clear_prompt` (`server_client/api.rs:1136-1150`, which calls `prompt_free`, **not** the input cb) | confirmed/not (retcode 0/1) | insert_after must precede continue. Client lost: `status_prompt_clear` (`server_client/mod.rs:1208`) runs **before** `cmdq_abort_owned_wait` (1210) |
| 5 | `command_prompt.rs:111-261` | prompt input (possibly multi-prompt) | cb 262-361: per-prompt `PROMPT_CONTINUE`; final: `args_make_commands` → `cmdq_insert_after(item)` (343-347) or `cmdq_append(c, cmdq_get_error)` (331-335); `cdata.item = Weak::new(); cmdq_continue` 356-359; `Drop` 370-380 also continues if still set | `cmd_command_prompt_cdata` (argv, state, prompts) | argv / none | `-i` forces `wait=0`. Error path appends to queue **end**, not after item |
| 6 | `run_shell.rs:146-275` (fg) | delay timer → job exit | `Timer` stored via `cmdq_set_wait_timer` 273; `cmd_run_shell_timer` 276-389 clears timer, `job_run` with `completecb` owning cdata → `cmd_run_shell_callback` 390-444 prints lines (`cmdq_print(item)` if no pane), sets return value, continue; `-C`: `args_make_commands` → `cmdq_insert_after(item)` 379-382 → continue; errors → `cmdq_error` → continue | `cmd_run_shell_data` (client, session refs released in `Drop` 445-457); zero-delay timer defers one runtime turn | exit code / signal | Expired item ⇒ callback returns early (392-394, test 464) but the job keeps running |
| 7 | `if_shell.rs:72-184` (fg) | job exit | `completecb` 175-179 → `cmd_if_shell_callback` 185-243: pick if/else state → `cmdq_insert_after(item)` 232-235 or `cmdq_error`; continue | `cmd_if_shell_data` (client ref) | success/fail | **Not cancellable** by `cmdq_abort_owned_wait` (`queue.rs:286-288`) |
| 8 | `source_file.rs:250-381` | one `file_read_with_cmdq_wait` per path, chained | terminal file cb (98-118) → `cmd_source_file_done` 183-231: `load_cfg_from_buffer(.., after, .., &mut new_item)` inserts parsed commands after `after` (`cfg.rs:244-258`); next file → `cmd_source_file_read` 168-181; last file → `cmd_source_file_complete` 141-166 inserts completion callback after `after`; `cmdq_continue` 228 | `cmd_source_file_data`: depth counter, client ref, path list; `after: Weak item` | parse status → `set_return_value(1)` only | `after` chaining order; completion cb after all inserted commands; depth decrement must happen on cancel |
| 9 | `load_buffer.rs:129-172` | `file_read_with_cmdq_wait` | `cmd_load_buffer_done` 70-128 on `closed`: paste_set / tty selection / `cmdq_error` → release client → continue | `cmd_load_buffer_data` (target client ref) | error | progress events ignored |
| 10 | `save_buffer.rs:89-178` | `file_write_with_cmdq_wait` | `cmd_save_buffer_done` 71-88 on `closed`: `cmdq_error` → continue | none beyond the file | error | — |
| 11 | `display_message.rs:83-91` `-I` → `window_pane_start_input` (`window_pane/mod.rs:1609-1648`) | client stdin stream (`file_read_with_cmdq_wait_init(c, "-")`) | `window_pane_input_callback` 1567-1607: data → `input_parse_buffer`; `closed`/error → continue; dead client/pane gone → `file_cancel` | `window_pane_input_data` (client ref, pane id, file Weak) | none | needs the file Weak before dispatch |
| 12 | `split_window.rs:440-449`: `-I` (as 11), `-W` → `wait_until_close` (`window_pane/api.rs:811`) | pane process exit | `window_pane_wait_finish` (`mod.rs:628-644`) from `lifecycle.rs:247` (exit) or `mod.rs:710` (destroy): sets return value, continue | `window_pane.wait_item: Weak item`; `destroy_ready` (`mod.rs:360`) delays destruction while a waiter is live | exit code (+128 for signal) | **Not cancellable** today |
| 13 | `display_menu.rs:1185-1206` → `popup_display(.., Some(item), ..)` (`popup.rs:944,1035`) | popup close | `impl Drop for popup_data` (`popup.rs:203-233`): if `published`, set return value = `status`, continue 219 | `popup_data.item: Weak`; overlay freed by `server_client_clear_overlay` (`mod.rs:1207`) | popup status | continue happens inside a `Drop`; startup failure returns ERROR synchronously |
| 14 | `cfg.rs:57-62` `cfg_client_done` **callback item** | config load finished | `cfg_done` 63-73 continues `cfg_item` (static Weak, 40) | static | none | callback-type item |
| 15 | `file.rs` glue: `file_set_cmdq_wait` 116-129, `file_cancel_cmdq_wait` 133-149, `file_fire_done_cb` 207-262, `file_fire_done` 264-300 (local reads: completion `Task` stored via `cmdq_set_file_task` 294), `client_files_interrupt` 1356-1364 | — | — | `client_file.{wait_item,wait_active,wait_client,cancel_data}` (`file/model.rs:32-36`), item `{wait_file,file_task}` | — | local completion deliberately deferred (`completion_tests.rs:33`) |
| 16 | `completion_tests.rs:8-69` | asserts `CMDQ_WAITING` after `cmdq_next`; cancel after runtime shutdown; `cmdq_continue` + `cmdq_next` removes | — | — | — | must be rewritten |

Other consumers of the flags:

- `cmdq_running` (`queue.rs:1087-1098`, used by `hooks.rs:313,420`): must
  return nothing while an item is suspended.
- `cmdq_remove` (`queue.rs:647-678`): asserts `wait_file` empty, runs
  `cancel_data` if `!FIRED`.
- `cmdq_abort_owned_wait` (`274-294`, called from `server_client_lost` at
  `server_client/mod.rs:1210`): returns **without draining** unless the head is
  `FIRED|WAITING` with a file/timer wait.
- `server_loop` (`server.rs:299-313`): level-triggered re-run of `cmdq_next`
  while `items != 0`.

## 2. Target design

### 2.1 Resume primitive: `tiri_rt::oneshot` (new `tiri-rt/src/oneshot.rs`)

```rust
pub fn channel<T>() -> (Sender<T>, Receiver<T>);
impl<T> Sender<T> { pub fn send(self, value: T) -> Result<(), T>; pub fn is_closed(&self) -> bool; }
impl<T> Future for Receiver<T> { type Output = Option<T>; }   // None = sender dropped unsent
```

Implementation: `Rc<Slot<T>>` with `value: Cell<Option<T>>`,
`waker: RefCell<Option<LocalWaker>>`, `closed: Cell<bool>`. `send` takes the
waker out before calling `wake()` (same discipline as `stream.rs:59-70`).
Dropping `Sender` marks closed and wakes; dropping `Receiver` marks closed so
`is_closed()` lets registries prune.

Why this is the one `Rc` the plan introduces: the waiter (job, prompt, popup,
pane, channel) and the item have independent lifetimes, so a signal needs a
location both can reach. Today that location is the whole `cmdq_item`, reached
through `Weak<UnsafeCell<cmdq_item>>`. That is shared, mutable, and exposes the
entire item to every waiter. The one-shot replaces it with a single-use, typed,
two-endpoint cell, and waiters stop touching queue items entirely.
`LocalWaker` is itself `Rc<dyn LocalWake>` and already in use, so this adds no
new ownership category. The no-Rc alternative was rejected: each waiter would
grow a `wake: Option<LocalWaker>` slot plus a value slot the future polls, as
`ReadState` does. That duplicates the same state machine in 8 owners
(`wait_channel`, prompt data, `job`, `popup_data`, `window_pane`,
`client_file`, cfg) and the future would need more `Weak`s to reach them.

### 2.2 Queue item state (`src/cmd/queue.rs`)

Replace `flags`, `type_0`, `cb`, `cancel_data`, `wait_timer`, `file_task` and
`wait_file` with one field:

```rust
enum Work {
    Command,                                           // dispatch entry.exec
    Callback(Box<dyn FnOnce(&Rc<UnsafeCell<cmdq_item>>) -> cmd_retval>),
    Executing,                                         // inside exec/callback; cmdq_suspend may install
    Suspended(Pin<Box<dyn Future<Output = ()>>>),
    Polling,                                           // future moved out for poll
    Fired,                                             // returned/ready; removal pending
}
```

`cmdq_type`, `CMDQ_COMMAND` / `CMDQ_CALLBACK` and `CMDQ_FIRED` /
`CMDQ_WAITING` (`shared/command.rs:115-116`) are deleted. `cmd` and `cmdlist`
stay, because hooks' `cmdq_merge_formats` reads `command_handle()` of a
suspended item.

Suspension API. This is the single way to suspend, and it replaces
`CMD_RETURN_WAIT` + `cmdq_continue`:

```rust
/// Install the item's continuation. Must be the value returned from exec/callback.
#[must_use]
pub unsafe fn cmdq_suspend(
    item: &Rc<UnsafeCell<cmdq_item>>,
    future: impl Future<Output = ()> + 'static,
) -> cmd_retval
```

It asserts `Work::Executing`, stores `Suspended(Box::pin(future))`, and returns
a **module-private** sentinel `const SUSPENDED: cmd_retval = 1`.
`CMD_RETURN_WAIT` is deleted from `shared/command.rs:14`, and the ABI test line
`command.rs:134` goes with it. `cmdq_next` panics if
`(retval == SUSPENDED) != matches!(work, Suspended)`. A command can therefore
never install a future and return something else, or return the sentinel
without installing one. `cmd_exec_fn` (`command.rs:265-267`) and `cmdq_cb`
(`286-287`) keep returning `cmd_retval`, so the 48 non-waiting entries are
untouched. Open question 2 covers the typed-enum alternative.

The future's output is `()` rather than `cmd_retval`. tmux ignores the
post-wait status: `cmdq_continue` carries none, and `%end`/`%error` guards and
`cmdq_remove_group` are decided on the synchronous return in
`cmdq_fire_command` / `cmdq_next:1067-1069`. Returning a value would invite
behaviour tmux does not have (open question 1).

Item access from a future: capture `Weak<UnsafeCell<cmdq_item>>` and upgrade
**inside poll only**, via a helper:

```rust
/// Only valid while the queue polls this item; drop before returning Pending.
pub(crate) unsafe fn cmdq_resumed(item: &Weak<UnsafeCell<cmdq_item>>) -> Rc<UnsafeCell<cmdq_item>>
```

`cmdq_next` asserts `Rc::strong_count(&head) == 2` (queue + local) after every
poll. A future that retains the item across an await is caught the moment it
happens, which keeps the `cmdq_remove` `strong_count == 1` invariant
(`queue.rs:648-652`). Futures that only need the client capture `ClientWeak`
(`item.client`) and need no item at all: #1, #2, #12, #13, #14 and pane input.

### 2.3 Polling (`cmdq_next`, `queue.rs:1027-1085`)

```
loop {
  head = first()?  (Rc clone, outside with_queue)
  match take(head.work) {
    Command/Callback => { work = Executing; queue.item = weak(head);
                          retval = fire_command / cb(&head); queue.item = Weak::new();
                          assert !removed;
                          if retval == ERROR && was_command { cmdq_remove_group }
                          verify sentinel <-> Suspended; if Suspended { fallthrough to poll } else { work = Fired; items += 1 } }
    Suspended(fut)   => { work = Polling; cx = ContextBuilder::from_waker(noop).local_waker(&queue.wake);
                          match fut.poll(cx) { Pending => { work = Suspended(fut); if queue.wake.take_pending() { items += 1 }; return items }
                                               Ready(()) => { work = Fired; drop(fut); items += 1 } }
                          assert strong_count(&head) == 2 }
  }
  drop(head); pop_front → cmdq_remove
}
```

- **Same-pass poll.** A freshly suspended item is polled in the same pass, so a
  wait that completed synchronously resolves immediately. This replaces the
  "arm `CMDQ_WAITING` before dispatch" trick at `1054-1056`.
- **`cmdq_running()` is `None` during polls.** `queue.item` is set only during
  synchronous dispatch, so `cmdq_running()` (`1087-1098`) returns `None` while
  resume code runs. That matches tmux, where callbacks run outside `cmdq_next`
  and `cmdq_running` is NULL for a waiting item. It matters for
  `hooks.rs:420` (monitor hooks insert after the running item) and
  `hooks.rs:313` (NOHOOKS check).
- **Waker.** `cmdq_list` gains `wake: Rc<QueueWake>`, where
  `QueueWake(Cell<bool>) impl LocalWake` only sets the flag. It must not touch
  the runtime, because it has to work after `shutdown_runtime`
  (`completion_tests.rs:55`). v1 is level-triggered and always polls the head
  future. A wake raised *during* a poll (cross-queue `wait-for -S`,
  `yield_now`) counts as progress, so `server_loop` (`server.rs:302-313`)
  re-enters instead of blocking in `runtime.poll(None)` (`proc.rs:311`). v2
  (optional step 11) skips the poll when the flag is clear.
- **No queue borrow while polling.** Polling and dropping futures happen outside
  `with_queue` (`queue.rs:311-313` contract), so resume code may call
  `cmdq_insert_after`, `cmdq_append` and `cmdq_error` freely.
- **Re-entrancy guard.** `cmdq_list.abort_pending: bool`. When the head is
  `Executing|Polling`, `cmdq_abort` sets the flag instead of draining, and
  `cmdq_next` drains after the step. `server_client_lost` is reached from the
  peer-disconnect message (`server_client/mod.rs:3544`), the exit timer
  (`3148`) and `ClientRef::lost()` (`api.rs:1480`). All of these are runtime
  callbacks today, but the guard makes the invariant explicit.

### 2.4 Cancellation = drop the future

`cmdq_remove` drops `Work` **first**, before `client_owner.release()` at
`queue.rs:662-664`, so future destructors can still see the client.
`cmdq_abort_owned_wait` (`274-294`) becomes `cmdq_abort(owner)`: it drains
unconditionally, with no wait-family special cases.

What each future owns, so that dropping it cancels the wait:

| Wait | Owned by the future | Drop effect |
|---|---|---|
| timer | `tiri_rt::mio::Sleep` | timer entry removed (`waits.rs` Drop) |
| job | `oneshot::Receiver<JobCompletion>`; job's `completecb` = `move \|c\| { let _ = tx.send(c); }` | job keeps running (as today, `run_shell.rs:392-394`); its send is a no-op |
| prompt | `Receiver<...>`; sender inside the prompt's data box | prompt stays open until `clear_prompt`; its later send is a no-op |
| popup | `Receiver<i32>`; sender in `popup_data` | same |
| pane close | `Receiver<i32>`; sender in `window_pane.wait_close` | `destroy_ready` checks `is_closed()` |
| wait-for channel | `Receiver<()>`; `Sender` in `waiters/lockers` | `prune_expired` uses `Sender::is_closed()` |
| file | `FileWait { done: Receiver<FileOutcome>, file: Weak<UnsafeCell<client_file>> }` | Drop → `file_cancel_wait(file)` (today's `file_cancel_cmdq_wait` body) |

### 2.5 File wait API (`src/file.rs`, `src/file/model.rs`)

These replace `file_read_with_cmdq_wait`, `file_read_with_cmdq_wait_init`,
`file_write_with_cmdq_wait`, `FileCmdqWait`, `file_set_cmdq_wait` and
`file_cancel_cmdq_wait`:

```rust
pub struct FileOutcome { pub path: Option<CString>, pub error: i32, pub buffer: SegmentedBuf }
pub struct FileWait { .. }  impl Future for FileWait { type Output = Option<FileOutcome>; }  // None = cancelled by the file (dead client)
pub(crate) unsafe fn file_read_wait(client, path, progress: impl FnOnce(Weak<UnsafeCell<client_file>>) -> client_file_cb) -> FileWait
pub(crate) unsafe fn file_write_wait(client, path, flags, data: &[u8]) -> FileWait
```

- **`client_file` fields.**
  `wait_item/wait_active/wait_client/cancel_data` become
  `wait: Option<FileWaiter { done: Sender<FileOutcome>, client: ClientWeak }>`.
- **Terminal event.** In `file_fire_done_cb` (207-262), when a waiter is
  present, the terminal event is delivered by `send(FileOutcome)` instead of
  `cb(closed=true)`. The `dead` branch drops the sender unsent.
- **`client_file_cb`** becomes progress-only for waiting reads (pane input).
- **The local-read completion `Task` is deleted** (`file_fire_done:286-295`,
  `cmdq_set_file_task`). The outcome is sent synchronously into the one-shot,
  and the command observes it on its immediate poll after exec has returned.
  The deferral existed only because the callback used to run inside exec.
- **Unchanged:** the `done_task` path for indexed streams (`296-299`).
- **`client_file.cancel_data` is deleted** (test-only).

### 2.6 Deleted at the end of the series

- **Return code and flags:** `CMD_RETURN_WAIT`, `CMDQ_FIRED`, `CMDQ_WAITING`,
  `cmdq_type`, `CMDQ_COMMAND`, `CMDQ_CALLBACK`.
- **Queue functions:** `cmdq_continue`, `cmdq_set_cancel_callback`,
  `cmdq_set_wait_timer`, `cmdq_clear_wait_timer`, `cmdq_set_wait_file`,
  `cmdq_set_file_task`, `cmdq_clear_wait_file`, `cmdq_cancel_unfired_data`.
  `cmdq_abort_owned_wait` is replaced by the uniform `cmdq_abort`, and its
  file/timer branches go.
- **`cmdq_item` fields:** the seven fields replaced by `Work`.
- **Item back-references held by waiters:** `wait_item.item`, `cfg_item`,
  `popup_data.item`, `window_pane.wait_item`, `window_pane_input_data.item`,
  and `cmd_*_data.item` in run_shell, if_shell, confirm_before,
  command_prompt, source_file and load_buffer. Futures capture a `Weak` only
  where resume code needs `cmdq_*` on the item.
- **File waits:** all `file_*_with_cmdq_wait*`, `file_set_cmdq_wait`,
  `file_cancel_cmdq_wait`, and
  `client_file.{wait_item,wait_active,wait_client,cancel_data}`.

## 3. Ordered migration steps

Each step compiles and is verified on its own. Steps 1→9 use a **bridge**
inside `queue.rs`: a legacy `CMD_RETURN_WAIT` return is translated to
`Suspended(Receiver<()>)`, with the `Sender` kept in the item, and
`cmdq_continue` becomes `sender.take().send(())`. This overlap is deliberate.
It exists only between commits 1 and 10, and the series must land together.
The alternative is one atomic commit touching queue.rs, file.rs and 13 entry
files (open question 3).

**Step 0: `tiri_rt::oneshot`.**
- Add `tiri-rt/src/oneshot.rs` and `pub mod oneshot` in `tiri-rt/src/lib.rs`.
- Unit tests:
  - send before poll;
  - poll then send wakes the registered LocalWaker, after the borrow is
    released;
  - sender drop gives `None`;
  - receiver drop makes `is_closed()` true.
- Verify: `cargo test -p tiri-rt`.

**Step 1: queue core + bridge.**
- In `src/cmd/queue.rs`, introduce `Work`, `QueueWake`, `cmdq_suspend`,
  `cmdq_resumed` and polling in `cmdq_next` (§2.3).
- `cmdq_running` uses `queue.item`, which is set only during synchronous
  dispatch.
- Add `cmdq_abort` with `abort_pending` and unconditional drain. Keep the
  timer/file pre-drop special cases from `274-294` until steps 3 and 9 fold
  them into futures.
- Keep `CMD_RETURN_WAIT` and `cmdq_continue` as the bridge.
- Delete `cancel_data`, `cmdq_set_cancel_callback` and
  `cmdq_cancel_unfired_data` now (no users). Rewrite `lifecycle_tests`
  (`1214-1262`) to assert cancellation through a `Suspended` future's Drop
  guard instead.
- Adjust `completion_tests.rs:34`: replace the `CMDQ_WAITING` assert with
  "`cmdq_next` returned 0 and the item is still queued". The constant itself
  goes in step 10, but the test should stop reading flags here.
- Update `server_loop` if `cmdq_next`'s progress accounting changes (its
  signature does not).
- Verify:
  - `cargo build`;
  - `make test-modules MODULE=queue`;
  - `make test-core TEST_ARGS="-E 'test(/queue/) | test(/wait_for/) | test(/run_shell/) | test(/source_file/)'"`;
  - `tests/cmdq_handle_interfaces.rs`.

**Step 2: wait-for.**
- `shared/command.rs:291-295`: `wait_item` becomes
  `{ client: ClientWeak, wake: oneshot::Sender<()> }`.
- `wait_event_item` keeps a `Weak` item **only** for `cmdq_print` and format
  context, and gains `wake: Sender<()>`.
- `cmd_wait_for_wait/lock/event` return
  `cmdq_suspend(item, async move { let _ = rx.await; })`.
- `continue_waiter` becomes `send(())`; `prune_expired` uses `!is_closed()`;
  `_client_name` reads from `ClientWeak`.
- Delete the `cmdq_continue` imports here.
- Verify: `TEST_ARGS="-E 'test(/wait_for/)'"` and
  `queue.rs::*does_not_block_other_clients`.

**Step 3: run-shell (foreground).**
- Split `cmd_run_shell_timer` and `cmd_run_shell_callback` into sync bodies
  parameterised by `Option<&Rc<item>>`.
- The `-b` path keeps `cmdq_background` + job `completecb` (timing unchanged).
- Foreground returns
  `cmdq_suspend(item, async move { if delay > 0 { Sleep::new(deadline).await }; start job; if let Some(c) = rx.await { body(Some(&cmdq_resumed(&weak)), c) } })`.
- Delete `cmdq_set_wait_timer`, `cmdq_clear_wait_timer`, `item.wait_timer`,
  the timer branch of `cmdq_abort`, and `cmd_run_shell_data.item`.
- Verify: `TEST_ARGS="-E 'test(/run_shell/)'"`, `run_shell_bg.rs`,
  `coverage_run_shell_failures_*`, `conformance_uncovered_run_shell_delay_*`.

**Step 4: if-shell (foreground).**
- Same shape as step 3 (job one-shot); `-b` keeps the callback.
- Delete `cmd_if_shell_data.item`.
- Verify: `queue/if_shell.rs`, `conformance_uncovered_if_shell_*`.

**Step 5: prompts.**
- `confirm_before`: the prompt cb sends `bool` (confirmed) and returns
  `PROMPT_CLOSE`. The future does
  `match rx.await { Some(true) => insert_after(resumed, cmdq_get_command(cmdlist, state)); retcode 0, _ => retcode 1 }`,
  then `set_return_value` if there is no session.
- `command_prompt`: the cb keeps its per-prompt `PROMPT_CONTINUE` logic and
  sends `Option<Vec<CString>>` (the final argv) on close. The future does
  `args_make_commands` + `insert_after`, or `cmdq_append(cmdq_get_error)`.
  Delete the `Drop` continue (`370-380`), since sender drop → `None` covers
  it, and delete `cdata.item`.
- `-b` and `-i` paths are unchanged.
- Verify: `queue/prompt_substitution.rs`, `tests/prompt_*`,
  `mode_prompt_cleanup.rs`; manually run `confirm-before`, then detach the
  client.

**Step 6: popup / display-menu.**
- `popup_display` takes `done: Option<oneshot::Sender<i32>>` instead of
  `Option<&Rc<item>>` (`popup.rs:944,1035`).
- `Drop` sends `status` when `published` (`203-233`).
- `display_menu.rs:1185-1206`: create the pair and pass the sender. On success,
  `cmdq_suspend(item, async move { if let Some(status) = rx.await { set_return_value on ClientWeak if no session } })`.
  On `-1`, drop the receiver and return ERROR as today.
- Verify: `make test-overlay` (popup suite); `display-popup -E` from a detached
  control client returns the exit status.

**Step 7: pane wait (`split-window -W`).**
- `window_pane.wait_item: Weak` becomes `wait_close: Option<Sender<i32>>`.
- `wait_until_close(&self) -> Receiver<i32>` (`api.rs:141,811`).
- `window_pane_wait_finish` (`mod.rs:628-644`) computes retval and `send`s it.
- `destroy_ready` (`360`) becomes
  `wait_close.as_ref().is_some_and(|s| !s.is_closed())`.
- `split_window.rs:445-447` suspends on the receiver.
- Verify: conformance tests for the `split-window -W` exit status; detach while
  waiting. The pane now becomes destroyable because the receiver is dropped;
  this is a behavioural fix (risk 9).

**Step 8: cfg.**
- `cfg_item` (`cfg.rs:40`) becomes `static mut CFG_WAITER: Option<Sender<()>>`.
- `cfg_client_done` becomes `cmdq_suspend(item, async { let _ = rx.await; })`.
- `cfg_done` sends.
- Verify: `config_parse_*`, and startup with a config that `source-file`s.

**Step 9: file waits (atomic for all file users).**
- Implement §2.5 in `file.rs` and `model.rs`.
- Migrate `source_file`: loop over the files inside one future —
  `let out = file_read_wait(..).await; let item = cmdq_resumed(&weak); load_cfg_from_buffer(.., Some(&after), ..)`.
  Insert the completion item. The depth decrement stays in the `Drop` of the
  data, which the future now owns.
- Migrate `load_buffer` and `save_buffer`.
- `window_pane_start_input` becomes `Result<Option<FileWait>, CString>` with a
  progress-only callback (`display_message.rs:83-91`,
  `split_window.rs:440-443`).
- Delete `cmdq_set_wait_file`, `cmdq_set_file_task`, `cmdq_clear_wait_file`,
  `item.wait_file`, `item.file_task`, and the file branch in `cmdq_abort`
  (which is now uniform). `client_files_interrupt` stays; it only sets the
  error and fires done.
- Rewrite `completion_tests.rs`:
  - the local read outcome is observed by the first poll;
  - cancellation after `shutdown_runtime` means dropping the item: the future's
    Drop cancels the file wait, the captured data is released, and
    `is_closed()` is true.
- Verify:
  - `make test-modules MODULE=file`;
  - `TEST_ARGS="-E 'test(/source_file/) | test(/load_buffer/) | test(/save_buffer/) | test(/queue_stdin/)'"`;
  - `client_file_protocol.rs`, `cmd_file_owner.rs`;
  - `queue.rs::{fifo_source_file,nested_buffer_file_io,save_buffer_file_io}_does_not_block_other_clients`.

**Step 10: remove the bridge.**
- Delete `cmdq_continue`, the legacy sender slot, `CMD_RETURN_WAIT` (the
  sentinel becomes private), `CMDQ_FIRED`, `CMDQ_WAITING`, the `cmdq_type`
  constants, and test line `command.rs:134`.
- This grep must come back empty outside `src/compat/`:
  `grep -rn "CMD_RETURN_WAIT\|cmdq_continue\|CMDQ_WAITING\|CMDQ_FIRED\|cmdq_set_wait\|cmdq_set_file_task\|cmdq_set_cancel_callback\|_with_cmdq_wait"`.
- Verify: full `make test` (core, hmux, misc, overlay, leak).

**Step 11 (optional): edge-triggered polling.**
- `cmdq_next` skips polling a `Suspended` head when `queue.wake` is clear.
- `server_loop` loops while `items != 0 || any queue wake pending`.
- Verify with the same suites, plus a soak test (`wait-for` with many idle
  clients) to confirm no wakeups are lost.

## 4. Risks / semantic differences vs tmux

1. **Resume code runs in `cmdq_next` (poll) instead of the event callback.**
   Queue placement is unchanged, because `insert_after(item)` is positional.
   But `cmdq_running()` must be `None` during a poll (tmux: NULL while
   waiting), or monitor hooks (`hooks.rs:420`) would attach after the resumed
   item instead of going to the global queue.
   Test: `queue/nested_suppression.rs`, `conformance_window_closed_hooks.rs`,
   `conformance_uncovered_last_pane_hook_order_*`, plus a new core test: a
   foreground `run-shell` whose output enters view mode while a
   `pane-mode-changed` hook is set.
2. **Timing shifts.** Foreground `run-shell` without `-d` starts its job on the
   immediate poll instead of one runtime turn later. Local `source-file` and
   `load-buffer` outcomes are observed on the immediate poll instead of through
   a deferred `Task`. Ordering relative to other events in the same turn can
   therefore differ.
   Test: `queue/run_shell.rs`, `queue/source_file.rs`,
   `queue/insertion_order.rs`, `conformance_uncovered_source_file_*`,
   `source_file_pattern.rs`.
   If a divergence shows up, add `reactor::yield_now().await` at the top of the
   run-shell future; the wake-during-poll accounting in §2.3 makes it re-poll
   in the same `server_loop` pass.
3. **A lost client now drains its queue unconditionally.** Today
   `cmdq_abort_owned_wait` returns without draining when the head is not
   `FIRED|WAITING`, for example after `status_prompt_clear` or `clear_overlay`
   already continued it (`mod.rs:1207-1208`). Items and their strong
   `client_owner` are then left in a dead client's queue; tmux has the same
   leak. With the drain, cleanup that previously never ran now runs when the
   client is lost:
   - the `cmd_run_shell_data` drop releases its session and client;
   - `source_file` decrements its depth;
   - `load_buffer` releases its client.

   Test: detach or kill a client while `wait-for`, `if-shell`,
   `confirm-before`, `display-popup` or `split-window -W` is pending; run
   `make test-leak`; check the server stays responsive (in the style of
   `queue.rs::assert_responsive`).
4. **Global-queue item whose wait client dies** (e.g. a hook-run `load-buffer`
   from a control client that then disconnects). Today `file_fire_done_cb`'s
   `dead` branch cancels the wait and the item hangs forever, blocking the
   global queue. Now `FileWait` resolves to `None` and the command returns.
   Test: a new core test where a control client issues a hook that loads a
   FIFO, then disconnects.
5. **Prompt freed without an answer** (client lost, or `clear_prompt` from
   another command). Through the sender drop, `confirm-before` now resolves as
   "not confirmed" (retcode 1); tmux's `cmd_confirm_before_free` does not
   continue at all. The difference is only observable if the item is not
   drained, and step 1 rules that out for lost clients. For an in-session
   `clear_prompt` from another command:
   - test: `queue/prompt_substitution.rs`;
   - manual check: start a `command-prompt`, then replace it with a prompt
     triggered by `display-message`.
6. **ABI test.** `command.rs:127-143` loses `CMD_RETURN_WAIT`; nothing in
   `src/compat/` references it (grep confirmed).
7. **Strong-count invariant.** A future that keeps an `Rc<item>` across an
   await trips the new post-poll assert immediately, instead of failing later
   in `cmdq_remove`. Running all queue tests in CI covers this.
8. **`wait-for -E`** keeps a `Weak` item for `cmdq_print` and format
   evaluation inside the sink callback, as today; resume goes through the
   sender.
   Test: `conformance_uncovered_wait_for_*`, and hook event waits whose `-F`
   filter rejects and then accepts.
9. **`split-window -W` pane destruction.** `destroy_ready` used to keep the
   pane alive while any `Weak` item could be upgraded. With `is_closed()`, a
   drained waiter releases the pane.
   Test: detach during `-W`, then `list-panes`.

## 5. Open questions

1. **Future output.** `()` (recommended; mirrors tmux ignoring the post-wait
   status), or `cmd_retval`, where `ERROR` after resume triggers
   `cmdq_remove_group` and the `command-error` hook (new behaviour).
2. **Suspension signature.** `cmdq_suspend` returning a private sentinel
   through the unchanged `cmd_exec_fn` / `cmdq_cb` (recommended: 61 entries
   untouched, and a runtime assert closes the mismatch hole). The alternative
   is a typed `enum cmd_result { Done(cmd_retval), Wait(Pin<Box<..>>) }`
   return type, which touches 102 signatures, 373 `CMD_RETURN_*` sites,
   4 casts in `queue.rs` and the ABI test.
3. **Commit shape.** Bridge series (steps 1–10, with overlap between commits)
   or one atomic commit.
4. **Client-lost semantics.** Is an unconditional drain when a client is lost
   (tiri-specific; tmux leaks instead) the intended behaviour? And is the
   `abort_pending` deferral acceptable, or should we assert that a client is
   never lost mid-dispatch?
5. **Dead wait client on a live queue.** Resolve as cancelled and continue
   (recommended), or keep today's hang.
6. **Home of the one-shot.** `tiri-rt` (recommended; a generic LocalWaker
   primitive) or `src/reactor/oneshot.rs`.
7. **run-shell deferral.** Keep tmux's one-turn deferral for foreground
   `run-shell` via `yield_now` (risk 2), or accept the earlier start.
8. **Edge-triggered polling.** Do step 11 now or defer it.
9. **`wait-for -E` item access.** Keep the read-only `Weak` item in
   `wait_event_item` for printing and formatting, or route payload printing
   through the future (which needs a multi-shot channel).

## Critical files

- `src/cmd/queue.rs`
- `src/file.rs` (with `src/file/model.rs` and `src/file/completion_tests.rs`)
- `src/shared/command.rs`
- `tiri-rt/src/oneshot.rs` (new) and `tiri-rt/src/lib.rs`
- `src/cmd/entries/run_shell.rs`: the template for the job/timer futures;
  the same shape is reused by if_shell, source_file, confirm_before and
  command_prompt.
