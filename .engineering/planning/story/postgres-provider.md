---
format: aep.planning-md/1
id: story:postgres-provider
kind: story
status: implemented
title: 'entity-postgres: a provider with a server, and a gate that says when it did not run'
summary: A Store over Postgres with one transaction per commit and a real two-writer test; runs when ENTITY_POSTGRES_URL is set and prints that it was skipped when not.
relations:
- decomposes: epic:the-store-an-adopter-runs-on
- depends_on: story:store-enumeration
- informed_by: story:sqlite-provider
revision: 9
---
# Story: `entity-postgres` — a provider with a server, and a gate that says when it did not run

## Outcome

A team whose record of truth is a Postgres they already back up can keep entities there, with two
writers to one instance resolving to one accepted write and one `RevisionConflict` that names the
revision it lost to — and this repository's gate stays runnable on a laptop with no database.

## Context

`entity-sqlite` proved the SPI with one file and no server (R-103: one transaction, both halves).
The adopter's P5 (`engineering-protocols` `story:postgres-backend`) is *the backend an organisation
actually runs*, and after their wave F it is one type instantiation there — now that the provider
exists here. The constraint that shaped this story is this repository's own: the gate reaches no
network, and a provider that cannot be tested without a server cannot be in `task check`
unconditionally.

## Acceptance

- `crates/entity-postgres` implements `Store` (and `ids`) over a connection the caller opens;
  instance and events written in one transaction; a stale `Expect` refused as `RevisionConflict`;
  `SERIALIZABLE` or `SELECT … FOR UPDATE`, stated and justified in the module documentation. **Done**
  — `SELECT … FOR UPDATE` under `READ COMMITTED`, and why not `SERIALIZABLE`, in the module doc; a
  racing creation's key violation is turned into the same conflict by re-reading what landed.
- The conformance suite and `a_broken_provider_is_caught` pass against it; a test with two threads
  writing one instance from one revision leaves exactly one accepted (R-84 under real concurrency).
  **Done** — `the_postgres_provider_conforms` (9 cases), `a_broken_copy_of_the_provider_is_caught`,
  `two_writers_from_one_revision_leave_exactly_one_accepted` (the loser told `found: Some(2)`),
  `two_creators_of_one_identity_leave_exactly_one_accepted` (`found: Some(1)`), each in a schema of
  its own on the server; 6 tests green against `postgres:16` on 2026-08-28.
- Schema creation is a function (`PostgresStore::migrate`), idempotent, and the only DDL. **Done** —
  `migrate_is_idempotent_and_a_store_survives_being_reopened`.
- The tests run when `ENTITY_POSTGRES_URL` is set and are **skipped with a printed line** when it is
  not — `task check` prints *"postgres-check: skipped, ENTITY_POSTGRES_URL unset"*. CI sets it, with
  a service container. **Done** — `scripts/postgres-check.sh`, the `postgres-check` step in
  `Taskfile.yml`, a `postgres:16` service and the `Postgres provider` step in
  `.github/workflows/gate.yml`; each test also says so on stderr when it returns without a server.
- The one new dependency is justified in the manifest, and `entity-core`'s dependency-pin test is
  untouched. **Done** — `postgres` 0.19, `default-features = false`; the synchronous client because
  the SPI is synchronous; no TLS backend chosen on an adopter's behalf. R-111 added;
  `store-v0.1.md` § 12.

## Decision taken

The open question's default: when `ENTITY_POSTGRES_URL` is set and the server does not answer, the
tests **fail** — a variable somebody set is a claim that the server is there. A server that does not
answer is `Unreachable` at the SPI, never an empty store
(`a_server_that_does_not_answer_is_unreachable_and_never_an_empty_store`).

## Out of Scope

Connection pooling, TLS configuration, authentication: the caller opens the connection. A remote
protocol over Postgres — that is `entity-remote`'s `Transport`, and somebody else's.

## Open Questions

None outstanding; the one that was open is decided above.
