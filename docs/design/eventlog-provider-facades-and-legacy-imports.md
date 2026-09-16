# Eventlog provider facades and legacy imports

Status: implementation design for the remaining ESS evolution M2 provider work (R-155).

## Boundary and dependency direction

`entity-store` remains the Rust 1.85 home of the pure legacy ports and of the typed legacy
acquisition document. It does not depend on `entity-eventlog`. `entity-eventlog` remains the Rust
1.91 IO closure and depends on `entity-store`, `entity-executor`, and the selected Eventlog native
provider. `entity-sqlite` and `entity-postgres` may add provider-specific constructors that depend
on `entity-eventlog`; no dependency points back from `entity-eventlog` to those ER packages.

This preserves the existing package identities and resolves the `entity-store -> entity-eventlog`
cycle without moving a runtime, filesystem, SQL driver, or interpreter into `entity-core` or
raising the minimum Rust version of an independent pure crate. Each SQL package keeps its legacy
Rust 1.85/default dependency closure; its explicit `eventlog-facade` feature selects the recorded
Rust 1.91 IO closure. Consumers opt into the new runtime boundary without making an old
legacy-reader build resolve or compile Eventlog.

The old `entity_store::FileStore`, `entity_sqlite::SqliteStore`, and
`entity_postgres::PostgresStore` remain byte-compatible legacy readers and writers. They are not
reinterpreted in place. New Eventlog-backed handles have explicit `Eventlog` names and open a
separate Eventlog authority. The CLI must select the new mode explicitly; the old mode remains an
explicit source/import mode.

The reference CLI keeps `--store` as the legacy File Store by default. Its opt-in
`eventlog-providers` feature adds `store provision-eventlog-file`, which prints a closed JSON
selection containing the verified `Authority`, every caller-selected capture bound and the bridge
queue bound. Passing that document with `--eventlog-config` on `create`, `execute` or `list`
selects the recorded File facade. No path is probed to infer a provider. The ordinary CLI retains
its Rust 1.85 dependency closure; building the explicit Eventlog feature uses the adapter's stated
Rust 1.91 closure.

## Public facade

`entity-eventlog` owns one synchronous `RecordedProviderFacade`. It owns a
`RecordedEventlogBridge` and exposes the complete recorded surface without erasing bridge results:

* current state, revision and sorted identities from one provider-owned complete snapshot;
* flattened domain events, mixed recorded history, decisions and observations;
* global original-record and original-batch lookup with original immutable receipts;
* single recorded append and ordered named recorded batch append;
* executor-backed create, execute and observe operations;
* provider-neutral containment query over the captured terminal instances; and
* explicit shutdown with the bridge's drain/cancel and retirement outcomes.

Every write takes an `EventlogOperationContext` and returns `AppendOutcome` or the existing typed
sync execution/write error. A pre-dispatch refusal, a conclusive adapter rejection, an original
committed or recovered receipt, and an after-dispatch uncertain result remain distinct. The facade
does not implement the legacy `Store` or `AtomicBatchStore` write traits: those traits return only
`StoreError` and cannot truthfully carry uncertainty or a receipt. Read-only compatibility traits
may be implemented where their result is lossless. Compatibility writes are inherent methods with
the same state/history vocabulary plus the mandatory recording and operation context.

An ordered recorded group accepts `AppendMember`s and one caller-named `BatchKey`; it delegates to
the adapter's one `AppendGroup`. Repeated subjects retain request order and transaction-local
expectations. There is no second decision evaluator. Bare `AtomicCommit` values remain available
only on the legacy stores and are not silently upgraded into invented envelopes.

## Native owners and authority

File, SQLite-memory, SQLite-file and PostgreSQL constructors all produce the same facade. Runtime
open is non-administrative: it requires the fixed projector, immutable binding, exact logical
scope, Eventlog tenant and captured stream generation to exist and match. Provisioning is a
separate named constructor/action which creates the fixed projections, attaches the projector,
publishes or recovers the exact binding, and then opens the facade. Import is a further explicit
mutation after that binding is settled.

The public types keep that separation structural. `EventlogRecordedStoreOwner` can only open an
already prepared authority. `EventlogRecordedStoreProvisioner` is required by the named
`provision` path. PostgreSQL provisioning receives distinct migration-role and application-role
`PostgresConfig` values: the first creates the declared projections, then retires; the second opens
the bounded DML-only runtime and establishes the binding. An ordinary PostgreSQL open receives
only the application authority and therefore cannot acquire DDL as a side effect.

Provisioning names a `ProvisionAuthority`: logical scope, tenant and an optional expected native
generation. `Some(generation)` is an exact precondition and refuses a freshly opened provider with
a different identity. `None` is an explicit request to accept the generation minted by this
provisioning operation; the code reads that generation from the native provider and persists the
resulting closed `Authority` in the immutable binding before returning a facade. Ordinary open
always requires the complete closed `Authority`. This makes process-local SQLite usable without
guessing an identity and keeps every reopen an exact owner/scope/generation check.

PostgreSQL has two deliberately separate authorities:

* the retained legacy `PostgresStore::from_client(postgres::Client)` is a caller-provided source
  reader, so an existing TLS/client choice is preserved for acquisition; and
* the Eventlog destination accepts Eventlog's typed `PostgresConfig` and `PoolOptions`, including
  either its built-in verified roots or a caller-owned `PostgresConnectionAuthority` that returns
  one custom-transport client and owned driver per bounded pool connection. Hosted admission still
  verifies the role, schema and connection budget. The loopback URL constructor is test-only and
  is not documented as equivalent to production authority.

Opening, provisioning and importing are separate calls with separate errors. All three validate
the same logical scope, tenant and generation. No constructor selects credentials, TLS roots,
schema, tenant, prefix, scope or generation on a caller's behalf.

## Legacy acquisition document

`entity-store` defines a closed `LegacyStoreSnapshot` containing a nonblank source identity and
sorted `SubjectHistory` values whose origins are imported anchors and whose suffixes are empty.
`LegacyStoreSource::acquire_legacy` is read-only. Each provider implements it using one consistent
provider boundary where available and refuses corrupt, partial or changing input.

For every subject the source records:

* the exact terminal `EntityInstance` as the anchor;
* every complete decision and observation envelope available, including its original global
  record id;
* bare legacy decisions/events when no complete envelope exists;
* only the order the source actually stored; and
* a stable provider-native locator for every envelope.

SQL `history.position` establishes mixed decision/observation order only when the subject has no
separate bare-event history. File v2 keeps decisions, observations and bare events in separate
arrays, and pre-history SQL event rows likewise retain no mixed coordinate with recorded history;
those sources use `PerKindOnly`. A source that cannot establish genesis, a cross-subject order,
batch membership or a receipt does not invent one.
All imported histories use `HistoryOrigin::Imported`; `LegacyCompleteness::CompleteSubject` means
complete evidence available in that source through the anchor, not replay-verified genesis.

File acquisition verifies the format marker, rejects symlinks and temporary/foreign files, reads
each subject once under the existing persistent writer lock, and retains its exact relative file
locator. SQLite uses an immediate read transaction over `instances`, `events`, `history`, and
`legacy_origins`. PostgreSQL uses one repeatable-read, read-only transaction through the exact
caller-supplied client. Subjects and evidence are sorted by provider coordinates before the public
document is returned.

## Import and equivalence

Import validates the whole acquisition document before writing. It refuses duplicate subjects,
duplicate global record identities, mismatched subject/revision data, malformed envelopes and an
already populated destination subject. Each subject is written through the adapter's existing
`AsyncImportedAnchorWriter`, so imported global IDs are reserved and the typed evidence is retained
without a fabricated committed receipt.

The validation is repeated at the import boundary even though provider acquisition constructs a
valid document. The document remains public typed data and a caller can inspect or mutate it after
acquisition; reconstructing it under `LegacyStoreSnapshot::new` before the first append prevents a
post-acquisition mutation in a later subject from leaving an earlier subject partially imported.
When the source retains a decision at the terminal revision, its exact result must also equal the
anchor instance; a caller cannot substitute terminal fields while retaining the old envelope.

The import report lists every source subject and its settled imported assurance. A retry recovers
the exact existing anchor; different bytes at the same destination subject or record identity are
a conclusive conflict. An unknown native commit remains uncertain until authoritative recovery
settles it.

After all subject anchors settle, import obtains an actual provider-owned complete snapshot and
compares its scope, subjects, terminal instances, typed imported evidence, original record IDs and
known order to the source document. The comparison ignores facts the source never possessed:
Eventlog physical positions, new import-reference events, cross-subject chronology and receipts.
No editable marker can claim completeness.

Import is atomic per subject because that is the existing imported-anchor contract. The report
does not claim one cross-subject transaction. A failed multi-subject import can be resumed by exact
source identity; already settled equal anchors are recovered, and unequal anchors refuse.

## File atomic groups

The Eventlog File facade's named recorded batch is the FileStore atomic-group implementation. All
members, their immutable blobs, fixed inline indexes, global IDs and group receipt are committed by
one Eventlog file journal transaction. The legacy `entity_store::FileStore` remains a legacy
single-subject store and never claims this property.

Acceptance uses separate OS processes against one Eventlog file root. Concurrent groups either
commit wholly in serialized order or return a typed conflict/recovery result. Kill points cover a
process after blob staging and during journal publication; reopening verifies that no terminal
state, record index or batch receipt is partially visible. Repeated-subject members retain order,
and a late conflict rolls back every earlier member.

## Formats and compatibility

No existing FileStore, SQLite or PostgreSQL table or JSON document changes. Acquisition is a
reader of those formats. No `entity-eventlog` binding, record, request, batch or imported-anchor
encoding changes. New public acquisition/report types are runtime API data, not a persisted format.

Old readers therefore retain exact bytes. The Eventlog destination uses the already frozen
canonical `er.record/*`, `er.request/1`, `er.batch/1` and imported-anchor encodings. Corrupt source
JSON, duplicated global IDs, stale snapshots, missing Eventlog blobs, substituted authority and
unknown fields are refusals rather than repair guesses.

## Evidence

Provider-independent tests pin acquisition validation, whole-document revalidation before writes,
partial-import retry/conflict, zero-event decisions, observations, repeated-subject groups, query
paging and all error classifications. Provider tests cover File, SQLite memory/file and disposable
PostgreSQL. File additionally uses competing child processes and crash/reopen. PostgreSQL exercises
caller-supplied legacy clients and caller-owned verified native authorities separately. Compiled
fault controls remove state/events, substitute identity and expose a partial group to prove that
the corresponding tests fail.

The final gate includes the full actual-PostgreSQL task check, Rust 1.91 adapter/facade checks,
Rust 1.85 checks for unchanged pure crates, strict formatting/lints/rustdoc and the documentation
site when the public documentation source changes.
