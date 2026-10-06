---
format: aep.planning-md/3
id: review-result:er-issues-54-55-design-round-2
kind: review-result
status: active
title: 'Design critic, round 2: issues 54 and 55 stories'
relations:
- reviews: story:set-increments-a-numeric-field
- reviews: story:set-clears-an-optional-field
- reviews: story:operation-writes-an-optional-field-from-an-optional-argument
- reviews: story:a-condition-reads-the-length-of-a-text
- reviews: story:a-text-field-declares-its-alphabet
- reviews: story:recorded-open-checkpoint-design
- reviews: story:recorded-open-verifies-a-checkpoint-and-its-suffix
- reviews: epic:ess-lowering-entity-core-features
revision: 1
---
approve

**What I read:** 8 artifacts (the epic, its 5 stories, and the 2 recorded-open stories). I ran `aep plan artifact show` on each, `aep plan artifact relations`, `aep plan artifact graph` and `aep plan artifact validate` (valid, 149 artifacts). I also read the round-1 design `review-result` and `crates/entity-core/src/validation.rs:625-700` and `:829-861`. I walked 20 non-review edges, including those leaving the set: `serves vision:O2`, `decomposes initiative:entity-runtime`, and `informed_by` to `story:bounded-batch-and-facade-reads` and `story:seeded-open-under-one-second`. I found no cycle. The `depends_on` edges are `set-clears` after `set-increments` and after `operation-writes`, and the recorded-open implementation after the design. That is not a serialising chain.

The round-1 finding is fixed. Acceptance 1 of `story:recorded-open-verifies-a-checkpoint-and-its-suffix` (`.engineering/planning/story/recorded-open-verifies-a-checkpoint-and-its-suffix.md:65`) is now a gate: "implements the option `story:recorded-open-checkpoint-design` chose". The restated design content is gone, so the design story owns the decision and the implementation story owns carrying it out.

Shape notes:
- **Epic set:** `set-increments` decides the typed-assignment form and `set-clears` declares it reuses that decision, so the edge matches a stated reason. The `set_if_present` conflict rule is owned once, by `set-clears`, and `operation-writes` defers to it by name. The two text stories are independent of the rest and of each other. No story is a horizontal slice.
- **Recorded-open pair:** the design story ends in a document, a measured baseline and a recorded decision, and the implementation story consumes both. The `depends_on` edge records the order. The conditional `blocks` edge on the Eventlog option is stated in the design story's acceptance.

**What I could not establish:**
- Unease, not a finding: whether `{increment: n}` conflicts with a `set_if_present` target on one outcome. The existing `ConditionalTargetConflict` check (`validation.rs:652-658`) refuses a `set` key that is also a `set_if_present` key, so it covers this only if the typed assignment stays inside the `set` map. That is `set-increments`' open decision, so no body can be named yet.
- Out of my lane (parallel safety): `set-increments`, `set-clears` and `operation-writes` edit the same step-8 lines in `runtime.rs` and the same `error.rs` tail. `set-increments` and `operation-writes` have no edge between them.
- Out of my lane (acceptance): round 1 raised that the five `unchanged-head-*-tamper` scenarios have no implementation-side criterion. Implementation acceptance 3 now covers them, so I did not re-raise it.

```findings
[]
```
