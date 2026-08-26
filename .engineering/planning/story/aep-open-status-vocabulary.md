---
format: aep.planning-md/1
id: story:aep-open-status-vocabulary
kind: story
status: implemented
title: 'Phase 4: an open status vocabulary'
summary: correction-owed and the other rungs the closed ArtifactStatus enum cannot hold (gap register :70), added as data.
relations:
- derived_from: epic:drive-engineering-protocols
- depends_on: story:aep-move-through-kernel
revision: 6
---
# Story: Phase 4 — an open status vocabulary

## Outcome

`correction-owed` and the other rungs `ArtifactStatus` cannot hold (gap-register :70 there) are
added as states in a definition, with the operations that reach and leave them, and no Rust enum
changes.

## Acceptance

The new states appear in `examples/aep/`, `protocol artifact lifecycle <kind>` reports them, and
the gap-register row is closed by `engineering-protocols` citing the definition.
