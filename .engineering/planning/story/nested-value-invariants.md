---
format: aep.planning-md/1
id: story:nested-value-invariants
kind: story
status: draft
title: Enforce value invariants at every typed boundary
owner: ess-evolution-01a089ee
relations:
- depends_on: story:encoded-scalar-outcomes
scope:
- confidence: cited
  path: CHANGELOG.md
- confidence: cited
  path: crates/entity-core
- confidence: cited
  path: crates/entity-surface
- confidence: cited
  path: docs/design/kernel-value-invariants-v5.md
- confidence: cited
  path: docs/requirements.md
revision: 3
---
## Context

ESS ResolvedBody::Newtype and ResolvedBody::Struct carry invariants in crates/specify/ess-compiler/src/ir.rs. ER 3af07dc checks entity invariants but FieldDefinition has no equivalent rule boundary. Lowering only top-level entity rules would omit nested argument, collection, event and error constraints.

## Acceptance

An explicit new outcome profile validates locally scoped rules on typed values at every nesting depth and input/output/default boundary, accumulates sibling failures with precise locations and distinguishes false from unobservable rules, while complete replay enforces those rules and legacy/profile-1/2/3/4 reader and serialization behavior remains unchanged.

## Design

Field invariants use the existing closed RuleDefinition AST and a fixed lexical $bound.value reference to the entire value; nested property paths and quantifier scopes stay schema checked. They cannot read entity identity/state, command arguments, siblings or outer quantifier bindings. Validate rule definitions even in dormant variants. Evaluate after structural validation and defaults at the value boundary; absent values do not acquire a value or a rule invocation. Optional wrappers put source-type invariants on the wrapped type. Preserve all failures from independent siblings and local rules. Projection must retain the rule obligation explicitly without claiming ordinary JSON Schema enforces the predicate AST.

The kernel stays pure and gains no dependencies or synthetic domain entity. This closes nested rule placement; remaining predicate/codec semantics, complete ESS lowering and Connectors adoption remain full-goal obligations. This is one prerequisite, not a multi-story decomposition requiring a critic panel.

## Scope

Cited: crates/entity-core/src/{definition,validation,runtime,outcome}.rs own schema admission, scoped evaluation and replay profiles; crates/entity-surface/src/lib.rs owns schema projection. Core/surface behavior tests, docs/requirements.md, CHANGELOG.md and docs/design/kernel-value-invariants-v5.md record the contract. No concurrent implementation is scheduled.

## Verification

Implemented explicit outcome definition/record profile 5, local field invariants with the existing closed RuleDefinition AST, and a fixed $bound.value lexical scope. Admission checks every rule/path including dormant variants; the runtime value context also refuses nonlocal references. Defaults and nullable/union boundaries preserve their established behavior. Independent local and sibling failures accumulate; false and unobservable results remain distinct. Parent checks are skipped after child rejection to avoid cascade errors. JSON Schema retains x-entity-value-invariants as an explicit runtime obligation, not a predicate assertion. R-130 and docs/design/kernel-value-invariants-v5.md record these boundaries.

The affected core/surface package suites and doctests passed. Six focused value-invariant cases cover nested siblings, absent/null/defaults, local scope, quantifier capture/shadowing, state/event/error checks, update assignments and replay changes. The final focused tests passed after extending update/replay coverage and strengthening the runtime scope guard. Clippy all-targets -D warnings, rustdoc -D warnings, Rust 1.85 all-target compilation, formatting and the existing requirement checker passed (105 requirements, 370 live test functions, zero findings). No full gate, dependency, MSRV or consumer pin change.

Independent Rust probe er-value-invariant-probe compiles authored newtype/struct invariants through ESS e14d75fb and compares ten manually expected true/false/unknown vectors against ER at root and map/list locations. It covers exact integers above 2^53, optional omission and null. The source observer binds explicit typed FactStore values to the compiled predicates; it is not a claim that an ESS-to-ER lowerer exists. Admitted records round-trip and replay. Published ER 3af07dc independently proves legacy and profile-1/2/3/4 definition/record byte equality, using collections, unions and encoded values in their respective profiles, and refuses profile-5 envelopes and invariant metadata.

Sensitivity: disabling only local rule execution made the independent probe accept -1 for Positive and fail (exit 101). validation.rs was restored byte-for-byte before focused tests and comparison passed. The initial compile check exposed four legacy replay context constructors needing explicit Some(definition); these were fixed and the existing replay suites passed. The initial ESS fixture used a bare right-hand word parsed as a text literal; its refusal was retained and the corrected fixture uses the declared bounds.end field path. These failed attempts remain distinct from successful evidence.

Evidence, exact fixture, source-pinned Rust manifest/lock and failure logs live under /home/timo/.cache/ess-evolution-20260910 with er-value-invariant prefixes. The first recorded definition remains caller-authenticated authority: semantic replay detects changes between records and recomputes values, not a consistently rewritten entire history. Story status remains draft under the local explicit-status-move policy. Remaining predicate/codec semantics, typed identity, complete ESS lowering, SDK delegation, source integration and actual Connectors runtime adoption are still required for the full goal.
