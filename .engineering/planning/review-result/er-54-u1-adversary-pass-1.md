---
format: aep.planning-md/3
id: review-result:er-54-u1-adversary-pass-1
kind: review-result
status: active
title: 'Issue 54 U1 (operation set_if_present): adversary pass 1'
relations:
- reviews: story:operation-writes-an-optional-field-from-an-optional-argument
revision: 1
---
```
unit: story:operation-writes-an-optional-field-from-an-optional-argument, worktree er-54-u1 at 8b7d4dab plus one untracked test file
verdict: CONFIRMED (one introduced warning, one pre-existing; no blocker)
cases: executed 372→378, red 2
origin: introduced 3 / pre-existing 1 / undecided 0
wrote-outside-worktree: none
needs-coordinator: yes. Fixing the parent-default gap for creation as well would change definitions admitted at base, which the story's "same bytes" acceptance forbids. Choose operation-only or both.
```

**1. Diff stat.** `git --no-pager diff --stat` is empty because my only change is untracked. `git status --short` shows one test file and no other path:
```
?? crates/entity-core/tests/adversary_operation_set_if_present.rs
```

**2. Cases added** (all in that file; red output from running each case alone, before the suite):

| case | asserts | now |
|---|---|---|
| `a_parent_default_cannot_make_an_operation_set_if_present_leaf_present_for_a_caller_who_sent_none` (:77) | a required parent `bound` whose `default` contains the leaf is refused with `ConditionalArgumentInvalid` (design rule 3: "exactly the leaf controls presence") | **red** |
| `a_parent_default_cannot_make_a_creation_set_if_present_leaf_present_for_a_caller_who_sent_none` (:114) | the same rule on a creation outcome | **red** |
| `kernel_1_refuses_an_operation_outcome_carrying_set_if_present` | kernel/1 refuses it as `SemanticsKeyNotAvailable` at `operations.annotate.outcomes` | green |
| `an_empty_text_argument_is_present_and_replaces_the_field` | `""` counts as present and replaces `"old"` | green |
| `the_prepared_continuation_writes_the_bytes_decide_writes_for_an_operation_set_if_present` | `decide_before_load` + `continue_with` record bytes equal `decide`'s, with the argument present and absent | green |
| `a_service_3_removal_beside_an_operation_set_if_present_records_one_change_and_one_removal` | `Remove` on `stamp` plus a present `note`: `changed={note}`, `removed={stamp}`, the events agree, replay matches | green |

Red output, each case run alone:
```
panicked at crates/entity-core/tests/adversary_operation_set_if_present.rs:101:13:
registered a set_if_present leaf whose parent default makes it present: a caller that sent no argument had note 'kept' overwritten with "from-default" (changed = {"note":"from-default"})
```
```
panicked at crates/entity-core/tests/adversary_operation_set_if_present.rs:137:13:
registered a creation set_if_present leaf whose parent default makes it present: a caller that sent no `bound` got note "from-default"
```
Origin was settled by running the same file against a copy of base 08d49cbd (`git archive` into `build/scratch/base`):
- The creation case fails at base with the identical message, so it is pre-existing.
- The operation case fails at base for a different reason: the definition is refused as `conditional_state_insertion is creation-only`. The overwrite is therefore introduced, because the unit opened a path base refused.

**3. Suite run** (after the cases existed): `cargo test -p entity-core --locked --no-fail-fast` exited 101.
- Every test binary passed except `adversary_operation_set_if_present`: 4 passed, 2 failed.
- Totals: 376 passed, 2 failed, 378 executed.
- The 372 "before" figure is that same run minus this file's 6 cases.
- `cargo fmt -p entity-core -- --check` exited 0; `cargo clippy -p entity-core --all-targets --locked -- -D warnings` exited 0.

**4. Findings** (they cover 8b7d4dab plus the untracked test):

| # | file:line | verdict | origin | measured by | what reaches it |
|---|---|---|---|---|---|
| F1 | crates/entity-core/src/validation.rs:818 | CONFIRMED | introduced | the operation parent-default case | Any hand-written service/2 definition with a parent default. Every `annotate {}` call then overwrites the stored `note` with the default, although the acceptance says "absent → unchanged". I could not check whether the ESS lowering emits such defaults: the guard blocked reading the ess repo. Suggested fix: in the parent loop of `present_argument_leaf`, refuse a parent whose `default` is not `Absent`. |
| F2 | crates/entity-core/src/validation.rs:818 | CONFIRMED | pre-existing | the creation parent-default case | The same gap on creation outcomes, unchanged since base. |
| F3 | website/docs/concepts/service-semantics.md:274 | CONFIRMED | introduced | read | The public service/2 row still reads "copied into creation state, events and responses" and does not mention operation state. |
| F4 | crates/entity-core/src/runtime.rs:1140 | CONFIRMED | introduced | read | The `decide` step table row 8 names only `set` "against pre-operation fields"; `set_if_present` also writes at step 8, from arguments. The same row is at docs/design/service-semantics-v0.1.md:424. |

**5. Attacked and could not break:**
- JSON null and empty text both count as present.
- The order set → set_if_present → fulfillment cannot change the result: the keys are disjoint, and registration enforces that.
- Invariants run after set and events run last.
- `decide` takes `&EntityInstance`, so a refusal cannot mutate caller data.
- `replay` reruns the decision and `rehydrate` refuses service histories.
- `set` and `set_if_present` naming one field gives `ConditionalTargetConflict`.
- Record framing depends on semantics, not keys.
- Nothing still names the removed variant: no code in this tree and no consumer code at origin/main of aep, aep-service, atlas, bench or ess (only ESS design prose mentions it).
- `kinds.rs` reads block-form match arms.
- In `suite.json`, only the named reviewed scenario changed; the other 435 scenarios are byte-identical and the 4 new ones were added.

**6. Paths written outside the worktree:** none. Inside the worktree, in git-ignored scratch:
- `build/scratch/adv/` (logs and base `suite.json` extracts, about 10M)
- `build/scratch/base/` (the base source copy with its own `target/`, about 137M). I deleted the `.engineering/` that `git archive` copied into it.

**7.**
```findings
- file: crates/entity-core/src/validation.rs
  line: 818
  category: boundary
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: a required argument parent whose default carries the leaf is admitted, so an operation called without the argument overwrites the stored field with the default
- file: crates/entity-core/src/validation.rs
  line: 818
  category: boundary
  severity: note
  verdict: CONFIRMED
  origin: pre-existing
  message: present_argument_leaf ignores parent defaults, so a creation set_if_present leaf becomes present for a caller who omitted the parent, contrary to design rule 3
- file: website/docs/concepts/service-semantics.md
  line: 274
  category: contract-drift
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: the public service/2 row still limits copied optional arguments to creation state, events and responses
- file: crates/entity-core/src/runtime.rs
  line: 1140
  category: contract-drift
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: the decide step table row 8 omits set_if_present, which writes at step 8 from arguments rather than pre-operation fields
```
