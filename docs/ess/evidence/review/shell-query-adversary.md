unit: story:er-shell-query-contracts; er-ess-contracts-integration working tree based on 44c14c05f68025085f4a3a89b6c9f5e544f7921e
verdict: nothing found
cases: executed 58→60, red 0
origin: introduced 0 / pre-existing 0 / undecided 0
wrote-outside-worktree: 14 paths, all in the assigned managed worker tree or assigned cache subtree
needs-coordinator: none

```text
 .../ess-conformance/tests/shell_query_adversary.rs | 111 +++++++++++++++++++++
 1 file changed, 111 insertions(+)
```

This is the adversary-only diff, obtained with git diff --no-index --stat /dev/null. The new Rust test imports the root integration adapters by absolute #[path] and changes no adapter or production source. The production shell/query/memory/file source digests in the worker match root exactly. No root writes or AEP commands were made.

The baseline 58 was supplied by the coordinator. Two independent targeted cases were written before any unit execution and each ran alone before the authoritative suite. Both first executions were green:

```text
running 1 test
test shell_record_identity_collision_cannot_recover_another_subject ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 2 filtered out; finished in 1.21s

```

Exit 0. shell_record_identity_collision_cannot_recover_another_subject runs against both MemoryStore and FileStore: after one subject records an operation, another subject reusing that record identity receives concrete RecordConflict/store classification. Independent Get, Records and Events reads prove the second subject remains revision 1 with its original fields, one creation record and no events. A fresh record identity subsequently succeeds and changes fields/revision, preventing an always-refuse result from satisfying the probe.

```text
running 1 test
test query_numeric_equivalence_does_not_rebind_cursor_identity ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 2 filtered out; finished in 0.00s

```

Exit 0. query_numeric_equivalence_does_not_rebind_cursor_identity proves the declared distinction between exact numeric containment and serialized query identity. Query data:100 returns the first matching document; continuation with data:100.0 refuses as a foreign query. The original query continues to the second document with a changed limit, and a fresh data:100.0 query returns both matching documents. The probe uses literal wire numbers, not a floating-point adaptation.

The final Rust test binary runs three tests: those two probes, plus a wrapper executing all 58 authoritative shell/query ESS scenarios. Thus the bounded semantic case total is 60; the wrapper is not counted as an additional semantic case. Its selector includes both generated command/outcome IDs and authored domain IDs, with an exact assertion of 58 before runner execution. An initial overly narrow selector was corrected before executing the authoritative suite; the count assertion was retained.

```console
TMPDIR=EVIDENCE_CACHE/er-ess-store-executor/shell-query-review-tmp CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --manifest-path checks/ess-conformance/Cargo.toml --config EVIDENCE_CACHE/er-ess-local.toml -j2 --test shell_query_adversary
```

```text
   Compiling er-ess-conformance v0.1.0 (WORKTREE_ROOT/entity-runtime/er-store-executor-contracts/checks/ess-conformance)
    Finished `test` profile [unoptimized] target(s) in 0.95s
     Running tests/shell_query_adversary.rs (checks/ess-conformance/target/debug/deps/shell_query_adversary-48739bf52c073f7f)

running 3 tests
test query_numeric_equivalence_does_not_rebind_cursor_identity ... ok
test shell_record_identity_collision_cannot_recover_another_subject ... ok
test existing_shell_query_58_scenarios_execute_from_authoritative_suite ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 6.92s

```

Exit 0. The ESS report counts total 58, passed 58, failed 0, error 0, unsupported 0, skipped 0. Selected original suite digest: sha256:a72f956b0d83c77daa35eeef51722f2741b1bee75facff4f34842b1ba0f403f7, ess-conformance/27. The retained input carrier includes the exact parent suite and selection lineage.

No concrete finding from this bounded review. The adapter inspection found actual API calls and actual returned/state/history projections, with fixture controls identified explicitly. Shell Fault delegates successful calls and injects only the configured provider error; Race prepares and commits a real competitor. Query retains an actual returned cursor and invokes the provider again. Neither adapter reads expected values or computes expected answers. Numeric documents remain arbitrary-precision JSON text through adaptation.

Reviewed the acceptance statement, both domain declarations, adapter source, traceability registers, public implementation, and literal scenarios for retry conflicts, stale commit races, complete traversal, cursor refusals, recursive containment and consumed ordered input. This review does not establish completeness of all possible shell/query behavior or replace the integration repository gate.

Exact reviewed source and parent-suite digests:

```text
30f47d495f93889ee4f3b947d92f91278a216631731a609f1e3597c358610d96  checks/ess-conformance/src/shell.rs
d4971217684106eecc346d53727336ffc0e57e4504c9ec1843e71625a2db050a  checks/ess-conformance/src/query.rs
f35c2f397cd7842444afc6377eb2e8c70f6e641d237860e18cf116c5b1354669  checks/ess-conformance/src/common.rs
d169a8e0f05327c7175c1c9197c368618eded4ba71501d682ca84b0c2d324010  crates/entity-shell/src/lib.rs
394403e6f7fe1aa9ebe3daeca1affb94269db3ba0d27d26dc66a94edb3a0db8f  crates/entity-query/src/lib.rs
629ae7d943f718e4d454ef7200ff78460407a8af184eec712fce48ae50d2992e  crates/entity-store/src/memory.rs
05bf010331be86a9b87cf05508fd95f9a083cb44b6acf613ede6d4ad0316076d  crates/entity-store/src/file.rs
74e4a50672108495af65f0ca1009963e51f00b916894260c334314dca53706e6  ess/generated/suite.json
```

All outside-root write paths:

- WORKTREE_ROOT/entity-runtime/er-store-executor-contracts/checks/ess-conformance/tests/shell_query_adversary.rs
- WORKTREE_ROOT/entity-runtime/er-store-executor-contracts/checks/ess-conformance/target
- EVIDENCE_CACHE/er-ess-store-executor/shell-query-review-tmp
- EVIDENCE_CACHE/er-ess-store-executor/evidence/shell-query-adversary-shell-first.log
- EVIDENCE_CACHE/er-ess-store-executor/evidence/shell-query-adversary-query-first.log
- EVIDENCE_CACHE/er-ess-store-executor/evidence/shell-query-adversary-suite.log
- EVIDENCE_CACHE/er-ess-store-executor/evidence/shell-cross-subject-observations.json
- EVIDENCE_CACHE/er-ess-store-executor/evidence/query-numeric-cursor-observations.json
- EVIDENCE_CACHE/er-ess-store-executor/evidence/shell-query-reviewed-input.json
- EVIDENCE_CACHE/er-ess-store-executor/evidence/shell-query-reviewed-suite.json
- EVIDENCE_CACHE/er-ess-store-executor/evidence/shell-query-reviewed-report.json
- EVIDENCE_CACHE/er-ess-store-executor/evidence/shell-query-reviewed-run.json
- EVIDENCE_CACHE/er-ess-store-executor/evidence/shell-query-adversary-identity.txt
- EVIDENCE_CACHE/er-ess-store-executor/evidence/shell-query-adversary-final.md

```findings
[]
```

