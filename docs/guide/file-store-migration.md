# Migrate a File Store to v2

0.15.0 changes the on-disk File Store format. Older stores used caller-controlled names as paths
and split state from event JSONL; v2 encodes path components and atomically replaces one complete
subject document. The formats are intentionally not opened interchangeably.

## Before the cutover

Stop every writer, identify the legacy root, choose a destination that does not exist, and retain a
filesystem-level backup or snapshot of the source. Do not rename the old directory over the new
one and do not edit either format by hand.

```console
entity store migrate-file --from /srv/entity-v1 --to /srv/entity-v2 --dry-run
```

Dry-run reads and validates the entire source but creates no destination. It refuses malformed
state, incomplete JSONL, orphan event logs, nested directories, symlinks, subject/path mismatches,
an existing destination, or an unsupported layout. Resolve every refusal in the source or choose a
fresh destination, then run the same command without `--dry-run`:

```console
entity store migrate-file --from /srv/entity-v1 --to /srv/entity-v2
entity list --store /srv/entity-v2 --entity order
```

The migration builds a sibling staging directory and publishes the complete destination with one
rename. The source remains byte-for-byte available. Verify the expected entity types and ids before
pointing writers at v2.

## Rollback

Stop writers, point them back at the retained v1 root using the pre-0.15 binary, and preserve the
v2 directory for investigation. Writes made after cutover are not reverse-migrated automatically;
decide explicitly how to reconcile them before resuming both sides.

Migrated events are retained, and each subject is marked `legacy_snapshot`. They do not contain the
original normalized commands or definition snapshots, so replay verification begins only with new
0.15 decision records; never describe the migrated prefix as verified from genesis.
