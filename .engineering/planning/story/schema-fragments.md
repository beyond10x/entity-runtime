---
format: aep.planning-md/3
id: story:schema-fragments
kind: story
status: active
title: Reusable schema fragments and definition inheritance
summary: Authoring convenience for large definition sets; nothing in the kernel's semantics changes.
relations:
- derived_from: epic:kernel
- serves: vision:O2
scope:
- confidence: inferred
  path: CHANGELOG.md
- confidence: inferred
  path: crates/entity-cli/src/main.rs
- confidence: inferred
  path: crates/entity-yaml/src/lib.rs
- confidence: inferred
  path: crates/entity-yaml/tests/fragments.rs
- confidence: inferred
  path: docs/requirements.md
revision: 6
transitions:
- {from: "draft", to: "proposed", at: "2026-10-31T03:17:14Z", actor: "human:timo", revision: 5}
- {from: "proposed", to: "active", at: "2026-10-31T03:57:38Z", actor: "human:unknown", revision: 6, executor: "agent:takt"}
---
# Story: Reusable schema fragments and definition inheritance

## Outcome

Large definition sets share field groups (`audit_fields`, `money`) and base definitions without
copy-paste.

## Acceptance

`$ref`-style fragments resolved by the YAML adapter before the definition reaches the kernel, so the
kernel's model is unchanged; a test that two definitions sharing a fragment validate identically to
their expanded forms.

Fragments are not YAML merge keys: merge keys (`<<`) and duplicate keys stay refused, as
`crates/entity-yaml/tests/runtime.rs:273-287` and `crates/entity-yaml/tests/service_numbers.rs:133-145`
assert, and those tests do not change.

Claimed: takt pod run (gpt-6-astra), 2026-10-31
