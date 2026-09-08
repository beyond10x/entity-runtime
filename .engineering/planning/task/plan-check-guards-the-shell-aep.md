---
format: aep.planning-md/1
id: task:plan-check-guards-the-shell-aep
kind: task
status: implemented
title: plan-check version-checks the aep the shell will run
summary: entity-xtask resolved aep through cargo's augmented PATH while aep artifact validate used the shell's; the Taskfile now hands it the shell's path
relations:
- decomposes: story:gate-and-release-hardening
- serves: vision:O2
revision: 5
---
## Outcome

`task plan-check` version-checks the very `aep` binary its next line runs.

## Scope

- Cited: `crates/entity-xtask/src/main.rs` (`aep-version --binary`), `Taskfile.yml` (`plan-check`).

## Acceptance

`task plan-check` prints `aep implements protocol X.Y.Z at <path> (needs 0.26.0)` for the path the
shell's `command -v aep` resolves, then `aep artifact validate`. With no `aep` on the shell's PATH
the guard exits 2 saying so, before `validate` can fail with command-not-found.

## Authorization

Follow-up surfaced by the 2026-09-08 review's F3 agent: `cargo run` prepends its own directories to
the child's PATH, so a lookup inside the guard could find an `aep` the shell would not. Fixed in the
same change set at the operator's request to close all follow-ups.

## Implementation evidence

`--binary` takes `$(command -v aep)` through a value parser that accepts an empty string; empty is
refused as "aep is not on the shell's PATH". Observed: `task plan-check` → `aep implements protocol
0.54.0 at /home/timo/.cargo/bin/aep (needs 0.26.0)` then `valid`; `--binary ""` → exit 2.
