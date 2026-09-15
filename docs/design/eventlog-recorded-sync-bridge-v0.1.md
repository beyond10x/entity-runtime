# ER explicit synchronous Eventlog bridge v0.1

Status: coordinator-selected proposal for `story:eventlog-recorded-adapter-and-bridge`; complete
independent design review precedes implementation. The async Eventlog adapter, native capture,
inline attachment/rebuild and provider qualification remain dependencies. Source references use
ER17da35a7 and accepted Eventlog18322cbe. Original scoping evidence remains unchanged externally.

## Result

The bridge belongs in the proposed `entity-eventlog` IO crate, in a public `sync` module. It should
be a synchronous facade over the existing `AsyncRecordedStore` types and `entity-executor::Executor`,
not an implementation of the older synchronous `entity_store::Store` family. One dedicated OS
thread must construct and own the Tokio runtime, Eventlog provider, recorded adapter and `Registry`.
Callers exchange only closed request/result enums through a bounded queue. The worker constructs a
borrowed `Executor` for each executor request.

A current-thread Tokio runtime is sufficient and is the narrowest selection. It isolates the
provider runtime from callers outside Tokio and callers using either Tokio runtime flavor. A sync
call made from a Tokio task blocks that caller thread as any synchronous API does, but it never
enters or blocks on the caller's runtime. A call made on the bridge worker thread must refuse before
queueing, because it would deadlock the only request driver.

The queue should use fail-fast bounded admission. A request owns an atomic
`Queued -> Cancelled | Dispatched -> Completed` phase. Deadline cancellation succeeds only through
`Queued -> Cancelled`; the worker then proves it never invoked the store. After
`Queued -> Dispatched`, the worker continues the operation even if its caller stops waiting. A lost
write result at that point maps to the existing `WriteFailure::Uncertain` with the original
`BatchKey`. The bridge never aborts a dispatched write, invents a retry identity or automatically
retries it.

Startup waits for a conclusive worker handshake rather than adding a bridge timeout that could
leave an unowned provider thread. Shutdown first closes admission, then either drains or cancels
queued work, performs provider-specific retirement on the owner thread, and joins explicitly. A
shutdown deadline returns `TimedOut` while preserving the live handle and join state; it never
claims the thread stopped.

There is one startup capability gap in addition to the dependencies already named by the adapter
design. A provider created on the worker must attach the projector code to already admitted tables
without admitting, creating, recovering or running DDL. Current `register_inline` does not express
that cross-provider operation. The smallest Eventlog seam is a validate-only
`attach_inline_existing(projector)` operation, or an equivalent provider constructor contract,
which refuses absent/drifted admission and only installs the in-memory inline driver before serving.

## Source facts that constrain the bridge

The async recorded port is already object-safe, `Send + Sync`, and returns boxed `Send` futures. It
contains state load; global record, batch, subject-history and provider-complete snapshot reads; and
one ordered atomic append (`crates/entity-store/src/asynchronous/ports.rs:10-75`). `AppendRequest`
has exactly one inert empty shape and otherwise owns an optional `BatchKey` plus ordered members;
every writer must validate its public mutable fields
(`crates/entity-store/src/asynchronous/types.rs:309-415`).

The existing executor is runtime-neutral and borrows `&Registry` and
`&dyn AsyncRecordedStore` (`crates/entity-executor/src/lib.rs:166-177`). It performs identity-first
semantic recovery before current state or registry lookup, and it recovers an uncertain provider
append before returning the uncertainty (`crates/entity-executor/src/lib.rs:214-289,371-430`). The
worker can therefore own both values and create this borrowed executor inside one request; neither
borrow nor a caller-created provider crosses the thread boundary.

Existing errors already distinguish:

* `ExecutionError::Core`, `Store`, and `Write` (`entity-executor/src/lib.rs:111-152`);
* the concrete `AsyncStoreError` variants, including record/batch/revision conflicts, corruption,
  `Unreachable` and `Backend` (`entity-store/src/asynchronous/types.rs:651-778`); and
* `WriteFailure::NotCommitted(AsyncStoreError)` from `WriteFailure::Uncertain { key, cause }`
  (`entity-store/src/asynchronous/types.rs:780-808`).

Those variants must pass through unchanged when the async operation returns them. Bridge-local
admission and response failures need separate variants so queue saturation is not mislabeled as a
provider refusal.

The accepted adapter design requires a caller-supplied closed `EventlogOperationContext`, validates
it only when a new nonempty physical write remains necessary, and permits semantic recovery under
a different current context (`docs/design/eventlog-recorded-adapter-v0.1.md:145-192`). The bridge
must carry that exact value to the worker without parsing ER `recorded_at`, choosing a clock,
principal, trace or authority. This is consistent with the repository boundary that world facts
enter as caller arguments (`AGENTS.md:61-69,107-110`).

Eventlog PostgreSQL is natively async and documents why per-call sync runtimes panic within an
existing runtime (`eventlog-postgres/src/lib.rs:3-15`). File and SQLite also offload blocking work
with Tokio (`eventlog-file/src/lib.rs:48-53`; `eventlog-sqlite/src/lib.rs:87-90`). PostgreSQL has an
explicit bounded async `shutdown`, while File and SQLite expose no corresponding shutdown method;
their owner must await all bridge work and drop them on the worker
(`eventlog-postgres/src/lib.rs:197-205`, File/SQLite public constructors at
`eventlog-file/src/lib.rs:55-84` and `eventlog-sqlite/src/lib.rs:54-85`).

## Public owner and API proposal

The selected names below form one finite contract. The crate is `crates/entity-eventlog`, and
the module is `entity_eventlog::sync`:

```text
pub struct RecordedEventlogBridge                 // non-Clone owner; Send + Sync
pub struct SyncEventlogOperation<'a>               // bridge + copied operation context

pub struct BridgeConfig {
    pub queue_capacity: NonZeroU16,
}

pub enum CallWait {
    Forever,
    Until(std::time::Instant),
}

impl RecordedEventlogBridge {
    pub fn start(
        registry: Registry,
        owner: EventlogRecordedStoreOwner,
        config: BridgeConfig,
    ) -> Result<Self, BridgeStartError>;

    pub fn operation(
        &self,
        context: EventlogOperationContext,
    ) -> SyncEventlogOperation<'_>;

    pub fn load(&self, subject: &Subject, wait: CallWait)
        -> Result<Option<EntityInstance>, SyncReadError>;
    pub fn lookup_record(&self, record_id: &str, wait: CallWait)
        -> Result<Option<RecordLookup>, SyncReadError>;
    pub fn lookup_batch(&self, key: &BatchKey, wait: CallWait)
        -> Result<Option<StoredBatch>, SyncReadError>;
    pub fn history(&self, subject: &Subject, wait: CallWait)
        -> Result<SubjectHistory, SyncReadError>;
    pub fn complete_snapshot(&self, scope: &str, wait: CallWait)
        -> Result<CompleteStoreSnapshot, SyncReadError>;

    pub fn shutdown(&mut self, mode: ShutdownMode, wait: CallWait)
        -> ShutdownOutcome;
}

impl SyncEventlogOperation<'_> {
    pub fn append(&self, request: AppendRequest, wait: CallWait)
        -> Result<AppendOutcome, SyncWriteError>;
    pub fn create(&self, request: CreateRequest, wait: CallWait)
        -> Result<AppendOutcome, SyncExecutionError>;
    pub fn execute(&self, request: ExecuteRequest, wait: CallWait)
        -> Result<AppendOutcome, SyncExecutionError>;
    pub fn observe(&self, request: RecordedObservation, wait: CallWait)
        -> Result<AppendOutcome, SyncExecutionError>;
    pub fn batch(&self, key: BatchKey, actions: Vec<BatchAction>, wait: CallWait)
        -> Result<AppendOutcome, SyncExecutionError>;
}
```

`NonZeroU16` gives an explicit finite queue bound of `1..=65535` without a magic policy constant.
There is no hidden default queue, unbounded request channel, or blocking wait for queue capacity.
`try_send` either accepts one owned request or returns `QueueFull`/`Closed` while the provider has
seen nothing.

`EventlogRecordedStoreOwner` is a non-clone, opaque `entity-eventlog` value built from the caller's
provider configuration, credentials, exact authority and capture limits through feature-specific
constructors. `start` moves it to the new thread before it opens anything. Its private provider
variants may be File, SQLite or PostgreSQL, but the queue never contains construction closures or
arbitrary callbacks. The owner invokes the async adapter's eventual nonmutating open; current
Eventlog constructors and the unimplemented native-capture dependency do not by themselves prove
that open contract.

At Eventlog `18322cbe`, `register_inline` is the only public attachment operation. File can open the
journal and record a missing projection declaration during it
(`eventlog-file/src/lib.rs:914-965`). PostgreSQL calls `create_projections`, which migrates for a
schema-writing handle and validates only for its hosted DML handle
(`eventlog-postgres/src/lib.rs:957-1028`). SQLite likewise calls its table-creating projection
admission path from `register_inline` (`eventlog-sqlite/src/lib.rs:1454-1542`). A bridge start must
not call that API and merely hope admission already exists. The missing validate-only attach must:

* accept the exact projector name/specs and require them already present with the exact physical
  shape;
* perform no DDL, persistent registry write, journal recovery, catch-up or rebuild;
* install the projector and projection names in the current handle before its serving freeze; and
* refuse duplicate, missing or incompatible admission; preserve provider dirty markers and all
  existing projection-use/capture refusals without claiming readiness from structural attachment.

This follows the accepted Eventlog inline-projection-administration design. Worker startup still
requires the adapter's native capture and exact independent row comparison before returning a
ready handle. SQL has no dirty registry flag and File's marker outlives local registration; neither
fact permits a dirty or stale adapter to serve. The adapter's redacted-authority refusal remains.

This is Eventlog core/provider ownership, not queue logic. Until it and the selected nonmutating
provider/capture construction path exist, the bridge API can be implemented against a fake owner
but cannot truthfully open all three real providers.

Reads do not require operational context. Writes use the exact context stored by
`SyncEventlogOperation`; each submitted request owns a clone so the borrowed facade can return
immediately on rejection. The bridge neither validates context before semantic recovery nor shares
context between operations. Direct `append` first invokes `AppendRequest::validate` on every public
shape. Only the valid `key: None, members: []` request then returns `AppendOutcome::Empty`
synchronously without queue admission, context validation or provider IO; `key: Some(_), members:
[]` retains `WriteFailure::NotCommitted(AsyncStoreError::InvalidInput(...))`. In contrast, the
separate executor `batch(key, actions: [])` intentionally returns Empty before validating or
consuming its key (`asynchronous/types.rs:356-415`; `entity-executor/src/lib.rs:214-230`).

The request enum is private and closed: `Load`, `LookupRecord`, `LookupBatch`, `History`,
`CompleteSnapshot`, `Append`, `Create`, `Execute`, `Observe`, and `Batch`. Every variant carries
owned input, optional operation context, a shared phase/result cell, and, for a possible write, its
original `BatchKey`. No `Fn`, boxed future or user callback enters the queue.

## Runtime, worker and call protocol

`start` performs these steps:

1. Use the typed nonzero `queue_capacity` to allocate the bounded request channel and shared
   lifecycle, then spawn one named OS thread. A reported thread-spawn failure returns before any
   provider exists.
2. On that thread, build `tokio::runtime::Builder::new_current_thread().enable_all()`. Build the
   provider and async recorded adapter from `EventlogRecordedStoreOwner`, invoke the missing
   validate-only attachment for the already admitted inline projector, and perform the nonmutating
   adapter open. Provider-specific deadlines remain explicit configuration; they do not establish
   a universal bounded filesystem operation. This synchronous start waits for a conclusive
   handshake and offers no startup deadline or claim that local filesystem waits are bounded.
   It retains ownership until failure cleanup joins or a usable owner is returned.
3. Send one startup result only after the adapter is ready for requests. On failure or panic, retire
   anything already constructed on the same thread, join it, and return `BridgeStartError`. A
   successful result publishes the worker `ThreadId`, sender and join handle.
4. Run one async receiver loop. Requests execute sequentially. Provider-internal Tokio tasks and
   `spawn_blocking` work continue to run because the receiver and each operation are driven by the
   owner's runtime.

Sequential request execution is a compatibility-bridge choice, not an Eventlog limitation. It gives
deterministic store/registry ownership and bounded outstanding work: at most one dispatched request
plus `queue_capacity` queued requests. Eventlog still provides cross-process concurrency. A future
parallel bridge would need a new reviewed bound and shutdown proof.

Submission and waiting use this exact state protocol:

1. Reject `Reentrant` if `std::thread::current().id()` equals the worker ID. Check an already expired
   `CallWait::Until` as `DeadlineBeforeAcceptance`. Check lifecycle `Running`.
2. Create the result cell in `Queued` and call bounded `try_send`. `Full` becomes `QueueFull`;
   disconnected or closing becomes `Closed`. In each case the request was not accepted.
3. Immediately before invoking the typed async method, the worker compare-exchanges
   `Queued -> Dispatched`. If it observes `Cancelled`, it drops the request without touching the
   adapter. The phase transition is the admission acknowledgement; a separate best-effort message
   is unnecessary.
4. The caller waits on a standard mutex/condition variable, never on Tokio. At its deadline it first
   rechecks `Completed`, then compare-exchanges `Queued -> Cancelled`. Success proves
   `DeadlineBeforeDispatch`. If the state is `Dispatched`, the caller returns an after-dispatch
   result described below and the worker continues. If `Completed`, it takes the exact stored result.
5. The worker stores the operation result before publishing `Completed` with release ordering and
   waking waiters. A completed result therefore cannot be lost merely because a response channel
   closes. Dropping a waiting caller never cancels a dispatched future.

A panic around a dispatched request is caught on the worker with `std::panic::catch_unwind`. It
closes bridge admission, records the appropriate after-dispatch failure for the current request,
cancels queued requests before dispatch, performs best-effort owner-thread provider retirement and
exits. No new dependency is needed for this containment. An uncatchable worker disappearance uses
the same shared phase: `Queued` is definite non-dispatch; `Dispatched` remains uncertain for a
write. Panic containment is not permission to keep using potentially poisoned adapter state.

Contain panic during future polling, not merely during future construction: the private typed
request driver catches unwinding around each poll using the standard Future/Pin/panic facilities.
A worker-exit guard closes admission, publishes terminal before/after-dispatch results from the
shared phase and wakes every waiter. A waiting Forever caller must not hang because its owner
unwound. Process abort/termination is outside in-process panic recovery and claims no returned result.

## Exact bridge errors and write uncertainty

```text
pub enum BridgeStartError {
    ThreadSpawn(std::io::Error),
    RuntimeBuild(std::io::Error),
    Open(AsyncStoreError),
    WorkerPanicked,
}

pub enum BridgeRejection {
    DeadlineBeforeAcceptance,
    DeadlineBeforeDispatch,
    QueueFull { capacity: u16 },
    Closed,
    Reentrant,
    WorkerStoppedBeforeDispatch,
}

pub enum BridgeAfterDispatch {
    Deadline,
    WorkerPanicked,
    WorkerStopped,
}

pub enum SyncReadError {
    Rejected(BridgeRejection),
    Store(AsyncStoreError),
    AfterDispatch(BridgeAfterDispatch),
}

pub enum SyncWriteError {
    Rejected(BridgeRejection),
    Write(WriteFailure),
}

pub enum SyncExecutionError {
    Rejected(BridgeRejection),
    Execution(ExecutionError),
}
```

An async read error is preserved as `SyncReadError::Store`. A returned append failure is preserved
as `SyncWriteError::Write`, including its exact `NotCommitted` or `Uncertain` variant. Executor
errors remain the exact `ExecutionError` inside `SyncExecutionError::Execution`; core and semantic
store refusals are not rewritten as transport errors.

After dispatch, a local deadline, worker panic or disappearance on a nonempty direct append becomes:

```text
SyncWriteError::Write(WriteFailure::Uncertain {
    key: original_batch_key,
    cause: stable bridge cause,
})
```

The corresponding executor call becomes
`SyncExecutionError::Execution(ExecutionError::Write(WriteFailure::Uncertain { ... }))`.
Stable causes are respectively `"sync bridge deadline after dispatch"`,
`"sync bridge worker panicked after dispatch"`, and
`"sync bridge worker stopped after dispatch"`. `create`, `execute` and `observe` derive the same
`SingleRecord(record_id)` key the Executor uses; `batch` retains its caller-supplied key. A bridge
must validate every directly constructed `AppendRequest` before queueing. Malformed empty and
nonempty shapes return the existing
`WriteFailure::NotCommitted(AsyncStoreError::InvalidInput(...))` and never reach the queue; only a
validated nonempty request supplies a key for possible after-dispatch uncertainty.

A read cannot commit ER content, so its lost after-dispatch result uses the explicit
`BridgeAfterDispatch` variant rather than fabricating `AsyncStoreError::Unreachable`. A request that
the worker never dispatched uses `BridgeRejection`; it is not `WriteFailure::NotCommitted`, because
the provider writer was never called. Callers may resubmit that same request. After-dispatch write
uncertainty permits only semantic recovery with the original key and bytes. A later recovery may
carry fresh operational context exactly as the selected adapter design allows; the bridge does not
compare it to the original physical event.

## Shutdown and ownership outcomes

```text
pub enum ShutdownMode { Drain, CancelQueued }

pub enum ShutdownOutcome {
    Joined { provider: Result<(), AsyncStoreError> },
    TimedOut {
        queued: usize,
        dispatched: Option<BridgeOperationIdentity>,
    },
    Reentrant,
}

pub enum BridgeOperationIdentity {
    Read(BridgeReadKind),
    Write(BatchKey),
}
```

`shutdown` atomically changes `Running -> Closing` before signaling the worker, so all later
nonempty submissions return `Closed`. `Drain` processes every already accepted request in queue
order. `CancelQueued` changes every still-queued cell to `Cancelled` and reports its callers as
definite `Closed`/before-dispatch rejection; it never cancels the one dispatched future. After that
future and the selected drain behavior finish, PostgreSQL runs its explicit async `shutdown` on the
owner runtime. File and SQLite are dropped there only after no bridge future can reference them.
The runtime is dropped, the OS thread exits, and only then does `Joined` return.

`CallWait::Until` bounds how long the shutdown caller waits for that sequence. On expiry,
`TimedOut` retains the join handle inside `RecordedEventlogBridge`; another `shutdown`/join attempt
can finish later. If `dispatched` is `Write(key)`, the status makes the possible committed identity
visible but does not manufacture a write result. A joined thread whose provider shutdown returned
an error is distinct from a live timed-out thread.

The lifecycle is Running, Closing with a retained selected mode, or Joined with a cached provider
result. shutdown takes &mut self, so attempts on the owner are serialized; shared submission
handles observe the same atomic admission state. Reentrant detection happens before any change.
The first call closes admission and selects its mode even if its wait deadline is already expired.
After TimedOut, another shutdown operates on that same Closing state and retained thread handle:
Drain stays Drain unless a later CancelQueued explicitly escalates it; CancelQueued is sticky and
a later Drain never resurrects cancelled work. Escalation cancels only cells still in Queued using
the existing per-cell CAS; any already Dispatched future keeps running. Signal mode changes to the
same worker and continue waiting under the new call's wait policy; do not start another worker,
restart provider shutdown or reopen admission. Repeated timeouts retain the same ownership.

The worker initiates provider retirement once after the retained queue policy and active future
finish. A retry after retirement started only waits for it. Cache the concrete provider result,
observe actual OS-thread completion and join its retained handle exactly once before changing to
Joined. Subsequent shutdown calls return that cached Joined result, including provider failure,
without another provider shutdown/join. Completion is checked before returning a deadline timeout.
An unexpected worker exit runs the existing exit guard, releases every waiter and stores an
explicit failure result; a panic is never cached as successful provider retirement.

`Drop` performs Running -> Closing with CancelQueued, or escalates an existing Closing/Drain to
CancelQueued with the same CAS rules, then signals the worker and detaches the retained join handle.
After Joined there is no live handle to detach. It never waits indefinitely, calls `block_on`, or reports clean shutdown. Hosts that
need deterministic retirement must call `shutdown` and require `Joined { provider: Ok(()) }`.

## Existing synchronous surfaces

The bridge can truthfully serve new synchronous calls corresponding exactly to the async recorded
ports and executor operations listed above. It can therefore be integrated into a future explicit
Eventlog mode of a CLI, MCP server or adopter shell once that surface accepts
`EventlogOperationContext`, `AppendOutcome`, `ExecutionError` and write uncertainty.

It cannot implement or be substituted behind the current `entity_store::Store`, `RecordedStore` or
`AtomicBatchStore` traits. Those are mutable synchronous providers with different record/history
shapes and no `WriteFailure` result (`entity-store/src/lib.rs:175-250,400-415`). They also expose
`ids(entity)` and flattened domain-event reads, neither of which exists on `AsyncRecordedReader`.
Inventing those queries from incomplete scans would violate the bridge boundary.

Consequently the existing `entity_shell::StoredRuntime` cannot accept this bridge: it is generic
over `S: Store`, returns `RecordedCommit`, and maps all provider failures through legacy
`StoreError` (`entity-shell/src/lib.rs:105-240`). Current CLI store-backed commands instantiate
`FileStore` and `StoredRuntime` directly (`entity-cli/src/main.rs:470-543`), and MCP does the same
through its generic stored server (`entity-mcp/src/lib.rs:240-295`). The generated HTTP contract's
write refusal explicitly says state was unchanged (`entity-surface/src/lib.rs:130-155`), which
cannot represent a dispatched uncertain write. Those surfaces require separately reviewed additive
modes/error schemas; this bridge must not squeeze uncertainty into their existing `refused` result.

The existing async `Executor` is the reusable execution surface. Existing legacy Entity SQLite,
PostgreSQL, remote and file provider facades remain separate and gain no claimed equivalence from
this bridge.

## Cargo and feature closure

The `entity-eventlog` crate is the Rust 1.91 IO closure already selected by the adapter design
(`docs/design/eventlog-recorded-adapter-v0.1.md:15-18`), while the current ER workspace declares
Rust 1.85 (`Cargo.toml:11-17`). The new package must explicitly declare `rust-version = "1.91"` and
the workspace/gate must treat that higher-MSRV edge deliberately; silently inheriting 1.85 would
be false. Final Eventlog source pins remain outside this report.

The bridge module requires:

* `entity-core`, `entity-store`, and `entity-executor` for the owned registry, complete async types
  and borrowed executor;
* `tokio` with `default-features = false` and `rt`, `sync`, `time`, `macros`; `rt` builds the
  current-thread runtime, `sync` supplies the bounded request/control channels, `time` drives
  provider deadlines, and `macros` supplies the worker's typed request/control `select!`;
* the async adapter's `eventlog-core` dependency and one or more optional `eventlog-file`,
  `eventlog-sqlite`, `eventlog-postgres` dependencies selected by same-named Cargo features; and
* `time` for the already selected typed `EventlogOperationContext.occurred_at` if the adapter does
  not re-export that concrete type.

Eventlog itself pins Tokio 1.53.1 at source `18322cbe` and declares it without default features
(`Cargo.toml:20-36`, `Cargo.lock:1169-1173`). File requests `rt,sync`, SQLite `rt`, and PostgreSQL
`rt,sync,time` (`eventlog-file/Cargo.toml:11-17`, `eventlog-sqlite/Cargo.toml:11-16`,
`eventlog-postgres/Cargo.toml:11-21`). The bridge does not require `rt-multi-thread`, an async-trait
crate, a general thread pool, crossbeam, or futures utilities. `std` supplies the owner thread,
atomics, mutex/condition variable, deadlines and panic containment. Tests may enable Tokio's
`rt-multi-thread` only to exercise callers inside that flavor.

The selected Cargo shape is `sync-bridge` gating this module and Tokio runtime features, with
independent `file`, `sqlite`, and `postgres` provider features. No default provider should silently
select filesystem, network, credentials or authority.

## Decisive tests

Use a deterministic internal fake async recorded store with barriers rather than timing sleeps for
the bridge mechanics, then run provider-backed cases where the adapter dependencies permit:

* hold one dispatched future, time out Drain, retry Drain, then escalate CancelQueued; only queued
  cells are cancelled and a subsequent Drain cannot revive them. Release the future, join once,
  and repeat shutdown to obtain the same cached result. Repeat with provider-retirement failure,
  a second timeout during retirement, worker panic and Drop after a timed-out Drain. Assert one
  worker, one provider retirement, unchanged dispatched identity and no waiter stranded;

* startup constructs and drops registry/store on the worker thread; thread spawn, runtime build,
  adapter open and startup panic return distinct errors and no usable handle;
* exact pre-admitted projection shapes attach without a persistent write, while missing, dirty or
  drifted admission makes startup refuse without creating or repairing anything;
* ordinary synchronous calls work outside Tokio, inside a current-thread runtime, and inside a
  multi-thread runtime without nested-runtime panic or dependence on caller-runtime progress;
* a call made by an internal worker-thread test hook refuses `Reentrant` before queue admission;
* capacity `1` admits exactly one queued request behind one barrier-held dispatch; the next request
  returns `QueueFull` and the fake store observes no call for it;
* an expired pre-send deadline and a successful `Queued -> Cancelled` deadline observe no provider
  call; the worker discards the cancelled cell;
* at the exact dispatch/deadline race, one CAS wins: cancellation proves no call, or a direct append
  and each executor write return `WriteFailure::Uncertain` with the unchanged original key while
  the worker continues;
* a dispatched write that commits before its reply is withheld remains uncertain to the timed-out
  caller, and a semantic retry with the same bytes/key but fresh context recovers one original
  commit; no automatic retry or metadata substitution occurs;
* provider `NotCommitted`, provider `Uncertain`, every `AsyncStoreError`, and kernel
  `ExecutionError::Core` pass through without bridge reclassification;
* worker panic/disappearance before dispatch is a definite rejection; after dispatch it is a
  read-side `BridgeAfterDispatch` or write-side existing uncertainty, and queued work is cancelled;
* `Drop` of a waiting caller after dispatch does not cancel the store future;
* `AppendRequest { key: None, members: [] }` stays inert even when the queue is full or bridge is
  closing, while `Some(key) + []` is validated first and returns
  `NotCommitted(InvalidInput)`; executor `batch(key, [])` independently stays inert without
  validating the key;
* `ShutdownMode::Drain` completes accepted work in order and joins; `CancelQueued` proves queued
  requests undispatched while allowing the active one to finish; later submissions refuse;
* a shutdown deadline returns `TimedOut` with the live dispatched identity, a later join succeeds,
  and `Drop` requests closure without blocking; and
* PostgreSQL shutdown is awaited on the worker and reports provider deadline separately from bridge
  join, while File/SQLite drop only after all accepted futures are gone.

These tests close only runtime isolation, bounded admission, response truthfulness and ownership.
They do not prove native capture, inline rebuild, canonical decoding/indexes, provider transaction
semantics, final dependency pins or legacy import.
