---
format: aep.planning-md/1
id: story:legacy-recorded-batch-compatibility
kind: story
status: draft
title: Preserve complete recorded batches through SQL compatibility providers
relations:
- serves: vision:O2
- decomposes: epic:the-store-an-adopter-runs-on
- depends_on: story:async-recorded-execution
revision: 2
---
## Acceptance
SQLite and PostgreSQL commit ordered complete-record batches atomically through the explicit compatibility adapter, and a PostgreSQL caller transaction can query and read its staged history while any caught batch refusal rolls back its state, provenance and identity reservations without poisoning the outer transaction.

## Required migration boundary
ESS docs/design/ess-evolution/migration.md section 1 requires explicit synchronous compatibility, retained complete records, atomic batches, query/transaction capabilities and readable legacy SQL layouts. AtomicRecordedStore currently exists only on MemoryStore; SQL Store::commit_recorded owns an entire transaction and PostgresSession::commit_batch accepts only unrecorded decisions. Reuse each SQL provider's existing single-record writer inside a transaction and expose AtomicRecordedStore plus recorded session history. This is support for legacy compatibility and migration, not the final replacement of SQL persistence by Eventlog facades.

## Design and scope
Cited: crates/entity-sqlite/src/lib.rs (Store::commit_recorded and AtomicBatchStore), crates/entity-postgres/src/lib.rs (Store::commit_recorded, PostgresSession and with_transaction), crates/entity-store/src/asynchronous.rs (AtomicRecordedStore and BlockingRecordedStore), crates/entity-store/src/conformance.rs, both SQL conformance test files, docs/design/provider-query-v0.1.md, docs/requirements.md and CHANGELOG.md. No stored layout changes, new domain types, new third-party dependencies or runtime ownership. The kernel stays unchanged. PostgreSQL record identity locks must be ordered before the batch writes, while decision application stays in caller order. A session batch uses a savepoint, including when its error is caught. Existing query and sequence primitives stay in that transaction.

## Verification
One shared recorded-batch exercise covers zero-event records, repeated subjects, exact retries, changed/global identities and a late failure with no surviving prefix. Real PostgreSQL tests couple recorded batches to queries, history reads, outer rollback and caught savepoint refusal. Check the blocking async adapter composes with both SQL providers. Run affected tests, strict Clippy and Rust 1.85 checks; mutate the session savepoint protection and show the caught-failure case fails. Do not run another full gate. This is one bounded compatibility prerequisite, not a multi-story decomposition.

## Implementation and verification

Implemented AtomicRecordedStore for the existing SQLite and PostgreSQL layouts by extracting and reusing each provider's single-record writer. PostgreSQL sessions expose records, observations and a savepoint-contained recorded batch; standalone batches delegate to that same session path. Recorded PostgreSQL writes acquire ordered record and subject identity locks before applying decisions in caller order. No dependency or persisted-layout change was made. The existing BlockingRecordedStore blanket implementation now admits these SQL types for the recorded batch capability; it remains explicitly synchronous IO.

The shared recorded-batch exercise now runs on MemoryStore and both SQL providers, preserving zero-event genesis, complete envelopes, repeated subjects, exact retries and global conflicts. It reuses an identity that was actually written before the refused batch rolled back. PostgreSQL tests additionally prove transaction-local queries and history, invisibility to a second connection, rollback of sequence reservations with the outer transaction, continued operation after a caught batch refusal, and one complete winner for opposite-order competing batches.

The initial affected-package test selection passed. After the savepoint and independent-commit mutations were restored and the shared rollback fixture strengthened, the final conformance selection passed: PostgreSQL 16 cases in 0.11s, SQLite four in 0.08s, and store conformance three in 0.21s. The SQLite independent-commit mutation failed with a surviving state/history/event prefix. The PostgreSQL savepoint mutation failed because a caught refusal left the first subject visible inside the outer transaction. Both guards are restored. Strict all-target Clippy, strict rustdoc, formatting and Rust 1.85 all-target checks passed. Requirements validation reported 98 requirements, 319 test functions and zero findings.

Evidence: local-evidence:ess-evolution-20260910/er-sql-recorded-batch-tests.log, er-sql-recorded-batch-final.log, er-sql-savepoint-mutation.log, er-sql-batch-mutation.log, er-sql-recorded-batch-clippy-final.log, er-sql-recorded-batch-msrv.log, er-sql-recorded-batch-docs.log and er-sql-batch-fixture.txt. The isolated loopback PostgreSQL fixture was stopped after checks. No full gate, release or remote production proof ran. Repository main has no new commits beyond this continuation's base; the branch still requires integration. Eventlog-native indexed query/session semantics, actual SQL facade conversion, AEP migration and ESS/application acceptance remain unfinished. The owning native gap is recorded in story:eventlog-recorded-provider.
