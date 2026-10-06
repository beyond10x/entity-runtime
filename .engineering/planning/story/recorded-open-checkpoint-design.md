---
format: aep.planning-md/3
id: story:recorded-open-checkpoint-design
kind: story
status: active
title: The bounded recorded-store open is designed, and the provider proof it needs is named
owner: entity-runtime
refs:
- provider: github
  reference: beyond10x/entity-runtime#55
relations:
- serves: vision:O2
- informed_by: story:bounded-batch-and-facade-reads
scope:
- confidence: cited
  path: crates/entity-eventlog/tests/shared_clock_cost.rs
- confidence: cited
  path: docs/design/recorded-open-checkpoint-v0.1.md
revision: 6
transitions:
- {from: "draft", to: "proposed", at: "2026-10-06T09:42:08Z", actor: "human:timo", revision: 5, decided_on: {"recorded":{"review_outcome":1}}}
- {from: "proposed", to: "active", at: "2026-10-06T09:42:08Z", actor: "human:timo", revision: 6, decided_on: {"recorded":{"review_outcome":1}}}
---
# The bounded recorded-store open is designed, and the provider proof it needs is named

## Outcome

`docs/design/recorded-open-checkpoint-v0.1.md` exists, has passed an independent review, and
decides how `story:recorded-open-verifies-a-checkpoint-and-its-suffix` bounds the cost of opening a
recorded Eventlog store under `CapturePolicy::ProviderTracked` (GitHub
beyond10x/entity-runtime#55). A release-mode baseline of today's open cost is measured in this
repository.

## Why

The scoper for the implementation story found that its acceptance cannot be met with the pinned
Eventlog alone (that story's `## Scope`, proof level 2): eventlog-core's `CaptureCheckpoint` is
"never a persisted or caller-made cursor" (`capture.rs:47-52` at rev `6983cc25`), the SQLite
change proof lives inside one process (`eventlog-sqlite` `tracked_capture.rs:20-41`), and five
reviewed scenarios (`ess/provider-tracking/scenarios/unchanged-head-*-tamper.yaml`) require a
`ProviderTracked` reopen to refuse a second connection's edit to old data. Eventlog `origin/main`
(`0.6.0-4-g1d089714`, read 2026-10-06) adds no durable proof. Which way to go is a design decision,
and it decides whether Eventlog work comes first.

## Acceptance

- The design states: where the checkpoint is persisted; what it binds (provider identity and
  binding, position, a canonical digest of the verified model — `CapturedModel` has no canonical
  encoding today, `crates/entity-eventlog/src/adapter.rs:3657-3684`); when it is written; what
  invalidates it; how a bounded open obtains the tenant totals `admit_growth` needs
  (`adapter.rs:1122`); whether an unkeyed digest is enough (it catches corruption, not a rewritten
  checkpoint); and what a bounded open does not detect that a complete one does.
- The design chooses between at least (a) a durable provider change proof in Eventlog (named as the
  exact Eventlog API and provider behaviour required, ready to file as an Eventlog issue) and (b) an
  Entity-Runtime-only checkpoint over the existing port (`read_feed`, `save_snapshot_checked` /
  `load_snapshot`, eventlog-core `lib.rs:879`, `:905-940`) with the five tamper scenarios' contract
  narrowed. The chosen option says what happens to each of the five scenarios.
- One independent review recorded as a `review-result` with its findings block, and each finding's
  outcome recorded.
- `crates/entity-eventlog/tests/shared_clock_cost.rs` gains a release-mode open probe at the 55 /
  601 / 1,203-event shapes it already seeds (`:287`, `:310-427`), median of 5 runs per size. Its
  output is committed at `docs/ess/evidence/provider-tracking/open-cost-baseline.txt` (with the
  commit, toolchain and machine it ran on) and recorded as `metric_observation` evidence on this
  story naming that file; the implementation story compares against it.
- If option (a) is chosen, a `dependency-blocker` on the Eventlog capability is filed with a
  `blocks` edge to the implementation story.

## Out of scope

Implementing the checkpoint (`story:recorded-open-verifies-a-checkpoint-and-its-suffix`); the
constant factor of `build_model` (`story:seeded-open-under-one-second`); Eventlog source changes.

## Scope

Derived 2026-10-06 from the implementation story's scoper report at `7926ec45`.

- **Files:** `docs/design/recorded-open-checkpoint-v0.1.md` (new) — cited
- **Files:** `crates/entity-eventlog/tests/shared_clock_cost.rs` (the probe) — cited
- **Read only:** `crates/entity-eventlog/src/adapter.rs`, `adapter/tracked.rs`, `sync.rs`, `facade.rs`; eventlog-core and eventlog-sqlite at rev `6983cc25`; `ess/provider-tracking/` — cited
- **Coordinator-owned:** the `review-result`, the evidence, any `dependency-blocker` — cited
- **Confidence:** high
- **Would collide with:** `story:seeded-open-under-one-second` and the implementation story only on `shared_clock_cost.rs` — inferred
