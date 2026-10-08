---
format: aep.planning-md/3
id: story:entity-yaml-and-entity-surface-are-specified
kind: story
status: draft
title: entity-yaml and entity-surface are specified in ESS
relations:
- serves: vision:O2
- informed_by: decision-blocker:defects-outside-ess-scope
scope:
- confidence: cited
  path: AGENTS.md
- confidence: inferred
  path: checks/ess-conformance
- confidence: inferred
  path: docs/ess
- confidence: inferred
  path: ess/components
- confidence: inferred
  path: ess/coverage.json
- confidence: inferred
  path: ess/domains
- confidence: cited
  path: ess/ess-inputs.yaml
- confidence: inferred
  path: ess/generated
- confidence: inferred
  path: ess/scenarios
revision: 3
---
# entity-yaml and entity-surface are specified in ESS

## Outcome

The definition-loading surface of `entity-yaml` (`from_str`: which YAML spellings load, which are
refused, duplicate keys) and the contract generation of `entity-surface` (`documentation`: the
OpenAPI and AsyncAPI documents and their schemas) are part of the ESS composition in
`ess/ess-inputs.yaml`, with scenarios a conformance run executes, so later changes to either crate
start in the specification.

## Why

Both crates ship behaviour no specification holds (`AGENTS.md` § Which documents are normative
names five libraries). Two defects in them were fixed with tests only for that reason:
`story:service-1-moves-outcome-loads-from-yaml` and `story:generated-yaml-contracts-write-numbers`.

## Acceptance

- An ESS domain and component for each crate, derived with `ess:retrofitting` from the code and
  tests, every declaration citing its source; `ess specify validate --path ess` with the newest
  `ess` ends `valid`.
- Named scenarios cover the two fixed defects and the refusals the crates give today.
- `task ess-check` runs them; `AGENTS.md` § Which documents are normative names the two crates.

## Out of scope

Behaviour changes to either crate.
