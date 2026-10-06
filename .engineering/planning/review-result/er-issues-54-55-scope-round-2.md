---
format: aep.planning-md/3
id: review-result:er-issues-54-55-scope-round-2
kind: review-result
status: active
title: 'Scope critic, round 2: issues 54 and 55 stories'
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

No findings. Everything the epic promises is claimed once, and no item reaches past issues #54 and #55.

**What I read:** 8 artifacts in full: the epic, its five stories and the two recorded-open stories. I also read GitHub issues #54 and #55 and the round-1 scope review.
- **Commands run:** `aep plan artifact show` on each artifact, `gh issue view 54` and `55`, `aep plan artifact graph`, and `aep plan artifact show review-result:er-issues-54-55-scope-round-1`.
- **Epic and #54:** I extracted 9 promises and traced 9 to an item. They are the five constructs, spec-first scenarios, requirement rows plus CHANGELOG, the invariants, and the single release. The release is the one promise no story claims; it is epic-level and each story's Out of scope leaves it there.
- **#55:** I extracted 6 promises and traced 6 to an item. They are:
  - the bounded open (impl AC cost/Outcome)
  - integrity kept (design AC; impl AC3)
  - the checkpoint shape (design AC; impl Outcome)
  - full verification on demand (impl AC4)
  - the raw-edit treatment (the design's "what a bounded open does not detect" plus the five tamper scenarios in impl AC3)
  - the baseline measurement (design AC)
- **Round-1 finding:** it is fixed. Acceptance 1 of `story:recorded-open-verifies-a-checkpoint-and-its-suffix` is now only a gate on the design story, so the design document and its review are no longer claimed twice.

**What I could not establish:**
- "One Entity Runtime release carries all five" has no owning story. I read it as a later release task, as in round 1, and did not see the drafter's report to confirm.
- #54 says Optional-to-Optional assignment "leaves the field unset when the input is absent". `story:operation-writes-an-optional-field-from-an-optional-argument` leaves the field unchanged, so a present field stays present. It puts clearing on absence out of scope. I read #54's wording as "does not set" and did not treat it as a narrowing; if the operator meant "unset", the story would need to say so.
- Whether each acceptance is checkable, whether the set holds together, and whether stories can run in parallel belong to other critics. I did not weigh them.

```findings
[]
```
