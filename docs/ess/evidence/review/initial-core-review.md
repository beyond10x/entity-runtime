unit: ER core contracts/shared checker; uncommitted er-ess-contracts-integration on44c14c05
verdict: NEEDS-CHANGE
cases: provided core137 → probe executed2; red2; mutant core execution137/137 passed
origin: introduced2 / pre-existing0 / undecided0
wrote-outside-worktree: assigned EVIDENCE_CACHE/er-ess-extension/er-review only
needs-coordinator: implement corrections and record review; no source changes made by reviewer

Reviewer diff: none. Root source remained untouched; probe test files and mutated fixture are outside the repository in the assigned cache.

| Location | Verdict/origin | What was measured | What reaches it |
|---|---|---|---|
| checks/ess-conformance/src/main.rs:137 | NEEDS-CHANGE / introduced | A copied root with a compile_error! inserted into core number.rs is reported passing1 scenario by the previously built checker. Its runtime hash labels those changed, uncompilable source bytes as the executed implementation. Red test `a_stale_checker_cannot_claim_new_source_as_its_implementation`; retained fixture/evidence and preserved binary. | Public `--root ... run`, including a stale binary after a normal source edit. |
| ess/scenarios/core/rehydrate-rejects-empty-history.yaml | NEEDS-CHANGE / introduced | A target mutation returning validation failure for every Rehydrate invocation passes all137 core scenarios. Red test `the_core_suite_rejects_an_always_refusing_rehydrate`; actual CountReport retained. | Existing real Rehydrate adapter command; R-81/complete core replay acceptance requires successful histories as well as rejection. |

The coordinator accepted these and is implementing corrections. Additional confirmed corpus gaps were communicated as coverage work, not separately measured behavioral defects: no simultaneous assignments reading pre-operation fields (R-41), no service in_state branch (R-141), no service relation declarations (R-144), and no branch-selected derived creation (R-154). The coordinator reports new cases underway. A scenario-ID-only coverage baseline also deserves assertion-removal probing; this was not executed before reassignment and is not claimed as a confirmed finding.

Executed command, after both tests existed:

`CARGO_TARGET_DIR=EVIDENCE_CACHE/er-ess-extension/er-review/build CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 cargo test --offline -j2 -- --nocapture`

```text
running 2 tests
stale binary claimed changed source: {"total":1,"passed":1,"failed":0,"error":0,"unsupported":0,"skipped":0}
test tests::a_stale_checker_cannot_claim_new_source_as_its_implementation ... FAILED
always-refusing event rehydrate survived every core scenario: ScenarioCounts { total: 137, passed: 137, failed: 0, error: 0, unsupported: 0, skipped: 0 }
test tests::the_core_suite_rejects_an_always_refusing_rehydrate ... FAILED
test result: FAILED. 0 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out
```

Exit101. Full output `probe-red.log`. The first run compiled and both assertions failed for the claimed reasons.

External paths:

- EVIDENCE_CACHE/er-ess-extension/er-review/report.md
- EVIDENCE_CACHE/er-ess-extension/er-review/probe/Cargo.toml
- EVIDENCE_CACHE/er-ess-extension/er-review/probe/Cargo.lock
- EVIDENCE_CACHE/er-ess-extension/er-review/probe/src/lib.rs
- EVIDENCE_CACHE/er-ess-extension/er-review/build/
- EVIDENCE_CACHE/er-ess-extension/er-review/old-checker
- EVIDENCE_CACHE/er-ess-extension/er-review/probe-red.log
- EVIDENCE_CACHE/er-ess-extension/er-review/stale-checker.stdout
- EVIDENCE_CACHE/er-ess-extension/er-review/stale-checker.stderr
- EVIDENCE_CACHE/er-ess-extension/er-review/identity-fixture/ (source snapshot, mutated number.rs and exact evidence/ report)
- EVIDENCE_CACHE/er-ess-extension/er-review/rehydrate-mutant-report.json

```findings
- file: checks/ess-conformance/src/main.rs
  line: 137
  category: contract-drift
  severity: blocker
  verdict: NEEDS-CHANGE
  origin: introduced
  message: A previously built checker reports passing conformance with the identity of changed source files containing a compile error.
- file: ess/scenarios/core/rehydrate-rejects-empty-history.yaml
  category: mutant
  severity: blocker
  verdict: NEEDS-CHANGE
  origin: introduced
  message: An always-refusing Rehydrate mutation passes every core scenario because no successful event-only rehydration is asserted.
```
