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
batch rollback share one Eventlog transaction. Reopened file stores and SQLite are exercised by
the same contract tests. PostgreSQL acceptance and legacy-provider facade migration remain open.

See [the persistence design](../../docs/design/eventlog-recorded-provider.md) for the versioned
mapping, cache invalidation and refusal rules.
