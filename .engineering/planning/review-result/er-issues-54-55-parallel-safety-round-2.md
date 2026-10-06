---
format: aep.planning-md/3
id: review-result:er-issues-54-55-parallel-safety-round-2
kind: review-result
status: active
title: 'Parallel-safety critic, round 2: issues 54 and 55 stories'
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
approve

**What I read:** 7 artifacts, via `aep plan artifact waves --kind story --status draft --format json` (waves and collisions), `aep plan artifact show` on each story, `aep plan artifact graph`, `aep plan artifact show review-result:er-issues-54-55-parallel-safety-round-1`, and `git grep` on `crates/entity-core/src/validation.rs` for the range claims.

The computed waves hold. No two stories in one wave share a scope path, and the two same-wave pairs are file-disjoint:

| Wave | Stories |
|---|---|
| 1 | `story:a-condition-reads-the-length-of-a-text`, `story:recorded-open-checkpoint-design` |
| 2 | `story:a-text-field-declares-its-alphabet`, `story:recorded-open-verifies-a-checkpoint-and-its-suffix` |
| 3 | `story:operation-writes-an-optional-field-from-an-optional-argument` |
| 4 | `story:set-increments-a-numeric-field` |
| 5 | `story:set-clears-an-optional-field` |

So the set cannot all be worked at once; it is worked as five waves. The recorded `depends_on` edges are `set-clears` on `set-increments` and on `operation-writes`, and `verifies` on `checkpoint-design`.

Both round-1 blockers are fixed. `story:a-text-field-declares-its-alphabet` and `story:recorded-open-verifies-a-checkpoint-and-its-suffix` now each carry a "Coordinator-owned, not unit scope" line naming `CHANGELOG.md` and `docs/requirements.md`.

Surfaces established: 7 cited (each has a cited primary path), 0 inferred-only, 0 unplaced. Secondary files on several stories are inferred and marked so in their bodies.

**What I could not establish:**
- `story:a-condition-reads-the-length-of-a-text` says "no sibling" touches `validate_reference_path` or `walk_field_path`. `set-increments` plans a helper that returns a reference's declared type, which may reach `walk_field_path` (`validation.rs:1392`, `:1418`). It is inferred and does not change the verdict, because the waves already put them in waves 1 and 4.
- Only `set-clears` has recorded edges among the five core stories. The order of the other four comes from the waves computation, not from recorded edges. Their same-file collisions on `runtime.rs`, `validation.rs`, `definition.rs` and `error.rs` are admitted in each body with disjoint line ranges. The waves enforce the order, so I filed nothing.
- Out of my lane (scope): `verifies` lists `docs/design/recorded-open-checkpoint-v0.1.md` as "(new)" under Documents, but `recorded-open-checkpoint-design` creates it. The `depends_on` edge orders them, so nothing runs concurrently.
- Outside the set: `story:seeded-open-under-one-second` has no scope, so `waves` lists it as unassessed. The `verifies` body says to run the two one after the other, and I did not assess that.
- Line numbers: `git rev-parse --short HEAD origin/main` failed in the working checkout (detached `HEAD`), so I did not confirm HEAD is `7926ec45`. `walk_field_path` is at `validation.rs:1418` in this tree against 1417 in the story body, a one-line drift that changes no collision.

```findings
[]
```
