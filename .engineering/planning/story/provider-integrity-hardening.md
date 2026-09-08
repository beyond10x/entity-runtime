---
format: aep.planning-md/1
id: story:provider-integrity-hardening
kind: story
status: implemented
title: Every provider preserves one recorded history contract
summary: File, SQLite, PostgreSQL, Remote and Hybrid stores agree on ordering, ranges, freshness, failures and divergence.
relations:
- decomposes: epic:the-store-an-adopter-runs-on
- serves: vision:O2
revision: 5
---
## Context

Providers currently disagree about same-revision events, Hybrid authority and freshness, PostgreSQL reachability, integer ranges and Remote wire evolution.

## Acceptance

The shared conformance suites prove every provider preserves recorded commits and observations in append order with checked revisions, truthful freshness, typed reachability and directional catch-up, while optional atomic batches still rollback completely.

## Implementation evidence

Moved to implemented on 2026-09-09 after an audit of every non-implemented artifact against `CHANGELOG.md`, `docs/roadmap.md` and the crates.

Ordering, numeric ranges, freshness, typed reachability, directional catch-up and atomic rollback were corrected and pinned in 0.17.7 (`docs/reviews/2026-09-05-runtime-hardening.md`); its task `task:address-full-review` is implemented, and the 2026-09-08 review's tasks under it are implemented too.
