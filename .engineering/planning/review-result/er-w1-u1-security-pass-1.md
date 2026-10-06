---
format: aep.planning-md/3
id: review-result:er-w1-u1-security-pass-1
kind: review-result
status: active
title: 'U1 text length: security review pass 1 (machine paths redacted)'
relations:
- reviews: story:a-condition-reads-the-length-of-a-text
revision: 1
---
```
unit: story:a-condition-reads-the-length-of-a-text — working tree of er-w1-u1 over base b7362882 (uncommitted)
verdict: red
cases: executed 322→333, red 4
origin: introduced 2 / pre-existing 2 / undecided 0
wrote-outside-worktree: 1 path (~/.cache/er-w/u1/attack-security/)
needs-coordinator: yes — decide the projection-key fix (refuse it at registration or teach entity-store `key_of`); route the 2 pre-existing findings
```

**1. Diff stat.** The tracked diff is the same as when I started: `11 files changed, 4281 insertions(+), 1051 deletions(-)`. `git status` shows two new untracked files: my `crates/entity-core/tests/security_text_count.rs` and the adversary's `adversary_text_count.rs`. I changed no implementation file. Both mutants ran only in a scratch copy.

**2. Cases** in `security_text_count.rs`. Each was run alone first, logs in scratch as `case-*.log`:

| case | now | base | output when run alone |
|---|---|---|---|
| `a_projection_keyed_on_a_text_length_is_refused_where_it_is_written` | red | green | `a projection keyed on $fields.name.count registered, but the store's projection walks objects only…` |
| `a_projection_keyed_on_an_array_count_is_refused_where_it_is_written` | red | red | same message for `$fields.tags.count` |
| `a_map_inside_a_quantifier_element_is_addressed_by_its_size` | red | red | `left: (Unobservable(["$m.count"]), Violated)` / `right: (Held, Held)` |
| `a_stored_text_that_holds_no_text_answers_no_length` | red | definition refused | `Ok(Decision { … fields: {"name": "Bo"} … })`; expected `PreconditionUnobservable` |
| `a_text_length_through_an_untyped_element_or_argument_keeps_resolving_to_nothing` | green | green | `ok` |
| `a_text_length_in_a_template_is_deterministic_and_replays` | green | refused (no feature) | `ok` |

Three revisions after a first run:
- The map case now asserts both answers in one comparison, so the run shows both.
- I removed an "array with no `items`" sub-case, because the schema refuses such an array (`array must declare 'items'`).
- I mutated a scratch copy twice to check that the green R-97 case would catch a regression. Removing the union reset (runtime.rs:2923) made it fail: `left: Held, right: Unobservable(["$fields.payees.0.value.count"])`. Making the text arm value-keyed made it fail: `left: Held, right: Unobservable(["$t.count"])`.

**3. Suite.** `cargo test --locked -p entity-core --no-fail-fast` exited 101 with 333 executed: 326 passed, 7 failed. The 322 before comes from the implementor's `g-core.log`; the 11 new cases are my 6 and the adversary's 5. The 7 failures are my 4 above and 3 in the adversary's file. Clippy on my file with `-D warnings` exits 0, and rustfmt `--check` exits 0.

**4. Findings** (cover the working tree above):

| file:line | what | verdict / origin | what reaches it |
|---|---|---|---|
| crates/entity-core/src/validation.rs:1482 | Projection keys are checked with the `service/1` path rules, so `$fields.<text>.count` now registers. But `entity_store::project`'s `key_of` (entity-store/src/projection.rs:77-81) walks objects only, so every instance is left out. That breaks the promise at validation.rs:163-165. Fix: refuse the key at registration, or teach `key_of` the address forms. | CONFIRMED / introduced | Registration via `Registry::register` or `entity validate`. Nothing calls `project` in this repo or on origin/main of aep, aep-service, atlas or bench. |
| crates/entity-core/src/validation.rs:1443 | The same gap for array `count` (and by the same code, map `count` and ordinals), present since R-148. | CONFIRMED / pre-existing | Same as above. |
| crates/entity-core/src/runtime.rs:2803 | Inside a quantifier, `$m.count` on an element declared as a map registers, but the run-time walk reads the element without its declaration. A map with a member named `count` returns that member's value (5), not its size; without one the rule is unobservable. This confirms the brief's lead. | CONFIRMED / pre-existing | Any `service/1` rule that runs a quantifier over a list of maps. |
| crates/entity-core/src/runtime.rs:2913 | When a declared `string` holds an object, `.count` falls through and returns the object's own `count` member. The map arm returns nothing in that situation. Fix: in `walk`, return `None` after a checked declared `string`. | INFEASIBLE / introduced | Only a corrupted or hand-edited stored instance reaches it. Arguments are validated before any rule reads them (runtime.rs:1187, :845), and no store validates instances on load. |

**5. Checked and found nothing wrong:**
- **Invariant 1 (no IO):** `purity.rs` is green and no import was added.
- **Invariant 2 (same inputs, same bytes):** two runs of the same decision serialise to identical bytes, and it replays.
- **Invariant 5 (every path checked), for rules and templates:** every `service/1` run-time context carries the same argument schema registration used. The three that carry none are `kernel/1`-only: `rehydrate` refuses `service/1` (replay.rs:233), and the creation `emit` has no `$args`.
- **Invariant 7 (closed reference set):** unchanged.
- **Invariant 8 (evaluation order):** not touched.
- **`kernel/1`:** unchanged in both registration and run time.
- **No newly refused definitions:** registration refuses nothing the base admitted, so no replayed definition snapshot starts failing (R-97).
- **R-97 safety fact, now at step 4:** the untyped paths (quantifier over a `json` path, a union inside an array element, an undeclared argument under an open schema) give the same answer on base and unit.
- **Counting:** `max_length` counts `chars()` the same way (validation.rs:2635).
- **ESS scenarios:** all 7 agree with the code.

**6. Paths written outside the worktree.** All are under `~/.cache/er-w/u1/attack-security/`:
- `base/` (37M) and `mutant/` (37M): `git archive` copies of the base and the unit tree.
- `base-target/` (99M) and `mutant-target/` (99M): build directories.
- `case-*.log`, `suite.log`, `suite-nff.log`, `base-security.log`, `mut-union-reset.log`, `mut-value-keyed.log`, `clippy.log`.

The worktree's own build directory was `~/.cache/b10x-target/er-w1-u1` (shared, as the brief assigns).

```findings
- file: crates/entity-core/src/validation.rs
  line: 1482
  category: contract-drift
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: "a projection keyed on $fields.<text>.count now registers, but entity-store key_of walks objects only, so the read model silently files no instance (a_projection_keyed_on_a_text_length_is_refused_where_it_is_written)"
- file: crates/entity-core/src/validation.rs
  line: 1443
  category: contract-drift
  severity: note
  verdict: CONFIRMED
  origin: pre-existing
  message: "projection keys admit the R-148 collection addresses such as $fields.tags.count, which key_of cannot resolve (a_projection_keyed_on_an_array_count_is_refused_where_it_is_written)"
- file: crates/entity-core/src/runtime.rs
  line: 2803
  category: boundary
  severity: warning
  verdict: CONFIRMED
  origin: pre-existing
  message: "$m.count on a declared map quantifier element registers but at run time reads the map's own count member or nothing instead of its size (a_map_inside_a_quantifier_element_is_addressed_by_its_size)"
- file: crates/entity-core/src/runtime.rs
  line: 2913
  category: integrity
  severity: note
  verdict: INFEASIBLE
  origin: introduced
  message: "a declared string holding an object answers its own count member as a length instead of being unobservable; only a non-conforming stored instance reaches it (a_stored_text_that_holds_no_text_answers_no_length)"
```
