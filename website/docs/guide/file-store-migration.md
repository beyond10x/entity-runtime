---
sidebar_position: 11
title: Migrate File Store v1 to v2
description: Safely validate, migrate, verify, cut over, and roll back a File Store created before entity 0.15.
---

# Migrate File Store v1 to v2

Entity 0.15 introduced File Store v2. Older stores used caller-controlled names as paths and kept
state separate from event JSONL. V2 encodes path components and atomically replaces one complete
subject document. The formats are deliberately not opened interchangeably.

## Prepare

1. Stop every writer.
2. Identify the legacy root exactly.
3. Take a filesystem snapshot or backup.
4. Choose a destination path that does not exist.
5. Keep the pre-0.15 binary available for rollback.

Do not rename the legacy directory into place, edit either layout by hand, or run old and new writers
against one root.

## Validate without writing

```bash
entity store migrate-file \
  --from /srv/entity-v1 \
  --to /srv/entity-v2 \
  --dry-run
```

Dry-run reads and validates the complete source but creates no destination. It refuses malformed
state, incomplete JSONL, orphan logs, nested directories, symlinks, subject/path mismatches, an
existing destination, and unsupported layouts.

Resolve every reported source problem or choose a fresh destination. Do not route around a refusal
by deleting a record whose meaning is unclear.

## Migrate and verify

```bash
entity store migrate-file \
  --from /srv/entity-v1 \
  --to /srv/entity-v2

entity list --store /srv/entity-v2 --entity refund
```

The migrator builds a sibling staging directory and publishes the complete destination with one
rename. The source remains untouched. Verify every expected entity type and identity before pointing
writers at v2.

## Cut over

1. Keep writers stopped after migration.
2. Configure every 0.15-or-newer process to use the v2 destination.
3. Run read-only enumeration checks.
4. Start one writer and verify a recorded decision and its history.
5. Resume the remaining writers.

## Roll back

Stop writers and point them back at the retained v1 root using the pre-0.15 binary. Preserve the v2
directory for investigation.

Writes accepted after cutover are not reverse-migrated. Decide explicitly how to reconcile them
before resuming both sides; never copy individual v2 documents into v1.

## Replay boundary

Migrated events are retained and each subject is marked `legacy_snapshot`. Legacy records do not
contain the original normalized commands or definition snapshots. Replay verification begins with
new 0.15 decision records after migration; the imported prefix is not verified from genesis.
