---
format: aep.planning-md/3
id: story:schema-fragments
kind: story
status: draft
title: Reusable schema fragments and definition inheritance
summary: Authoring convenience for large definition sets; nothing in the kernel's semantics changes.
relations:
- derived_from: epic:kernel
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
revision: 3
---
# Story: Reusable schema fragments and definition inheritance

## Outcome

Large definition sets share field groups (`audit_fields`, `money`) and base definitions without
copy-paste.

## Acceptance

`$ref`-style fragments resolved by the YAML adapter before the definition reaches the kernel, so the
kernel's model is unchanged; a test that two definitions sharing a fragment validate identically to
their expanded forms.
