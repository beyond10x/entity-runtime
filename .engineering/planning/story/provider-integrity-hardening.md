---
format: aep.planning-md/1
id: story:provider-integrity-hardening
kind: story
status: draft
title: Every provider preserves one recorded history contract
summary: File, SQLite, PostgreSQL, Remote and Hybrid stores agree on ordering, ranges, freshness, failures and divergence.
relations:
- decomposes: epic:the-store-an-adopter-runs-on
revision: 1
---
## Context

Providers currently disagree about same-revision events, Hybrid authority and freshness, PostgreSQL reachability, integer ranges and Remote wire evolution.

## Acceptance

The shared conformance suites prove every provider preserves recorded commits and observations in append order with checked revisions, truthful freshness, typed reachability and directional catch-up, while optional atomic batches still rollback completely.
