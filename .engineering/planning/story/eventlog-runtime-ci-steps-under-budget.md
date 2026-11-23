---
format: aep.planning-md/3
id: story:eventlog-runtime-ci-steps-under-budget
kind: story
status: draft
title: The Eventlog runtime CI job's test steps end within 120 s
summary: 'Forge: provider contracts step 156 s on main e21de938; local gate: eventlog-runtime-test 391 s, provider-ess-run 303 s'
owner: entity-runtime
relations:
- informed_by: story:entity-eventlog-tests-under-budget
revision: 1
---
## Outcome

Every step of the CI job "Eventlog runtime (Rust 1.91)" that runs tests or conformance finishes
within 120 s, locally and on the forge, and the step fails when it runs longer.

## Finding

Test budget defect (`~/beyond10x/AGENTS.md` "Tests stay fast": the test step of a gate ends within
2 minutes; a step over the budget is a defect). Found while gating #93 and #94; no review finding
id.

## Evidence

- Forge, `main` at `e21de938`, CI run 45110829804, job "Eventlog runtime (Rust 1.91)"
  (`.github/workflows/gate.yml:118`):
  - "Executable SQLite provider contracts" 04:20:05 -> 04:22:41 UTC, 156 s (provider ESS clippy
    plus `cargo run ... --features eventlog -- --spec-root ess/provider-tracking check`);
  - "Test every Eventlog provider and bridge" 04:18:45 -> 04:19:46 UTC, 61 s with a warm cache.
- Local integration gate of #93 (`er-integ/out/summary.txt`, host under load):
  `eventlog-runtime-test` (`cargo +1.91.0 test -p entity-eventlog --all-features --locked`) 391.1 s,
  `provider-ess-clippy` 134.6 s, `provider-ess-run` 303.3 s.
- er-g measured the pre-fix `entity-eventlog` step at 1,325 s (`assignment.md` line 53); er-d2 cut
  the `batch_cost` tests from 268 + 875 s to 8.6 s (`e09db8b9`, line 102). The per-test defects are
  the story `entity-eventlog-tests-under-budget`.
- The workflow has no step timeout under the job's `timeout-minutes: 30`.

## Acceptance

On a CI run of `main` after the change, the steps "Test every Eventlog provider and bridge" and
"Executable SQLite provider contracts" (or the steps that replace them) each take at most 120 s,
and each carries a bound that fails it past 120 s.
