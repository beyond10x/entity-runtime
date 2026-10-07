---
format: aep.planning-md/3
id: design:wave-issue-54-entity-core-features
kind: design
status: draft
title: 'Wave: the three remaining entity-core features ESS lowering refuses (#54)'
tags:
- non-interactive
refs:
- provider: github
  reference: beyond10x/entity-runtime#54
relations:
- designs: story:operation-writes-an-optional-field-from-an-optional-argument
- designs: story:set-increments-a-numeric-field
- designs: story:set-clears-an-optional-field
revision: 1
---
# Wave: the three remaining entity-core features ESS lowering refuses (#54)

## Stage and authority

Skill version 0.20.1, `aep:implementing`, wave mode. Three stories of
`epic:ess-lowering-entity-core-features` (GitHub #54), run as three **sequential** units on one
integration branch: `aep plan artifact waves` puts each in its own wave because all three edit
`crates/entity-core/src/{definition,validation,runtime}.rs`, and `story:set-clears-an-optional-field`
depends on the other two. Each unit forks from the integration branch after the previous unit has
merged. One integration branch and one pull request for the three.

Run: non-interactive. The wave was approved under the operator's standing grant for this
repository's issue work, which also authorises the commits below; no operator turn is available
in this session.

Commits this wave makes: the opening coordinator commit (store moves, this page), each unit's
commits on its own branch, the merge of each unit branch into `wave/er-54-w2`, the store commits,
and the merge of `wave/er-54-w2` into `main` through one pull request.

This wave runs beside `design:wave-issue-55-bounded-open`, which touches only `entity-eventlog`,
`ess/provider-tracking` and `checks/ess-conformance`. The two meet only in `CHANGELOG.md` and the
store; whichever lands second merges `main` first.

## Selection

`aep plan artifact waves --kind story --status draft` (aep 0.68.0), before the stories moved:

```
wave 1
  story:operation-writes-an-optional-field-from-an-optional-argument
  story:recorded-open-verifies-a-checkpoint-and-its-suffix (inferred)
wave 2
  story:set-increments-a-numeric-field (inferred)
wave 3
  story:set-clears-an-optional-field (inferred)
3 wave(s), 15 collision(s), 19 unassessed
```

Collisions, verbatim:

```
collision: story:operation-writes-an-optional-field-from-an-optional-argument story:set-clears-an-optional-field crates/entity-core/src/definition.rs (inferred)
collision: story:operation-writes-an-optional-field-from-an-optional-argument story:set-clears-an-optional-field crates/entity-core/src/error.rs (inferred)
collision: story:operation-writes-an-optional-field-from-an-optional-argument story:set-clears-an-optional-field crates/entity-core/src/runtime.rs
collision: story:operation-writes-an-optional-field-from-an-optional-argument story:set-clears-an-optional-field crates/entity-core/src/validation.rs
collision: story:operation-writes-an-optional-field-from-an-optional-argument story:set-increments-a-numeric-field crates/entity-core/src/definition.rs
collision: story:operation-writes-an-optional-field-from-an-optional-argument story:set-increments-a-numeric-field crates/entity-core/src/error.rs (inferred)
collision: story:operation-writes-an-optional-field-from-an-optional-argument story:set-increments-a-numeric-field crates/entity-core/src/runtime.rs
collision: story:operation-writes-an-optional-field-from-an-optional-argument story:set-increments-a-numeric-field crates/entity-core/src/validation.rs
collision: story:set-clears-an-optional-field story:set-increments-a-numeric-field crates/entity-core/src/definition.rs (inferred)
collision: story:set-clears-an-optional-field story:set-increments-a-numeric-field crates/entity-core/src/error.rs (inferred)
collision: story:set-clears-an-optional-field story:set-increments-a-numeric-field crates/entity-core/src/lib.rs (inferred)
collision: story:set-clears-an-optional-field story:set-increments-a-numeric-field crates/entity-core/src/replay.rs (inferred)
collision: story:set-clears-an-optional-field story:set-increments-a-numeric-field crates/entity-core/src/runtime.rs
collision: story:set-clears-an-optional-field story:set-increments-a-numeric-field crates/entity-core/src/validation.rs
collision: story:set-clears-an-optional-field story:set-increments-a-numeric-field docs/design/kernel-v0.1.md (inferred)
```

The 19 unassessed stories are not candidates.

## Pre-flight

| check | value |
|---|---|
| base | `main` at `cfcba172` |
| free disk on `/` | 49G at wave open (`df -h /`), with the issue 55 unit building |
| other trees | `er-55-w1`, `er-55-u1` (the issue 55 wave) |
| build directories | in-tree `target/` per worktree |

## Units

| unit | story | stage | branch | worktree id | build dir | scratch |
|---|---|---|---|---|---|---|
| integration | — | opened | `wave/er-54-w2` | `er-54-w2` | in-tree `target/` | `build/er-54/` in the tree |
| U1 | `story:operation-writes-an-optional-field-from-an-optional-argument` | planned | `impl/operation-writes-optional-from-optional` | `er-54-u1` | in-tree `target/` | `build/scratch/` in the tree |
| U2 | `story:set-increments-a-numeric-field` | waits on U1 merge | `impl/set-increments-a-numeric-field` | `er-54-u2` | in-tree `target/` | `build/scratch/` in the tree |
| U3 | `story:set-clears-an-optional-field` | waits on U2 merge | `impl/set-clears-an-optional-field` | `er-54-u3` | in-tree `target/` | `build/scratch/` in the tree |

Briefs: `build/er-54/u<n>-brief.md` in the integration tree. Dispatch types: `aep:implementor`, then
`aep:adversary`.

## Decisions taken

- Sequential units in one wave instead of three waves: one pull request and one full gate for the
  three, as the one-pull-request-per-wave rule intends.
- Order U1 → U2 → U3: U3 depends on both; U2 decides the typed-assignment form U3 reuses.
