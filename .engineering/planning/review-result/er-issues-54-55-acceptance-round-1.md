---
format: aep.planning-md/3
id: review-result:er-issues-54-55-acceptance-round-1
kind: review-result
status: active
title: 'Acceptance critic, round 1: issues 54 and 55 stories'
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
story:a-text-field-declares-its-alphabet — the acceptance asks for a composed-versus-decomposed `é` scenario but states no expected verdict for either form, so any outcome passes — .engineering/planning/story/a-text-field-declares-its-alphabet.md:50
story:a-text-field-declares-its-alphabet — the Outcome promises an error that "names the path and the first offending character", but no acceptance bullet asserts the character (the first bullet says only "refused with its path"), so that promise can never be checked — .engineering/planning/story/a-text-field-declares-its-alphabet.md:48
story:recorded-open-verifies-a-checkpoint-and-its-suffix — item 5 ends the unit at "the unit stops there" when an Eventlog change is needed and names neither the story's resulting state nor which of items 1–4 then count as met, so the story can be neither done nor not done — .engineering/planning/story/recorded-open-verifies-a-checkpoint-and-its-suffix.md:83
story:recorded-open-verifies-a-checkpoint-and-its-suffix — the acceptance never says the five existing `unchanged-head-*-tamper` scenarios still pass or are narrowed, although a suffix-only open breaks them (safety fact), so the story could close while `provider-ess-check` fails on them — .engineering/planning/story/recorded-open-verifies-a-checkpoint-and-its-suffix.md:71
story:recorded-open-checkpoint-design — "its output is recorded as the baseline the implementation story compares against" names no artifact or location, so the implementation story's "Baseline and treatment outputs are both recorded" has nothing to compare against — .engineering/planning/story/recorded-open-checkpoint-design.md:57

What I read: 8 of 8 ids given (the epic and the 7 stories). I ran `aep plan artifact show <id>` for each, plus `aep plan artifact kinds` and `aep plan artifact lifecycle story`. I checked line numbers against the store files with `sed` and `git grep`.

What I could not establish:
- Whether `provider-ess-check` already runs all existing provider-tracking scenarios, which would make the second finding on the implementation story partly covered. I did not run it, since this pass is read-only.
- Out of my lane, not counted in the verdict: `story:operation-writes-an-optional-field-from-an-optional-argument` carries no `depends_on` edge to the two `set:` siblings, although its scope lists them under "Would collide with". That is a question for the design and parallel-safety critics.

```findings
- file: .engineering/planning/story/a-text-field-declares-its-alphabet.md
  line: 50
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: the acceptance asks for a composed-versus-decomposed `é` scenario but states no expected verdict for either form, so any outcome passes
- file: .engineering/planning/story/a-text-field-declares-its-alphabet.md
  line: 48
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: the Outcome promises an error that names the path and the first offending character, but no acceptance bullet asserts the character, so that promise can never be checked
- file: .engineering/planning/story/recorded-open-verifies-a-checkpoint-and-its-suffix.md
  line: 83
  category: acceptance
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: item 5 ends the unit at "the unit stops there" when an Eventlog change is needed and names neither the story's resulting state nor which of items 1-4 then count as met, so the story can be neither done nor not done
- file: .engineering/planning/story/recorded-open-verifies-a-checkpoint-and-its-suffix.md
  line: 71
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: the acceptance never says the five existing unchanged-head-*-tamper scenarios still pass or are narrowed, although a suffix-only open breaks them, so the story could close while provider-ess-check fails on them
- file: .engineering/planning/story/recorded-open-checkpoint-design.md
  line: 57
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: its output is recorded as the baseline the implementation story compares against, but the acceptance names no artifact or location for that baseline
```
