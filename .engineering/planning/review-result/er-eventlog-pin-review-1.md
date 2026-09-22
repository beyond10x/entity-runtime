---
format: aep.planning-md/1
id: review-result:er-eventlog-pin-review-1
kind: review-result
status: active
title: 'Independent verification pass 1: one git URL, one rev, and what the pin actually buys a caller'
relations:
- reviews: task:eventlog-provider-pin-verify-once
revision: 2
---
# Independent verification pass 1 — task:eventlog-provider-pin-verify-once

```
unit: task:eventlog-provider-pin-verify-once — commit 4ce78c11bf357e9cf4f6ef9bd8ea45dba595a0d0, worktree ~/.local/state/worktree/trees/b10x/entity-runtime/ess-evolution-er-eventlog-pin-review-1-20260921 (detached, no tracked file modified)
verdict: red
cases: executed 634→640, red 1
origin: introduced 1, pre-existing 1, undecided 1
wrote-outside-worktree: 3 roots — ~/.cache/ess-wave-v2/er1r1/tmp/ (logs), ~/.cache/ess-wave-v2/er1r1/base/ (git archive of 8b175736 + my two test files + its own target/), and this report; full list in §7
needs-coordinator: yes
```

**The pin itself is clean.** Invariants 1, 2 and 3-first-half hold, measured and not read. The two
findings are one level out from the manifest edit: the gate still cannot see the class of defect
this unit hit on its first attempt, and one sentence of the CHANGELOG entry the unit wrote claims a
caller-visible effect that does not appear when it is measured.

## 1. The bound — `git status --porcelain`

```console
$ cd ~/.local/state/worktree/trees/b10x/entity-runtime/ess-evolution-er-eventlog-pin-review-1-20260921
$ git --no-pager diff --stat
$ git status --porcelain
?? crates/entity-eventlog/tests/pin_review_verify_once.rs
?? crates/entity-xtask/tests/
```

`diff --stat` is empty because both files I wrote are new and I ran no `git add`. `git status
--porcelain` is therefore the proof: two paths, both under a `tests/` directory. No implementation
file, no manifest, no lockfile, no document, no conformance exercise was touched, and no git write
command was run at any point.

## 2. The cases I added, each run alone before any suite

All six were written before anything was run, and each was run alone in the order below.
`TMPDIR=~/.cache/ess-wave-v2/er1r1/tmp`, `CARGO_NET_OFFLINE=true`, build dir
`<worktree>/target`, `CARGO_TARGET_DIR` never set.

| case | file:line | asserts | now |
|---|---|---|---|
| A | `crates/entity-eventlog/tests/pin_review_verify_once.rs:200` | a handle held open while a second facade commits observes the extended history | green |
| B | `…/pin_review_verify_once.rs:235` | a read through an open handle refuses a history whose committed frame was damaged after `open` | green |
| C | `…/pin_review_verify_once.rs:266` | an append refuses onto a history the handle can no longer validate, or the store it wrote into still opens | green |
| D | `…/pin_review_verify_once.rs:298` | a read refuses a record whose blob bytes were altered after `open`, and the refusal names the integrity failure | green |
| E | `…/pin_review_verify_once.rs:341` | repeated reads through one open handle over a 48-record store stay exact, and cost what they cost (prints the number) | green |
| F | `crates/entity-xtask/tests/gate_sees_pinned_git_features.rs:133` | every feature that binds a pinned git dependency is compiled by a gate step | **red** |

Cases A–E are the brief's invariant-4 probes (invariant 4 below). Case F is invariant 3's second
half. Verbatim, in the order run:

```console
$ cargo test -p entity-eventlog --features sync-bridge,file --test pin_review_verify_once -- --exact an_open_facade_observes_the_history_a_second_facade_committed
running 1 test
test an_open_facade_observes_the_history_a_second_facade_committed ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 2 filtered out; finished in 4.96s

$ cargo test … -- --exact an_open_facade_does_not_serve_state_whose_committed_frame_was_damaged_after_open
running 1 test
test an_open_facade_does_not_serve_state_whose_committed_frame_was_damaged_after_open ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 2 filtered out; finished in 0.26s
EXIT=0

$ cargo test … -- --exact an_open_facade_refuses_to_commit_onto_a_history_it_can_no_longer_validate
running 1 test
test an_open_facade_refuses_to_commit_onto_a_history_it_can_no_longer_validate ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 2 filtered out; finished in 0.25s
EXIT=0

$ cargo test … -- --exact an_open_facade_does_not_serve_a_record_whose_blob_was_altered_after_open
running 1 test
test an_open_facade_does_not_serve_a_record_whose_blob_was_altered_after_open ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 3 filtered out; finished in 0.59s
EXIT=0

$ cargo test … -- --exact --nocapture repeated_reads_through_one_open_facade_stay_exact_over_a_populated_store
running 1 test
pin-review timing: 48 commits in 29.973759376s, open in 446.695003ms, 24 reads in 5.525111705s (230.212987ms per read)
test repeated_reads_through_one_open_facade_stay_exact_over_a_populated_store ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 4 filtered out; finished in 36.05s
EXIT=0
```

Case F, the red one. Its **first** run reported two of the three uncovered configurations: my
`[features]` parse stopped at the first line and `entity-postgres` writes its feature body across
several lines, so that manifest read clean when it is not. I corrected my own parse and ran it alone
again, before any suite; both runs are below, the second is the one the finding rests on.

```console
$ cargo test --locked -p entity-xtask --test gate_sees_pinned_git_features        # first run, my parse bug
thread 'the_gate_compiles_every_feature_that_binds_a_pinned_git_dependency' panicked at crates/entity-xtask/tests/gate_sees_pinned_git_features.rs:116:5:
every configuration that binds a pinned git dependency must be compiled by a gate step, because a split pin in one of them is invisible to `task check` and was exactly this unit's first outcome; uncovered: ["entity-sqlite --features eventlog-facade", "entity-cli --features eventlog-providers"]
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
EXIT=101

$ cargo test --locked -p entity-xtask --test gate_sees_pinned_git_features        # after correcting the multi-line body parse
running 1 test
test the_gate_compiles_every_feature_that_binds_a_pinned_git_dependency ... FAILED

---- the_gate_compiles_every_feature_that_binds_a_pinned_git_dependency stdout ----
thread 'the_gate_compiles_every_feature_that_binds_a_pinned_git_dependency' (2782458) panicked at crates/entity-xtask/tests/gate_sees_pinned_git_features.rs:133:5:
every configuration that binds a pinned git dependency must be compiled by a gate step, because a split pin in one of them is invisible to `task check` and was exactly this unit's first outcome; uncovered: ["entity-sqlite --features eventlog-facade", "entity-postgres --features eventlog-facade", "entity-cli --features eventlog-providers"]
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

failures:
    the_gate_compiles_every_feature_that_binds_a_pinned_git_dependency

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
error: test failed, to rerun pass `-p entity-xtask --test gate_sees_pinned_git_features`
EXIT=101
```

The three it names are exactly the three the implementor measured at E0308 under the split pin
(implementation-report §3). The case derives them from the manifests; it does not carry a list.

### Origin, settled by running at the base and never by moving the tree

`git archive 8b175736 | tar -x -C ~/.cache/ess-wave-v2/er1r1/base`, both test files copied
in, nothing else changed. No `checkout`, `switch`, `stash`, `branch` or `worktree` command was run.

```console
$ cd ~/.cache/ess-wave-v2/er1r1/base && cargo test --locked -p entity-xtask --test gate_sees_pinned_git_features
thread 'the_gate_compiles_every_feature_that_binds_a_pinned_git_dependency' (2797704) panicked at crates/entity-xtask/tests/gate_sees_pinned_git_features.rs:133:5:
… uncovered: ["entity-sqlite --features eventlog-facade", "entity-postgres --features eventlog-facade", "entity-cli --features eventlog-providers"]
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
EXIT=101

$ cd ~/.cache/ess-wave-v2/er1r1/base && cargo test -p entity-eventlog --features sync-bridge,file --test pin_review_verify_once -- --nocapture
running 5 tests
test an_open_facade_refuses_to_commit_onto_a_history_it_can_no_longer_validate ... ok
test an_open_facade_does_not_serve_state_whose_committed_frame_was_damaged_after_open ... ok
test an_open_facade_does_not_serve_a_record_whose_blob_was_altered_after_open ... ok
test an_open_facade_observes_the_history_a_second_facade_committed ... ok
pin-review timing: 48 commits in 74.864906434s, open in 534.873171ms, 24 reads in 5.993878867s (249.744952ms per read)
test repeated_reads_through_one_open_facade_stay_exact_over_a_populated_store ... ok
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 82.16s
EXIT=0
```

Case F is red at the base as well: **`pre-existing`**. Cases A–E are green at both revisions.

## 3. The suite, after the cases existed

Both gate test lanes. The eventlog lane ran once cases A–E existed; the workspace lane ran once all
six existed. Neither ran before a case of mine existed, and the `<before>` numbers are the
implementing state's own (`executed 546→546` for the workspace lane; 88 for the 1.91 lane,
`unit-4-er-eventlog-pin/gate-4manifest/01-task-check.log`).

```console
$ cargo +1.91.0 test -p entity-eventlog --all-features --locked            # gate step eventlog-runtime-check
     Running unittests src/lib.rs
test result: ok. 36 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.58s
     Running tests/fault_acceptance.rs
test result: ok. 25 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.23s
     Running tests/pin_review_verify_once.rs
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 42.45s
     Running tests/provider_facades.rs
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.57s
     Running tests/providers.rs
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.19s
     Running tests/reviewer_binding_unknown_conflict.rs
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
     Running tests/service_3_composition.rs
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.34s
   Doc-tests entity_eventlog
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
EXIT=0                                                                       # 88 → 93, red 0

$ cargo +1.91.0 clippy -p entity-eventlog --all-targets --all-features --no-deps --locked -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 11.22s
EXIT=0                                                                       # my file is clippy-clean under -D warnings

$ cargo test --workspace --locked --no-fail-fast                             # gate step test
86 `test result:` lines (84 before; my two new lanes are the other two)
passed=546 failed=1
test the_gate_compiles_every_feature_that_binds_a_pinned_git_dependency ... FAILED
EXIT=101                                                                     # 546 → 547, red 1
```

Every one of the unit's 634 existing cases passes. The single red lane is the one I added, and a red
suite here is the successful outcome.

## 4. Invariant by invariant

### 1 — One URL, one rev. **Holds.**

Run, not hand-read:

```console
$ grep -rn "beyond10x/eventlog" --include="*.toml" --include="Cargo.lock" . | grep -v ./target/
Cargo.lock:601,614,627,644                       → rev=c698923038de4413e0bbba3cd91ae108607d6be9
crates/entity-eventlog/Cargo.toml:22,23,24,25    → c698923038de4413e0bbba3cd91ae108607d6be9
crates/entity-sqlite/Cargo.toml:19               → c698923038de4413e0bbba3cd91ae108607d6be9
crates/entity-postgres/Cargo.toml:30,31          → c698923038de4413e0bbba3cd91ae108607d6be9
crates/entity-cli/Cargo.toml:31                  → c698923038de4413e0bbba3cd91ae108607d6be9
$ git grep -n f802eb8
(no output)
$ grep -n 'source = "git+' Cargo.lock | sed 's/#.*//' | sort -u
(one distinct line, c698923)
$ find . -name "*.toml" -not -path "./target/*" | xargs grep -ln "git = "
./crates/entity-eventlog/Cargo.toml  ./crates/entity-sqlite/Cargo.toml  ./crates/entity-postgres/Cargo.toml  ./crates/entity-cli/Cargo.toml
```

Eight `rev =` sites across four manifests, all c698923; `eventlog` is the only git URL in the
workspace; `f802eb8` appears in no tracked file. (The review brief says *seven* sites; the diff moves
eight — `entity-eventlog` 4, `entity-postgres` 2, `entity-sqlite` 1, `entity-cli` 1. The change is
right and the brief's count is off by one; noted as N3 so the record is not carried forward wrong.)

### 2 — Nothing else moved. **Holds.**

```console
$ git --no-pager diff 8b175736 4ce78c11 -- Cargo.lock | grep "^[+-][^+-]" | sed 's/=.*//' | sort | uniq -c
      4 +source
      4 -source
$ git show 8b175736:Cargo.lock | grep -E "^name = |^version = " | md5sum   → b587af62c56b817fe2df825c61550304
$ git show 4ce78c11:Cargo.lock | grep -E "^name = |^version = " | md5sum   → b587af62c56b817fe2df825c61550304
$ cargo metadata --locked --offline --format-version 1            → EXIT 0
$ cargo +1.91.0 metadata --locked --offline --format-version 1    → EXIT 0
```

Four changed lines, all `source =`. The package/version inventory of the lockfile is byte-identical
across the two commits, so no entry was added, removed or renumbered, and the deliberate
`tempfile` → `getrandom` revert is consistent under both toolchains.

### 3 — The three feature builds. **First half holds; second half is finding F1.**

```console
$ cargo check --locked -p entity-cli --features eventlog-providers      → EXIT 0
$ cargo check --locked -p entity-sqlite --features eventlog-facade      → EXIT 0
$ cargo check --locked -p entity-postgres --features eventlog-facade    → EXIT 0
```

All three compile at this commit. The gate cannot see them: no cargo line in `Taskfile.yml` enables
`eventlog-facade` or `eventlog-providers`, and `.github/workflows/gate.yml` carries the same step
list (`gate.yml:55,58,61,66,70-72,97,132,135` — read, not run; `--all-features` appears only under
`-p entity-eventlog`, which is the one crate that was never broken). Case F is the failing case, §2.

### 4 — Does the adapter expose a caller to the provider's post-open behaviour? **It contains it.**

The four probes the brief names, run as cases A–D against `EventlogFileStore`, which holds one
`FileEventStore` open on its worker thread for the life of the handle:

| probe | case | result |
|---|---|---|
| a second handle extending the history | A | the open handle observes the extended history; `ids` returns both records |
| a committed frame damaged in place after `open` | B | the read **refuses**; the control (a fresh handle cannot open the damaged store) passes first, so the damage is real |
| an append after the store's bytes changed underneath an open handle | C | the disjunction holds — the facade does not report a commit into a store no later handle can open |
| a blob altered after `open` | D | the read **refuses**, and the refusal names the integrity failure (`integrity`/`corrupt`/`digest`/`blob`) |

So the three findings open against `eventlog-file` (unit-3 F1/F2/F3) do not reach a runtime caller
through this adapter in these four shapes, at this pin. Not re-reported, and not claimed as fixed —
the adapter asks the provider for a complete tenant capture and verifies every referenced blob
against its domain-framed digest itself (`crates/entity-eventlog/src/adapter.rs:2041-2092`,
`get_bound_blob`/`verify_digest` at `:2528`/`:2521`), which is the mechanism that refuses in B and D.
What I measured is the adapter's behaviour, not the provider's.

### 5 — The CHANGELOG entry. **One Unreleased `### Changed` item, correctly placed; one sentence is finding F2.**

`CHANGELOG.md:76-83`, under `## [Unreleased]` (line 5) → `### Changed` (line 74). It names the four
crates, the new rev, and the single-`eventlog-core` consequence — all three of which the diff and
§4.1/§4.3 support. `CHANGELOG.md:80-81` claims something else:

> A caller making repeated reads against an already-open store no longer pays whole-store
> verification per transaction

Case E measures exactly that sentence — 48 records committed, the store reopened once, 24 reads
through the one open handle — at both revisions, interleaved, twice:

| | 48 commits | open | per read |
|---|---|---|---|
| **c698923 (head), pair 1** | 29.97 s | 447 ms | **230.2 ms** |
| **f802eb8 (base), pair 1** | 74.86 s | 535 ms | **249.7 ms** |
| **c698923 (head), pair 2** | 53.22 s | 459 ms | **245.2 ms** |
| **f802eb8 (base), pair 2** | 65.54 s | 465 ms | **237.2 ms** |

`/proc/loadavg` 9.45 at the end of pair 2, so the pairs are interleaved rather than compared across
time. The commit path is 2.5×/1.2× faster and that is the effect the pin buys. The **read** path —
the one the sentence names — moves 230↔250 ms per read with the base ahead in one of the two pairs:
no effect, at the one store size where the claim could show. This is the same *no change* the
implementor measured on the crate's own lane (implementation-report §5) and flagged as unmeasurable
there; case E is that measurement, at a store size that discriminates.

## 5. Findings

The commit is 4ce78c11bf357e9cf4f6ef9bd8ea45dba595a0d0; the base compared against is 8b175736.

### F1 — the gate compiles no configuration that binds the pinned git dependency

| | |
|---|---|
| **what was measured** | `crates/entity-xtask/tests/gate_sees_pinned_git_features.rs:133`, exit 101, naming `entity-sqlite --features eventlog-facade`, `entity-postgres --features eventlog-facade`, `entity-cli --features eventlog-providers` |
| **what reaches it** | `task check` and `.github/workflows/gate.yml` are the documented gate, and the state was actually reached: this unit's first implementation left one manifest at the old rev, cargo resolved two `eventlog-core` crates, those three configurations failed with E0308 — and all ten gate steps exited 0 (implementation-report §0, §3, §4) |
| **verdict / origin** | CONFIRMED / `pre-existing` — red at 8b175736 too |
| **the correction, named not applied** | the coordinator already holds it unapplied: `~/.cache/ess-wave-v2/er1/tmp/coordinator/check-git-rev-uniformity.py` + `Taskfile-rev-check.patch`. A `rev-check` step closes the narrow class (one URL, one rev); a step that *builds* the three configurations closes the broad one (any regression behind those features). My case asserts the broad one and goes green under either of the two, provided the built configurations appear as cargo lines in `Taskfile.yml` |

### F2 — the CHANGELOG entry claims a read-path benefit that does not appear

| | |
|---|---|
| **what was measured** | `crates/entity-eventlog/tests/pin_review_verify_once.rs:341` run at both revisions, twice interleaved: per-read cost through one open handle over a 48-record store is 230.2/245.2 ms at c698923 and 249.7/237.2 ms at f802eb8, while the same runs show 48 commits at 29.97/53.22 s against 74.86/65.54 s |
| **what reaches it** | `CHANGELOG.md:80-81` is the released record of what a user sees; `scripts/changelog-section.py` (gate step `notes-check`) publishes this section as release notes. A reader taking the pin for a read-heavy workload is the consumer |
| **verdict / origin** | CONFIRMED / `introduced` — the sentence is in this diff |
| **the correction, named not applied** | name the path the measurement supports: the whole-store re-verification that is no longer paid per transaction shows on **commits**, not on reads through an already-open store. Either reword to the write path or drop the caller-benefit sentence and keep the factual half (`verifies history and blobs once per open and re-checks only what a transaction changed`), which the provider's own design page supports |

### N3 — the review brief's site count is one short

| | |
|---|---|
| **what was measured** | `git --no-pager diff 8b175736 4ce78c11` moves eight `rev =` lines (4 + 2 + 1 + 1); `brief-review-1.md:37` says seven |
| **what reaches it** | nothing in the repository — a coordinator document only. The change is right; the count that describes it is not |
| **verdict / origin** | CONFIRMED / `undecided` — the document is outside the repository and has no base revision to run against |

## 6. Reviewed and could not fault

- The seven-then-eight `rev =` edits and the four `Cargo.lock` `source =` lines: textually exact, no
  other package entry, version or dependency edge moved (§4.2, md5 of the lock's name/version
  inventory identical across the two commits).
- The lockfile's consistency under both toolchains, i.e. the deliberate `tempfile` → `getrandom`
  revert: `cargo metadata --locked --offline` exits 0 under stable and under 1.91.0.
- The three previously broken feature configurations: all three exit 0 at this commit.
- The adapter's containment of post-open provider behaviour in all four shapes the brief names
  (cases A–D green here **and** at the base, so the pin neither introduces nor repairs an exposure).
- `entity-eventlog`'s 88 existing cases and the workspace's 546: unchanged and green, with no
  assertion weakened, skipped or rewritten anywhere in the diff (no source file is in it).
- The CHANGELOG entry's placement, count and every claim in it other than the one sentence in F2.
- The unit's scope discipline: `git status --porcelain` at 4ce78c11 is clean, and the commit touches
  six files, all of them manifests, the lock, or the CHANGELOG.

## 7. Every path written outside the worktree

All under the assigned scratch `~/.cache/ess-wave-v2/er1r1/`; nothing in `/tmp`.

- `tmp/case-a-head.log`, `tmp/case-b-head.log`, `tmp/case-c-head.log`, `tmp/case-d-head.log`,
  `tmp/case-e-head.log` — cases A–E run alone at 4ce78c11
- `tmp/case-f-head.log`, `tmp/case-f-head-2.log` — case F run alone, before and after my parse fix
- `tmp/case-f-base.log`, `tmp/cases-abcde-base.log` — the same cases at 8b175736
- `tmp/suite-eventlog-lane.log`, `tmp/clippy-eventlog-lane.log`, `tmp/suite-workspace.log`,
  `tmp/suite-workspace-nff.log` — the suite runs of §3
- `tmp/feature-entity-cli.log`, `tmp/feature-entity-sqlite.log`, `tmp/feature-entity-postgres.log`,
  `tmp/meta-stable.err`, `tmp/meta-191.err` — invariants 2 and 3
- `base/` — `git archive 8b175736` extraction plus my two test files, **and its own `target/`**
  (~12 GiB; `/` avail went 73G → 49G across this pass, floor 20G never approached). It is the origin
  evidence; the coordinator may reclaim it once this report is recorded. I removed no build
  directory and no worktree.

**Outside scratch:** this report, at the brief's `report:` path.

Lease `ess-v2-er1r1-review` on the review worktree: `session-start` taken, heartbeated,
`session-end` released before returning. No other session's lease was touched. The worktree and its
`target/` are left in place.

**For the coordinator, beyond the findings:** case E costs 36–42 s of wall time in the
`eventlog-runtime-check` lane (18 s → ~55 s). It is the only thing in the repository that measures
the CHANGELOG's caller claim, and it is also the only case here that is slow; keeping it, trimming
`RECORDS`/`READS`, or dropping it once F2 is settled is a call I do not get to make.

```findings
- file: Taskfile.yml
  category: mutant
  severity: warning
  verdict: CONFIRMED
  origin: pre-existing
  message: no gate step compiles entity-sqlite --features eventlog-facade, entity-postgres --features eventlog-facade or entity-cli --features eventlog-providers, so a split pin or any other regression behind those features passes a green `task check` — which is exactly what this unit's first implementation did.
- file: CHANGELOG.md
  line: 80
  category: contract-drift
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: the entry claims a caller making repeated reads against an already-open store no longer pays whole-store verification per transaction, but measured at both revisions the per-read cost is 230/245 ms at c698923 against 250/237 ms at f802eb8 — the effect the pin buys is on commits (30/53 s against 75/66 s), not on reads.
- file: ~/beyond10x/.ess-evolution/waves/0005-aep-migration/wave-validate-v2-20260920/unit-4-er-eventlog-pin/brief-review-1.md
  line: 37
  category: judgement
  severity: note
  verdict: CONFIRMED
  origin: undecided
  message: the brief says seven `rev =` sites move; the diff moves eight (entity-eventlog 4, entity-postgres 2, entity-sqlite 1, entity-cli 1) — the change is right and the count describing it is not.
```
