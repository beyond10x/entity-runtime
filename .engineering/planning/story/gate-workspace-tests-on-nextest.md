---
format: aep.planning-md/3
id: story:gate-workspace-tests-on-nextest
kind: story
status: draft
title: The Gate job's workspace tests run under nextest, killed at 60 s, under 120 s
summary: The Gate job still runs cargo test --workspace with no 60 s kill
revision: 1
---
## Outcome

The Gate job's workspace test step (`cargo test --workspace`, gate.yml and `task check`) runs under
cargo-nextest with the repository's `.config/nextest.toml` (a test over 60 s is killed), plus
`cargo test --doc` for doctests, ends within 120 s, and prints its ten slowest tests.

## Evidence (r2-budget hand-back, 2026-11-23)

- story:entity-eventlog-tests-under-budget moved the Eventlog runtime job onto nextest
  (`scripts/test-step.sh`, `.config/nextest.toml`); the Gate job still runs `cargo test
  --workspace`, which kills no test at 60 s (r2-budget D1, option A on default).
- `~/beyond10x/AGENTS.md` § Tests stay fast: the runner kills a test at 60 s and the step fails
  over 120 s, locally and in CI.

## Acceptance

- The Gate job and `task check` run the workspace tests under nextest through the step script;
  a stand-in 70 s test is killed at 60 s; doctests still run.
- Two measured runs of the step with the load stated, both under 120 s, or a story per crate that
  is over.
