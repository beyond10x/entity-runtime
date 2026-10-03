---
format: aep.planning-md/3
id: story:bounded-batch-and-facade-reads
kind: story
status: draft
title: Bound batch execution and facade reads to verified relevant records
refs:
- provider: github
  reference: beyond10x/entity-runtime#51
relations:
- serves: vision:O2
- informed_by: story:commands-read-their-entities-and-observations-do-not-fork
scope:
- confidence: cited
  path: CHANGELOG.md
- confidence: cited
  path: crates/entity-eventlog
- confidence: cited
  path: crates/entity-eventlog/src/adapter.rs
- confidence: cited
  path: crates/entity-eventlog/src/adapter/scoped.rs
- confidence: cited
  path: crates/entity-eventlog/src/adapter/small_store_cost.rs
- confidence: cited
  path: crates/entity-eventlog/src/facade.rs
- confidence: cited
  path: crates/entity-eventlog/src/sync.rs
- confidence: cited
  path: crates/entity-eventlog/tests/adversary_r2_per_entity_reads.rs
- confidence: cited
  path: crates/entity-eventlog/tests/per_entity_reads.rs
- confidence: cited
  path: crates/entity-eventlog/tests/provider_facades.rs
- confidence: cited
  path: docs/design/eventlog-recorded-adapter-v0.1.md
- confidence: cited
  path: docs/design/eventlog-recorded-sync-bridge-v0.1.md
- confidence: cited
  path: docs/requirements.md
revision: 3
---
## Context

GitHub issue #51 provides consumer timings and the shared-clock batch-closure reproducer: https://github.com/beyond10x/entity-runtime/issues/51. It requests bounded batch verification, subject-scoped facade history and batch reads including multi-subject histories, and a cheap verified unchanged answer without losing altered-blob detection. Optional startup snapshot reuse is also in scope if the provider can prove it safe.

## Acceptance

Named scenarios shared_clock_batch_cost_is_bounded, facade_reads_are_subject_scoped, multi_subject_histories_use_one_scope, and unchanged_capture_preserves_tamper_detection demonstrate bounded relevant-record reads with the reported scale probe within 2x, preserve atomic whole-batch verification and tamper refusals, and provide verified snapshot reuse only when supported by provider authority.

## Delivery

One issue-fix integration branch and one PR with issues #49 and #50. Measure baseline and treatment, preserve integrity, and record any provider capability limitation rather than weakening verification. No consumer repins or release are requested.

## Scope

Derived 2026-10-03 by aep:story-scoper.

- Primary surface: crates/entity-eventlog — cited.
- Files: src/adapter/scoped.rs:181, src/adapter.rs:1384, src/sync.rs:632, src/facade.rs:214 within that crate — cited.
- Tests: tests/per_entity_reads.rs, tests/adversary_r2_per_entity_reads.rs, tests/provider_facades.rs, src/adapter/small_store_cost.rs within that crate — cited.
- Documents: docs/design/eventlog-recorded-adapter-v0.1.md, docs/design/eventlog-recorded-sync-bridge-v0.1.md, docs/requirements.md, CHANGELOG.md — cited.
- Additional ESS provider component and conformance adapter — inferred; docs/ess/README.md explicitly excludes Eventlog provider behavior from the composed five-library specification.
- Confidence: high; the mechanism remains present and R-151 specifies whole-batch verification — cited.
- Collision: other Eventlog adapter work and shared requirements/changelog edits — cited.
- Safety: scoped reads retry inconsistent observations three times then fall back to complete capture (src/adapter/scoped.rs:148) — cited, source-walk level 2, unproven by execution.

## Feasible local work and unresolved dependency

Explicit scoped facade APIs and one-operation read_histories can use existing ReadScope and retain whole-batch verification. Existing handle reads promise complete tenant validation; changing them silently would narrow integrity coverage. New explicit scoped methods can preserve that existing contract.

This alone does not meet the issue: recursive batch closure remains, the clock's own history grows, and the pinned Eventlog API has no verified unchanged token. At Eventlog rev 0a0484634e8c640be29d6b6541d6cc1c1aaef7d3, crates/eventlog-core/src/capture.rs:273-336 provides complete/deferred captures; deferred capture may succeed where a content-read refusal occurs. src/lib.rs:1005-1017 projection_get supplies no authenticated checkpoint, and :1149-1179 read_many supplies no shared snapshot or verified token. Existing adapter/memory.rs:143-175,248-258 reuses verification only after comparing actual bytes.

A checkpoint or changed-content authority needs a separately accepted provider contract; a head fingerprint cannot detect altered blob bytes. Removing co-member recursion without a replacement proof would weaken verification or force complete fallback. Therefore the complete #51 acceptance is blocked on that contract. Provider ESS coverage is also needed before the proposed scenario names can be called executed ESS scenarios.
