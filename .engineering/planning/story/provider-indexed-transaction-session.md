---
format: aep.planning-md/1
id: story:provider-indexed-transaction-session
kind: story
status: implemented
title: Indexed queries inside one provider transaction
summary: Query JSON documents and lock the records a caller decides on without whole-store enumeration.
relations:
- decomposes: epic:the-store-an-adopter-runs-on
- serves: vision:O2
revision: 5
---
## Context

The central AEP authority must decide from current PostgreSQL state, while Entity Runtime previously offered point reads and complete type enumeration only. The capability belongs outside the IO-free kernel and remains optional for providers that do not need indexed access.

## Acceptance

A caller executes stable paginated document queries and point reads or writes inside one caller-scoped PostgreSQL transaction, including locking an absent logical identity, without enumerating or hydrating every instance of an entity type.

## Implementation

`entity-query` defines provider-neutral document filters, bounded keyset pages, and query-bound opaque cursors. Memory and PostgreSQL providers implement the contract. `PostgresStore::with_transaction` exposes a caller-scoped session with point reads, advisory identity locks, indexed document queries, and atomic batch commits.

## Evidence

- `a_cursor_is_bound_to_the_query_that_emitted_it` proves cursors cannot be replayed against a different filter.
- `page_limits_are_bounded` proves defaults and maximum page sizes.
- PostgreSQL conformance tests exercise indexed filtering, keyset continuation, identity locking, and rollback semantics when `ENTITY_POSTGRES_URL` is configured.
- `task check` passed for release 0.17.0 on 2026-08-31; the optional PostgreSQL step reported its explicit skip because `ENTITY_POSTGRES_URL` was unset.
