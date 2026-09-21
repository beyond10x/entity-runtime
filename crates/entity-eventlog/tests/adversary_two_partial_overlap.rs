//! Adversary pass 2, unit 10: a *partially* overlapping group — some members already committed,
//! some not — is admitted by the idempotent guard and then reports the wrong subject.
//!
//! `f0d618b7` made `ImportGuard` idempotent over its own commit: an occupied record or subject row
//! is admitted when it names this member's own `anchor_blob`
//! (`crates/entity-eventlog/src/adapter.rs:2359-2401`). The commit message and
//! `docs/design/eventlog-recorded-errors-import-v0.1.md` step 4 justify it for a retry of a group
//! the guard itself committed, and `tests/import_batch_retry_admission.rs` proves that case: the
//! *whole* group is a retry, the port answers from the command it recorded, and nothing is
//! appended twice.
//!
//! Nothing proves the partial case. `import_batch_retry_admission.rs:15-19` names its own reacher:
//!
//! > `import_anchors` reaches the second admission whenever it submits a group whose members its
//! > own pre-capture did not yet show as committed — a concurrent importer, or a lost reply whose
//! > commit is not yet visible.
//!
//! A concurrent importer does not reliably overlap *completely*. Two `import_anchors` calls over
//! an overlapping selection — the shape a resumed migration takes — give the second one a group
//! whose members are partly committed and partly not. `StaleCaptureBackend` below makes that
//! deterministic rather than racy: it answers exactly one `capture_tenant` from a snapshot taken
//! before the other importer committed, and every later one from the provider. That is what a
//! concurrent importer sees — a pre-capture from before the other writer, a post-capture from
//! after it — and it uses no seam the adapter does not already have.
//!
//! What happens: the guard admits the committed member (its row names that member's own digest)
//! and the fresh one, the group's `Expected::NoStream` then refuses on the committed member, and
//! `adapter.rs:2222-2236` recaptures and reports **the first member whose anchor is absent**. The
//! committed member's anchor *is* present, so the member reported is the innocent one — as
//! `RevisionConflict { expected: Absent, found: None }`, which says the subject was expected
//! absent and was found absent.
//!
//! At the base of this round (`d80fe66`) the same call refused inside the guard, naming the member
//! that is actually occupied and its revision:
//! `self.refuse(RevisionConflict { subject, expected: Absent, found })` with
//! `found = body["revision"]`. The idempotence change is what moved the report onto the wrong
//! subject.
#![cfg(feature = "file")]

use std::sync::{Arc, Mutex};

use entity_core::{Registry, Runtime};
use entity_eventlog::{
    AsyncBindingProvisioner, AsyncImportedAnchorWriter, Authority, ErRecordedProjector,
    EventlogBackend, EventlogBindingProvisioner, EventlogOperationContext, EventlogRecordedStore,
    ImportAnchorFailure, projection_specs,
};
use entity_store::{
    Expect, Recording,
    asynchronous::{
        AsyncRecordedReader, AsyncStateReader, AsyncStoreError, HistoryOrigin,
        ImportedRecordEvidence, KnownLegacyOrder, LegacyAnchor, LegacyCompleteness, LegacyEvidence,
        LegacyOrderDeclaration, RecordedEntry, Subject, SubjectHistory,
    },
};
use eventlog_core::{
    AppendGroup, AppendGroupResult, AppendResult, AtomicEventStore, BoxFuture, CaptureError,
    CaptureLimits, CatchUpProgress, Claim, ClaimedCommand, CommandMeta, ConsistentTenantCapture,
    EventLogError, EventStore, Expected, FeedPage, Guard, InlineProjectionAdmin,
    InlineRebuildResult, NewEvent, ProjectionSpec, Projector, RecordedEvent, Snapshot, StreamId,
    StreamSlice, TenantCapture, TenantId,
};
use serde_json::{Value, json};
use time::OffsetDateTime;

const LIMITS: CaptureLimits = CaptureLimits {
    max_events: 128,
    max_blobs: 512,
    max_projection_rows: 512,
    max_payload_bytes: 4 * 1024 * 1024,
};

/// A provider that answers exactly one `capture_tenant` from a snapshot taken earlier.
///
/// Every other call, including the append group itself and every later capture, goes straight to
/// the real provider. This is what a second importer observes when the first one commits between
/// its pre-capture and its group.
struct StaleCaptureBackend {
    inner: Arc<dyn EventlogBackend>,
    pinned: Mutex<Option<TenantCapture>>,
}

impl StaleCaptureBackend {
    fn new(inner: Arc<dyn EventlogBackend>) -> Self {
        Self {
            inner,
            pinned: Mutex::new(None),
        }
    }

    /// Serve `capture` to the next `capture_tenant`, and only to that one.
    fn arm(&self, capture: TenantCapture) {
        *self.pinned.lock().expect("pinned capture") = Some(capture);
    }
}

impl ConsistentTenantCapture for StaleCaptureBackend {
    fn capture_tenant<'a>(
        &'a self,
        tenant: &'a TenantId,
        projections: &'a [ProjectionSpec],
        limits: CaptureLimits,
    ) -> BoxFuture<'a, Result<TenantCapture, CaptureError>> {
        Box::pin(async move {
            if let Some(pinned) = self.pinned.lock().expect("pinned capture").take() {
                return Ok(pinned);
            }
            self.inner.capture_tenant(tenant, projections, limits).await
        })
    }
}

impl AtomicEventStore for StaleCaptureBackend {
    fn append_group_guarded<'a>(
        &'a self,
        group: &'a AppendGroup,
        admission: Arc<dyn Guard>,
    ) -> BoxFuture<'a, Result<AppendGroupResult, EventLogError>> {
        self.inner.append_group_guarded(group, admission)
    }

    fn append_group_guarded_with_blobs<'a>(
        &'a self,
        group: &'a AppendGroup,
        admission: Arc<dyn Guard>,
        blobs: &'a [(String, Vec<u8>)],
    ) -> BoxFuture<'a, Result<AppendGroupResult, EventLogError>> {
        self.inner
            .append_group_guarded_with_blobs(group, admission, blobs)
    }
}

impl InlineProjectionAdmin for StaleCaptureBackend {
    fn attach_inline_existing(
        &self,
        projector: Arc<dyn Projector>,
    ) -> BoxFuture<'_, Result<(), EventLogError>> {
        self.inner.attach_inline_existing(projector)
    }
    fn rebuild_inline_projection<'a>(
        &'a self,
        projector_name: &'a str,
        tenant: &'a TenantId,
    ) -> BoxFuture<'a, Result<InlineRebuildResult, EventLogError>> {
        self.inner.rebuild_inline_projection(projector_name, tenant)
    }
}

impl EventStore for StaleCaptureBackend {
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

fn block_on<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime")
        .block_on(future)
}

fn context(label: &str) -> EventlogOperationContext {
    EventlogOperationContext {
        subject: "adversary-two-partial".into(),
        actor: "entity-eventlog-test".into(),
        request_id: format!("request-{label}"),
        trace_id: format!("trace-{label}"),
        causation_id: None,
        causation_depth: 0,
        occurred_at: OffsetDateTime::UNIX_EPOCH,
    }
}

fn registry() -> Registry {
    let definition = serde_json::from_value(json!({
        "entity": "ticket",
        "version": 1,
        "schema": { "fields": { "title": { "type": "string", "required": true } } },
        "lifecycle": { "initial": "open", "states": ["open"] },
        "operations": {
            "touch": {
                "transitions": [{ "from": "open", "to": "open" }],
                "arguments": { "fields": {} },
                "emits": []
            }
        }
    }))
    .expect("definition parses");
    let mut registry = Registry::new();
    registry.register(definition).expect("definition validates");
    registry
}

fn recording(record_id: &str) -> Recording {
    Recording {
        record_id: record_id.into(),
        recorded_at: "2026-09-16T00:00:00Z".into(),
        correlation: Some("adversary-two-partial".into()),
        causation: None,
        actor: None,
    }
}

fn imported(label: &str) -> SubjectHistory {
    let registry = registry();
    let decision = Runtime::new(&registry)
        .create("ticket", 1, label, json!({ "title": label }))
        .expect("legacy decision");
    let commit =
        entity_store::RecordedCommit::new(decision, &recording(&format!("{label}-create")))
            .expect("legacy record");
    SubjectHistory {
        subject: Subject::new("ticket", label).expect("legacy subject"),
        origin: HistoryOrigin::Imported(LegacyAnchor {
            instance: commit.instance.clone(),
            completeness: LegacyCompleteness::AvailableEvidenceOnly,
            order: LegacyOrderDeclaration::PerKindOnly,
            evidence: vec![LegacyEvidence::Envelope(
                ImportedRecordEvidence::new(
                    RecordedEntry::Decision(commit),
                    "adversary-two-source".to_owned(),
                    "records/0".to_owned(),
                    KnownLegacyOrder::PerKind(0),
                )
                .expect("imported evidence"),
            )],
        }),
        records: Vec::new(),
    }
}

#[test]
fn a_partially_overlapping_group_reports_the_member_that_is_actually_occupied() {
    block_on(async {
        let directory = tempfile::tempdir().expect("temporary directory");
        let tenant = TenantId::new("adversary-two-partial").expect("valid tenant");
        let provider = Arc::new(
            eventlog_file::FileEventStore::open(directory.path())
                .await
                .expect("file provider"),
        );
        let stream_identity = provider
            .stream_identity(&tenant)
            .await
            .expect("tenant identity");
        let projector = Arc::new(ErRecordedProjector::new());
        provider
            .create_projections(projector.clone())
            .await
            .expect("projection admission");
        provider
            .attach_inline_existing(projector)
            .await
            .expect("projection attachment");
        let authority = Authority {
            logical_scope: "adversary-two-partial-scope".to_owned(),
            tenant: tenant.as_str().to_owned(),
            stream_identity,
        };
        let real: Arc<dyn EventlogBackend> = provider;
        EventlogBindingProvisioner::new(real.clone(), LIMITS)
            .provision_binding(authority.clone(), context("partial"))
            .await
            .expect("binding provisioned");

        // The second importer's handle, over a provider that will answer one capture from before
        // the first importer committed.
        let stale = Arc::new(StaleCaptureBackend::new(real.clone()));
        let erased: Arc<dyn EventlogBackend> = stale.clone();
        let second = EventlogRecordedStore::open(erased, authority.clone(), LIMITS)
            .await
            .expect("bound store");

        let shared = imported("partial-shared");
        let fresh = imported("partial-fresh");

        // What the second importer's pre-capture saw: the destination before anyone imported.
        let before = real
            .capture_tenant(&tenant, projection_specs(), LIMITS)
            .await
            .expect("capture before the other importer");

        // The first importer commits the shared member.
        let first = EventlogRecordedStore::open(real.clone(), authority.clone(), LIMITS)
            .await
            .expect("bound store");
        first
            .operation(context("partial-first"))
            .import_anchor(shared.clone())
            .await
            .expect("the first importer commits the shared member");

        // The second importer now submits the overlapping batch against its own older view.
        stale.arm(before);
        let refused = second
            .operation(context("partial-second"))
            .import_anchors(vec![shared.clone(), fresh.clone()])
            .await
            .expect_err("the overlapping batch cannot commit the shared member a second time");

        // Whatever the adapter decides to do with an overlapping batch, the member it names has to
        // be the member that is actually in the way, and a conflict report that says a subject was
        // expected absent and found absent describes nothing.
        match &refused {
            ImportAnchorFailure::NotCommitted(AsyncStoreError::RevisionConflict {
                subject,
                expected,
                found,
            }) => {
                assert_eq!(
                    (&subject.entity, &subject.id),
                    (&shared.subject.entity, &shared.subject.id),
                    "the batch named {subject:?} — the member nothing holds — where \
                     {:?} is the one the destination already answers for; full refusal: {refused:?}",
                    shared.subject
                );
                assert!(
                    !(matches!(expected, Expect::Absent) && found.is_none()),
                    "expected {expected:?} and found {found:?} describes no conflict at all: \
                     {refused:?}"
                );
            }
            other => panic!("unexpected refusal shape: {other:?}"),
        }

        // Nothing of the second importer's batch was committed either way.
        assert!(
            first
                .load(&fresh.subject)
                .await
                .expect("state read")
                .is_none(),
            "the fresh member must not survive a refused batch"
        );
    });
}

/// The same construction, asking only what was *written*.
///
/// The finding above is about the report, not the data, and this is what says so: on the
/// partially overlapping group the destination keeps the member the first importer committed and
/// gains nothing of the second importer's batch. If this one ever goes red the severity of the
/// one above changes completely, so it is here rather than inferred.
#[test]
fn a_partially_overlapping_group_commits_no_part_of_itself() {
    block_on(async {
        let directory = tempfile::tempdir().expect("temporary directory");
        let tenant = TenantId::new("adversary-two-partial-safety").expect("valid tenant");
        let provider = Arc::new(
            eventlog_file::FileEventStore::open(directory.path())
                .await
                .expect("file provider"),
        );
        let stream_identity = provider
            .stream_identity(&tenant)
            .await
            .expect("tenant identity");
        let projector = Arc::new(ErRecordedProjector::new());
        provider
            .create_projections(projector.clone())
            .await
            .expect("projection admission");
        provider
            .attach_inline_existing(projector)
            .await
            .expect("projection attachment");
        let authority = Authority {
            logical_scope: "adversary-two-partial-safety-scope".to_owned(),
            tenant: tenant.as_str().to_owned(),
            stream_identity,
        };
        let real: Arc<dyn EventlogBackend> = provider;
        EventlogBindingProvisioner::new(real.clone(), LIMITS)
            .provision_binding(authority.clone(), context("safety"))
            .await
            .expect("binding provisioned");

        let stale = Arc::new(StaleCaptureBackend::new(real.clone()));
        let erased: Arc<dyn EventlogBackend> = stale.clone();
        let second = EventlogRecordedStore::open(erased, authority.clone(), LIMITS)
            .await
            .expect("bound store");

        let shared = imported("safety-shared");
        let fresh = imported("safety-fresh");

        let before = real
            .capture_tenant(&tenant, projection_specs(), LIMITS)
            .await
            .expect("capture before the other importer");

        let first = EventlogRecordedStore::open(real.clone(), authority.clone(), LIMITS)
            .await
            .expect("bound store");
        first
            .operation(context("safety-first"))
            .import_anchor(shared.clone())
            .await
            .expect("the first importer commits the shared member");

        stale.arm(before);
        second
            .operation(context("safety-second"))
            .import_anchors(vec![shared.clone(), fresh.clone()])
            .await
            .expect_err("the overlapping batch cannot commit the shared member a second time");

        assert!(
            first
                .load(&fresh.subject)
                .await
                .expect("state read")
                .is_none(),
            "the fresh member must not survive a refused batch"
        );
        assert_eq!(
            first
                .complete_snapshot(&authority.logical_scope)
                .await
                .expect("snapshot")
                .histories
                .len(),
            1,
            "the destination holds only what the first importer committed"
        );
        assert!(
            first
                .operation(context("safety-replay"))
                .import_anchor(shared.clone())
                .await
                .expect("singular replay of the committed member")
                .replayed,
            "the committed member's bytes were not disturbed"
        );
    });
}
