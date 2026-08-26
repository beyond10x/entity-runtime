---
format: aep.planning-md/1
id: story:provider-spi
kind: story
status: implemented
title: Provider SPI outside the core
summary: State, event, search and blob provider traits in a crate that depends on entity-core, with optimistic concurrency on revision and an in-memory reference implementation.
relations:
- derived_from: epic:kernel
- decomposes: epic:the-shell
revision: 6
---
# Story: Provider SPI outside the core

## Outcome

State, event, search and blob providers have traits to implement, in a crate that depends on
`entity-core` and never the reverse (R-82), with optimistic concurrency on `revision` (R-44) and an
in-memory reference implementation the CLI can use.

## Acceptance

`entity-providers` (or similar) with `StateProvider`/`EventProvider` traits taking a `Decision` and
an expected revision; the in-memory implementation refusing a stale revision; a test that two
concurrent executions from the same revision yield exactly one accepted; `entity` gains a `--store`
option using it, so `execute` no longer needs `--instance` when a store is given.
