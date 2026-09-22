---
format: aep.planning-md/1
id: story:pure-service-semantics
kind: story
status: implemented
title: Complete pure ER service semantics for ESS lowering
refs:
- provider: ess
  reference: initiative:ess-evolution
relations:
- informed_by: initiative:entity-runtime
- serves: vision:O2
scope:
- confidence: cited
  path: AGENTS.md
- confidence: cited
  path: CHANGELOG.md
- confidence: cited
  path: crates/entity-cli
- confidence: cited
  path: crates/entity-core
- confidence: cited
  path: crates/entity-executor
- confidence: cited
  path: crates/entity-graph
- confidence: cited
  path: crates/entity-store/src/asynchronous
- confidence: cited
  path: crates/entity-surface
- confidence: cited
  path: docs/design/kernel-v0.1.md
- confidence: cited
  path: docs/design/recorded-execution-encoding-v0.1.md
- confidence: cited
  path: docs/design/service-semantics-v0.1.md
- confidence: cited
  path: docs/requirements.md
- confidence: cited
  path: ess/service-semantics
revision: 6
---
## Approved requirement and prerequisite

ESS evolution M5/M6 requires complete ER service semantics before ESS lowering and SDK delegation.
The admitted contract is docs/design/service-semantics-v0.1.md and its existing typed model
ess/service-semantics/, delivered in ess-evolution-service-semantics-20260916. Design admission and
source-profile witnesses are recorded under task:service-semantic-contract. These documents are
not executed runtime semantics; the missing implementation is this task's entire deliverable.

## Scope and completion contract

Implement the complete ten-dimension pure ER service contract: creation, update, transitions,
predicates, invariants, identity, relations, values, outcomes and ordered event multiplicity;
responses and complete decision recording/replay are included. Preserve kernel/1 bytes and refusal
order, introduce only the specified closed format/version changes with old-reader rejection, retain
the IO-free kernel's dependency boundary and supported Rust minimum. All affected ER workspace
consumers must compile and preserve old behavior. No ESS/AEP dependency, SQL work or Eventlog adapter.

Implementor owns source, tests and requirement/design/CHANGELOG consistency in the existing managed
service-semantics tree. Exact brief in root wave0009/service-semantics-implementation-brief.md names
allowed surfaces, literal checks, build token and stopping condition. Root owns planning writes,
submission, independent source review, full gate with actual PostgreSQL and integration. Required
source reviews are separate from the exhausted design budget; they may not conceal unresolved
design findings. Runtime service/SDK acceptance remains downstream and is not closed by this task.

Stop the implementation assignment when the full contracted source and required affected checks
are delivered with raw exits and a precise remaining-root-check list. Do not add incidental tooling,
new semantic restrictions, follow-up tickets or an implementation-only claim of final acceptance.

## Verified local acceptance

Pure ER service semantics is implemented, verified and integrated locally at
24d31cf1f97a3744db7e65c5629e056bc7a3a241 (treef1420cfc794edf257c90c79c8038672d5ca5cf2b).
Both design and source examination budgets are closed. Final source findings corrected by class:
recursive identity addressability (including implicit JSON in open objects), literal numeric
bound admission, selected union variant typing and exact YAML128-bit authoring seam. Prior
numeric operand origin, branchless request/replay, relation carrier and creation guard fixes remain.
Kernel/1 bytes, old readers, binder semantics and IO-free boundaries remain covered by the gate.

Final workspace tests, strict Clippy, fmt, pure-library Rust1.85 and actual task check all exit0,
including the real PostgreSQL lane. Independent reviewer cases remain unchanged;4 fault controls
and the additional identity regression provide red/restored-green evidence. Exact source hashes
match the committed and fast-forwarded integration tree. No planning lineage was copied.

Evidence: local-evidence:ess-evolution/waves/0009-service-convergence/service-source-correction-2/integration.md.
This completes the bounded pure semantics owner. ESS lowering, opt-in /4, Eventlog integration and
generated service/SDK/Connectors acceptance remain open under their existing parent scope.
