//! Adversary pass 2, unit 10: the design document this unit changed in its last round still tells
//! a reader that the *port's default* uploads the blobs, and it does not.
//!
//! `docs/design/eventlog-recorded-errors-import-v0.1.md` § "One capture and one group for a batch",
//! step 4, in the paragraph that justifies why an orphan blob is admissible on SQLite and
//! PostgreSQL:
//!
//! > On a provider that takes the port's default, **the default is the singular sequence** — every
//! > blob uploaded on its own path, then the guarded group — and a refusal can leave those blobs
//! > bound as orphans, exactly as the singular `import_anchor` can.
//!
//! That was true at the `9a2a6df8` pin. At the `7fbd37cf` pin this unit now carries, the same
//! document's own step 3 says the opposite of it four paragraphs earlier — "A provider that does
//! not implement that method refuses it, by the port's default, with
//! `EventLogError::Invalid(eventlog_core::UNAVAILABLE)`, **having written and committed nothing**"
//! — and `eventlog-core/src/atomic_group.rs:163-171` is that refusal. The blobs on the default
//! path are uploaded by `crates/entity-eventlog/src/adapter.rs:2185-2189`, this adapter's own
//! fallback loop, not by any port default.
//!
//! The sentence is load-bearing rather than decorative: it is the whole justification the document
//! gives for why a refused batch may leave bound orphans on SQLite, and it attributes the
//! behaviour to a component that no longer has it. A reader auditing "who writes a blob before
//! admission on SQLite" is sent to the port and finds nothing there.
//!
//! This case drove the document against the pinned provider and asserted exactly what that
//! sentence said the default does. The sentence has since been corrected, so the assertions are
//! inverted to hold the corrected one instead: the default **fails closed**, binding no blob and
//! committing no group, and the blobs on that path are uploaded by this adapter's own fallback.
//! Amended by unit 10 in correction round 2; the before/after is quoted in that round's report.
//!
//! The other half of the corrected sentence — that this adapter's fallback is what uploads them —
//! is held by `providers.rs::a_batch_import_commits_through_the_port_default_on_the_sqlite_provider`.
#![cfg(feature = "sqlite")]

use std::sync::Arc;

use eventlog_core::{
    AppendGroup, AtomicEventStore, CommandMeta, EventStore, Expected, NewEvent, NoGuard,
    StreamAppend, StreamId, TenantId,
};
use serde_json::json;
use time::OffsetDateTime;

fn block_on<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime")
        .block_on(future)
}

fn meta(label: &str) -> CommandMeta {
    CommandMeta {
        idempotency_key: format!("adversary-two-{label}-key"),
        request_hash: format!("adversary-two-{label}-hash"),
        subject: "adversary-two-default".into(),
        actor: "entity-eventlog-test".into(),
        request_id: format!("request-{label}"),
        trace_id: format!("trace-{label}"),
        causation_id: None,
        causation_depth: 0,
        occurred_at: OffsetDateTime::UNIX_EPOCH,
        claim: None,
    }
}

#[test]
fn the_port_default_fails_closed_and_binds_no_blob() {
    block_on(async {
        let tenant = TenantId::new("adversary-two-port-default").expect("valid tenant");
        let store = Arc::new(
            eventlog_sqlite::SqliteEventStore::in_memory("adversary_two_default")
                .await
                .expect("SQLite memory provider"),
        );

        let bytes = b"adversary-two-blob".to_vec();
        let digest = eventlog_core::blob_integrity_sha256(&bytes);

        let group = AppendGroup {
            tenant: tenant.clone(),
            appends: vec![StreamAppend {
                stream: StreamId::new(tenant.clone(), "adversary", "one").expect("stream id"),
                expected: Expected::NoStream,
                events: vec![
                    NewEvent::new("adversary.two", 1, json!({ "blob": digest }))
                        .expect("new event"),
                ],
            }],
            meta: meta("default"),
        };

        let settled = store
            .append_group_guarded_with_blobs(
                &group,
                Arc::new(NoGuard),
                std::slice::from_ref(&(digest.clone(), bytes.clone())),
            )
            .await;

        // The default writes nothing, commits nothing, and says which it is.
        assert!(
            matches!(
                &settled,
                Err(eventlog_core::EventLogError::Invalid(detail))
                    if detail == eventlog_core::UNAVAILABLE
            ),
            "a provider that does not override this method must fail closed with UNAVAILABLE, \
             so that a caller holding a trait object is never handed the weaker guarantee under \
             the stronger name. The pinned default answered: {settled:?}"
        );
        assert_eq!(
            store.get_blob(&tenant, &digest).await.expect("blob read"),
            None,
            "the default binds no blob — an orphan on this path is written by the adapter's own \
             fallback loop, not by the port"
        );
    });
}
