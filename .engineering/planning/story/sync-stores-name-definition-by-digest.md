---
format: aep.planning-md/3
id: story:sync-stores-name-definition-by-digest
kind: story
status: draft
title: File, SQLite and PostgreSQL stores name the definition by digest in new records
summary: '#94 follow-up: only the Eventlog adapter writes er.record/5; the synchronous stores still embed the definition in every record'
owner: entity-runtime
revision: 1
---
## Outcome

The File, SQLite and PostgreSQL synchronous stores write new decision records that name their
entity definition by digest and keep each definition document once per store, as the Eventlog
adapter does since #94 (`er.record/5`), while records already stored keep their bytes and digests
and still open, verify and replay.

## Finding

Follow-up of issue #94 ("record names definition by digest", connectors gap; connectors#267
measured 94 % of a consumer's Eventlog blob bytes as repeated definitions). Decision er-j D1 = A,
"sync stores follow-up" (master log `assignment.md` line 127).

## Evidence

- `23cd3e50` "feat(store,eventlog): name the entity definition by digest in new records" on
  `feat/record-definition-digest` (PR #95, release 0.33.0). Its body: "The File, SQLite and
  PostgreSQL synchronous stores keep their own envelope formats and are unchanged."
- Measured on the Eventlog SQLite provider (same commit body): one service/1 record of a 32-field
  definition appended 21,075 blob bytes before and 4,373 after; record blob 10,062 -> 1,711 bytes;
  the 8,444-byte definition stored once per tenant.
- The embedded snapshot: `DecisionRecord.definition: Option<EntityDefinition>`
  (`crates/entity-core/src/runtime.rs:141-143`), written whole into each record by
  - File Store: one document per subject, `crates/entity-store/src/file.rs` (`entity.file-subject/2`);
  - SQLite: `history.document`, `crates/entity-sqlite/src/lib.rs:278`, inserts at lines 555 and 607;
  - PostgreSQL: `history` table, `crates/entity-postgres/src/lib.rs:399`.
- The digest is computed by the dependency-free SHA-256 in `entity-store` (held to the FIPS vectors)
  that #94 added; the synchronous stores can use the same function.

## Acceptance

On each of the File, SQLite and PostgreSQL stores, 32 records of one entity type written after the
change hold one copy of the definition document (a test counts the stored definition bytes), and a
store written by 0.33.0 with embedded definitions opens, verifies and returns the same `history()`
records and digests after the change.
