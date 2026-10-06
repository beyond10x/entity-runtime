---
format: aep.planning-md/3
id: review-result:er-w2-u1-adversary-pass-1
kind: review-result
status: active
title: 'Wave 2 alphabet: adversary pass 1 (machine paths redacted)'
relations:
- reviews: story:a-text-field-declares-its-alphabet
revision: 1
---
**Verdict: red.** Three cases fail. They cover two findings: one the unit introduced, and one that already fails the same way on the base commit.

```
unit: story:a-text-field-declares-its-alphabet
verdict: red
cases: executed 359→369, red 3
origin: introduced 1, pre-existing 1, undecided 0
wrote-outside-worktree: ~/.cache/er-w/w2u1/attack-adversary/ (base copy, base-target, 10 logs); ~/.cache/b10x-target/er-w2-u1/release/ (new release build in the unit's build dir)
needs-coordinator: yes
```

The coordinator is needed to send finding 2 out of this wave and finding 1 back to the implementor.

**1. What I changed.** `git diff --stat` is the same as the implementor's (10 tracked files, +2997/−889). My only additions are two new test files:
- `crates/entity-core/tests/adversary_text_alphabet.rs` (370 lines)
- `crates/entity-surface/tests/adversary_alphabet.rs` (92 lines)

No implementation file was touched. Afterwards I ran `rustfmt` on those two files only; clippy with `-D warnings` is clean on both.

**2. Cases added.** Three are red now, seven green:

| Case (`adversary_text_alphabet.rs`) | Now | Red output from the first run of that file alone |
|---|---|---|
| `a_response_outside_its_declared_alphabet_is_refused` :88 | red | `accepted, answering Some({"echo": String("12x")}) through a response field whose alphabet is "0123456789"` |
| `a_response_longer_than_its_declared_max_length_is_refused` :114 (the same check with a key the base already had) | red | `accepted, answering Some({"echo": String("12345")}) through a response field whose max_length is 2` |
| `checking_many_values_against_a_large_alphabet_costs_about_what_a_small_one_does` :179 | red | first version: `1,000 one-character values took 723.46945ms against an 8,000-character alphabet and 710.014µs against a 1-character one` |
| union variant; defaults inside an object, array and map; `set` checked after it runs; conditional target equality; `kernel/1` refusal at depth | green | — |
| `adversary_alphabet.rs`: `x-alphabet` in every projection; YAML round trip of `on`, `~`, `0x1F`, `1e5` | green | — |

I had to correct the cost case once. Its first version was red in a debug build but **green in release**, because its 100 ms margin absorbed the cost. I raised the alphabet to 40,000 characters and lowered the margin to 30 ms. It is now red in both builds:
- debug: `4.896983109s` against `1.371275ms`
- release: `425.444771ms` against `412.331µs`

A fixed implementation pays the one-time set build of about 5 ms, well inside the margin.

**3. Suite run** (after the cases existed). `cargo test -p entity-core -p entity-surface --locked --no-fail-fast` exited 101: 366 passed, 3 failed, 369 executed. The same three cases fail, and every other binary is ok.
- The "before" number, 359, comes from a separate run that left out my two files: `--lib` plus each other `--test` target, then `--doc`. All exits were 0.
- To settle origin, I ran the core file against `git archive c07f1c10` in scratch. The `max_length` case is red there with the identical message. Every alphabet case fails there with "unknown field `alphabet`", as expected before the key existed.

| file:line | what | what reaches it | verdict / origin | case |
|---|---|---|---|---|
| `crates/entity-core/src/validation.rs:2732` | The set of allowed characters is rebuilt for every value. Checking N values therefore costs N × the alphabet's size (about 425 µs per value in release at 40,000 characters). | The caller chooses how many values to send in an array or map. The author chooses the alphabet, and its length has no bound (ESS § 1). | NEEDS-CHANGE / introduced | :179 |
| `crates/entity-core/src/runtime.rs:1695` | A branch's response is never checked against the declared `response`, although `docs/design/service-semantics-v0.1.md:708` says it is. Registration accepts an alphabet on a response field that nothing then enforces. | A hand-written `service/1` response field stricter than the value it is filled from. Reaching it through ESS lowering is a guess I have not shown. | CONFIRMED / pre-existing | :88, :114 |

Fixes, which I have not applied:
- **Finding 1:** build the set once per field definition, either at registration or once per array or map rather than per element.
- **Finding 2:** check responses at step 14 against `response`. This belongs in its own story: responses already recorded could start failing replay once the check exists.

**4. Judgement findings with no failing case:** none.

**5. Attacked and could not break:**
- Value checks, defaults and registration refusals in union variants, nested properties, array items and map values.
- A `set` that copies an unconstrained argument into a field with an alphabet is refused after the set runs.
- The conditional-target equality check includes the alphabet.
- `kernel/1` refuses the key with the right path at every depth.
- The OpenAPI and AsyncAPI projections, and the YAML round trip.
- The implementor's tests compare complete expected values; none of them could pass if the code were wrong.
- Read but not run: replay re-validation (`replay.rs:503`, `:672`), record framing, and the MCP schema, which reuses `object_schema`.

**6. Paths written outside the worktree:**
- `~/.cache/er-w/w2u1/attack-adversary/base` (37M: the base source plus my core case)
- `~/.cache/er-w/w2u1/attack-adversary/base-target` (98M)
- Logs in that directory: `base-run.log`, `clippy-core.log`, `core-after-fmt.log`, `core-first-run.log`, `core-perf-v2.log`, `core-release-perf.log`, `suite-after-final.log`, `suite-after.log`, `suite-before-deselected.log`, `surface-first-run.log`
- `~/.cache/b10x-target/er-w2-u1/release/`

I took and released my own worktree lease (`er-w2-u1-adversary-pass1`).

```findings
- file: crates/entity-core/src/validation.rs
  line: 2732
  category: property
  severity: warning
  verdict: NEEDS-CHANGE
  origin: introduced
  message: "validate_string rebuilds the alphabet's BTreeSet for every value, so 1,000 one-character values cost 425 ms in release against a 40,000-character alphabet and 0.41 ms against a 1-character one."
- file: crates/entity-core/src/runtime.rs
  line: 1695
  category: contract-drift
  severity: warning
  verdict: CONFIRMED
  origin: pre-existing
  message: "materialize_response never validates a branch's response against the declared response schema, contrary to service-semantics-v0.1.md:708, so an alphabet admitted on a response field is never enforced, as max_length already was not at c07f1c10."
```
