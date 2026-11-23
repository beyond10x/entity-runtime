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
become path syntax. Each name is one path component of at most 255 bytes, so an entity type over
127 bytes, or an identity whose temporary document name (`<hex id>.json.writing.<pid>.<n>`) would
exceed 255 bytes — about 116 bytes of identity — is refused with `StoreError::Backend` before
anything is written. Existing path components and subject files may not be symlinks. A write creates
and syncs a temporary document, then renames it over the subject, so current state and history
cannot tear apart. On Unix the temporary document is created `0600` and, when it replaces a
subject, takes that subject's permission bits before any data is written, so a replacement never
widens what an operator restricted. Unix also syncs containing directories, and the parent of
every directory a write creates (`subjects/` and a type directory on the first write of a type);
Windows flushes file contents but does not promise directory-entry persistence across power loss. Recognizable abandoned subject
temporary files are ignored by enumeration and record-id scans. All existing path components,
including the format marker, are checked on reads and writes.

Threat model: the File Store trusts the directory it is rooted in. Every operation resolves its
path under the root again, and the operating system follows symlinks while doing so. The symlink
checks above refuse a symlink that is already in place; they cannot confine an open, create or
rename against a directory another process replaces between the check and the call. Only the
store's principal — the account its readers and writers run as — may modify the root and its
ancestors. A root another principal can modify is not supported: the advisory store lock
coordinates cooperating writers and is no defence against anyone else.

An unmarked non-empty directory is never guessed to be v2. `entity store migrate-file --from OLD
--to NEW` is the only supported v1 transition: it validates every state and complete JSONL record,
holds each subject to the recorded importer's boundary (a terminal revision in `1..=i64::MAX`, no
event later than it), rejects symlinks, nesting, orphan logs and partial trailing records, writes
a sibling staging tree, and publishes it by rename. The destination must not exist and must not
lie inside the source (both resolved canonically, dry runs included), and the source is never
modified.
`--dry-run` performs the validation without creating destination bytes. Validation holds one
subject at a time and keeps only counts and identities; the real run then reads the source a
second time and writes each subject as it reads it. Migrated subjects carry a
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

From 0.17.7 every write holds an OS advisory exclusive lock on `.entity-store-lock` across root
initialization, global record-id lookup, revision checking and subject publication. The lock file
is never unlinked. Its length is an invalidation epoch, advanced by one synced byte after the
write's checks pass and before its first durable change: a handle invalidates its cached index if
another writer advanced the epoch. A refused write (a revision or record conflict) and an
idempotent retry change nothing and leave the epoch alone. A process crash releases
the lock. This retains the single-writer index benefit while serializing independent handles and
processes. Multi-subject transactions remain unsupported. All concurrent writers must use 0.17.7
or later and a filesystem that supports advisory locking and atomic rename; older writers do not
participate in this protocol. The subject and format-marker bytes remain v2.

`tests/file_record_index.rs` pins concurrent revision conflicts, cache invalidation, orphan-file
tolerance and read-path symlink refusals. `verify_recorded` also checks mixed recorded/unrecorded
event history: revision order and repeated equal emissions are both preserved.

After the change the same 9,085-call import completes in 34 s and reads 0.7 GB — the one scan plus
one subject read per lookup.

### 2.2 File Store v3: state apart from append-only history

A v2 subject document holds the state and the whole history, so every recorded write rewrote all
of it: with 8 prior records one `entity execute` wrote 57,107 bytes, with 32 it wrote 207,967 (KPI
`file.execute.write_bytes.prior8` / `.prior32`, review finding P14), and every record embedded its
entity definition again.

A v3 root (`.entity-store-format` = `entity.file-store/3`) is what a handle creates for a missing
or empty root. Per subject it holds:

| file | what | how it is written |
|---|---|---|
| `subjects/<hex entity>/<hex id>.log` | the history: a header line naming the subject and its origin, then one JSON line per decision record, observation, or state written without a record (a plain commit) | appended, then synced; never rewritten |
| `subjects/<hex entity>/<hex id>.json` | the current instance and the history length it reflects | replaced by rename; not synced |
| `definitions/<64 hex>.json` | `["er.definition/1", definition]`, canonical, once per store | created and synced before the first line that names it |

A record line spells `record.definition` as the digest `definition_digest` gives the definition
document (the `er.record/5` rule of the Eventlog adapter) or `null`; `history()` returns the record
with the definition resolved, so its bytes and digests are the same in either record form. A
definition document is held to its file name on every read; a mismatch is `StoreError::Backend`.

The history is the authority and its sync is the commit point. A reader trusts the state document
only when the history still has exactly the length it names, and folds the history otherwise, so a
state document lost, torn or left behind by a crash is rebuilt rather than served. A history line
without its newline is an interrupted write: readers ignore it, the next writer cuts it before
appending, and an acquisition refuses it as it refuses a stale v2 temporary document. Every
complete line must parse; a corrupt one is refused with its line number. A subject's files are
created `0600` on Unix; the replaced state document takes the bits of the one it replaces, and the
history keeps its inode and so its bits.

One recorded write on an existing subject syncs the epoch and the history (v2: epoch, document
and directory); a new subject adds one directory sync, a new definition one file and one directory
sync. Reads of a whole subject still read its whole history (the record-id scan of a cold handle
reads every subject), but the history no longer repeats the definition.

A v2 root stays v2 and is read and written as before. `entity store migrate-file-v3 --from V2
--to V3 [--dry-run]` (`migrate_file_store_v2`) copies it out of place under the v2 store's shared
lock: records and observations in their order, then one state line holding the v2 instance and its
events, so the fold ends where the v2 document did. Each subject is encoded and decoded before
anything is written and must come back identical. The destination is staged beside it and published
by one rename; the source is never modified. Entity 0.33.1 and earlier refuse a v3 root as
`declares unsupported format "entity.file-store/3"`, so roll back by pointing the old release at the
retained v2 root. The migration is pinned against a v2 store written by the 0.33.1 library
(`crates/entity-store/tests/fixtures/file-store-0.33.1/`).

## 3. Hybrid truthfulness

Freshness is relative to the declared authority, not merely to whether a network request returned.
A divergence records source, destination and record id. Catch-up follows that direction, never
clears an unreadable or absent source, and never overwrites a destination that moved independently.
Catch-up transfers exact recorded envelopes and observations, inserting observations before moving
beyond their revisions. Equal state alone cannot establish that evidence was replicated. Missing
legacy prefixes, missing records behind the destination revision, or observations whose revision
the destination has already passed retain the divergence for explicit repair. Catch-up never
converts those records into event-only legacy imports or silently clears their divergence. A
legacy import appends only the source's events above the destination's revision, so it runs only
when the destination's events up to that revision are exactly the source's; a destination whose
earlier events differ moved independently and keeps its divergence.

`Store::history` advertises optional recorded-history access without changing the remote wire
contract. The shipped providers expose it. Custom wrappers must forward this capability to support
recorded catch-up and shared-shell retries; unavailable history is an explicit catch-up refusal.
