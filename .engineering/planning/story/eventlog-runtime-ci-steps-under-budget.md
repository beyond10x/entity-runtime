---
format: aep.planning-md/3
id: story:eventlog-runtime-ci-steps-under-budget
kind: story
status: implemented
title: The Eventlog runtime CI job's test steps end within 120 s
summary: 'Forge: provider contracts step 156 s on main e21de938; local gate: eventlog-runtime-test 391 s, provider-ess-run 303 s'
owner: entity-runtime
relations:
- informed_by: story:entity-eventlog-tests-under-budget
- serves: vision:O2
revision: 5
transitions:
- {from: "draft", to: "proposed", at: "2026-11-23T21:53:45Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-23T21:53:45Z", actor: "human:timo", revision: 3}
- {from: "active", to: "implemented", at: "2026-11-24T13:23:36Z", actor: "agent:entity-runtime", revision: 5, decided_on: {"recorded":{"test_result":1}}}
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

## Result

CI run 45277382818 (main `29de5694`, the merge of #103), job "Eventlog runtime (Rust 1.91)":
"Test every Eventlog provider and bridge" 22:16:26 -> 22:16:33 UTC, 7 s; "Executable SQLite
provider contracts" 22:23:11 -> 22:23:12 UTC, 1 s. Each step carries `timeout-minutes: 2`
(`.github/workflows/gate.yml:191` and `:207`), and the test step runs `scripts/test-step.sh`, which
fails it past 120 s. The same two steps on main `f08eda98` (#105, run 45326703964): 6 s and 1 s.
The compile steps before them are separate: "Lint and build the SQLite provider contract checker"
took 332 s on a cold cache in the first run and 32 s in the second.
