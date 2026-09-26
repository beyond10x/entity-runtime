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
//!
//! **Amended again at the Eventlog 0.5.0 pin (`fe8a0a7e`).** SQLite now *overrides*
//! `append_group_guarded_with_blobs`, so it no longer takes the port's default at all, and driving
//! the default through it would observe the override instead. The fail-closed assertions are kept
//! unchanged and moved onto [`DefaultOnly`], a SQLite store behind a wrapper that implements the
//! port and overrides nothing optional — the default is what it offers, and every write the
//! default might make would land in real SQLite storage where the blob read below would see it.
//! What SQLite itself now does is held by the second case: the group and its blob commit together,
//! and a guard refusal binds neither.
#![cfg(feature = "sqlite")]

use std::sync::Arc;

use eventlog_core::{
    AppendGroup, AppendGroupResult, AppendResult, AtomicEventStore, BoxFuture, CatchUpProgress,
    Claim, ClaimedCommand, CommandMeta, EventLogError, EventStore, Expected, FeedPage, Guard,
    NewEvent, NoGuard, ProjectionSpec, ProjectionStore, Projector, RecordedEvent, Snapshot,
    StreamAppend, StreamId, StreamSlice, TenantId,
};
use eventlog_sqlite::SqliteEventStore;
use serde_json::{Value, json};
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

fn group(tenant: &TenantId, digest: &str, label: &str) -> AppendGroup {
    AppendGroup {
        tenant: tenant.clone(),
        appends: vec![StreamAppend {
            stream: StreamId::new(tenant.clone(), "adversary", "one").expect("stream id"),
            expected: Expected::NoStream,
            events: vec![
                NewEvent::new("adversary.two", 1, json!({ "blob": digest })).expect("new event"),
            ],
        }],
        meta: meta(label),
    }
}

/// A SQLite store behind the port with no optional method overridden.
///
/// Every required method goes to the real provider, so anything the port's default wrote would
/// be bound in real storage. `append_group_guarded_with_blobs` is deliberately left to the
/// default: this is the provider that takes it.
struct DefaultOnly {
    inner: Arc<SqliteEventStore>,
}

impl AtomicEventStore for DefaultOnly {
    fn append_group_guarded<'a>(
        &'a self,
        group: &'a AppendGroup,
        admission: Arc<dyn Guard>,
    ) -> BoxFuture<'a, Result<AppendGroupResult, EventLogError>> {
        self.inner.append_group_guarded(group, admission)
    }
}

impl EventStore for DefaultOnly {
    fn append<'a>(
        &'a self,
        stream: &'a StreamId,
        expected: Expected,
        events: &'a [NewEvent],
        meta: &'a CommandMeta,
    ) -> BoxFuture<'a, Result<AppendResult, EventLogError>> {
        self.inner.append(stream, expected, events, meta)
    }
    fn recorded_claim<'a>(
        &'a self,
        tenant: &'a TenantId,
        claim: &'a Claim,
    ) -> BoxFuture<'a, Result<Option<ClaimedCommand>, EventLogError>> {
        self.inner.recorded_claim(tenant, claim)
    }
    fn recorded_command<'a>(
        &'a self,
        stream: &'a StreamId,
        idempotency_key: &'a str,
        request_hash: &'a str,
    ) -> BoxFuture<'a, Result<Option<AppendResult>, EventLogError>> {
        self.inner
            .recorded_command(stream, idempotency_key, request_hash)
    }
    fn read_stream<'a>(
        &'a self,
        stream: &'a StreamId,
        after_version: u64,
        limit: usize,
    ) -> BoxFuture<'a, Result<StreamSlice, EventLogError>> {
        self.inner.read_stream(stream, after_version, limit)
    }
    fn stream_version<'a>(
        &'a self,
        stream: &'a StreamId,
    ) -> BoxFuture<'a, Result<Option<u64>, EventLogError>> {
        self.inner.stream_version(stream)
    }
    fn read_feed<'a>(
        &'a self,
        tenant: &'a TenantId,
        after_position: u64,
        limit: usize,
    ) -> BoxFuture<'a, Result<FeedPage, EventLogError>> {
        self.inner.read_feed(tenant, after_position, limit)
    }
    fn redact<'a>(
        &'a self,
        stream: &'a StreamId,
        version: u64,
        reason: &'a str,
    ) -> BoxFuture<'a, Result<RecordedEvent, EventLogError>> {
        self.inner.redact(stream, version, reason)
    }
    fn save_snapshot<'a>(
        &'a self,
        stream: &'a StreamId,
        snapshot: &'a Snapshot,
    ) -> BoxFuture<'a, Result<(), EventLogError>> {
        self.inner.save_snapshot(stream, snapshot)
    }
    fn load_snapshot<'a>(
        &'a self,
        stream: &'a StreamId,
    ) -> BoxFuture<'a, Result<Option<Snapshot>, EventLogError>> {
        self.inner.load_snapshot(stream)
    }
    fn forget_tenant<'a>(
        &'a self,
        tenant: &'a TenantId,
    ) -> BoxFuture<'a, Result<(), EventLogError>> {
        self.inner.forget_tenant(tenant)
    }
    fn append_guarded<'a>(
        &'a self,
        stream: &'a StreamId,
        expected: Expected,
        events: &'a [NewEvent],
        meta: &'a CommandMeta,
        guard: Arc<dyn Guard>,
    ) -> BoxFuture<'a, Result<AppendResult, EventLogError>> {
        self.inner
            .append_guarded(stream, expected, events, meta, guard)
    }
    fn create_projections(
        &self,
        projector: Arc<dyn Projector>,
    ) -> BoxFuture<'_, Result<(), EventLogError>> {
        self.inner.create_projections(projector)
    }
    fn register_inline(
        &self,
        projector: Arc<dyn Projector>,
    ) -> BoxFuture<'_, Result<(), EventLogError>> {
        self.inner.register_inline(projector)
    }
    fn is_inline<'a>(&'a self, name: &'a str) -> BoxFuture<'a, bool> {
        self.inner.is_inline(name)
    }
    fn run_catch_up<'a>(
        &'a self,
        projector: Arc<dyn Projector>,
        tenant: &'a TenantId,
        batch: usize,
    ) -> BoxFuture<'a, Result<CatchUpProgress, EventLogError>> {
        self.inner.run_catch_up(projector, tenant, batch)
    }
    fn rebuild_projection<'a>(
        &'a self,
        projector: Arc<dyn Projector>,
        tenant: &'a TenantId,
    ) -> BoxFuture<'a, Result<u64, EventLogError>> {
        self.inner.rebuild_projection(projector, tenant)
    }
    fn projection_get<'a>(
        &'a self,
        projection: &'a ProjectionSpec,
        tenant: &'a TenantId,
        key: &'a str,
    ) -> BoxFuture<'a, Result<Option<Value>, EventLogError>> {
        self.inner.projection_get(projection, tenant, key)
    }
    fn projection_find<'a>(
        &'a self,
        projection: &'a ProjectionSpec,
        tenant: &'a TenantId,
        field: &'a str,
        value: &'a str,
        limit: usize,
    ) -> BoxFuture<'a, Result<Vec<Value>, EventLogError>> {
        self.inner
            .projection_find(projection, tenant, field, value, limit)
    }
    fn projection_list<'a>(
        &'a self,
        projection: &'a ProjectionSpec,
        tenant: &'a TenantId,
        after_key: Option<&'a str>,
        limit: usize,
    ) -> BoxFuture<'a, Result<Vec<(String, Value)>, EventLogError>> {
        self.inner
            .projection_list(projection, tenant, after_key, limit)
    }
    fn stream_identity<'a>(
        &'a self,
        tenant: &'a TenantId,
    ) -> BoxFuture<'a, Result<String, EventLogError>> {
        self.inner.stream_identity(tenant)
    }
    fn put_blob<'a>(
        &'a self,
        tenant: &'a TenantId,
        digest: &'a str,
        bytes: &'a [u8],
    ) -> BoxFuture<'a, Result<(), EventLogError>> {
        self.inner.put_blob(tenant, digest, bytes)
    }
    fn get_blob<'a>(
        &'a self,
        tenant: &'a TenantId,
        digest: &'a str,
    ) -> BoxFuture<'a, Result<Option<Vec<u8>>, EventLogError>> {
        self.inner.get_blob(tenant, digest)
    }
    fn delete_blob<'a>(
        &'a self,
        tenant: &'a TenantId,
        digest: &'a str,
    ) -> BoxFuture<'a, Result<(), EventLogError>> {
        self.inner.delete_blob(tenant, digest)
    }
}

/// A guard that refuses every group it is shown.
struct RefuseAll;

const REFUSED: &str = "adversary.two.refused";

impl Guard for RefuseAll {
    fn check<'a>(
        &'a self,
        _store: &'a mut dyn ProjectionStore,
    ) -> BoxFuture<'a, Result<(), EventLogError>> {
        Box::pin(async {
            Err(EventLogError::GuardRefused {
                code: REFUSED.into(),
            })
        })
    }
}

#[test]
fn the_port_default_fails_closed_and_binds_no_blob() {
    block_on(async {
        let tenant = TenantId::new("adversary-two-port-default").expect("valid tenant");
        let inner = Arc::new(
            SqliteEventStore::in_memory("adversary_two_default")
                .await
                .expect("SQLite memory provider"),
        );
        let store = Arc::new(DefaultOnly {
            inner: inner.clone(),
        });

        let bytes = b"adversary-two-blob".to_vec();
        let digest = eventlog_core::blob_integrity_sha256(&bytes);

        let settled = store
            .append_group_guarded_with_blobs(
                &group(&tenant, &digest, "default"),
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
            inner.get_blob(&tenant, &digest).await.expect("blob read"),
            None,
            "the default binds no blob — an orphan on this path is written by the adapter's own \
             fallback loop, not by the port"
        );
        let stream = StreamId::new(tenant.clone(), "adversary", "one").expect("stream id");
        assert_eq!(
            inner.stream_version(&stream).await.expect("stream head"),
            None,
            "the default commits no group"
        );
    });
}

#[test]
fn the_sqlite_override_commits_a_group_with_its_blob_and_a_refused_guard_binds_neither() {
    block_on(async {
        let tenant = TenantId::new("adversary-two-sqlite-override").expect("valid tenant");
        let store = Arc::new(
            SqliteEventStore::in_memory("adversary_two_override")
                .await
                .expect("SQLite memory provider"),
        );
        let stream = StreamId::new(tenant.clone(), "adversary", "one").expect("stream id");

        let bytes = b"adversary-two-override-blob".to_vec();
        let digest = eventlog_core::blob_integrity_sha256(&bytes);
        let blobs = [(digest.clone(), bytes.clone())];

        // A refusing guard: admission runs before any blob is bound, so neither lands.
        let refused = store
            .append_group_guarded_with_blobs(
                &group(&tenant, &digest, "refused"),
                Arc::new(RefuseAll),
                &blobs,
            )
            .await;
        assert!(
            matches!(&refused, Err(EventLogError::GuardRefused { code }) if code == REFUSED),
            "SQLite implements the method, so the guard's own refusal is what comes back, not \
             UNAVAILABLE. It answered: {refused:?}"
        );
        assert_eq!(
            store.get_blob(&tenant, &digest).await.expect("blob read"),
            None,
            "a refused guard binds no blob on SQLite: the blob and the group share one transaction"
        );
        assert_eq!(
            store.stream_version(&stream).await.expect("stream head"),
            None,
            "a refused guard commits no event"
        );

        // An admitting guard: the group and its blob commit in the same call.
        let committed = store
            .append_group_guarded_with_blobs(
                &group(&tenant, &digest, "admitted"),
                Arc::new(NoGuard),
                &blobs,
            )
            .await
            .expect("SQLite commits a guarded blob-bearing group itself");
        assert_eq!(committed.appends.len(), 1);
        assert!(!committed.deduplicated, "a fresh group is not a replay");
        assert_eq!(
            store.get_blob(&tenant, &digest).await.expect("blob read"),
            Some(bytes),
            "the blob is bound by the same call that committed the group"
        );
        assert_eq!(
            store.stream_version(&stream).await.expect("stream head"),
            Some(1),
            "the group's one event is committed"
        );
    });
}
