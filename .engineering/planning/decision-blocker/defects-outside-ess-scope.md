---
format: aep.planning-md/3
id: decision-blocker:defects-outside-ess-scope
kind: decision-blocker
status: cleared
title: Two filed defects lie outside the declared ESS scope
relations:
- blocks: story:service-1-moves-outcome-loads-from-yaml
- blocks: story:generated-yaml-contracts-write-numbers
revision: 3
transitions:
- {from: "open", to: "cleared", at: "2026-10-07T09:09:57Z", actor: "human:timo", revision: 3}
---
# Two filed defects lie outside the declared ESS scope

`story:service-1-moves-outcome-loads-from-yaml` is fixed in `crates/entity-yaml` and
`story:generated-yaml-contracts-write-numbers` in `crates/entity-surface`. `AGENTS.md` § Which
documents are normative composes ESS for `entity-core`, `entity-store`, `entity-executor`,
`entity-shell` and `entity-query` only (`ess/ess-inputs.yaml` components), so the specification
cannot express either fix, and the organisation rule is that every change starts in the
specification.

Options: (A) fix both with tests only, naming the declared scope, and file a story to retrofit ESS
for both crates; (B) retrofit first, then fix; (C) leave both out until the crates' ESS scope is
decided. Recommended: A. Until decided, both stay out of the next wave.

## Decided, 2026-10-07

Option A. A defect in a crate outside the declared ESS scope is fixed with a red test when the fix
adds no noun, command or model, and the pull request names the declared scope. Both stories join
the next wave with the four ESS-covered defect stories. Shipped behaviour without a specification
gets a retrofit story, planned for a later wave: `story:entity-yaml-and-entity-surface-are-specified`.
