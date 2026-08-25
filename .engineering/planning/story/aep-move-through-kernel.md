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
revision: 6
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

## Built 2026-08-25

`engineering-protocols` `crates/aep-backend-markdown/src/kernel.rs`, reached from
`Document::move_status`. `entity-core` is a dependency of that crate, pinned by git revision
`7656cf4` — the arrow of `atlas/architecture/adr/0002`, now real. Their full gate exits 0.

**The verdicts are identical, and that is checked by exhaustion.**
`crates/aep-backend-markdown/tests/kernel_equivalence.rs` compares the kernel's answer with
`ArtifactLifecycle::permits_transition` for every kind either planning store holds and **every
ordered pair of the ten statuses** — 800 pairs, of which about 90% are illegal, so agreement is a
claim rather than a tautology. It also covers the permissive fallback a kind with no ladder gets,
and a custom kind reaching a ladder through `ArtifactKind::parent`. Verified by breaking the
translation: three of the five tests fail, naming the exact edges that moved.

The kinds are a committed fixture (`tests/fixtures/store-kinds.md`) rather than read from the
stores, for the reason phase 1's pin is committed: a test that reads a sibling checkout says
something different on a machine that has none. `agentic-principles` ships no ladders of its own —
it is governed by these eight — so its contribution is the kinds it uses, recorded at `8c1460b`.

**One decision the design got wrong, and phase 2 did not follow.** § 2 said
`move --to implemented` becomes `execute --operation implement`. It does not: the operations are
named for the **target status**, because a verb vocabulary is a published surface on their side and
nobody has agreed to one. `story:entity-runtime-mapping` asks for that decision and has not had it.
Phase 2 therefore changes no verdict *and* introduces no name that is not already theirs. The
verb-named definitions in `examples/aep/` stay a proposal.

**A precondition that was not met.** ADR 0002's order of moves puts the manifest line after phase
0's verdict. There is no written verdict: `story:entity-runtime-mapping` is `draft` in their store,
and this was built on the operator's instruction instead. The dependency is reversible in one
commit — delete the module and the lookup it replaced is still standing behind it — so the risk this
takes is small and named rather than unnoticed.
