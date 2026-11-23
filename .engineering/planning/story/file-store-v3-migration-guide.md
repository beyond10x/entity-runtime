---
format: aep.planning-md/3
id: story:file-store-v3-migration-guide
kind: story
status: draft
title: The website guide describes File Store v3 and its migration before the release that ships it
summary: migrate-a-file-store.md still describes v2 only; the CHANGELOG and entity-skill.md carry the v3 steps
revision: 1
---
## Outcome

The website guide `website/docs/guides/migrate-a-file-store.md` (and any page that names the File
Store's on-disk form) describes File Store v3 and `entity store migrate-file-v3`, before the
release that ships v3.

## Evidence

- story:file-store-v3-state-and-append-only-history shipped v3 with the operator steps in
  CHANGELOG [Unreleased] and `crates/entity-cli/assets/entity-skill.md`, and the generated CLI
  reference; the guides were not updated (r1-file-store hand-back, D2 option A on default).

## Acceptance

- The guide gives the steps the CHANGELOG gives: stop every writer, `--dry-run`, the run,
  `entity list --store <new root> --entity <type>`, point writers at the new root, keep the v2
  root to roll back with the older release; and says 0.33.1 and earlier refuse a v3 root.
- Site build and docs-check green.
