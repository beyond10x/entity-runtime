# Store design v0.2 — recorded history and File Store migration

**Status:** normative for 0.15.0. This document supersedes the envelope and File Store portions of
[`store-v0.1.md`](store-v0.1.md).

## 1. Recorded commits

`RecordedCommit` binds the complete resulting instance to an `Envelope<DecisionRecord>`. The
envelope requires a caller-supplied record id, valid recorded time, explicit correlation,
causation and actor fields (present `null` means absent), and the record. A record id is global to a
store: identical bytes are an idempotent retry; different bytes are `RecordConflict`.

`HistoryProvider` reads decision envelopes and non-state-changing `RecordedObservation`s in append
order. Memory, File, SQLite, PostgreSQL, Remote and Hybrid implement the contract. Integer
conversions are checked. Remote wire `/4` converts runtime values into explicit JSON documents;
PostgreSQL distinguishes an unreachable transport from a database response.

`AtomicBatchStore` remains optional and retains its 0.14 rollback contract. Recorded single-subject
writes are the required provenance path used by the CLI.

## 2. File Store v2

A v2 root contains `.entity-store-format` with `entity.file-store/2`. Each subject is one JSON
document under `subjects/<hex entity>/<hex id>.json`; caller-controlled entity names and ids never
become path syntax. Existing path components and subject files may not be symlinks. A write creates
and syncs a temporary document, renames it over the subject, then syncs the containing directory,
so current state and history cannot tear apart.

An unmarked non-empty directory is never guessed to be v2. `entity store migrate-file --from OLD
--to NEW` is the only supported v1 transition: it validates every state and complete JSONL record,
rejects symlinks, nesting, orphan logs and partial trailing records, writes a sibling staging tree,
and publishes it by rename. The destination must not exist and the source is never modified.
`--dry-run` performs the validation without creating destination bytes. Migrated subjects carry a
`legacy_snapshot` origin and retain legacy events, but have no invented decision records.

## 3. Hybrid truthfulness

Freshness is relative to the declared authority, not merely to whether a network request returned.
A divergence records source, destination and record id. Catch-up follows that direction, never
clears an unreadable or absent source, and never overwrites a destination that moved independently.
