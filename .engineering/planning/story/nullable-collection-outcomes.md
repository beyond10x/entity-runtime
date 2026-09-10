---
format: aep.planning-md/1
id: story:nullable-collection-outcomes
kind: story
status: draft
title: Decide typed nullable collection outcomes in the kernel
relations:
- serves: vision:O2
- informed_by: story:versioned-outcome-decisions
scope:
- confidence: cited
  path: CHANGELOG.md
- confidence: cited
  path: crates/entity-core
- confidence: inferred
  path: crates/entity-surface
- confidence: inferred
  path: docs/design/kernel-typed-outcomes-v2.md
- confidence: cited
  path: docs/requirements.md
revision: 4
---
## Context

ESS compiled-input quantification is independently observable at ESS e14d75fb. ER outcome profile 1 at 8432f3f still cannot admit nullable typed values or typed maps and has no element quantifier. Those semantics belong inside the pure kernel before ESS lowering can preserve them.

## Acceptance

An explicitly versioned outcome definition admits and validates nullable typed values and string-keyed typed maps at every supported depth, selects outcomes with lexically scoped all/any element quantifiers using three-valued truth, and replays the complete original command with the same typed state/events/errors, while legacy and profile-1 registration refuse the new vocabulary and existing bytes remain unchanged.

## Design

Extend existing FieldKind with nullable and map, both using the existing items slot for their nested type. Nullable preserves explicit null and validates non-null values recursively; required still controls omission. Map validates every string-keyed value. Defaults never replace explicit null. No JSON catch-all substitutes for a typed element. Add closed forall and any_element condition operators with an over reference, binder name and body; existing exists retains value-presence meaning. Typed lexical binders use the reserved $bound namespace only inside their body. Empty forall is true, empty any_element false, absent/null collections Unknown, and existing Kleene dominance is preserved. Scope and collection shape are checked before execution, including dormant branches.

New outcome definition/record version 2 explicitly opts into the vocabulary. Legacy entry points and outcome version 1 retain their registration rules. Reuse kernel schema/default/reference/condition/effect machinery, with private version-aware validation; never expose a weakened branch program. Register and replay reject format downgrades. No provider, consumer pin or old event-history conversion is changed.

## Scope

- crates/entity-core — cited: definition.rs, validation.rs, runtime.rs, registry.rs, outcome.rs and regression tests.
- crates/entity-surface — inferred: exhaustive FieldKind projection must retain the typed new shapes without altering old schemas.
- docs/design/kernel-typed-outcomes-v2.md — inferred binding design.
- docs/requirements.md and CHANGELOG.md — cited local requirement and release-note obligations.

## Verification

Use independent JSON definitions and exact outcome/replay assertions for required omission versus null, nested defaults/constraints, malformed map elements, nullable payload/state, empty/absent collections, nested binder shadowing and free/outer references, Unknown and ambiguity, invalid dormant schemas/paths, wire closure and format downgrade refusal. Mutate a decisive guard and retain the failure. Run affected packages, strict Clippy, rustdoc, MSRV and the existing requirement checker; no full repository gate is authorized.

## Remaining evolution work

Non-string map key codecs, tagged unions, scalar codecs, ESS cardinality/ordinal lowering and complete ESS-to-ER integration remain required work. This story does not certify the service engine or application adoption. No new lifecycle move is requested; retain the initial story status under repository policy.

## Implemented behavior and evidence

The kernel now has Nullable and Map field kinds using the existing items contract, transparent nullable reference validation, recursive map defaults/value checks, and closed forall/any_element predicates with lexical $bound scopes. Registration is version-aware through a private outcome preparation path. Legacy registration and profile 1 refuse new schema kinds and quantifiers; profile 2 produces record/2 and replay compares the complete recomputed record. The public legacy serialization shapes were not extended with default-valued fields. Surface schema projection preserves the new inner contracts.

Affected package tests passed for entity-core and entity-surface, including the new typed outcome tests and independent JSON Schema admission tests. Strict all-target Clippy, rustdoc with warnings denied, Rust 1.85 all-target compilation and formatting passed. The existing requirement checker reports: 102 requirement(s), 348 test function(s) under crates/, 0 finding(s). The old test's deliberately future definition/2 discriminator was updated to definition/3 because version 2 now exists; separate downgrade tests and an actual published-reader probe retain the old-reader boundary. No full repository gate ran under the operator's instruction.

Independent compiled-input correspondence: ESS e14d75fb8c0b9f845dfead6a4fa5309f14bb7c63 and the new ER implementation agree on 27 List/Map, empty, absent, null and nested quantifier vectors. Expected results come from the pinned ESS compiler and InputFacts observer. The separate ER 8432f3f85b5c398ae221ab247ad72c154137b792 reader compares exact legacy definition and profile-1 record bytes and refuses both new schema kinds and version-2 discriminators. The retained probe records exact dependencies; this is comparison evidence, not an ESS lowerer or application adoption.

Mutation evidence: reversing the initial quantifier identity makes the ESS comparison fail with ER False versus ESS True for an empty input. Skipping typed map element validation makes the malformed-value test report only the unrelated payload error instead of the three additional map value errors. Both sources were restored and byte-compared to saved originals; restored typed outcome and purity tests pass. One mutation-edit selector initially refused because it was not unique and made no source change; the corrected exact loop selector was the executed mutation.

Retained evidence uses local-evidence:ess-evolution-20260910/er-typed-values- prefixes: packages-final.log, restored.log, clippy-final.log, rustdoc.log, msrv.log, requirements.log, ess-comparison.log, ess-mutation.log and map-mutation.log. The er-typed-values-probe directory contains the independent comparison source and lockfile. Builds use a task-owned temporary target; no dependency or MSRV change was made to the repository.

The story remains draft under the repository's explicit operator-only lifecycle-move policy. This status is not a claim that the implementation is absent. Non-string map key codecs, tagged unions, scalar operators/codecs, direct collection cardinality/ordinal lowering, provider adoption and ESS/service/application integration remain outside this implemented contract and remain required by the larger evolution objective.
