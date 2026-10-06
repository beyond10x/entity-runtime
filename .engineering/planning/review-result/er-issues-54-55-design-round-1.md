---
format: aep.planning-md/3
id: review-result:er-issues-54-55-design-round-1
kind: review-result
status: active
title: 'Design critic, round 1: issues 54 and 55 stories'
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
needs-revision
story:recorded-open-verifies-a-checkpoint-and-its-suffix — acceptance 1 restates the design document's content and its independent review, which story:recorded-open-checkpoint-design already owns (its acceptance 1 and 3), so both stories claim one outcome; the body should say "the implementation follows the option the design chose" and drop the restated list, the `depends_on` edge already orders the two — .engineering/planning/story/recorded-open-verifies-a-checkpoint-and-its-suffix.md:65

**What I read:** 8 artifacts (the epic, its 5 stories, and the 2 recorded-open stories), each with `aep plan artifact show`, plus `gh issue view 54` and `gh issue view 55`. I also ran `aep plan artifact relations`, `aep plan artifact graph` and `aep plan artifact validate` (valid, 145 artifacts). I walked 19 edges, including the ones out of the set: `serves vision:O2`, `decomposes initiative:entity-runtime`, and `informed_by` to `story:bounded-batch-and-facade-reads` and `story:seeded-open-under-one-second`. I found no cycle, and the `depends_on` edges are not a serialising chain.

Findings on the shape:
- **Epic set:** it holds. `set-increments` and `operation-writes` are independent roots. `set-clears` depends on both, and each edge has its reason in the body: the typed-assignment decision for increments, and the `set_if_present` conflict rule for operation-writes. The two text stories are independent of each other and of the rest. No story is a horizontal slice or half an abstraction.
- **Recorded-open pair:** design first, then implementation, with an edge recorded. The one overlap is `shared_clock_cost.rs`, which the edge already orders.

**What I could not establish:**
- Out of my lane (acceptance, for `plan-critic-acceptance`): `story:recorded-open-verifies-a-checkpoint-and-its-suffix` acceptance 2 does not say what happens to the five `unchanged-head-*-tamper` scenarios. The design story decides their fate under option (b), but no implementation criterion carries it.
- Out of my lane (parallel safety): `set-increments`, `set-clears` and `operation-writes` edit the same step-8 lines and the same `error.rs` tail. The only recorded order is `set-clears` after the other two; `set-increments` and `operation-writes` have no edge to each other. The `depends_on` edges already serialise the first and last stories.
- Unease, not a finding: no body says how `{increment: n}` combines with a `set_if_present` target on the same field of one outcome, because `set-increments` refuses only creations and optional fields. I could not name the body that should state it.

```findings
- file: .engineering/planning/story/recorded-open-verifies-a-checkpoint-and-its-suffix.md
  line: 65
  category: design
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: acceptance 1 restates the design document's content and its independent review, which story:recorded-open-checkpoint-design already owns (its acceptance 1 and 3), so both stories claim one outcome; the body should say "the implementation follows the option the design chose" and drop the restated list, the depends_on edge already orders the two
```
