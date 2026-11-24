---
format: aep.planning-md/3
id: story:definition-floats-exact-in-fragments-and-tags
kind: story
status: implemented
title: Definition bounds keep their written digits in $defs/$ref documents and tagged nodes
summary: 'E31 remainder: the fragment resolver and tagged nodes still round a bound to binary64'
owner: entity-runtime
relations:
- serves: vision:O2
revision: 4
transitions:
- {from: "draft", to: "proposed", at: "2026-11-24T03:20:02Z", actor: "human:timo", revision: 2, decided_on: {"recorded":{"test_result":1}}}
- {from: "proposed", to: "active", at: "2026-11-24T03:20:02Z", actor: "human:timo", revision: 3, decided_on: {"recorded":{"test_result":1}}}
- {from: "active", to: "implemented", at: "2026-11-24T03:20:03Z", actor: "human:timo", revision: 4, decided_on: {"recorded":{"test_result":1}}}
---
## Outcome

A definition bound written with more digits than a binary64 holds is read at its written value in
every document shape the definition reader accepts, including a document with a root `$defs` or
`$ref` and a document with a tagged node, so the kernel's exact comparison runs on the bound the
author wrote.

## Finding

Remainder of review E31 (Entity Runtime 0.32.0, round 5): "Definition loading can round a numeric
bound and accept an invalid value." The source minimum 1.0000000000000001 was read as 1.0 and
`rate = 1` was accepted.

## Evidence

- `6f9bbede` (fix/review-er-h, merged in #93) reads float scalars at their written value through a
  second, exact reading (`crates/entity-yaml/src/exact.rs`). Its commit body: "Not covered: a
  document with a root $defs or $ref is still read through the fragment resolver, which rounds as
  before; a document with a tagged node keeps its old reading."
- Master log `assignment.md` (er-h DONE): "E31 gap: root $defs/$ref docs and tagged nodes still
  round (note for integration)".
- The early return: `crates/entity-yaml/src/lib.rs:78-79` (`$defs`/`$ref` ->
  `fragments::parse(input)`), before the exact reading at lines 94-95.
- Tagged nodes: `crates/entity-yaml/src/exact.rs:72` refuses the exact reading ("a tagged node is
  read as written"), so the binary64 reading stands.
- Review evidence: `round5/runtime/precision.yaml:5`, `precision-create.json:9`,
  `precision-json-inspect.json:11`.

## Acceptance

For a definition whose root `$defs` fragment declares `minimum: 1.0000000000000001`, and for one
that carries the same bound inside a tagged node, in YAML and in JSON syntax, `entity inspect`
shows the bound as written and a create with `rate = 1` is refused, while a definition already
read exactly (0.50, 0.1, 1e3) keeps its digest.
