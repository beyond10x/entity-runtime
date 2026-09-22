---
format: aep.planning-md/1
id: task:pure-service-semantics
kind: task
status: implemented
title: Implement complete pure service semantics and recorded replay
refs:
- provider: ess
  reference: initiative:ess-evolution
relations:
- serves: vision:O2
- informed_by: task:service-semantic-contract
- decomposes: story:pure-service-semantics
revision: 8
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

## Complete source submission

Complete pure service implementation delivered and preserved as local bot-authored submission
cb2c1a16602afec90912ec12fb33605d33abdb51, tree901239ea8247c992ca030cef30e3d5ac337d7455.
The service-semantics checkout is clean. Author/committer identity verified as b10x-bot[bot].
No publication or integration. Implementor18109 closed exit0 and released its own lease/build token.

Root checked all28 source hashes against the handoff, all six raw check-log exits0 and the20
fault-control records (mutated101/restored0 with exact-byte restoration). Required named test and
complete semantic coverage still receive independent source examination; counts alone are not
acceptance. The admitted design/model hashes remain unchanged.

Evidence: ESS evolution wave0009 service-implementation/handoff.md, changed-files.sha256,
check-{test,clippy,fmt,msrv-1.85.0,task-check,ess-specify-validate}.log and fault-sensitivity.json.
The local task gate explicitly skipped PostgreSQL because ENTITY_POSTGRES_URL was unset. This is
NOT a passing PostgreSQL lane or full integration receipt. Root retains actual disposable-PG gate,
independent source examination and integration before declaring the owning outcome verified.

One numeric fault exposed integer-shaped authored literal parsing that rounded a literal above
the binary64 exact span. The author fixed it, added the decisive regression and reran controls.
An external one-off Python fault driver was disclosed outside the assigned evidence directory,
contrary to the brief's no-new-checker/evidence-location rules. It is not included in the submission;
raw source-mutation/test/restore records remain available for examination. No incidental tooling
rewrite is added to the product completion requirements.

Next: examine this exact source submission, execute actual PostgreSQL at accepted source, resolve
findings within the two-pass SOURCE budget and integrate. Both DESIGN passes remain closed.
M5/M6 also still require ESS lowering, SDK /4 and real service acceptance under the full roadmap.

# Root disposition of source review1

2026-09-16T01:26Z. Reviewer97176 terminal0 (9b29c4); original exact report preserved, never edited.
All2 test hashes match report. Raw core suite6failing review cases, store1failing review case plus
one ENOSPC case: 7 review failures +1environment failure. Report's later six-review-failure/count
sentence is arithmetic error; no new gate verdict is inferred. Report lists3warning severities,
not2. Source origin is source-established only, no base test execution claim.

F1–F5 accepted for one bounded correction under existing task:pure-service-semantics:
- F1 numeric equality/membership must retain operand origin like compare.
- F2 branchless service/1 creations must preserve caller input in request comparison/replay/retry.
- F3 References One/Many carrier kind must match target identity kind.
- F4 out-of-domain authored numeric literals must be refused across all admitted operators.
- F5 creation in_state guard must actually apply using lifecycle initial state.
Close each class, retain legacy kernel/1 behavior and complete original service scope.

F6 requires clarification, not the proposed behavioral change. The report correctly measures
binder shadowing. However, the authoritative source ESS predicate Element::rebind (predicate.rs
around435) rewrites ANY matching first namespace, then passes through other namespaces. Admitted
ER design §10.4 likewise says $<bind> is the element, every other address passes through, and inner
same-name binder wins. Giving fixed roots precedence or rejecting these binder names would narrow
that source behavior. The vague §2.2 sentence 'Nothing else changes' must be clarified to apply
only to addresses not matching the binder. Preserve shadowing including fixed-root names, pin it
against actual ESS source and correct the copied test expectation/name. Original reviewer test
and raw red remain immutable in its separate tree/evidence. This is an adjudicated incorrect
expectation, not weakening a required check to obtain green. Reviewer2 receives this reasoning.

F7 is an infeasible presence distinction in current typed definition, as reviewer notes. The
single-variant NumberObservation defaults to source-number/1; typed admission cannot distinguish
explicitly supplied default from omitted default, and kernel/1 serialization omits it. Clarify
§2.1 to exclude that indistinguishable default from its refusal list and retain opt-in service/1
interpretation plus unknown variant decoding refusal. No format/variant or new kernel restriction.
Preserve old bytes, snapshot behavior and existing old-reader tests. This corrects documentation,
not an excuse to drop numeric semantics.

No third design pass. Source review1 is used; one final source examination remains after complete
correction, with all original findings/dispositions supplied. No extra unit resets that budget.

## Final source examination correction contract

Final source examination2of2 on f7904be5 is closed. Root exact executions expose three introduced
assertion failures: nested JSON identity admission, unobservable numeric bounds, and typed union
continuation. The fourth new case fails at a pre-existing YAML i128/u128 decode refusal before its
planned parity assertion. Do not claim observed rounding. Both source-review passes are used;
no third or renamed examination is scheduled.

M5's total identity and exact typed value contract requires recursive identity addressability,
literal-domain min/max admission, and union continuation using the actual selected variant.
The shipped YAML authoring path must also represent the admitted exact integer literal; its
bounded decoder seam is included here, with no generic decoder rewrite or new numeric format.
Original reviewer cases and earlier dispositions remain immutable. Creation-key and observation
framing notes receive bounded documentation clarification, with no changed legacy behavior.

Stop after these classes, retained red/green evidence, complete workspace/Clippy/fmt/pure-library
MSRV and full gate with actual PostgreSQL. Root inspects the final correction against this closed
review and integrates the qualified source. No ESS lowering/SDK implementation is included in this
assignment. Evidence: local-evidence:ess-evolution/waves/0009-service-convergence/service-source-review-2/report.md
and service-source-correction-2-brief.md. Pure ER completion does not close end-to-end M5 acceptance.

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
