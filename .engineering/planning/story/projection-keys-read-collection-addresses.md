---
format: aep.planning-md/3
id: story:projection-keys-read-collection-addresses
kind: story
status: active
title: Projection keys read collection addresses, or refuse them for new definitions only
owner: entity-runtime
relations:
- serves: vision:O2
- informed_by: review-result:er-w1-u1-adversary-pass-2
scope:
- confidence: cited
  path: CHANGELOG.md
- confidence: inferred
  path: crates/entity-core/src/validation.rs
- confidence: cited
  path: crates/entity-core/tests/adversary_text_count.rs
- confidence: cited
  path: crates/entity-core/tests/security_text_count.rs
- confidence: cited
  path: crates/entity-core/tests/service_values.rs
- confidence: cited
  path: crates/entity-store/src/projection.rs
- confidence: inferred
  path: crates/entity-store/tests/projections.rs
- confidence: cited
  path: docs/design/service-semantics-v0.1.md
- confidence: inferred
  path: docs/design/store-v0.1.md
- confidence: inferred
  path: docs/ess/store-traceability.md
- confidence: cited
  path: docs/requirements.md
- confidence: inferred
  path: ess/coverage.json
- confidence: inferred
  path: ess/ess-inputs.yaml
- confidence: inferred
  path: ess/generated/suite.json
- confidence: inferred
  path: ess/scenarios/store/projection-collection-address-keys.yaml
revision: 6
transitions:
- {from: "draft", to: "proposed", at: "2026-10-07T09:15:43Z", actor: "human:timo", revision: 5}
- {from: "proposed", to: "active", at: "2026-10-07T09:15:43Z", actor: "human:timo", revision: 6}
---
# Projection keys read collection addresses, or refuse them for new definitions only

## Outcome

A `service/1` projection keyed on a collection address (`<array>.count`, `<array>.<n>`,
`<map>.count`) either files each instance under the address's value, or is refused for new
registrations while stored definitions that already use it keep replaying.

## Why

Wave 1's adversary and security passes on `story:a-condition-reads-the-length-of-a-text` confirmed a
pre-existing defect: projection keys are checked with the `service/1` rule walk, which admits these
addresses (R-148, since 0.19.0), but `entity_store::project`'s `key_of`
(`crates/entity-store/src/projection.rs:70-81`) walks object members only, so the read model files
no instance. Refusing the forms at registration (wave 1, correction 1) broke replay of stored
histories, because `replay` re-validates each stored definition snapshot
(`crates/entity-core/src/replay.rs:131`); correction 2 reverted it (review-result
`er-w1-u1-adversary-pass-2`). Nothing in this repository or on aep, aep-service, atlas or bench
`origin/main` calls `project` (security pass 1).

## Acceptance

- Either `key_of` resolves the three address forms (tests per form, red before), or registration of
  a **new** definition refuses them while a stored snapshot that holds them still replays to the
  same bytes (the two adversary cases `…_still_replays` stay green).
- The wave-1 case that pins today's "registers and projects nothing" behaviour is rewritten to the
  new behaviour.
- Requirement row and `CHANGELOG.md` line.

## Out of scope

Text length as a projection key (refused since wave 1; no stored record can hold it).

## Scope

Derived 2026-10-07 by `aep:story-scoper` at `cfcba172` (= `origin/main`). Every line is **cited** (read from the story or the tree) or
**inferred** (a reading that could be wrong).

- **Primary surface:** `crates/entity-store`, `key_of` — cited (story: `crates/entity-store/src/projection.rs:70-81`)
- **Option taken:** "`key_of` resolves the three forms" (option A) — inferred. The story leaves it open, but the tree's own text names A: `docs/design/service-semantics-v0.1.md:1736-1740` ("a projection key walk that reads the collection forms") and the test messages at `crates/entity-core/tests/adversary_text_count.rs:106-107` ("should change this case to assert the instance is filed under its count")
- **Files:** `crates/entity-store/src/projection.rs:65-97` `key_of` (walk at :76-94), and `project` at :38-63, which must pass the definition's schema and semantics into `key_of` — cited
- **Files:** `crates/entity-core/tests/adversary_text_count.rs:90-115` `a_projection_keyed_on_an_array_count_registers_as_on_the_base_until_keys_read_collection_addresses`. It names this story, and the acceptance says to rewrite it. Keep `…_still_replays` at :269-305 green — cited
- **Files:** `crates/entity-core/tests/security_text_count.rs:96-122` `a_projection_keyed_on_an_array_count_registers_where_it_is_written_until_keys_read_collection_addresses`. It names this story ("should change this case") — cited
- **Files:** `crates/entity-core/tests/service_values.rs:1820-1873` `a_projection_key_refuses_a_text_length_and_registers_the_base_collection_forms_unchanged`. The row at :1857-1868 names this story. R-161 cites the test by name, so a rename edits `docs/requirements.md` in the same commit — cited
- **Files:** `crates/entity-store/tests/projections.rs`, new cases one per form, red first — inferred. The entity-core tests cannot call `project` because `crates/entity-core/Cargo.toml` has no entity-store dev-dependency and `purity.rs` pins the dependency tables
- **Symbols read, not changed:** `entity_core::runtime::{lookup, walk, collection_address}` (`crates/entity-core/src/runtime.rs:2855-2990`, private) is the semantics `key_of` must reproduce under `service/1`. It needs only public types: `FieldDefinition.kind/items/properties` (`definition.rs:339-398`), `FieldKind` (:517) and `Semantics::has_service_semantics` (:137) — cited
- **Also likely:** `crates/entity-core/src/validation.rs`, comments only, at :164-169 and in the `Reader` docs at :1319-1336. Both say the collection forms "still register and project nothing". The refusal text at :1518-1523 stays, because text length is out of scope — inferred
- **ESS specification:** a new `entity.store.Project` scenario, `ess/scenarios/store/projection-collection-address-keys.yaml` (name inferred), with a `service/1` definition keyed on `$fields.tags.count`, `$fields.tags.0`, `$fields.meta.count`, plus `$fields.stats.count` as an object member. No domain or component edit: `entity.store.Project` takes and returns JSON documents (`ess/domains/store.yaml:130-138`, `ess/components/store.yaml:22`), and the harness passes the definition through unvalidated (`checks/ess-conformance/src/store.rs:426-430`) — inferred
- **ESS specification:** registering it rewrites `ess/ess-inputs.yaml` (near :330-331), `ess/coverage.json` (near :428-429, :865-866) and `ess/generated/suite.json` through `task ess-regenerate -- --coverage-review` — inferred
- **ESS unchanged under A:** `ess/scenarios/core/service1-text-count-projection-key.yaml` and `ess/scenarios/store/projection-scalar-and-nested-keys.yaml` (`kernel/1`; must stay byte-identical) — inferred
- **Documents:** `docs/requirements.md`, a new row after R-163 (:236) and an amendment to R-161 (:234) — cited
- **Documents:** `CHANGELOG.md` `## [Unreleased]` (:5) — cited
- **Documents:** `docs/design/service-semantics-v0.1.md:1733-1740` and :1877 — cited
- **Documents:** `docs/ess/store-traceability.md:60` (the R-98–R-100 row gains the new scenario); `docs/design/store-v0.1.md` §7 (:122-143) — inferred
- **If option B is chosen instead:** the work lands in entity-core, splitting authoring-time from recorded-snapshot validation across `validation.rs` (:164-200), `registry.rs:13-21` (`ValidatedDefinition::new`, a public API consumers pin) and `replay.rs:131`, plus every site that re-validates a stored snapshot (`entity-executor/src/lib.rs:1153`, `entity-shell/src/lib.rs:198`, `entity-store/src/asynchronous/verify.rs:150`) and `ess/scenarios/core/service1-text-count-projection-key.yaml`. None of this is in the typed entries — inferred
- **Confidence:** medium; high if the brief pins option A
- **Would collide with:** under A, the entity-core #54 wave only on `validation.rs` comments, the entity-core test directory, `ess/ess-inputs.yaml`, `ess/coverage.json`, `ess/generated/suite.json`, `docs/requirements.md` (new R-number at the tail), `docs/design/service-semantics-v0.1.md` and `CHANGELOG.md`; under B, head-on on `validation.rs`, `replay.rs`, `registry.rs`, `lib.rs`. The entity-eventlog #55 wave: `CHANGELOG.md` only
- **Safety fact:** under A, `project`'s output is a derived read model nothing records or replays; its callers are `crates/entity-store/tests/projections.rs` and `checks/ess-conformance/src/store.rs:429`, and no consumer (aep, aep-service, atlas, bench, ess at their local `origin/main`) calls it. Level 2, unproven
- **Safety fact (behaviour):** `<map>.count` is not always "no instance" today: `key_of` (:80-82) reads a map member named `count`, so such a map is filed under that member's value now and under its size after A. Level 3 (walked), unproven

### Not established

- Option A or B: the story leaves it open; the informing review recommends B, the in-tree design text and test messages point at A.
- Whether `key_of` must also resolve value-keyed arrays under `json` fields and union payloads (`runtime.rs:2953-2990`; registration admits them untyped, `validation.rs:1537`, :1559-1561).
- Copy the kernel walk into entity-store, or export it from entity-core (a copy risks drift, `validation.rs:1427-1431`).
- "The wave-1 case" (singular) in the acceptance: three cases across three files name this story.
- Whether option B's recorded-mode constructor is additive enough to avoid a coordinated consumer migration.
