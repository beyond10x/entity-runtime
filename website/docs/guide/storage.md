---
sidebar_position: 6
title: Persist and replay decisions
description: Choose a provider, record provenance, prevent lost updates, and understand what replay proves.
---

# Persist and replay decisions

The kernel returns a value and performs no IO. A shell decides whether to store it, publish its
events, or discard it. Durable systems should store the resulting state, decision record, and events
as one accepted write.

## The write contract

Every commit says what the caller expected:

- `Expect::Absent` for creation;
- `Expect::Revision(n)` for an operation based on revision `n`.

The provider checks the expectation before writing. If another writer has already advanced the
subject, the commit returns `RevisionConflict` and writes nothing. Reload and re-decide; never patch
the newer state with an older result.

`RecordedCommit` adds an envelope around the decision. The shell supplies:

- a globally meaningful record ID used for idempotency;
- a validated ISO-8601 recorded-at time;
- an actor, or an explicit statement that there was no actor;
- optional correlation and causation IDs.

Reusing a record ID for identical bytes succeeds. Reusing it for different bytes is a
`RecordConflict`.

The shared stored runtime, generated CLI, and MCP tools recognize an exact operation retry even
after the subject has advanced. Keep the original record ID, metadata, arguments and expected
revision; a new request still checks the current revision.

## Provider guide

| Provider | Best for | Important boundary |
|---|---|---|
| `MemoryStore` | tests and process-local experiments | nothing survives the process |
| `FileStore` | the `entity` CLI and local single-root storage | one subject document is replaced atomically; use v2 only |
| `SqliteStore` | embedded durable applications | state, history, and events share a database transaction |
| `PostgresStore` | centralized multi-process deployments | the caller opens the connection and chooses transport/TLS |
| `RemoteStore` | using a store behind an application-owned transport | this crate defines a versioned JSON protocol, not an HTTP client |
| `Hybrid` | explicit local/remote authority and offline behavior | authority, read path, unreachable behavior, and divergence behavior have no defaults |

The CLI's `--store` flag uses `FileStore`. SQLite, PostgreSQL, Remote, and Hybrid are Rust library
integrations; the command does not pretend a filesystem path is a database connection.

`MemoryStore`, `SqliteStore`, and `PostgresStore` also implement `AtomicBatchStore` for ordered,
multi-subject batches that commit completely or roll back completely. File Store atomicity is per
subject document, not an arbitrary multi-subject transaction.

File Store 0.17.7 serializes concurrent writers to one root and refreshes cached record identities
when another writer changes the store. Upgrade every writer together: older binaries do not take
the lock. Use a filesystem that supports advisory locks and atomic rename. Subject data is flushed
before replacement; Unix also flushes directories, while Windows does not promise directory-entry
persistence across power loss. Abandoned temporary subject files do not block reads or later writes.

## Replay and legacy history

A complete decision record contains the normalized command, exact validated definition snapshot,
result, changed fields, and events. `entity_core::replay` executes that command again and compares
the complete outcome. Altered input, output, or event evidence is refused.

Legacy event-only history can be folded with `rehydrate`, but it does not prove that the original
commands would have passed the original definitions. Data imported by the File Store v2 migrator is
marked with a legacy snapshot boundary. Replay verification begins with new complete records after
that boundary; do not claim verification from genesis.

## Observations

Some evidence concerns a subject without changing its lifecycle revision. Recorded observations are
stored separately from state-changing decisions and retain their own provenance. Providers return
decisions and observations in append order through `HistoryProvider`.

## Remote and hybrid failures

`Unreachable` is not `Absent`. A server that did not answer has said nothing about whether an entity
exists. Preserve that distinction in retries, user messages, and agent tools.

A hybrid store makes conflict policy explicit. Divergences survive the process that noticed them and
can be replayed later with `catch_up`; they are never silently treated as synchronized.
Catch-up preserves recorded envelopes and observations. If the destination has already passed
missing evidence or lacks a legacy prefix needed for replay, the divergence remains visible for
explicit repair. Matching current state is insufficient to prove matching history.

For an existing local store, follow the [File Store v2 migration](./file-store-migration) before
using a 0.15 or newer binary.
