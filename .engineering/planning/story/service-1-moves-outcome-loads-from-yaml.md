---
format: aep.planning-md/3
id: story:service-1-moves-outcome-loads-from-yaml
kind: story
status: implemented
title: A service/1 outcome with a moves effect loads through entity-yaml and the entity command
owner: entity-runtime
relations:
- serves: vision:O2
scope:
- confidence: cited
  path: CHANGELOG.md
- confidence: inferred
  path: crates/entity-core/src/definition.rs
- confidence: inferred
  path: crates/entity-runtime-docs/src/status.rs
- confidence: cited
  path: crates/entity-yaml/src/lib.rs
- confidence: cited
  path: crates/entity-yaml/tests
- confidence: inferred
  path: docs/requirements.md
- confidence: inferred
  path: website/data/status.json
- confidence: cited
  path: website/docs/concepts/service-semantics.md
- confidence: inferred
  path: website/docs/reference/definitions.md
- confidence: inferred
  path: website/docs/status.md
revision: 7
transitions:
- {from: "draft", to: "proposed", at: "2026-10-07T09:15:43Z", actor: "human:timo", revision: 5}
- {from: "proposed", to: "active", at: "2026-10-07T09:15:44Z", actor: "human:timo", revision: 6}
- {from: "active", to: "implemented", at: "2026-10-07T13:09:33Z", actor: "human:timo", revision: 7, decided_on: {"recorded":{"test_result":1}}}
---
# A service/1 outcome with a moves effect loads through entity-yaml and the entity command

## Outcome

A `service/1` definition whose outcome declares a `moves` effect (`OutcomeEffect::Moves { to, from }`,
`crates/entity-core/src/definition.rs:842-857`) loads from YAML through `entity-yaml` and the
`entity` command, as it does from JSON.

## Why

Found by the 2026-10-06 documentation overhaul (Stage A, worktree `wt-747b1108ca67`): the map form
fails with `expected a YAML tag`, and the tagged form with `expected unambiguous YAML`. The new
`website/docs/concepts/service-semantics.md` states it as a known limitation. Not re-run by the
coordinator; the failure strings are the docs agent's.

## Acceptance

- A YAML fixture with a `moves` outcome validates with `entity validate` and executes; a test in
  `crates/entity-yaml/tests/` pins it, red before the fix.
- The documented YAML spelling is stated once, in the definitions guide, and the known-limitation
  note is removed.

## Out of scope

Changing the JSON form.

## Scope

Derived 2026-10-07 by `aep:story-scoper` at `cfcba172` (= `origin/main`). Every line is **cited** (read from the story or the tree) or **inferred** (a reading that could be wrong).

- **Primary surface:** `crates/entity-yaml` — cited (story Outcome and Acceptance)
- **Files:** `crates/entity-yaml/src/lib.rs:55-61` `from_str`: its second pass reads enums only as `!tag`; the map-form error comes from `serde_yaml_ng-0.10.0/src/de.rs:1760` — cited
- **Files:** `crates/entity-yaml/src/lib.rs:79-169` `NoDuplicatesVisitor` has no `visit_enum`; its `expecting` at :85 produces the tagged form's "unambiguous YAML" — cited
- **Files:** `crates/entity-yaml/tests/`, a new test with a `moves` fixture that fails before the fix — cited (Acceptance)
- **Symbols:** `OutcomeEffect::Moves { to, from }`, `crates/entity-core/src/definition.rs:848-898` (the story's `842-857` is out of date) — cited
- **Also likely:** `serde_yaml_ng::with::singleton_map_recursive` (present in 0.10.0, `src/with.rs:988`) inside `from_str`, adding no dependency — inferred
- **Alternative site:** a hand-written `Deserialize` for `OutcomeEffect` in `crates/entity-core/src/definition.rs:848-871`; the kernel cannot take a YAML dependency (`tests/purity.rs`) — inferred
- **No change:** the `entity` command. `load_definition` (`crates/entity-cli/src/main.rs:1505-1508`) and generated CLIs (`:1364`) both go through `entity_yaml::from_str` — cited
- **ESS:** no file. `entity-yaml` is outside the `ess/ess-inputs.yaml` composition, and `ess/service-semantics/domains/service.yaml:83,337` models `EffectKind` without a spelling — inferred; it becomes ESS core scope if the fix moves into `definition.rs`
- **Documents:** remove the known-limitation note at `website/docs/concepts/service-semantics.md:112-120` — cited
- **Documents:** state the YAML spelling once in `website/docs/reference/definitions.md` (the story's "definitions guide"; no page has that name) — inferred
- **Documents:** move "Moves outcomes in YAML" from planned to shipped at `crates/entity-runtime-docs/src/status.rs:378-383` with the new test as evidence, regenerating `website/docs/status.md:87` and `website/data/status.json:314-319` — inferred
- **Documents:** `CHANGELOG.md:5` — cited; the R-11 row (`docs/requirements.md:47`) may add the new test — inferred
- **entity-yaml public API:** unchanged; `from_str` accepts a document it used to refuse — inferred
- **Consumer surface:** `AGENTS.md` § Boundaries treats any `entity-yaml` change as a coordinated migration (atlas `e5ee9d6c`, bench `0.17.3`) — cited; whether a change that only accepts more triggers it is the coordinator's call
- **Confidence:** high on surface; medium on placement (the fix could land in `definition.rs`)
- **Would collide with:** both in-flight waves at `CHANGELOG.md`; the entity-core #54 wave only if the fix lands in `definition.rs:848-898`, and at `website/docs/reference/definitions.md` and the end of `docs/requirements.md`; no overlap with `entity-eventlog` — inferred
- **Semantic overlap:** if the #54 wave makes `{increment: n}` / `{cleared: true}` an externally tagged enum rather than a `Value`, YAML loading hits this same defect, and a recursive singleton-map fix changes how those load — inferred
- **Safety fact:** `OutcomeEffect` is the only externally tagged enum with data that an `EntityDefinition` deserializes; the others are unit-only, `untagged` or hand-written, so a recursive singleton-map pass changes no other accepted spelling. Unproven

### Not established

- Both failure strings were traced to code, not reproduced.
- Whether `singleton_map_recursive` behaves with `Condition` (untagged, hand-written `Deserialize`) and `OneOrMany` (untagged).
- Whether `!moves {...}` should still load: a definition written out as YAML would use the tagged form.
- Whether an accept-more change to `entity-yaml` counts as the coordinated migration `AGENTS.md` § Boundaries describes.
