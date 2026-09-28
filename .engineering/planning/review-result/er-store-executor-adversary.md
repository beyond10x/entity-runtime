---
format: aep.planning-md/2
id: review-result:er-store-executor-adversary
kind: review-result
status: active
title: Store and executor conformance adversary review
relations:
- reviews: story:er-store-executor-contracts
revision: 1
---
unit: uncommitted store/executor ESS deliverables in er-store-executor-contracts over 44c14c05f68025085f4a3a89b6c9f5e544f7921e
verdict: nothing found
cases: executed 30→32, red 0
origin: introduced 0 / pre-existing 0 / undecided 0
wrote-outside-worktree: 15 files and one build directory, listed below
needs-coordinator: integrate the worker's latest version-filter fixture and preserve runner-produced evidence

`git --no-pager diff --stat` in the reviewed worktree produced no output. No file in that worktree was changed by this review. Its new deliverables remain untracked, as before review.

Two scratch tests were written before their first execution. `kernel_refusal_must_reach_the_supplied_recorder` asserts a literal Kernel reason for a valid request whose operation is absent. A scratch executor copy changed only the `ExecutionError::Core` recorder mapping to `None`. Its first isolated run exited 101:

```text
assertion `left == right` failed: a valid command reaching a kernel refusal must be recorded
  left: Text("[]")
 right: Text("[{\"kind\":\"Kernel\",\"message\":\"operation 'missing' is not defined\"}]")
test tests::kernel_refusal_must_reach_the_supplied_recorder ... FAILED
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 1 filtered out; finished in 0.00s
```

The subsequent current authored executor suite also rejected this mutant through `kernel-refusal-is-recorded.yaml` at `RefusalReasons.value`. This scenario had been added by the worker while review started; it closes the suspected omission.

`authored_projection_rejects_other_definition_versions` reads the worker's projection fixture and compares its literal expected document with production projection code copied to scratch. The original fixture lacked another definition version despite claiming version filtering; the worker added that input after notification. A scratch copy removed only the version guard. Its first isolated run exited 101:

```text
assertion `left == right` failed: authored projection expectation excludes another definition version
  left: "{\"open_titles\":{\"a\":[\"a\"],\"b\":[\"b\"],\"foreign-version\":[\"other-version\"]},\"states\":{\"closed\":[\"c\"],\"open\":[\"a\",\"b\",\"missing\",\"other-version\"]}}"
 right: "{\"open_titles\":{\"a\":[\"a\"],\"b\":[\"b\"]},\"states\":{\"closed\":[\"c\"],\"open\":[\"a\",\"b\",\"missing\"]}}"
test tests::authored_projection_rejects_other_definition_versions ... FAILED
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 2 filtered out; finished in 0.00s
```

Both scratch production copies were restored before the final run. The following command ran the two focused assertions plus all 30 authored executor scenarios, after both cases existed:

```console
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --config EVIDENCE_CACHE/er-ess-local.toml --manifest-path EVIDENCE_CACHE/er-ess-shell-query/review/probe/Cargo.toml --target-dir EVIDENCE_CACHE/er-ess-shell-query/build -j2 -- --nocapture
```

```text
running 3 tests
test tests::authored_projection_rejects_other_definition_versions ... ok
test tests::kernel_refusal_must_reach_the_supplied_recorder ... ok
existing authored executor scenarios executed: 30
test tests::existing_executor_scenarios_survive_the_missing_kernel_recorder_mutant ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s
```

Exit status 0. The 30→32 count counts contract cases, with the original 30 observed in the post-case suite run; the Rust runner groups their loop into one test. This review did not rerun the complete 146-case ESS lane reported by the implementor.

No outstanding judgement findings. Inspected the adapter response paths, provider boundaries, fixture limitations, recovery ordering and refusal mappings. Stored state/history/receipts come from actual APIs. Optional fork/merge ports propagate injected failures without inventing committed success. The supplied refusal-recorder fixture is documented as such, rather than attributed to MemoryRecordedStore.

Every path written outside the reviewed worktree (build contents are reproducible):

- EVIDENCE_CACHE/er-ess-shell-query/review/executor-mutant/Cargo.toml
- EVIDENCE_CACHE/er-ess-shell-query/review/executor-mutant/src/lib.rs
- EVIDENCE_CACHE/er-ess-shell-query/review/probe/Cargo.toml
- EVIDENCE_CACHE/er-ess-shell-query/review/probe/Cargo.lock
- EVIDENCE_CACHE/er-ess-shell-query/review/probe/src/common.rs
- EVIDENCE_CACHE/er-ess-shell-query/review/probe/src/executor.rs
- EVIDENCE_CACHE/er-ess-shell-query/review/probe/src/store.rs
- EVIDENCE_CACHE/er-ess-shell-query/review/probe/src/lib.rs
- EVIDENCE_CACHE/er-ess-shell-query/review/probe/src/projection_mutant.rs
- EVIDENCE_CACHE/er-ess-shell-query/evidence/review-kernel-refusal-red.log
- EVIDENCE_CACHE/er-ess-shell-query/evidence/review-current-suite-mutant.log
- EVIDENCE_CACHE/er-ess-shell-query/evidence/review-restored-green.log
- EVIDENCE_CACHE/er-ess-shell-query/evidence/review-projection-red.log
- EVIDENCE_CACHE/er-ess-shell-query/evidence/review-final-green.log
- EVIDENCE_CACHE/er-ess-shell-query/evidence/store-executor-adversary-report.md
- EVIDENCE_CACHE/er-ess-shell-query/build/

```findings
[]
```
