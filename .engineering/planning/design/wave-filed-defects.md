---
format: aep.planning-md/3
id: design:wave-filed-defects
kind: design
status: draft
title: 'Wave: six filed defects'
tags:
- non-interactive
relations:
- designs: story:binder-elements-carry-their-declaration
- designs: story:declared-collection-count-answers-its-declared-kind
- designs: story:generated-yaml-contracts-write-numbers
- designs: story:projection-keys-read-collection-addresses
- designs: story:service-1-moves-outcome-loads-from-yaml
- designs: story:service-response-is-checked-against-its-schema
revision: 1
---
# Wave: six filed defects

## Stage and authority

Skill version 0.20.1, `aep:implementing`, wave mode. Six defect stories filed in earlier waves,
scoped in `b97e5a7d`, run as units on one integration branch, `wave/er-def-w3`, and one pull
request. Release 0.30.0 follows the wave, or main as it stands by 2026-10-08T09:13Z, whichever
comes first.

Run: non-interactive. The wave was approved under the operator's standing grant for this
repository's issue work, which also authorises the commits below; no operator turn is available
in this session.

Commits this wave makes: the opening coordinator commits (scoping, the ESS-scope decision, this
page), each unit's commits on its own branch, the merge of each unit branch into `wave/er-def-w3`,
the store commits, and the merge of `wave/er-def-w3` into `main` through one pull request.

## Selection

`aep plan artifact waves --kind story --status draft` (aep 0.68.0), before the stories moved:

```
wave 1
  story:binder-elements-carry-their-declaration (inferred)
wave 2
  story:declared-collection-count-answers-its-declared-kind (inferred)
wave 3
  story:generated-yaml-contracts-write-numbers (inferred)
wave 4
  story:projection-keys-read-collection-addresses (inferred)
wave 5
  story:service-1-moves-outcome-loads-from-yaml (inferred)
wave 6
  story:service-response-is-checked-against-its-schema (inferred)
6 wave(s), 72 collision(s), 15 unassessed
```

The verb and the coordinator disagree, and the disagreement is this: every pair of the six collides
on `CHANGELOG.md` and `docs/requirements.md`, and the ESS-covered ones on `ess/ess-inputs.yaml`,
`ess/coverage.json` and `ess/generated/suite.json`. Those are shared files the coordinator merges
(changelog lines and requirement rows are appended; R-numbers are handed out in the briefs; the
generated ESS files are regenerated after each merge). The collisions on code are three
`crates/entity-core/src/runtime.rs` stories, which run in sequence (declared count, then binder,
then response check: the binder scope names the order), and the test files
`crates/entity-core/tests/{adversary_text_count,security_text_count,service_values}.rs`, where the
binder and projection-key stories rewrite different named cases.

## ESS route

`story:service-1-moves-outcome-loads-from-yaml` (`entity-yaml`) and
`story:generated-yaml-contracts-write-numbers` (`entity-surface`) are outside the declared ESS
scope. Decided: fixed with a red test each, the pull request names the declared scope, and
`story:entity-yaml-and-entity-surface-are-specified` retrofits ESS later
(`decision-blocker:defects-outside-ess-scope`).

## Units

| unit | story | round | R-number | branch | worktree id |
|---|---|---|---|---|---|
| U1 | `story:declared-collection-count-answers-its-declared-kind` | 1 | R-166 | `impl/declared-collection-count` | `er-def-u1` |
| U2 | `story:projection-keys-read-collection-addresses` | 1 | R-167 | `impl/projection-keys` | `er-def-u2` |
| U3 | `story:service-1-moves-outcome-loads-from-yaml` | 1 | none expected | `impl/moves-outcome-yaml` | `er-def-u3` |
| U4 | `story:generated-yaml-contracts-write-numbers` | 2 | none expected | `impl/yaml-contract-numbers` | `er-def-u4` |
| U5 | `story:binder-elements-carry-their-declaration` | 2, after U1 | R-168 | `impl/binder-declarations` | `er-def-u5` |
| U6 | `story:service-response-is-checked-against-its-schema` | 3, after U5 | R-169 | `impl/response-schema-check` | `er-def-u6` |

Build directories: each tree's own `target/`. Scratch: `build/scratch/` in each tree. At most three
unit trees at once (`/` near 30G free).

## Decisions taken

- Projection keys: option A, `key_of` in `entity-store` resolves the three collection forms; no
  change to `entity-core`'s public API (`ValidatedDefinition::new` is consumer surface).
- Declared collection count: the guard keys on the declared field, as the map arm at
  `runtime.rs:2976` does.
