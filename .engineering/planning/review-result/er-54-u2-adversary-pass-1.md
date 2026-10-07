---
format: aep.planning-md/3
id: review-result:er-54-u2-adversary-pass-1
kind: review-result
status: active
title: 'Issue 54 U2 (set increments): adversary pass 1'
relations:
- reviews: story:set-increments-a-numeric-field
revision: 1
---
unit: story:set-increments-a-numeric-field, at branch head 5cafe065 plus my one untracked test file
verdict: red (2 failing cases, both about documents; the arithmetic, the overflow checks and replay held up)
cases: executed 394→399, red 2
origin: introduced 3 / pre-existing 0 / undecided 0
wrote-outside-worktree: none
needs-coordinator: none

**1. What I touched.** `git --no-pager diff --stat` is empty because my only file is untracked. `git status --short`:
```
?? crates/entity-core/tests/increment_adversary.rs
!! build/
```
That is one test file. No implementation file changed, and nothing under `.engineering/`.

**2. Cases added** (`crates/entity-core/tests/increment_adversary.rs`). I ran this file alone, before any suite run.

| case | asserts | now |
|---|---|---|
| `the_kernel_1_evaluation_order_names_increment_overflow_at_the_set_step` :40 | a kernel/1 overflow returns `IncrementOverflow`, and `kernel-v0.1.md` § 6 names it at the set step | **red** |
| `the_reserved_one_key_literal_on_a_json_field_is_refused_as_the_changelog_says_on_a_creation_too` :91 | a `json` field given `{increment: 1}` on a creation branch is refused as `increment_target_invalid`, as the CHANGELOG says | **red** |
| `an_increment_amount_reading_an_optional_field_with_a_default_is_refused_at_registration` :129 | `$fields.<optional field with a default>` as an amount is refused | green; red against a mutant |
| `a_number_increment_is_the_exact_sum_of_any_two_spellings_and_folds_to_the_same_bytes` :262 | 3,000 drawn pairs of number spellings (exponents, trailing zeros, `-0`) sum exactly; 300 of them fold back to the same bytes | green |
| `an_integer_increment_is_exact_inside_the_span_and_overflow_outside_it_under_both_semantics` :324 | 800 pairs at the i64/u64 edges: exact inside the range, `IncrementOverflow` outside it | green |

Red output from the first run of this file alone (exit 101):
```
assertion `left == right` failed: a json field written with the reserved literal on a creation branch: [IncrementOnCreate { path: "create.outcomes.opened.set.note", field: "note" }]
  left: ["increment_on_create"]
 right: ["increment_target_invalid"]
kernel-v0.1.md § 6 names only [" 6. set, every assignment against pre-operation fields      Template"] at the set step, but a kernel/1 set now refuses as IncrementOverflow there
test result: FAILED. 3 passed; 2 failed
```

**Mutant check, on a copy** (`build/scratch/adversary/mutant`): I changed `crates/entity-core/src/validation.rs:2286` from `(Some(&scope.fields.fields), path, false)` to `true`. The unit's own tests stayed green (replay 45/45, requirements 78/78, service_semantics 46/47). The one failure is the doc-reading test, which cannot find `docs/` inside the copy. My :129 case failed at :141.

**3. Suite run, after the cases existed.** `cargo test -p entity-core --locked --no-fail-fast` exited 101. 399 cases ran: 27 unit, 368 integration, 4 doc-tests. The only failures were the 2 cases above. The 394 is the same run with my file's 5 cases taken out. My file passes rustfmt and `cargo clippy -p entity-core --test increment_adversary --locked -- -D warnings`. I did not run `task check` or `task ess-check`.

**4. Findings**

| # | file:line | verdict | origin | case | what reaches it |
|---|---|---|---|---|---|
| 1 | `docs/design/kernel-v0.1.md:430` | NEEDS-CHANGE | introduced | :40 red | Any kernel/1 increment past the integer range, e.g. the ESS scenario `set-increment-overflow-is-refused`, step 1. § 6 is the normative order (invariant 8, R-70). `service-semantics-v0.1.md` § 4.2 and the `runtime.rs` step table were updated; this page was not. Fix: name `IncrementOverflow` on the step-6 line. |
| 2 | `CHANGELOG.md:39` | INFEASIBLE | introduced | :91 red | The code (`validation.rs:2170`) refuses a creation-branch `set` before it checks the target field, so the refusal is `increment_on_create`, whose message says "no value to add to". Reaching this needs a creation branch that writes `{increment: …}` as a literal object to a `json` or `object` field. That worked at the base, but the story's search of consumers found no such definition, so nothing I found reaches it. Fix: one CHANGELOG clause, or check the target first. |
| 3 | `crates/entity-core/src/validation.rs:2286` | CONFIRMED | introduced | :129 green, red on the mutant | No unit test or ESS scenario pins "defaults do not count for `$fields`". It matters because an optional field can become absent: a service/3 `fulfills` Remove deletes it (`runtime.rs:1328`), and so will the next unit's `{cleared: true}`. Keeping the :129 case closes the gap. |

**5. Attacked and could not break**
- Exact decimal sum and its spelling: 3,000 drawn pairs, plus byte-identical folds through `rehydrate`.
- Integer ranges at the i64/u64 edges under kernel/1 and service/1, and the place-count limit, tested both sides.
- A `{increment: …}` inside a larger value stays a template, because `SetAssignment::of` only looks at the top level of each `set` entry.
- `set_if_present` and `fulfills` overlapping an increment are already refused.
- Amounts that are refused as they should be: a string, null, an optional or json argument, an absent nested field.
- Replay and the legacy event fold use the same function as `execute`.
- The CLI refusal mapping covers every variant, the refusals page lists every new kind, and no other crate reads `set` values (surface and CLI read only the keys).

**6. Paths written:** none outside the worktree. Inside it:
- `crates/entity-core/tests/increment_adversary.rs`, untracked.
- `build/scratch/adversary/`, git-ignored, 147M: logs, plus the mutant copy with its own `target/`. Cleanup is yours.

The worktree's own `target/` was rebuilt incrementally. My session lease is released.

```findings
- file: docs/design/kernel-v0.1.md
  line: 430
  category: contract-drift
  severity: warning
  verdict: NEEDS-CHANGE
  origin: introduced
  message: The normative kernel/1 evaluation order names only Template at the set step, while a kernel/1 set now also refuses as IncrementOverflow.
- file: CHANGELOG.md
  line: 39
  category: contract-drift
  severity: note
  verdict: INFEASIBLE
  origin: introduced
  message: The changelog says the reserved one-key literal on an object or json field is refused as increment_target_invalid, but on a creation branch it is refused as increment_on_create.
- file: crates/entity-core/src/validation.rs
  line: 2286
  category: mutant
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: No unit test or scenario pins that defaults do not fill a $fields amount path, so flipping that guard leaves the unit's whole suite green.
```
