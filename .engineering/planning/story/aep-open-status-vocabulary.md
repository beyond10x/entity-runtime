---
format: aep.planning-md/3
id: story:aep-open-status-vocabulary
kind: story
status: implemented
title: 'Phase 4: an open status vocabulary'
summary: correction-owed and the other rungs the closed ArtifactStatus enum cannot hold (gap register :70), added as data.
relations:
- derived_from: epic:drive-engineering-protocols
- depends_on: story:aep-move-through-kernel
revision: 6
transitions:
- {from: "draft", to: "proposed", at: "2026-08-26T01:36:31Z", actor: "timo", revision: 4, imported: true}
- {from: "proposed", to: "active", at: "2026-08-26T01:36:31Z", actor: "timo", revision: 5, imported: true}
- {from: "active", to: "implemented", at: "2026-08-26T01:36:46Z", actor: "timo", revision: 6, decided_on: {"recorded":{"test_result":1,"artifact":1}}, imported: true}
---
# Story: Phase 4 — an open status vocabulary

## Outcome

`correction-owed` and the other rungs `ArtifactStatus` cannot hold (gap-register :70 there) are
added as states in a definition, with the operations that reach and leave them, and no Rust enum
changes.

## Acceptance

The new states appear in `examples/aep/`, `protocol artifact lifecycle <kind>` reports them, and
the gap-register row is closed by `engineering-protocols` citing the definition.
