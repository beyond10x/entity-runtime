---
format: aep.planning-md/1
id: story:provider-session-sequence-and-containment
kind: story
status: implemented
title: Complete provider command-session primitives
summary: Allocate transaction-scoped identities and query nested documents without realm enumeration.
relations:
- decomposes: epic:the-store-an-adopter-runs-on
- serves: vision:O2
revision: 6
---
## Context

The first indexed session release proved transaction-scoped reads, locks, queries and commits. A central command evaluator also has to reserve identities in that transaction and find nested logical addresses without enumerating a realm.

## Acceptance

A PostgreSQL session reserves a bounded sequence range atomically, reads an entity's events from its own transaction view, and document containment finds recursively matching objects while cursors remain bound to the complete query.

## Implementation

`PostgresSession::reserve_sequence` allocates disjoint namespace ranges inside the caller's transaction, and `events` reads the same transaction view. `DocumentQuery` now defines recursive JSON containment; its memory implementation mirrors PostgreSQL JSONB containment, including nested objects and array members.

## Evidence

- `nested_document_matching_has_the_same_containment_meaning_as_jsonb` exercises the provider-neutral reference semantics.
- `a_session_reserves_disjoint_sequence_ranges_and_reads_its_events` runs against PostgreSQL when `ENTITY_POSTGRES_URL` is configured.
- `task check` passed for 0.17.1 on 2026-08-31; the optional PostgreSQL step explicitly skipped because the environment variable was unset.
