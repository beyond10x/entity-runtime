---
format: aep.planning-md/3
id: design:wave-issue-59-model-holds-each-record-once
kind: design
status: draft
title: 'Wave: a verified model holds each committed record once (#59)'
owner: entity-runtime
tags:
- non-interactive
refs:
- provider: github
  reference: beyond10x/entity-runtime#59
relations:
- designs: story:verified-model-holds-each-record-once
- designs: story:aep-lifecycle-fixture-matches-aep-main
- designs: story:website-dependencies-pass-npm-audit
- designs: story:unified-site-source-carries-no-project-pages-caller
- designs: story:entity-runtime-builds-against-a-released-eventlog
revision: 3
---
# Wave: a verified model holds each committed record once (#59)

## Stage and authority

Skill version 0.20.1, `aep:implementing`, wave mode. Wave of one: the only ready story this
dispatch covers. GitHub #55 (`story:recorded-open-verifies-a-checkpoint-and-its-suffix`) stays out:
`dependency-blocker:eventlog-durable-capture-continuity` blocks it on beyond10x/eventlog#39.

Run: non-interactive. The wave was approved under the operator's standing grant for this
repository's issue work, which also authorises the commits below; no operator turn is available
in this session.

Commits this wave makes: one commit per unit, the merge of each unit branch into `wave/er-59`, the
store commits, and the merge of `wave/er-59` into `main` through one pull request. Standing rule
from 2026-10-07: one integration branch and one pull request per wave; units get no pull request
of their own. The website audit branch already had pull request #63 open; it joined this wave by
merge, so #63 closes when this wave lands.

## Selection

`aep plan artifact waves --kind story --status active` (aep 0.68.0):

```
wave 1
  story:verified-model-holds-each-record-once (inferred)
1 wave(s), 0 collision(s), 0 unassessed
```

## Pre-flight

| check | value |
|---|---|
| base | `main` at `a5e40968` |
| free disk on `/` at dispatch | 48G (`df -h /`) |
| previous wave trees | none (`worktree repo list`) |
| build directories | in-tree `target/` per worktree |

## Units

| unit | story | stage | branch | worktree id | build dir | scratch |
|---|---|---|---|---|---|---|
| integration | — | opened | `wave/er-59` | `er-59-int` | in-tree `target/` | `~/.cache/er-59/int` |
| U1 | `story:verified-model-holds-each-record-once` | green round 2, committed `d9fcaf1f`; adversary dispatched | `impl/verified-model-holds-each-record-once` | `er-59-u1` | in-tree `target/` | `~/.cache/er-59/u1/scratch` |
| U2 | `story:website-dependencies-pass-npm-audit` | implemented; `9c254cf2` + `317f83f0` merged as `a98e0d2b` | `fix/website-npm-audit` | `er-audit` (finished, archived) | — | `~/.cache/er-59/audit/scratch` |
| U3 | `story:aep-lifecycle-fixture-matches-aep-main` | dispatched | `impl/aep-lifecycle-fixture-matches-aep-main` | `er-59-u3` | in-tree `target/` | `~/.cache/er-59/u3/scratch` |

Brief: `~/.cache/er-59/u1/brief.md`. Dispatch types: `aep:implementor`, then `aep:adversary`.

## Decisions taken

- Heap measurement uses `/proc/self/status` resident and peak bytes, one process per shape:
  `unsafe_code = "forbid"` rules out a counting allocator in the workspace, and the crates that
  wrap one safely would be new dependencies.
- Shared decoded definitions need `DecisionRecord.definition` to change type, a public
  `entity-core` change five consumer repositories would have to absorb. The unit measures what one
  copy per record achieves first; definition sharing becomes its own story if the threshold holds
  without it.

## Round 1 (implementor)

`Arc<StoredRecord>` shared by `records`, `histories`, batches, the handle's memory and the tracked
sub-model; a private history view in the `entity-store` verifier plus two additive
`#[doc(hidden)]` functions. Gate green per command. Probe medians (bytes per event, one process
per shape, resident bytes from `/proc/self/status`):

| events | base | round 1 | ratio |
|---|---|---|---|
| 55 | 513,712 | 316,881 | 0.617 |
| 601 | 472,825 | 272,905 | 0.577 |
| 1,203 | 441,384 | 241,272 | 0.547 |

Threshold (≤ 0.5 at 601 and 1,203) missed. Decision: round 2 stops the provider-tracked handle
keeping a second copy of every record and batch blob (estimated 31 MB at 601 and 62 MB at 1,203
by the implementor, not measured). Definition sharing moved to
`story:recorded-decisions-share-one-decoded-definition`, because it changes a public `entity-core`
field. Accepted deviations: three private-type renames in tests (`adapter.rs:6126`, `:6133`,
`tracked/review_tests.rs:193`), and a `#[doc(hidden)]` re-export in
`crates/entity-store/src/asynchronous.rs`.

Agent cost, round 1: 408,740 tokens, 162 tool uses, 35 min.

## Round 2 (implementor)

The ProviderTracked open builds by ownership: verification over the borrowed capture, then record
and batch blobs move into the model, and the handle keeps no second copy. Committed `d9fcaf1f`.
Implementor's medians (bytes per event): 55 events 511,255 → 266,314 (0.521), 601 events 472,525 →
221,252 (0.468), 1,203 events 441,336 → 189,662 (0.430). Coordinator re-run: base 601 events
472,675; treatment 601 events 221,238, 1,203 events 189,791. Verdict VERIFIED.

Agent cost, round 2: 537,955 tokens, 235 tool uses, 59 min.

## U2: website npm audit

`npm audit --audit-level=high` in `website/`: 47 vulnerabilities (3 low, 13 moderate, 14 high,
17 critical), exit 1 → 4 low, exit 0. `braces` vendored with the depth guards of
micromatch/braces#78; `tinypool`, `serialize-javascript`, `postcss-selector-parser` overridden.
Agent cost: 186,793 tokens, 97 tool uses, 16 min.

## Disk

`/` was at 18–19G free during round 2 (`df -h /`). Under the floor: package-scoped gates only;
the full gate waits until `/` is above 20G.
