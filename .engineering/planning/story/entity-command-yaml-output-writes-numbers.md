---
format: aep.planning-md/3
id: story:entity-command-yaml-output-writes-numbers
kind: story
status: draft
title: The entity command writes numbers as numbers in its YAML output
relations:
- serves: vision:O2
- informed_by: story:generated-yaml-contracts-write-numbers
revision: 1
---
# The entity command writes numbers as numbers in its YAML output

## Outcome

`entity inspect --format yaml`, the decision and commit output with `--format yaml`, and the YAML a
generated Rust CLI prints write every number as a plain YAML number with the digits the JSON output
uses, as `entity generate docs` does since `story:generated-yaml-contracts-write-numbers`.

## Why

Found while fixing that story: `entity inspect examples/refund.yaml --format yaml` prints the
`arbitrary_precision` private map four times (`to_yaml`, `crates/entity-cli/src/main.rs:2026`, used
at :209, :1823, :1855; the generated CLI template at :1408 does the same). The contract helper in
`entity-surface` takes a `serde_json::Value`, so typed output would first become one, which may
reorder keys; and the generated CLI cannot reach `entity-surface` without a new dependency.

## Acceptance

- A test reads each `--format yaml` output back through `serde_yaml_ng::Value` and finds every
  number the `--format json` output holds, as a number with the same digits; red before the fix.
- Key order of the YAML output is unchanged for `kernel/1` definitions, or the change is named in
  `CHANGELOG.md`.
- No new dependency in a generated CLI, or the dependency is justified in its manifest.
