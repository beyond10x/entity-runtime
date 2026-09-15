---
format: aep.planning-md/1
id: story:async-recorded-contract-executor
kind: story
status: active
title: Execute complete recorded commands through async atomic storage ports
relations:
- serves: vision:O2
- depends_on: story:recorded-command-contract
- depends_on: story:atomic-batch-store
- depends_on: story:verifiable-decision-replay
scope:
- confidence: cited
  path: AGENTS.md
- confidence: inferred
  path: CHANGELOG.md
- confidence: cited
  path: Cargo.lock
- confidence: cited
  path: Cargo.toml
- confidence: inferred
  path: crates/entity-executor/
- confidence: inferred
  path: crates/entity-store/src/asynchronous.rs
- confidence: inferred
  path: crates/entity-store/src/asynchronous/
- confidence: cited
  path: crates/entity-store/src/lib.rs
- confidence: inferred
  path: crates/entity-store/tests/
- confidence: cited
  path: docs/design/recorded-execution-encoding-v0.1.md
- confidence: cited
  path: docs/design/recorded-execution-v0.1.md
- confidence: cited
  path: docs/requirements.md
- confidence: cited
  path: ess/recorded-execution/
revision: 9
---
## Outcome

An object-safe async recorded store and an executor outside entity-core preserve complete decisions,
ordered observations, exact retry receipts and atomic multi-entity batches, with verifiable genesis
or explicitly anchored history. An in-memory reference provider establishes this full contract.

## Authority and typed home

Approved plan ess-evolution-20260915 revision 1, SHA-256
7579145c3de5a1c6f8088fd7fb804d29dac8903ec505048f3ce595c45023b787; Atlas ADR 0050.
Binding design: docs/design/recorded-execution-v0.1.md. Typed coordinates and relations:
ess/recorded-execution/system.yaml and domains/recording.yaml, validated and compiled before this
story was created. Exact complete payloads reuse DecisionRecord, RecordedCommit, RecordedObservation
and Envelope in the existing Rust packages; the coordinate model does not replace their wire schema.

## Acceptance

- Add object-safe Send boxed-future read/history/recorded-write ports with no bare-decision fallback,
  hidden runtime or entity-core async/IO/hash/thread dependency. Preserve synchronous APIs/bytes.
- The executor validates metadata and recovers exact global record/batch retries before current
  state or registry lookup; changed normalized request, expectation, subject, kind or provenance
  conflicts. Verify immutable history prefixes through every hit before returning a committed
  receipt, using original saved definitions, and never re-decide stale intent.
- Fresh batches apply all expectations to ordered transaction-local state and commit state/history/
  global IDs/batch claims/receipts atomically. Zero-domain-event decisions advance revision once;
  observations retain exact order and do not advance it. Preserve duplicate emitted domain facts.
- Empty batches perform no IO. Duplicate IDs refuse. New named batches containing prior identical
  records refuse without writes; original batch retries recover. Single retries of named members
  retain original batch coordinates through an explicit Single receipt, never a fabricated claim.
- Canonical versioned request/record/batch comparison bytes preserve complete values, explicit nulls,
  exact scalars and array order, using the exact companion encoding design and literal fixtures.
  Derived identities reproduce their component fields. Revisions remain 1..=i64::MAX; physical
  ordinals/member indices use checked u64 domains and whole-batch allocation preflight.
- Verify full genesis or an explicit imported-anchor suffix, reporting its assurance boundary;
  reject tampered commands/results/events, skipped revisions, wrong order/subject/materialization
  and creation or a second import after an anchor. Kernel legacy replay refusals remain intact.
- Preserve available imported envelopes in a distinct typed evidence form with reserved global IDs
  and explicit unavailable receipt/order coordinates. Exact historical retry has a distinct outcome,
  never a fabricated committed receipt. Separate subject replay from explicitly scoped cross-subject
  verification; a partial transcript never proves completeness of a whole provider.
- Model commit-then-lost-response and same-identity recovery without duplicate effects/receipts.
  Unreachable and uncertain outcomes must not be reported as absence or proved rollback.
- Retain a decisive red before implementation, causal fault mutations, independent review and
  actual affected suites/fmt/strict Clippy. Add live requirement pins. Run real task check with
  PostgreSQL selected, preserve the AEP plan-check separation, and verify relevant Rust 1.85 closure.

## Scope

- crates/entity-store/src/lib.rs — cited; existing recorded types, error and store contracts.
- crates/entity-store/src/asynchronous/ and asynchronous.rs — inferred; select one Rust module layout
  after inspection, with reference provider and full recorded conformance outside the kernel.
- crates/entity-executor/ — inferred; new pure executor/verifier package; no runtime dependency.
- Cargo.toml and Cargo.lock — cited; root owns workspace integration; worker returns proposed patches.
- docs/design/recorded-execution-v0.1.md — cited; root-owned normative decisions, additive requirement
  pins may be proposed with exact test names after implementation.
- docs/design/recorded-execution-encoding-v0.1.md — cited; exact coordinate and comparison fixtures.
- ess/recorded-execution/ — cited; validated coordinate model, root-owned; never a payload substitute.
- docs/requirements.md — cited; every new requirement must pin a live test/manifest/type.
- crates/entity-store/tests/async_recorded*.rs — inferred; additive gate-selected reference/port
  regressions may live here or beside the asynchronous module; preserve existing sync assertions.
- CHANGELOG.md — inferred; record actual additive capability without claiming later adapter adoption.
- AGENTS.md — cited; root added the normative design reference; preserve its existing invariants.
- Confidence: high for semantic requirements; inferred package/module placement needs source check.
- Would collide with: recorded-store/kernel/shell API work and workspace manifest or requirement edits.

## Later units and limits

The Eventlog adapter, explicit synchronous bridge, SQLite facade/import and PostgreSQL facade/import
remain separate implementation units consuming this full contract. Do not implement substitutes or
weaken it for this reference provider. AEP owns exactly one planning-store migration story; this
unit does not move real stores, change old formats, select a runtime or publish/release anything.
