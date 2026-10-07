---
format: aep.planning-md/3
id: story:aep-lifecycle-fixture-matches-aep-main
kind: story
status: implemented
title: The AEP lifecycle fixture matches current AEP main
owner: entity-runtime
relations:
- serves: vision:O2
scope:
- confidence: cited
  path: crates/entity-yaml/tests/aep_lifecycles.rs
- confidence: cited
  path: crates/entity-yaml/tests/fixtures/aep-lifecycles
- confidence: cited
  path: examples/aep/executable-system-specification.yaml
revision: 7
transitions:
- {from: "draft", to: "proposed", at: "2026-10-07T00:02:37Z", actor: "human:timo", revision: 5}
- {from: "proposed", to: "active", at: "2026-10-07T00:02:37Z", actor: "human:timo", revision: 6}
- {from: "active", to: "implemented", at: "2026-10-07T00:09:22Z", actor: "human:timo", revision: 7, decided_on: {"recorded":{"test_result":1,"verification":1}}}
---
# The AEP lifecycle fixture matches current AEP main

## Outcome

`crates/entity-yaml/tests/fixtures/aep-lifecycles/` holds the lifecycle documents of
`github.com/beyond10x/aep` at its current `main`, `PIN.md` records that commit and the new sums,
and `examples/aep/` still declares what the refreshed ladders declare.

## Why

Atlas's consumer compatibility run 37325042164 reports 13 pinned and 13 upstream ladders with
7 findings: pinned at `35b5c99`, upstream at `5a2a0e5`. The ladders that differ are `epic`,
`executable-system-specification`, `initiative`, `outbound-claim`, `story`, `task` and `vision`.
Six differ in comments only (`protocol artifact` became `aep plan artifact`). In
`executable-system-specification.yaml` the `conforming` rung now accepts
`ess_conformance_v2` or `ess_conformance_coverage_v1` as alternatives to `ess_conformance`
(`or:` beside `evidence:`). Upstream also gained `artifacts/lifecycles/.gitkeep`, which is not a
lifecycle document.

## Acceptance

- The 13 `*.yaml` files equal AEP `5a2a0e5f3f498e70d7e86f59e2b4693292c32248`
  `artifacts/lifecycles/` byte for byte; `PIN.md` names that commit, today's date and 13 new sums;
  `task pin-check` passes.
- `examples/aep/executable-system-specification.yaml`'s `conform` operation admits any one of the
  three evidence kinds, and the equivalence test checks a rung's `or:` alternatives as well as its
  `evidence:` kind, so dropping one alternative from the example fails it.
- `cargo test -p entity-yaml` passes, and `entity validate examples/aep/*.yaml` passes.

## Out of scope

The other ladders' semantics, which did not change.
