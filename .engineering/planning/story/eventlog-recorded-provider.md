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
  path: crates/entity-query
- confidence: cited
  path: crates/entity-shell
- confidence: cited
  path: crates/entity-store
- confidence: cited
  path: docs/design
- confidence: cited
  path: docs/requirements.md
revision: 12
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

## PostgreSQL acceptance follow-up

The next implementation slice closes the missing real PostgreSQL evidence for the existing Eventlog adapter. Reuse the same recorded contract exercised by file and SQLite, then verify reconnect/replay, concurrent independent handles, and rollback of both identity claims and subject bodies on a late required projection refusal. Use a disposable loopback PostgreSQL fixture and bounded tests, never an existing application database. Add an explicit PostgreSQL test feature and task; an enabled lane without its assigned URL must fail, while an unselected local lane reports that it did not run. CI must select the lane against its existing PostgreSQL service. Source authority: crates/entity-eventlog/tests/persistence.rs, .github/workflows/gate.yml and Eventlog crates/eventlog-postgres/src/lib.rs at 55d90845ac22689c64b9bc96dcad2f9750075804. Query/transaction facade convergence remains required after this evidence slice.

## Enumeration compatibility finding

The first PostgreSQL acceptance run failed at recorded_contract's immediate ids("thing") assertion: the durable subject existed but enumeration returned []. The adapter incorrectly used Eventlog's feed as a current inventory; an unrelated transaction can withhold that feed under its documented PostgreSQL watermark. Fix enumeration through the new optional committed-stream inventory capability owned by Eventlog story:committed-stream-enumeration. Validate each selected ER history after resolving its first record. Preserve feed semantics and use point stream reads for physical-position assertions. Add an independent adapter test holding an unrelated assigned transaction to prove immediate enumeration while the feed remains empty. This is a compatibility fix, not a test sleep or an eventual-consistency exception.

## PostgreSQL verification result

The adapter now pins published Eventlog 06c8e1c806ece76bf2874107d73691ca71e4f18f and enumerates committed stream inventory. Four PostgreSQL cases passed in 0.25 seconds against an isolated PostgreSQL 17 fixture: shared recorded contract with reconnect/replay, late projection rollback of bodies and identity claims, independent-handle conflicts/exact retries, and immediate enumeration under a deliberately withheld feed. The original empty-list failure is fixed without altering Eventlog feed semantics. Reintroducing a feed dependency made the deterministic adapter regression fail with [] instead of ["one"]; restoring inventory passed the exact case.

Final task eventlog-check passed on Rust 1.91: format, strict all-target/all-feature Clippy, ten file/SQLite cases in 0.38 seconds and rustdoc. The requirements checker reported 97 requirements, 314 test functions and no findings. The explicit PostgreSQL task refuses an absent database URL; CI selects it unconditionally against its PostgreSQL service. Every case has a 20-second timeout. The disposable fixture was stopped and removed after verification. Eventlog's published prerequisite worktree was cleaned through worktree finish and reviewed exact-ID GC; its evidence logs are retained. No full gate or remote production proof was run.

Evidence: local-evidence:ess-evolution-20260910/er-eventlog-postgres-final.log, er-eventlog-inventory-mutation.log, er-eventlog-inventory-restored.log, er-eventlog-inventory-gate.log, er-postgres-missing-url.log and er-postgres-fixture.txt. The new provider dependency is published on feat/committed-stream-enumeration; both Eventlog and ER main integration remain outstanding. Query/transaction and legacy facade convergence, catalog dependency intent, the single AEP migration story, ESS semantics convergence and application acceptance remain required by the broader evolution goal. Next owner is this continuing session, starting with the existing provider-query-v0.1 contract and Eventlog projection transaction capability gap.

## Native query and transaction gap

Current source review distinguishes legacy preservation from Eventlog convergence. The accepted provider-query-v0.1 contract has recursive JSON containment with exact numeric meaning, query-bound keyset cursors and a PostgreSQL GIN index; PostgresSession adds transaction-local queries, point/absent-identity locks, sequence reservations and staged writes. Eventlog ProjectionStore currently exposes scalar indexed find/get/get_for_update plus guard reservations, not that complete query/session surface. Replacing queries with ids()+load() loops or invoking the outer store from a locked projector would not satisfy the migration contract. Native convergence must retain provider-owned indexed filtering, transaction-local reads and write atomicity, explicit hosted migration/application-role separation, and backward-compatible projection declarations. The legacy-recorded-batch-compatibility story closes complete-record support in the existing SQL transaction boundary; it does not close this Eventlog-native gap or authorize declaring facade migration complete.

## Native indexed query adoption

Consume published Eventlog 6f7ea5a113d76a3334cf04d2bccef7770e1aae4e. Add the asynchronous optional document-query port beside the existing query types and an inline ER document projector over the existing complete recorded payload. Namespace/entity key prefixes preserve original identity byte ordering; containment runs in Eventlog PostgreSQL, never an ids()+load() filtering loop. Query results remain derived hints: verify only the selected bounded candidates against recorded history before returning them, including current physical position and redaction invalidation. Do not treat persisted projection bytes as sealed kernel proof.

The host registers the projector on every writer before traffic. Enabling the query adapter validates complete namespace coverage against committed inventory once; a pre-existing unindexed namespace refuses rather than returning an empty result. Existing stores can rebuild the derived projection before inline registration under a fenced startup, then enable only when the inventory/position check proves coverage. A held feed watermark that leaves rebuild incomplete must refuse readiness. The adapter does not start a runtime or silently rebuild on reads.

Reuse DocumentQuery identity binding and bounded page rules, adding a shared page constructor for a native backend's explicit continuation signal. Tests compare PostgreSQL results with the memory reference, cover observations, exact retries, namespaces/entities/tenants, reopen, partial rebuild refusal, late rollback, redaction and forged projection bodies. Mutation checks must demonstrate filtering and provenance safeguards. The full native transaction session and SQL facade retirement remain required after this query capability.

## Native query implementation result

The adapter now consumes published Eventlog 6f7ea5a113d76a3334cf04d2bccef7770e1aae4e and implements AsyncDocumentQueryProvider using the native indexed projection capability. EntityDocumentProjector is explicitly registered by the host; enable_document_queries verifies committed namespace coverage at startup. Query pages verify only selected candidates against the existing recorded history/generation cache. Existing unindexed histories require a fenced derived rebuild, and a held feed watermark cannot make an incomplete rebuild count as ready. No history format or kernel authority changed. The query port and native page constructor share the existing predicate/cursor contract; the memory reference remains the comparison.

The three new PostgreSQL cases passed in 1.26 seconds. The restored adapter run passed all 17 cases in 1.84 seconds, including existing file/SQLite persistence, PostgreSQL concurrency/reconnect/rollback, native-vs-memory numeric containment, namespace/entity/tenant isolation, query-bound paging, zero-event transitions, observations, reopen, missing and watermark-incomplete rebuild readiness, forged candidate refusal and redaction invalidation. PostgreSQL fixtures share one test-only lock because the feed watermark is cluster-wide. Disabling coverage enumeration failed the missing-projection readiness assertion; disabling candidate/history comparison failed the forged-state refusal. Both deliberate mutations were restored before the final run. The four query-port unit tests, strict query/adapter Clippy, Rustdoc and the requirement pin checker passed. Query-port Rust 1.85.0 compatibility passed.

Evidence: local-evidence:ess-evolution-20260910/er-native-query-final.log; er-native-query-coverage-mutation.log; er-native-query-provenance-mutation.log; er-native-query-port.log; er-native-query-clippy.log; er-native-query-port-clippy.log; er-native-query-rustdoc.log; er-native-query-port-rustdoc.log; er-native-query-port-msrv.log; er-native-query-fixture.txt. These are focused checks, not a full task check, hosted production proof, main integration or completed consumer adoption.

The continuing session retains the published ER continuation for the next required step: native caller-scoped transactions combining locks, sequences, queries and dynamically staged recorded writes, then SQL facade convergence. This query capability does not retire those legacy implementations by itself. Keep the existing lifecycle status under this repository's operator-controlled move rule; implementation evidence is recorded here rather than claiming the full migration has landed.
