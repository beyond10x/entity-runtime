---
format: aep.planning-md/3
id: story:service-1-moves-outcome-loads-from-yaml
kind: story
status: draft
title: A service/1 outcome with a moves effect loads through entity-yaml and the entity command
owner: entity-runtime
relations:
- serves: vision:O2
revision: 1
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
