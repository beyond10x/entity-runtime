# Core contract traceability

This register maps the scoped `entity-core` behavior to the existing requirement IDs, normative
design, public Rust APIs, implementation, ESS declarations and named scenarios. It describes the
current integration inputs; admitted conformance and authority adoption are recorded in
[`evidence/final/release/`](evidence/final/release/README.md).
The [canonical manifest](../../ess/ess-inputs.yaml) composes
[`entity.core`](../../ess/domains/core.yaml) with its
[`entity-core` component](../../ess/components/core.yaml). Scenario names below resolve to
`ess/scenarios/core/<name>.yaml` and have the runner ID `entity.core/authored/<name>`.

## Authority and API boundary

The retained [requirements register](../requirements.md) provides the requirement wording.
Design abbreviations in the tables mean:

- **K1:** [kernel v0.1](../design/kernel-v0.1.md).
- **K2:** [kernel v0.2](../design/kernel-v0.2.md), which supersedes matching K1 sections.
- **S1:** [service semantics v0.1](../design/service-semantics-v0.1.md), including the explicit `service/1` opt-in.
- **B:** [service binding boundary](../design/service-binding-boundary-v0.1.md), including `service/2` conditional presence.
- **F:** [operation field fulfillment](../design/service-operation-field-fulfillment-v0.1.md), including `service/3`.
- **D:** [selected creation identity](../design/selected-creation-identity-v0.1.md).

The adapter invokes `ValidatedDefinition::new`, `Registry`, `Runtime`, free decision functions,
pre-load continuations, fulfillment, `replay`, `rehydrate`, exact number comparison, identity
addressing and timestamp validation. The declarations name function calls and returned values;
they do not introduce persistence or event publication into the kernel. A response's `status`,
typed `error_kind`, `defect_kinds` and validation `paths` are derived from the actual result.
Accepted responses include the actual instance, ordered events, complete record and declared
command response. A named service refusal remains distinct from an evaluation error.

The shared `JsonDocument` owner is `entity.core`. Dynamic definitions and exact public values
cross ESS as lossless serialized documents. Binary64 behavior here is ER's runtime behavior,
not a native ESS Binary64 type-admission claim. `ReplayReturned` retains actual records only
within a scenario; `ReplayTampered` changes that supplied evidence before calling the real replay
API. Neither computes an expected replay result.

## Requirements and executable observations

Implementation paths in this table are relative to `crates/entity-core/src/`. A row establishes
the observations named in its scenarios; it is not an assertion that every clause of a broad
requirement has been covered. Additional clause scenarios are mapped below.

| Requirements / design | Public API and implementation | ESS commands and named scenarios |
|---|---|---|
| R-01, R-03, R-55, R-62, R-82; K1 §§1, 8, 11 | Pure value signatures in `runtime.rs`; closed expression resolution; dependency direction | Static checks below; `Create`: `creation-applies-defaults-and-returns-an-exact-event`; `ValidateDefinition`: `creation-template-has-no-previous-state` |
| R-02, R-05; K1 §8, K2 §3 | `create`, `decide`; ordered fields and recursive canonicalization in `runtime.rs` | `Create`: `deterministic-kernel-record-bytes` compares repeated calls to one literal complete record |
| R-04; K1 §§6–7 | Shared-reference execution; `CoreError`, `Evaluation`; `runtime.rs`, `error.rs` | `Execute`: `failed-precondition-produces-no-decision`, `invariants-judge-the-post-operation-fields`; immutable input signature is separate static evidence |
| R-10, R-12, R-15; K1 §3, K2 §1 | `Registry::{register,replace,get,iter}`, `Runtime::{create,execute}`; `registry.rs`, `runtime.rs` | `Register`, `Replace`, `Registry`, `RuntimeCreate`, `RuntimeExecute`: `registry-duplicate-and-invalid-replacement-preserve-definition`, `runtime-refuses-missing-registry-version`, `runtime-executes-the-exact-instance-definition` |
| R-13, R-113 kernel part; K2 §1 | `ValidatedDefinition::new`, `EntityDefinition::validate`; `registry.rs`, `validation.rs` | `ValidateDefinition`: `definition-validation-accumulates-independent-defects`, `registration-lifecycle-defects-do-not-cascade`, `defaults-must-satisfy-schema` |
| R-14, R-52, R-64; K1 §§3–5 | `validate_reference`, `validate_reference_path`, template scope checks; `validation.rs` | `ValidateDefinition`: `nested-reference-must-be-declared`, `invariant-cannot-read-arguments`, `precondition-cannot-read-next-state`, `creation-template-has-no-previous-state` |
| R-16; K1 §3 | Closed definition structures and `Condition::deserialize`; `definition.rs` | `ValidateDefinition`: `definition-keys-are-closed`; additional clause scenarios below |
| R-20, R-21, R-23, R-24; K1 §3.1, K2 §1 | `create`, object/field validation; `validation.rs`, `number.rs` | `Create`: `all-scalar-and-collection-kinds-validate`, `constraints-accumulate-independent-value-errors`, `creation-accumulates-field-errors`, `additional-top-level-and-nested-fields-are-explicit` |
| R-22, R-40; K1 §3.1 | `normalize_arguments`, default application; `runtime.rs`, `validation.rs` | `Create`, `NormalizeArguments`: `creation-applies-defaults-and-returns-an-exact-event`, `nested-defaults-apply-only-to-supplied-objects`, `normalization-applies-argument-defaults` |
| R-25, R-26; K1 §3.1 | Object validation and constraint-kind validation; `validation.rs` | `Create`, `ValidateDefinition`: `field-input-must-be-an-object`, `constraint-kind-is-checked`; full clause coverage remains bounded by the fixtures |
| R-27, R-28; K1 §3.5 | Reference validation and `Registry::validate_all`; `validation.rs`, `registry.rs` | `Create`, `Register`, `ValidateRegistry`: `all-scalar-and-collection-kinds-validate`, `registry-resolves-reference-targets-as-a-set`; instance existence is deliberately outside the kernel |
| R-30, R-31, R-32, R-34, R-35; K1 §3.2 | `create`, `execute`, identity/state checks and transition selection; `runtime.rs` | `Create`, `Execute`: `creation-applies-defaults-and-returns-an-exact-event`, `transition-refusal-precedes-failed-precondition`, `unknown-state-precedes-unknown-operation`; list-form transition scenarios below |
| R-33; K1 §3.2 | Transition ambiguity validation; `validation.rs` | `ValidateDefinition`: `registration-lifecycle-defects-do-not-cascade` |
| R-41, R-42; K1 §§3.3, 6 | Simultaneous pre-operation template resolution, post-set validation; `runtime.rs` | `Execute`: `assignments-read-one-pre-operation-map`, `post-set-fields-are-revalidated` |
| R-43, R-44, R-72, R-89, R-110; K1 §§3.3, 7, K2 §2 | `DomainEvent`, `DecisionRecord`, revision checks; `runtime.rs` | `Create`, `Execute`: `creation-applies-defaults-and-returns-an-exact-event`, `duplicate-events-retain-declared-order-and-multiplicity`, `silent-execution-advances-once-with-no-events`, `revision-exhaustion-refuses-before-effects` |
| R-45, R-70; K1 §6, S1 §4 | `Runtime`, `ensure_instance_matches`, ordered decision path; `runtime.rs` | `Execute`, `RuntimeCreate`, `RuntimeExecute`: `entity-mismatch-precedes-unknown-state`, `unknown-state-precedes-unknown-operation`, `unknown-operation-is-typed`, `argument-validation-precedes-transition-refusal`, `transition-refusal-precedes-failed-precondition`, `runtime-refuses-missing-registry-version` |
| R-50, R-51, R-56; K1 §§3.4, 6 | Preconditions and next-state invariants; `runtime.rs`, `error.rs` | `Create`, `Execute`: `failed-precondition-produces-no-decision`, `invariants-judge-the-post-operation-fields`, `unknown-rules-do-not-become-false`; exact diagnostic scenarios below |
| R-53, R-54; K1 §4 | Condition evaluation; `runtime.rs`, `truth.rs`, `number.rs` | `Create`: `kernel-condition-all`, `kernel-condition-any`, `kernel-condition-not`, `kernel-condition-eq`, `kernel-condition-ne`, `kernel-condition-gt`, `kernel-condition-gte`, `kernel-condition-lt`, `kernel-condition-lte`, `kernel-condition-in`, `kernel-condition-contains`, `kernel-condition-prefix`, `kernel-condition-suffix`, `kernel-condition-prefix-case-sensitive` |
| R-57, R-58; K1 §4.1 | Three-valued evaluation and unresolved diagnostics; `truth.rs`, `runtime.rs` | `Create`, `Execute`: `kernel-condition-missing`, `kernel-condition-exists`, `unknown-rules-do-not-become-false`, `unobservable-rules-report-sorted-unique-addresses`; null distinction scenarios below |
| R-59; K1 §4, K2 §3 | `is_valid_timestamp` and temporal conditions; `timestamp.rs`, `runtime.rs` | `Timestamp`, `Create`: `timestamp-48`, `timestamp-49`, `timestamp-50`, `kernel-condition-before`, `kernel-condition-after`; malformed/offset diagnostic scenarios are listed below |
| R-60, R-61, R-63; K1 §5 | Recursive template resolution against declared scopes; `runtime.rs` | `Create`, `Execute`: `creation-applies-defaults-and-returns-an-exact-event`, `assignments-read-one-pre-operation-map`, `post-set-fields-are-revalidated`, `templates-recurse-escape-dollar-and-read-post-set-fields`, `unresolved-json-template-path-is-an-error` |
| R-71, R-74, R-75; K1 §7 | Public `EntityInstance`, `CoreError`, `DefinitionErrors`; `runtime.rs`, `error.rs` | Typed refusals throughout; `Create`: `blank-identity-is-a-typed-validation-refusal`; service logical identity has its separately declared address rules |
| R-73; K1 §7, K2 §2, S1 §5 | `Decision` for accepted operations; `Evaluation` also represents named service refusal | `Create`, `Execute`: `creation-applies-defaults-and-returns-an-exact-event`, `service1-selection-fast`, `service1-selection-terminal`; the older Decision-only wording is read with the later service extension |
| R-81, R-97, R-114; K1 §10.1, K2 §2 | `replay`, `rehydrate`; `replay.rs` reruns/validates complete recorded evidence | `Replay`, `ReplayReturned`, `ReplayTampered`, `Rehydrate`: `replay-zero-event-genesis`, `replay-rejects-empty-history`, `replay-rejects-fields-tampering`, `replay-rejects-identity-tampering`, `replay-rejects-state-tampering`, `replay-rejects-revision-tampering`, `replay-rejects-missing-definition-tampering`, `legacy-event-rehydration-retains-duplicate-events`, `legacy-event-rehydration-rejects-revision-gap`, `legacy-event-rehydration-rejects-changed-payload`, `legacy-event-rehydration-rejects-missing-duplicate`, `rehydrate-rejects-empty-history` |
| R-98, R-99 kernel portion; K1 §9 | Projection declarations and registration reference/state checks; `definition.rs`, `validation.rs` | `ValidateDefinition`: `projection-references-and-state-are-validated`; actual projection results belong to the [store register](store-traceability.md) |
| R-140; S1 §§1–2 | `Semantics`, feature-key validation, versioned snapshots; `definition.rs`, `validation.rs` | `ValidateDefinition`: `conditional-presence-requires-service2`, `fulfillment-declaration-boundary-service1`, `fulfillment-declaration-boundary-service2`, `fulfillment-declaration-boundary-service3`; record/request framing assertions live in `entity.store/record-domain-*` and `older-framing-reader-refuses-before-payload` |
| R-141; S1 §4 | Ordered outcome selection, in-state short-circuit and refusal precedence; `runtime.rs` | `Execute`: `service1-first-matching-outcome-wins`, `service1-state-guard-skips-unobservable-input`, `service1-bare-state-guard-is-a-selector`, `service1-no-matching-outcome-is-typed`, `service1-selection-fast`, `service1-selection-fallback`, `service1-selection-terminal`, `service1-selection-wrong-source` |
| R-142; S1 §§2, 5–6 | Named effects, responses and registration validation; `runtime.rs`, `validation.rs` | `ValidateDefinition`, `Create`, `Execute`, `Rehydrate`: `service-outcome-names-defaults-and-observability`, `service-wrong-state-and-effect-declarations-are-closed`, `service-responses-must-be-declared-and-complete`, selection scenarios above, `legacy-event-rehydration-refuses-service1`, `legacy-event-rehydration-refuses-service2`, `legacy-event-rehydration-refuses-service3` |
| R-143; S1 §7 | `identity::address`, logical mirror validation; `identity.rs`, `runtime.rs` | `Address`, `CreateDerived`, `Execute`: `identity-address-42`, `identity-address-43`, `identity-address-44`, `identity-address-45`, `identity-address-46`, `identity-address-47`, `derived-identity-uses-logical-value`, `logical-identity-must-match-storage-address`, `operation-cannot-change-identity-mirror` |
| R-144; S1 §8 | Local carrier shape, target kind and ownership set checks; `validation.rs`, `registry.rs` | `ValidateDefinition`, `ValidateRegistry`: `references-carrier-kind-comes-from-target-identity`, `registry-relations-check-carrier-on-target`, `registry-refuses-two-owners-of-one-entity`; additional carrier scenarios below |
| R-145; S1 §10.1 | Map, adjacent union, finite Binary64 validation; `validation.rs`, `number.rs` | `Create`: `service1-map-values-are-typed`, `service1-map-member-errors-retain-paths`, `service1-adjacent-union-preserves-tag-and-value`, `service1-union-refuses-unknown-tag`, `service1-binary64-retains-negative-zero`, `service1-binary64-refuses-nonfinite-magnitude` |
| R-146; S1 §10.2, K2 §1 | `compare_numbers`, source observation/literal doors; `number.rs`, `observed.rs`, `validation.rs`, `runtime.rs` | `CompareNumbers`, `Create`: `exact-number-35` through `exact-number-41`, `numeric-wire-and-literal-doors`, `numeric-two-wire-doors`, `numeric-kernel-exact-door`, `numeric-underflow-remains-stored`, `numeric-adjacent-integers-past-binary64` |
| R-147; S1 §10.3 | `scale_compare`, declared scale snapshot; `runtime.rs` | `Create`: `service1-compare-ranked-text`, `service1-rank-without-scale`, `service1-rank-disagreement` |
| R-148; S1 §§10.4, 10.6 | Compare, truthy, quantified predicates and collection addressing; `runtime.rs`, `validation.rs`, `observed.rs` | `Create`: `service1-compare-number`, `service1-compare-missing`, `service1-truthy-true`, `service1-truthy-zero`, `service1-truthy-missing`, `service1-forall-true`, `service1-forall-false`, `service1-forall-empty`, `service1-forany-true`, `service1-forany-empty`, `service1-forany-missing`, `service1-unknown-negation`, `service1-collection-count`, `service1-array-ordinal`, `service1-map-count-not-count-key`, `service1-map-quantified-values`; depth/binder registration scenarios below |
| R-149; B §§1.1–1.2 | `decide_before_load`, `PreparedOperation::{subject,normalized_arguments,continue_with,select_with}`; `runtime.rs` | `Prepare`: `preload-payment-negative`, `preload-payment-zero`, `preload-payment-positive`, `preload-validates-input-before-refusal`; `Fulfill`: `service3-refusal-needs-no-loaded-instance` |
| R-150; B §§2–3 | Conditional source-leaf presence and complete record choice; `definition.rs`, `validation.rs`, `runtime.rs` | `Create`, `ReplayReturned`, `ReplayTampered`: `service2-presence-absent`, `service2-presence-present`, `service2-presence-null`, `service2-presence-rejects-null-string`, `conditional-presence-requires-service2`, `returned-service2-decisions-replay-and-reject-tampering`; store/executor registers cover framing and exact retry |
| R-152, R-153 kernel portion; F | `PreparedOutcome::{requirements,complete}`, fulfillment actions and removal evidence; `runtime.rs`, `replay.rs` | `Fulfill`, `Execute`, `ReplayReturned`, `ReplayTampered`: `service3-fulfillment-set-remove`, `service3-fulfillment-preserve`, `service3-fulfillment-remove-required`, `service3-fulfillment-invalid-value`, `service3-fulfillment-missing-coordinate`, `service3-fulfillment-extra-coordinate`, `service3-direct-execute-requires-fulfillment`, `returned-service3-decisions-replay-and-reject-tampering`; persistence/retry claims belong to store/executor |
| R-154; D | `decide_create_derived`, `create_derived`, selected branch and circular-read checks; `runtime.rs` | `CreateDerived`: `derived-creation-selects-first-identity`, `derived-creation-selects-second-identity`, `derived-creation-refuses-circular-selector`, `derived-creation-refuses-circular-assignment`, `derived-create-requires-logical-identity` |
| R-160; S1 §10.6 | Text length address keyed on a checked `string` declaration; `runtime.rs` (`walk`), `validation.rs` (`walk_field_path`) | `Execute`: `service1-text-count-precondition`, `service1-text-count-outcome-guard`, `service1-text-count-non-text-value`; `Create`: `service1-text-count-invariant`, `service1-text-count-scalar-values`, `service1-text-count-absent-optional`, `service1-text-count-untyped-path`; `ValidateDefinition`: `service1-text-count-registration` |
| R-161; S1 §10.6 | Text length checked for the reading walk (`Reader`); base collection forms unchanged; `validation.rs` (`validate_reference`, `walk_field_path`) | `ValidateDefinition`: `service1-text-count-projection-key`, `service1-text-count-registration` |

Positive complete replay followed by altered returned evidence is exercised for every supported
semantics through `returned-kernel1-decisions-replay-and-reject-tampering`,
`returned-service1-decisions-replay-and-reject-tampering`,
`returned-service2-decisions-replay-and-reject-tampering` and
`returned-service3-decisions-replay-and-reject-tampering`. This is distinct from accepting a
hand-authored record, and from legacy event folding, which refuses service semantics.

## Static Rust obligations

Some properties concern what an API can access or mutate rather than one returned value. These
retain explicit static checks alongside ESS:

| Requirement | Mechanical evidence and limit |
|---|---|
| R-01, R-02, R-55, R-62 | [`tests/purity.rs`](../../crates/entity-core/tests/purity.rs): `the_kernel_reaches_no_clock_filesystem_network_or_random_source`, `the_scan_sees_every_evasion_it_is_meant_to_see`, `the_scan_does_not_fire_on_prose_or_lookalikes`. The scan forbids IO/random/clock sources and unordered collections; its planted violations test the scan itself. |
| R-01, R-03, R-82 | `the_kernel_depends_on_serialisation_and_nothing_else` checks dependency tables; [`entity-core/Cargo.toml`](../../crates/entity-core/Cargo.toml) contains serialization dependencies, with provider traits owned outside core. The standalone checker's ESS dependencies point toward production libraries, never back into them. |
| R-04, R-34, R-113 | Private `ValidatedDefinition` inner value in [`registry.rs`](../../crates/entity-core/src/registry.rs), and shared-reference free execution/replay signatures. Public `EntityInstance` fields remain deserializable data; their type does not prove an instance came from this kernel or seal its lifecycle state. |
| R-90 | Workspace `unsafe_code = "forbid"`, `missing_docs` raised by Clippy `-D warnings`, crate workspace-lint enrollment, and rustdoc with warnings denied. ESS behavior scenarios do not establish source compatibility or documentation completeness. |

Existing Rust behavioral tests remain useful regression checks, but they are not substituted for
missing ESS assertions. R-11 and the YAML parsing part of R-113 belong to `entity-yaml`, outside
this scope. CLI, graph, generated-surface, MCP and other-provider clauses retain their existing
repository gates. R-80, R-83–R-88, R-100 and the async recorded-execution requirements are mapped
in the other domain registers rather than attributed to the pure kernel.

## Clause review and integration limits

The clause review identified behavior missing from the initial scenario inventory. The following
additional authored scenarios address those clauses through the same public APIs and typed
returns. They retain literal expected values, rather than asserting an existing Rust test's
boolean result. Final acceptance still requires the integrated runner evidence.

| Requirements / design | Additional named scenarios and observation |
|---|---|
| R-12, R-25, R-31; K1 §§3.1–3.2 | `default-version-multiple-transition-sources-and-object-arguments`: a successful default version, both declared transition sources, and a nonobject argument refusal. |
| R-16, R-26; K1 §§3–4 | `condition-documents-have-one-known-nonempty-operator`, `schema-constraints-refuse-every-inapplicable-family`: closed condition decoding, empty connectives and each inapplicable constraint family. |
| R-27, R-28; K1 §3.5 | `references-require-a-target-declaration-and-nonblank-value`, `registry-mutual-nested-and-argument-references-resolve-together`: typed ref validation, nested/argument targets, mutual registration and set validation. |
| R-56, R-58, R-59; K1 §§3.4–4.1 | `rule-refusals-retain-names-and-default-messages`, `missing-and-null-references-differ-from-literal-null`, `timestamp-unknown-diagnostics-and-literal-validation`: exact diagnostics, present-null/absent versus literal-null answers, and malformed or offset instants. |
| R-97; K1 §10.1 | `legacy-event-fold-checks-every-recorded-operation-fact`: positive folding followed by altered subject, from/to states, argument type, precondition input, changed fields, schema bound, invariant, event order/type and creation evidence. |
| R-140; S1 §§1–2, 10 | `kernel-rejects-service-only-declaration-families`: identity, relations, scales, creation arguments/responses/outcomes, operation responses/outcomes, new field kinds and new operators are unavailable under kernel semantics. |
| R-143; S1 §7 | `identity-rejects-untyped-and-invalid-public-address-values`: unknown/optional/JSON-containing identity declarations, wrong public value shapes, null composite members and unobservable magnitude; empty logical text remains addressable. |
| R-144; S1 §8 | `relation-carrier-optionality-and-single-claim-are-enforced`, `owns-carriers-are-required-on-the-target`: references-many shape/requiredness, both references-one optionalities, two claims on one field, and required owned carriers on the target. |
| R-145; S1 §10.1 | `service-map-and-union-declarations-and-outer-values-are-closed`: absent map key/value declarations, absent union tag/variants, nontext tags, wrong nested payload type and extra outer members with precise paths. |
| R-148; S1 §§10.4, 10.6 | `service-quantifiers-validate-binders-collections-operands-and-depth`, `nested-quantifier-binders-shadow-only-their-inner-scope`: invalid binder/body/collection/compare operand, accepted and rejected depth boundaries, and inner shadowing with positive and negative results. |
| R-149; B §1 | `prepared-continuation-binds-subject-and-matches-direct-result`, `continuation-keeps-implicit-paths-refusals-and-fulfillment-boundary`, `preload-quantifiers-preserve-vacuity-and-dominating-truth`: `Prepare`, `Continue` and `Fulfill` call the real continuation APIs; literal direct/prepared results agree, entity/identity/state checks remain ordered, implicit kernel/service paths agree, and known input can finish before loading. |
| R-150; B §2 | `conditional-presence-registration-requires-one-typed-optional-leaf`: leaf requiredness/default, parent/root closure, source path, target optionality/default/type/identity, overlapping writes and forbidden operation set-if-present. |
| R-152; F | `fulfillment-registration-closes-placement-identity-and-action-shapes`, `fulfillment-request-follows-subject-state-and-preconditions`: create/identity/unknown fields, action presence, set conflicts and mutating refusal declarations; subject, state and precondition refusals precede action parsing. |
| R-154; D | `derived-and-supplied-creation-share-literal-record-bytes`: both public creation routes return the same independently authored complete record literal, including the compatible fields carrier when the creation argument schema is empty. |

Purity, dependency direction, immutable input access and documentation/unsafe lints retain the
static obligations above. These observations cover the identified requirement clauses; they do
not claim exhaustive input combinations or replace the repository's other checks. A new
code/contract disagreement must be recorded separately rather than changing either expectation
or production behavior merely to make the suite pass.

The native ESS Binary64 admission limitation remains: exact JSON documents are the declared
input and actual response here, and ER performs its own numeric semantics without adapter
rounding. The direct-return extension and response-resource budget are required ESS capabilities
tracked separately from ER behavior. Full response documents remain intact. The checker manifest's
exact ESS revision is the dependency authority; the integration guide explains the matching CLI
pin.

The integrated reports, reviewed coverage baseline, source/executable identity and retained core
mutation evidence establish the execution result. This register itself does not declare
conformance, authorize a production correction or move an AEP artifact. Repository gate and
governance acceptance remain separate completion requirements.
