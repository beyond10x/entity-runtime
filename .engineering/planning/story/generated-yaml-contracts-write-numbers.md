---
format: aep.planning-md/3
id: story:generated-yaml-contracts-write-numbers
kind: story
status: implemented
title: Generated YAML contracts write numbers as numbers
owner: entity-runtime
relations:
- serves: vision:O2
scope:
- confidence: cited
  path: CHANGELOG.md
- confidence: inferred
  path: crates/entity-cli/tests/cli.rs
- confidence: cited
  path: crates/entity-surface/src/lib.rs
- confidence: inferred
  path: docs/requirements.md
revision: 7
transitions:
- {from: "draft", to: "proposed", at: "2026-10-07T09:15:42Z", actor: "human:timo", revision: 5}
- {from: "proposed", to: "active", at: "2026-10-07T09:15:42Z", actor: "human:timo", revision: 6}
- {from: "active", to: "implemented", at: "2026-10-07T13:09:33Z", actor: "human:timo", revision: 7, decided_on: {"recorded":{"test_result":1,"verification":1}}}
---
# Generated YAML contracts write numbers as numbers

## Outcome

`entity generate docs` writes every number in its YAML contracts (`openapi.yaml`, `asyncapi.yaml`)
as a YAML number, as the JSON contracts already do.

## Why

Found by the 2026-10-06 documentation overhaul on an `entity` 0.27.0 build (not re-run by the
coordinator): the refund example's generated `openapi.yaml` holds `$serde_json::private::Number:
'1'` in 24 places and its `asyncapi.yaml` in 14, where numbers belong; the JSON files are clean.
Inferred, not checked: `serde_json`'s `arbitrary_precision` feature (`Cargo.toml:31` notes it)
represents a number as a private map that `serde_yaml_ng` serializes literally. Earlier releases
were not checked.

## Acceptance

- A test generates the refund example's YAML contracts and asserts that no
  `$serde_json::private::Number` key appears and that each number parses back as the same number;
  red before the fix.
- The generated contracts under the site's examples are regenerated.
- `CHANGELOG.md` line.

## Scope

Derived 2026-10-07 by `aep:story-scoper` at `cfcba172` (= `origin/main`). Every line is **cited** (read from the story or the tree) or **inferred** (a reading that could be wrong).

- **Primary surface:** `crates/entity-surface`. `entity generate docs` calls `entity_surface::documentation` (`crates/entity-cli/src/main.rs:410`) — cited
- **Files:** `crates/entity-surface/src/lib.rs:267-307` `documentation`. The only two YAML writes are `serde_yaml_ng::to_string(&openapi)` at :281 and `(&asyncapi)` at :289. The JSON files go through `pretty_json` (:998-1003) — cited
- **Where numbers enter:** every `serde_json::Number` in the contract: `json!` literals (:70, :120, :427-455, :498), `field_schema` lengths, `minimum` and `maximum` (:389-405) and `with_default` (:409) — cited
- **Test:** extend `generated_documentation_is_complete_and_replaces_only_generator_owned_output` (`crates/entity-cli/tests/cli.rs:1447-1516`, which already generates the refund example's bundle) or add a test in `entity-surface` (`mod tests`, lib.rs:1007+) reading `examples/refund.yaml` — inferred
- **Also likely:** `docs/requirements.md:198` (R-116) cites the new test — inferred
- **ESS specification:** none. `ess/ess-inputs.yaml` composes the core, store, executor, shell, query, service and recording domains; `entity-surface` and `entity-cli` are outside the declared ESS scope (`AGENTS.md` § Which documents are normative), so nothing under `ess/` expresses this change — cited
- **Documents:** `CHANGELOG.md:5` — cited. `website/docs/guides/generate-entity-docs.md` and `website/docs/concepts/service-semantics.md:249-257` show no numeric YAML — cited
- **Public API:** `documentation`'s signature can stay; its dependents are `entity-cli` and `entity-mcp`. `entity-yaml` and `entity-shell` are untouched, so no consumer migration — inferred
- **Not in this story, same defect:** `entity-cli` `to_yaml` (`main.rs:2016-2019`) used by `inspect` (:209), decision (:1823) and commit (:1855) `--format yaml`, and the generated rust-cli template (:1408) — inferred, not run
- **Confidence:** high for where the fix goes; medium overall (test placement and acceptance item 2 open)
- **Would collide with:** `CHANGELOG.md`, `docs/requirements.md` and possibly `crates/entity-cli/tests/cli.rs`; no file overlap with the entity-core #54 or Eventlog #55 waves — inferred
- **Safety fact:** the change can stay at the serialization point. `arbitrary_precision` (`Cargo.toml:32`) must stay workspace-wide because kernel exact-number tokens rely on it (`docs/design/service-semantics-v0.1.md:1276`). Level 2, unproven

### Not established

- "The generated contracts under the site's examples are regenerated": no `openapi.*` or `asyncapi.*` file is committed in this tree (`git ls-files`).
- Very large or very precise numbers: the YAML library's number type holds only i64, u64 or f64; whether the fix writes the exact original digits is not stated.
- The `arbitrary_precision` cause is the story's inference and was not reproduced.
- Spec-first: no ESS domain covers `entity-surface`; either retrofit one or record that this crate is outside the declared ESS scope.
- Whether the `entity-cli` `--format yaml` paths join this story.
