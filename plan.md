# Plan: command-queue suspension → `std::future::Future`

Refreshed at `8ada83037` (branch `h2` == master). This replaces the version
written at `50d82145e`. That version was implemented once, on branch `h1`
(`git -C ../h1 log 0d1d423bc..h1`, 12 commits). The h1 series regressed and is
being redone here in small increments.

**Goal.** Replace the following with one Future-based suspension:

- `CMDQ_FIRED` and `CMDQ_WAITING`;
- `CMD_RETURN_WAIT`;
- `cmdq_continue`;
- the item's side fields `wait_timer`, `file_task` and `wait_file`.

How it works:

- A waiting command installs its continuation as a future, and the queue item
  owns it.
- Whatever ends the wait sends on a `tiri_rt::oneshot`.
- Dropping the item, or its client's queue, drops the future, which cancels the
  wait.

**Yardstick.** Behaviour must stay identical to tmux 3.8, as pinned by
`tests/core`, and especially by the `conformance_future_*` files that
`0d1d423bc` added.

## 0. What changed since the old plan

### Master, `50d82145e..8ada83037`

- **`016f6aad6`** deleted `cmdq_item.cancel_data`, `cmdq_set_cancel_callback`
  and `cmdq_cancel_unfired_data`.
- **`3e2c375fd`** added `impl Drop for cmdq_item` (`queue.rs:86-98`).
  - The Drop cancels the file wait (`file_cancel_cmdq_wait`) and the
    `wait_timer`, then releases `cmdlist`, `state` and `cb`.
  - Gone with it: the `removed` flag, `cmdq_list`'s "queue not empty"
    `fatalx`, and the "dropped without cmdq_remove" assert.
  - `cmdq_remove` (`579-591`) is now just a strong-count assert plus a drop.
- **`26ee86676`** deleted `cmdq_item.client_owner`, so queued items no longer
  keep their client alive.
  - `cmdq_fire_command` treats a freed client like a dead one
    (`queue.rs:754-759`).
  - Waiters upgrade their `Weak` item or skip:
    - `confirm_before.rs:151-155`;
    - `command_prompt.rs:270-271`;
    - `save_buffer.rs:165-168`;
    - `window_pane/mod.rs:1588-1592`.
  - `window_pane_destroy_ready` keeps a waited pane even after its item is
    dropped (`window_pane/mod.rs:360-364`), as tmux does.
- **`fcbdf8dde`** deleted `cmdq_abort_file_wait`. A lost client's queue is now
  released by Drop only.
  - `server_client_lost` (`server_client/mod.rs:1202-1265`) does five things:
    1. marks the client `CLIENT_DEAD`;
    2. clears its overlay and prompt (`1208-1209`);
    3. interrupts its files (`1211-1212`);
    4. resumes its wait-for waiters (`cmd_wait_for_client_lost`, `1216`);
    5. runs `cmdq_next` once (`1217`).
  - The queue is dropped in `server_client_free` (`1275`) when the last
    `ClientRef` goes.
  - `lost_queue_tests` (`4411-4538`) pin this.
- **`4b72f3636`** (tmux catch-up):
  - added `cmd_wait_for_client_lost` (`wait_for.rs:557-596`);
  - `window_pane_wait_finish` now defaults to `128 + SIGHUP`
    (`window_pane/mod.rs:638`). h1's `0fa934362` is therefore already on
    master, and no increment is needed for it.
- **`0d1d423bc`** added 38 `tests/core/conformance_future_*` files.
  - They were written while h1 was under review.
  - The `gap_*` files pin tmux timings that h1's design breaks.

### Consequences for the design

1. **No drain on client loss.** There is no `cmdq_abort`, no `abort_pending`
   and no drain. A lost client's queue is never run again (tmux parity) and is
   freed with the client.
2. **Poll on wake (§2.3).** The old plan, and h1, ran continuation bodies when
   `cmdq_next` polled the item inside `server_loop`, then patched ordering with
   `yield_now`. The gap tests show that tmux does that work inside the event
   callback, before any queue runs in that turn, and reads mutable state
   (session attachment, environment, buffers) at that moment. The new rule is
   that an item's waker polls its future synchronously. The body therefore runs
   inside the send, exactly where tmux runs its callback and where master runs
   it today.
   - This removes from the old plan: `QueueWake`, level-triggered polling,
     changes to `server_loop` accounting, step 11, and every yield.

## h1 lessons (each one is a requirement below)

- **L1. Event-time work and hook order.** h1 needed a yield after foreground
  `run-shell` completion. Otherwise a hook queued on the global queue by the
  output (`pane-mode-changed` from view mode) ran after the next command:
  `run-shell -t %0 'echo hi' \; set -gF @o '#{@o}a'` gave `ah` instead of `ha`.
  - Root cause: the body ran at poll time.
  - Under poll on wake, the body runs in the job's event and no yield exists.
    Re-check the scenario in I9.
- **L2. Local file completion is reported one runtime turn later.** h1 needed a
  yield after local file completion: `rename-session x ; source-file f` ran `f`
  before the global `session-renamed` hook (also with `load-buffer` and
  `save-buffer`).
  - tmux reports through `event_once`, in the next event-loop turn, after
    `server_loop` and `server_client_loop` (`s09_local_wait_turn` pins this).
  - Keep master's runtime-turn deferral (a task). A queue-pass yield is not
    enough.
- **L3. `split-window -W` sets the client's return value at finish time, not at
  poll time.** A key-bound `-W` whose pane outlives the original pane destroys
  the session when it exits. h1 then exited 3 where tmux exits 0. The same rule
  applies to popup status (`s06_popup_session_loss`) and to the prompt answer
  status (`s05_prompt_answer`).
- **L4. A `confirm-before` prompt freed unanswered leaves its item pending
  forever** (tmux's `cmd_confirm_before_free` never continues it). A
  `command-prompt` prompt freed unanswered resumes with nothing queued
  (`cmd_command_prompt_free`). See `s00_prompt_release` and
  `gap_display_client_loss`.
- **L5. A waiter records the item's own client, not the lent one.**
  `cmdq_fire_command` lends a clientless (hook or global) item a substitute
  client during exec (`queue.rs:775-784`) and restores it at `875`. A waiter
  must record the pre-dispatch client.
  - tmux reads `cmdq_get_client(item)` lazily and gets NULL.
  - `s02_hook_waiter_identity`, `s07_pane_wait_global_queue` and
    `s06_popup_queue_placement` pin this.
- **L6. A pane wait cut short reports `128 + SIGHUP`.** Already on master; keep
  it pinned (`s07_pane_wait_sighup`).
- **L7. Foreground `run-shell` and `if-shell` keep their client reference while
  the job runs.** After a client is lost, the job still runs and its callback
  still prints, or compiles the branch (parse-time `NAME=value` applies). Only
  the resume is lost. See `s01_lost_client_job_completion`,
  `s03_run_shell_client_loss` and `s04_if_shell_client_loss`.
- **L8. h1's commits were too large to review.** Its file step alone touched 12
  files and about 900 lines. h1 also changed several semantics silently:
  - `cmdq_running` visibility;
  - releasing a drained pane;
  - resolving dead file waits.

  Every semantic change below is its own increment, and it is labelled.

## Repo facts

- **Layout.**
  - Toplevel: `/home/blmarket/proj/h2`, a worktree on branch `h2`. `tiri/` is a
    subdirectory: the `tiri` crate plus a workspace that includes `tiri-rt`.
  - Differential conformance suites (each file is a module of one nextest
    binary, generated by `build.rs`):
    `/home/blmarket/proj/h2/tests/{core,misc,hmux,overlay}`.
  - tiri's own tests: lib unit tests under `tiri/src`, integration tests under
    `tiri/tests/*.rs`. These include `cmdq_handle_interfaces.rs` (no raw or
    `NonNull` `cmdq_item` in any signature, field or static),
    `client_file_protocol.rs`, `cmd_file_owner.rs`, `mode_prompt_cleanup.rs`,
    `prompt_creation.rs` and `source_file_pattern.rs`.
- **Running tests.** We are already inside the nix dev shell. Run `make ...`
  directly from `/home/blmarket/proj/h2`.

  | Target | What it runs |
  |---|---|
  | `test-core` | `tests/core` plus every lib and bin unit test of the tiri workspace |
  | `test-hmux` | the tiri workspace's nextest, including `tiri/tests/*` and tiri-rt, plus `tests/hmux` |
  | `test-misc` | `tests/misc` |
  | `test-overlay` | `tests/overlay` (popups and menus) |
  | `test-leak` | LSan over the tiri workspace tests; it does **not** run conformance |
  | `test-modules MODULE=<regex>` | tiri workspace `--lib` tests matching `test(/<regex>/)`; `MODULE='popup\|queue'` works |

- **Narrowing runs with `TEST_ARGS`.** `TEST_ARGS` passes extra nextest
  arguments.
  - `test-core` already passes `-E 'binary(=core) | kind(lib) | kind(bin)'`.
  - Narrow with positional name filters, which nextest intersects with that
    filterset: `make test-core TEST_ARGS="wait_for future_s02 command_queue"`.
    h1 used this form.
  - A second `-E` in `TEST_ARGS` is ORed with the Makefile's filterset, so it
    widens the run instead of narrowing it.
  - Test names carry the module path, for example
    `conformance_future_s02_flush_20261014::…`, `queue::run_shell::…` or
    `command_queue::fifo_source_file_does_not_block_other_clients`.
- **Baseline.** Before I1, save the failing-test list of every suite
  (`--no-fail-fast`). "Passes all suites" below means "no test changes from
  pass to fail against that list".
- **tiri-rt.**
  - `Task`: dropping it cancels the task (`tiri-rt/src/mio/runtime.rs:193-205`).
  - `Sleep` (`waits.rs:11-64`): returns `Ready` immediately when its deadline
    has already passed, so a zero `Sleep` does **not** defer to the next turn.
  - `dispatch` (`runtime.rs:473-525`) pops each ready entry before calling
    `wake()` or polling, so user code may run inside a wake.
  - There is no one-shot primitive yet.
  - Local idiom: store an `Option<LocalWaker>` and wake after releasing borrows
    (`src/file/stream.rs:18,53,58-70`; `src/reactor/streams.rs:23`).
- **tiri reactor.**
  - `reactor::Timer` (`src/reactor/timers.rs:8-26`) spawns a task. A zero delay
    defers to a runtime turn.
  - `reactor::yield_now` (`src/reactor/mod.rs:49-60`) wakes its own
    `cx.local_waker()`. Under poll on wake that defers nothing, so it is
    forbidden in queue continuations (R10).
- **`cmd_retval`** (`shared/command.rs:12-16`): `STOP=2`, `WAIT=1`, `NORMAL=0`,
  `ERROR=-1`. `STOP` is used by `detach_client.rs:116`. The ABI test is at
  `command.rs:127-143` (`WAIT` at `134`).

## 1. Inventory of wait sites (master `8ada83037`)

The table has one row per wait site. The first column is the site, where exec
returns `WAIT`.

**Columns 1–3: the site, its wait, and its resume.**

| # | Site | Waits for | Resume today (event side) |
|---|---|---|---|
| 1 | `wait_for.rs:475-508` `cmd_wait_for_wait` | channel `-S` | `cmd_wait_for_signal` 454-474 → `wait_items_continue_all` 121-127. Also `-w` (432-453), `cmd_wait_for_client_lost` (557-596) and `cmd_wait_for_flush` (597-) |
| 2 | `wait_for.rs:509-530` `cmd_wait_for_lock` | `-U` | `cmd_wait_for_unlock` 531-556: hands the lock to the first locker (`locked` stays 1); `-w`; lost; flush (unlocks) |
| 3 | `wait_for.rs:322-366` `-E` | hook event sink | sink `cmd_wait_for_event_cb` 261-312: prints the payload with `cmdq_print(item)` (`-v`), runs the `-F` filter through `format_create_with_client(.., Some(item))`, then continues; `-Ew` 381-411; lost; flush |
| 4 | `confirm_before.rs:63-144` | prompt answer | prompt cb 145-187: if confirmed, `cmdq_insert_after(item, cmdlist with item.state)`; sets the client's return value (0/1) if it has no session; `cmdq_continue` |
| 5 | `command_prompt.rs:111-261` | prompt(s) | cb 262-361: per-prompt `PROMPT_CONTINUE`; at the end, `args_make_commands` → `insert_after` (343) or a parse error appended to the **answering** client (329-336); continue (355-358). `Drop` 370-381 also continues |
| 6 | `run_shell.rs:148-278` (foreground) | `-d` timer, then job | `Timer` (zero delay = one runtime turn) in `item.wait_timer` (274-277) → `cmd_run_shell_timer` 279-389 runs the job, or for `-C` compiles and inserts → `cmd_run_shell_callback` 390-446 prints, sets the return value, continues |
| 7 | `if_shell.rs:72-184` (foreground) | job | `completecb` → `cmd_if_shell_callback` 185-241: insert the chosen branch with the item's state, or `cmdq_error`; continue |
| 8 | `source_file.rs:250-381` | one read per path, chained | `cmd_source_file_done` 183-231: `load_cfg_from_buffer` after `after`; the next read (`cmd_source_file_read` 168-181); finally `cmd_source_file_complete` 141-166 queues the completion callback (132-139) |
| 9 | `load_buffer.rs:122-165` | `file_read_with_cmdq_wait` | `cmd_load_buffer_done` 70-121: `paste_set` / `tty_set_selection` (`-w`) / `cmdq_error` |
| 10 | `save_buffer.rs:89-180` | `file_write_with_cmdq_wait` | `cmd_save_buffer_done` 71-88: `cmdq_error` |
| 11 | `display_message.rs:83-91` `-I` → `window_pane_start_input` (`window_pane/mod.rs:1603-1645`) | client stdin | `window_pane_input_callback` 1569-1601: data → `input_parse_buffer`; closed/error → continue; pane gone → `request_exit(1)` + `file_cancel`, **never continues** |
| 12 | `split_window.rs:440-448`: `-I` (as row 11); `-W` → `wait_until_close` (`window_pane/api.rs:811-813`) | pane close | `window_pane_wait_finish` (`mod.rs:630-646`) from `lifecycle.rs:247` (exit) or `mod.rs:712` (destroy) |
| 13 | `display_menu.rs:1185-1206` → `popup_display(.., Some(item), ..)` (`popup.rs:941`, item stored at 1035) | popup close | `impl Drop for popup_data` (`popup.rs:206-239`): if `published`, sets the return value of a sessionless client and continues |
| 14 | `cfg.rs:57-62` `cfg_client_done` (callback item) | config loaded | `cfg_done` 63-74 continues `cfg_item` (static `Weak`, line 41; set at 84-89) |
| 15 | `file.rs` glue | — | — |

**Columns 4–5: state the site owns, and what is ordering-sensitive.**

| # | State / cleanup it owns | Ordering-sensitive |
|---|---|---|
| 1 | `wait_channel.waiters: Vec<wait_item{Weak item}>` (`shared/command.rs:291-295`); `prune_expired` 202-205 | names come from `cmdq_get_client(item)` read lazily (180-195); resume does not run the waiter early (`s02_resume_order`) |
| 2 | `wait_channel.lockers` | FIFO handoff; a dead locker gives up its turn |
| 3 | `wait_event_items` (`Drop` removes the sink, 313-317) | multi-shot: a rejected filter keeps waiting; needs the item at event time |
| 4 | `cmd_confirm_before_data{item, cmdlist, ..}`, owned by the prompt closure | all at answer time (L3); freed unanswered → never continues (L4); `-b` appends to the answering client's queue |
| 5 | `cmd_command_prompt_cdata` | `-i` forces no wait (141-143); freed unanswered → continue with nothing (L4) |
| 6 | `cmd_run_shell_data` (strong `client` for foreground, 248-250; session; `Drop` 447-459) | no-delay deferral (`s03_run_shell_deferral`); output/status at the job's event (`gap_completion_turn`); L7 |
| 7 | `cmd_if_shell_data` (strong `client`, 146-148) | branch compiled at the job's event; L7 |
| 8 | `cmd_source_file_data` (depth counter, strong `client`, `after`, files) | read errors before any file's commands; depth falls in the completion callback (`s10_*`); local reads finish one turn later (L2) |
| 9 | `cmd_load_buffer_data` (target client for `-w`) | buffer stored at READ_DONE (`gap_completion_turn`) |
| 10 | — | L2 for local writes |
| 11 | `window_pane_input_data{item, client, wp, file}` | `s09_pane_input_gone` |
| 12 | `window_pane.wait_item: Weak` (`window_pane/model.rs:33`); `destroy_ready` 357-367 keeps the pane while `wait_item` was ever set and no status exists | status computed and return value set at finish (L3, L6) |
| 13 | `popup_data.item` (`popup.rs:67`) | status decided inside the free (L3); a startup failure returns **NORMAL** (`display_menu.rs:1216-1219`, tmux's `goto out`) |
| 14 | static | `%config-error` lines before the client's blocks (`s08_control_startup`) |
| 15 | see below | the local completion task is deferred one turn (L2) |

Row 15, the `file.rs` glue in detail:

- `file_set_cmdq_wait` 115-129;
- `file_cancel_cmdq_wait` 131-148;
- `file_fire_done_cb` 213-266 (dead check: expired item, dead file client,
  dead wait client);
- `file_fire_done` 267-306 (local: the task goes into the item via
  `cmdq_set_file_task`, 280-291; remote: `done_task` observes a `Weak`,
  292-305);
- `file_fire_read` 307-340;
- the wait APIs `file_write_with_cmdq_wait` 454-472,
  `file_read_with_cmdq_wait` 574-582 and `file_read_with_cmdq_wait_init` 584-;
- `client_files_interrupt` 1352-1364;
- fields `client_file.{wait_item, wait_active, wait_client, cancel_data}`
  (`file/model.rs:31-35`) and `cmdq_item.{wait_file, file_task}`
  (`queue.rs:82-83`).

### Other consumers of the queue state

- **`cmdq_next`** (`queue.rs:942-994`).
  - It arms `WAITING` **before** dispatch (968-970). A synchronous continue
    therefore completes in the same pass, and a re-entrant `cmdq_next` on the
    same queue during dispatch returns.
  - `queue.item` points at the head and stays set when the call returns
    because the head waits.
- **`cmdq_running`** (`996-1005`) returns `queue.item` unless it is
  `FIRED|WAITING`, which matches tmux's `CMDQ_WAITING` check. It has one
  deviation: `cmdq_fire_callback` sets `FIRED` before the callback runs
  (939). A *callback* item is therefore invisible while it runs, whereas tmux
  returns it.
  - Users: `hooks.rs:313` (the `NOHOOKS` check) and `hooks.rs:420` (monitor
    hooks insert after the running item).
- **`server_loop`** (`server.rs:298-316`) re-runs every queue while
  `items != 0`.
- **`completion_tests.rs:1-69`** reads `CMDQ_WAITING`, uses
  `cancel_cb`/`file_cancel_cmdq_wait`, and expects local completion to be
  deferred.
- **`lost_queue_tests`** (`server_client/mod.rs:4411-4538`) uses
  `CMD_RETURN_WAIT`, `cmdq_continue`, `CMDQ_WAITING` and
  `file_read_with_cmdq_wait_init(.., cancel)`.
- **`wait_for.rs` tests** construct `wait_item { item: Weak::new() }`
  (`681-684`).

## 2. Target design

### 2.1 `tiri_rt::oneshot` (new `tiri-rt/src/oneshot.rs`; `pub mod oneshot` in `tiri-rt/src/lib.rs`)

```rust
pub fn channel<T>() -> (Sender<T>, Receiver<T>);
impl<T> Sender<T> { pub fn send(self, v: T) -> Result<(), T>; pub fn is_closed(&self) -> bool; }
impl<T> Future for Receiver<T> { type Output = Option<T>; }  // None = sender dropped unsent
```

- **Representation.** `Rc<Slot<T>>` holding `value: Cell<Option<T>>`,
  `waker: RefCell<Option<LocalWaker>>` and `closed: Cell<bool>`.
- **Wake discipline.** `send`, and dropping the `Sender`, take the waker out of
  the slot and release every borrow **before** calling `wake()`. Under §2.3 the
  `wake()` runs arbitrary command code.
- **Receiver drop** marks the slot closed and drops the stored waker.
- **Why this `Rc` is justified.** The waiter (job, prompt, popup, pane,
  channel, file) and the item have independent lifetimes. Today they meet
  through a `Weak<UnsafeCell<cmdq_item>>` that exposes the whole item to every
  waiter. The one-shot is single-use, typed and two-ended, and waiters stop
  holding items. The only exception is the read-only `-E` sink (Q9).
  `LocalWaker` is already `Rc`-based. h1 shipped this exact API
  (`7ad491de9`, 190 lines with tests).

### 2.2 Item state (`src/cmd/queue.rs`)

`type_0`, `cb` and `flags` (with `wait_timer`, `file_task` and `wait_file` at
the end of the series) collapse into one private field:

```rust
enum Work {
    Command,
    Callback(Box<dyn FnOnce(&Rc<UnsafeCell<cmdq_item>>) -> cmd_retval>),
    Executing { resumed: bool },            // in exec/callback; `resumed` = legacy cmdq_continue during exec (I3..I21)
    Waiting,                                // legacy CMD_RETURN_WAIT (I3..I21)
    Suspended { future: Pin<Box<dyn Future<Output = ()>>>, waker: LocalWaker },  // from I4
    Polling { rewoken: bool },              // future moved out while polled (from I4)
    Fired,                                  // finished; removal by the next cmdq_next
}
```

- **Deleted.** `cmdq_type`, `CMDQ_COMMAND`, `CMDQ_CALLBACK` (I3), `CMDQ_FIRED`
  and `CMDQ_WAITING` (I3). `cmd` and `cmdlist` stay, because hook formats read
  `command_handle()` of a waiting item.
- **Suspending.**
  `#[must_use] pub unsafe fn cmdq_suspend(item: &Rc<..>, future: impl Future<Output=()> + 'static) -> cmd_retval`.
  - It asserts `Work::Executing`, stores
    `Suspended { future, waker: LocalWaker::from(Rc::new(ItemWake(Rc::downgrade(item)))) }`,
    and returns the private `SUSPENDED` (value `1`, the same as `WAIT`, until
    I21).
  - `cmdq_next` panics when the work is `Suspended` but `retval != SUSPENDED`.
  - From I21, it also panics on `retval == SUSPENDED` without a `Suspended`
    work.
  - `cmd_exec_fn` and `cmdq_cb` signatures are unchanged.
- **Output is `()`.** tmux ignores the status after a wait: `%end`/`%error`
  and `cmdq_remove_group` are decided on the synchronous return. This is
  decided (Q1).
- **Item access from a continuation.**
  `pub(crate) unsafe fn cmdq_resumed(&Weak<..>) -> Rc<..>` asserts
  `Work::Polling`. The handle must be dropped before an `.await`. After each
  poll, assert `Rc::strong_count(&item) <= 2`: the queue plus the poller, or
  only the poller once the queue has been freed (§2.3).
- **The lent client (L5).** `cmdq_fire_command` sets a private
  `client_lent: bool` while `item.client` holds the substitute (775-784 → 875).
  `cmdq_wait` (§2.5) records `Weak::new()` when the client is lent, and
  `item.client` otherwise.

### 2.3 Poll on wake, re-entrancy, client loss

- **`ItemWake(Weak<UnsafeCell<cmdq_item>>) impl LocalWake`.** On a wake,
  upgrade the item and dispatch on its state:
  - `Suspended`: poll it now (`cmdq_poll`).
  - `Polling`: set `rewoken`. The poller re-polls after a `Pending`.
  - anything else (`Executing`, `Fired`, legacy): ignore. A wake during exec is
    covered by the first poll.
  - It never touches the runtime, so it is safe after `shutdown_runtime`.
- **`cmdq_poll(item)`**, shared by the first poll and by wakes:
  1. Move the future out (`Polling`) and poll it with
     `ContextBuilder::from_waker(Waker::noop()).local_waker(&waker)`.
  2. On `Pending`, store it back, re-polling while `rewoken`.
  3. On `Ready`, set `Work::Fired` **first**, then drop the future.
  4. Never call `with_queue` after dropping the future. Dropping the
     continuation's data can release the last `ClientRef` and free the item's
     own queue; the poller's `Rc` keeps the item alive until it returns.
- **`cmdq_next`.** Per head:
  - `Command`/`Callback`: dispatch as today. After exec:
    - `Suspended`: first poll now, same pass. A wait that already ended
      resolves now, which matches master's "arm before dispatch".
    - legacy `WAIT` with `resumed == false`: `Waiting`.
    - otherwise: `Fired`.
  - `Suspended`, `Waiting`, `Executing`, `Polling`: return immediately, leaving
    `queue.item` set. `Executing` and `Polling` are the re-entrant case, for
    example `server_client_lost` reached from inside a dispatch through
    `ClientRef::shutdown` (`server_client/api.rs:1068-1070`). This replaces
    h1's `abort_pending`.
  - `Fired`: remove the head and continue.
  - `cmdq_next` never polls an old suspension, and `server_loop` is unchanged.
- **Where bodies run.** A wake raised by a runtime event (job exit, timer task,
  file done task, READ_DONE, SIGCHLD) runs the body inside the runtime
  dispatch. That is before `server_loop`, so a hook the body queues on the
  global queue runs first in the following `server_loop`, as in tmux.
  - A wake raised inside another queue's dispatch (`wait-for -S`,
    `send-keys -K` answering a prompt, `kill-pane` ending `-W`,
    `display-popup -C`) completes the item at once. Its queue moves on at
    `server_loop`'s next sweep, which runs because that dispatch counted as
    progress (`s02_resume_order`).
- **`cmdq_running`** keeps master's `queue.item` handling and maps states as
  follows:
  - `Suspended` / `Polling` / `Waiting` → `None`, as tmux's
    `CMDQ_WAITING`. The body runs while "waiting".
  - `Executing` / `Fired` / `Command` / `Callback` → `Some`.
  - A completed global head therefore stays visible until it is removed. This
    keeps the hook suppression of `s01_resumed_global_item_hooks` and
    `gap_resumed_prompt_hooks`, and keeps `s03_run_shell_view_mode_hook`
    seeing `None` during the body.
- **Client loss (master semantics, unchanged).** Nothing drains a lost queue.
  - Its suspended head lives until the client is freed, and is woken only by
    events tmux would also deliver.
  - When the client is freed, `server_client_free` drops the queue. Each item's
    `Drop` takes `Work` out (sets `Fired`) and then drops it. The future's
    drop cancels the wait (§2.4).
  - A continuation's destructor must not call `cmdq_next` on the item's own
    queue: between `Fired` and the poller's return the poller still holds the
    item, so `cmdq_remove`'s sole-owner assert would trip (found in I4).
  - A waiter must send or resume without holding an upgraded item `Rc`, or
    the post-poll `strong_count <= 2` assert trips (found in I4).
  - Printing re-enters: `cmdq_print`/`print_to` to an attached client can
    enter view mode and fire `pane-mode-changed` synchronously, which can
    run any sink or resume any waiter. Never hold an upgraded item `Rc`, a
    registry borrow or a reference into a waiter list across a print, a
    hook/event fire or a resume (found in I7: a verbose `-E` waiter on
    `pane-mode-changed` crashed the server).
  - `cmdq_suspend` stores `Suspended` while exec is still running. The waker
    is unreachable until the first poll registers it, so a send during exec
    only fills the slot. A site must never hand the item's waker out (or
    poll the future itself) before exec returns, or `ItemWake` would poll
    during exec and `cmdq_next` would panic on the non-`Executing` state
    (found in I6 review; relevant to I9+).
  - Destructors must not upgrade that client or call `with_queue` on it,
    because the `Weak` is dead.
- **Invariant R2 (no permanent cycle).** A future may own the strong
  `ClientRef`s that tmux's cdata owns (L7). It must then complete, or drop that
  data, when its waiter ends:
  - a job always exits or is killed;
  - a file always reaches `done` (`client_files_interrupt` at loss);
  - a prompt or popup is freed at loss.

  A wait that tmux never continues (the file client is dead, or confirm-before
  is freed unanswered) **releases its data and parks**:
  `drop(data); std::future::pending::<()>().await`. The item stays blocked, as
  in tmux, but no reference cycle `client → queue → item → future → client`
  survives.

### 2.4 Cancellation = drop the future

| Wait | The future owns | Drop effect | Waiter's view |
|---|---|---|---|
| run-shell delay | `reactor::Timer` (task) + `Receiver<()>` | task cancelled | — |
| job | `Receiver<JobCompletion>`; `completecb = move \|c\| drop(tx.send(c))` | job keeps running and its send is a no-op (tmux: the item lives, but parity is kept because the cdata keeps the client and therefore the item alive, L7) | `None` on `job_kill_all` → release and park |
| prompt | `Receiver<..>`; `Sender` in the prompt data | the prompt stays until cleared; a later answer sees `is_closed()` and queues nothing (master: `confirm_before.rs:151-155`) | `None` = freed unanswered (L4) |
| plain waits (wait-for, `-W`, popup, cfg) | `Receiver<()>` via `cmdq_wait` | registries prune with `is_closed()` | — |
| file | `FileWait` (§2.6) | `file_cancel_wait`: drop the waiter, the listener and the local completion task | `None` (dead file client) → release and park |

### 2.5 Plain waits: `cmdq_wait` / `CmdqWaiter` (`queue.rs`)

```rust
pub(crate) struct CmdqWaiter { client: ClientWeak, done: tiri_rt::oneshot::Sender<()> }
/// Suspend until resumed; the continuation is `async { if rx.await.is_none() { pending().await } }` (a dropped waiter never resumes).
#[must_use] pub(crate) unsafe fn cmdq_wait(item: &Rc<..>) -> (CmdqWaiter, cmd_retval);
impl CmdqWaiter {
    fn client(&self) -> Option<ClientRef>;   // the waiting client (L5), upgraded
    fn is_cancelled(&self) -> bool;          // item dropped
    unsafe fn resume(self);
    unsafe fn finish(self, status: i32);     // tmux popup_free_cb / window_pane_wait_finish:
                                             // set_return_value(status) if client has no session *now*, then resume
}
```

- **Users.** wait-for (rows 1–3), the pane `-W` wait, the popup and cfg.
- **API rule (overlap).** A plain wait uses `cmdq_wait`. `cmdq_suspend` with a
  hand-made one-shot is only for continuations with a body. `shared::wait_item`
  is deleted (I6).

### 2.6 File waits (`src/file.rs`, `src/file/model.rs`)

```rust
pub struct FileOutcome { pub path: Option<CString>, pub error: i32, pub buffer: SegmentedBuf }
pub struct FileWait { done: Receiver<FileOutcome>, file: Weak<UnsafeCell<client_file>>, local: Option<Task> }
impl Future for FileWait { type Output = Option<FileOutcome>; }   // None = file client dead → caller releases and parks
impl Drop for FileWait { /* file_cancel_wait(file): drop waiter + listener; transfer runs to its terminal completion unobserved */ }
pub(crate) unsafe fn file_write_wait(c, path, flags, data) -> FileWait;                                   // I17
pub(crate) unsafe fn file_read_wait(c, path, progress: impl FnOnce(Weak<..client_file>) -> client_file_cb) -> FileWait; // I18
```

- **Field change.** `client_file.{wait_item, wait_active, wait_client}` becomes
  `wait: Option<FileWaiter { done: Sender<FileOutcome> }>`.
  - The waiter carries no client. tmux checks only `cf->c`. Master's
    `wait_client` is the item's (possibly lent) client, which is the same
    client passed as the file's client.
- **Terminal event.** `file_fire_done_cb` sends the terminal event as a
  `FileOutcome`. If `cf.c` is dead, it drops the sender unsent.
  `client_file_cb` remains for read progress only.
- **Local transfers (L2).** `file_fire_done`'s local branch (280-291) keeps
  spawning the completion task. The task is owned by `FileWait.local` instead
  of the item, so the outcome arrives one runtime turn later. Do **not**
  replace this with a queue-pass yield.
  - The remote `done_task` path (292-305) is unchanged.

### 2.7 Deleted by the end (I21)

- **Return code, flags and queue functions:** `CMD_RETURN_WAIT` (and ABI line
  `command.rs:134`), `cmdq_continue`, `Work::Waiting`,
  `Work::Executing::resumed`.
- **Item fields:** `cmdq_set_wait_timer`/`cmdq_clear_wait_timer`/`wait_timer`
  (I9), `cmdq_set_wait_file`/`cmdq_set_file_task`/`cmdq_clear_wait_file`/`wait_file`/`file_task`
  (I20).
- **File API:** `file_*_with_cmdq_wait*`, `FileCmdqWait`, `file_set_cmdq_wait`,
  `file_cancel_cmdq_wait` (I17/I20), `client_file.{wait_item, wait_active, wait_client, cancel_data}`.
- **Item back-references in waiters:**
  - `cmd_*_data.item` in run_shell, if_shell, confirm_before, command_prompt,
    source_file and load_buffer;
  - `popup_data.item`, `window_pane.wait_item`, `window_pane_input_data.item`
    and `cfg_item`.
  - The one exception is `wait_event_item.item` (read-only, Q9).
- **Final grep.** Empty outside `src/compat/` and this file:
  `grep -rn "CMD_RETURN_WAIT\|cmdq_continue\|CMDQ_WAITING\|CMDQ_FIRED\|cmdq_type\|cmdq_set_wait\|cmdq_set_file_task\|_with_cmdq_wait\|wait_item\b" tiri/src tiri/tests`.

## 3. Increments

### Rules for every increment

- **Commit shape.** One commit per increment, with one concern. Aim well under
  300 changed lines.
- **Each one must:**
  - compile with no new warnings;
  - pass every suite against the baseline;
  - be behaviour-preserving against tmux, unless it says **Behaviour change**.
- **Overlap windows (AGENTS "no overlapping APIs"; temporary, stated
  explicitly):**
  - **W1.** The legacy `Work::Waiting` + `cmdq_continue` path coexists with
    `cmdq_suspend` from I4 to I21.
  - **W2.** The legacy read API `file_read_with_cmdq_wait*` coexists with
    `file_read_wait` from I18 to I20 (two users left).
  - No other overlap is allowed. Each migrated site deletes its old plumbing in
    the same commit.
- **Gate, run before every commit:**
  `make test-core && make test-hmux && make test-misc && make test-overlay && make test-leak`,
  each with `TEST_ARGS="--no-fail-fast"`, diffed against the baseline.
- **Every waiting site** also needs a unit test for R2 where it applies: "a
  client whose queue head is suspended here is freed once its last external
  ref goes; the data is released; the body does not run". Use
  `lost_queue_tests`-style asserts: `Weak::upgrade().is_none()` and drop-guard
  flags. LSan does not see servers.

### Differential harness (by hand; from `/home/blmarket/proj/h2`)

```sh
cargo build --manifest-path tiri/Cargo.toml --bin tiri
TIRI="env TMUX_C2RS_PLUGINS=none $PWD/tiri/target/debug/tiri"
d() { for T in tmux "$TIRI"; do echo "== ${T##*/}"; S="$T -L fut$$ -f /dev/null"
      $S new -d -s s -x 80 -y 24 'sleep 600'; $S set -g @t ''; eval "$1"; $S kill-server 2>/dev/null; done; }
# attached terminal client X (prompts, popups):  script -qfc "$S attach -t s" /dev/null >/dev/null & sleep .3; X=$($S list-clients -F '#{client_name}' | head -1)
# control client:  ( printf 'cmd1\ncmd2\n'; sleep 2 ) | $S -C attach -t s
```

Compare these between tmux and tiri:

- the command client's stdout and stderr, and `echo $?`;
- the `@t` trace (`$S show -gv @t`);
- `%begin`/`%end`/`%error` framing, ignoring time and number fields;
- `list-clients`, `wait-for -l`, `list-panes`, `show-messages`;
- server liveness (`$S ls` answers);
- that killed or detached waiters leave no blocked queue behind (a follow-up
  command finishes within 1 s).

---

**I1. `tiri_rt::oneshot`.**
- **Change.** Add `tiri-rt/src/oneshot.rs` (§2.1) and `pub mod oneshot`. h1's
  `7ad491de9` can be used as a reference. No tiri change.
- **Must not change.** Anything in tiri.
- **Tests (tiri-rt unit):**
  - send before poll yields `Some`;
  - poll, then send: the stored `LocalWaker` is woken after the slot borrow is
    released (the waker itself re-borrows the slot);
  - sender dropped unsent gives `None` and wakes;
  - receiver dropped gives `is_closed()` and drops the stored waker;
  - send after the receiver dropped returns `Err(v)`.
- **Validation.**
  - (a) `make test-modules MODULE=oneshot`.
  - (b) None. Gate only.

**I2. `cmdq_running` reports a running callback item.**
- **Behaviour change (toward tmux).**
- **Change.**
  - `queue.rs:939`: stop setting `CMDQ_FIRED` in `cmdq_fire_callback`.
    `cmdq_next` sets it after dispatch (979).
  - Rewrite the `cmdq_running` doc comment.
- **Must not change.** Command items, waiting items and continued items keep
  their current visibility.
- **Tests (`lifecycle_tests`):** a global callback item sees `cmdq_running()`
  as itself; a waiting item gives `None`; after `cmdq_continue`, `Some` until
  removed.
- **Validation.**
  - (a) `make test-modules MODULE=queue`;
    `make test-core TEST_ARGS="startup_config queue:: hook future_s01_resumed future_gap_resumed"`.
  - (b) A config file with `set -g @t ''`,
    `set-hook -g pane-mode-changed "set -gF @t '#{@t}m'"`, a line `nosuchcmd`
    and `set -g @x 1`: `$T -L c -f bad.conf new -s s` (attached, so
    `cfg_done` shows causes in view mode), then from another shell
    `$S show -gv @t`, `display -p '#{pane_mode}'`. Compare `@t` and the
    pane mode. Also `$S source-file bad.conf`, which runs the completion
    callback.

**I3. `Work` enum, legacy states only. No behaviour change.**
- **Change.** In `queue.rs`, replace `type_0`/`cb`/`flags` with
  `Work::{Command, Callback, Executing{resumed}, Waiting, Fired}`.
  - `cmdq_get_command` (635) → `Command`; `cmdq_get_callback_owned` (913) →
    `Callback`.
  - `cmdq_next` (942-994):
    - `Executing{resumed:false}` before dispatch (replaces arming at 968-970);
    - after dispatch: `WAIT && !resumed` → `Waiting`, else `Fired`;
    - re-entrant calls return on an `Executing` head.
  - `cmdq_continue` (574-577): `Executing` → `resumed = true`; `Waiting` →
    `Fired`; otherwise no-op.
  - `cmdq_running`: the §2.3 map, with `Waiting` → `None`.
  - Log the work label instead of the flags (955-959).
  - Delete `CMDQ_FIRED`/`CMDQ_WAITING` (`shared/command.rs:115-116`),
    `cmdq_type` (289), `CMDQ_COMMAND`/`CMDQ_CALLBACK` (`queue.rs:162-163`).
  - Add `#[cfg(test)] pub(crate) fn cmdq_is_waiting(&cmdq_item) -> bool` and
    use it in `completion_tests.rs:34` and `lost_queue_tests` (4468, 4525).
- **Must not change.**
  - Same-pass completion of a continue that happens during exec.
  - `%begin`/`%end` at exec return.
  - `cmdq_remove_group` on ERROR.
  - Numbering and time.
  - Item `Drop` order: the timer and file are cancelled before `cmdlist`,
    `state` and `cb` are released.
- **Tests:**
  - legacy `WAIT`, then `cmdq_continue`, removes the item on the next pass and
    runs the next one;
  - a continue during exec completes in the same pass;
  - a re-entrant `cmdq_next` from inside exec returns 0 and removes nothing;
  - `cmdq_running` for each state;
  - the existing two `lifecycle_tests`.
- **Validation.**
  - (a) `make test-modules MODULE='queue|file|lost_queue'`;
    `make test-core TEST_ARGS="queue:: command_queue future_s02 run_shell"`.
  - (b) Run under `d`:
    - `$S wait-for -S c; $S wait-for c; echo $?` (sticky);
    - `($S wait-for c \; set -gF @t '#{@t}W' &) ; sleep .2; $S wait-for -S c \; set -gF @t '#{@t}S'; sleep .2; $S show -gv @t` → `SW`;
    - control framing: `( printf 'wait-for c\ndisplay -p after\n'; sleep .5; ) | $S -C attach -t s &` then `$S wait-for -S c` (the wait-for block closes at once and `after` gets its own block);
    - `$S run 'sleep .2' \; display -p x`.

**I4. `cmdq_suspend`, poll on wake, `cmdq_resumed`. Unit tests only.**
- **Change.** In `queue.rs`, add `Suspended`/`Polling`, `ItemWake`,
  `cmdq_poll`, `cmdq_suspend` and `cmdq_resumed` (§2.2–2.3).
  - Add the `cmdq_next` branches and the `cmdq_running` mapping for them.
  - `cmdq_item::drop` takes `Work` out before dropping it.
  - Mark `cmdq_suspend` and `cmdq_resumed`
    `#[cfg_attr(not(test), expect(dead_code))]` until I6 or I9 use them.
- **Must not change.** Every production path (all still legacy).
- **Tests (new `suspend_tests` in `queue.rs`; no runtime needed):**
  - first poll `Pending`; `tx.send` runs the body **inside `send`** (a flag is
    set before `send` returns, before any `cmdq_next`); the next `cmdq_next`
    removes the item and runs the next one;
  - a send during exec gives same-pass completion;
  - dropping the queue owner drops the future (drop guard) without running the
    body, and drops the waker;
  - wakes in `Executing`/`Fired` are ignored; a self-wake during `Polling`
    re-polls;
  - `cmdq_running()` is `None` inside the body and `Some(head)` after `Ready`
    until removal (global queue);
  - a continuation that keeps an `Rc` item across an await trips the
    post-poll assert (`catch_unwind`);
  - `cmdq_suspend` followed by returning NORMAL panics;
  - a re-entrant `cmdq_next` from inside a body returns 0;
  - a body whose completion drops the last `ClientRef` of its own queue's
    client: no panic, client freed (R5);
  - send after `shutdown_runtime` still completes the item.
- **Validation.**
  - (a) `make test-modules MODULE='queue|suspend_tests'`; run
    `cmdq_handle_interfaces` through `make test-hmux`.
  - (b) None beyond the gate. There is no production change.

**I5. `cmdq_wait` / `CmdqWaiter` and the lent-client record. Unit tests only.**
- **Change.** In `queue.rs`:
  - `client_lent: bool`, set and cleared around exec in `cmdq_fire_command`
    (775-784, 875);
  - `CmdqWaiter`, `cmdq_wait` (§2.5);
  - `#[cfg_attr(not(test), expect(dead_code))]` until I6.
- **Must not change.** Dispatch: the substitute client is still visible to exec
  through `cmdq_get_client`.
- **Tests:**
  - a waiter created by a clientless global item that executes with a lent
    client records no client;
  - one created by a client-queue item records that client;
  - one created by an item `insert_after`'d behind a lent item records the
    lent client (tmux copies `after->client`; `s02_lost_global_waiter`);
  - `finish(status)` sets the return value only for a sessionless client, and
    reads the session at call time;
  - a dropped `CmdqWaiter` never resumes the item;
  - `is_cancelled()` after the item drops.
- **Validation.**
  - (a) `make test-modules MODULE=queue`.
  - (b) None.

**I6. wait-for channel waiters and lockers → `CmdqWaiter`.**
- **Change.**
  - `wait_channel.{waiters, lockers}: Vec<CmdqWaiter>`; delete
    `shared/command.rs:291-295 wait_item`.
  - `cmd_wait_for_wait`/`lock` (475-530) return the `cmdq_wait` retval.
  - `wait_items_continue_all` (121-127), `cmd_wait_for_continue_waiter`
    (196-200), unlock (531-556) and flush (597-) call `resume()`.
  - `cmd_wait_for_prune_expired` (202-205) uses `!is_cancelled()`.
  - Names (180-195) come from `waiter.client()`.
  - `cmd_wait_for_client_lost` (557-596) matches `waiter.client()` with
    `Rc::ptr_eq(c)`.
  - Fix the test at 674-689 (a cfg(test) detached waiter constructor).
  - The event list (`-E`) is untouched.
- **Must not change:**
  - FIFO order;
  - lock handoff keeps `locked = 1`;
  - a dead waiter gives up its place;
  - empty name for hook waiters;
  - "not able to wait"/"not able to lock" checks at exec (they use the lent
    client).
- **Tests:**
  - signal resumes in FIFO order;
  - the unlock handoff resumes only the first locker;
  - a pruned cancelled waiter is skipped by `-S` (the signal stays sticky);
  - `client_lost` resumes only matching waiters;
  - R2: a lost client whose head waits in `wait-for` is freed after
    `cmd_wait_for_client_lost` + `cmdq_next`.
- **Validation.**
  - (a) `make test-modules MODULE='wait_for|queue'`;
    `make test-core TEST_ARGS="wait_for future_s00_dead future_s01_lost_client_wait future_s02 command_queue"`.
  - (b) Under `d`:
    - **Listing.**
      `$S wait-for c & sleep .2; $S wait-for -l c; $S wait-for -S c; wait $!; echo $?`.
    - **Hook waiter identity (L5).**
      `$S set-hook -g window-renamed 'wait-for h'; $S rename-window x; $S wait-for -l h | od -c`
      should give one empty name. Then `$S wait-for -w "$(...client name...)" h`
      should not wake it, and `$S wait-for -w '' h` should.
    - **Killed waiter.**
      `$S wait-for c & p=$!; sleep .2; kill $p; sleep .2; $S wait-for -l c; $S wait-for -S c; timeout 1 $S wait-for c; echo $?`
      (0: the signal was remembered).
    - **Killed locker.**
      `$S wait-for -L l; ($S wait-for -L l \; set -g @t B &) ; p=$!; sleep .2; kill $p; $S wait-for -U l; timeout 1 $S wait-for -L l; echo $?; $S show -gv @t`.
    - **Global lost waiter.** A control client runs
      `set-hook -g window-renamed "if -F 1 'wait-for g'"` and
      `rename-window y`, then the control client is killed. Afterwards
      `wait-for -l g` is empty and the next `rename-window` hook still runs.
    - **Flush.**
      `$S wait-for c \; display -p after & sleep .2; $S kill-server; wait $!; echo $?`
      prints `after` and exits 0.
    - **Resume order and control framing** as in I3.

**I7. wait-for `-E` event waiters resume through `CmdqWaiter`.**
- **Change.**
  - `wait_event_item` gains `waiter: CmdqWaiter` and keeps `item: Weak` only
    for `cmdq_print` and the filter's format tree (Q9).
  - `cmd_wait_for_event_cb` (261-312) resumes through the waiter after
    printing and filtering.
  - `-Ew` (381-411), `-El` (367-380), `client_lost` and flush use
    `waiter.client()`/`resume()`.
  - Prune with `!is_cancelled()` (319-321).
- **Must not change:**
  - payload printing goes to the waiting item's client at event time;
  - a rejected filter keeps waiting;
  - the sink is removed with the entry;
  - a lost event waiter is not listed and is not printed to
    (`gap_lost_event_waiter`).
- **Tests:**
  - an event accepted by the filter resumes the item;
  - a rejected one keeps it `Suspended`;
  - dropping the item makes the next event prune the entry and remove its
    sink.
- **Validation.**
  - (a) `make test-modules MODULE=wait_for`;
    `make test-core TEST_ARGS="wait_for future_gap_lost_event future_s02_flush"`.
  - (b) Under `d`:
    - `$S wait-for -E window-renamed \; display -p woke & sleep .2; $S wait-for -El window-renamed; $S rename-window x; wait`;
    - `-v` payload lines;
    - `-F '#{==:#{window_name},y}'`: rename to x (still waiting), then y
      (wakes);
    - `-Ew <client name>`;
    - kill the waiting client, then `$S wait-for -El window-renamed` (empty),
      rename again (server alive, nothing printed).

**I8. run-shell prep: completion bodies take the waiting item explicitly. No
behaviour change.**
- **Change.** In `run_shell.rs`:
  - `cmd_run_shell_print`, `cmd_run_shell_timer` and `cmd_run_shell_callback`
    (95-446) take `item: Option<&Rc<item>>` (present exactly for the
    foreground form, as tmux tests `cdata->item`) instead of upgrading
    `cdata.item` inside;
  - the legacy timer and `completecb` closures do the upgrade-or-skip and pass
    it in;
  - delete the `cdata.wait` checks that the `Option` now expresses.
- **Must not change.** Any output, status or ordering.
- **Tests.** Adapt `expired_wait_skips_job_completion` (461-488) to the new
  signature.
- **Validation.**
  - (a) `make test-modules MODULE=run_shell`;
    `make test-core TEST_ARGS="run_shell shell_command coverage_run_shell"`.
  - (b) Under `d`: `$S run 'echo hi; exit 3'; echo $?`; `$S run -b 'true'`;
    `$S run -C 'set -g @t c' \; show -gv @t`.

**I9. Foreground run-shell on a future.**
- **Change.** In `cmd_run_shell_exec` (248-277), the foreground path suspends on
  a continuation that owns `cdata`, **including its strong `client`** (L7):
  1. create the oneshot;
  2. `reactor::Timer::new(tv, move || tx.send(()))`, so a zero delay still
     defers one runtime turn (`s03_run_shell_deferral`; `Sleep` would not);
  3. await;
  4. take `cmdq_resumed`;
  5. run the timer body. It either finishes (`-C` insert, no command, start
     failure) or installs `completecb = send(JobCompletion)` and returns the
     receiver;
  6. await the job and run the callback body with `cmdq_resumed`.

  A `None` from the job (killed or freed) releases `cdata` and parks. Then
  delete:
  - `cmdq_set_wait_timer`, `cmdq_clear_wait_timer` and `cmdq_item.wait_timer`
    (`queue.rs:80-81, 93, 179-189`);
  - `cmd_run_shell_data.{item, wait}`;
  - the expired-item test.

  `-b` is untouched (`cmdq_background`).
- **Must not change:**
  - everything in the timer and callback bodies runs at the timer or job
    event, before `server_loop` (`gap_completion_turn` run-shell cases,
    `delayed_command_line_*`);
  - `cmdq_running` is `None` while printing (`s03_run_shell_view_mode_hook`);
  - the sessionless return value is judged at job exit (`run_shell_status_*`);
  - a lost client's job still runs and prints (L7);
  - **no `yield_now`** (L1, R10).
- **Tests:**
  - the zero-delay wait does not start in the dispatching pass and completes
    after one runtime turn;
  - a delayed wait resumes at its deadline;
  - a job completion runs the callback body inside `completecb` (an output
    flag is set before `cmdq_next`);
  - item drop during the delay cancels the Timer (the job never starts);
  - R2: the client is freed after the job completes for a lost client.
- **Validation.**
  - (a) `make test-modules MODULE='run_shell|queue'`;
    `make test-core TEST_ARGS="run_shell future_s03 future_s01_lost_client_job future_gap_completion_turn future_gap_global_job shell_command queue:: command_queue"`;
    `make test-misc TEST_ARGS="run_shell"`.
  - (b) Under `d`:
    - **L1, the hook-order case h1 got wrong:**
      `$S set-hook -g pane-mode-changed "set -gF @t '#{@t}h'"; $S run -t %0 'echo hi' \; set -gF @t '#{@t}a'; $S show -gv @t`
      → `ha`. Repeat from a hook body
      (`set-hook -g window-renamed "run -t %0 'echo x'"`).
    - **Deferral:**
      `$S set-hook -g window-renamed "set -gF @t '#{@t}h'"; $S rename-window x \; run 'true' \; set -gF @t '#{@t}r'`
      → `hr`. The same with `run -C "set -gF @t '#{@t}c'"`, `-d 0` and
      `-d 0.3`.
    - **Status:** `run 'exit 3'` → rc 3 and `'exit 3' returned 3`;
      `run 'kill -TERM $$'` → rc 143 and `terminated by signal 15`.
    - **Client loss (L7):**
      `$S run -t %0 -d 1 'echo late' & sleep .2; kill $!; sleep 1.5; $S capture-pane -p -t %0 | tail -2; $S display -p '#{pane_mode}'`
      (both show `late` in view mode). Also kill during a running
      `sleep 1; echo late`.
    - **Global queue:** a hook-run `run 'sleep .5'` makes later event hooks
      queue behind it.

**I10. Foreground if-shell on a future.**
- **Change.** In `if_shell.rs`:
  - foreground exec (146-183) installs `completecb = send(JobCompletion)` and
    suspends on a continuation that owns `cdata` (strong `client`, L7), awaits
    it, and runs `cmd_if_shell_callback(completion, &mut cdata, Some(&cmdq_resumed(..)))`;
  - the callback (185-241) takes `item: Option<&Rc<..>>`;
  - delete `cmd_if_shell_data.{item, wait}` and the expired-item test;
  - `-b` and `-F` are unchanged.
- **Must not change:**
  - the branch is compiled at job exit (parse-time assignments are visible to
    a command another client runs in that turn: `gap_completion_turn`
    if-shell case);
  - the branch shares the item's state (`s04_branch_state`);
  - control framing (`s04_control_resume`);
  - lost-client branch compilation (`s04_if_shell_client_loss`).
- **Tests:**
  - completion runs the body inside the send;
  - a dropped job sender releases data and parks;
  - R2.
- **Validation.**
  - (a) `make test-modules MODULE=if_shell`;
    `make test-core TEST_ARGS="if_shell future_s04 future_gap_completion_turn future_gap_global_job queue:: command_queue config_parse"`.
  - (b) Under `d`:
    - `$S if 'true' 'display -p y' 'display -p n'` and `if 'false' ..`;
    - `if 'kill -9 $$' 'display -p y' 'display -p n'`;
    - branch parse error `if true 'nosuch'` (stderr, rc 1);
    - the same from a clientless hook (check `show-messages`);
    - nested `if 'true' "if 'true' 'display -p in'"`;
    - `if -F 1 ..`;
    - a client killed while `if 'sleep 1' 'GAPX=1'` runs, then
      `sleep 1.5; $S show-environment -g GAPX` (set in both);
    - control client `if 'sleep .2' 'display -p y'` framing.

**I11. confirm-before on a future.**
- **Change.** In `confirm_before.rs`:
  - the wait form stores a `Sender<bool>` in the prompt data instead of `item`
    (26-31, 87-89);
  - the callback (145-187), at answer time:
    - if the sender `is_closed()`, return `PROMPT_CLOSE` and queue nothing;
    - otherwise send the decision. The continuation (owning `cmdlist`) runs
      inside that send: confirmed → `insert_after(item, cmdlist with item.state)`;
      then set the return value 0/1 if the waiting client has no session;
  - `None` (prompt freed unanswered) → release and park (L4);
  - `-b` keeps the append to the answering client.
- **Must not change.**
  - **All effects at answer time (L3).**
  - A refusal or cancel gives retcode 1.
  - The queue the confirmed command lands in.
- **Tests:**
  - y / n / Enter with `-y`;
  - a dropped sender parks (the item stays `Suspended`; the data is released);
  - an answer after the item was dropped queues nothing.
- **Validation.**
  - (a) `make test-modules MODULE='confirm_before|queue'`;
    `make test-core TEST_ARGS="future_s05 future_s00_prompt_release future_gap_display_client_loss future_gap_resumed_prompt prompt"`;
    `make test-misc TEST_ARGS="prompt"`.
  - (b) With attached client X:
    - `$S confirm -t $X 'set -g @t yes' \; display -p after & sleep .3; $S send-keys -K -c $X y; wait $!; echo $?`.
      Repeat with `n`, `Escape`, and `-y` + `Enter`.
    - A second `confirm -t $X` replaces the first prompt: the first waiter
      stays blocked (`timeout 1`), and `@t` is unchanged.
    - Kill the waiter, then answer `y`: nothing runs, and the server is alive.
    - A hook-run confirm answered by
      `send-keys -K -c $X y \; rename-window z` with a `window-renamed` hook:
      the hook is suppressed in both.
    - `send-keys -K -c $X y \; kill-session` in one line: compare the waiter's
      rc (decided at answer time).

**I12. command-prompt on a future.**
- **Change.** In `command_prompt.rs`:
  - `cdata.item` becomes `Option<Sender<Option<Rc<RefCell<cmd_list>>>>>`
    (34-36, 145-158);
  - the callback (262-361) keeps `PROMPT_CONTINUE` and `args_make_commands` at
    answer time. A parse error is still appended to the **answering** client.
    `Ok` sends the list, and the continuation inserts it after the item with
    the item's state;
  - closing without a list drops the sender (the item resumes with nothing);
  - delete the `Drop` continue (370-381): a dropped sender resumes with
    nothing (L4);
  - `-b` and `-i` are unchanged.
- **Must not change.**
  - Multi-prompt walking, `-1`/`-k`/`-N`.
  - A pane prompt (`-P`) freed by `kill-pane` resumes the waiter.
  - An orphaned prompt (waiter lost) still reports parse errors to the
    answering client (`s05_orphaned_prompt`).
- **Tests:**
  - the answer inserts after the item inside the send;
  - a dropped sender resumes with nothing;
  - an answer after the item was dropped still appends parse errors;
  - adapt the tests at 383-430.
- **Validation.**
  - (a) `make test-modules MODULE='command_prompt|queue'`;
    `make test-core TEST_ARGS="future_s05 future_s00_prompt_release future_gap_display_client_loss queue::prompt_substitution prompt"`;
    `make test-misc TEST_ARGS="command_prompt prompt"`.
  - (b) With X:
    - `$S command-prompt -t $X -p a,b "set -g @t '%1-%2'" \; display -p after`,
      answered with `send-keys -K -c $X 1 Enter 2 Enter` (`@t`, rc,
      `after`);
    - `-1`, `-N` with a non-number, `-i` (does not block);
    - template parse error (`"nosuch %1"`): status message on X, waiter rc 0;
    - replace the prompt with `confirm -t $X` (the waiter resumes and `after`
      prints);
    - `command-prompt -P -t %0` with `kill-pane -t %0` (resumes);
    - kill the waiter, then answer with a bad template (the error is shown on
      X).

**I13. display-popup on a `CmdqWaiter`.**
- **Change.**
  - `popup_display` (`popup.rs:941`) takes `Option<CmdqWaiter>` instead of
    `Option<&Rc<item>>`;
  - `popup_data.item` (67, 1035) becomes `waiter: Option<CmdqWaiter>`, stored
    only once published. Then the `published` flag goes: a startup failure
    drops the waiter;
  - `Drop` (206-239) calls `waiter.finish(self.status)`. This is still at free
    time (L3);
  - `display_menu.rs:1185-1206` passes `popup_display` a `wait` closure that
    calls `cmdq_wait(item)`; `popup_display` invokes it only at publish, after
    every failure point, and the command returns its retval. On failure the
    closure never runs and the command returns **NORMAL**, unchanged. (Done
    this way in I13: `cmdq_wait` suspends at once, so waiting first and
    dropping the waiter on failure would trip `cmdq_next`'s sentinel assert.)
  - display-menu is untouched.
- **Must not change:**
  - which client takes the status (L5: a hook or global popup records
    nothing; an `insert_after`'d one reports to the command client;
    `s06_popup_queue_placement`);
  - session judged at free time (`s06_popup_session_loss`).
- **Tests:**
  - dropping a published `popup_data` resumes and sets the return value of a
    sessionless waiter only;
  - unpublished: no resume.
- **Validation.**
  - (a) `make test-modules MODULE='popup|display_menu|queue'`; `make test-overlay`;
    `make test-core TEST_ARGS="future_s06 future_gap_display_client_loss"`;
    `make test-misc TEST_ARGS="interactive_command_queue"`.
  - (b) With X:
    - `$S display-popup -c $X -E 'exit 5'; echo $?` (5);
    - closed by `$S display-popup -c $X -C` from another client (129);
    - C-c; Escape after exit without `-E`;
    - a too-small popup `-w 500` (returns at once, rc 0);
    - X killed or detached while open;
    - waiter killed before close;
    - a popup from a `window-renamed` hook (no rc recorded; later hooks wait);
    - `display-popup -c $X -C \; kill-session -t s` in one line (the waiter
      keeps its rc).

**I14. split-window `-W` on a `CmdqWaiter`.**
- **Change.**
  - `window_pane.wait_item` (`model.rs:33,109`) becomes
    `wait_close: Option<CmdqWaiter>`;
  - `wait_until_close(&self, CmdqWaiter)` (`api.rs:141, 811-813`). (Done
    this way in I14: `WindowPane` is a `pub` trait, so `CmdqWaiter` itself
    became `pub` to avoid a `private_interfaces` warning; its fields and
    methods stay private or `pub(crate)`.)
  - `window_pane_wait_finish` (`mod.rs:630-646`) computes the status as today
    (`128 + SIGHUP` default, L6) and calls `waiter.finish(status)`, at finish
    time (L3);
  - `destroy_ready` (360-364) keeps the pane while `wait_close.is_some()` and
    no status exists. This is presence, **not** `is_closed()`: tmux and master
    keep the pane after the waiter's item is gone. That reverses h1's risk 9;
  - `split_window.rs:445-447` returns the `cmdq_wait` retval.
- **Must not change:** exit and signal statuses, `-d`, `respawn-pane -k`,
  `remain-on-exit`, `kill-pane`/`kill-window`/`kill-session` → 129.
- **Tests:**
  - process exit resumes and sets the status for a sessionless waiter;
  - destroy before reap gives 129;
  - an exited pane with a cancelled waiter is still kept until its status is
    ready.
- **Validation.**
  - (a) `make test-modules MODULE='wait_close|window_pane|queue'`;
    `make test-core TEST_ARGS="future_s07 future_s01_resumed_global"`.
  - (b) Under `d`:
    - `$S splitw -W -t s 'exit 5'; echo $?` (5);
    - `'kill -TERM $$'` (143);
    - `-d 'exit 0'`;
    - waiting, then `kill-pane`, `kill-window` or `kill-session` on the new
      pane (129, the L6 regression check);
    - a HUP-ignoring child (`trap "" HUP; sleep 1`) killed by `kill-pane`;
    - `respawn-pane -k`;
    - `remain-on-exit on`;
    - a command client killed while waiting: `list-panes` until the child
      exits, and nothing after the wait runs;
    - `-W` from a `window-renamed` hook (blocks the global queue; the
      triggering client keeps rc 0);
    - **the L3 regression:** bind a key to `split-window -W 'sleep .3'`, run it
      on X, kill the original pane so the waited pane's exit destroys the
      session, and compare X's exit status (0).

**I15. Startup config (`cfg_client_done`) on a `CmdqWaiter`.**
- **Change.**
  - `cfg.rs:41` `cfg_item` becomes `static mut CFG_WAITER: Option<CmdqWaiter>`;
  - the callback (84-89 / 57-62) returns `cmdq_wait`'s retval while
    `cfg_finished == 0` and stores the waiter;
  - `cfg_done` (63-74) takes it and calls `resume()` after
    `cfg_show_causes`.
- **Must not change.**
  - A lost first client: the next client re-runs `start_cfg` and is resumed by
    the first load's `cfg_done` (`s08_lost_startup_client`).
  - `%config-error` before blocks.
- **Tests.** Resume after `cfg_done`; a replaced waiter drops the old one
  without resuming.
- **Validation.**
  - (a) `make test-core TEST_ARGS="future_s08 startup_config config_parse source_file"`.
  - (b) Config `c.conf`: `run 'sleep 1'` and `set -ga @t c`.
    - `$T -L x -f c.conf new -d \; set -ga @t n; $T -L x show -gv @t` (`cn`);
    - a second client during the wait;
    - the first client killed during the wait (config loaded twice:
      compare `@t`);
    - `$T -L x -f c.conf -C new` (`%config-error` ordering with a bad line).

**I16. File prep: delete `client_file.cancel_data`. No behaviour change.**
- **Change.**
  - Remove `cancel_data` (`model.rs:35,72`), the `cancel_cb` parameters of
    `file_set_cmdq_wait` (115-129), `file_read_with_cmdq_wait(_init)`
    (574-605) and `FileCmdqWait` (473-474), and the cancel call in
    `file_cancel_cmdq_wait` (144-147);
  - every production caller passes `None`;
  - rewrite `completion_tests.rs` and `lost_queue_tests` (4492-4538) to
    observe cancellation through a drop guard captured by the callback.
- **Must not change.** Anything at runtime.
- **Validation.**
  - (a) `make test-modules MODULE='file|completion_tests|lost_queue'`;
    `make test-core TEST_ARGS="source_file load_buffer save_buffer queue_stdin"`.
  - (b) None.

**I17. `FileWait` and `file_write_wait`; save-buffer migrated.**
- **Change.**
  - `FileOutcome`, `FileWait`, `FileWaiter`, `client_file.wait`;
  - `file_fire_done_cb` sends for waiters that use the new field;
  - the local branch of `file_fire_done` hands the completion task to the
    `FileWait` (§2.6, L2);
  - `file_write_with_cmdq_wait` (454-472) is replaced by `file_write_wait`
    (its only user);
  - in `save_buffer.rs:157-179`, suspend on
    `async { match wait.await { Some(o) => cmd_save_buffer_done(&cmdq_resumed(..), o) /* cmdq_error on o.error */, None => park } }`.
  - No overlap for writes.
- **Must not change.**
  - A local write reports one runtime turn later (`s09_local_wait_turn`
    save-buffer cases).
  - `save-buffer -` from a command client waits for the client's ack
    (`s09_killed_file_waiter`).
- **Tests:**
  - a local write outcome is observed after one runtime turn, not in the
    dispatching pass;
  - dropping the `FileWait` cancels the task and releases the file;
  - a dead file client drops the sender, so the continuation parks;
  - R2.
- **Validation.**
  - (a) `make test-modules MODULE='file|save_buffer'`;
    `make test-core TEST_ARGS="save_buffer future_s09 command_queue::save_buffer"`;
    `client_file_protocol` and `cmd_file_owner` through `make test-hmux`.
  - (b) Under `d`:
    - `$S set-buffer -b b x; $S save-buffer -b b /tmp/o.$$; cat /tmp/o.$$`;
    - `-a`;
    - `/nonexistent/x` (message, rc 1);
    - `$S save-buffer -b b -` (stdout);
    - `$S save-buffer -b b - | sleep 3` killed mid-way (server alive);
    - **L2 ordering** from a control client:
      `set-hook -g session-renamed "set -gF @t '#{@t}h'"`, then
      `rename-session y \; save-buffer -b b /tmp/o \; set -gF @t '#{@t}w'`
      → `hw`.

**I18. `file_read_wait`; load-buffer migrated. Opens W2.**
- **Change.**
  - add `file_read_wait` beside the legacy read API (two users remain);
  - `load_buffer.rs:122-165`: suspend on a continuation owning the target
    client (`-w`) and name. On `Some`, run `cmd_load_buffer_done` with
    `cmdq_resumed`; on `None`, park;
  - delete `cmd_load_buffer_data.item` and its progress-coalescing and
    expired-item tests (168-).
- **Must not change.** The buffer is stored at READ_DONE, before the next
  queue (`gap_completion_turn` load-buffer case); local reads report one turn
  later.
- **Tests:**
  - a remote-style outcome runs the body inside the send;
  - local deferral;
  - R2 for `load-buffer -` on a lost client.
- **Validation.**
  - (a) `make test-modules MODULE='file|load_buffer'`;
    `make test-core TEST_ARGS="load_buffer future_s09 future_gap_completion_turn queue_stdin command_queue"`.
  - (b) Under `d`:
    - `$S load-buffer -b b /etc/hostname; $S show-buffer -b b`;
    - `echo hi | $S load-buffer -b b -`;
    - a missing file (message, rc);
    - `-w` from attached X (selection);
    - a hook-run load-buffer;
    - `$S load-buffer -b b - \; set -g @t ran` with stdin held open, then the
      client killed (`@t` unset, server alive);
    - **L2 ordering** with `rename-session y \; load-buffer f \; set -gF @t ..`
      from a control client.

**I19. source-file migrated.**
- **Change.**
  - `source_file.rs:168-231` + 369-377 become one continuation that owns
    `cdata` (strong `client`, depth): for each path, `file_read_wait(..).await`,
    then `cmdq_resumed`, then report the error or `load_cfg_from_buffer` after
    `after`;
  - after the last path, `cmd_source_file_complete`;
  - `None` releases `cdata` (its `Drop` leaves the depth, as master's dead path
    does) and parks;
  - delete `cmd_source_file_data.item` and `into_read_callback`.
- **Must not change:**
  - every read error before any file's commands, and parse causes printed by
    the first completion callback (`s10_source_chain`);
  - depth semantics, including startup not decrementing (`s10_source_depth`);
  - stdin source;
  - parse at READ_DONE (`gap_completion_turn` source-file case);
  - L2 for local reads.
- **Tests:**
  - a two-file chain inserts the second file's commands after the first's;
  - drop mid-chain releases depth;
  - a dead client parks and releases depth.
- **Validation.**
  - (a) `make test-modules MODULE='source_file|file'`;
    `make test-core TEST_ARGS="source_file future_s10 future_s09 future_gap_completion_turn queue::source_file command_queue::fifo startup_config"`;
    `source_file_pattern` through `make test-hmux`.
  - (b) Under `d`:
    - nested `source` (a file that sources another);
    - `source f1 missing f2` (error order);
    - `-q`, `-n`, `-v`, a glob;
    - `echo 'set -g @t x' | $S source -`;
    - 51-level nesting (`too many nested files`);
    - **L2:** `rename-session y \; source-file f` from a control client with a
      `session-renamed` hook (hook first).

**I20. Pane input migrated; legacy file wait deleted. Closes W2.**
- **Change.**
  - `window_pane_start_input` (`mod.rs:1603-1645`) returns
    `Result<Option<FileWait>, CString>`;
  - the progress listener keeps `input_parse_buffer`, and when the pane is
    gone or the client dead it calls `request_exit(1)` + `file_cancel`;
  - the continuation, on an outcome, re-checks the pane. If the pane is gone,
    it calls `request_exit(1)` and parks (`s09_pane_input_gone`: never
    resumes); otherwise it completes;
  - callers: `display_message.rs:83-91` and `split_window.rs:440-443`; `api.rs`
    (143, 814-816);
  - delete:
    - `file_read_with_cmdq_wait(_init)`, `file_set_cmdq_wait`,
      `file_cancel_cmdq_wait`;
    - `client_file.{wait_item, wait_active, wait_client}`;
    - `cmdq_set_wait_file`, `cmdq_set_file_task`, `cmdq_clear_wait_file`,
      `cmdq_item.{wait_file, file_task}` and the file branch of
      `cmdq_item::drop`;
  - rewrite `completion_tests.rs` and `lost_queue_tests` on `FileWait`.
- **Must not change.**
  - Input that ends after its pane is gone exits the client with 1 and runs
    nothing after it.
  - Server-side `file_cancel` behaviour.
- **Tests:**
  - data feeds the pane;
  - EOF resumes;
  - pane gone parks and requests exit;
  - lost client: the queue is freed and `client_files_is_empty` holds at
    `server_client_free`.
- **Validation.**
  - (a) `make test-modules MODULE='file|window_pane|lost_queue|completion_tests'`;
    `make test-core TEST_ARGS="future_s09 display_message split_window stdin"`.
  - (b) Under `d`:
    - `echo hi | $S display -I -t %0; $S capture-pane -p -t %0`;
    - `echo hi | $S splitw -I`;
    - `(echo a; sleep 2; echo b) | $S display -I -t %1 \; set -g @t moved; echo $?`
      with `kill-pane -t %1` after 1 s (rc 1, `@t` unset);
    - stdin closed by killing the client.

**I21. Remove the legacy wait. Closes W1.**
- **Change.**
  - Delete `CMD_RETURN_WAIT` (`shared/command.rs:14`, ABI line 134),
    `cmdq_continue`, `Work::Waiting` and `Executing::resumed`;
  - `SUSPENDED` becomes private, and the assert becomes two-way;
  - rewrite the remaining legacy tests (`lifecycle_tests`, `lost_queue_tests`)
    on `cmdq_suspend`;
  - the §2.7 grep must come back empty.
- **Must not change.** Anything at runtime.
- **Tests.** Returning `SUSPENDED` without installing panics.
- **Validation.**
  - (a) The full gate.
  - (b) Rerun one scenario each from I6, I9, I11, I13, I14, I17 and I19.

## 4. Risks

1. **R1, poll-on-wake re-entrancy.** Bodies run inside `send` and
   sender-drop.
   - Call sites:
     - job `completecb` (after `job_finish` releases its borrow,
       `job.rs:521-534`);
     - `reactor::Timer` task;
     - file done task and READ_DONE;
     - prompt input callback (closure holds `&mut cdata`);
     - `status_prompt_clear` / `prompt_free`;
     - `popup_data::drop`;
     - `window_pane_wait_finish` (inside pane destroy);
     - `cmd_wait_for_*` (inside another item's exec);
     - `server_client_lost` (inside a runtime event or `ClientRef::shutdown`).
   - Each site runs the same body code inline on master today, so its borrow
     state is unchanged. New code must not send while holding a `with_queue`
     borrow, a model `RefCell` or a runtime borrow.
   - Tests: I4's re-entrancy cases, plus each site's "body runs inside send"
     test.
2. **R2, client cycles.** A future that holds its own client's `ClientRef` and
   never completes leaks the client and its queue. LSan does not see servers.
   - Mitigation: §2.3 release-and-park, and a per-site unit test that the
     client is freed.
   - Audit every new continuation for `ClientRef` / `SessionRef` captures.
3. **R3, timing.** Every body must run at its event and every deferral must be
   a runtime turn.
   - Guarded by `gap_completion_turn`, `s03_run_shell_deferral`,
     `s09_local_wait_turn` and `s02_resume_order`.
   - A body placed after an extra `.await` (for example awaiting a Sleep that
     is already past) silently moves work.
4. **R4, `cmdq_running`.** I2 is the only semantic change. The rest maps
   states 1:1. Watch `hooks.rs:313,420` users:
   - `nested_suppression`;
   - `conformance_window_closed_hooks`;
   - `conformance_uncovered_last_pane_hook_order_*`;
   - `s01_resumed_global_item_hooks`.
5. **R5, the queue freed during a poll.** Dropping a completed continuation can
   free the item's own client and queue.
   - `cmdq_poll` sets `Fired` before dropping the future, and never touches the
     queue afterwards.
   - The post-poll assert is `<= 2`, not `== 2`.
6. **R6, overlap windows W1 and W2.** Keep W2 to three adjacent commits (I18,
   I19, I20). Do not start I18 unless I19 and I20 are ready to follow.
7. **R7, the `wait-for -E` exception.** The sink keeps a read-only `Weak` item.
   It may only print and evaluate the filter while the item is `Suspended`, and
   must never resume except through its waiter.
8. **R8, dead-code allowances in I4 and I5.** These are
   `cfg_attr(not(test), expect(dead_code))`. `expect` fails the build once the
   code is used, so the allowance cannot outlive its need.
9. **R9, the strong-count assert.** A continuation that keeps a `cmdq_resumed`
   handle across `.await` panics at once. This is intended, and I4 tests it.
10. **R10, yields are forbidden.** No `yield_now` in queue continuations: under
    poll on wake it re-polls immediately and defers nothing. Defer with
    `reactor::Timer` (one runtime turn) or the file layer's task.
11. **R11, the popup failure path** returns NORMAL (tmux's `goto out`). The old
    plan said ERROR, which was wrong.

## 5. Open questions

**Resolved:**

1. **Future output `()`.** Decided; h1 confirmed nothing needs a post-wait
   status.
2. **Private sentinel through the unchanged `cmd_exec_fn`.** Decided; h1
   confirmed it. The typed enum would touch about 100 signatures.
3. **Commit shape.** Decided: small increments with the explicit overlaps W1
   and W2. h1's 11 large steps were not reviewable.
4. **Client-lost semantics.** Decided by master: no drain, Drop-based release,
   a lost queue never runs again (tmux parity), and re-entrant `cmdq_next`
   returns on a busy head instead of h1's `abort_pending`.
5. **Dead file client on a live (global) queue.** Decided: keep tmux's hang
   (release data, park). Resolving it as cancelled would be a behaviour change,
   and it would get its own increment and a new conformance test if ever
   wanted.
6. **Home of the one-shot.** Decided: `tiri-rt`.
7. **run-shell deferral.** Decided: keep tmux's one-turn deferral via
   `reactor::Timer` (`s03_run_shell_deferral`).
8. **Edge-triggered polling (old step 11).** Obsolete: poll on wake does no
   periodic polling.
9. **`wait-for -E` item access.** Decided: keep the read-only `Weak` item for
   print and filter. A multi-shot channel would add a primitive for one user.

**Still open:**

- **A. `CmdqWaiter::finish`.** Is `finish` (status + resume) worth a method for
  two users (popup, pane), or should each site inline the sessionless check?
  It is kept as a method because both are the same tmux logic.
- **B. `-E` payload under poll on wake.** Could `-E` drop its `Weak` item by
  formatting the payload inside the continuation? That would need a multi-shot
  primitive. Revisit after I21.
- **C. Callback visibility (I2).** Is anything outside the tests relying on
  master's "callback invisible" `cmdq_running`? If I2's differential run shows
  a tmux mismatch elsewhere, drop I2 and keep master's behaviour in I3.

## Critical files

- `tiri/src/cmd/queue.rs`
- `tiri-rt/src/oneshot.rs` (new) and `tiri-rt/src/lib.rs`
- `tiri/src/file.rs`, `tiri/src/file/model.rs`, `tiri/src/file/completion_tests.rs`
- `tiri/src/cmd/entries/{wait_for,run_shell,if_shell,confirm_before,command_prompt,source_file,load_buffer,save_buffer,display_menu,display_message,split_window}.rs`
- `tiri/src/{popup,cfg}.rs`, `tiri/src/window_pane/{mod,api,model}.rs`
- `tiri/src/shared/command.rs`
- `tiri/src/server_client/mod.rs` (`lost_queue_tests`)
- Tests: `tests/core/conformance_future_*`, `tests/core/{command_queue,queue}.rs`, `tests/core/queue/*`
