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
revision: 2
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
| U1 | `story:operation-writes-an-optional-field-from-an-optional-argument` | implemented; merged as `eb3e275a` | `impl/operation-writes-optional-from-optional` | `er-54-u1` | in-tree `target/` | `build/scratch/` in the tree |
| U2 | `story:set-increments-a-numeric-field` | implemented; merged as `a70a22e5` | `impl/set-increments-a-numeric-field` | `er-54-u2` | in-tree `target/` | `build/scratch/` in the tree |
| U3 | `story:set-clears-an-optional-field` | implemented; merged as `79aaf49d` | `impl/set-clears-an-optional-field` | `er-54-u3` | in-tree `target/` | `build/scratch/` in the tree |

Briefs: `build/er-54/u<n>-brief.md` in the integration tree. Dispatch types: `aep:implementor`, then
`aep:adversary`.

## Decisions taken

- Sequential units in one wave instead of three waves: one pull request and one full gate for the
  three, as the one-pull-request-per-wave rule intends.
- Order U1 → U2 → U3: U3 depends on both; U2 decides the typed-assignment form U3 reuses.

## U1 round 1 (implementor)

Commit `8b7d4dab` on `impl/operation-writes-optional-from-optional`. Red first: `task ess-check`
434/439 with the scenarios written and no code; `service_binding_boundary` 15 passed, 7 failed, then
18/4 with only the refusal lifted (the field was not written). Final, per command, all exit 0:
`entity-core` 365 → 372 passed, `entity-store` 73 → 73, `entity-eventlog --lib` 80 passed,
`ess-check` 439/439 (base 435), `req-check` 129 requirements and 0 findings, `docs-check`,
`example-check`; `ess specify validate --path ess` (0.55.0) valid.

Decisions the implementor took: a `required` destination is not admitted on an operation (creation
target rules kept; admitting later is additive); `ConditionalSetOnOperation` removed (nothing can
return it; no consumer names it at its `origin/main`); `fulfills` plus `set_if_present` on one
field refused as the new `FulfillmentConditionalSetConflict`. Three patches left for the
coordinator: the `service/2` row of `website/docs/concepts/service-semantics.md`, the conflict rule
in `docs/design/service-operation-field-fulfillment-v0.1.md`, and a status-page entry held for
release preparation.

Agent cost: 325,450 tokens, 180 tool uses, 19.5 min.

## U1 adversary pass 1 and correction

`review-result:er-54-u1-adversary-pass-1`: 4 findings. Introduced and fixed in `81baf64b`: an
operation `set_if_present` under an argument parent with a default overwrote the stored field
(refused at registration now); the public `service/2` row and the step-8 table rows omitted the
operation write. Pre-existing, filed as `story:creation-set-if-present-ignores-parent-defaults`:
the same parent-default gap on creation outcomes, which this story's "same bytes" acceptance keeps
open; the adversary case asserts today's behaviour and names that story. The implementor found the
same class for `responds_if_present` and `payload_if_present` (pre-existing; to be added to that
story). Coordinator read the correction diff: no assertion dropped. Merged as `eb3e275a`.
Agent cost: adversary 224,375 tokens, 101 tool uses, 12 min; correction 370,770 tokens, 43 tool
uses, 6 min.

## U2 round 1 (implementor)

Commits `9585b439` (specification), `353ee095` (feature), coordinator commit for the `entity-cli`
refusal arm and one design-table line. Red first: `ess-check` 439/445 (the six new scenarios);
`--test replay` read the increment as an object template (`expected integer`). Final, exit 0 per
command: `entity-core` 379 → 394, `entity-store` 73, `entity-yaml` 29 → 30, `entity-eventlog --lib`
80, `ess-check` 445/445, `req-check` 130 requirements (R-164 new), `docs-check`, `example-check`.

Decisions recorded in `docs/design/kernel-v0.1.md` § 3.3: a `set` value is a typed assignment when
it is a one-key mapping whose key is a keyword (`increment` today, `cleared` next); `set` keeps its
`Value` type, so record snapshots, Eventlog `Debug` pins and YAML loading are unchanged. Overflow is
refused at step 8 as `CoreError::IncrementOverflow`; binary64 targets are refused at registration;
no semantics gate (kernel/1 included); `number` sums are exact decimal, with operands spanning more
than 1,024 decimal places refused. Open for the ESS lowering: exact decimal versus binary64 sums.
Agent cost: 406,670 tokens, 124 tool uses, 18 min (plus the run stopped by the rate limit).

## U2 adversary pass 1 and coordinator correction

`review-result:er-54-u2-adversary-pass-1`: 3 introduced findings, all fixed by the coordinator in
`7b82f305` (one line each): `docs/design/kernel-v0.1.md` § 6 names `IncrementOverflow` at the set
step; the changelog names `increment_on_create` for the reserved literal on a creation; the
adversary's five cases are committed, the creation case re-pinned to `increment_on_create`, and the
case pinning that a defaulted optional `$fields` amount is refused. Arithmetic, overflow and replay
held under 3,000 drawn decimal pairs and 800 integer pairs. Merged as `a70a22e5`. Agent cost:
adversary 255,661 tokens, 76 tool uses, 15 min.

## Merge of main (0.29.0)

`f2632099`: the wave takes release 0.29.0; the changelog keeps this wave's entries under
Unreleased; the status page regenerated.

## U3 round 1, adversary pass 1 and correction

Commits `0bb38b3e` (specification), `8b26f7e1`, `c466fcd7`. Red first: `ess-check` 445/451 (the
six new scenarios). Decisions: clearing an absent field still names it in `removed`; no clear on a
creation (`ClearOnCreate`); a field with a default may be cleared; conflicts refused as
`ConditionalTargetConflict`, `FulfillmentSetConflict`, `SetAssignmentConflict`. Agent cost:
362,245 tokens, 209 tool uses, 25 min.

`review-result:er-54-u3-adversary-pass-1` (5 findings): the generated OpenAPI and AsyncAPI event
schemas lacked `removed` (introduced for clears, pre-existing for service/3 `Remove`; both fixed in
`33016024`); three documentation corrections on mixed-version behaviour (a 0.29.0 binary writes the
literal `{"cleared": true}` into a field whose schema admits it; entity-core before 0.19.0 drops
`removed` from an event; a mapping of both keywords is refused). The adversary ran 0.29.0 and 0.17.6
readers against this branch's records: 0.29.0's verifying paths refuse them. Coordinator decision
`0afc679c`: no new record framing, because `removed` is written only when not empty. Agent cost:
adversary 321,121 tokens (plus a run stopped by a rate limit); correction 409,468 tokens, 47 tool
uses, 5 min.

## Website katex

`ba32d873`: `katex` overridden to 0.19.0 (0.18.11 is deprecated on npm); `npm audit` in `website/`
finds 0 vulnerabilities (4 low before); `task site-build` exit 0. `task:website-katex-passes-its-advisory`.

## Trees

`er-54-u1`, `er-54-u2`, `er-54-u3` finished, archived and removed; `er-54-u2` and `er-54-u3` after
`cargo clean` of the nested build copies their scratch held (`decision-blocker:er-54-u2-tree-retirement`).

## Gate

Full `task check` on `79aaf49d` in this tree's own `target/`, each step alone, private `TMPDIR`, load
19.77: fmt-check 0, clippy 0, test 0 (738 passed; `clear_adversary`, `increment_adversary`,
`adversary_operation_set_if_present` and `adversary_clear_events` ran from this tree), doc-check 0,
docs-check 0, example-check 0, req-check 0 (131 requirements, 0 findings), pin-check 0,
postgres-check 0 (**skipped**: `ENTITY_POSTGRES_URL` unset; CI runs it), eventlog-runtime-check 0
(232 passed, 7 ignored), notes-check 0, ess-check 0 (451/451), provider-ess-check 0 (31/31).

## Release

Not part of this wave: 0.29.0 was cut the same morning, so these features ship with the next batch.
