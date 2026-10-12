# Trait interfaces for shared models

Session, Window, WindowPane and Client retain their existing
`Rc<UnsafeCell<T>>` allocations and explicit lifecycle protocols. Their fields,
concrete constructors and implementation helpers are private to their respective
owner modules. External code uses the holder traits in
[session/api.rs](../src/session/api.rs),
[window/api.rs](../src/window/api.rs),
[window_pane/api.rs](../src/window_pane/api.rs) and
[server_client/api.rs](../src/server_client/api.rs).

## Boundary and method selection

A trait method may return an existing holder. Its consumer then uses that
holder's trait; it must not project the underlying record. Each implementation
may access its own model only. Window and pane are sibling modules, so neither
inherits access to the other's state.

Expose the decisions and operations a real consumer needs. Keep the bookkeeping
that implements an operation inside its owner. Combine related writes when they
preserve one invariant, and move existing algorithms into their owner when they
manage its storage. Keep cross-entity orchestration explicit through the other
traits. A getter for every field or a whole-model borrower would recreate the
representation boundary.

| Entity | External capabilities | Work kept inside the entity |
| --- | --- | --- |
| Session | Creation and lookup, identity, window navigation and membership, attachment/status decisions, configuration, grouping, lifetime | Index keys, current/last-used links, group synchronization, activity/lock timers, attached counts, status caches |
| Window | Creation and lookup, pane ordering and membership, strip arrangement, sizing, focus, redraw, configuration, lifetime | Strip arrangement (the only writer of pane geometry), pane history, pending resize state, alerts, scene invalidation |
| WindowPane | Creation and lookup, geometry, input and modes, screen copying and rendering, output consumption, configuration, lifetime | Parser, screen selection, mode stack, stream offsets, pipe/process state, resize/sync queues, scrollbar state |
| Client | Creation and registry traversal, attachment, focus/size decisions, input and output, redraw, UI installation, lifetime | TTY, status/prompt and control state, attachment history, redraw completion, timers, transport cleanup |

SessionIndex and WindowIndex expose operations on the existing independent index
headers. They preserve weak entry/index identities and retirement on the last
removal. Supporting components such as winlinks, screens and input contexts
retain their own APIs; component APIs do not grant access to a core
model's record.

## Borrow and callback contract

Callbacks retain an Rc or Weak identity and access the model through its trait
when they execute. Copy values or retain independent owners before calling
another model, formatting, resizing or dispatching a notification. End every
model/component loan before such calls and borrow again afterward when needed.
Preserve whether the old code captured its target before a callback or looked up
a fresh target afterward; these behaviors can differ when callbacks reparent or
replace objects.

Associated guard types let callers borrow existing components. Current
implementations return references. The API permits future mapped Ref/RefMut
guards from one whole-model RefCell; it does not require a RefCell per field.
Component-only closure adapters obey the same bounded-loan contract: no reentry,
callbacks, parent replacement, freeing or escaped references/pointers while the
loan remains active. These are unsafe caller obligations in the current storage
implementation.

Pane rectangles leave the Window only as copied geometry. Options inheritance
resolves owning-scope identities and carries owned values across parser/format
calls. Rendering extracts and restores the exact
screen owner before terminal calls; it does not lend a pane screen through a raw
result. Screen-write contexts keep their existing explicit stop requirement.

Terminal output helpers retain Client holders and open brief terminal component
loans for each read or update. There is no TerminalOutput view. Input decoding
uses SegmentedBuf's contiguous bytes directly, extracts owned reply data before
dispatch, and retains no copied Rc byte cache. Typed overlay state groups the
installed callbacks, generation and popup payload; callback extraction and
restoration preserve reentrant replacement and explicit free ordering.

## Lifetime and destruction

A holder keeps allocation memory alive, not necessarily a logically registered
object or its resources. Keep explicit release, free, destroy and cancellation
operations at their established points and preserve their notification order.
Temporary scope upgrades are distinct from transferred logical owners. In
particular, a bounded Session scope upgrade must not enqueue a deferred Session
release merely because the temporary Rc goes out of scope.

Session and Client release operations retain the existing deferred event-loop
cleanup. Window release runs close callbacks while an owner remains live, then
checks whether callbacks retained it. Pane destroy/stream/parser/mode cleanup,
overlay free, monitor_destroy and timer/task cancellation remain explicit. The
migration adds no Drop responsibility for core-model resource cleanup.

Tests use production model operations and preserve explicit teardown.
Assertions that inspect private index, resize or cache storage live in their
actual owner.

## Existing types and owned results

Reuse options, environ, termios, spawn_context, screen, mode/prompt descriptors,
byte buffers, format_tree and FormatValue. Geometry tuples and copied decision
records carry only values needed by callers. PaneScrollbar groups the related
appearance and slider geometry needed across drawing calls without allocation.
Resize snapshots and typed identities prevent component loans from escaping;
their definitions are reviewed in the migration report.

Monitor sets have one RefBox owner in their control client or hook. Operations
and timers carry refbox::Weak observers, release state borrows before formatting
or callbacks, and stop if the owner is destroyed. monitor_destroy consumes the
owner, cancels its timer, clears records, and expires observers before releasing
Session ownership and callback captures. Runtime task registrations still use Rc.

## Validation

`tests/model_trait_boundary.rs` checks all four field and inherent-helper
visibilities, legacy helper exports, external storage types, representation casts
and component/callback result contracts. Compiler field inventories include
integration and unit test targets. Client, Session, Window and pane private-storage
probes also restrict storage get() access in disposable source copies; they catch
inferred/unused projections and storage-specific generic helpers.

These probes check encapsulation, not dynamic borrow correctness. The workspace
behavior tests and isolated live terminal smokes exercise reentry, replacement,
cancellation, rendering, layouts and explicit teardown. Production storage
remains UnsafeCell under the project's manually reasoned lifetime and borrow
rules.
