---
format: aep.planning-md/1
id: story:file-store-v2-migration
kind: story
status: draft
title: Migrate File Store data safely to v2
summary: 0.15.0 uses a confined atomic file layout and an explicit out-of-place migration from legacy stores.
relations:
- decomposes: epic:the-store-an-adopter-runs-on
revision: 1
---
## Context

Legacy File Store paths embed caller-controlled entity and identity strings and persist state and events separately. The 0.15.0 format must close path escapes and partial writes without silently treating old directories as new data.

## Acceptance

`entity store migrate-file --from OLD --to NEW` validates without writes in dry-run mode, publishes a complete confined v2 destination atomically, preserves the source, records the legacy replay boundary, and the published website documents cutover and rollback.
