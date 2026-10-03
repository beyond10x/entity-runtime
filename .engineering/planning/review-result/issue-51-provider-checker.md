---
format: aep.planning-md/3
id: review-result:issue-51-provider-checker
kind: review-result
status: active
title: Real provider conformance and negative controls
relations:
- reviews: story:bounded-batch-and-facade-reads
revision: 1
---
Provider conformance checker verification — issue 51

The eventlog-enabled checker compiled without adapter or compiler changes. The separate provider contract was generated with the explicit coverage-review reason: “Issue 51: admit seven real SQLite and public facade scenarios for exact state/history semantics and five SQL corruptions with unchanged event heads; retain the original 421-scenario contract unchanged.”

Commands used Rust 1.91, CARGO_BUILD_JOBS=2, CARGO_PROFILE_DEV_DEBUG=0 and the temporary `target/issue-wave-scratch/provider-paths.toml` patch to the five local Eventlog crates. Build command: `cargo +1.91.0 --config target/issue-wave-scratch/provider-paths.toml build --manifest-path checks/ess-conformance/Cargo.toml --features eventlog`. First build exited 0 in 37.74 seconds. Checker formatting check exited 0. No executable or test source was changed in this task.

`er-ess-conformance --spec-root ess/provider-tracking check` ran three times with identical report-document counts: total 17, passed 17, failed 0, error 0, unsupported 0, skipped 0. Each report declares complete coverage: seven authored scenarios and ten generated command-response scenarios. The generated command-response cases are not represented as extra semantic coverage.

The seven authored scenarios executed:

- multi-subject-histories-one-scope: actual history order, duplicated subjects, missing subject, exact IDs/revisions, Closed state and revision 2 after a multi-subject batch; full-reopen comparison.
- unchanged-capture-retains-verified-state: exact records, revisions and Closed state before and after repeated snapshots and full reopen.
- unchanged-head-blob-tamper: one SQL row changed, event head unchanged, warm read and reopen refuse ProviderIntegrity.
- unchanged-head-delete-blob-tamper: one blob deleted, event head unchanged, warm read and reopen refuse ProviderIntegrity.
- unchanged-head-event-tamper: one event changed, event head unchanged, warm read and reopen refuse ProviderIntegrity.
- unchanged-head-identity-tamper: identity changed, event head unchanged, warm read and reopen refuse ProviderIntegrity.
- unchanged-head-projection-tamper: projection changed, event head unchanged, warm read and reopen refuse ProviderIntegrity.

The primary provider report is `target/provider-ess-conformance/run.json`. Repeated reports are in this scratch directory under `repeat-2` and `repeat-3`. The original five-library contract was also executed with the eventlog-enabled checker: 421 passed, no failures/errors/skips. SHA-256 checks proved `ess/coverage.json`, `ess/generated/model.json` and `ess/generated/suite.json` remained byte-for-byte unchanged.

Negative controls were made only in scratch copies of the provider specification, leaving the actual semantic assertions intact:

1. Keep the provider domain and commands but remove CapturePolicy. Regeneration exits 1 with `provider specification must declare CapturePolicy`.
2. Change only the expected returned lifecycle state from Closed to Open in `unchanged-capture-retains-verified-state`; keep the actual definition transition and commands unchanged. The real provider still returns Closed. The run exits 1, with 16 passed and exactly that authored scenario failed. Both Load checks report `ESS-CF-PAYLOAD`: `response field value differs from its declared literal`. This is an assertion negative control, not a production-source mutation claim.

New generated files are `ess/provider-tracking/coverage.json`, `ess/provider-tracking/generated/model.json` and `ess/provider-tracking/generated/suite.json`. No AEP or runtime file was edited.

The checker lock initially lacked the optional provider dependency graph. The local path build added 413 lines and removed one. Its exact provisional diff is retained as `local-lock.patch`, with added package names in `added-packages.txt`; additions include entity-eventlog, eventlog-core, eventlog-sqlite, rusqlite 0.40.2, libsqlite3-sys, time, tokio and their transitive dependencies. No new manifest dependency was needed. The checker lock was restored from its pre-build copy, and the workspace lock remained byte-for-byte unchanged. Final Git pins and lock regeneration belong to the coordinator.

Provider source fingerprints matched before and after checks. Original build logs contain private absolute checkout paths and remain scratch-only; this report contains no personal paths. The coordinator subsequently released runtime source for test-wrapper changes, so these results identify the pre-wrapper, locally patched binary recorded in `executable.sha256`, not a final published-pin gate. The pinned build and repository final checks must be rerun after those changes. No CI-image execution was performed in this bounded task.

```findings
[]
```
