---
format: aep.planning-md/2
id: story:guarded-blob-writes-commit-with-their-group
kind: story
status: draft
title: Every guarded blob-bearing write commits its blobs with its group
relations:
- serves: vision:O1
revision: 1
---
# Every guarded blob-bearing write commits its blobs with its group

## Outcome

On a provider that implements `append_group_guarded_with_blobs` (SQLite, File since Eventlog 0.5.0), every Entity Runtime write path that carries blobs commits them in the same transaction as the guarded group, so a refused guard binds no blob. Providers without it keep the per-blob fallback.

## Why

The batch import already does this. Four other paths in `crates/entity-eventlog/src/adapter.rs` upload each blob first and then call `append_group_guarded`; a refusal leaves unreferenced blobs and each blob costs its own durability barrier.

## Acceptance

- A test per path: a refused guard on SQLite leaves no blob bound; an admitted one commits both.
- Each path keeps its observable errors; the PostgreSQL fallback still works.
