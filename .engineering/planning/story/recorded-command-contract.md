---
format: aep.planning-md/1
id: story:recorded-command-contract
kind: story
status: implemented
title: Stored CLI commands require complete recording metadata
summary: Create and execute commit the exact record they print and reject partial provenance.
relations:
- decomposes: epic:the-shell
- serves: vision:O2
revision: 5
---
## Context

Only execute exposes envelope flags, partial sets are silently discarded, timestamps are not validated consistently and stored commands currently commit bare events.

## Acceptance

Stored create and execute require a record id, valid recorded time and explicit actor choice, preserve optional correlation and causation, print the committed record in the selected format and reject incomplete invocation with exit 2.

## Implementation evidence

Moved to implemented on 2026-09-09 after an audit of every non-implemented artifact against `CHANGELOG.md`, `docs/roadmap.md` and the crates.

Stored `entity create` and `entity execute` require `--record-id`, a valid `--recorded-at` and exactly one of `--actor`/`--no-actor`; the record printed is the record committed — `CHANGELOG.md` `## [0.15.0]` Changed; since the 2026-09-08 review both verbs run through `entity_shell::StoredRuntime`.
