---
format: aep.planning-md/1
id: story:async-recorded-execution
kind: story
status: draft
title: Execute recorded commands through asynchronous storage ports
relations:
- serves: vision:O2
- informed_by: story:recorded-command-contract
- decomposes: epic:the-store-an-adopter-runs-on
scope:
- confidence: cited
  path: CHANGELOG.md
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
Provide the asynchronous recorded-store boundary and executor required by the authorized ESS evolution migration, outside the IO-free kernel.

## Acceptance
An executor using a suspending recorded provider preserves complete replayable decisions, including zero-event commands, exact retries and conflicts, optimistic revision checks and non-state-changing observations, while an explicit synchronous adapter retains existing providers without claiming nonblocking IO.

## Authority and typed inputs
The operator's ESS evolution initiative and docs/design/ess-evolution/migration.md in ESS require this prerequisite before the Eventlog adapter and consumer migrations. Existing typed inputs are DecisionRecord and DecisionCommand in crates/entity-core/src/runtime.rs, Recording and Envelope in crates/entity-store/src/envelope.rs, and RecordedCommit and RecordedObservation in crates/entity-store/src/lib.rs. This extends execution over those existing types; no new domain entity or persisted format is introduced.

## Design
Add mandatory asynchronous recorded-store ports in entity-store and an async executor in entity-shell. Futures are Send and runtime-neutral. Recorded writes have no event-only default. A named synchronous compatibility adapter explicitly executes blocking IO on the polling thread; it neither starts an executor nor calls block_on. Complete records are the executor's replay authority; legacy or tampered histories are refused, not promoted to verified state. Providers own global record-ID equality and optimistic commit atomicity. Expose recorded atomic batches separately; never implement them as independent durable commits.

## Scope
Cited: crates/entity-store, crates/entity-shell, docs/design/store-v0.2.md, docs/requirements.md, CHANGELOG.md. Add an async boundary design alongside the existing store design. Keep existing kernel, provider layouts and synchronous public entrypoints compatible.

## Remaining evolution work
The Eventlog adapter, SQLite/PostgreSQL facades, AEP authority migration, ESS service/protocol/UI evolution and application adoption remain separate unfinished work. This story alone cannot establish the goal. The existing asynchronous consumer API must exercise actual suspension, not merely wrap a synchronous implementation in an async signature.

## Verification policy
Use bounded focused tests and strict Clippy for changed crates; verify tamper refusal by mutation. The operator forbids repeating the expensive full gate. Do not claim an unexecuted gate or application adoption. This is one implementation story, so no multi-story decomposition panel is applicable.

## Implementation evidence

Implemented the runtime-neutral ports, mandatory complete recorded writes, explicit blocking compatibility adapter, optional recorded batches and async executor. The executor verifies history before execution and retries; existing synchronous APIs and kernel source remain unchanged. Design: docs/design/async-recorded-storage.md; requirement R-121.

Local verification on 2026-09-10: cargo test --locked -p entity-shell -p entity-store passed (40 tests, no failures); the additional concurrent-publication test then passed with the final nine-case asynchronous suite. All four kernel purity tests passed. Strict Clippy over all targets of both changed crates passed. cargo +1.85.0 check --locked -p entity-store -p entity-shell passed. The requirements register checker passed. The replay guard was deliberately replaced by trusting the stored result: the tamper test failed because forged state was returned; restoring replay restored success.

Evidence: local-evidence:ess-evolution-20260910/er-async-affected-tests.log, er-async-final-tests.log, er-async-clippy.log, er-async-purity.log, er-async-msrv.log and er-async-tamper-mutation.log. Builds used two jobs with debug information, incremental compilation and compiler wrappers disabled. Retained source is based on faadc04f2f273517e21815d32ba3866f3aea7642.

No full task check or application adoption gate has run for this change. Keep the work in a continuation branch pending integration verification. Eventlog-backed execution and the consumer migrations remain required; this does not establish completed ESS evolution. The story retains its initial draft lifecycle because repository instructions reserve advancement for an explicit lifecycle request; this evidence records the actual implemented and verified subset without claiming landed completion.

## Refreshed upstream baseline

On the operator's request, fetched origin in AEP, ESS and Eventlog and compared each primary HEAD to its fetched origin/main. All three were already equal with zero commits on either side: AEP 28abe09bb6e5b0a6b4db839f6bf5693957d39324 (0.55.0), ESS 0de935d92c49df1147f8196e2424f49634ec2925 (0.22.0), and Eventlog 55d90845ac22689c64b9bc96dcad2f9750075804 (0.2.0). No merge or history rewrite was needed. AEP's existing local planning and documentation edits were preserved; ESS and Eventlog were clean.

ESS now includes at-most-once bindings, independent Gates adoption, and native test-profile scoping. Eventlog's published changes since the previously verified file-provider prerequisite are release and gate/documentation changes; its provider source directories are unchanged. The next persistence adapter must resolve its dependency against the refreshed Eventlog commit. The planning writer used for this record reports protocol 0.54.0; refreshing source does not claim that an installed executable was upgraded. The ER continuation remains source 550c54c879e944543ca9478f9d3744b7039ba69c on feat/async-recorded-execution, not integrated main.

## Shared verification follow-up

The Eventlog adapter review removed duplicate command matching and full-history verification from the async shell. entity-core::VerifiedReplay extends a sealed prefix using the same code as full replay; entity-store::VerifiedHistory binds envelopes and subject identity. Async providers can share an immutable proof through verified_history, while the default still verifies complete records. The Eventlog adapter checks storage generation and head before reusing a cached proof and verifies appended tails. Both command surfaces use the shared execute_retry matcher. Incremental verification refuses a bad tail without changing the prior proof. See story:eventlog-recorded-provider for current verification and remaining integration work.
