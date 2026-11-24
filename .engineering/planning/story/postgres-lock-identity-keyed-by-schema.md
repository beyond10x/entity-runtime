---
format: aep.planning-md/3
id: story:postgres-lock-identity-keyed-by-schema
kind: story
status: implemented
title: PostgreSQL lock_identity keys its advisory lock by schema
summary: 'E57 shape in lock_identity: the key omits the schema, so two schemas of one database serialize on one identity'
owner: entity-runtime
relations:
- serves: vision:O2
revision: 4
transitions:
- {from: "draft", to: "proposed", at: "2026-11-24T03:20:00Z", actor: "human:timo", revision: 2, decided_on: {"recorded":{"test_result":1}}}
- {from: "proposed", to: "active", at: "2026-11-24T03:20:01Z", actor: "human:timo", revision: 3, decided_on: {"recorded":{"test_result":1}}}
- {from: "active", to: "implemented", at: "2026-11-24T03:20:01Z", actor: "human:timo", revision: 4, decided_on: {"recorded":{"test_result":1}}}
---
## Outcome

`PostgresSession::lock_identity` keys its advisory lock by the storage schema as well as the
namespace and identity, so contenders for one absent identity in two schemas of one database do not
wait on each other, while contenders in one schema still serialize.

## Finding

Follow-up of review E57 (Entity Runtime 0.32.0, round 7: "PostgreSQL record locks serialize writes
across independent schemas (advisory lock key omits the schema)"). E57 was fixed for record locks;
the er-i2 agent reported the same shape in `lock_identity` (master log `assignment.md`:
"Follow-up finding: PostgresSession::lock_identity key omits schema (same shape as E57) ->
story").

## Evidence

- `crates/entity-postgres/src/lib.rs:543-551`: `pg_advisory_xact_lock(hashtextextended($2,
  hashtextextended($1, 0)))` over `namespace` and `identity` only. Advisory locks are
  database-wide, so the key carries no schema.
- The E57 fix (`df5211c7`, merged in #93) seeds record locks with
  `hashtextextended('history'::regclass::oid::text, 412731)` (`crates/entity-postgres/src/lib.rs:851`),
  the identity of the table the transaction resolves. Its commit body states the rolling-upgrade
  effect: older and newer binaries on one schema take different keys.
- Caller in this repository: `crates/entity-postgres/tests/conformance.rs:365`
  (`session.lock_identity("aep.locator", ...)`); the method is public, consumers outside this
  repository may call it.

## Acceptance

A PostgreSQL test holds `lock_identity(ns, id)` in a transaction on schema A and shows that
`lock_identity(ns, id)` on schema B of the same database is granted within its lock timeout while
the same call on schema A waits until its timeout expires, and the CHANGELOG states the
rolling-upgrade effect for one schema as E57's line does.
