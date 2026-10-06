---
sidebar_position: 9
title: Migrate a File Store to v2
description: Validate, migrate, verify, cut over and roll back a File Store created before entity 0.15.
lede: entity store migrate-file copies a pre-0.15 File Store into the v2 format out of place, and --dry-run proves it first without writing.
source: "crates/entity-store/src/file.rs (migrate_file_store_v1), crates/entity-cli/tests/cli.rs::file_store_migration_is_out_of_place_and_dry_run_writes_nothing, run with entity 0.27.0"
---

# Migrate a File Store to v2

Entity 0.15 introduced File Store v2. Older stores used caller-chosen names as paths and kept state
apart from an event log file per subject. v2 encodes path components and replaces one complete
subject document atomically. The two formats are deliberately not opened interchangeably.

## Prepare

1. Stop every writer.
2. Identify the legacy root exactly.
3. Take a filesystem snapshot or backup.
4. Choose a destination path that does not exist.
5. Keep the pre-0.15 binary for a rollback.

Do not rename the legacy directory into place, edit either layout by hand, or run old and new
writers against one root.

## Validate without writing

Here the legacy store `./entity-v1` holds one refund:

```shell-session
$ entity store migrate-file --from ./entity-v1 --to ./entity-v2 --dry-run
valid: 1 subject(s), 1 event(s); no files written
```

A dry run reads and validates the whole source and creates no destination. It refuses malformed
state, incomplete event logs, orphan logs, nested directories, symlinks, a subject whose content
disagrees with its path, an existing destination and unsupported layouts. Resolve each reported
problem; do not route around a refusal by deleting a record whose meaning is unclear.

## Migrate and verify

```shell-session
$ entity store migrate-file --from ./entity-v1 --to ./entity-v2
migrated 1 subject(s), 1 event(s) from ./entity-v1 to ./entity-v2
$ entity list --store ./entity-v2 --entity refund
refund-104
```

The migrator builds a sibling staging directory and publishes the complete destination with one
rename; the source is not modified. A second run is refused because the destination now exists
(exit 2):

```shell-session
$ entity store migrate-file --from ./entity-v1 --to ./entity-v2 --dry-run
error: the store failed: migration destination ./entity-v2 already exists
```

Check every expected entity type and id with `entity list` before pointing writers at v2.

## Cut over

1. Keep writers stopped after the migration.
2. Point every 0.15-or-newer process at the v2 destination.
3. Run read-only enumeration checks.
4. Start one writer and verify a recorded decision and its history.
5. Resume the remaining writers.

## Roll back

Stop the writers and point them back at the retained v1 root with the pre-0.15 binary. Keep the v2
directory for investigation. Writes accepted after the cutover are not migrated back: decide how
to reconcile them before resuming either side, and never copy individual v2 documents into v1.

## Replay boundary

Migrated events are kept and each subject is marked `legacy_snapshot`. Legacy records hold neither
the normalized commands nor the definition snapshots, so replay verification starts with the
complete records written after the migration; the imported prefix is not verified from genesis.
