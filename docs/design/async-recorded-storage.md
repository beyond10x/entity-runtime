# Asynchronous recorded execution

The ESS evolution migration requires recorded execution over asynchronous persistence without
putting an executor, IO or clocks in `entity-core`. This extends the existing types and formats
in [store-v0.2](store-v0.2.md); it does not replace synchronous callers or provider layouts.

## Ports and execution

`entity-store::asynchronous::AsyncRecordedStore` requires all recorded read/write methods. Its
boxed Send futures borrow a mutable provider and may suspend on any caller-selected executor.
The trait is object-safe and does not depend on an async runtime. Unlike `Store`'s compatibility
default, a recorded commit cannot fall through to an event-only write.

`entity-shell::asynchronous::AsyncStoredRuntime` reads a consistent prefix of decision envelopes,
validates their metadata and subject identity, and replays the pinned definitions and normalized
commands. That replayed result is execution authority, including for exact command retries.
Materialized state is consulted only when records are empty, to distinguish a legacy snapshot
from absence. Legacy state without complete history is refused. A concurrent write after the read
is fenced at commit by the expected entity revision; the executor does not retry automatically.
Record history is append-only, so a consistent earlier prefix is safe under this optimistic fence.

An exact retry returns its original result even after later decisions or registry changes. It
compares normalized input, expected revision and every recording metadata field against the
original pinned record. Providers enforce record-id identity globally across subjects and
observations, atomically with publication. Observations do not change entity revisions or events.
No physical Eventlog position is interpreted as an entity revision.

Cancellation after submission or an unreachable provider can leave the outcome unknown. A caller
keeps the same record id and content for recovery; dropping a future is not a rollback receipt.
The executor starts no background work and does not invent authority, actor, time or identity.

## Compatibility and atomicity

`BlockingRecordedStore` explicitly runs an existing complete synchronous provider on the polling
thread. It does not promise nonblocking IO and uses no nested executor. Applications can place it
on a dedicated worker. Existing provider packages remain readable and their APIs unchanged.

`AtomicRecordedStore` and `AsyncAtomicRecordedStore` are separate optional capabilities. Batches
retain complete envelopes, apply entries in request order, and see earlier transaction-local
revisions. Every failure rolls back state, events, records and retry bookkeeping. The reference
MemoryStore stages a clone before publication. A durable adapter must use one real transaction;
looping over independent durable commits is forbidden. A batch is not a new persisted record or
group identity. [The Eventlog adapter](eventlog-recorded-provider.md) supplies its persistence mapping.

## Evidence and limits

R-121 is pinned by `crates/entity-shell/tests/asynchronous.rs`: a provider actually yields Pending,
zero-event history replays, retries survive registry changes, tampered/legacy histories refuse,
observations retain their revisions, and a failed ordered batch rolls back its global retry index.
The original kernel purity tests continue to govern `entity-core`.

The default asynchronous port verifies history through `VerifiedHistory`, which couples private
incremental kernel replay state with validated envelopes. A provider may share an immutable proof
after validating its stored prefix; the shell then does not replay it again. The Eventlog adapter
checks generation and head before reuse and verifies only appended records. No proof is loaded
from an unverified persisted snapshot. Synchronous and asynchronous execution use one retry matcher.

Provider facade migration, query transactions, AEP migration and ESS/application adoption remain
unfinished. Cache reuse is tested separately from persistence; no production throughput claim is
inferred from small functional examples.
