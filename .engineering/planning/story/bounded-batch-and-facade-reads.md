---
format: aep.planning-md/3
id: story:bounded-batch-and-facade-reads
kind: story
status: implemented
title: Bound batch execution and facade reads to verified relevant records
refs:
- provider: github
  reference: beyond10x/entity-runtime#51
relations:
- serves: vision:O2
- informed_by: story:commands-read-their-entities-and-observations-do-not-fork
scope:
- confidence: cited
  path: AGENTS.md
- confidence: cited
  path: CHANGELOG.md
- confidence: cited
  path: Cargo.lock
- confidence: cited
  path: Taskfile.yml
- confidence: cited
  path: checks/ess-conformance
- confidence: cited
  path: crates/entity-cli/Cargo.toml
- confidence: cited
  path: crates/entity-eventlog
- confidence: cited
  path: crates/entity-eventlog/Cargo.toml
- confidence: cited
  path: crates/entity-eventlog/src/adapter.rs
- confidence: cited
  path: crates/entity-eventlog/src/adapter/scoped.rs
- confidence: cited
  path: crates/entity-eventlog/src/adapter/tracked.rs
- confidence: cited
  path: crates/entity-eventlog/src/adapter/tracked/review_tests.rs
- confidence: cited
  path: crates/entity-eventlog/src/facade.rs
- confidence: cited
  path: crates/entity-eventlog/src/lib.rs
- confidence: cited
  path: crates/entity-eventlog/src/sync.rs
- confidence: cited
  path: crates/entity-eventlog/tests/shared_clock_cost.rs
- confidence: cited
  path: crates/entity-postgres/Cargo.toml
- confidence: cited
  path: crates/entity-sqlite/Cargo.toml
- confidence: cited
  path: docs/design/eventlog-recorded-adapter-v0.1.md
- confidence: cited
  path: docs/design/eventlog-recorded-sync-bridge-v0.1.md
- confidence: cited
  path: docs/ess/README.md
- confidence: cited
  path: docs/ess/evidence/provider-tracking
- confidence: cited
  path: docs/requirements.md
- confidence: cited
  path: ess/provider-tracking
revision: 19
transitions:
- {from: "draft", to: "proposed", at: "2026-10-03T15:35:45Z", actor: "human:timo", revision: 10}
- {from: "proposed", to: "active", at: "2026-10-03T15:35:45Z", actor: "human:timo", revision: 11}
- {from: "active", to: "implemented", at: "2026-10-03T16:39:23Z", actor: "human:timo", revision: 19, decided_on: {"recorded":{"test_result":1,"metric_observation":2,"review_outcome":4,"verification":1,"ess_conformance_coverage_v1":2}}}
---
## Context

GitHub issue #51 provides consumer timings and the shared-clock batch-closure reproducer: https://github.com/beyond10x/entity-runtime/issues/51. It requests bounded batch verification, subject-scoped facade history and batch reads including multi-subject histories, and a cheap verified unchanged answer without losing altered-blob detection. Optional startup snapshot reuse is also in scope if the provider can prove it safe.

## Acceptance

Named scenarios shared_clock_batch_cost_is_bounded, facade_reads_are_subject_scoped, multi_subject_histories_use_one_scope, and unchanged_capture_preserves_tamper_detection demonstrate bounded relevant-record reads with the reported scale probe within 2x, preserve atomic whole-batch verification and tamper refusals, and provide verified snapshot reuse only when supported by provider authority.

## Delivery

One issue-fix integration branch and one PR with issues #49 and #50. Measure baseline and treatment, preserve integrity, and record any provider capability limitation rather than weakening verification. No consumer repins or release are requested.

## Initial scope assessment (superseded below)

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

## Provider investigation after approval

Current Eventlog main 06c1e99c86c7130169c7ed584ec7326f7e9a546e has the same crates tree as pinned 0a0484634e8c640be29d6b6541d6cc1c1aaef7d3; a dependency update alone supplies no runtime capability.

A feasible SQLite warm path is inferred, not yet implemented: provider-owned connection-local data_version plus own-write and schema generations, sampled under its mutex and transaction, can attest to unchanged SQL-visible state. A bounded journal of acknowledged guarded append groups could then supply proven deltas. Other writes, external SQL mutation, uncertainty, journal expiry and mismatched limits or provider identity must reset to complete verification. A mere event-head fingerprint is insufficient.

The extension belongs to Eventlog's ConsistentTenantCapture contract because ER cannot access the private SQLite connection or prove before/after append continuity through current methods. ER must additionally avoid whole-model clones and whole-history verification scans when advancing a verified model. Approximate scope is 4–6 provider files and 5–8 ER files, plus both specifications and tests, not a facade-only patch.

An operator question is pending: may the warm cache trust SQLite change tracking after full verification on open, including detection of independent SQL tampering, or must every read detect raw file-byte edits bypassing SQLite? Elapsed time is not approval of the narrower boundary. Existing full-verification behavior stays in force meanwhile.

Even the warm design cannot make complete fresh open or full growing-history output constant cost. The consumer invoke benchmark must distinguish those costs and retained-handle operation. A claim of complete #51 resolution requires both provider evidence and the actual consumer acceptance measurement.

Sources: Eventlog eventlog-sqlite/src/lib.rs:48-65 (private connection), :1975-2043 (insufficient snapshot generations), eventlog-sqlite/src/atomic_group.rs:135-193 (transaction boundary); ER adapter.rs:1023-1090,1280-1350,3910-3965 and entity-store/src/asynchronous/verify.rs:765-811 (remaining model/history work).

## Accepted integrity boundary and resumed scope

Operator answer on 2026-10-03: Accept SQLite change tracking. Full verification on open remains mandatory; warm cache reuse may rely on SQLite-mediated change detection, including writes and tampering from other connections. Direct database-file byte edits bypassing SQLite are outside that warm guarantee. This is an explicit contract choice, not evidence that an implementation already meets it.

Implementation spans a generic optional Eventlog capture checkpoint/acknowledged append-delta capability, incremental Entity Runtime verification without repeated whole-model/history copies, and facade multi-subject reads. Unsupported providers fall back to complete capture. New checkpoints are ephemeral, provider-instance and request-bound, never persisted or signed evidence. Changed SQLite state without a proven contiguous append delta requires a complete verified capture. Exact provider API and scenario names will be recorded after scope review, before implementation.

The approved delivery remains one Entity Runtime integration branch and one PR. Necessary upstream Eventlog support is isolated on fix/er-51-capture-checkpoints based on 06c1e99c86c7130169c7ed584ec7326f7e9a546e and will be bot-published as an exact dependency commit, without a second PR, main merge or release.

## Accepted implementation design

# Issue 51 runtime design

Accepted boundary: full verification on every open. Existing constructors retain FullVerification. Explicit ProviderTracked permits provider-owned proof that SQL-visible content has not changed or has gained a complete acknowledged append suffix. SQL mutation through any connection invalidates unaccounted state; raw file writes bypassing SQLite are excluded from the warm guarantee. Missing capability or invalidation means complete verification, not acceptance from a head fingerprint.

Public API: exported transient CapturePolicy::{FullVerification,ProviderTracked}; EventlogRecordedStore::open_with_policy; RecordedProviderFacade::start_with_read_policy; bridge equivalent constructor. The existing constructors delegate to FullVerification. A scoped facade view offers load_recorded, read_history, read_histories, lookup_record and lookup_batch; multi-subject histories use one worker request and one read scope, preserving input order and duplicate positions. Complete reads and full snapshot output remain available. No persisted format changes.

Provider contract: ConsistentTenantCapture::capture_tenant_since(tenant,projections,limits,previous:Option<&CaptureCheckpoint>) returns Complete{capture,checkpoint}, Unchanged{checkpoint}, or AppendDelta{checkpoint,delta}. Immutable checkpoint is opaque and provider/connection/tenant/generation/spec/limits bound. Delta holds exact ordered new events, newly bound blobs, complete before/after projection key mutations, and exact resulting usage. Provider supplies all of a new batch; gaps, external SQL changes, uncertain writes and expired journal force Complete. Normal ER batches already use guarded atomic append with blobs.

Runtime: retain one mutable verified cache behind a short synchronous lock, never across IO; the checkpoint has a local generation for racing refresh checks. Store a verified model, exact usage, retained admitted blobs/projection rows, and per-subject verified terminal and last position. Scoped answers copy only requested terminal/lookup/history outputs under the lock. Shared batch pre-read retains a compact operation view. Do not clone the entire model on ordinary append, compare complete captures, rewalk batch closure, rescan old histories, or rebuild complete expected projection sets.

Bounded suffix verifier: fast path only for linear er.recorded_entry events whose previous verified prefixes and origins are attested unchanged. Reuse event/blob admission and whole-new-batch construction. Check positions, unique record/batch identities, and exact new receipts; replay each suffix entry against the cached predecessor with validate_entry_against_state. Compare every changed projection row to exact per-key expected content and its verified previous value. Install checkpoint only after successful validation. Imports, lineage, nonordinary changes and uncertainty use complete fallback. On an unsuccessful tentative advance discard it and ask the complete verifier for the authoritative refusal.

Atomicity: a caller cannot observe a partly advanced model. A concurrent cache advance invalidates an in-flight local generation and retries. Provider checkpoint proof is fresh at its capture transaction; the existing append guard and post-commit verification preserve the existing race semantics. No new claim that all historical damage racing a commit prevents that commit.

Verification: red shared-clock fixture at 55/601/1203 events, representative embedded definitions; unchanged reads and fixed-size batches measured separately from open/output cloning. Real public facade timing must meet 2x before a performance claim. Provider counters must show no old events/blobs/rows for unchanged and only new suffix verification for acknowledged append. Differential full-vs-tracked results and corrupt suffix mutations must match. External SQL old-blob/event/row/binding changes with stable event head must invalidate and refuse exactly as full verification. Token expiry, wrong scope/limits/generation, imports, lineage and concurrent reader coverage remain mandatory.

State of this file: accepted implementation direction, not verification evidence. ESS declaration is a typed transient coordinate only; coordinator adds executable provider scenarios through the real adapter. No production changes or build are authorized until root admission.

## Provider conformance execution plan

The standalone ESS entry point ess/provider-tracking declares the transient CapturePolicy enum before source implementation. Coordinator expands it with real SQLite/facade command adapters in the existing checker under optional feature eventlog and --spec-root. The original five-library suite and Rust1.85 path remain separate; the provider suite uses Rust1.91. Compiled CapturePolicy enum must match the complete Rust variant set. Seven authored scenarios are validated in coordinator scratch: unchanged-head-blob-tamper, unchanged-head-event-tamper, unchanged-head-projection-tamper, unchanged-head-identity-tamper, unchanged-head-delete-blob-tamper, multi-subject-histories-one-scope, unchanged-capture-retains-verified-state. Generated and authored cases must all execute; validation alone is not conformance. Performance is separately measured by the real public facade release probe at55/601/1203 exact event counts, with raw medians and <=2x gate.

## Measured scope and review progress

The earlier reference to an actual consumer invocation is superseded by the accepted public-facade probe: consumer adoption and repins are excluded from this delivery. The release-mode fixture uses real SQLite, canonical Entity Runtime records, the public facade, 55/601/1203 exact events and a clock shared by every historical batch. Opening and full-history output are measured separately from warm bounded batches. Baseline medians 92.517484/7038.153041/12449.723852 ms fail the unchanged <=2x assertion (exit101); tracked treatment 10.511861/11.983571/17.701283 ms passes (exit0), growth1.140/1.684x. Unchanged reads cause no extra capture/decode/model work. Raw outputs, source diff and hashes remain in the unit's perf scratch until evidence admission.

Provider focused tests pass195, including19 new controls, with four compile-valid defects caught and restored. Final upstream/integration gates and independent reviews are still pending. Runtime adversary found a receipt-coordinate parity gap between delta and full verification; implementation is being corrected before acceptance. These intermediate results do not clear the provider blocker or finish the story.

## Scope

The initial inferred provider extension is now implemented, replacing the earlier facade-only feasibility estimate. Runtime unit1516e748 changes adapter.rs, adapter/scoped.rs, new adapter/tracked.rs and its independent review_tests.rs, facade.rs, sync.rs and lib.rs; shared_clock_cost.rs is the explicitly run release performance probe; the provider policy has its own ESS root. Actual new runtime tests are five implementor cases plus three adversary cases; library66→74 and all-feature package186 passed, with the separately executed performance probe ignored by ordinary package selection. Receipt parity was an introduced defect, reproduced through a native SQLite writer, fixed by bounded receipt-key validation, and independently rechecked without weakening assertions.

Coordinator surfaces are checks/ess-conformance (real SQLite/facade adapter and selected spec root), ess/provider-tracking (typed policy, ten adapter commands and seven authored scenarios), Taskfile.yml (required provider contract lane), AGENTS.md and docs/ess/README.md (gate contract), requirements R-151, two provider designs, dependency manifests/locks, changelog and AEP evidence. The previous five-library scenario contracts remain authority and must remain unchanged. Upstream Eventlog core/SQLite work is separately governed by story:sqlite-tracked-capture and its independent review; delivery consumes one exact published revision.

## Final integrated acceptance

Eventlog6983cc25eb92e07844b3a6fa3e0decbdb300f43c is published on fix/er-51-capture-checkpoints with signed common checks. All runtime/checker manifests and lockfiles use that exact Git revision, with no path override. Final task check exits0: formatting, workspace lint/tests, docs, examples, requirements, fixture pins, real PostgreSQL, all-feature Eventlog runtime tests/lint, notes, original ESS and provider ESS all complete. Runtime186 passed; its timing probe was deliberately ignored in the package selection and then separately executed. Original ESS421/421 and provider ESS17/17 pass without failed/error/unsupported/skipped cases; all original421 scenario contract bytes remain unchanged from the prior integration.

The final release probe exits0 with one actual test: medians11.532148/13.049370/13.568977ms at55/601/1203 events, growth1.132/1.177x under the unchanged2x assertion. It followed all review corrections and used the published provider revision. Its exact stdout and source/Cargo hashes are public under docs/ess/evidence/provider-tracking. The original development benchmark and final integrated check are distinct observations, not selected repeats of one measurement. Upstream full/proof gates execute545 tests with zero skips. The dependency blocker is cleared and all introduced review findings are fixed.

Five asynchronous test bodies were wrapped in conventional #[test] entrypoints with an identical current-thread runtime because the legacy requirement checker recognizes that form. Names and assertions were preserved; final gate includes them, and all125 requirements have live evidence. No Python checker change was made. Agent token/tool/wall accounting was not supplied by the harness and remains unknown. Delivery is one Entity Runtime PR; no upstream PR, main merge, release or consumer repin.
