---
format: aep.planning-md/1
id: story:eventlog-recorded-provider
kind: story
status: draft
title: Persist complete entity history through Eventlog atomic groups
relations:
- serves: vision:O2
- decomposes: epic:the-store-an-adopter-runs-on
- depends_on: story:async-recorded-execution
scope:
- confidence: cited
  path: .github/workflows/gate.yml
- confidence: cited
  path: CHANGELOG.md
- confidence: cited
  path: Cargo.toml
- confidence: cited
  path: Taskfile.yml
- confidence: cited
  path: crates/entity-core
- confidence: cited
  path: crates/entity-eventlog
- confidence: cited
  path: crates/entity-shell
- confidence: cited
  path: crates/entity-store
- confidence: cited
  path: docs/design
- confidence: cited
  path: docs/requirements.md
revision: 5
---
## Outcome
Implement the Eventlog persistence adapter required by the authorized ESS evolution migration, consuming current Eventlog source 55d90845ac22689c64b9bc96dcad2f9750075804.

## Acceptance
Complete entity decisions and observations survive reopening the file provider with exact global record-ID retry/conflict behavior, distinct entity revisions and physical positions, and ordered multi-entity writes that either publish all record bodies and identity claims or none.

## Existing typed authority
RecordedCommit, RecordedObservation and Envelope in entity-store and DecisionRecord in entity-core already define the stored domain values. The new adapter maps those existing types into an explicitly versioned Eventlog envelope. Storage indexes are derived coordinates, not new application entities. The owning migration is ESS docs/design/ess-evolution/migration.md section 1.

## Design and scope
Add crates/entity-eventlog as a separate workspace: Eventlog requires Rust 1.91, while existing ER crates retain Rust 1.85. Pin Eventlog at the freshly verified remote commit. Implement the asynchronous ER ports and optional atomic batch capability with Eventlog atomic groups. One record-ID reservation stream plus a subject-history append participate in each group; no independent append loop and no dual authority. Validate complete decisions by kernel replay, preserve observations without advancing entity revisions, and refuse redacted or invalid history. Store selectors and tenant context come from the caller.

Cited scope: Cargo.toml workspace exclusion, crates/entity-eventlog, docs/design, docs/requirements.md and CHANGELOG.md. Preserve existing provider layouts and synchronous APIs; migrating SQLite/PostgreSQL facades and consumers remains later required work.

## Verification
Use focused real file-provider tests covering close/reopen, zero-event decisions, observations, exact retries, changed IDs, global collisions, concurrent writers, malformed history and atomic rollback. Run adapter Clippy and its declared MSRV checks. Do not rerun the prohibited expensive ESS full gate or count absent consumer acceptance as success.

## Review correction

The operator asked whether this implementation was adding technical debt. Review identified repeated full-history verification between the provider and async shell, duplicate synchronous/async retry matching, and a standalone adapter omitted from the ordinary gate. These are unfinished work, not accepted shortcuts. Before considering the adapter ready, share verified history and command matching, reuse only unchanged history prefixes under provider generation/head checks, and wire the adapter into local and CI validation with its explicit Rust 1.91 requirement while retaining the original ER MSRV lane. Add evidence proving cache invalidation and sensitivity to broken atomicity. Do not claim throughput from small functional tests.

## Implementation and verification

The adapter now consumes exact Eventlog 55d90845ac22689c64b9bc96dcad2f9750075804 through its own locked Rust 1.91 workspace. The normal local gate includes task eventlog-check, and the existing required GitHub Gate job includes identical adapter checks. Original ER packages retain Rust 1.85. Kernel replay now has a sealed incremental cursor; VerifiedHistory couples it to envelopes, and both storage and the async shell share that proof. Both shells use one retry matcher. The provider caches only generation-checked prefixes, with bounded configurable retention and immutable shared collections; it reads and verifies new tails rather than replaying all prior records on each call.

Final task eventlog-check passed: strict format, Clippy, ten real-provider tests and rustdoc. The tests completed in 0.40 seconds. File and SQLite exercise one recorded contract, with file reopen and concurrent handles, observations/physical positions, exact/global retries, late atomic rollback, forged history refusal, redaction invalidation, shared proof identity and cache budgets. Deliberately replacing atomic groups with independent appends made the rollback assertion fail because a subject survived. Deliberately ignoring generation changes made the cache test return a proof after redaction. Both mutations were restored before the passing final gate. These small tests do not establish production throughput.

The core/store/shell test selection passed after extracting shared verification (including all existing kernel replay and purity tests). The added incremental-prefix refusal test also passed. Strict Clippy for all targets of those three crates passed, as did their Rust 1.85 check. The requirements checker passed with no findings. No full task check or ESS consumer gate was run.

Retained evidence: local-evidence:ess-evolution-20260910/er-eventlog-gate.log, er-eventlog-atomicity-mutation.log, er-eventlog-cache-mutation.log, er-verified-history-tests.log, er-verified-prefix-test.log, er-shared-history-clippy.log and er-shared-history-msrv.log. Work remains on the continuation branch, not main. PostgreSQL adapter acceptance, facade/query/transaction convergence, provider dependency catalog intent before integration, AEP migration and application adoption remain unfinished.
