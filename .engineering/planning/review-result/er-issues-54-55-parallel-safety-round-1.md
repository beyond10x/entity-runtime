---
format: aep.planning-md/3
id: review-result:er-issues-54-55-parallel-safety-round-1
kind: review-result
status: active
title: 'Parallel-safety critic, round 1: issues 54 and 55 stories'
relations:
- reviews: story:set-increments-a-numeric-field
- reviews: story:set-clears-an-optional-field
- reviews: story:operation-writes-an-optional-field-from-an-optional-argument
- reviews: story:a-condition-reads-the-length-of-a-text
- reviews: story:a-text-field-declares-its-alphabet
- reviews: story:recorded-open-checkpoint-design
- reviews: story:recorded-open-verifies-a-checkpoint-and-its-suffix
revision: 1
---
needs-revision
story:recorded-open-verifies-a-checkpoint-and-its-suffix — `CHANGELOG.md` and `docs/requirements.md` (R-151, line 224) are edited by this story and by `story:a-text-field-declares-its-alphabet`, which `waves` puts in the same wave 2 (cited, both bodies); neither body says so, and the verifies story has no coordinator-owned line, so the computed waves cannot see the collision. Remedy A: declare both files coordinator-owned here, as `set-increments` does. Remedy B: record an ordering edge naming them. — .engineering/planning/story/recorded-open-verifies-a-checkpoint-and-its-suffix.md:109
story:a-text-field-declares-its-alphabet — the set's coordinator-owned premise does not hold for this story. It lists `ess/ess-inputs.yaml`, `ess/coverage.json`, `ess/generated/suite.json`, `docs/requirements.md` and `CHANGELOG.md` as its own files and carries no coordinator-owned line. Its sibling `story:recorded-open-verifies-a-checkpoint-and-its-suffix` also edits `CHANGELOG.md` and `docs/requirements.md` in the same wave. The body must either declare these coordinator-owned or name that pair as a collision. — .engineering/planning/story/a-text-field-declares-its-alphabet.md:88

**What I read:** 7 artifacts, via `aep plan artifact waves --kind story --status draft --format json`, `aep plan artifact show` on each, `aep plan artifact graph` (edges), and the on-disk story files for coordinator-ownership grep. In the 7 stories' typed scopes, all 7 have a cited primary surface (4 also have inferred secondary files) and 0 are unplaceable.

**What I could not establish:**
- `story:a-condition-reads-the-length-of-a-text`, `story:operation-writes-an-optional-field-from-an-optional-argument` and `story:recorded-open-verifies-a-checkpoint-and-its-suffix` also carry no coordinator-owned line, which contradicts the brief. Only `set-increments` (line 98), `set-clears` (line 91) and the design story (line 75) have one. Their `CHANGELOG.md`, `docs/requirements.md`, `ess/coverage.json` and `ess/generated/suite.json` collisions are sequenced by wave anyway, so I did not file them separately. Only the two bodies above share a wave.
- Several of the five core stories collide at file level on `validation.rs`, `definition.rs`, `runtime.rs` and `error.rs`, and on `docs/design/kernel-v0.1.md`. Most bodies name those collisions with disjoint line ranges. Only `set-clears` has `depends_on` edges, to `set-increments` and `operation-writes`. The 1-to-5 wave order of the other core stories is a computed tie-break, not recorded edges.
- Out of my lane: `a-condition-reads-the-length-of-a-text` requires "a named `DefinitionError`" in its acceptance but lists no `error.rs`. That is an acceptance and scope question. It does not change the wave order, because `validation.rs` already serialises it.
- `story:seeded-open-under-one-second` is outside the set. The verifies story says to run it one after the other. I cannot assess it, and `waves` lists it as unassessed.
- I did not check whether `ess/provider-tracking/ess-inputs.yaml` or `coverage.json` overlap the core `ess/` files. They are separate directories, so I treated them as disjoint.

```findings
- file: .engineering/planning/story/recorded-open-verifies-a-checkpoint-and-its-suffix.md
  line: 109
  category: parallel-safety
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: CHANGELOG.md and docs/requirements.md (R-151, line 224) are edited by this story and by story:a-text-field-declares-its-alphabet, which waves puts in the same wave 2 (cited, both bodies); neither body says so, and this story has no coordinator-owned line, so the computed waves cannot see it. Remedy A is to declare both files coordinator-owned; remedy B is an ordering edge naming them.
- file: .engineering/planning/story/a-text-field-declares-its-alphabet.md
  line: 88
  category: parallel-safety
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: The coordinator-owned premise does not hold for this story. It lists ess/ess-inputs.yaml, ess/coverage.json, ess/generated/suite.json, docs/requirements.md and CHANGELOG.md as its own files with no coordinator-owned line, and its same-wave sibling story:recorded-open-verifies-a-checkpoint-and-its-suffix also edits CHANGELOG.md and docs/requirements.md. The body must declare these coordinator-owned or name that pair as a collision.
```
