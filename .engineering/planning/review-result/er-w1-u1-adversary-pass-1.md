---
format: aep.planning-md/3
id: review-result:er-w1-u1-adversary-pass-1
kind: review-result
status: active
title: 'U1 text length: adversary pass 1 (machine paths redacted)'
relations:
- reviews: story:a-condition-reads-the-length-of-a-text
revision: 1
---
**Verdict: red.** I added 5 cases. 3 fail on the working tree. One failure was caused by this diff: a projection keyed on `$fields.<text>.count` now passes registration, but the shell never fills it. The other two failures already happen at the base commit.

```
unit: story:a-condition-reads-the-length-of-a-text
verdict: red
cases: executed 322→327, red 3
origin: introduced 2, pre-existing 2, undecided 0
wrote-outside-worktree: ~/.cache/er-w/u1/attack-adversary/ (contents in part 6), ~/.cache/b10x-target/er-w1-u1 (assigned build dir)
needs-coordinator: yes
```

### 1. Diff
`git status --short` shows one path of mine, and it is a test file: `?? crates/entity-core/tests/adversary_text_count.rs` (225 lines). `git --no-pager diff --stat` is the implementor's, unchanged: 11 files, +4281/−1051. I touched no implementation file. `security_text_count.rs` belongs to the security reviewer.

### 2. Cases (written first, each run alone)
| case | asserts | now |
|---|---|---|
| `a_projection_keyed_on_a_text_length_is_refused_at_registration` | `projections.by_key: $fields.title.count` is refused as `InvalidTemplate` | **red** |
| `a_projection_keyed_on_an_array_count_is_refused_at_registration` | the same, for `$fields.tags.count` | **red** |
| `a_map_count_inside_a_quantifier_is_the_elements_size` | `for_all g in groups: $g.count lte 1`, where `g = {"count": 5}`, gives True (map size 1) | **red** |
| `a_text_length_one_level_inside_a_quantifier_element_is_refused_at_registration` | `$t.name.count` and `$t.0.count` are refused as `QuantifierBodyScope` | green (catches 2 mutants the suite misses) |
| `a_text_length_in_a_set_template_and_an_event_payload_resolves_and_replays` | `set` and `emits` give 3 for `e`+U+0301+U+1F600, and the result replays | green |

Red output, verbatim (`cargo test -p entity-core --test adversary_text_count -- --exact <name>`, exit 101 each):
```
panicked at crates/entity-core/tests/adversary_text_count.rs:79:9:
a projection keyed on '$fields.title.count' registered, and the shell's key walk resolves it to nothing for every instance
panicked at crates/entity-core/tests/adversary_text_count.rs:101:9:
a projection keyed on '$fields.tags.count' registered, and the shell's key walk resolves it to nothing for every instance
panicked at crates/entity-core/tests/adversary_text_count.rs:129:5:
assertion `left == right` failed: a map of one entry has size 1, not the value of its `count` member
  left: False
 right: True
```

### 3. Suite, run after the cases existed
`CARGO_TARGET_DIR=~/.cache/b10x-target/er-w1-u1 cargo test -p entity-core --no-fail-fast` gave **exit 101**: 324 passed, 3 failed (the three above), 327 executed. The security file was not there yet; its binary is not in the log. The 322 "before" figure comes from the implementor's `~/.cache/er-w/u1/g-core.log`.

### 4. Findings (working tree of er-w1-u1 over b7362882)
| file:line | what | what reaches it | verdict / origin | case |
|---|---|---|---|---|
| `crates/entity-core/src/validation.rs:167` | Projection keys go through the rule walk, which now admits `<text>.count`. `entity-store/src/projection.rs:70` (`key_of`) only walks objects. A scratch probe printed `"by_title_length":{}` next to `"by_title":{"Ann":["a"],"Bob!":["b"]}`. The base refused this key. | The public `entity_store::project`. No in-repo caller outside tests. | NEEDS-CHANGE / introduced | `a_projection_keyed_on_a_text_length_…` |
| `crates/entity-core/src/validation.rs:167` | Same drift for `<array>.count`. The probe printed `"by_tag_count":{}`. | same | CONFIRMED / pre-existing (red at base) | `a_projection_keyed_on_an_array_count_…` |
| `crates/entity-core/src/runtime.rs:2803` | The lead is confirmed. Registration accepts `$g.count` on a map element, but at run time the walk has lost the element's declaration and reads the member named `count`: 5 instead of 1. The invariant gives a wrong False, not Unknown. | Any `service/1` quantifier over map elements. No in-repo definition uses it. | CONFIRMED / pre-existing (same False≠True at base) | `a_map_count_inside_a_quantifier_…` |
| `crates/entity-core/src/validation.rs:1505` (also `:1455`) | Two mutants survive the whole implementor suite (322 pass): `typed` → `true` in the object arm, and the same in the array arm. My case fails on each. I mutated a scratch copy only. | The unit's own binder guard | CONFIRMED / introduced | `a_text_length_one_level_inside_…` |

Suggested fixes, not applied:
- **Projection keys (rows 1–2):** you choose the fix. My case assumes the first option and would need replacing under the second.
  - Refuse the `count` and ordinal address forms in projection keys at registration.
  - Or teach `key_of` those forms. That code is in `entity-store`, outside this unit's crate.
- **Map count in a quantifier (row 3):** refuse map `count` under a binder, the same way the unit already refuses text `count` there. The alternative is to carry the element's declaration into the binder walk.

### 5. Attacked and not broken
- **Union payload:** the `checked` flag turns off after the content key, and registration admits no typed path past it.
- **R-97:** I found no path the base registered that now resolves to a different value.
- **Event-fold replay:** `$args.<text>.count` gets no schema there (`replay.rs:558`), but `rehydrate` refuses every service-semantics definition first (`replay.rs:229`), so a text count never reaches it.
- **`kernel/1`:** registration and run-time branches are unchanged on reading.
- **Counting:** `chars().count()` matches `max_length` (`validation.rs:2635`).
- **Templates:** `set`, `emits` and replay are correct (case 5).
- **Null or absent text:** resolves to nothing, as an absent array does.

### 6. Paths written outside the worktree
- `~/.cache/b10x-target/er-w1-u1`: the assigned build dir.
- `~/.cache/er-w/u1/attack-adversary/`:
  - `base/`: a `git archive` of b7362882 plus my test file.
  - `mutant/`: a copy of the working tree. Its `validation.rs` is restored to the unmutated version, and it holds my probe `crates/entity-store/tests/probe_projection_text_count.rs`.
  - `validation.rs.orig` and `*.log`.
  - `base-target/` and `mutant-target/`: my own build dirs. I deleted them after use because the disk was at 98%.
- Worktree session lease `er-w1-u1-adversary`: acquired and released.

### 7.
```findings
- file: crates/entity-core/src/validation.rs
  line: 167
  category: contract-drift
  severity: warning
  verdict: NEEDS-CHANGE
  origin: introduced
  message: "a projection keyed on `$fields.<text>.count` now registers, and entity-store `key_of` walks objects only, so the read model is always empty"
- file: crates/entity-core/src/validation.rs
  line: 167
  category: contract-drift
  severity: warning
  verdict: CONFIRMED
  origin: pre-existing
  message: "a projection keyed on `$fields.<array>.count` registers under service/1 and the shell's key walk resolves it to nothing for every instance"
- file: crates/entity-core/src/runtime.rs
  line: 2803
  category: boundary
  severity: warning
  verdict: CONFIRMED
  origin: pre-existing
  message: "inside a quantifier, `$g.count` on a declared map element reads the member named count (5) instead of the map's size (1)"
- file: crates/entity-core/src/validation.rs
  line: 1505
  category: mutant
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: "the unit's suite stays green when `typed` is forced true in walk_field_path's object arm (1505) or array arm (1455)"
```
