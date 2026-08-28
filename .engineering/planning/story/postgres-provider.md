---
format: aep.planning-md/1
id: story:postgres-provider
kind: story
status: draft
title: 'entity-postgres: a provider with a server, and a gate that says when it did not run'
summary: A Store over Postgres with one transaction per commit and a real two-writer test; runs when ENTITY_POSTGRES_URL is set and prints that it was skipped when not.
relations:
- decomposes: epic:the-store-an-adopter-runs-on
- depends_on: story:store-enumeration
- informed_by: story:sqlite-provider
revision: 5
---
# Story: `entity-postgres` — a provider with a server, and a gate that says when it did not run

## Outcome

A team whose record of truth is a Postgres they already back up can keep entities there, with two
writers to one instance resolving to one accepted write and one `RevisionConflict` that names the
revision it lost to — and this repository's gate stays runnable on a laptop with no database.

## Context

`entity-sqlite` proved the SPI with one file and no server (R-103: one transaction, both halves).
The adopter's P5 (`engineering-protocols` `story:postgres-backend`) is *the backend an organisation
actually runs*, and after their wave F it is one type instantiation there — if the provider exists
here. The constraint that shapes this story is this repository's own: the gate reaches no network
(`crates/entity-remote/src/lib.rs:6-16`), and a provider that cannot be tested without a server
cannot be in `task check` unconditionally.

## Acceptance

- `crates/entity-postgres` implements `Store` (and `ids`, after `story:store-enumeration`) over a
  connection the caller opens; instance and events written in one transaction; a stale `Expect`
  refused as `RevisionConflict`; `SERIALIZABLE` or `SELECT … FOR UPDATE`, stated and justified in the
  module documentation.
- The conformance suite and `a_broken_provider_is_caught` pass against it; a test with two threads
  writing one instance from one revision leaves exactly one accepted (R-84 under real concurrency).
- Schema creation is a function (`PostgresStore::migrate`), idempotent, and the only DDL.
- The tests run when `ENTITY_POSTGRES_URL` is set and are **skipped with a printed line** when it is
  not — `task check` prints *"postgres-check: skipped, ENTITY_POSTGRES_URL unset"*, so a green gate
  cannot read as a tested provider. CI sets it, with a service container.
- The one new dependency (`postgres` or `tokio-postgres` with a blocking wrapper — the sync SPI
  stands) is justified in the manifest, and `entity-core`'s dependency-pin test is untouched.

## Out of Scope

Connection pooling, TLS configuration, authentication: the caller opens the connection. A remote
protocol over Postgres — that is `entity-remote`'s `Transport`, and somebody else's.

## Open Questions

Whether the gate's `test` step should fail, rather than skip, when the variable is set and the server
is unreachable. Decides: runtime owner. Default if nobody answers: **fail** — a variable somebody set
is a claim that the server is there.
