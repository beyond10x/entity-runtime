---
format: aep.planning-md/3
id: story:eventlog-append-contention-is-a-typed-error
kind: story
status: implemented
title: An append out of its physical-conflict budget refuses with a typed contention error
summary: 'E19 follow-up: the 8-attempt budget ends in Unreachable with a detail string; a typed variant is a breaking entity-store change'
owner: entity-runtime
relations:
- serves: vision:O2
revision: 4
transitions:
- {from: "draft", to: "proposed", at: "2026-11-23T23:19:46Z", actor: "human:timo", revision: 2, decided_on: {"recorded":{"test_result":1}}}
- {from: "proposed", to: "active", at: "2026-11-23T23:19:47Z", actor: "human:timo", revision: 3, decided_on: {"recorded":{"test_result":1}}}
- {from: "active", to: "implemented", at: "2026-11-23T23:19:47Z", actor: "human:timo", revision: 4, decided_on: {"recorded":{"test_result":1}}}
---
## Outcome

An Eventlog append that runs out of its physical-conflict retry budget refuses with a typed,
retryable contention error that a caller can match, instead of
`NotCommitted(Unreachable { provider: "eventlog", .. })` with a detail string.

## Finding

Review E19 (Entity Runtime 0.32.0, round 3): physical write conflicts can keep an Entity append
retrying indefinitely. The review asked for an iterative retry budget and "a typed, retryable
contention result when it expires".

## Evidence

- `2fb64ee8` (fix/review-er-d, merged in #93) bounded the retry: a loop of at most 8 attempts
  (`PHYSICAL_CONFLICT_ATTEMPTS`), each reconciling afresh. On exhaustion it refuses with
  `NotCommitted(Unreachable { provider: "eventlog", .. })` whose detail starts
  "physical stream contention" (`crates/entity-eventlog/src/adapter.rs:2119`).
- Kept as `Unreachable` on purpose: "No new error kind: AsyncStoreError lives in entity-store and
  is not non_exhaustive" (commit body of `2fb64ee8`); a new variant is a breaking change of
  `entity-store` (master log `assignment.md` line 86: "E19 keep Unreachable (new variant =
  breaking entity-store migration, story)").
- `AsyncStoreError`: `crates/entity-store/src/asynchronous/types.rs:739`, no `#[non_exhaustive]`.
- Test today matches on the detail string:
  `an_append_that_meets_physical_conflicts_on_every_attempt_refuses_after_its_budget`
  (`crates/entity-eventlog/tests/guarded_write_blob_binding.rs:1116`, assertion at line 1137).

## Acceptance

`an_append_that_meets_physical_conflicts_on_every_attempt_refuses_after_its_budget` matches a
dedicated contention variant of the store error (no detail-string match), the CHANGELOG names the
breaking `entity-store` change, and an append that commits on its last allowed attempt still
returns `Ok`.
