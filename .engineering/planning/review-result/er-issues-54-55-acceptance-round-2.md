---
format: aep.planning-md/3
id: review-result:er-issues-54-55-acceptance-round-2
kind: review-result
status: active
title: 'Acceptance critic, round 2: issues 54 and 55 stories'
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
story:a-condition-reads-the-length-of-a-text — the Outcome claims text length is "usable in every predicate position where `count` of an array or a map is usable today", but the only position the acceptance exercises is a precondition ("a precondition refuses a text one scalar value over the bound"), so an invariant or an outcome guard could fail and the story would still close — .engineering/planning/story/a-condition-reads-the-length-of-a-text.md:33 against :48
story:a-text-field-declares-its-alphabet — the Outcome and title say "field or argument", but no acceptance item names an argument case (the scenarios and the Rust test do not say field or argument), so an argument-only gap would still pass; the coverage claim sits only in the Scope note "Arguments are covered by the same path" — .engineering/planning/story/a-text-field-declares-its-alphabet.md:31 against :48
story:operation-writes-an-optional-field-from-an-optional-argument — "Every definition admitted before this change decides the same bytes after it" names no check, unlike its siblings' "shown by a test"; the only support is the argument "no admitted definition can reach the new branch", which is a claim, not an observable — .engineering/planning/story/operation-writes-an-optional-field-from-an-optional-argument.md:76
story:recorded-open-verifies-a-checkpoint-and-its-suffix — acceptance 7 requires "`AGENTS.md`'s description of the open updated", but `grep -n -i -E "verif|ProviderTracked|checkpoint" AGENTS.md` finds no passage describing the open's verification, so there is no before state to change and nobody can tell it is done; name the passage, or the line to add, and where — .engineering/planning/story/recorded-open-verifies-a-checkpoint-and-its-suffix.md:90 (the story's own "Not established" at :700 of `aep plan artifact show` says the passage is unknown)

**What I read:** 8 of 8 ids (the epic, the five lowering stories and the two #55 stories), each with `aep plan artifact show <id>`; `gh issue view 54 -R beyond10x/entity-runtime` and `gh issue view 55 -R beyond10x/entity-runtime`; `aep plan artifact kinds`; `aep plan artifact lifecycle story`; `grep` of `AGENTS.md`, `docs/requirements.md` and `crates/entity-eventlog/tests/shared_clock_cost.rs`.

**What I could not establish:**
- `aep plan evidence` is not a subcommand here, so I could not check that `metric_observation` and `verification` are valid evidence kinds for the #55 stories.
- I did not run the sampled symbol and line citations beyond R-150 and R-151 at `docs/requirements.md:222` and `:224`, the `shared_clock_cost.rs:287` assertion, and the reviewed scenario file.
- Out of my lane, not counted in the verdict: the story:recorded-open-verifies-a-checkpoint-and-its-suffix Scope says "acceptance 6" for the `AGENTS.md` passage, which is acceptance 7 after renumbering (a stale reference). The design-versus-Eventlog sequencing is for plan-critic-design.

```findings
- file: .engineering/planning/story/a-condition-reads-the-length-of-a-text.md
  line: 48
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: the Outcome claims text length is usable in every predicate position where count of an array or a map is usable, but the acceptance exercises only a precondition, so an invariant or an outcome guard could fail while the story closes
- file: .engineering/planning/story/a-text-field-declares-its-alphabet.md
  line: 48
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: the Outcome and title cover a field or an argument, but no acceptance item names an argument case, so an argument-only gap would still pass
- file: .engineering/planning/story/operation-writes-an-optional-field-from-an-optional-argument.md
  line: 76
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: '"Every definition admitted before this change decides the same bytes after it" names no check that shows it, unlike the sibling items that say a test shows the result'
- file: .engineering/planning/story/recorded-open-verifies-a-checkpoint-and-its-suffix.md
  line: 90
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: acceptance 7 requires AGENTS.md's description of the open to be updated, but AGENTS.md holds no passage describing the open's verification, so there is no before state and no way to tell it is done
```
