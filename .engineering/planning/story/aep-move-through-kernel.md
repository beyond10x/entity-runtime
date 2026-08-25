---
format: aep.planning-md/1
id: story:aep-move-through-kernel
kind: story
status: draft
title: 'Phase 2: protocol artifact move evaluated by the kernel'
summary: Identical accept/refuse verdicts on the org's planning stores, behind the existing CLI.
relations:
- derived_from: epic:drive-engineering-protocols
- depends_on: story:three-valued-conditions
- depends_on: story:aep-lifecycles-as-definitions
revision: 5
---
# Story: Phase 2 — protocol artifact move evaluated by the kernel

## Outcome

`protocol artifact move` asks this kernel whether the move is permitted, behind the existing CLI,
refusing exactly what it refuses today.

## Context

Depends on `story:three-valued-conditions` (invariant 5 there) and `story:aep-lifecycles-as-definitions`.
How the kernel is reached — a dependency, a vendored copy, a process boundary — is an ADR in `atlas`
first. Both repositories are public as of 2026-08-25, so the open question is the arrow's direction,
not this repository's visibility.

Two coordination facts, recorded here because they are cheap now and expensive later:

* `engineering-protocols` has no mention of this repository at any commit through `79b641c`
  (`grep -rl entity-runtime` over its documents, artifact YAML and crates: no hits, 2026-08-25).
  This story's parent phase 0 has not been put to the other side at all.
* Their `story:journal-backed-store` reroutes the markdown store's writes through `CommandService`;
  this story reroutes the same store's verdicts through the kernel. Built independently, that seam
  is built twice.

## Acceptance

On the planning stores of `engineering-protocols` and `agentic-principles`, every legal and illegal
move produces the same verdict through the kernel as through `LifecycleRegistry`; the comparison is
a committed test with the store snapshots as fixtures.
