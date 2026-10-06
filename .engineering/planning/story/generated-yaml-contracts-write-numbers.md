---
format: aep.planning-md/3
id: story:generated-yaml-contracts-write-numbers
kind: story
status: draft
title: Generated YAML contracts write numbers as numbers
owner: entity-runtime
relations:
- serves: vision:O2
revision: 1
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
