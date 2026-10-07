---
format: aep.planning-md/3
id: story:binder-elements-carry-their-declaration
kind: story
status: draft
title: Quantifier binder elements carry their declaration at run time
owner: entity-runtime
relations:
- serves: vision:O2
- informed_by: story:a-condition-reads-the-length-of-a-text
- informed_by: review-result:er-w1-u1-adversary-pass-1
scope:
- confidence: cited
  path: CHANGELOG.md
- confidence: cited
  path: crates/entity-core/src/runtime.rs
- confidence: cited
  path: crates/entity-core/src/validation.rs
- confidence: cited
  path: crates/entity-core/tests/adversary_text_count.rs
- confidence: cited
  path: crates/entity-core/tests/security_text_count.rs
- confidence: cited
  path: crates/entity-core/tests/service_values.rs
- confidence: cited
  path: docs/design/kernel-v0.1.md
- confidence: cited
  path: docs/design/service-semantics-v0.1.md
- confidence: inferred
  path: docs/ess/core-traceability.md
- confidence: cited
  path: docs/requirements.md
- confidence: cited
  path: ess/coverage.json
- confidence: cited
  path: ess/ess-inputs.yaml
- confidence: inferred
  path: ess/generated/suite.json
- confidence: cited
  path: ess/scenarios/core/service1-text-count-registration.yaml
- confidence: cited
  path: website/docs/concepts/service-semantics.md
revision: 4
---
# Quantifier binder elements carry their declaration at run time

## Outcome

Inside a `service/1` quantifier (`for_all` / `exists`), a rule reading `$t.count` on a bound element
answers with the element's declared kind: a `string` element answers its number of Unicode scalar
values, a map element its number of entries, an array element its length. Today the run-time walk
reads binder elements without their declaration (`crates/entity-core/src/runtime.rs:2803`), so
registration refuses these forms by name instead.

## Why

- ESS `docs/design/string-alphabet-and-length.md` § 3 admits `forall … t.count <= 8` over a list of
  text; `story:a-condition-reads-the-length-of-a-text` (wave 1) refuses it in Entity Runtime because
  the element's declaration is lost at run time.
- Wave 1's adversary and security passes confirmed a pre-existing defect on the same path: a map
  element's `$m.count` registered and read the member named `count` (5) instead of the map's size
  (1). Wave 1 replaced that with a registration refusal (review-result
  `er-w1-u1-adversary-pass-1`, `er-w1-u1-security-pass-1`).

## Acceptance

- Named ESS core scenarios: text, map and array elements under `for_all` and `exists` answer their
  size; the two wave-1 refusals are lifted, and the cases that assert them are rewritten to assert
  the size answer.
- Untyped binder paths (a quantifier over a `json` path or a union payload) keep resolving to
  nothing (R-97); a test shows one.
- Requirement row pinned by a live test; `CHANGELOG.md` line.

## Out of scope

Any new quantifier operator.

## Scope

Derived 2026-10-07 by `aep:story-scoper` at `cfcba172` (= `origin/main`). Every line is **cited** (read
from the story or the tree) or **inferred** (a reading that could be wrong).

- **Primary surface:** `crates/entity-core`, the `service/1` quantifier binder. At run time, `Bindings` carries the element's declared field and whether registration checked it, and `resolve_expression_optional` walks with them instead of `walk(element, path, None, false, …)`. Registration lifts the `Reader::Binder` text-length refusal — cited (story; `runtime.rs:2803`)
- **Files:** `crates/entity-core/src/runtime.rs:2306-2326` `Bindings`, `Bindings::get`; `:2786-2806` `resolve_expression_optional`, binder branch (`:2803`) — cited
- **Files:** `crates/entity-core/src/runtime.rs:2256-2304` `quantify` (builds `Bindings` at `:2285-2289`); `:1941-1977` `quantify_before_load` (`:1964-1968`); `:2020-2035` `value_needs_subject` (`:2024`) — cited
- **Files:** `crates/entity-core/src/runtime.rs:2855-2950` `lookup`/`walk` doc comments and possibly signatures, so the run time can reach the `in:` collection's declaration — inferred
- **Files:** `crates/entity-core/src/validation.rs:1506-1531` `walk_field_path`, the `FieldKind::String` count arm (refusal at `:1512-1517`); `:1319-1338` the `Reader` enum; `:1340-1370` `validate_reference`, binder branch (`:1364`) — cited
- **Files:** `crates/entity-core/src/validation.rs:1913-1998` `collection_element`/`field_at`/`element_of`, likely shared as `pub(crate)` as `content_key` is — inferred
- **Unchanged:** `collection_address` (`runtime.rs:2968-2990`), `definition.rs`, `error.rs` (no new refusal), `replay.rs`, `observed.rs`; `kernel/1` (quantifiers are `service/1`-only, `definition.rs:998`) — inferred
- **Tests to rewrite:** `crates/entity-core/tests/service_values.rs:1799-1817` and `:1875-1912`; `crates/entity-core/tests/adversary_text_count.rs:117-170` and `:174-200`; `crates/entity-core/tests/security_text_count.rs:126-160`. Every name ending `…_until_binder_elements_carry_their_declaration` names this story — cited
- **Tests kept:** `security_text_count.rs:210` (R-97: `$t.count` over a `json` path stays unobservable); `adversary_text_count.rs:311` replay test — cited
- **Tests new:** a binder whose `in:` path passes through a union payload stays unobservable; text, map and array elements under `for_all` and `for_any` — inferred
- **ESS scenario rewritten:** `ess/scenarios/core/service1-text-count-registration.yaml`, step `00:00:06` (asserts `quantifier_body_scope`) → `ok`; step `00:00:07` unchanged — cited
- **ESS scenarios new:** files under `ess/scenarios/core/` (names not fixed) — inferred
- **ESS registers:** `ess/ess-inputs.yaml:149-194`; `ess/coverage.json` (a changed assertion needs `task ess-regenerate -- --coverage-review`) — cited; `ess/generated/suite.json` regenerated — inferred
- **ESS unchanged:** `ess/domains/core.yaml`, `ess/service-semantics/domains/service.yaml` (`QuantifiedClaim` `:753-777`), `ess/generated/model.json`, `checks/ess-conformance` — inferred
- **Documents:** `docs/requirements.md:233-234` (R-160, R-161; amend or add a row after R-163); `docs/design/service-semantics-v0.1.md:1725-1740` (§ 10.6, binder row `:1732`), `:1875`, `:1878`; `docs/design/kernel-v0.1.md:412`; `website/docs/concepts/service-semantics.md:243-244`; `CHANGELOG.md:5` — cited
- **Also likely:** `docs/ess/core-traceability.md:84-85` — inferred
- **Confidence:** high for the files; open is how the run time derives the declaration
- **Would collide with:** the entity-core #54 wave on `runtime.rs` and `validation.rs` (line-disjoint per its recorded scopes, except a possible increment helper beside `field_at`), and textually on `ess/ess-inputs.yaml`, `ess/coverage.json`, `ess/generated/suite.json`, `docs/requirements.md` (next R-number), `docs/ess/core-traceability.md`, `docs/design/kernel-v0.1.md`, `docs/design/service-semantics-v0.1.md`, `CHANGELOG.md`; the Eventlog #55 wave on `CHANGELOG.md` only; `story:declared-collection-count-answers-its-declared-kind` on `walk`/`collection_address` (order matters: landing first, it turns binder array counts from a length into nothing); `story:projection-keys-read-collection-addresses` on the `Reader` enum
- **Safety fact:** untyped binder paths keep resolving to nothing (R-97) only if the run time's element declaration equals registration's `BinderScope.element` (`collection_element` → `field_at` → `element_of`, `validation.rs:1913-1998`, `None` past `json`, past any union `:1968`, past undeclared members). The run-time `walk` types a union's variant by its tag (`runtime.rs:2932`); reusing it for the `in:` path would turn `$t.count` under a collection reached through a union from nothing into a number. Level 3, unproven
- **Replay hazard:** `$g.count` on a declared map element is registered and decided today as the member value (R-161). Decision `replay` re-runs rules under the current kernel (`replay.rs:113-201`), so a recorded decision where member and size give different rule answers stops replaying. The replay test (`adversary_text_count.rs:311`) uses `{count: 1}`, where both agree. No committed example or fixture uses a quantifier. Level 2, unproven

### Not established

- The story's account of wave 1 is wrong in one respect: `$g.count` on a map element still registers and reads the member (`service_values.rs:1875`, R-161, scenario step `00:00:07`); the only wave-1 binder refusal is the text-length one (`validation.rs:1512-1517`). "The two refusals are lifted" is one refusal lifted plus one answer changed.
- The kernel's quantifiers are `for_all` / `for_any` (`definition.rs:993-998`); `exists` is the presence operator (`runtime.rs:2091`). How ESS lowers `exists` was not read.
- An array element's `.count` already returns its length (`runtime.rs:2979`), so the array part of the acceptance pins existing behaviour.
- Whether changing the map-element answer is acceptable for replay of decisions already recorded; the acceptance does not say.
- Whether the run time shares `validation.rs`'s `field_at`/`collection_element` or gets its own walk.
- R-160/R-161 amended or a new row; new scenario file names.
