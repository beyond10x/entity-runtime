---
format: aep.planning-md/1
id: story:scalar-predicate-semantics
kind: story
status: draft
title: Preserve scalar truthiness and scale-dependent predicate ordering
relations:
- depends_on: story:nested-value-invariants
scope:
- confidence: cited
  path: CHANGELOG.md
- confidence: cited
  path: crates/entity-core
- confidence: cited
  path: docs/design/kernel-scalar-predicates-v6.md
- confidence: cited
  path: docs/requirements.md
revision: 3
---
## Source and outcome

ESS semantic-crosswalk.md identifies gaps between FactValue::is_truthy / Predicate::evaluate_compare and the existing ER exists / numeric ordering operators. ESS e14d75fb uses boolean value, finite numeric conversion != 0, and nonempty text other than exactly false for bare predicates. Text ordering consults all declared named scales: no common scale or conflicting orders is Unknown, including equal text absent from every scale. Existing ER numeric ordering on present nonnumbers returns false. Changing those legacy operators would alter replay meaning.

Add an explicit outcome definition/record profile for scalar truthiness and scalar comparisons. Keep the existing Condition and RuleDefinition homes, checked references and lexical value/quantifier scopes. New operations preserve numeric exact ordering of supplied normalized values, scalar type-sensitive equality, scale consensus and missing/unreadable Unknown with source addresses. Persist the scale declarations with the predicate; no ambient registry, clock, IO or new dependency. Old definition and record profiles reject the new operators, and their bytes and behavior remain unchanged.

## Acceptance and scope

Hand-audited vectors distinguish false, zero, signed zero, empty and false text from present truthy values, missing and nullable input, values above 2^53, missing/conflicting/agreeing scales, numeric versus text equality and negated Unknown. Registration refuses malformed operator shapes, unchecked references and non-scalar literal operands. Execute and replay new conditions in outcome selection, state/value invariants and nested lexical scopes; tampered records and omitted predicate execution fail independently derived vectors. Preserve existing legacy predicate results and profiles 1 through 5 with an independently pinned old reader.

Own crates/entity-core/src/{definition,runtime,validation,outcome}.rs, a pure scalar evaluator module, core tests, docs/design/kernel-scalar-predicates-v6.md, docs/requirements.md and CHANGELOG.md. No full gate. This is one prerequisite, not a multi-story decomposition. Decimal string operand codecs, complete ESS lowering, SDK delegation, application adoption and publication blockers remain separate requirements of the full objective.

## Verification and remaining boundary

Implemented explicit outcome definition/record profile 6 with truthy and scalar_compare conditions, closed typed comparison operation/scales, profile-aware scoped validation, and pure evaluation. Profiles 1 through 5 keep their schema capabilities while refusing the new operators; the next-format refusal fixture now uses profile 7. Numeric comparisons reuse exact JSON-number ordering; truthiness deliberately follows ESS finite-conversion behavior. Named scales preserve first occurrence and consensus, including missing/conflicting scale Unknown. No declaration order or lexical fallback is inferred.

Affected entity-core/entity-surface suites and doctests passed, as did strict Clippy, rustdoc, Rust 1.85 all-target compilation, repository formatting and the unchanged requirements checker. The final focused cases additionally cover scalar state invariants during creation and updates, typed event/error payload rules and replay. The checked source was restored after mutation; subsequent focused tests and the independent probe passed. No full gate or ownership run was invoked, and no runtime dependency or consumer pin was changed.

The retained er-scalar-probe binds source ESS e14d75fb FactValue/Predicate/Scales and independently expected vectors to the new kernel. It covers finite/scalar truth, exact integers above 2^53, type-sensitive equality, no common scale, conflicting and agreeing scales, identical text with and without scale authority, null and unknown. This is primitive correspondence, not a complete compiled-service/lowerer proof. Published ER 030a23de independently preserves legacy entity and profile-1/2/3/4/5 definition/record bytes, refuses new profile-6 envelopes and injected operators, and confirms legacy present-nonnumeric ordering stays false. New admitted records round-trip and replay.

Sensitivity: replacing the conflicting-scale refusal with continue made the independent ER result True instead of the manually expected Unknown and exited 101. The original scalar.rs was restored byte-for-byte before the final probe passed. The first probe compilation attempted an ESS-private numeric constructor; the corrected probe uses ESS's public serde admission, and the failed compile log is retained separately.

Evidence is retained under local-evidence:ess-evolution-20260910/er-scalar-* (package, focused, Clippy, rustdoc, MSRV, requirement, comparison, mutation and restored logs; source-pinned probe manifest/lock/source). The local status-move policy keeps this prerequisite story draft pending the operator's specific transition. Decimal-text numeric operands, full ESS lowering, SDK delegation, typed service identity, main integration and actual connectors_v2 runtime adoption remain required. The ESS publication blockers do not become satisfied by this kernel change.
