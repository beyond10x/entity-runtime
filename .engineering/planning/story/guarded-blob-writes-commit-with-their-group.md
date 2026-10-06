---
format: aep.planning-md/3
id: story:guarded-blob-writes-commit-with-their-group
kind: story
status: implemented
title: Every guarded blob-bearing write commits its blobs with its group
relations:
- serves: vision:O1
revision: 4
transitions:
- {from: "draft", to: "proposed", at: "2026-10-06T09:23:12Z", actor: "human:timo", revision: 2, decided_on: {"recorded":{"test_result":1}}}
- {from: "proposed", to: "active", at: "2026-10-06T09:23:12Z", actor: "human:timo", revision: 3, decided_on: {"recorded":{"test_result":1}}}
- {from: "active", to: "implemented", at: "2026-10-06T09:23:13Z", actor: "human:timo", revision: 4, decided_on: {"recorded":{"test_result":1}}}
---
# Every guarded blob-bearing write commits its blobs with its group

## Outcome

On a provider that implements `append_group_guarded_with_blobs` (SQLite, File since Eventlog 0.5.0), every Entity Runtime write path that carries blobs commits them in the same transaction as the guarded group, so a refused guard binds no blob. Providers without it keep the per-blob fallback.

## Why

The batch import already does this. Four other paths in `crates/entity-eventlog/src/adapter.rs` upload each blob first and then call `append_group_guarded`; a refusal leaves unreferenced blobs and each blob costs its own durability barrier.

## Acceptance

- A test per path: a refused guard on SQLite leaves no blob bound; an admitted one commits both.
- Each path keeps its observable errors; the PostgreSQL fallback still works.
