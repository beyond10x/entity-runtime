# Recorded Eventlog provider

This design owns R-122, R-124 and R-125.

`entity-eventlog` implements the asynchronous ER ports over `AtomicEventStore`. The caller owns
the Eventlog provider, exact tenant, namespace and opaque transport attribution. The adapter
opens no path and selects no credentials, clock or async executor. Eventlog is pinned to
f5cfba50afe6418dedac92a5b34e3d5f8fd6577a, the published native transaction follow-up to 0.2.0.

## Persistence contract

One physical subject stream stores `entity.record` events with schema version 1. Each body is a
strict `entity-eventlog/1` envelope with a tagged decision or observation payload. The payload is
the existing complete ER `RecordedCommit` or `RecordedObservation`, unchanged. Zero-event
decisions still occupy one physical position. Observations also occupy positions, but their
entity revision stays fixed. Reads validate subject identity, position continuity and recorded
metadata, then recompute the decisions through the kernel.

Record identity is global within one tenant and namespace. A record-id stream reserves the digest
of the complete payload and the subject stream/position that owns its body. Each reservation and
subject append are entries of **one** Eventlog atomic group. Ordered batches stage logical
expectations against earlier entries; any refusal rolls back bodies and reservation bookkeeping.
There is no independent append-loop fallback. Exact already-held records are accepted before old
expectations, matching the ER provider contract. Changed content under an existing identity is a
record conflict. A group identity hashes the complete ordered caller request, including logical
expectations; physical positions never become logical revisions.

Names and record ids are hashed into bounded stream coordinates with typed tuple boundaries;
the original names remain in the payload and are checked on read. Hashing is confined to the
record being admitted and its identity claim, not source inventories or tool binaries.

The Eventlog metadata carries host-selected opaque subject and actor identifiers. Original ER
actor, correlation, causation and recording time remain in the payload exactly. The physical
occurrence time is parsed from caller-provided recording time; an unsupported timestamp refuses
instead of being replaced with the current time. A lost commit response remains unknown;
resolve the same record identity rather than inventing a fresh command.

## Shared verification and cache authority

Enumeration uses Eventlog's committed stream inventory and validates selected ER histories. It
cannot use the catch-up feed: PostgreSQL deliberately withholds that feed behind unrelated older
transactions, including after this caller's append has succeeded. Inventory pagination is a series
of fresh reads, not a cross-page snapshot; concurrent inserts before a cursor need a fresh scan.

`entity-core::VerifiedReplay` verifies one next decision using the same path as genesis replay.
Its state is private and cannot be loaded from an unverified snapshot. `VerifiedHistory` adds
subject and envelope identity checks and shares the resulting proof between a provider and the
async shell. Both command shells use one retry matcher.

The adapter retains verified prefixes only in memory. Before reusing a prefix it checks the
provider's history generation and stream head. A changed generation discards the prefix; a
smaller head with the same generation is refused. With the same generation, only the appended
tail is read, checked against atomic identity claims and replayed. Generation is checked again
after reading. A provider with no generation support gets no cache reuse. Redaction is never
interpreted as an ordinary decision, and a forged or incomplete new tail is refused. A cache is
not portable evidence or persisted authority.

Retention defaults to at most 64 subjects and 16 MiB of encoded record bodies. The host may select
another budget or disable retention. This bounds retained input, not exact heap usage; immutable
proofs retained by callers are caller-owned. Eviction and oversized histories fall back to replay
without changing durable state. Shared immutable collections avoid cloning every prior envelope
and observation on an unchanged read.

## Native indexed queries (R-124)

`entity-query::AsyncDocumentQueryProvider` retains the existing `DocumentQuery`, containment,
limit and query-bound cursor contract on the caller's executor. The memory implementation is the
reference. `DocumentPage::from_page` preserves the query identity when a native provider reports
its own continuation rather than returning a discarded extra row. No runtime enters the kernel.

The host explicitly registers `entity_eventlog::EntityDocumentProjector` on every Eventlog writer
before traffic. Its `DOCUMENT_PROJECTION` opts into Eventlog PostgreSQL's document indexes, using
the migration role for hosted storage. Existing `ProjectionSpec` declarations and event formats
are preserved. One projection covers version-one ER history streams across tenant/namespace
coordinates, with rows keyed by namespace, the hashed entity discriminator, then the original
instance ID. The last segment preserves byte ordering. Zero-event decisions update the candidate
instance; observations advance its physical position without advancing the entity revision.
Inline projection writes participate in the original atomic group, including late rollback.

Before its first query, a handle explicitly calls `enable_document_queries`. This startup operation
checks that the projector is inline, that native querying is admitted, and that every committed
subject in the selected namespace has a corresponding row at the verified history's current
physical position. It refuses missing, stale or invalid rows. This is an inventory/verification
scan at explicit readiness, not an implicit scan for each query. The caller owns fencing: every
writer must register the projection, and old writers must be stopped during migration. Readiness
cannot make an independently misconfigured writer maintain a projection it never registered.

For an existing owner, create/admit the projection and call Eventlog `rebuild_projection` before
inline registration, under that fenced startup. Rebuild is derived work, not a rewrite of ER
history. Then register inline and enable the handle. A feed watermark held by an unrelated older
transaction can leave a rebuild incomplete; the committed inventory check refuses readiness in
that case. The host can settle that condition and retry the explicit rebuild with a fresh
unregistered Eventlog handle. Reads never start a background rebuild or wait indefinitely for the
feed. A redacted or invalid decision chain is refused, not silently imported as valid state.

For each ordinary query, Eventlog PostgreSQL performs JSONB containment and bounded keyset paging.
The adapter verifies only those selected candidates against recorded history using its existing
bounded prefix cache. A persisted projection row is not sealed kernel proof: changed instance
bytes, wrong scope/key, missing history, redaction or a changed physical position refuses the
query. This makes concurrent changes visible as a store error instead of silently returning a
projection that disagrees with history. The caller may issue another read; the adapter does not
retry mutations or change query identity. Separate pages do not promise a common snapshot.

The optional capability is PostgreSQL-native today. File/SQLite point and recorded APIs retain
their behavior, while document projector registration refuses on those providers. Native sessions below reuse this query implementation inside the caller transaction. Legacy SQL
layout migration and compatible facade replacement remain separate required work.

## Build and completion boundaries

Eventlog requires Rust 1.91; the existing ER workspace retains Rust 1.85. The adapter has an
independent workspace and lockfile, and is included in `task check` through `task eventlog-check`
and in the existing required CI Gate job. It cannot silently escape normal verification.

File, SQLite and PostgreSQL tests exercise the same adapter contract. File and PostgreSQL tests
cover reopening, independent writer handles and late transaction failure. PostgreSQL also verifies
immediate enumeration under an independently withheld feed. The explicit PostgreSQL lane requires
an assigned disposable database and bounds each case to 20 seconds; CI selects it against its service.
Existing ER provider layouts are still
readable through their original packages; this addition does not migrate them. SQL facade
convergence, AEP migration and application adoption remain required
evolution work. Small functional tests are not a production throughput claim.

## Native transactions (R-125)

`EventlogStore::with_transaction` is available when the caller's provider implements native
`TransactionalEventStore`. The callback receives an owned `EventlogSession` whose storage borrow
is limited to that callback; capture owned inputs and return an owned, Send value. No transaction,
connection or executor is owned by ER. The value is returned only after Eventlog confirms commit.
The callback runs once. There is no nested runtime or independent-write fallback.

The outer handle and scoped session forward the same async recorded/query ports to one private
`RecordedStore` implementation. Its narrow IO port forwards either to the caller-owned provider
or to the active Eventlog transaction. Replay, record equality, identity claims, physical/logical
revision checks, batching and document candidate verification are not copied. Session reads see
earlier staged writes. Each session starts with an empty bounded cache and discards it at the end;
neither successful nor rolled-back staged prefixes are copied into the outer cache. An outer
read still checks native generation and head before reusing its own earlier prefix.

`load_for_update` locks the exact history stream used by ordinary and grouped writers, including
an absent subject, then verifies its current state. Logical identity locks and sequence namespaces
hash a typed tuple containing the ER history namespace; Eventlog additionally enforces its tenant
boundary. Arbitrary application identity text is hashed into the lock coordinate. Positive sequence
reservations return the value before their range, matching the compatibility provider. The caller
owns ordering across multiple locks. Generation capture protects verified history against redaction
until transaction end, and the native publication gate precedes all callback locks.

The host establishes document readiness on the outer handle before starting query-bearing sessions.
The session inherits that readiness and performs native transaction-local filtering and candidate
verification. It exposes no registration or implicit rebuild method. A caught append-group refusal
rolls back the failed group's body/projection/claim prefix through Eventlog's savepoint; earlier
successful groups remain staged. Returning an error rolls back the complete callback, including
sequences. Ordinary unfinished SQL and caught cancellation cannot be converted into commit.

A bounded in-memory error slot carries a callback's original typed StoreError across Eventlog's
error boundary. It is read only when Eventlog reports the matching callback refusal after rollback;
settlement and unknown-outcome errors take precedence. No error is serialized into an event or
replaced by a new mutation. Cancellation drops the scoped cache and relies on Eventlog's quarantine
for the in-flight connection. Exact recorded identities remain retry authority. A sequence-only
unknown outcome has no deduplicated receipt and must not trigger blind callback replay.

The focused PostgreSQL cases exercise dynamic recorded execution and native queries, external
invisibility, exact retries, complete outer rollback with cached history, typed callback refusal,
namespace/tenant sequence separation, late projection savepoint refusal, and cancellation after
a complete recorded write. Existing file/SQLite/PostgreSQL cases exercise the shared implementation.
These are functional compatibility observations, not throughput or hosted deployment approval.
