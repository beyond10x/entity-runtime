---
format: aep.planning-md/3
id: story:declared-collection-count-answers-its-declared-kind
kind: story
status: draft
title: A declared collection answers count only for its declared kind
owner: entity-runtime
relations:
- serves: vision:O2
- informed_by: story:a-condition-reads-the-length-of-a-text
scope:
- confidence: cited
  path: CHANGELOG.md
- confidence: cited
  path: crates/entity-core/src/runtime.rs
- confidence: inferred
  path: crates/entity-core/tests/service_values.rs
- confidence: inferred
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
- confidence: inferred
  path: ess/scenarios/core/service1-collection-count-declared-kind.yaml
revision: 4
---
# A declared collection answers count only for its declared kind

## Outcome

Under `service/1`, `<path>.count` on a field declared as a map answers only when the stored value is
an object, and on a field declared as an array only when it is an array; a stored value of another
kind answers nothing (the rule is unobservable), as a declared `string` holding a non-text already
does after wave 1 (R-160).

## Why

Found in wave 1's correction round for `story:a-condition-reads-the-length-of-a-text`, measured with a
probe test that was run once and deleted: a declared map holding a 7-element array answered 7, and a
declared array holding `{count: 7}` answered 7. Pre-existing since R-148. Only a stored instance that
does not conform to its definition reaches it: arguments are validated before any rule reads them,
and no store validates instances on load. A patch was written in the wave's scratch directory and
deliberately not applied, because it changes R-148's behaviour.

## Acceptance

- Two tests, red before the fix: a declared map holding an array and a declared array holding an
  object each make a rule reading `.count` unobservable.
- R-148 amended and pinned; `CHANGELOG.md` line.

## Out of scope

Validating stored instances on load.

## Scope

Derived 2026-10-07 by `aep:story-scoper` at `cfcba172` (= `origin/main`). Every line is **cited** (read
from the story or the tree) or **inferred** (a reading that could be wrong).

- **Primary surface:** the `service/1` run-time path walk in `crates/entity-core/src/runtime.rs`. Run time only: registration already admits `count` on a declared `array` (`validation.rs:1467`) and `map` (`:1492`) — cited
- **Defect, declared map holding an array:** `collection_address` arm `runtime.rs:2979` (`Value::Array … if segment == "count"`) answers whatever `field` declares — cited (walked at `cfcba172`)
- **Defect, declared array holding `{count: 7}`:** no arm at `:2975-2989` matches and the stop at `:2921-2924` is map-only, so `walk` reads the `count` member at `:2927` — cited (walked)
- **Symbols:** `collection_address` (`:2966-2991`), `walk` (`:2881-2940`, doc `:2881-2890`), `lookup` doc (`:2855-2861`) — cited
- **Also reaches:** `set` and event templates, which resolve through the same `lookup` (`runtime.rs:2840`) — cited
- **Unchanged:** `validation.rs`, `definition.rs`, `error.rs`, `replay.rs`, `quantify` (`runtime.rs:2262-2280`) — inferred
- **Tests:** two new tests in `crates/entity-core/tests/service_values.rs`, § 10.6 collection addressing (`:1371-1489`), shaped like `a_stored_text_holding_an_array_answers_no_length` (`:1918`) — inferred
- **ESS scenario:** new `ess/scenarios/core/service1-collection-count-declared-kind.yaml` (name invented): two `Execute` steps with a nonconforming stored instance answering `precondition_unobservable`, as `service1-text-count-non-text-value.yaml` does — inferred
- **ESS inputs:** `ess/ess-inputs.yaml`, `service1-*` list `:154-165` — cited
- **ESS coverage:** `ess/coverage.json` (`review` line `:3`, scenario list `~:162`, digests `~:599`) — cited
- **ESS derived:** `ess/generated/suite.json`, regenerated — inferred
- **ESS unchanged:** `ess/domains/core.yaml`, `ess/generated/model.json` — inferred
- **Documents:** `docs/requirements.md:219` (R-148 amended, two new pins); `CHANGELOG.md:5` — cited
- **Also likely:** `docs/design/service-semantics-v0.1.md` § 10.6 (`:1679-1695`) and its test table (`:1870-1871`); `docs/ess/core-traceability.md:79` (R-148 row) — inferred
- **Confidence:** high — both probe cases walk to `runtime.rs:2979` and `:2927`
- **Would collide with:** the entity-core #54 wave on `runtime.rs` at file level only (their cited hunks `:904-907`, `:1263-1500`, `:2691-2738`, `:2775-2781`; this story's `:2855-2991`), and on `ess/coverage.json` (`review` line, certain), `ess/ess-inputs.yaml`, `ess/generated/suite.json`, `docs/requirements.md`, `docs/ess/core-traceability.md`, `CHANGELOG.md`, possibly `service_values.rs`; the Eventlog #55 wave on `CHANGELOG.md` only; any unit changing `walk`'s `field` parameter or `collection_address`, including `story:binder-elements-carry-their-declaration` (`runtime.rs:2803`)
- **Safety fact:** a value that matches its declaration takes the same arm as today. Paths with no declaration (`field: None`: binder, `json`, undeclared members) are untouched if the guard keys on a declared kind. Only a stored value whose kind disagrees with its declaration changes its answer, and a recorded decision over one replays differently (R-97); that is the intended R-148 amendment. Level 3, unproven

### Not established

- Which signal the new guard reads: the declared `field` (as the map arm at `:2976`) or the `checked` flag (as R-160's text arm at `:2907`). Keyed on `field`, a tag-selected union variant holding a nonconforming value changes too.
- A declared array holding an object also answers `<array>.0` from a member named `"0"` (via `:2927`); the story names only `.count`.
- `for_all`/`for_any` (`runtime.rs:2276-2280`) decide by the stored value, not the declaration; outside this story's wording.
- Function-level overlap with the #54 wave could not be confirmed: it had no `runtime.rs` diff when read.
