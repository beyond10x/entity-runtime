---
format: aep.planning-md/3
id: story:tree-copy-hands-redacted-events-as-redacted
kind: story
status: draft
title: Eventlog Tree copy() hands an inline projector a redacted event as redacted
summary: 'L43 remainder: the cold-replay fix (Eventlog 158a3e1f) leaves copy() into a target with inline projectors handing a tombstone as live'
owner: entity-runtime
revision: 2
---
## Outcome

`eventlog_tree::copy` into a target with inline projectors hands each projector a redacted event
as redacted, as a cold Tree replay does since the L43 fix, so Entity Runtime's inline projector
(`ErRecordedProjector`) never receives a tombstone body marked live from any Tree path.

The code that changes is in the Eventlog repository (`crates/eventlog-tree/src/copy.rs`); this
story is held here because Entity Runtime runs the Tree provider with an inline projector and
takes the fix through its Eventlog pin. It may move to Eventlog's own planning store.

## Finding

Remainder of review L43 (Eventlog 0.9.4, round 6): "Cold Tree replay hands an inline projector a
redacted event marked live."

## Evidence

- Fix for the replay path: Eventlog `158a3e1f` "fix(tree): never hand an inline projector a
  redacted event as live" on `origin/fix/review-findings` (Eventlog PR #62, not released; Entity
  Runtime pins Eventlog `0.9.4`, `crates/entity-eventlog/Cargo.toml:27`).
- Gap left open: master log `assignment.md` (el-g COMPLETE): "Gap: L43 copy() into a target with
  inline projectors still hands tombstone as live event (note)".
- `pub async fn copy` at `crates/eventlog-tree/src/copy.rs:48` (Eventlog `origin/fix/review-findings`).
- Review sources: `crates/eventlog-tree/src/history.rs:360`; `crates/eventlog-tree/src/lib.rs:654,685,693,758`.
- Entity Runtime does not call `copy()` today (no `.copy(` under `crates/entity-eventlog`); its
  Tree users are `crates/entity-eventlog/src/sync.rs` and the tests `tree_branches.rs`,
  `per_entity_reads.rs`, `providers.rs`, `adversary_observation_heads.rs`, `tree_text_blobs.rs`.

## Acceptance

An Eventlog Tree test copies a history holding a redacted event into a target with an inline
projector that refuses a live event whose body is a tombstone, and the copy completes with the
projector having received that event with its redaction mark.

## Check on main f08eda98

Entity Runtime now pins Eventlog `0.10.0` (`c0f961dc`). At that tag the cold-replay fix has a test,
`crates/eventlog-tree/tests/branches.rs:1240`
`a_cold_replay_hands_an_inline_projector_a_redacted_event_as_redacted`. `copy` has none:
`crates/eventlog-tree/tests/copy.rs` and `copy_adversary.rs` never name a projector, and
`crates/eventlog-tree/src/copy.rs` appends each event through `replay` (`lib.rs:1043`) with
`EventRecord::new_event` and redacts it afterwards (`copy.rs:88`), so the acceptance is not met
and the story stays draft. Entity Runtime still has no `.copy(` call.
