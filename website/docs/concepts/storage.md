---
sidebar_position: 4
title: Storage and replay
description: The write contract, the providers, Eventlog-backed recorded stores, forks and merge, and what replay proves.
lede: The kernel returns a value; a provider commits state, history and events as one write at the revision the caller expected.
source: "crates/entity-store, crates/entity-shell, crates/entity-sqlite, crates/entity-postgres, crates/entity-remote, crates/entity-executor, crates/entity-eventlog and their tests; CHANGELOG 0.19.0 to 0.27.0"
---

# Storage and replay

The shell decides whether to store the kernel's result, publish its events or discard it.

## The write contract

A commit states what the caller expected:

- `Expect::Absent` for a creation;
- `Expect::Revision(n)` for an operation decided on revision `n`.

The provider checks the expectation before writing. If another writer advanced the subject first,
the commit returns `RevisionConflict` and writes nothing. Reload and decide again instead of
patching the newer state with an older result.

`RecordedCommit` wraps a decision in an envelope the shell supplies:

- a record id, used for idempotency;
- a recorded-at time (ISO-8601, validated);
- an actor, or an explicit statement that there was none;
- optional correlation and causation ids.

Reusing a record id for identical bytes succeeds. Reusing it for different bytes is a
`RecordConflict`.

## Retry boundaries

`StoredRuntime` (in `entity-shell`) recognizes an exact retry of an accepted operation even after
the subject has moved on, and returns the original commit. The `entity` command's stored verbs, a
generated CLI and the MCP tools all run through it.

| Entry point | Revision a new operation is decided on | Exact retry of an accepted request |
|---|---|---|
| `entity execute --store` | `--expected-revision`, or the revision the store holds when the command runs | Original commit, when the retry repeats the revision, arguments and recording |
| Generated domain CLI | Required `--expected-revision` | Original commit |
| MCP operation tool | Required `expected_revision` | Original commit |
| `Store::commit_recorded` | The `Expect` the caller passes | Identical bytes are idempotent; different bytes under one record id conflict |

The returned commit describes the original operation and is not a fresh read of the subject. A new
record id is a new request and passes the current revision and policy checks. Without
`--expected-revision`, rerunning an `entity execute --store` after the subject moved decides on the
newer revision, so the same record id names a different request and is refused as
`record_conflict` ([getting started](../getting-started.md#retry-an-accepted-request) shows both).

## Providers

| Provider | Crate | For | Boundary |
|---|---|---|---|
| `MemoryStore` | `entity-store` | tests and process-local work | nothing survives the process |
| `FileStore` | `entity-store` | the `entity` command and local single-root storage | a new root is v3, which appends each record to the subject's history; a v2 root stays v2 until [migrated](../guides/migrate-a-file-store.md) |
| `SqliteStore` | `entity-sqlite` | embedded durable applications | state, history and events share one database transaction |
| `PostgresStore` | `entity-postgres` | centralized multi-process deployments | the caller opens the connection and chooses transport and TLS |
| `RemoteStore` | `entity-remote` | a store behind an application-owned transport | a versioned JSON protocol, not an HTTP client |
| `Hybrid` | `entity-remote` | explicit local and remote authority | authority, read path, offline and divergence behaviour have no defaults |
| `EventlogRecordedStore` and its facades | `entity-eventlog`, `entity-sqlite`, `entity-postgres` | complete recorded history on an Eventlog provider | the provider and its binding are provisioned explicitly before an open |

`MemoryStore`, `SqliteStore` and `PostgresStore` implement `AtomicBatchStore`: an ordered
multi-subject batch commits completely or rolls back completely. A File Store commits one subject
document at a time. One black-box conformance suite in `entity-store` holds every provider to the
same contract, and is itself checked against a broken provider.

The File Store serializes concurrent writers to one root and refreshes cached record identities
when another writer changed the store. Use a filesystem with advisory locks and atomic rename.
The File Store resolves paths under its root by following symlinks, so only the account it runs as
may modify the root and its ancestors. A root another account can modify is not supported.

Subject data is flushed before replacement. Unix also flushes directories, including the parent of
each directory a first write creates, while Windows does not promise directory entries survive
power loss. A store written before 0.15 must be migrated first, and a v2 store can be moved to v3:
[migrate a File Store](../guides/migrate-a-file-store.md).

## Eventlog-backed recorded stores

`entity-eventlog` keeps complete recorded state — the normalized request, the definition snapshot,
the result, the response and the events — on an
[Eventlog](https://beyond10x.github.io/ecosystem/eventlog/)
([GitHub](https://github.com/beyond10x/eventlog)) provider. It and `entity-sqlite` need Rust 1.91;
the rest of the workspace builds on 1.85. The workspace pins Eventlog at its 0.10.0 tag. Every provider is an
explicit Cargo feature:

| Feature | Provider | Boundary |
|---|---|---|
| `file` | `eventlog-file` | a caller-selected provider root |
| `sqlite` | `eventlog-sqlite` | a caller-selected database path and owner prefix |
| `postgres` | `eventlog-postgres` | the caller's connection configuration and pool bounds |
| `tree` | `eventlog-tree` | a directory of immutable files that version control can merge |
| `sync-bridge` | none | a synchronous facade over the asynchronous store and `entity-executor` |

`entity-sqlite` and `entity-postgres` expose the same recorded facades behind their
`eventlog-facade` feature. The `entity` command exposes Eventlog File behind `eventlog-providers`.

- **Provision, then open.** A provisioner prepares the provider and writes the immutable binding.
  `open` opens an already provisioned store without changing provider state, and refuses a missing
  binding or generation. Opening a newer provider does not convert a legacy store: import is a
  separate, explicit step.
- **One append, one group.** Each append — one decision or an ordered batch — is published as one
  Eventlog append group, so a multi-subject batch commits completely or not at all.
- **A command reads its own entities.** A command reads the binding stream, the streams of the
  subjects it names, the blobs those events bind and the index rows of its record ids, not the
  whole tenant. What it reads is still verified. A read that does not verify is taken again, and
  after three attempts one complete capture decides.
- **Bounded.** A command or batch that would leave the store holding more events, blobs or index
  rows than the handle's `CaptureLimits` is refused with `BatchExceedsReadBounds` before anything is
  uploaded, so the caller can divide the work.
- **Verified once per handle.** A handle remembers, in memory only, what it has already verified,
  so a later read hashes and decodes only what is new. A byte that differs is verified from
  scratch.
- **Scoped reads.** `RecordedProviderFacade::scoped` reads only the subjects a caller names, and
  `read_histories` reads several in one call, in the order requested.
- **Tree text kept once.** With `tree`, a new store is created as `eventlog-tree/2`, which keeps
  each long text once under its SHA-256. An `eventlog-tree/1` store opens and records as before, and
  `eventlog_tree::migrate` moves one explicitly. A build older than 0.25.0 recognizes the new
  format by its name and refuses it.
- **Tree reopens from its kept state.** Eventlog keeps the state a tree replay folded under
  `.cache/state/`, which is never committed. `ErRecordedProjector` reports a fingerprint, so a
  reopen of an unchanged tree by the same program loads that state instead of replaying every
  file. A changed file, another program or another projector configuration replays once.
- **Hosted PostgreSQL schema and roles.** Run `eventlog_postgres::PostgresEventStore::migrate`
  with the migration role, and `entity_eventlog::projection_specs()` as its projections, before an
  application-role open; provisioning does this itself. A hosted open refuses a schema an earlier
  Eventlog release migrated, and a history restored from a logical backup, until `migrate` has run
  on it. The application role needs `SELECT, INSERT, UPDATE, DELETE` on the other tables
  (`<prefix>_group_batches` among them) but only `SELECT` on `<prefix>_schema_version`,
  `<prefix>_projection_registry` and `<prefix>_settlement_floor`: a hosted open refuses a role
  that can change those three. A refused guarded write binds none of its blobs, as on the other
  providers.

On SQLite, `RecordedProviderFacade::start_with_read_policy` with `CapturePolicy::ProviderTracked`
reuses a verified observation when SQLite proves nothing changed, or verifies only a proven append
suffix. An open verifies the whole store, unless the store has durable open checkpoints enabled.
Edits that bypass SQLite are outside this guarantee. The default is `FullVerification`, whose open
always verifies the whole store.

After `RecordedProviderFacade::enable_durable_open_checkpoints`, a `Drain` shutdown persists the
last verified observation as the store's open checkpoint. The next `ProviderTracked` open verifies
that checkpoint, SQLite's proof that only acknowledged appends followed it, and those appends, so
its cost follows what was written since, not the store's size. `open_verification()` reports
`Complete`, `Checkpoint` or `Suffix`. A write through any SQLite connection in between makes the
next open complete.

A raw edit of the file is seen only by a `FullVerification` open or a complete read. The exception
is a read of an edited blob, which the blob's digest refuses. An edited index row is served.
Enabling checkpoints is one-way for older binaries: Entity Runtime 0.28.0 and earlier cannot open
the store until `disable_durable_open_checkpoints` runs. A store whose head is behind its
checkpoint refuses `ProviderTracked` opens until
`EventlogRecordedStoreOwner::discard_open_checkpoint` runs.

`AsyncImportedAnchorWriter::import_anchors` imports a batch of legacy histories in one append
group. An imported boundary records what was acquired from which source. It is not proof of
execution from genesis.

## Forked subjects and merge

A tree store lives in Git, so two branches can each record decisions and then merge as files. When
the provider keeps lineage, each `StoredRecord.lineage` carries the record's digest and parents,
and the history is verified branch by branch.

Two branches that changed different subjects merge into one store that keeps both. Only a
decision makes a head: an observation hangs off the decision it observed, so a branch that only
recorded an observation does not fork a subject. A subject on which both branches made a decision
is reported as `AsyncStoreError::Forked` with its heads. Other subjects keep serving, while reads
and ordinary writes of the forked subject are refused.

`entity_store::asynchronous::branch_heads` lists the decision heads and the state each reached.
`branch_tips` lists every record that no other record follows, observations included.
`BatchAction::Merge` joins a fork: it executes one operation on the state the chosen head reached,
at the highest revision any head reached, and the subject serves again at the next revision.

## Recorded execution and refusals

`entity-executor` runs complete recorded commands over asynchronous storage ports without choosing
an async runtime, a clock or an identifier. A retry with the same record identity, request,
metadata and expectation returns the original result, including after an uncertain commit.
`Executor::execute_versioned` and `batch_versioned` decide on a definition version the caller names
and return input-guarded refusals before checking whether the subject exists.

`Executor::recording_refusals` records each kernel refusal, expectation conflict and write to a
forked subject as a `RecordedRefusal` before returning it. An Eventlog store keeps one
`er.refused_request` event per distinct refusal on its own `er.refusal` stream. A retry refused for
the same reason is the same record, and no subject's revision moves.

## Replay and legacy history

A complete decision record holds the normalized command, the exact validated definition, the
result, the changed fields and the events. `entity_core::replay` executes the command again and
compares the whole outcome. Altered input, output or event evidence is refused.

`rehydrate` folds a legacy, event-only history. It checks every revision against the **current**
definition: one operation must emit exactly those events on that transition, accept the recorded
arguments, write the recorded changes and resolve every payload, and the folded fields must pass
the schema and invariants after each revision.

Without a definition snapshot, `rehydrate` cannot prove that the original commands passed the
definitions that decided them, and it cannot see a decision that emitted no event. It refuses a
`service/1` definition: replay those histories from their decision records. Data imported by the
File Store v2 migrator is marked with a legacy snapshot boundary, and verification starts with the
complete records written after it.

## Observations

Evidence about a subject that does not change its lifecycle is a recorded observation, stored
beside the decisions with its own provenance. `HistoryProvider` returns decisions and observations
in append order. An events read returns domain events only. The `entity` command, a generated CLI
and the MCP tools expose no history or observation verb, so read them through the provider library.

## Remote and hybrid failures

`Unreachable` is not `Absent`: a server that did not answer has not said whether an entity
exists. A hybrid store makes conflict policy explicit. Divergences survive the process that noticed
them and can be replayed with `catch_up`, which keeps what it could not replay instead of reporting
success. Matching current state does not prove matching history.
