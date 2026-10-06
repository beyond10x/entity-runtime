---
format: aep.planning-md/3
id: review-result:er-issues-54-55-scope-round-1
kind: review-result
status: active
title: 'Scope critic, round 1: issues 54 and 55 stories'
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
story:recorded-open-verifies-a-checkpoint-and-its-suffix — acceptance 1 restates the design document's contents and its independent `review-result`, which story:recorded-open-checkpoint-design acceptance already owns, so both stories claim one outcome and both would be marked done on it. Reduce it to a dependency gate naming the design story — .engineering/planning/story/recorded-open-verifies-a-checkpoint-and-its-suffix.md:65

**What I read:** 8 artifacts, each in full: the epic, its five stories, the two recorded-open stories, plus issues #54 and #55. I ran `aep plan artifact show` on the epic and four of the stories, `cat -n` on the story files, `gh issue view 54` and `55`, and `aep plan artifact graph` and `relations`.
- **Epic coverage:** I extracted 9 promises from the epic and issue #54 and traced 9 to an item. The 9 are the five constructs, spec-first scenarios, requirement rows plus CHANGELOG, the invariants, and the single release. The single release is not claimed by any story (see below).
- **Issue #55 coverage:** I extracted 5 promises and traced 5. The 5 are the bounded open, integrity kept, the checkpoint shape, full verification on demand, and raw-edit treatment. A sixth item, the consumer's own growth reduction, is excluded in the stories' `## Out of scope`.

**What I could not establish:**
- The epic promises "One Entity Runtime release carries all five" (epic file, line 50). No story claims it. The store keeps per-release tasks (`task/release-0-26-0.md`), so I took it as a later release task and not a gap. I did not have the drafter's report to confirm.
- Option (b) in the design story narrows the five reviewed tamper scenarios' contract, while #55 says "with integrity kept". It is offered as an option for the design to weigh, not decided, so I did not raise it.
- Whether each acceptance is checkable ("full verification stays available on demand" appears only in the Outcome) belongs to `plan-critic-acceptance`. I did not weigh it into the verdict.

```findings
- file: .engineering/planning/story/recorded-open-verifies-a-checkpoint-and-its-suffix.md
  line: 65
  category: scope
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: acceptance 1 restates the design document's contents and its independent review-result, which story:recorded-open-checkpoint-design acceptance already owns, so both stories claim one outcome; reduce it to a dependency gate naming the design story
```
