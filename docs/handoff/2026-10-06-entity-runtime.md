# Hand-over: entity-runtime session, 2026-10-06

## State at hand-over

| item | state |
|---|---|
| `main` | `ace9c3b1` (Merge PR #61), primary checkout clean apart from this file |
| release | 0.27.0 published 2026-10-06 by `b10x-bot[bot]`, 6 assets, tag on `4bf7351f` |
| open PRs | none |
| unpushed commits | none |
| managed worktrees | none (every tree this session made is finished; archives under `~/.local/state/worktree/archives/entity-runtime/`) |
| remote branches left, all merged | `chore/release-0.27.0`, `docs/overhaul-2026-10-06`, `plan/after-0.27.0`, `wave/er-54-55-w1`, `wave/er-54-55-w2` (safe to delete) |
| open dispatches | none |
| this file | uncommitted: no approval in this session covers a commit for it |

## What landed

| PR | content |
|---|---|
| #56 | wave 1: `<path>.count` text length in `service/1` rules (#54); design `docs/design/recorded-open-checkpoint-v0.1.md` and open-cost baseline (#55) |
| #57 | wave 2: `alphabet:` on `service/N` string fields (#54) |
| #58 | release 0.27.0 |
| #60 | store: release task implemented, #59 filed |
| #61 | documentation overhaul; independent site live at https://beyond10x.github.io/entity-runtime/ |

The wave page with every decision, review and cost: `design:wave-issues-54-55-unblock-dependents`.

## Next steps

1. **#54, remaining three features** (ESS lowering waits on them; issue comment lists what shipped):
   `story:operation-writes-an-optional-field-from-an-optional-argument`,
   `story:set-increments-a-numeric-field`, then `story:set-clears-an-optional-field` (depends on
   both). Scoped; `aep plan artifact waves --kind story --status draft` serialises them on
   `entity-core` files. A unit that adds a field to an `entity-core` definition type must also run
   `cargo +1.91.0 test -p entity-eventlog --all-features --lib` (Debug-based model pins; wave 2's
   first gate failed on this).
2. **#55** is blocked: `dependency-blocker:eventlog-durable-capture-continuity` on
   beyond10x/eventlog#39. The Entity Runtime 0.26.0/0.27.0 Eventlog pin `6983cc25` is on the
   unmerged Eventlog branch `fix/er-51-capture-checkpoints`. Connectors (#101) is taking the
   owner-reuse route meanwhile and wants to hear when a release carries the bounded open.
3. **#59** (model memory, blocks connectors#103): `story:verified-model-holds-each-record-once`,
   draft, not scoped.
4. **Filed defects, draft:** `story:binder-elements-carry-their-declaration`,
   `story:projection-keys-read-collection-addresses`,
   `story:declared-collection-count-answers-its-declared-kind`,
   `story:service-response-is-checked-against-its-schema`,
   `story:service-1-moves-outcome-loads-from-yaml`, `story:generated-yaml-contracts-write-numbers`.
5. **Docs, Stage C** (the docs skill): the repository still appears in the unified site
   (`/docs/entity-runtime/` answers 200) and the GitHub website field names it. Leaving it touches
   Website, the root caller and Atlas.
