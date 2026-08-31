# Provider query and transaction session v0.1

Status: accepted by `story:provider-indexed-transaction-session` and Atlas ADR 0009.

## 1. Optional query capability (R-119)

`entity-query` is outside `entity-core` and defines exact top-level field predicates over one
entity discriminator. Providers return byte-ordered keyset pages. A cursor carries the complete
canonical query identity and the last opaque instance id; using it for another question is refused.
Limits default to 100 and are bounded at 1,000.

The memory implementation is the behavioural reference. PostgreSQL answers the same question with
a JSONB containment predicate and a GIN expression index over existing JSON instance documents, so
the capability does not change stored document bytes.

## 2. Caller-scoped PostgreSQL session (R-120)

`PostgresStore::with_transaction` lends a `PostgresSession` to one caller operation and commits only
when its closure succeeds. The session offers `load_for_update`, query access, an ordered uncommitted
batch, and a transaction advisory lock for a logical identity that may have no row yet. The outer
provider owns commit and rollback; the adopter cannot accidentally make a partial prefix visible.

This is a provider capability, not a kernel API. It reads a database and therefore cannot enter
`entity-core` under R-01.
