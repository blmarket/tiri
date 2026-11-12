# Plan: the command system as a Future

Status: feasibility study, nothing implemented.

Constraint: tmux compatibility is a goal, control mode especially. Guard
blocks, hook order and which follow-ups run must stay as they are today.

## Verdict

Feasible, and most of the hard part already exists: each queue is already driven
by one future (`cmdq_drain` in [queue.rs](src/cmd/queue.rs)), and waiting
commands already hand back a future (`CmdOutcome::Suspend`). The refactor is an
inversion of the interface, not a new execution model.

It cannot be *only* a future. Three things have to survive in some form:

1. **An ordered owner for sync callers.** Key input, imsg dispatch, control
   lines, config loading and hooks all start commands from sync callbacks. They
   cannot `.await`, and per-client FIFO order is observable (a waiting
   `run-shell` holds back later keys). Something must own the futures and poll
   them in order. That is a generic sequencer, not a command concept.
2. **A tail, not a nested await inside the body.** `cmdq_insert_after` means
   "run this after me, before the rest of the queue". Awaiting the inserted
   commands inside the body would run them before the outer command's
   after-hook and inside its control-mode `%begin`/`%end` block. Instead the
   body returns its tail future and the runner awaits it after the epilogue,
   so the whole sequence is one `.await` chain (see "Execution order").
3. **A small ambient current command for hooks.** `hooks.rs` reads
   `cmdq_running()` for the `NOHOOKS` check and for monitor hooks.

With those three kept small, every external caller reduces to
`lane.push(cmd::run(commands, context))`, or `cmd::run(..).await` from async
code, and dropping the future cancels.

## How it works today

| Piece | Where | Role |
| --- | --- | --- |
| `cmdq_list` | one per client, one global thread-local | FIFO of `Rc<RefCell<cmdq_item>>` plus the drain future |
| `cmdq_item` | built by `cmdq_get_command` / `cmdq_get_callback_owned` | one command or one callback, plus its context: client, target client, source/target `cmd_find_state`, shared `cmdq_state` |
| `cmdq_next` | `server_loop`, `server_client_lost` | polls the drain inline until the head suspends; the loop repeats until no queue made progress, then redraws |
| `QueueWake` | `LocalWaker` | re-polls the drain inline on wake, never through the runtime |
| `cmd_exec_fn` | 89 entries | sync `fn(&cmd, &Rc<RefCell<cmdq_item>>) -> CmdOutcome` |
| `CmdOutcome::Suspend(fut)` | 11 entry files | continuation holding a `Weak` item, re-acquired with `cmdq_resumed` |
| `CmdqWaiter` | `wait-for`, pane waits, `cfg` | oneshot sender that resumes a suspended item |
| `cmdq_insert_after` | 5 entry files, hooks, key bindings, cfg, server_client | composition by queue mutation |
| callback items | ~36 lines outside `src/cmd` | sequencing arbitrary code behind commands (`server_client_command_done`, `cfg_done`, key callbacks) |

Details that constrain the design:

- `CmdOutcome::Error` drops the remaining items of the same group
  (`cmdq_remove_group`). `Stop` is handled exactly like `Normal`.
- Nothing consumes a command's result. Errors are reported by side effect
  (`cmdq_error` to the client, status line, or cfg causes).
- `cmdq_running()` reads the **global** queue only, and returns nothing while
  the head is suspended, including while its continuation runs.
- Repeated `cmdq_insert_after` on one anchor reverses order, so callers thread
  the returned `Weak` as the next anchor (`source-file`, hooks, `send-keys`).
- `events_fire` dispatches synchronously, so hooks run inside the command that
  fired the event.
- The item handle leaks widely as read-only context: `format`, `cmd::find`,
  `arguments::commands`, `window_pane` (capture, start_input, pipe, spawn),
  mode init functions, `server_acl`, `job`. About 100 mentions outside `src/cmd`.

## Target interface

```rust
pub fn run(commands: Rc<cmd_list>, context: Context) -> impl Future<Output = CmdResult> + 'static;

pub struct Context {
    pub client: ClientWeak,
    pub current: cmd_find_state,
    pub event: Option<key_event>,
    pub flags: c_int,
    pub formats: Option<Box<format_tree>>,
}

pub type CmdResult = Result<(), CmdFailed>;
pub struct CmdFailed;
```

`Context` replaces the queue owner argument plus `cmdq_new_state` /
`cmdq_copy_state` / `cmdq_add_format(s)`. `CmdFailed` is a marker: the error
was already reported, as today.

The sequencer is generic and lives outside `cmd`:

```rust
pub struct Lane;
impl Lane {
    pub fn new() -> Self;
    pub fn push(&mut self, work: impl Future<Output = ()> + 'static);
    pub fn drive(owner: ...) -> u_int;
}
```

`Lane` is today's `DrainSlot` + `QueueWake` + `cmdq_poll` + `cmdq_next`, moved
nearly verbatim, with a `VecDeque` of boxed futures instead of items. One per
client, one for the server. Dropping it drops, and so cancels, everything
pending.

Callback items disappear into ordinary code after an `.await`:

```rust
client.lane().push(async move {
    let _ = cmd::run(commands, context).await;
    server_client_command_done(&client);
});
```

### What a command body sees

```rust
pub type Tail = Pin<Box<dyn Future<Output = ()>>>;
pub struct ExecOutcome {
    pub result: CmdResult,
    pub tail: Option<Tail>,
}
pub type cmd_exec_fn =
    for<'a> fn(&'a cmd, &'a mut Invocation) -> Pin<Box<dyn Future<Output = ExecOutcome> + 'a>>;

impl Invocation {
    pub fn client(&self) -> Option<ClientRef>;
    pub fn target_client(&self) -> Option<ClientRef>;
    pub fn source(&self) -> &cmd_find_state;
    pub fn target(&self) -> &cmd_find_state;
    pub fn context(&self) -> &Context;
    pub fn print(&self, ...);
    pub fn error(&self, ...);
}
```

- `Invocation` is a plain struct owned by the runner's frame. No `Rc`, no
  `RefCell`, no `Weak`. `cmdq_resumed` and the "do not retain the item across a
  poll" assertion go away because the borrow checker enforces it.
- One signature for all 89 entries. The 78 sync ones wrap their body in
  `Box::pin(async move { .. })`. A separate sync signature would be a second API
  for the same use case.

### Execution order

Today, for one command, `cmdq_fire_command` and the queue produce:

1. `%begin` guard
2. body, which may insert follow-ups F after the item
3. on success, the `after-<name>` event; hooks insert H directly after the
   item, so ahead of F. On failure, the `command-error` event instead.
4. `%end` or `%error` guard
5. H runs, then F, then the rest of the group (skipped if the body failed)

The runner keeps exactly this as one chain. Nothing is scheduled; position in
the chain is the order:

```rust
async fn run_one(command: &cmd, invocation: &mut Invocation) -> CmdResult {
    guard(invocation, c"begin");
    let outcome = (command.entry.exec)(command, invocation).await;
    let hooks = fire_epilogue_event(command, invocation, &outcome.result);
    guard(invocation, if outcome.result.is_ok() { c"end" } else { c"error" });
    hooks.await;
    if let Some(tail) = outcome.tail {
        tail.await;
    }
    outcome.result
}
```

A body that used `cmdq_insert_after` returns its tail instead of awaiting it:

```rust
let status = job.await;
let branch = if status.success() { then_branch } else { else_branch };
ExecOutcome {
    result: Ok(()),
    tail: Some(Box::pin(async move {
        let _ = cmd::run(branch, context).await;
    })),
}
```

- The tail is a future, not a command list, because some tails are more than
  commands: `source-file` runs every loaded list and then its completion step.
  Order inside a tail is plain sequential code, so the anchor threading
  (`cdata.after`, `new_after`) goes away.
- A failing tail affects only its own groups, as inserted items do today. The
  outer command's result is fixed before the tail runs.
- The tail is independent of the result. A body that fails after inserting
  follow-ups (`source-file` with a bad glob) keeps those follow-ups today, so
  the runner awaits the tail on failure too; failure only skips the rest of
  the group.
- The tail must not borrow the `Invocation`. It shares the `cmdq_state`, as
  inserted items share `item.state` today, so that `Rc` stays.

### Hooks

- The `after-<name>` and `command-error` events are fired by the runner, which
  awaits the resulting hook commands in the chain above. The payload carries a
  collector in place of `EventPayloadIdentity::QueueItem`.
- Other events fired during a body already append to the global queue; they
  become a push of `cmd::run(..)` onto the server lane.
- `cmdq_running()` remains needed for two things: the `NOHOOKS` check and
  monitor hooks, which insert after the running global item. `cmd` keeps one
  narrow accessor for that, set only during a body's first poll on the server
  lane, matching today's "hidden while suspended" rule.

## Module boundary

```
src/cmd/
  mod.rs         run, Context, CmdResult, CmdFailed, parse and print helpers
  run.rs         list -> future: target resolution, guards, after-hook, group error rule, tails
  invocation.rs  Invocation, print, error
  entries/       unchanged layout, new signature
  find.rs parse.rs core.rs
src/reactor/lane.rs   Lane
```

| Removed | Replaced by |
| --- | --- |
| `cmdq_item`, `cmdq_list`, `cmdq_new` | `Invocation` (private to a run), `Lane` |
| `cmdq_append`, `cmdq_get_command`, `cmd_parse_and_append` | `lane.push(cmd::run(..))` |
| `cmdq_get_callback_owned`, `cmdq_get_error` | code after `.await`, or a plain async block on the lane |
| `cmdq_insert_after` | the `Tail` a body returns |
| `cmdq_next` | `Lane::drive` |
| `cmdq_running`, `EventPayloadIdentity::QueueItem` | runner-awaited hook commands; one narrow accessor for `NOHOOKS` and monitor hooks |
| `CmdOutcome::{Suspend, Stop}`, `cmdq_resumed` | `.await` inside the body |
| `cmdq_wait`, `CmdqWaiter` | `tiri_rt::oneshot` awaited in the body; `pane.wait(event) -> impl Future` |
| `cmdq_new_state`, `cmdq_copy_state`, `cmdq_add_format(s)`, `cmdq_state` | `Context` |
| `cmdq_get_*`, `cmdq_print`, `cmdq_error`, `cmdq_guard` | `Invocation` methods |
| `cmdq_background`, `cmdq_cancel_background` | a server-owned task set (`reactor::task_start`) |

## Risks and open questions

1. **One owner per free operation.** Dropping a command future is the
   cancellation path, which fits the project's direction (Drop over explicit
   free functions). The constraint from AGENTS.md is that a resource is
   released by exactly one entity. For each resource a command awaits (jobs,
   file reads, status and pane prompts, menus and popups, pane waits,
   `wait-for` channels), check which case applies:
   - release already happens in Drop: nothing to do;
   - release happens in a dedicated free function: migrate it fully to Drop
     and delete the free function in the same change, so that dropping the
     future is sufficient and there is no second path;
   - release is split between the two today: fix that first.

   Dropping a queue already drops continuations today, so part of this holds,
   but it has not been audited per resource.
2. **Parked-forever continuations.** Several continuations do
   `pending().await` when their source is freed without completing, to match
   tmux never resuming the item. That blocks the lane until the client dies.
   Decide whether to keep it or resolve to `Err(CmdFailed)`.
3. **Reference cycles.** `if-shell`, `run-shell`, `load-buffer` and
   `source-file` keep a strong `ClientRef` in data owned by the continuation,
   which the client's own queue owns. The same shape carries over. Rule to
   adopt: hold `ClientWeak` across `.await`, upgrade per segment.
4. **`RefCell` borrows across `.await`.** A new hazard class once bodies are
   async. Turn on `clippy::await_holding_refcell_ref` before converting entries.
5. **Stored `Weak` items.** `format_tree.item`, `cmd_parse_input.item` and
   `spawn_context.item` store a `Weak<RefCell<cmdq_item>>`. If every use is
   inside one sync call they become `Option<&Invocation>`; `format_tree` needs
   checking since format jobs may outlive the call.
6. **Running-command accessor only during the first poll.** Faithful to
   today, but subtle. The alternative, valid during any poll, changes where
   monitor hooks land for waiting commands.
7. **Tests.** About 100 `cmdq_next` call sites in tests assert returned item
   counts and build callback items by hand. They need rewriting against
   `Lane::drive` or by polling `cmd::run` directly. This is the largest
   mechanical cost after the entry signature change.
8. **Should errors carry data?** `CmdFailed` keeps side-effect reporting.
   Returning the message would let callers decide where it goes, but that is a
   separate, larger change.

## Phasing

Each phase compiles and passes tests on its own. Per the no-overlapping-API
rule, a phase that introduces a replacement also migrates every user and
deletes the old API.

1. **Lane.** Extract the drain driver from `queue.rs` into `reactor::Lane`
   holding boxed futures. `cmdq_append` becomes a thin push of a per-item
   future. No behaviour change.
2. **Context and Invocation.** Replace `Rc<RefCell<cmdq_item>>` with
   `&mut Invocation` in the exec signature and in the external context users
   (`format`, `find`, `window_pane`, modes). Still sync, still `CmdOutcome`.
   Largest diff, purely mechanical.
3. **Tails.** Let a body return its tail and have the runner own the
   epilogue chain (hook event, guard, hook commands, tail). Migrate all
   `cmdq_insert_after` users and the after-hook path; delete
   `cmdq_insert_after` and the payload identity. Waiting bodies still use
   `Suspend` here, so their continuation yields the tail.
4. **Async exec.** Switch the signature to return a future, convert the 11
   waiting entries to straight-line `.await`, delete `Suspend`, `cmdq_resumed`,
   `cmdq_wait`, `CmdqWaiter`. Do the cancellation audit from risk 1 here.
5. **`cmd::run`.** Expose it, migrate external enqueue sites to
   `lane.push(cmd::run(..))`, fold callback items into async blocks, delete the
   remaining `cmdq_*` surface and `cmd_parse_and_append`.

Phases 1 to 3 are low risk and independently useful. Phase 4 is where behaviour
can change; `tests/command_suspension.rs` and the control-mode tests are the
guard.
