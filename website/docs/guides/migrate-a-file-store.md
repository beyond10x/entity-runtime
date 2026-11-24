---
sidebar_position: 9
title: Migrate a File Store
description: Move a File Store to v2 (from before entity 0.15) or to v3 (from v2); validate, migrate, verify, cut over and roll back.
lede: entity store migrate-file copies a pre-0.15 File Store into v2 and entity store migrate-file-v3 copies a v2 store into v3, both out of place, and --dry-run proves each first without writing.
source: "crates/entity-store/src/file.rs (migrate_file_store_v1, migrate_file_store_v2), crates/entity-cli/tests/cli.rs::file_store_migration_is_out_of_place_and_dry_run_writes_nothing and ::a_0_33_1_file_store_migrates_to_v3_out_of_place_and_keeps_taking_commands, the v2 steps run with entity 0.27.0, the v3 steps run on a build of main f08eda98 against crates/entity-store/tests/fixtures/file-store-0.33.1"
---

# Migrate a File Store

A File Store root has one of three layouts. Read the root's `.entity-store-format` file to tell
which one you have:

| `.entity-store-format` | Layout | Written by | Migrate with |
|---|---|---|---|
| no such file in a non-empty directory | pre-0.15 | entity 0.14 and earlier | `entity store migrate-file` (to v2) |
| `entity.file-store/2` | v2 | entity 0.15 to 0.33.1 | `entity store migrate-file-v3` (to v3) |
| `entity.file-store/3` | v3 | entity 0.34.0 and later | nothing to migrate |

An unmarked non-empty directory is never guessed to be v2. Every migration is out of place: it
reads the source, never modifies it and publishes a destination that did not exist.

## Migrate a v2 store to v3

File Store v3 keeps a subject's current state apart from its append-only history, and keeps each
entity definition once per store. One recorded command appends one record to the subject's log and
replaces a small state document, where v2 rewrote the whole subject document. One `entity execute`
on a counter writes 917 bytes with 8 prior records and 928 with 32; on v2 it wrote 57,107 and
207,967.

A missing or empty root is created as v3 by entity 0.34.0 and later. An existing v2 root is read
and written as v2, unchanged, until you migrate it.

:::warning[Entity 0.33.1 and earlier refuse a v3 root]
They stop with `declares unsupported format "entity.file-store/3"`. Do not point an older release
at a store a newer one created, and keep the v2 root for a rollback.
:::

### Layout

| File under the root | Holds | Written |
|---|---|---|
| `.entity-store-format` | `entity.file-store/3` | once |
| `subjects/<hex entity>/<hex id>.log` | a header line, then one JSON line per decision record, observation, or state committed without a record | appended and synced, never rewritten |
| `subjects/<hex entity>/<hex id>.json` | the current instance and the history length it reflects | replaced by rename, not synced |
| `definitions/<digest>.json` | the entity definition, once per store, named by its digest | created and synced before the first record that names it |

A reader trusts the state document only while the history has exactly the length it names, and folds
the history otherwise, so a state document lost or left behind by a crash is rebuilt. A history
line cut short by a crash is ignored and removed by the next write. A complete line that does not
parse is refused with its line number.

### Prepare

1. Stop every writer.
2. Identify the v2 root exactly: its `.entity-store-format` reads `entity.file-store/2`.
3. Take a filesystem snapshot or backup.
4. Choose a destination path that does not exist.
5. Keep the older binary, or the v2 root alone, for a rollback.

The migration holds the source's shared lock until it ends, so a writer that starts meanwhile
waits and no write lands half-way through.

### Validate without writing

Here the v2 store `./entity-v2` holds five subjects:

```shell-session
$ entity store migrate-file-v3 --from ./entity-v2 --to ./entity-v3 --dry-run
valid: 5 subject(s), 4 event(s); no files written
```

A dry run reads and validates the whole source, encodes every subject as v3, decodes it again and
checks that it comes back identical, and creates no destination. It refuses a source that is not
a v2 root, unreadable or inconsistent source data, an existing destination and a destination inside
the source. Each refusal exits 2:

| Cause | Message after `error: the store failed: ` |
|---|---|
| the source is already v3 | `File Store ./entity-v2 is already v3; there is nothing to migrate` |
| the source is missing or empty | `./entity-v2 is not a File Store v2 root` |
| the destination exists | `migration destination ./entity-v3 already exists` |

### Migrate and verify

```shell-session
$ entity store migrate-file-v3 --from ./entity-v2 --to ./entity-v3
migrated 5 subject(s), 4 event(s) from ./entity-v2 to ./entity-v3
$ cat ./entity-v3/.entity-store-format
entity.file-store/3
$ cat ./entity-v2/.entity-store-format
entity.file-store/2
$ entity list --store ./entity-v3 --entity counter
c1
c2
legacy-1
p1
```

Every subject keeps its state, records, observations and events, in their order. The migrator
builds a sibling staging directory and publishes the complete destination with one rename. A
second run is refused because the destination now exists. Check every expected entity type and id
with `entity list` before pointing writers at v3.

### Cut over

1. Keep writers stopped after the migration.
2. Point every 0.34.0-or-newer process at the v3 destination.
3. Run read-only enumeration checks.
4. Start one writer and verify a recorded decision and its history.
5. Resume the remaining writers.

### Roll back

The v2 root is unmodified. Stop the writers and point them back at it. Any entity release from 0.15
on reads it. Keep the v3 directory for investigation. Writes accepted after the cutover are not
migrated back: decide how to reconcile them before resuming either side, and do not copy
individual v3 files into v2.

## Migrate a pre-0.15 store to v2

Entity 0.15 introduced File Store v2, which encodes path components and replaces one complete
subject document atomically. Older stores used caller-chosen names as paths and kept state apart
from an event log file per subject. The two formats are not opened interchangeably. The command
writes v2: to reach v3 from a pre-0.15 store, run it first and then `migrate-file-v3` on its
destination.

### Prepare

1. Stop every writer.
2. Identify the legacy root exactly.
3. Take a filesystem snapshot or backup.
4. Choose a destination path that does not exist.
5. Keep the pre-0.15 binary for a rollback.

Do not rename the legacy directory into place, edit either layout by hand, or run old and new
writers against one root.

### Validate without writing

Here the legacy store `./entity-v1` holds one refund:

```shell-session
$ entity store migrate-file --from ./entity-v1 --to ./entity-v2 --dry-run
valid: 1 subject(s), 1 event(s); no files written
```

A dry run reads and validates the whole source and creates no destination. It refuses malformed
state, incomplete event logs, orphan logs, nested directories, symlinks, a subject whose content
disagrees with its path, an event later than its subject's saved revision, an existing destination,
a destination inside the source and unsupported layouts. Resolve each reported
problem, and do not route around a refusal by deleting a record whose meaning is unclear.

### Migrate and verify

```shell-session
$ entity store migrate-file --from ./entity-v1 --to ./entity-v2
migrated 1 subject(s), 1 event(s) from ./entity-v1 to ./entity-v2
$ entity list --store ./entity-v2 --entity refund
refund-104
```

The migrator builds a sibling staging directory, publishes the complete destination with one
rename and leaves the source unmodified. A second run is refused because the destination now exists
(exit 2):

```shell-session
$ entity store migrate-file --from ./entity-v1 --to ./entity-v2 --dry-run
error: the store failed: migration destination ./entity-v2 already exists
```

Check every expected entity type and id with `entity list` before pointing writers at v2.

### Cut over

1. Keep writers stopped after the migration.
2. Point every 0.15-or-newer process at the v2 destination.
3. Run read-only enumeration checks.
4. Start one writer and verify a recorded decision and its history.
5. Resume the remaining writers.

### Roll back

Stop the writers and point them back at the retained v1 root with the pre-0.15 binary, and keep the
v2 directory for investigation. Writes accepted after the cutover are not migrated back: decide how
to reconcile them before resuming either side, and do not copy individual v2 documents into v1.

### Replay boundary

Migrated events are kept and each subject is marked `legacy_snapshot`. Legacy records hold neither
the normalized commands nor the definition snapshots, so replay verification starts with the
complete records written after the migration, and the imported prefix is not verified from genesis.
