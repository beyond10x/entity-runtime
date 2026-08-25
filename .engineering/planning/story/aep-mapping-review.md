---
format: aep.planning-md/1
id: story:aep-mapping-review
kind: story
status: draft
title: 'Phase 0: the AEP mapping is reviewed by both repositories'
summary: docs/design/engineering-protocols-adoption-v0.1.md is accepted, accepted in part or refused on a plan page in engineering-protocols, with the reason.
relations:
- derived_from: epic:drive-engineering-protocols
- informed_by: story:aep-lifecycles-as-definitions
revision: 4
---
# Story: Phase 0 — the AEP mapping is reviewed by both repositories

## Outcome

`docs/design/engineering-protocols-adoption-v0.1.md` is accepted, accepted in part or refused by
`engineering-protocols`, with the reason recorded on a plan page there, so later phases build from
a decision rather than a proposal.

## Context

**`engineering-protocols` has never been told this repository exists.** A grep for `entity-runtime`
and `entity-core` across its documents, artifact YAML and crates at `79b641c` returns nothing
(2026-08-25). This phase is not awaiting a verdict; it has not been put to the other side at all.

## Decided 2026-08-25 — the order is inverted

Phase 1 (`story:aep-lifecycles-as-definitions`) is built **first**, and is what phase 0 sends. A
paper review of a mapping table is weak evidence; eight definitions plus a test proving they yield
exactly the transitions their YAML declares is decidable. Phase 1 costs `examples/` in this tree,
changes nothing in theirs, and is thrown away for free if the verdict is no. The store still carries
the older `story:aep-lifecycles-as-definitions depends_on story:aep-mapping-review` edge because the
CLI has no way to remove an edge — `informed_by` records the real order, and the stale edge is itself
a small finding for the adoption argument.

## Where it lands

Not a new plan page. Their `story:open-vocabulary-audit` already asks this repository's question —
its acceptance is *one table over every adopter-facing declaration, open or closed, and for each
closed one the guarantee the closure buys*, opened by an adopter's meta-defect that *things the docs
invite an adopter to declare keep turning out to be fixed in the engine*. The mapping is the other
half of that table: for each closed vocabulary, what it costs to open it.

Lead with their backlog, not with our mapping: gap-register rows `:39`, `:70`, `:73` and `:77`, plus
`story:decision-with-default`, `story:time-based-transitions`, `story:blocker-relation` and
`story:outbound-claims-and-status-vocabulary`, are four Rust changes and four YAML edits respectively.

Name the collision in the same message: their `story:journal-backed-store` reroutes the markdown
store's writes through `CommandService` while phase 2 reroutes the same store's verdicts through this
kernel. Built independently, that seam is built twice.

## Acceptance

A plan page or story in `engineering-protocols` names the verdict; this repository's design header
is updated to match; refused rows are listed in `AGENTS.md` here as binding refusals.
