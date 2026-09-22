---
format: aep.planning-md/1
id: task:service-semantic-contract
kind: task
status: implemented
title: Define complete pure service semantics and replay compatibility
refs:
- provider: ess
  reference: initiative:ess-evolution
relations:
- decomposes: initiative:entity-runtime
- serves: vision:O2
revision: 10
---
## Approved requirement and evidence gap

ESS evolution revision 1 step 6 (M5/M6) requires ER to represent the service semantics before ESS
admits their lowering. Existing owner initiative:entity-runtime and the coordinating ESS initiative
retain the implementation outcome. The exact crosswalk at local-evidence:ess-evolution/waves/0009-service-convergence/source-routing-result.md
identifies unsupported creation multiplicity, state-preserving update, named outcome branching,
identity/relation contracts, predicate/type invariants and exact-value mappings. Existing ER async
and provider evidence cannot prove semantics absent from entity-core's current definition model.
This is the single binding contract for that missing semantic implementation, not a new feature.

## Acceptance

One internally coherent, concrete, versioned pure ER service-semantics contract covers all ten
approved dimensions, preserves existing definition/record readers and canonical fixtures, and
names executable acceptance for decisions and complete replay without depending on Eventlog.

## Bounded deliverable and scope

Inferred new docs/design/service-semantics-v0.1.md and, where new semantic nouns need typed homes,
ess/service-semantics/ model documents. Reuse existing recorded-execution models and current ESS
service contract types rather than inventing an unrelated entity vocabulary. Cite the existing
kernel/record/executor interfaces and exact concrete Rust additions/changes the implementation
will require. Cover creation, updates, transitions, predicates, invariants, identities, relations,
exact values, named outcomes and zero/one/many ordered events in one contract. Host-provided
responses, generated values, authentication, authorization and effects remain explicit binding data.

Retain old meanings and bytes; select closed opt-in versions where persisted semantics change,
with old-reader rejection specified before implementation. No generic facet registry, arbitrary
property bag, ESS/AEP dependency in the kernel, guessed defaults, synthetic self-transition for an
update, caller-selected outcome masquerading as kernel branch selection, or silent unsupported
subset presented as full admission. SQL/provider/adapter code is outside this pure contract.

## Checks and stopping condition

Fresh Opus authors the concrete contract against the already measured crosswalk, not another
source-routing survey. No Cargo build, service, network, store write, production source or children.
If authored ESS model files are added, run the existing ess specify validate CLI and retain the
actual result without compiling another executable. Finish with file hashes, exact version choices,
acceptance commands/cases for the eventual implementation and only irreducible concrete decisions.
Then close the author assignment. Root owns design admission and the existing maximum two technical
review passes, followed by the full semantic implementation under this same M5 outcome; no review
or SQL rejection budget is reset. This design task does not close M5/M6 or replace implementation.

## Review and bounded correction

First independent whole-contract review requires revision: seven blocking findings and two warnings,
plus timestamp mapping and fallback-order questions to resolve against source. Exact report is
local-evidence:ess-evolution/waves/0009-service-convergence/semantic-contract-review-1.md.
The author and first substantive review assignments are closed; design is not adopted.

Correction 1 is separately bounded in local-evidence:ess-evolution/waves/0009-service-convergence/semantic-contract-correction-1-brief.md.
It corrects the same complete ten-dimension contract, including required billing map/union types,
creation/record/version/replay meaning, relation carriers, branch order, effects, responses and
selector scope. Its stopping condition is corrected contract/model, actual model validation,
hashes and complete finding dispositions, or a finite cited irreducible decision. No source Rust
implementation in this assignment; exactly one substantive review pass remains after correction.

Initial AEP ingestion of the first report refused malformed findings YAML. Preserve the original
report and have its reviewer correct quoting only; this does not consume or create a review round.
Record the exact corrected report immutably after successful parsing. No finding is yet marked fixed.

## Whole contract completion after correction

Correction 1 author closed successfully (handle 79585 exit 0); its model/control validations pass
and useful creation/record-version/map/union/relation corrections are retained. Root did not admit
the proposal: sections 7/10/15 still replace source-supported predicates, quantified/nested
invariants, non-text identity and some branch behavior with target refusals. Approved step 6 requires
implementing those missing ER semantics, so billing-only success cannot close the full contract.

Separately bounded whole-contract completion now runs under this same task and review budget:
local-evidence:ess-evolution/waves/0009-service-convergence/semantic-contract-completion-brief.md.
It specifies concrete pure source-faithful semantic operators/types, preserves kernel/1 bytes and
behavior, and reconciles the entire contract/model/test table. No production Rust or new reviews
in the author assignment. Stop at the complete contract and installed-model checks, or one finite
source contradiction requiring a decision. Exactly one substantive review pass remains unused.

## Full contract submission and final review

Full-contract completion author CLOSED (handle 81185 exit 0); own lease released. It now specifies
closed quantifiers/nested invariants, source scalar and scale semantics, typed identities and the
union-based wrong-state contract, alongside the earlier creation/record/map/union fixes. This is a
submitted complete design, not admitted semantics or runtime implementation.

Root recomputed all three deliverable hashes:
docs/design/service-semantics-v0.1.md 5af2f4753702d5cfc54ca701f9d0063b72e7412aa0950fe90a0e85ef560295e7
ess/service-semantics/system.yaml ca77cd1c65422fbaf316e496e127873cc9a93422ffda5a2c16cefef1086f683f
ess/service-semantics/domains/service.yaml 71c014524fae0095c76f356b8844b87a13993dc29dadc7db8694eb704dd23aca
Actual installed model/control validation and authored source witnesses are in
local-evidence:ess-evolution/waves/0009-service-convergence/semantic-contract/completion/ess-specify-validate.txt.

Second and FINAL substantive independent review is active under the original two-pass limit:
local-evidence:ess-evolution/waves/0009-service-convergence/semantic-contract-review-2-brief.md.
Root identified remaining assessment risks in exact Number/Binary64 conversion, empty and composite
identities, and the documented UnspecifiedMoveSource choice. Reviewer judges the full contract and
first-review corrections, records one complete report and closes. No third review or status promotion
is admitted here; unresolved findings stay explicit. The final source/Rust implementation is still due.

## Final review findings and bounded resolution

The second and FINAL independent review closed (handle 57469 exit 0), needs-revision with five
blockers and one warning. Exact immutable review-result:service-semantic-contract-round-2 is
recorded. Findings: missing Binary64 address rule; creation template argument scope; optional
References carrier; state-only selector; numeric parity/direction; inconsistent step references.
The review budget is exhausted. No third review, new review unit or status promotion is authorized.

A separately bounded correction against all six findings is active, with root's explicit policy
choices, under this same task: local-evidence:ess-evolution/waves/0009-service-convergence/semantic-contract-final-correction-brief.md.
Root selects current ESS numeric observation semantics for service/1 predicates while preserving
exact stored tokens and old kernel/1 operators; the separate future decimal-reader change remains
out of scope. New service/1 text storage addresses may use total injective encoding, preserving
public logical IDs and old kernel/1 records rather than refusing source-admitted empty identities.
The author must reconcile all model/format/binding/test implications and check the synthesizer's
answer to the remaining move-source witness. Stop at the corrected whole contract and model checks,
or one finite cited source contradiction. Root verifies corrections and records unresolved findings;
the completed reviewer and prior authors remain closed. No runtime implementation is claimed yet.

## Final corrective author closure

Final corrective author closed at handle 64989, terminal exit 0 (d40d23), session
e5500014-0916-4000-8000-202609160215, observed model claude-opus-5. Its fixed contract was
the six findings from immutable service-semantic-contract-round-2 plus the stated numeric and
identity resolutions. No third review was commissioned. The author reports its own lease released.

Delivered design SHA 7136d438d3518d8210e5f396284fad4db36312bd2c23f2de7ad628419b57cec2,
system SHA ca77cd1c65422fbaf316e496e127873cc9a93422ffda5a2c16cefef1086f683f,
domain SHA 6fb5b7861a346beb628fe0b96589737644441cd239b18ae88b4154bb1c7c5126.
Root independently recomputed all three hashes and confirmed only the intended untracked design
and model paths in the author tree. Installed ESS validates the corrected model and existing
recorded model; the absent-path negative control refuses. These are model checks, not runtime proof.

Evidence: semantic-contract/final-correction/handoff.md, hashes.txt, ess-specify-validate.txt
and cli.stream.jsonl under the root initiative's wave 0009 evidence directory.
Root must still verify the source-level dispositions and record design admission or concrete
unresolved contradictions. The task remains active. Both independent review passes remain consumed;
the review verdict is not rewritten by the correction author's claim. No Rust implementation,
full ER gate, runtime integration or M5/M6 acceptance is claimed. The next authorized implementation
is the complete pure service semantics after admission, preserving kernel/1 and recorded replay.

## Coordinator admission

Coordinator resolved the final six design findings and admits the complete contract for implementation.
Both independent design review passes remain closed; this does not rewrite their immutable verdicts.
The final author fixed Binary64 addressing, creation argument templates, relation optionality,
state-only selection and evaluation order. Root verified source and corrected numeric profile
selection with actual Rust witnesses and locked Cargo feature inspection, plus the remaining
text-relation carrier narrowing. Final model validation is green.

Exact decision and evidence: local-evidence:ess-evolution/waves/0009-service-convergence/semantic-contract/coordinator-verification/admission.md.
Typed model: ess/service-semantics/ in ess-evolution-service-semantics-20260916; complete binding
design: docs/design/service-semantics-v0.1.md in that tree. Final submission hashes are retained
beside the admission. No Rust implementation or runtime acceptance is asserted by this task.

The design task's bounded deliverable is complete. The separately owned whole pure-service
implementation must satisfy all ten semantic dimensions, kernel/1 byte/refusal compatibility,
er.record/2 and er.request/2 replay, exact values and actual required code/gate checks before landing.
