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
revision: 1
---
# Wave: a verified model holds each committed record once (#59)

## Stage and authority

Skill version 0.20.1, `aep:implementing`, wave mode. Wave of one: the only ready story this
dispatch covers. GitHub #55 (`story:recorded-open-verifies-a-checkpoint-and-its-suffix`) stays out:
`dependency-blocker:eventlog-durable-capture-continuity` blocks it on beyond10x/eventlog#39.

Run: non-interactive. The wave was approved under the operator's standing grant for this
repository's issue work, which also authorises the commits below; no operator turn is available
in this session.

Commits this wave makes: one unit commit, the merge of the unit branch into `wave/er-59`, the
closing planning-store commit, and the merge of `wave/er-59` into `main` through a pull request.

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
| U1 | `story:verified-model-holds-each-record-once` | dispatched | `impl/verified-model-holds-each-record-once` | `er-59-u1` | in-tree `target/` | `~/.cache/er-59/u1/scratch` |

Brief: `~/.cache/er-59/u1/brief.md`. Dispatch types: `aep:implementor`, then `aep:adversary`.

## Decisions taken

- Heap measurement uses `/proc/self/status` resident and peak bytes, one process per shape:
  `unsafe_code = "forbid"` rules out a counting allocator in the workspace, and the crates that
  wrap one safely would be new dependencies.
- Shared decoded definitions need `DecisionRecord.definition` to change type, a public
  `entity-core` change five consumer repositories would have to absorb. The unit measures what one
  copy per record achieves first; definition sharing becomes its own story if the threshold holds
  without it.
