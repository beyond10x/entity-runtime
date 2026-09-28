---
format: aep.planning-md/3
id: story:aep-evidence-preconditions
kind: story
status: implemented
title: 'Phase 3: implemented and accepted require evidence'
summary: Preconditions on the moves that today are claims nothing checks (gap register :39).
relations:
- derived_from: epic:drive-engineering-protocols
- depends_on: story:aep-move-through-kernel
revision: 6
transitions:
- {from: "draft", to: "proposed", at: "2026-08-26T01:36:32Z", actor: "timo", revision: 4, imported: true}
- {from: "proposed", to: "active", at: "2026-08-26T01:36:32Z", actor: "timo", revision: 5, imported: true}
- {from: "active", to: "implemented", at: "2026-08-26T01:36:46Z", actor: "timo", revision: 6, decided_on: {"recorded":{"test_result":1,"artifact":1}}, imported: true}
---
# Story: Phase 3 — implemented and accepted require evidence

## Outcome

Moving a story to `implemented` or an ADR to `accepted` has a precondition that names the evidence
it needs, so gap-register row :39 there ("a claim nothing checks") closes with a mechanism.

## Acceptance

Preconditions on the two moves in `examples/aep/`, evaluated three-valued; a move without the
evidence is refused as *unobservable*, not as *false*; the row in the gap register is closed by
`engineering-protocols` citing the mechanism.
