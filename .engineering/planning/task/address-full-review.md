---
format: aep.planning-md/1
id: task:address-full-review
kind: task
status: draft
title: Correct failures found by the full runtime review
relations:
- decomposes: story:provider-integrity-hardening
- serves: vision:O2
revision: 2
---
## Outcome

Correct all 14 reproduced full-review findings: numeric ordering; File Store concurrency, recovery, confinement, event order and multiplicity; hybrid decision/observation catch-up; PostgreSQL session rollback/history and concurrent retries; shell retries; query equality; generated schema unions and identifier collisions. Publish a verified patch release and clean task-owned artifacts.

## Scope

- Cited: entity-core number comparison; entity-store, entity-sqlite, entity-postgres, entity-remote and entity-shell storage/history paths.
- Cited: entity-query containment, entity-mcp and entity-surface schema projections.
- Inferred: regression tests, manifests, changelog, design clarification and CI persistence coverage needed by those fixes.

## Acceptance

All 14 review cases have passing regression coverage. Full task check with PostgreSQL passes. The published release tag, manifests, changelog, assets and author are verified. Unrelated primary edits remain intact; only task-owned disposable artifacts are removed.

## Authorization

The operator requested all fixes, commit, new release and cleanup on 2026-09-05. No consumer pin promotion or consumer releases are requested. One task owns the fixes; no plan decomposition or concurrent implementation is scheduled.

## Implementation evidence

The corrections and regression mapping are recorded in `docs/reviews/2026-09-05-runtime-hardening.md`.
Release 0.17.7 preserves the existing record index, adds process-safe writes and invalidation,
corrects numeric semantics, retains provenance during catch-up, rolls back PostgreSQL session
batches, supports exact shared-shell retries, and repairs projected contracts. Release jobs exercise
native File Store persistence on every shipped operating system. Full gate with PostgreSQL 17,
Rust 1.85 all-target checks and the website build passed during implementation; the final release
tree is checked again before publication. No lifecycle status claim is made by this body update.
