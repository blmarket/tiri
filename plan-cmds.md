# Plan: the command system as a Future

Status: in progress. Prerequisite crate extraction (`tiri-core` / `tiri-cmds`),
subsystem narrowing ([plan-tiri-modules.md](plan-tiri-modules.md) Alternative 1:
moving `cfg`, `hooks`, `bindings` dispatch, and `wait` channels into `tiri-core`
and shrinking `Runtime` to the 15-field `CommandEngine` in
[actions.rs](tiri-core/src/actions.rs)), and removal of mid-queue splicing
(`cmdq_insert_after` and `EventPayloadIdentity::Operation` replaced by
`CmdqFollowup` / `cmdq_defer` and `hooks_fire_command`) are implemented. The
`Lane` extraction, borrowed `Invocation` / `Context`, async `cmd_exec_fn` +
`Tail`, and `cmd::run` cutover below remain.

Constraint: this refactor must not change any observable behavior. Existing
control clients, including iTerm2, must work without modifications. Preserve
guard blocks and their timing, reply flags and ordering, notifications, hook
selection and execution order, follow-ups, group failure rules, and cancellation
behavior, including existing edge cases. Internal mechanics remain design work;
behavior-changing alternatives are outside this refactor.

## Verdict

Feasible, and most of the hard part already exists: each queue is already driven
by one future (`cmdq_drain` in [queue.rs](tiri-cmds/src/cmd/queue.rs)), waiting
commands already hand back a future (`TaskOutcome::Suspend`), and post-command
follow-ups (`CmdqFollowup` / `cmdq_defer`) already execute as futures rather than
mutating `cmdq_list`. The refactor is an inversion of the interface and removal
of shared item/state wrappers, not a new execution model.

It cannot be *only* a future. Three things have to survive in some form:

1. **An ordered owner for sync callers.** Key input, imsg dispatch, control
   lines, config loading and hooks all start commands from sync callbacks. They
   cannot `.await`, and per-client FIFO order is observable (a waiting
   `run-shell` holds back later keys). Something must own the futures and poll
   them in order. That is a generic sequencer, not a command concept.
2. **A tail, not a nested await inside the body.** Follow-ups scheduled via
   `CmdqFollowup` / `cmdq_defer` mean "run this after me, before the rest of the
   queue". Awaiting follow-up commands inside a synchronous/unsuspended body
   would run them before the outer command's `after-<name>` hook and inside its
   control-mode `%begin`/`%end` block. Instead the body returns a tail that
   receives `&mut Context` when the runner executes it after the epilogue, so
   the whole sequence is one `.await` chain without `Rc<cmdq_item>` follow-up
   wrappers (see "Execution order" for the suspension compatibility constraint).
3. **A small ambient current command for hooks.** [hooks.rs](tiri-core/src/hooks.rs)
   reads `actions::queue::running()` (`cmdq_running()`) for the `NO_HOOKS` check
   on events and for ambient key-event, format, and error-reporting context when
   monitor hooks fire.

With those three kept small, every external caller reduces to a lane-owned
async block awaiting `cmd::run(commands, context)`, or `cmd::run(..).await`
from async code, and dropping the future cancels.

## How it works today

| Piece | Where | Role |
| --- | --- | --- |
| `cmdq_list` (`WorkQueue`) | one per client (`Client::borrow_queue_mut`), one global thread-local | FIFO of `Rc<cmdq_item>`, active head, pending command-hook `CmdqFollowup`, background timer map, and drain future |
| `cmdq_item` (`ContextRef` / `ContextSource`) | built by `cmdq_get_command` / `cmdq_get_callback_owned` | one command or callback plus `actions::Context`: client, target client, `source`/`target` `Target`, shared `Rc<ExecutionState>` |
| `cmdq_next` (`actions::queue::drive`) | `server_loop`, `server_client_lost` | polls the drain inline until the head suspends; the loop repeats until no queue made progress, then redraws |
| `QueueWake` | `LocalWake` in [queue.rs](tiri-cmds/src/cmd/queue.rs) | re-polls the drain inline on wake, never through `tiri-rt`, so waits can complete after runtime shutdown |
| `cmd_exec_fn` | 89 entries in `tiri-cmds/src/entries/` | sync `fn(&cmd, &Rc<cmdq_item>) -> TaskOutcome` (`CmdOutcome`) |
| `TaskOutcome::Suspend(fut)` | 10 entry files, `tiri_core::wait`, `tiri_core::cfg`, `window_pane` stdin wait, `CmdqFollowup` | continuation holding a `Weak<cmdq_item>` (or `Completion`), re-acquired in entries with `cmdq_resumed` |
| `Completion` (`CmdqWaiter` / `queue::wait`) | [actions.rs](tiri-core/src/actions.rs), used by `wait`, pane waits, `display-menu`, `split-window`, `cfg` | oneshot sender (+ optional sessionless client exit status) that resumes a suspended item |
| `CmdqFollowup` / `cmdq_defer` (`queue::defer`) | 7 entry files (`source-file`, `set-option`, `send-keys`, `run-shell`, `if-shell`, `confirm-before`, `command-prompt`), `cmdq_insert_hook`, `server_client_default_command` | post-item follow-up chain (`cmdq_insert_after` is already gone) |
| callback items (`queue::callback`, `queue::error_task`) | `server_client`, `cfg`, `bindings`, `source-file`, tests | sequencing non-command steps behind commands (`server_client_command_done`, `cfg_done`, `cfg_client_done`, key callbacks, read-only / parse errors) |

Details that constrain the design:

- `TaskOutcome::Error` drops the remaining items of the same group
  (`cmdq_take_same_group`), both in `cmdq_list` and within a `CmdqFollowup`.
  `Stop` is handled exactly like `Normal`.
- Nothing consumes a command's result. Errors are reported by side effect
  (`cmdq_error` / `actions::queue::report_error` to the client, status line, or
  `cfg` causes).
- `cmdq_running()` (`actions::queue::running()`) reads the **global** queue only,
  and returns nothing while the head is `HeadState::Suspended`, including while
  its continuation runs (`HeadState::Resumed` briefly re-exposes it only during
  post-park finish).
- `events_fire` dispatches synchronously, so non-command event hooks enqueue onto
  the global queue while the firing command is active; `after-<name>` and
  `command-error` use `hooks_fire_command` (`EventPayloadIdentity::CommandHook`),
  which collects hook tasks onto `q.hooks` while still firing the event bus for
  other sinks (`control_notify`, `wait-for -E`).
- `cmdq_finish_active` distinguishes unsuspended follow-ups (`!was_parked`, where
  `q.hooks` runs *before* `CmdqFollowup` items) from genuinely parked suspensions
  (`was_parked`, where continuation follow-ups run *before* trailing `q.hooks`).
- Context handles still leak widely as read-only views: `&dyn ContextSource` /
  `ContextWeak` across `tiri-core` (`format`, `wait`, `hooks`, `cfg`,
  `window_pane`, UI modes, `server_acl`, `job`, `control`, `menu`, `spawn`,
  `arguments::number`) and `&Rc<cmdq_item>` / `Weak<cmdq_item>` inside
  `tiri-cmds` (`cmd::find`, `cmd::parse`, `arguments::commands`, `entries/*`).

## Target interface

Inside `tiri-cmds` (and exposed across `tiri_core::actions::CommandEngine` over
`ProgramRef` and `Target`):

```rust
pub fn run(commands: ProgramRef, context: ExecContext) -> impl Future<Output = CmdResult> + 'static;

pub struct ExecContext {
    pub client: ClientWeak,
    pub current: Target,
    pub event: Option<key_event>,
    pub flags: c_int,
    pub formats: Option<Box<format_tree>>,
}

pub type CmdResult = Result<(), CmdFailed>;
pub struct CmdFailed;
```

*(Naming note: `tiri_core::actions::Context` currently holds per-item fields
plus `Option<Rc<ExecutionState>>`. Once `ContextRef` / `Operation` and
`ExecutionState` are deleted, `ExecContext` can take the plain name `Context`.)*

`ExecContext` replaces the queue owner argument plus `actions::queue::new_state`,
`cmdq_copy_state`, and `ExecutionState::add_format(s)`. `CmdFailed` is a marker:
the error was already reported, as today.

And `tiri_core::actions::CommandEngine` shrinks from 15 fields to 5:

```rust
pub struct CommandEngine {
    pub parse: fn(&[u8], ParseOrigin<'_>) -> Result<ProgramRef, CString>,
    pub parse_argv: fn(&[CString]) -> Result<ProgramRef, CString>,
    pub command_names: fn() -> Vec<&'static CStr>,
    pub run: fn(ProgramRef, Context) -> Pin<Box<dyn Future<Output = CmdResult>>>,
    pub running_nohooks: fn() -> bool, // plus ambient monitor-hook view if still needed after audit
}
```

### Context ownership

- The outer `run` future owns one `Context`. Commands in that run borrow it
  sequentially through `Invocation<'ctx>`, which holds `&'ctx mut Context` along
  with per-invocation `target_client`, `source`, `target`, and guard metadata
  (`number`, `time`). Changes to `current` and `formats` remain visible to
  subsequent commands.
- Follow-ups that share `item.state` (`Rc<ExecutionState>`) today borrow that
  same context when they execute via `Tail`. They neither capture an owned
  context nor retain an invocation.
- Hook sequences and other paths that create or copy state today (`new_state`,
  `cmdq_copy_state`) receive an independently owned context with the same
  initialization semantics. Copying must preserve today's treatment of current
  target, event, flags and formats; it is not a blanket clone of every field.
  Within each such sequence, commands borrow its context sequentially.
- Detached background work owns an explicitly constructed context and holds
  weak client handles across awaits. It does not borrow the foreground run.
- The final design has no `ExecutionState` (`cmdq_state`), `Rc<ExecutionState>`,
  `ContextRef`, `Operation`, or state-sharing `RefCell`. Shared access during
  sequential execution uses ordinary borrows; independently scheduled work owns
  independent state. Audit existing sharing and copying sites before the
  ownership cutover.

Public `run` takes ownership to produce a `'static` future for the lane. Inside
the runner, one private list-execution function borrows `&mut Context`; both
`run` and tails use that implementation. This is an ownership boundary, not a
second public command API.

The sequencer is generic and lives in `tiri-core/src/reactor`:

```rust
pub struct Lane { /* DrainSlot + QueueWake + VecDeque<BoxFuture> */ }
impl Lane {
    pub fn new() -> Self;
    pub fn push(&mut self, work: impl Future<Output = ()> + 'static);
    pub fn drive(owner: Option<&ClientRef>) -> u_int;
}
```

`Lane` is today's `DrainSlot` + `QueueWake` + `cmdq_poll` + `cmdq_next`, moved
nearly verbatim into `tiri-core/src/reactor`, with a `VecDeque` of boxed futures
instead of `Rc<cmdq_item>` and owned directly by `Client` (replacing
`Option<Box<dyn WorkQueue>>`) and a server thread-local. Dropping it drops, and
so cancels, everything pending.

*(Note: an unwired draft exists at [tiri-rt/src/mio/queue.rs](tiri-rt/src/mio/queue.rs)
that spawns a drain task onto `mio::Handle`. Do not use `mio::Handle::spawn` for
`Lane`: `QueueWake` intentionally polls inline on `LocalWake` without touching
`mio::Runtime` so wakes after runtime shutdown still run the continuation body,
as tested by `send_after_runtime_shutdown_runs_the_body_and_leaves_dispatch_pending`.
Remove the unused `tiri-rt/src/mio/queue.rs` when landing `Lane`.)*

Callback items disappear into ordinary code after an `.await`:

```rust
let client_weak = ClientRef::downgrade(&client);
client.lane_mut().push(async move {
    let _ = actions::run(commands, context).await;
    if let Some(client) = client_weak.upgrade() {
        server_client_command_done(&client);
    }
});
```

### What a command body sees

```rust
pub type Tail = Box<
    dyn for<'ctx> FnOnce(&'ctx mut Context)
        -> Pin<Box<dyn Future<Output = ()> + 'ctx>>,
>;
pub struct ExecOutcome {
    pub result: CmdResult,
    pub tail: Option<Tail>,
}
pub type cmd_exec_fn =
    for<'a, 'ctx> fn(&'a cmd, &'a mut Invocation<'ctx>)
        -> Pin<Box<dyn Future<Output = ExecOutcome> + 'a>>;

impl Invocation<'_> {
    pub fn client(&self) -> Option<ClientRef>;
    pub fn target_client(&self) -> Option<ClientRef>;
    pub fn source(&self) -> &Target;
    pub fn target(&self) -> &Target;
    pub fn context(&self) -> &Context;
    pub fn context_mut(&mut self) -> &mut Context;
    pub fn print(&self, ...);
    pub fn error(&self, ...);
}
```

- `Invocation` is a plain struct owned by the runner's frame, borrowing its
  context. It has no shared ownership wrapper. `cmdq_resumed` and the "do not
  retain the item across a poll" assertion go away because the borrow checker
  enforces it. Client and target references remain nonowning handles.
- One signature for all 89 entries. The ~78 sync ones wrap their body in
  `Box::pin(async move { .. })`. A separate sync signature would be a second API
  for the same use case.

### Execution order

Today, for a command that completes without parking across a poll,
`cmdq_fire_command` and `cmdq_exec_item` / `CmdqFollowup` produce:

1. `%begin` guard
2. body, which may return follow-ups F (`CmdqFollowup`)
3. on success, the `after-<name>` event (`hooks_fire_command` collects H onto
   `q.hooks`). On failure, the `command-error` event instead.
4. `%end` or `%error` guard
5. H runs, then F (`!was_parked` in `cmdq_finish_active`), then the rest of the
   group (skipped if the body failed)

For a non-suspending command, the runner expresses this as one chain. The
sketch below shows context borrowing for tails, not the suspension protocol:

```rust
async fn run_one(command: &cmd, invocation: &mut Invocation<'_>) -> CmdResult {
    guard(invocation, c"begin");
    let outcome = (command.entry.exec)(command, invocation).await;
    let hooks = fire_epilogue_event(command, invocation, &outcome.result);
    guard(invocation, if outcome.result.is_ok() { c"end" } else { c"error" });
    hooks.await;
    if let Some(tail) = outcome.tail {
        tail(invocation.context_mut()).await;
    }
    outcome.result
}
```

Suspending commands must preserve today's boundary: `after-<name>` hooks are
collected and the closing `%end` guard is emitted when the body suspends, before
its continuation waits, even if that continuation completes immediately. When a
suspended head actually parks (`was_parked`), follow-ups produced by the
continuation execute *before* the already-collected `after-<name>` hooks
(`cmdq_finish_active(_, false)`), whereas an unsuspended follow-up runs *after*
`after-<name>` hooks (`cmdq_finish_active(_, true)`). A continuation failure
does not emit another closing guard, fire `command-error` in place of the
already-collected after-hook, or drop the remaining commands of its group.
Awaiting the entire body before the epilogue, or always executing hooks before
tails regardless of suspension, would change behavior and is excluded. Implement
an explicit internal suspension boundary rather than inferring suspension from
the first `Poll::Pending`. The phase state, hook storage and guard bookkeeping
stay private to `cmd`; external callers only await `cmd::run`.

A body that used `CmdqFollowup` / `cmdq_defer` returns its tail instead of
building `Rc<cmdq_item>` tasks:

```rust
let status = job.await;
let branch = if status.success() { then_branch } else { else_branch };
ExecOutcome {
    result: Ok(()),
    tail: Some(Box::new(move |context| Box::pin(async move {
        let _ = run_list(branch, context).await;
    }))),
}
```

- The tail is a callable that produces a future when given `&mut Context`,
  rather than a future that owns or captures the context. It can perform more
  than commands: `source-file` runs every loaded list and then its completion
  step. Order inside a tail is plain sequential code.
- A failing tail affects only its own groups, as follow-up items do today. The
  outer command's result is fixed before the tail runs.
- The tail is independent of the result. A body that fails after scheduling
  follow-ups (`source-file` with a bad glob) keeps those follow-ups today, so
  the runner awaits the tail on failure too; failure only skips the rest of
  the group.
- A returned tail owns its captures and must not borrow the `Invocation` or
  capture the run's context. Once the body and epilogue release their borrows,
  the runner lends its context to the tail. The borrow lasts until the tail's
  future completes or is dropped, then the rest of the list uses the same
  context. No `Rc<ExecutionState>` is needed.

### Hooks

- `hooks.rs` already lives in `tiri-core`. `after-<name>` and `command-error`
  already call `hooks_fire_command` (using `EventPayloadIdentity::CommandHook`
  so `hooks_event_cb` skips them while other event sinks still fire) and return
  collected hook tasks directly to the runner, which awaits them in the epilogue.
- Other events (`hooks_event_cb`) and monitor hooks (`hooks_monitor_hook_cb`)
  already enqueue onto the global queue; they become pushes of `actions::run(..)`
  onto the server `Lane`.
- `actions::queue::running()` (`cmdq_running()`) remains needed in `hooks.rs`
  for two things: the `NO_HOOKS` check (in `hooks_event_cb` and
  `hooks_monitor_hook_cb`) and ambient context (`event`, formats, and parse-error
  reporting) when `hooks_monitor_hook_cb` collects an expanded `@` or array
  hook. Keep a narrow ambient accessor on `cmd` set only while a global-lane
  command is in `Running` / `Resumed` (hidden while `Suspended`), matching
  today's visibility rule.

## Crate boundary

Following [plan-tiri-modules.md](plan-tiri-modules.md) (Alternative 1, now
landed):

- `tiri-core` owns sessions, windows, panes, terminal state, UI modes, options,
  events, `bindings` tables and dispatch, `hooks`, `cfg`, `wait` channels,
  `reactor` (`Lane`), and the `actions` integration seam (`Program`,
  `CommandEngine`).
- `tiri-cmds` owns `cmd`, `cmd_list`, `cmd_entry`, command parsing (over
  `tiri-cmdparse`), target-language lookup (`cmd::find`), the 89 concrete
  command entries, and command execution (`tiri_cmds::ENGINE`).
- `tiri` owns the CLI entrypoint, default keybinding scripts
  (`src/default_keys.rs`), startup wiring (`tiri::initialize`), and cross-crate
  integration tests (`tests/engine_wiring.rs`, `tests/lazy_client_queue.rs`,
  `tests/command_suspension.rs`).

Core depends on neither `tiri-cmds` nor `tiri-cmdparse`. Model storage holds
`ProgramRef` and `Lane`, never concrete command ASTs or `cmdq_*` queues. As
queue APIs disappear in the phases below, delete their `CommandEngine` callbacks
and `actions::queue` wrappers in the same commit.

## Module boundary

```
tiri-cmds/src/cmd/
  mod.rs         run, Context, CmdResult, CmdFailed, parse and print helpers
  run.rs         list -> future: target resolution, guards, after-hook, group error rule, tails
  invocation.rs  Invocation, print, error
  find.rs parse.rs core.rs
tiri-cmds/src/entries/ unchanged layout, new async signature
tiri-core/src/reactor/lane.rs   Lane
```

| Removed | Replaced by |
| --- | --- |
| `cmdq_item`, `ContextRef`, `ContextWeak`, `ContextSource`, `Operation` | `Invocation<'ctx>` (private to a run / borrowed by helpers) |
| `cmdq_list`, `cmdq_new`, `WorkQueue` | `reactor::Lane` |
| `cmdq_append`, `cmdq_get_command`, `actions::queue::{enqueue, program_task}`, unused `cmd_parse_and_append` | lane-owned async block awaiting `cmd::run(..)` / `actions::run(..)` |
| `cmdq_get_callback_owned`, `cmdq_get_error`, `Callback`, `actions::queue::{callback, error_task}` | code after `.await`, or a plain async block on the lane |
| `CmdqFollowup`, `cmdq_followup`, `cmdq_defer`, `actions::queue::defer` | `Tail` returned in `ExecOutcome` |
| `cmdq_next`, `actions::queue::drive` | `Lane::drive` |
| `cmdq_running`, `actions::queue::running` | narrow ambient accessor for `NO_HOOKS` and monitor-hook expansion |
| `TaskOutcome` (`Normal`, `Error`, `Stop`, `Suspend`), `cmdq_resumed` | `CmdResult` / `ExecOutcome` and `.await` inside the body |
| `cmdq_wait`, `Completion` (`CmdqWaiter`), `actions::queue::wait` | `tiri_rt::oneshot` awaited in the body; async wait helpers on `wait` / `window_pane` |
| `ExecutionState` (`cmdq_state`), `Rc<ExecutionState>`, `new_state`, `cmdq_copy_state` | owned `Context`, borrowed sequentially by invocations and tails |
| `cmdq_get_*`, `cmdq_print`, `cmdq_error`, `cmdq_guard`, `actions::queue::{print, report_error, guard}` | `Invocation` methods (and narrow helper trait/borrow for `tiri-core` callers) |
| `cmdq_background`, `cmdq_cancel_background`, `actions::queue::cancel_background` | server-owned timer/task set in `tiri-core/src/reactor` |

## Implementation risks and audits

1. **One owner per free operation.** Dropping a command future is the
   cancellation path, which fits the project's direction (Drop over explicit
   free functions). The constraint from AGENTS.md is that a resource is
   released by exactly one entity. For each resource a command awaits (jobs,
   file reads, status and pane prompts, menus and popups, pane waits,
   `wait-for` channels/event sinks), check which case applies:
   - release already happens in Drop: nothing to do (popups and `oneshot`-backed
     waits already cancel on drop);
   - release happens in a dedicated free function: migrate it fully to Drop
     and delete the free function in the same change, so that dropping the
     future is sufficient and there is no second path;
   - release is split between the two today: fix that first (e.g. check
     `WAIT_EVENT_ITEMS` sink cleanup in `tiri_core::wait` and `run-shell` /
     `if-shell` jobs).
2. **Parked-forever continuations and post-shutdown wakes.** Several
   continuations do `pending().await` when their source is freed without
   completing, to match tmux never resuming the item. That blocks the lane until
   the client dies. Preserve this behavior; resolving to `Err(CmdFailed)` would
   change it. Likewise, `Lane`'s `LocalWake` must poll inline without routing
   through `mio::Runtime` so wakes after runtime shutdown still run the body.
3. **Reference cycles.** `if-shell`, `run-shell`, `load-buffer` and
   `source-file` keep a strong `ClientRef` in data owned by the continuation,
   which the client's own queue owns. The same shape carries over. Rule to
   adopt: hold `ClientWeak` across `.await`, upgrade per segment.
4. **`RefCell` borrows across `.await`.** A new hazard class once bodies are
   async. Turn on `clippy::await_holding_refcell_ref` before converting entries.
5. **Stored `Weak` items.** `format_tree.item` (`ContextWeak`),
   `spawn_context.item` (`ContextWeak`), `wait_event_item.item` (`ContextWeak`),
   and `cmd_parse_input.item` (`Weak<cmdq_item>`) store nonowning context
   handles. Audit each: synchronous callers (`spawn_context`, `cmd_parse_input`)
   become `Option<&Invocation>` (or snapshot the needed fields); `format_tree`
   must snapshot what format jobs need if a `format_tree` outlives the call;
   `wait_event_item` only uses `item` as an identity key and to report/print on
   wake, which moves into the awaiting future's frame.
6. **Running-command visibility.** Preserve today's global-queue visibility,
   including hiding the item while its continuation is parked/polled. Use the
   explicit suspension boundary rather than treating the entire first async poll
   as visible: that poll may also execute continuation code. Extending
   visibility to continuation polls changes `NO_HOOKS` and monitor-hook
   context and is excluded.
7. **Tests.** Roughly 100 `cmdq_next` / `actions::queue::drive` call sites across
   `tiri-cmds`, `tiri-core`, and `tests/` (`engine_wiring.rs`,
   `lazy_client_queue.rs`, `command_suspension.rs`) assert returned item counts
   and build callback items by hand. Rewrite them against `Lane::drive` or by
   polling `cmd::run` directly. This is the largest mechanical cost after the
   entry signature change.
8. **Error reporting.** Keep `CmdFailed` as a marker and preserve side-effect
   reporting, including its destination and timing. Redesigning error routing
   is outside this refactor.
9. **Context ownership audit.** Classify each existing `ExecutionState` user as
   borrowing the enclosing run's context or owning a separately initialized
   context. Include `hooks_collect_event`, `hooks_run`, `cfg`, key bindings,
   prepared commands (`choose-tree` / `display-panes`), and detached work
   (`run-shell -b`). Preserve which mutations reach subsequent commands and
   which paths start with fresh formats or flags. Do not reintroduce shared
   command-state ownership to keep retained observers alive.
10. **Suspension mechanism.** Preserve the exact epilogue timing and `was_parked`
    hook-vs-followup ordering from `cmdq_finish_active` before removing
    `TaskOutcome::Suspend` and `CmdqFollowup`.

## Phasing

Each phase compiles and passes tests on its own. Per the no-overlapping-API
rule, a phase that introduces a replacement also migrates every user and
deletes the old API.

1. **`Lane` and background timers.** Extract the drain driver (`DrainSlot` +
   `QueueWake` + `cmdq_poll`) from `tiri-cmds/src/cmd/queue.rs` into
   `tiri-core/src/reactor::Lane` holding boxed futures, and move
   `cmdq_background` / `cmdq_cancel_background` to reactor timers. Delete the
   unwired `tiri-rt/src/mio/queue.rs` draft, dead `cmd_parse_and_append`,
   `WorkQueue`, `queue_new`, `queue_drive`, and `queue_cancel`. `cmdq_append`
   becomes an internal item queue driven on `Lane` (or per-group futures on
   `Lane`). No behavior change.
2. **`Context` and `Invocation`.** Audit state sharing, copying, and retained
   `ContextWeak` / `Weak<cmdq_item>` observers (`format_tree`, `spawn_context`,
   `cmd_parse_input`, `wait_event_item`). Replace `&Rc<cmdq_item>` and
   `&dyn ContextSource` / `ContextRef` / `Operation` with borrowed
   `Invocation` / `Context` access in `cmd_exec_fn` and across `tiri-core`
   helpers (`format`, `wait`, `window_pane`, UI modes, `server_acl`, `job`,
   `control`, `menu`). Still sync, still `TaskOutcome`. Do not leave a parallel
   context API.
3. **`Tails` and epilogue runner.** Replace `CmdqFollowup` / `cmdq_defer`
   (`queue_defer`) and `q.hooks` with `ExecOutcome { result, tail }` and
   runner-awaited `hooks_fire_command` tasks, preserving the suspension boundary
   and `was_parked` ordering between hooks and follow-ups. Delete `CmdqFollowup`,
   `cmdq_followup`, `cmdq_defer`, and `CommandEngine::queue_defer`.
4. **Async `cmd_exec_fn`.** Enable `clippy::await_holding_refcell_ref` and
   complete the Drop cancellation audit from risk 1. Switch `cmd_exec_fn` to
   return a future, convert the 10 suspending entry files plus `tiri_core::wait`
   (and remaining `TaskOutcome::Suspend` callbacks in `cfg` and `window_pane`)
   to straight-line `.await`, and delete `TaskOutcome::Suspend`, `cmdq_resumed`,
   `cmdq_wait`, `Completion`, and `CommandEngine::queue_wait`.
5. **`cmd::run` cutover.** Expose `cmd::run` (`CommandEngine::run`), migrate all
   external enqueue and callback sites (`server_client`, `cfg`, `hooks`,
   `bindings`, `window_pane`, tests) to lane-owned async blocks awaiting
   `actions::run(..)`, and move state ownership into run frames lending
   `&mut Context` to invocations and tails. Delete `ExecutionState`, `new_state`,
   `cmdq_copy_state`, `Callback`, and all remaining `cmdq_*` / `actions::queue`
   plumbing in the same cutover, leaving `CommandEngine` with its final
   5-function surface.

Suspension compatibility is a prerequisite for phases 3 and 4;
`tests/command_suspension.rs`, `tests/engine_wiring.rs`, and the control-mode
tests are the guard.
