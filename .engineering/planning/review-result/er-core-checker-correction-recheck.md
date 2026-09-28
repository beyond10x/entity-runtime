---
format: aep.planning-md/3
id: review-result:er-core-checker-correction-recheck
kind: review-result
status: active
title: Core and checker correction adversary recheck
relations:
- reviews: story:er-core-conformance-integration
revision: 1
---
unit: ER core/checker bounded correction recheck; integration working tree on44c14c05, suite78b96338769bd75421bf8544c6c5ec730e57419ca05b4899d3a29f89c3eb8962
verdict: nothing found in the requested correction recheck
cases: review probes executed2→3, red0; intentional Rehydrate mutant185 executed with184 passed/1 failed
origin: introduced0 / pre-existing0 / undecided0
wrote-outside-worktree: assigned EVIDENCE_CACHE/er-ess-extension/er-review paths below
needs-coordinator: none

Reviewer source diff: none. This recheck changed no file in the integration worktree. Every mutation was made inside an assigned cache fixture or the newly written Rust test target.

The root coordinator requested rechecks of stale implementation identity, successful core event rehydration and assertion removal without a scenario-ID change. The source was read before the probes: build.rs compiles the shared source digest and tracks input files; run compares the actual source digest before constructing the target and appends the executable hash; coverage/2 compares full serialized scenario contract digests in addition to IDs.

## Outcomes

| Correction | Executable observation |
|---|---|
| Stale source identity | The preserved current checker successfully ran the unmodified copied source root as a positive control. Inserting compile_error! into its copied core number.rs then caused exit1 with `checker binary is stale for these sources; rebuild with cargo run --locked`. No mutant evidence/report.json was created. |
| Successful Rehydrate | The same always-refusing mutation that formerly survived137/137 core scenarios now executes185 core scenarios and fails exactly `entity.core/authored/legacy-event-rehydration-retains-duplicate-events`:184 passed,1 failed,0 errors,0 unsupported,0 skipped. The test asserts that exact failed scenario name. |
| Same-ID assertion removal | The unmodified copied specification regenerated successfully. Removing only the literal `value` response assertion from exact-number-35 preserved its scenario ID but caused regeneration without a review reason to exit1: `scenario inventory or assertions changed: explicit --coverage-review <reason> required`. The baseline and generated suite files remained byte-identical to their unmodified copies. |

## Commands and observed output

The three tests existed before execution. Command, from the assigned recheck probe directory:

```console
CARGO_TARGET_DIR=EVIDENCE_CACHE/er-ess-extension/er-review/build CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 cargo test --locked --offline -j2 -- --nocapture
```

Exit0, full output retained in recheck.log:

```text
running 3 tests
rehydrate mutant counts ScenarioCounts { total: 185, passed: 184, failed: 1, error: 0, unsupported: 0, skipped: 0 }; failures ["entity.core/authored/legacy-event-rehydration-retains-duplicate-events"]
test tests::the_core_suite_rejects_an_always_refusing_rehydrate ... ok
test tests::removing_an_assertion_requires_review_even_when_the_scenario_id_stays ... ok
test tests::a_stale_checker_cannot_claim_new_source_as_its_implementation ... ok
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 12.68s
```

The checker invocations inside these tests use `--root <assigned fixture>` and respectively `run --scenario entity.core/authored/exact-number-35 --reports <fixture evidence>` and `regenerate`. Mutated inputs and exact runner reports remain in those fixture directories.

## Covered identity

- Current preserved checker SHA256:73320945619082e1d06d3e23531c9c7859310202f5416eedf972c06b5165d4a0.
- Full390-scenario suite SHA256:78b96338769bd75421bf8544c6c5ec730e57419ca05b4899d3a29f89c3eb8962.
- Coverage baseline SHA256:d30694ffab81e3fda05dfe2559ce1043a73e5670b1222e626b8da3f19c823dbf.
- identity.rs SHA256:c7d908cfaeec0db8bfed16128a6c08236a4056129e8a69550153bf44e954aa5e.
- build.rs SHA256:06c05cc9696cac18de1232b67704464a9f6ea7792579b2dc98d5c1573fd66c8f.
- main.rs SHA256:ffdc5b5710b32e9e4362c75540744ec74c2382f932e697b2f5fa308aef75f5fe.

This is a bounded correction recheck, not a claim that all possible core behavior or checker threats have been explored. Root's full390-scenario execution was not repeated here; the real core adapter executed all185 core scenarios under the intentional Rehydrate mutation, with the exact single failure described above.

## External paths written

- EVIDENCE_CACHE/er-ess-extension/er-review/recheck-report.md
- EVIDENCE_CACHE/er-ess-extension/er-review/rechecked-checker
- EVIDENCE_CACHE/er-ess-extension/er-review/recheck/Cargo.toml
- EVIDENCE_CACHE/er-ess-extension/er-review/recheck/Cargo.lock
- EVIDENCE_CACHE/er-ess-extension/er-review/recheck/src/lib.rs
- EVIDENCE_CACHE/er-ess-extension/er-review/recheck.log
- EVIDENCE_CACHE/er-ess-extension/er-review/build/ (existing assigned build cache)
- EVIDENCE_CACHE/er-ess-extension/er-review/identity-recheck-fixture/ (copied source, changed number.rs, baseline run evidence)
- EVIDENCE_CACHE/er-ess-extension/er-review/coverage-recheck-fixture/ (copied ESS specification with one removed assertion)
- EVIDENCE_CACHE/er-ess-extension/er-review/stale-checker-recheck.stdout
- EVIDENCE_CACHE/er-ess-extension/er-review/stale-checker-recheck.stderr
- EVIDENCE_CACHE/er-ess-extension/er-review/coverage-review-recheck.stderr
- EVIDENCE_CACHE/er-ess-extension/er-review/rehydrate-mutant-recheck-report.json

```findings
[]
```
