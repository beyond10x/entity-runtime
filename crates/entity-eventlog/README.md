# entity-eventlog

Complete recorded Entity Runtime storage through Eventlog atomic groups. Compose a caller-owned
file, SQLite or PostgreSQL Eventlog provider with `EventlogStore`, supplying an exact tenant,
namespace and opaque host attribution. Use `entity_shell::asynchronous::AsyncStoredRuntime`
for recorded commands. The adapter never chooses credentials, a path or an async runtime.

This crate requires Rust 1.91 and has its own workspace and lockfile. Existing ER crates keep
their Rust 1.85 requirement. From the repository root, `task eventlog-check` runs format, strict
Clippy, tests and rustdoc. The ordinary local and required CI gates include those checks.

The provider retains full pinned decisions, including zero-event decisions, and observations
without treating physical log positions as entity revisions. Global record identity and ordered
batch rollback share one Eventlog transaction. File, SQLite and PostgreSQL share the same contract
assertions. Enumeration uses committed stream inventory so an unrelated PostgreSQL transaction
holding back the feed cannot hide a just-created subject.

The explicit PostgreSQL lane is `task eventlog-postgres-check`. Assign a disposable database via
`ENTITY_EVENTLOG_POSTGRES_URL`; selecting the lane without it fails. CI supplies its existing service,
and local `task check` reports the lane as not run when no URL is assigned. Each case is bounded to
20 seconds. The fixture must be test-owned: acceptance creates and drops prefixed tables.
Register `EntityDocumentProjector` inline and call `enable_document_queries` before querying.
When the provider implements native transactions, `with_transaction(|mut session| Box::pin(async
move { ... }))` provides the same recorded/query ports plus `load_for_update`, `lock_identity` and
`reserve_sequence`. All staged work rolls back on callback refusal or cancellation. The callback
returns an owned value only after commit; unknown outcomes never cause automatic callback replay.
Legacy-layout migration and compatible SQL facade replacement remain open.

See [the persistence design](../../docs/design/eventlog-recorded-provider.md) for the versioned
mapping, cache invalidation and refusal rules.
