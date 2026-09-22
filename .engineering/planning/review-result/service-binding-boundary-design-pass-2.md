---
format: aep.planning-md/1
id: review-result:service-binding-boundary-design-pass-2
kind: review-result
status: active
title: Service binding boundary final complete design review
relations:
- reviews: task:service-binding-boundary
- reviews: story:service-binding-boundary
revision: 1
---
approve

Covered proposal: `docs/design/service-binding-boundary-v0.1.md` at
`sha256:8861b04214e0d27d38fa555bf5930bc98612f82d2e5f2f4f2a435853067b2e76`,
`docs/design/models/service-binding-boundary/system.yaml` at
`sha256:3354ec4adc6dfba82e280dd8af6ef4c151661eefa55f08c0d419bec803978c95`, and
`docs/design/models/service-binding-boundary/domains/contract.yaml` at
`sha256:9743dae6c6c7693b813f73df711976df4dcd0d396df70cbae2a9f18fdb07dc24`,
against Entity Runtime `24d31cf1f97a3744db7e65c5629e056bc7a3a241` and the corrected ESS
lowerer proposal at `sha256:c937aa9f039016a77e9448d8860980d554512da8d2b1aece42a48ce247a1a528`.
Prior review: `service-binding-boundary-review-1.md` at
`sha256:d7768deb21c8f77691b2d0545f4757dc4a7593aa47dcd3aedc4b622389fd742c`.

## Prior finding dispositions

1. **Resolved — implicit-branch operations.** Preparation now returns `Load` before outcome scanning
   whenever direct `decide` uses its implicit transition branch, explicitly covering every
   `kernel/1` operation and branchless `service/1` or `service/2` operation
   (`docs/design/service-binding-boundary-v0.1.md:146-155`). The continuation runs the same
   named-versus-implicit dispatch as direct `decide` rather than scanning an empty outcome list
   (`:207-218`). Verification now requires all three semantics cases to match direct evaluation,
   including transition/precondition refusals and exact accepted bytes, and the corresponding mutant
   must fail (`:504-520`, `:535-542`, and `:576-577`). This preserves the accepted implicit path in
   `crates/entity-core/src/runtime.rs:764-810` without changing direct behavior.

2. **Resolved — Kleene-decidable subject references.** Preload selection now returns one internal
   `Known(Truth)` or `NeedsSubject` value instead of treating a syntactic subject reference as an
   unconditional load (`docs/design/service-binding-boundary-v0.1.md:160-180`). `all` and `any`
   preserve the accepted dominating `False` and `True` rows, while a combination whose final answer
   can still change after loading remains `NeedsSubject`; known-input `Unknown` and its diagnostics
   remain distinct (`:182-188`). Quantifiers preserve available binders, empty-collection truth,
   missing available input, and unavailable subject collections (`:190-200`). These rules agree with
   the existing truth table and evaluator (`crates/entity-core/src/truth.rs:37-57` and
   `crates/entity-core/src/runtime.rs:1259-1503`). Named cases cover both child orders, `exists`,
   quantifiers, unknown identities, known missing input, and the exact syntactic-scan mutant
   (`docs/design/service-binding-boundary-v0.1.md:510-512`, `:538-542`, and `:574-575`).

3. **Resolved — unreachable definition error.** The contract retains
   `SemanticsKeyNotAvailable` for conditional keys under `kernel/1` and `service/1`, and the new public
   error list now contains only the four variants with concrete validation rules
   (`docs/design/service-binding-boundary-v0.1.md:305-347`). The implementation table repeats the same
   four-error boundary and stable wrong-semantics behavior (`:483-489`).

No first-pass finding is carried, and this final examination found no new finding.

## Final examination

The corrected preload contract is total over named and implicit operations, preserves registry and
argument admission, stops at state-dependent selection when the answer truly requires a subject,
binds definition/input/operation/identity into an opaque continuation, and rejoins the exact direct
decision order after entity, identity, and state checks. It gives the real PayInvoice matrix its
declared input-before-subject precedence without fabricating an instance, delegating outcome choice,
or producing a record for refusal.

The `service/2` addition remains a closed conditional-presence form over a validated optional
argument leaf. Registration distinguishes wrong semantics, bad paths, invalid targets, conflicts,
and operation updates; runtime materialization preserves absence versus present `null`, ordinary
required/determined values, and shared slots across creation state, events, and responses. Complete
normalized arguments and outputs remain in the decision record, so replay and retry recompute the
same presence choice.

The version boundary remains coherent: `kernel/1` and `service/1` definitions and `/1` and `/2`
records retain their bytes and behavior; `service/2` uses `er.record/3` and `er.request/3`; each
unchanged `er.batch/1` member carries its own record domain; older readers refuse `/3` at the tag;
and no downgrade or payload-shape inference is admitted. The lowerer handoff preserves one typed
slot per semantic bound value and covers the actual billing `note`/`issued_at`, gatepass `badge`,
payment ordering, restart/replay, and no-host-selector obligations. Implementation files, focused
tests, causal controls, purity, Rust 1.85, and repository gates are finite and sufficient for the
separately assigned source unit.

Read the complete corrected contract and unchanged diagnostic models, both review briefs, immutable
first report, original design brief/READY, lowerer review and correction, corrected lowerer design,
accepted Entity Runtime definition/validation/runtime/truth/replay and store/executor framing paths,
and the real billing/gatepass specifications and realization boundary. No Cargo command, source,
test, manifest, planning, model, or publication edit, live system, credential, external integration,
new prerequisite, ticket, or scope extension was used. The sole write was this authorized report.
No unresolved fact changes this verdict. This is pass 2 of 2; the design review budget closes here.

```findings
[]
```
