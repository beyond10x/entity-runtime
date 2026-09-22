---
title: Release highlights
---

## 0.19.0 — September 22, 2026

- Service semantics progress through `service/3`: named outcomes and responses, optional-value
  preservation, typed update actions and derived creation addresses.
- Async recorded execution supports complete receipts, bounded history verification and exact
  single-command and batch retries.
- Eventlog-backed File, SQLite and PostgreSQL providers make open, provision and import authority
  explicit. Retained legacy stores remain available.
- Batched anchor import reduces repeated complete captures; source identity remains part of each
  imported boundary.
- The provider path pins Eventlog 0.3.0 and preserves its committed-prefix and blob checks.

The pure runtime crates retain Rust 1.85. The Eventlog adapter and enabled provider closure need
Rust 1.91. `AsyncStoreError::BatchExceedsReadBounds` is a new public enum variant; exhaustive
matches may need updating.

Read the [authoritative release notes](https://github.com/beyond10x/entity-runtime/releases/tag/0.19.0)
and [tagged changelog](https://github.com/beyond10x/entity-runtime/blob/0.19.0/CHANGELOG.md)
for compatibility details. Earlier release entries remain historical records.
