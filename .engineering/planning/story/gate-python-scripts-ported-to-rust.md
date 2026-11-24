---
format: aep.planning-md/3
id: story:gate-python-scripts-ported-to-rust
kind: story
status: draft
title: The gate, Taskfile and release workflow run no Python
summary: check-requirements.py, check-pin.py and changelog-section.py run in the required Gate job and the release workflow
owner: entity-runtime
relations:
- informed_by: story:gate-and-release-hardening
revision: 1
---
## Outcome

The gate, the Taskfile and the release workflow run no Python. The three scripts under `scripts/`
(`check-requirements.py` 110 lines, `check-pin.py` 93 lines, `changelog-section.py` 163 lines) are
ported to Rust (a subcommand of `crates/entity-xtask`) or POSIX sh, with the same findings and the
same exit codes, and the Python files are deleted.

## Why

`~/beyond10x/AGENTS.md` "No Python for permanent tools": existing Python in a repository is a
defect that gets a story and is ported. Here it runs in the required `gate / Gate` job and in the
release workflow, so a runner image without `python3` fails both.

## Evidence

- `Taskfile.yml:76` (`changelog-section.py --self-test`), `:126` (`check-requirements.py`), `:131`
  (`check-pin.py`).
- `.github/workflows/gate.yml:84` (`python3 scripts/check-requirements.py`).
- `.github/workflows/release.yml:153-154` (`changelog-section.py --self-test`, then the section
  for `$TAG`).
- `AGENTS.md:154` and `crates/entity-core/tests/requirements.rs:4` name `check-requirements.py`.
- `story:gate-and-release-hardening` ("Open items") already records `req-check`, `pin-check` and
  `notes-check` as Python and the "one Rust-owned gate" as not met; this story is the port of
  those three and nothing more.

## Acceptance

- `grep -rn "python" Taskfile.yml .github scripts` finds no call, and `git ls-files '*.py'` lists
  no file outside the vendored or generated paths.
- Each ported check fails on the planted input its Python original failed on: an unreferenced
  `R-nn`, a pin citing a function that is not a `#[test]`, an `#[ignore]`d pin, a row with no
  evidence cell, a mentioned id without a row (`check-requirements`); a `PIN.md` sum that does not
  match its file (`check-pin`); the self-test shapes of `changelog-section` and the section for a
  tag.
- The release workflow's notes for the same tag are byte-identical before and after the port.
- The gate's added time is measured before and after and stays inside the 120 s test-step budget.

## Scope

- `scripts/check-requirements.py`, `scripts/check-pin.py`, `scripts/changelog-section.py`
  (deleted).
- `crates/entity-xtask` (the ported checks and their tests).
- `Taskfile.yml`, `.github/workflows/gate.yml`, `.github/workflows/release.yml` (the calls).
- `AGENTS.md:154`, `crates/entity-core/tests/requirements.rs:4` (the names).
