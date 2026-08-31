---
format: aep.planning-md/1
id: story:postgres-document-query-parameter
kind: story
status: implemented
title: PostgreSQL document queries bind JSON text correctly
summary: Cast serialized containment input through text before PostgreSQL JSONB matching.
relations:
- decomposes: epic:the-store-an-adopter-runs-on
- serves: vision:O2
revision: 5
---
## Context

The first live EP fresh-session query against ER 0.17.2 failed before execution because PostgreSQL inferred the containment placeholder as `jsonb` while the provider supplied serialized JSON text. The memory reference tests therefore did not prove the PostgreSQL adapter.

## Acceptance

The PostgreSQL provider binds its serialized containment predicate as text and casts it to JSONB server-side, and a live provider test selects a nested matching document through the same public query capability EP uses.

## Implementation

The PostgreSQL query now binds the serialized containment document as `text` and performs the JSONB cast in SQL. A provider integration test commits a real document and selects it through `DocumentQueryProvider` against PostgreSQL, closing the adapter gap that memory-only tests missed.
