---
format: aep.planning-md/3
id: review-result:er-w2-u1-adversary-pass-2
kind: review-result
status: active
title: 'Wave 2 alphabet: adversary pass 2 (machine paths redacted)'
relations:
- reviews: story:a-text-field-declares-its-alphabet
revision: 1
---
**Pass 2: nothing found.** I found no defect in correction round 1's cache, so the findings block is empty. My three new cases all pass, and two mutated copies of the cache are each caught by at least one case.

```
unit: story:a-text-field-declares-its-alphabet
verdict: green
cases: executed 369→372, red 0
origin: introduced 0, pre-existing 0, undecided 0
wrote-outside-worktree: ~/.cache/er-w/w2u1/attack-adversary/{pass2-mutant (37M), pass2-target (358M), validation.rs.pass2-orig, 10 pass2-*.log}
needs-coordinator: no
```

**1. What I changed.** `git diff --stat` is unchanged: the implementor's 10 tracked files, +3037/−906. My only edit this pass is to `crates/entity-core/tests/adversary_text_alphabet.rs`, which goes from 375 to 499 lines. `crates/entity-surface/tests/adversary_alphabet.rs` is unchanged. No implementation file was touched; rustfmt and clippy with `-D warnings` are clean.

**2. Cases added** (`adversary_text_alphabet.rs`). All 11 cases in the file passed on the first run (`test result: ok. 11 passed`).

| Case | What it asserts |
|---|---|
| `every_declaration_met_in_one_call_answers_from_its_own_alphabet` :429 | One `create` meets seven alphabet declarations: properties repeated across array elements, union variants repeated across array elements, map values, and two declarations spelled the same. It must return exactly 8 errors in the expected order. |
| `a_default_meeting_two_alphabets_names_each_offence_from_its_own` :452 | A default that reaches two sibling alphabets reports one exact error for each, from each one's own alphabet. |
| `where_a_definition_sits_in_memory_changes_no_answer_and_no_byte` :477 | The original definition, a clone and a rebuilt copy all produce identical refusals and identical decision-record bytes. |

I strengthened one of these after its first run. Under the constant-key mutant below, the default case originally passed, because `'c'` is outside both alphabets. I changed `"c"` to `"a1"`, so a shared set now gives a different error. The fault was in my case; the code is unaffected.

**3. Suite.** `cargo test -p entity-core -p entity-surface --locked --no-fail-fast` exits 0 with 372 executed and 0 failed. Running it again with my three new cases skipped by name gives 369, which is the "before" number.

I tried two mutants of the cache, each on a copy in scratch, never in the worktree:

| Mutant | Result |
|---|---|
| Constant key, so every declaration shares one set | 4 cases failed, mine and the implementor's. The strengthened default case also fails (`'a'` at position 1 instead of `'1'` at position 2). |
| Set rebuilt on every lookup | The cost case fails: `3.798218443s against a 40,000-character alphabet and 1.447031ms against a 1-character one`. |

**4. Judgement findings:** none.

**5. Attacked and could not break:**
- **An address reused within one call:** every recursive call borrows its sub-declaration from a definition that outlives the call. Value validation never builds a temporary `FieldDefinition`; nested declarations are stored separately on the heap, so no two live declarations can share an address.
- **Determinism:** the cache is only looked up, never iterated. The clone and rebuild case gives byte-identical records.
- **Purity scan:** `purity.rs` passes. It does not ban keys derived from memory addresses, so if a later change iterated `alphabets`, results would follow memory order and the scan would not notice. That is a gap in the scan, not a defect today.
- **Error order or count:** the change only replaces `Vec` pushes with `Findings::push`. Every existing case still passes, and my exact 8-error list holds.
- **Replay of definitions the base registered:** the `entity-store` suite exits 0, including the `service_2_framing` fixture bytes. The entity-core `replay.rs` tests pass (43).
- **Cost case:** passes in both debug and release.
- **By design, not raised:** the set is built once per call per declaration. Replaying R records against an A-character alphabet therefore costs R × A, which matches the round-1 decision.

**6. Paths written outside the worktree this pass:** all under `~/.cache/er-w/w2u1/attack-adversary/`.
- `pass2-mutant/` (base copy plus the current entity-core)
- `pass2-target/` (build dir, 358M)
- `validation.rs.pass2-orig`
- Logs: `pass2-clippy.log`, `pass2-core-first-run.log`, `pass2-core-second-run.log`, `pass2-entity-store.log`, `pass2-mutant-M1.log`, `pass2-mutant-M1b.log`, `pass2-mutant-M2.log`, `pass2-release-perf.log`, `pass2-suite.log`, `pass2-suite-deselected.log`

The release build also wrote into the unit's build dir, `~/.cache/b10x-target/er-w2-u1`. My pass-1 `base/` and `base-target/` copies had already been removed when I looked.

I took and released lease `er-w2-u1-adversary-pass2`.

```findings
[]
```
