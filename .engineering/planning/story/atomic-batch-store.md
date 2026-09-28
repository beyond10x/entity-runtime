---
format: aep.planning-md/3
id: story:atomic-batch-store
kind: story
status: implemented
title: A provider commits one ordered batch or nothing
summary: Add an additive transactional batch contract for multi-entity commands.
relations:
- derived_from: epic:the-store-an-adopter-runs-on
- serves: vision:O2
revision: 4
transitions:
- {from: "draft", to: "proposed", at: "2026-08-30T22:10:01Z", actor: "human:operator", revision: 2, imported: true}
- {from: "proposed", to: "active", at: "2026-08-30T22:10:01Z", actor: "human:operator", revision: 3, imported: true}
- {from: "active", to: "implemented", at: "2026-08-30T22:25:18Z", actor: "human:operator", revision: 4, decided_on: {"recorded":{"test_result":1}}, imported: true}
---
## Finding

The provider contract makes one `Decision` atomic but an adopter command can produce several decisions. Calling `Store::commit` repeatedly exposes a successful prefix and cannot enforce optimistic expectations against one transaction-local view.

Source: `engineering-protocols/docs/plan/architecture-hardening.md`, accepted by the operator on 2026-08-30; `crates/entity-store/src/lib.rs` owns the existing single-decision seam.

## Acceptance

- `AtomicCommit` pairs a decision with its expectation and `AtomicBatchStore::commit_batch` applies entries in order against transaction-local state.
- An empty batch is a no-op. Any conflict or provider failure leaves every instance and event unchanged. Later entries may expect a revision produced earlier in the same batch.
- Memory, SQLite and Postgres implement the additive trait. A shared suite tests ordered same-identity writes, rollback and conflict, and a deliberately faulty batch provider proves each case can fail.
- Requirements and the normative store design state the contract. The workspace releases as 0.14.0 before engineering-protocols repins all entity crates.

## Scope

- `crates/entity-store/` — cited owner of `Store` and provider conformance.
- `crates/entity-sqlite/` and `crates/entity-postgres/` — cited transactional providers.
- workspace version, requirements, design and changelog — required by this repository's release contract.
- `entity-file` and `entity-remote` do not claim the stronger extension in this release.
