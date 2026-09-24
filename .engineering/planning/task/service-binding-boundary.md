---
format: aep.planning-md/2
id: task:service-binding-boundary
kind: task
status: implemented
title: Complete service pre-load decisions and optional-output binding boundary
refs:
- provider: ess
  reference: initiative:ess-evolution
relations:
- informed_by: task:pure-service-semantics
- serves: vision:O2
- decomposes: story:service-binding-boundary
revision: 9
---
## Approved requirement and concrete gap

ESS evolution revision1 §6 (M5/M6) requires missing ER semantics before complete ESS lowering and
SDK delegation. The accepted pure ER24d31cf1 remains verified and its design/source reviews closed.
Lowerer review1 found two concrete capabilities it cannot express: PayInvoice nonpositive-input
refusal before an unknown invoice lookup; per-invocation host-owned optional entity/event/response
presence. Existing instance-required decide and unconditional templates cannot establish those
fixture behaviors. No stateless engine or host infrastructure follows from this gap.

## Bounded outcome

Complete the pure pre-load/continuation and typed conditional-presence boundary together, with
explicit new persisted versions wherever meaning changes, exact old-reader/byte/behavior preservation,
normalized command/subject/definition identity, complete records/replay and existing pure minimum.
The accepted design docs/design/service-binding-boundary-v0.1.md SHA8861b042 and validated diagnostic
model docs/design/models/service-binding-boundary/ specify the complete interface. Final design
review9ac3faea approved with all three first-pass findings resolved; both design passes are CLOSED.
The model is not an executable duplicate of the normative Rust contract. Complete implementation
is active under the admitted contract, including original required checks and nine causal controls.

## Completion contract

Implement all admitted API/type/registration/materialization/framing/replay/retry changes in the
named entity-core, entity-store asynchronous encoding/verification and entity-executor surfaces.
Exact billing/gatepass known/unknown identity, positive/zero/negative amount, wrong-state ordering,
continuation mismatch, optional absent/present/null/type/default/reuse and replay cases, literal
version vectors and causal controls must pass. Preserve kernel/1 and service/1 canonical bytes and
behavior; retain pure Rust1.85, kernel dependency/purity rules and actual repository task check with
required backend lanes. No test counts substitute for the complete fixture/API acceptance.

At most two whole design and two source examinations for this distinct concrete seam. Do not reopen
accepted extraction/pure ER reviews or split unresolved findings into new review units. Close each
worker after its exact deliverable and checks. Root owns AEP, integration and full acceptance.
No publication, deployment, facade migration or new SDK hosting/auth/query/effect platform.

## Integration collision and sequence

The new framing changes and the existing ER Eventlog adapter both touch asynchronous store/executor
interfaces and shared manifests/docs. Source work must use isolated bases; serialize changes to
shared framing/verification helpers and exact dependency pins. Prefer accepting this bounded pure
target change before adapter integration so its readers can be checked against /3 at the final
vector. Provider administration can proceed independently. This does not make provider source
acceptance wait for the lowerer. Do not infer /3 envelope compatibility without exact reader checks.

Local implementation/review contracts and hashes:
local-evidence:ess-evolution/waves/0009-service-convergence/service-binding-boundary-design/READY.md.

## First design review correction

Root corrected all three first design findings in the same proposal. F1 now explicitly selects
implicit-branch Load for kernel/1 and branchless service/1/service/2, with continuation/direct
equivalence checks. F2 uses partial Known(Truth)/NeedsSubject evaluation through Kleene connectives
and quantifiers, preserving dominating outcomes, empty collections and genuine missing-input facts;
unloaded subject is never substituted by an absent field. Both child orders and a syntactic-scan
fault control are specified. F3 removes the unused ConditionalPresenceUnavailable variant; existing
SemanticsKeyNotAvailable owns wrong-semantics keys, four distinct conditional errors remain.

Corrected design SHA8861b04214e0d27d38fa555bf5930bc98612f82d2e5f2f4f2a435853067b2e76;
both companion model hashes unchanged and installed ESS validation0. No source implementation or
runtime verification is claimed. Whole final design review2of2 is now eligible; no budget reset.

## Final design admission

review-result:service-binding-boundary-design-pass-2 records exact final report9ac3faea. All three
first-pass findings are resolved; no new findings. Both design passes are closed and implementation
has started. Earlier first-pass/proposal language is historical. Runtime verification and source
acceptance remain open; neither is inferred from model validation or design approval.

## Completed implementation and source examination

Whole accepted source/check assignment CLOSED at local bot commit
5da72ebd0cb343912cfdcc4adbd54a8c9e16b562, tree7c30534a25b7e2afa665cee3725af4c8922bcf10,
on pure24d31cf1. Root verified20source file hashes against manifest9e900666. Accepted design8861b042
and models3354ec4a/9743dae6 unchanged. Nine compile-valid controls red101/restored0, full taskcheck0
including actual PostgreSQL12/12, pureRust1.85finalbuild0. Final doc-only requirement reference
relocation is separately checked0; executable/test/gate source unchanged after full gate.
Evidence: local-evidence:ess-evolution/waves/0009-service-convergence/service-binding-boundary-implementation/implementation-result.md.

First whole-source examination is active against exact5da72ebd in a separate managed checkout,
max2underlyingpasses. Both design passes and original pure service/1 reviews remain closed.
No implementation scope extension. Lowerer and SDK acceptance remain downstream and unclaimed.

## Final source acceptance and local integration

Implemented, verified and integrated at da5d368756f5a63e4b2efd5589f7bc3441cd7aff,
tree c84addcccd7141131fe155ef818feeb5124e2572. Whole source examination23a0d6ae
found no issues; all six added regression cases are included. One commissioned
source examination completed; no second is required without findings. Both design
passes remain closed; no underlying review budget was reset.

Complete task check passed on the final composition, including actual PostgreSQL
12/12 and the adopted reviewer cases. Pure Rust1.85 and nine compile-valid controls
retain original red/restored evidence. All twenty source hashes match. Current
upstream gate-adoption/docs ancestry was merged unchanged; common checks passed
and signed receipt verified. Canonical fast-forward preserved fifteen dirty private
planning files byte-for-byte. No publication, baseline override or journal merge.

Exact local-evidence:ess-evolution/waves/0009-service-convergence/service-binding-integration/integration.md.
The required pre-load and optional-presence target gaps are resolved. Complete ESS
lowering, SDK acceptance and full M5/M6 remain downstream and are not claimed here.
