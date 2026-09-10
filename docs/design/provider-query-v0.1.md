# Provider query and transaction session v0.1

Status: accepted by `story:provider-indexed-transaction-session` and Atlas ADR 0009.

## 1. Optional query capability (R-119)

`entity-query` is outside `entity-core` and defines recursive JSON containment predicates over one
entity discriminator. Providers return byte-ordered keyset pages. A cursor carries the complete
canonical query identity and the last opaque instance id; using it for another question is refused.
Limits default to 100 and are bounded at 1,000.

The memory implementation is the behavioural reference. PostgreSQL answers the same question with
a JSONB containment predicate and a GIN expression index over existing JSON instance documents, so
the capability does not change stored document bytes.

## 2. Caller-scoped PostgreSQL session (R-120)

`PostgresStore::with_transaction` lends a `PostgresSession` to one caller operation and commits only
when its closure succeeds. The session offers `load_for_update`, event reads, query access, an
ordered uncommitted batch, a monotonically increasing namespace reservation, and a transaction
advisory lock for a logical identity that may have no row yet. The outer
provider owns commit and rollback; the adopter cannot accidentally make a partial prefix visible.

This is a provider capability, not a kernel API. It reads a database and therefore cannot enter
`entity-core` under R-01.

## 3. Recorded compatibility sessions (R-123)

The SQL providers additionally implement `AtomicRecordedStore` for complete `RecordedCommit`
batches. They use their existing history and instance layouts, including accepted zero-event
decisions. An exact record retry succeeds before the old expectation; changed provenance or a
record id already owned by another subject refuses the complete batch. Decisions apply in request
order so repeated subjects see earlier entries. A late refusal rolls back state, envelopes, events
derived from those envelopes, and global identity reservations together.

`PostgresSession::commit_recorded_batch` stages those writes within a savepoint. Catching an error
does not retain its prefix or poison the caller transaction; the caller may then query, reserve
sequences or stage another batch. The session's `records` and `observations` reads use its current
transaction view. An outer refusal rolls back successful inner batches and sequence reservations.
Other connections see none of the staged changes before that outer commit.

Recorded PostgreSQL batches acquire record-id and subject-identity locks in deterministic order
before applying the caller's ordered decisions. This includes absent subjects. A caller that
already holds arbitrary locks across several session operations still owns its wider lock order;
this does not claim that every possible user transaction is deadlock-free.

Both SQL implementations compose with the existing explicit `BlockingRecordedStore` adapter.
It executes synchronous IO on the polling thread; callers must place it appropriately. No runtime,
clock or new persistence format is selected here. This preserves the legacy migration boundary;
the eventual Eventlog-backed SQL facades and native query/transaction capabilities remain required.
