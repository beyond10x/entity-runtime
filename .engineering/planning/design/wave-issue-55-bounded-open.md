---
format: aep.planning-md/3
id: design:wave-issue-55-bounded-open
kind: design
status: draft
title: 'Wave: a recorded-store open verifies a checkpoint and its suffix (#55), Eventlog 0.8.0'
tags:
- non-interactive
refs:
- provider: github
  reference: beyond10x/entity-runtime#55
relations:
- designs: story:recorded-open-verifies-a-checkpoint-and-its-suffix
revision: 2
---
# Wave: a recorded-store open verifies a checkpoint and its suffix (#55), Eventlog 0.8.0

## Stage and authority

Skill version 0.20.1, `aep:implementing`, wave mode. Wave of one story:
`story:recorded-open-verifies-a-checkpoint-and-its-suffix` (GitHub #55), with the Eventlog re-pin
it needs as the coordinator's opening commit.

Run: non-interactive. The wave was approved under the operator's standing grant for this
repository's issue work, which also authorises the commits below; no operator turn is available
in this session.

Commits this wave makes: the opening coordinator commit (Eventlog pin, store moves, this page), the
unit's commits on its own branch, the merge of the unit branch into `wave/er-55-w1`, the store
commits, and the merge of `wave/er-55-w1` into `main` through one pull request. One integration
branch and one pull request per wave; units get no pull request of their own.

## Selection

`aep plan artifact waves --kind story --status draft` (aep 0.68.0), before the story moved:

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

The fifteen collisions are all among the three `entity-core` stories of GitHub #54; none names
this wave's story. The 19 unassessed stories are not candidates for this wave.
`story:operation-writes-an-optional-field-from-an-optional-argument` is disjoint from this story
and left out: a downstream release waits on this wave alone, so it gets the next wave.

## Blocker cleared

`dependency-blocker:eventlog-durable-capture-continuity` moved to `cleared`: Eventlog `0.8.0`
(tag on `de30462b`) carries `durable_checkpoint`, `restore_checkpoint`, `checkpoint_usage` and
`SqliteEventStore::enable_durable_continuity` / `disable_durable_continuity`; beyond10x/eventlog#39
is closed. The blocker's body cites the lines.

## Pre-flight

| check | value |
|---|---|
| base | `main` at `cfcba172` |
| free disk on `/` | 63G after the 0.8.0 compile check (`df -h /`) |
| previous wave trees | none (`git worktree list` showed the primary only) |
| build directories | in-tree `target/` per worktree |
| Eventlog 0.8.0 compile | `cargo +1.91.0 check -p entity-eventlog --all-features --all-targets --locked`: Finished |

## Units

| unit | story | stage | branch | worktree id | build dir | scratch |
|---|---|---|---|---|---|---|
| integration | — | opened | `wave/er-55-w1` | `er-55-w1` | in-tree `target/` | `build/er-55/` in the tree |
| U1 | `story:recorded-open-verifies-a-checkpoint-and-its-suffix` | implemented; merged as `39965110` | `impl/recorded-open-verifies-a-checkpoint` | `er-55-u1` | in-tree `target/` | `build/scratch/` in the tree |

Brief: `build/er-55/u1-brief.md` in the integration tree. Dispatch types: `aep:implementor`, then
`aep:adversary`.

## Decisions taken

- The Eventlog re-pin (0.7.0 → 0.8.0 in five manifests and both lockfiles) is the coordinator's
  opening commit, so the unit starts from a tree that already builds against the API it uses.

## U1 round 1 (implementor)

Commits on `impl/recorded-open-verifies-a-checkpoint`: `f4eea1e9` (ESS specification first),
`e418ab4f`, `dac08bf3`, `af413382`. Red first: `provider-ess-check` 21/31 against the unchanged
harness (all ten new scenarios failed at `EnableDurableCheckpoints`); a bounded handle's own write
first fell back to complete verification (`(1, 1, 0)` instead of `(0, 0, 1)`), fixed by keeping the
verified usage on the bounded state. Final, per command, all exit 0: `entity-eventlog
--all-features` 194 → 221 passed (7 ignored), `provider-ess-check` 31/31, `docs-check`,
`req-check` (129 requirements, 0 findings).

The base was red on Eventlog 0.8.0 before any change:
`review_open_checkpoint::a_checkpoint_snapshot_is_not_captured_and_writing_it_ends_in_process_continuity`
asserted two captures; Eventlog 0.8.0 keeps in-process continuity across the provider's own
snapshot writes (its changelog), so the test is renamed `…_keeps_in_process_continuity` and asserts
one.

Decisions the implementor took: wire form is the snapshot state object with a pinned reference
vector; calls `enable_durable_open_checkpoints` / `disable_durable_open_checkpoints`,
`write_open_checkpoint`, `discard_open_checkpoint`, `open_verification()`; complete-opened handles
keep their whole model. Departures from the design: a discard marker carries the digest of the
record it replaced instead of a random nonce (the library reads no random source); a bounded
handle's write preflight reads written subjects' histories per entity; a tracked handle refuses a
complete capture whose head is behind a position it verified. Declared limit: a suffix naming an
already-bound digest as newly bound is trusted to the provider.

Cost, median `start` in ms (median of three runs of five opens), baseline from
`open-cost-baseline.txt`, treatment `open-cost-checkpoint.txt`:

| events | baseline | checkpoint at previous head | 2-event suffix |
|---|---|---|---|
| 55 | 138.6 | 1.333 | 5.949 |
| 601 | 1,079.0 | 1.305 | 5.737 |
| 1,203 | 2,167.0 | 1.286 | 6.664 |

Pre-existing, seen while gating: `entity-eventlog` with `sync-bridge` alone does not compile, and
`sync-bridge,tree` has an unused import (`story:provider-feature-combinations-compile-and-are-gated`).

Agent cost: 711,272 tokens, 301 tool uses, 60.7 min.

## Adversary passes and corrections

`review-result:er-55-u1-adversary-pass-1` (3 findings, all introduced): a checkpoint-opened handle
read index rows in a separate transaction after the provider's Unchanged answer, so a foreign SQL
write in between was served as verified state (blocker); the design and the storage concept page
claimed a raw row edit is refused at read time (only blob edits are); a discard landing inside a
live handle's drain is overwritten (Eventlog's snapshot write has no compare-and-set). Fixed in
`d68ce28a`: every row-backed read asks the provider again after reading and serves only if nothing
changed, re-reading up to three times before one complete verification answers; the two documents
corrected; the drain limit documented. The F2 and F3 cases assert today's documented behaviour.
The record's report has one machine path replaced by a repository-relative one.

`review-result:er-55-u1-adversary-pass-2` (1 note): a co-tenant's write (two tenants in one SQLite
file and prefix) arrived as a zero-event delta that moved the read confirmation, costing a complete
verification. Coordinator fix `332423d8`: a verified delta carrying nothing of this tenant leaves the
confirmation serial in place; forcing it to move turns the co-tenant case red. Package gate after
the fix: `entity-eventlog --all-features` 231 passed, 7 ignored; clippy and fmt exit 0.

Agent cost: adversary pass 1 312,971 tokens, 53 tool uses, 17 min (plus a run stopped by a rate
limit); correction 799,513 tokens, 72 tool uses, 15 min; pass 2 389,432 tokens, 35 tool uses, 9 min.

## Gate

Full `task check` on `39965110` in this tree's own `target/`, each step alone, private `TMPDIR`:
fmt-check 0, clippy 0, test 0 (682 passed), doc-check 0, docs-check 0, example-check 0, req-check 0
(129 requirements, 0 findings), pin-check 0, postgres-check 0 (**skipped**: `ENTITY_POSTGRES_URL`
unset; CI runs it), eventlog-runtime-check 0 (231 passed, 7 ignored; `adversary_bounded_open` and
`bounded_open` ran from this tree's `target/debug`), notes-check 0, ess-check 0 (435/435),
provider-ess-check 0 (31/31).

## Release

`task:release-0-29-0`: workspace 0.28.0 → 0.29.0 (both lockfiles move only the workspace packages),
changelog section dated 2026-10-07, README status and install lines, status page date.
