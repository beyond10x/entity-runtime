# Recorded Eventlog provider

This design owns R-122.

`entity-eventlog` implements the asynchronous ER ports over `AtomicEventStore`. The caller owns
the Eventlog provider, exact tenant, namespace and opaque transport attribution. The adapter
opens no path and selects no credentials, clock or async executor. Eventlog is pinned to
06c8e1c806ece76bf2874107d73691ca71e4f18f, the published committed-inventory follow-up to 0.2.0.

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

## Build and completion boundaries

Eventlog requires Rust 1.91; the existing ER workspace retains Rust 1.85. The adapter has an
independent workspace and lockfile, and is included in `task check` through `task eventlog-check`
and in the existing required CI Gate job. It cannot silently escape normal verification.

File, SQLite and PostgreSQL tests exercise the same adapter contract. File and PostgreSQL tests
cover reopening, independent writer handles and late transaction failure. PostgreSQL also verifies
immediate enumeration under an independently withheld feed. The explicit PostgreSQL lane requires
an assigned disposable database and bounds each case to 20 seconds; CI selects it against its service.
Existing ER provider layouts are still
readable through their original packages; this addition does not migrate them. PostgreSQL facade
convergence, query/transaction adaptation, AEP migration and application adoption remain required
evolution work. Small functional tests are not a production throughput claim.
