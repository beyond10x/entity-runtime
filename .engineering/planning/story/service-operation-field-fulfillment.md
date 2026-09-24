---
format: aep.planning-md/2
id: story:service-operation-field-fulfillment
kind: story
status: implemented
title: Preserve host-owned operation fields through pure decisions and durable replay
owner: Entity Runtime maintainers
refs:
- provider: ess
  reference: initiative:ess-evolution
- provider: ess
  reference: review-result:entity-runtime-lowering-design-pass-2
relations:
- serves: vision:O2
- depends_on: story:service-binding-boundary
scope:
- confidence: cited
  path: CHANGELOG.md
- confidence: cited
  path: crates/entity-cli/src/main.rs
- confidence: cited
  path: crates/entity-core/src/
- confidence: cited
  path: crates/entity-core/tests/
- confidence: cited
  path: crates/entity-executor/src/lib.rs
- confidence: cited
  path: crates/entity-executor/tests/
- confidence: cited
  path: crates/entity-shell/src/lib.rs
- confidence: cited
  path: crates/entity-sqlite/tests/conformance.rs
- confidence: cited
  path: crates/entity-store/src/asynchronous/
- confidence: cited
  path: crates/entity-store/src/conformance.rs
- confidence: cited
  path: crates/entity-store/src/lib.rs
- confidence: cited
  path: crates/entity-store/tests/
- confidence: cited
  path: docs/design/eventlog-recorded-encoding-v0.1.md
- confidence: cited
  path: docs/design/recorded-execution-encoding-v0.1.md
- confidence: cited
  path: docs/design/service-operation-field-fulfillment-v0.1.md
- confidence: cited
  path: docs/requirements.md
revision: 26
---
## Approved requirement and exact blocker

Approved ESS evolution revision1 §6/M5/M6 requires actual billing/gatepass semantics. Final lowerer finding F1 in d3621b48 identifies omitted operation fields as host-owned. Accepted ER da5d3687 does not express explicit field removal or record that choice; its completed pre-load/presence acceptance remains valid.

The complete typed correction is ESS docs/design/ess-evolution/entity-runtime-lowering.md SHA25622d0eb4bb0c0847855b75e2458c3ce57d573c3d12e068ddcf360a31001a536ce and validated diagnostic model docs/design/models/entity-runtime-lowering, model3f9bffda. Existing typed inventory names OperationFieldRequirementKind, OperationFieldActionKind and OperationFieldFulfillmentInventory. Author correction closed; no third lowerer design review.

## Complete bounded delivery and acceptance

Implement the entire operation-field class: pure Service3 typed Set/Preserve/optional Remove after exact-subject load and ER-selected accepting outcome, closed registration, opaque continuation, durable actions/removals, record/request4 canonical replay/retry/folding and old-byte compatibility. No lowerer default, SDK outcome selector or post-record state patch. Preserve pure MSRV/dependencies and full gates.

Exact fixed source/check/stopping contract: local-evidence:ess-evolution/waves/0009-service-convergence/operation-field-fulfillment-unit.md. Prepared, not dispatched. One whole source delivery, not separate API/format tickets. Original finding remains open at executable acceptance until this passes. No design-review reset. At most2whole new-source examinations after delivery. Stop author at all scoped checks/report, root integrates. Blocks full lowerer and SDK/service acceptance; adapter source and accounting proceed independently, shared integration is serialized.

## Source dispatch after shared-source handoff

2026-09-16T10:22Z. Adapter worker confirmed worktree-local full task check and pure MSRV exit0; final contract audit found projection capture compared keys/counts but not independent row values. Its remaining correction is bounded to entity-eventlog. Existing shared ER source freeze is reaffirmed.

Dispatch complete operation-field source/tests now in its own accepted da5d3687 tree. No build slot; source-only until explicit grant. Active-waves still reports full-scope overlap; no claim of disjoint source or reordered integration. Root composes frozen shared changes and serializes docs/CHANGELOG before final gates. Existing original unit contract and all checks unchanged. This unblocks M5 implementation while M2 completes its original capture correction.

## Complete fulfillment source delivered; independent source examination active

The complete operation-field amendment is submitted at bff1d7f8953aa932ca538913eb3b8e7ed66fbdfe (tree 2f3f1a1393ef51117eaa42cb60a09a69c2782e6e), based on accepted da5d368756f5a63e4b2efd5589f7bc3441cd7aff. All 22 submitted source hashes match the final author manifest; both commit identities are the organization bot. The affected packages, Rust 1.85 workspace build and actual offline task check passed, including the PostgreSQL provider. Eight compiled mutation controls failed at the intended assertions and were restored. Signed common checks passed and the receipt verified.

The author assignment is closed and its lease released. The first independent examination of this complete source amendment is active in a separate checkout at the exact submitted commit. At most two whole source examinations apply; existing lowerer design examinations remain closed. No local integration or ESS/SDK acceptance is claimed yet. Original service lowering and real billing/gatepass acceptance remain required after target acceptance.

Root serialized the original requirement pins R-152/R-153 and changelog before the final gate. Adapter integration must preserve its public validation seam while adopting the new shared record/request framing, followed by the composed gate. Provider administration acceptance remains a separate unresolved restriction.

## First source examination closed; whole correction dispatched

Complete operation-field target is verified and integrated locally at250f6993181822ab1e36c17d38dbc909d084d423, treee638ce427b25e1e756f643e9dfa33ca610f2927c. All25 source hashes match exact manifest e1adfac7da661743b1d551ab2712fdd02467a25a6078d574a4c9b4d617215036; all19 canonical planning hashes survived the fast-forward unchanged. Bot identities and signed common receipt verified. Full actual-PostgreSQL task check, strict formatting/Clippy/docs, Rust1.85 and original causal controls passed.

Both complete source examinations are closed. First-review exact legacy retry conflict corrected at a66122d; final-review legacy event-removal defect corrected across creation/operation/all multi-event members, with original reviewer regressions retained unchanged. Coordinator directly verified final finding against source and executed evidence; original final review remains needsrevision and its fixed outcome is separate. No third review. Exact receipt local-evidence:ess-evolution/waves/0009-service-convergence/operation-field-fulfillment-integration/integration.md.

The target now supports complete post-selection Set/Preserve/optionalRemove with service/3 and record/request4 while preserving legacy bytes and refusals. This closes this ER target story. Full ESS lowering, SDK service acceptance and composed adapter gates remain their original downstream outcomes; no publication or cutover.
