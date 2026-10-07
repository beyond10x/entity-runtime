---
format: aep.planning-md/3
id: review-result:er-59-u1-adversary-pass-1
kind: review-result
status: active
title: 'Issue 59 unit: adversary pass 1 (machine paths redacted)'
owner: entity-runtime
relations:
- reviews: story:verified-model-holds-each-record-once
revision: 1
---
# Issue 59 unit: adversary pass 1 (machine paths redacted)

Adversary report on `impl/verified-model-holds-each-record-once` at `d9fcaf1f` (base `3763b08d`),
as returned, with the home directory prefix of local paths replaced by `~`. The findings block is
the author's corrected response after the CLI refused the first three, whose keys, severity and verdict words the
coordinator had given wrongly.

```
unit: story:verified-model-holds-each-record-once
verdict: red
cases: executed 265→267, red 2
origin: introduced 3, pre-existing 0, undecided 0
wrote-outside-worktree: 3 paths in ~/.cache/er-59/u1/adversary/ (see part 6)
needs-coordinator: yes
```

I found no case where the change gives a different answer or refusal than the base. Both red cases
are about the unit's memory-sharing claims under the default `FullVerification` policy, not about
answers. The coordinator needs to decide whether to fix the one-line gap in finding 1 within this
unit, or narrow the CHANGELOG line instead, and whether to accept resident memory as the measure
for "live heap" (finding 3).

**1. Diff stat** (worktree vs d9fcaf1f, test file only)

```
 .../src/adapter/small_store_cost.rs                | 99 ++++++++++++++++++++++
 1 file changed, 99 insertions(+)
```

**2. Cases added** (both in `crates/entity-eventlog/src/adapter/small_store_cost.rs`, a test-only
module). Both are red now. This output is from running the two cases alone, before any suite run:

```
cargo +1.91.0 test -p entity-eventlog --all-features --locked --lib adversary_er59
---- ...adversary_er59_a_default_policy_handle_holds_no_second_decoded_copy_of_a_committed_record stdout ----
panicked at crates/entity-eventlog/src/adapter/small_store_cost.rs:747:5:
a default-policy handle holds a second decoded entry of 12 of 12 committed records in its verified memory, beside the model's one record: ["history-0-0", ..., "seed-created-05"]
---- ...adversary_er59_a_rebuild_after_a_foreign_write_leaves_the_memory_sharing_the_current_model stdout ----
panicked at crates/entity-eventlog/src/adapter/small_store_cost.rs:706:5:
after a rebuild over a remembered prefix the handle's memory keeps the previous model's copy of 12 of 14 remembered records beside the current model's: ["seed-created-03", ..., "history-2-1"]
test result: FAILED. 0 passed; 2 failed; 0 ignored; 0 measured; 78 filtered out
EXIT=101
```

After the runs I made one whitespace-only change: `" {"` became `"{"` at line 721, so that
`cargo fmt --check` passes. I did not re-run after it.

**3. Suite runs** (after the cases existed)

- `cargo +1.91.0 test -p entity-eventlog --all-features --locked --no-fail-fast`: lib
  `FAILED. 78 passed; 2 failed`, every other binary ok. Total 192 passed, 2 failed, 6 ignored.
  EXIT=101.
- `cargo test -p entity-store --locked`: 73 passed, 0 failed. EXIT=0.
- The "before" count of 265 comes from the implementor's gate logs (`scratch/c1-gate2.log` with
  192, `c1-gate4.log` with 73).

**4. Findings** (they cover d9fcaf1f plus my test file)

| # | file:line | test | base | change | origin |
|---|---|---|---|---|---|
| 1 | `crates/entity-eventlog/src/adapter/memory.rs:339` | `adversary_er59_a_rebuild_after_a_foreign_write_leaves_the_memory_sharing_the_current_model` (:662) | Memory owned its own copy of every record; nothing was shared. | Memory shares records only with the handle's first build. When a read rebuilds over an already-verified prefix, `extend(history.shared_from(length))` keeps the previous model's records, so 12 of 14 are held twice. | introduced (an incomplete version of the unit's own claim) |
| 2 | `CHANGELOG.md:20` against `adapter.rs:3810` | `adversary_er59_a_default_policy_handle_holds_no_second_decoded_copy_of_a_committed_record` (:720) | The decode cache keeps a full decoded copy of each record. This is unchanged by the diff. | The new CHANGELOG line says the handle "holds each committed record once". Under the default policy it holds 12 of 12 records' decoded entries twice. | introduced (the claim is new; the copy itself was already in the base) |
| 3 | `docs/ess/evidence/provider-tracking/model-heap-issue-59.txt:25` | none (judgement) | — | The acceptance asks for "live heap", but the probe measures resident memory (VmRSS). The evidence file itself says "live here is the open's resident peak, not its live heap". The ≤0.5 ratio is shown for resident memory only. | introduced |

- **Who reaches 1:** any `FullVerification` handle that reads after another handle has written. The
  read path goes `model_of`, then a whole build, then `verified_prefix`, then the growth branch. The
  cost is one extra copy of every remembered record, up to the 64 MiB cap. `ProviderTracked`
  handles (the issue's consumer and the probe) never use this memory.
  - The fix: in the growth branch, replace `remembered.records` with `history.shared_from(0)`.
    `verified_prefix` has just checked that this prefix is equal by value.
- **Who reaches 2:** every default-policy open.
  - The fix: narrow the CHANGELOG wording to the model indexes and `ProviderTracked`.
- **Why 3 is only "plausible":** I could not measure live heap. The brief rules out release builds,
  and the workspace's `unsafe_code = "forbid"` rules out a counting allocator in the crate.

**5. What I attacked and could not break**

- **Owning build vs borrowing build**, with: duplicate blob digests (the last copy wins in both); a
  batch blob shared by members or by two keys; one record blob named twice; one digest reused
  across blob domains; imported evidence blobs. The first pass and the refusal order are the same in
  both builds, and the step that moves blobs cannot fail.
- **Tracked advance:** `model.held.digests` matches the base's set of held blobs at open and after
  every advance, so the replacement check refuses the same blobs. A suffix that names a blob the
  model holds is refused in both; it is never shown to callers, because a refused advance rebuilds
  the whole model.
- **entity-store verifier refactor:** it makes the same checks in the same order. The new hidden
  entry point accepts exactly what the existing hidden `verify_subject_history_with_checked_bytes`
  accepts.
- **Read answers and model-digest pins:** reads return the same values, and debug output prints the
  same text.
- **Semver:** no public signature or trait impl changed; the one new impl, `PartialEq<ModelHistory>`,
  exists only in test builds. Send/Sync and the other auto traits look unchanged from reading the
  code; I did not compile a check for them.

**6. Paths written outside the worktree**

- ~/.cache/er-59/u1/adversary/cases-alone.log
- ~/.cache/er-59/u1/adversary/suite-eventlog.log
- ~/.cache/er-59/u1/adversary/suite-store.log

Builds went into the tree's own `target/`, so no new build directory was created. Session lease:
acquired and released.

```findings
[
  {"file": "crates/entity-eventlog/src/adapter/memory.rs", "line": 339, "category": "contract-drift", "severity": "warning", "verdict": "CONFIRMED", "origin": "introduced", "message": "After a whole rebuild over a remembered prefix (any default-policy read after a foreign write), VerifiedMemory keeps the previous model's record allocations, so 12 of 14 remembered records are held twice and the memory-sharing claim holds only on a handle's first build"},
  {"file": "CHANGELOG.md", "line": 20, "category": "contract-drift", "severity": "note", "verdict": "CONFIRMED", "origin": "introduced", "message": "The new line says the handle holds each committed record once, but a default-policy handle's decode cache keeps a second decoded entry of every committed record (12 of 12)"},
  {"file": "docs/ess/evidence/provider-tracking/model-heap-issue-59.txt", "line": 25, "category": "acceptance", "severity": "note", "verdict": "INFEASIBLE", "origin": "introduced", "message": "The acceptance asks for live heap per event, but the probe measures VmRSS resident peak, so the 0.5 threshold is shown only for resident memory; live heap cannot be measured here because unsafe_code is forbidden and release builds were barred"}
]
```
