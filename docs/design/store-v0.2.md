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

### 2.1 The record-id index (0.17.6)

R-88 makes a record id global to a store, so every recorded write — `commit_recorded` and
`observe` — must first answer *is this id already held anywhere*. Until 0.17.6 the File Store
answered by reading and parsing every subject document in the store on every write
(`record_document` walked `subjects/*/*.json`). That is O(subjects) per write and O(subjects²) per
import; measured on 2026-09-03 while replaying an adopter's history, 517 subjects written cost
24 GB read for 45 MB written, and the run did not finish in ten minutes.

From 0.17.6 a handle keeps a `RecordIndex`: record id → (entity, id, decision-or-observation).

| moment | what happens |
|---|---|
| `open` | nothing; the index is `None` |
| first lookup (`record_document`) | `scan_records` reads every subject once and fills the index — the same walk as before, run one time |
| a hit | the located subject alone is read and the matching envelope or observation returned |
| a miss | `None`, with no further read |
| after a successful `write_subject` in `commit_recorded` / `observe` | `remember` inserts the id, so the handle's own writes are found without a second scan |
| `clone` | the clone carries what the original knew at that moment |

The on-disk format is unchanged: no index file, no marker change, nothing to migrate; a reader
older than 0.17.6 reads the same bytes. The index lives behind a `Mutex<Option<RecordIndex>>`
so the handle stays `Send + Sync`; a poisoned lock is taken over rather than propagated, because
the index is a cache of what is on disk and never the authority.

**What the index does not do.** It is per handle. A second handle, or a second process, writing the
same directory is outside File Store's atomicity, which § 2 limits to one subject document; such a
writer's record ids are unknown to this handle until it is reopened. The store-global rule itself
is unchanged and is pinned by `tests/file_record_index.rs`: an id reused for different bytes is
`RecordConflict` and identical bytes are an idempotent success, across handles opened at different
times and for a handle's own writes; `the_file_provider_conforms` is unchanged.

After the change the same 9,085-call import completes in 34 s and reads 0.7 GB — the one scan plus
one subject read per lookup.

## 3. Hybrid truthfulness

Freshness is relative to the declared authority, not merely to whether a network request returned.
A divergence records source, destination and record id. Catch-up follows that direction, never
clears an unreadable or absent source, and never overwrites a destination that moved independently.
