---
format: aep.planning-md/1
id: story:entity-cli-renders-its-skill
kind: story
status: implemented
title: The entity CLI renders its own agent skill
summary: A deterministic entity skill teaches agents the installed CLI surface without duplicating it in a plugin.
relations:
- decomposes: epic:the-shell
- serves: vision:O2
revision: 5
---
## Context

Agents can discover `entity --help`, but they do not know when to validate a definition set, how store recording metadata works, or why File Store migration is out of place. The binary version that owns those rules should render the compact skill that teaches them.

## Acceptance

`entity skill` emits a deterministic version-stamped Agent Skills document, and `--out` writes identical bytes while refusing replacement unless `--force` is explicit.

## Implementation evidence

Moved to implemented on 2026-09-09 after an audit of every non-implemented artifact against `CHANGELOG.md`, `docs/roadmap.md` and the crates.

`entity skill` renders the installed CLI's Agent Skill, version-stamped and deterministic, with `--out` byte-identical to stdout and replacement only under `--force` — `CHANGELOG.md` `## [0.15.0]`; `crates/entity-cli/tests/cli.rs` `skill_stdout_and_file_are_identical_and_replacement_is_explicit`.
