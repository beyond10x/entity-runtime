//! A batch-bearing retry is admitted again, so `ImportGuard` has to tolerate its own commit.
//!
//! `AtomicEventStore::append_group_guarded_with_blobs` (eventlog `7fbd37cf`) documents it:
//!
//! > A retry that carries a batch is admitted again, so the guard must tolerate its own commit.
//! > […] admission runs first, which means a guard can be presented a second time with a group it
//! > already admitted and committed: it has to be idempotent over that, and refuse only what is
//! > genuinely somebody else's. A guard that reads "this subject is occupied" without checking
//! > *by what* will refuse the retry that idempotency exists to serve.
//!
//! The consequence of getting it wrong is not a bad error message. The caller's only recovery
//! from a refused retry is a new command key, and a new command key appends every member of the
//! batch a second time into an append-only log.
//!
//! `import_anchors` reaches the second admission whenever it submits a group whose members its
//! own pre-capture did not yet show as committed — a concurrent importer, or a lost reply whose
//! commit is not yet visible. `ReplayingBackend` makes that deterministic instead of racy: it
//! forwards every call to a real provider, and submits each blob-bearing group **twice**,
//! returning the second answer. The first submission is the commit; the second is the retry the
//! port describes, admitted again over what the first one wrote.
//!
//! The File provider is the subject because it implements the method. A provider taking the
//! port's default refuses with `UNAVAILABLE` and never reaches admission at all.
#![cfg(feature = "file")]

use std::sync::{Arc, Mutex};

use entity_core::{Registry, Runtime};
use entity_eventlog::{
    AsyncBindingProvisioner, AsyncImportedAnchorWriter, Authority, ErRecordedProjector,
    EventlogBackend, EventlogBindingProvisioner, EventlogOperationContext, EventlogRecordedStore,
    ImportAnchorFailure, projection_specs,
};
use entity_executor::{ExecuteRequest, Executor};
use entity_store::{
    Recording,
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

/// A provider with two knobs, each making one otherwise-concurrent situation deterministic.
///
/// `replay` submits every blob-bearing group twice and answers with the second result: the retry
/// that carries its batch and is therefore admitted again.
///
/// `pinned` answers exactly one `capture_tenant` from a snapshot taken earlier and every later
/// one from the provider. That is what a second importer sees when it began before another writer
/// committed — a pre-capture from before, a post-capture from after — and it is the only way the
/// destination guard is reached at all: under a single writer the adapter's own pre-capture
/// answers every refusal the guard could, from the same authority, before a byte is uploaded.
struct ReplayingBackend {
    inner: Arc<dyn EventlogBackend>,
    replay: bool,
    pinned: Mutex<Option<TenantCapture>>,
    /// The event ids each submission of each group returned, in submission order.
    submissions: Mutex<Vec<Vec<String>>>,
}

impl ReplayingBackend {
    fn new(inner: Arc<dyn EventlogBackend>) -> Self {
        Self {
            inner,
            replay: true,
            pinned: Mutex::new(None),
            submissions: Mutex::new(Vec::new()),
        }
    }

    /// A handle that forwards faithfully, until `arm` gives it one stale answer to hand back.
    fn stale(inner: Arc<dyn EventlogBackend>) -> Self {
        Self {
            inner,
            replay: false,
            pinned: Mutex::new(None),
            submissions: Mutex::new(Vec::new()),
        }
    }

    fn arm(&self, capture: TenantCapture) {
        *self.pinned.lock().expect("pinned capture") = Some(capture);
    }

    fn record(&self, result: &AppendGroupResult) {
        self.submissions.lock().expect("submissions").push(
            result
                .appends
                .iter()
                .flat_map(|append| append.events.iter().map(|event| event.event_id.clone()))
                .collect(),
        );
    }
}

impl AtomicEventStore for ReplayingBackend {
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
        Box::pin(async move {
            let first = self
                .inner
                .append_group_guarded_with_blobs(group, admission.clone(), blobs)
                .await?;
            self.record(&first);
            if !self.replay {
                return Ok(first);
            }
            // The retry: same group, same blobs, same guard, after the commit above.
            let second = self
                .inner
                .append_group_guarded_with_blobs(group, admission, blobs)
                .await?;
            self.record(&second);
            Ok(second)
        })
    }
}

impl ConsistentTenantCapture for ReplayingBackend {
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

impl InlineProjectionAdmin for ReplayingBackend {
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

impl EventStore for ReplayingBackend {
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
        subject: "adapter-provider-test".into(),
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
        "operations": {}
    }))
    .expect("definition parses");
    let mut registry = Registry::new();
    registry.register(definition).expect("definition validates");
    registry
}

fn imported(label: &str) -> SubjectHistory {
    let registry = registry();
    let decision = Runtime::new(&registry)
        .create("ticket", 1, label, json!({ "title": label }))
        .expect("legacy decision");
    let commit = entity_store::RecordedCommit::new(
        decision,
        &Recording {
            record_id: format!("{label}-create"),
            recorded_at: "2026-09-16T00:00:00Z".into(),
            correlation: None,
            causation: None,
            actor: None,
        },
    )
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
                    "retry-admission".to_owned(),
                    "records/0",
                    KnownLegacyOrder::PerKind(0),
                )
                .expect("imported evidence"),
            )],
        }),
        records: Vec::new(),
    }
}

#[test]
fn a_batch_bearing_retry_is_admitted_over_the_commit_it_already_made() {
    block_on(async {
        let directory = tempfile::tempdir().expect("temporary directory");
        let tenant = TenantId::new("import-batch-retry").expect("valid tenant");
        let file = Arc::new(
            eventlog_file::FileEventStore::open(directory.path())
                .await
                .expect("file provider"),
        );
        let stream_identity = file
            .stream_identity(&tenant)
            .await
            .expect("tenant identity");
        let projector = Arc::new(ErRecordedProjector::new());
        file.create_projections(projector.clone())
            .await
            .expect("projection admission");
        file.attach_inline_existing(projector)
            .await
            .expect("projection attachment");
        let authority = Authority {
            logical_scope: "import-batch-retry-scope".into(),
            tenant: tenant.as_str().to_owned(),
            stream_identity,
        };
        let inner: Arc<dyn EventlogBackend> = file;
        EventlogBindingProvisioner::new(inner.clone(), LIMITS)
            .provision_binding(authority.clone(), context("retry"))
            .await
            .expect("binding provisioned");

        let replaying = Arc::new(ReplayingBackend::new(inner));
        let erased: Arc<dyn EventlogBackend> = replaying.clone();
        let store = EventlogRecordedStore::open(erased, authority.clone(), LIMITS)
            .await
            .expect("bound store");
        let operation = store.operation(context("retry-run"));

        let histories = vec![imported("retry-a"), imported("retry-b")];
        let outcomes = operation
            .import_anchors(histories.clone())
            .await
            .expect("a batch whose own retry is admitted again still settles");

        assert_eq!(outcomes.len(), 2);

        // The retry appended nothing: both submissions answer with the same physical events.
        let submissions = replaying.submissions.lock().expect("submissions").clone();
        assert_eq!(submissions.len(), 2, "the group was submitted twice");
        assert_eq!(
            submissions[1], submissions[0],
            "the retry must answer with the events the first commit made, not new ones"
        );

        // And the destination holds each subject once, with the anchor the first commit wrote.
        let snapshot = store
            .complete_snapshot(&authority.logical_scope)
            .await
            .expect("snapshot");
        assert_eq!(
            snapshot.histories.len(),
            2,
            "a re-admitted batch must not append its members a second time"
        );
        for history in &histories {
            assert!(
                matches!(
                    store
                        .history(&history.subject)
                        .await
                        .expect("history")
                        .origin,
                    HistoryOrigin::Imported(_)
                ),
                "{:?} kept its imported boundary",
                history.subject
            );
        }
    });
}

/// Setup shared by the stale-capture cases: a real File destination and its authority.
async fn destination(
    directory: &std::path::Path,
    label: &str,
) -> (Arc<dyn EventlogBackend>, Authority, TenantId) {
    let tenant = TenantId::new(format!("import-guard-{label}")).expect("valid tenant");
    let file = Arc::new(
        eventlog_file::FileEventStore::open(directory)
            .await
            .expect("file provider"),
    );
    let stream_identity = file
        .stream_identity(&tenant)
        .await
        .expect("tenant identity");
    let projector = Arc::new(ErRecordedProjector::new());
    file.create_projections(projector.clone())
        .await
        .expect("projection admission");
    file.attach_inline_existing(projector)
        .await
        .expect("projection attachment");
    let authority = Authority {
        logical_scope: format!("import-guard-{label}-scope"),
        tenant: tenant.as_str().to_owned(),
        stream_identity,
    };
    let inner: Arc<dyn EventlogBackend> = file;
    EventlogBindingProvisioner::new(inner.clone(), LIMITS)
        .provision_binding(authority.clone(), context(label))
        .await
        .expect("binding provisioned");
    (inner, authority, tenant)
}

fn registry_with_touch() -> Registry {
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

fn imported_with(label: &str, record_id: &str, locator: &str) -> SubjectHistory {
    let registry = registry();
    let decision = Runtime::new(&registry)
        .create("ticket", 1, label, json!({ "title": label }))
        .expect("legacy decision");
    let commit = entity_store::RecordedCommit::new(
        decision,
        &Recording {
            record_id: record_id.to_owned(),
            recorded_at: "2026-09-16T00:00:00Z".into(),
            correlation: None,
            causation: None,
            actor: None,
        },
    )
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
                    "retry-admission".to_owned(),
                    locator.to_owned(),
                    KnownLegacyOrder::PerKind(0),
                )
                .expect("imported evidence"),
            )],
        }),
        records: Vec::new(),
    }
}

/// The guard's record arm, reached the only way it can be.
///
/// Under a single writer the adapter's pre-capture answers this from the same authority before a
/// byte is uploaded, so the guard never sees it. A second importer whose pre-capture predates
/// another writer's commit is what leaves the collision for the guard to find.
#[test]
fn the_guard_refuses_a_record_identity_another_writer_committed_after_the_pre_capture() {
    block_on(async {
        let directory = tempfile::tempdir().expect("temporary directory");
        let (inner, authority, tenant) = destination(directory.path(), "record").await;

        let stale = Arc::new(ReplayingBackend::stale(inner.clone()));
        let erased: Arc<dyn EventlogBackend> = stale.clone();
        let second = EventlogRecordedStore::open(erased, authority.clone(), LIMITS)
            .await
            .expect("bound store");

        // What the second importer's pre-capture saw: nothing imported yet.
        let before = inner
            .capture_tenant(&tenant, projection_specs(), LIMITS)
            .await
            .expect("capture before the other writer");

        // Another writer takes the record identity the second importer is about to claim.
        let first = EventlogRecordedStore::open(inner.clone(), authority.clone(), LIMITS)
            .await
            .expect("bound store");
        first
            .operation(context("record-first"))
            .import_anchor(imported_with(
                "guard-other",
                "contested-record",
                "records/0",
            ))
            .await
            .expect("the other writer commits the record identity");

        stale.arm(before);
        let refused = second
            .operation(context("record-second"))
            .import_anchors(vec![imported_with(
                "guard-mine",
                "contested-record",
                "records/0",
            )])
            .await
            .expect_err("the destination guard refuses a record identity another writer holds");
        assert!(
            matches!(
                &refused,
                ImportAnchorFailure::NotCommitted(AsyncStoreError::RecordConflict { record_id })
                    if record_id == "contested-record"
            ),
            "{refused:?}"
        );
        assert!(
            second
                .load(&Subject::new("ticket", "guard-mine").expect("subject"))
                .await
                .expect("state read")
                .is_none(),
            "nothing of the refused batch survives"
        );
    });
}

/// The guard's subject arm, reached the same way, with a *different* anchor on the subject.
///
/// An identical anchor is admitted — that is the idempotence the retry case holds. This is the
/// other side of the same comparison: a subject another writer answered for under different
/// bytes is refused, and the refusal names that subject and the revision it is actually at.
#[test]
fn the_guard_refuses_a_subject_another_writer_answered_for_under_different_bytes() {
    block_on(async {
        let directory = tempfile::tempdir().expect("temporary directory");
        let (inner, authority, tenant) = destination(directory.path(), "subject").await;

        let stale = Arc::new(ReplayingBackend::stale(inner.clone()));
        let erased: Arc<dyn EventlogBackend> = stale.clone();
        let second = EventlogRecordedStore::open(erased, authority.clone(), LIMITS)
            .await
            .expect("bound store");

        let before = inner
            .capture_tenant(&tenant, projection_specs(), LIMITS)
            .await
            .expect("capture before the other writer");

        let first = EventlogRecordedStore::open(inner.clone(), authority.clone(), LIMITS)
            .await
            .expect("bound store");
        first
            .operation(context("subject-first"))
            .import_anchor(imported_with("contested", "theirs", "records/0"))
            .await
            .expect("the other writer commits the subject");

        stale.arm(before);
        let refused = second
            .operation(context("subject-second"))
            .import_anchors(vec![imported_with("contested", "mine", "records/1")])
            .await
            .expect_err("the destination guard refuses a subject another writer answers for");
        match &refused {
            ImportAnchorFailure::NotCommitted(AsyncStoreError::RevisionConflict {
                subject,
                expected,
                found,
            }) => {
                assert_eq!(subject.id, "contested", "{refused:?}");
                assert!(
                    !(matches!(expected, entity_store::Expect::Absent) && found.is_none()),
                    "expected {expected:?} and found {found:?} describes no conflict: {refused:?}"
                );
            }
            other => panic!("unexpected refusal shape: {other:?}"),
        }
    });
}

/// One anchor whose own evidence names a record identity twice.
///
/// This case exists to say where that is answered, because the answer decided whether the
/// destination guard keeps an in-batch duplicate arm. It does not: the refusal comes from
/// `verify_subject_history`, before any capture, any upload and any admission. Together with
/// `a_batch_refuses_a_record_identity_two_of_its_members_share` — which covers the cross-member
/// case from the batch's own capture — that leaves the guard with no way to be handed a duplicate
/// key at all, so the arm that refused one was removed rather than left as unreachable cover.
/// If this ever stops refusing here, that decision is the one to revisit.
#[test]
fn one_anchor_naming_a_record_identity_twice_is_refused() {
    block_on(async {
        let directory = tempfile::tempdir().expect("temporary directory");
        let (inner, authority, _) = destination(directory.path(), "twice").await;
        let store = EventlogRecordedStore::open(inner, authority, LIMITS)
            .await
            .expect("bound store");

        let mut history = imported_with("doubled", "doubled-create", "records/0");
        let HistoryOrigin::Imported(anchor) = &mut history.origin else {
            panic!("fixture is imported");
        };
        let duplicate = anchor.evidence[0].clone();
        anchor.evidence.push(duplicate);

        let refused = store
            .operation(context("twice-run"))
            .import_anchor(history)
            .await
            .expect_err("one anchor cannot claim one record identity twice");
        assert!(
            matches!(
                &refused,
                ImportAnchorFailure::NotCommitted(AsyncStoreError::CorruptHistory { subject, .. })
                    if subject.id == "doubled"
            ),
            "the refusal must come from history verification, above the capture and the guard: \
             {refused:?}"
        );
    });
}

/// A subject this anchor started and somebody has since appended to is not "its own row".
///
/// `fold_subject_source` (`projection.rs:371-378`) copies `origin` verbatim onto every later
/// entry, so after a suffix the subject row still names the importing anchor under
/// `origin.anchor_blob`. `state_source` is what moves: the import writes
/// `{"kind":"anchor","anchor_blob":…}` and the first suffix entry replaces it. A guard comparing
/// `origin` alone calls the moved row its own and admits it.
///
/// Admitting it is not a data-safety failure — the group's `Expected::NoStream` refuses the same
/// condition immediately afterwards, and nothing commits either way. What changes is *who*
/// answers, and that is observable: a guard refusal is returned from the typed slot without
/// another look at the destination, while falling through to the group's conflict costs a second
/// authoritative capture to work out what happened. This case pins the capture count, so that a
/// guard which stops recognising a moved row is caught by something other than a reviewer.
#[test]
fn the_guard_refuses_a_subject_its_anchor_started_and_something_has_since_appended_to() {
    block_on(async {
        let directory = tempfile::tempdir().expect("temporary directory");
        let (inner, authority, tenant) = destination(directory.path(), "moved").await;

        let stale = Arc::new(ReplayingBackend::stale(inner.clone()));
        let erased: Arc<dyn EventlogBackend> = stale.clone();
        let second = EventlogRecordedStore::open(erased, authority.clone(), LIMITS)
            .await
            .expect("bound store");

        // What the second importer's pre-capture saw: nothing imported yet.
        let before = inner
            .capture_tenant(&tenant, projection_specs(), LIMITS)
            .await
            .expect("capture before the other writer");

        // Another writer imports the very same anchor, then appends a suffix to that subject, so
        // the row keeps this anchor in `origin` and moves on in `state_source`.
        let history = imported_with("moved-on", "moved-on-create", "records/0");
        let first = EventlogRecordedStore::open(inner.clone(), authority.clone(), LIMITS)
            .await
            .expect("bound store");
        first
            .operation(context("moved-first"))
            .import_anchor(history.clone())
            .await
            .expect("the other writer imports the anchor");
        Executor::new(
            &registry_with_touch(),
            &first.operation(context("moved-touch")),
        )
        .execute(ExecuteRequest {
            subject: Subject::new("ticket", "moved-on").expect("subject"),
            expected_revision: 1,
            operation: "touch".into(),
            arguments: json!({}),
            fulfillments: Default::default(),
            recording: Recording {
                record_id: "moved-on-touch".into(),
                recorded_at: "2026-09-16T00:00:00Z".into(),
                correlation: None,
                causation: None,
                actor: None,
            },
        })
        .await
        .expect("the other writer appends a suffix");

        stale.arm(before);
        let taken = second.calls().captures;
        let refused = second
            .operation(context("moved-second"))
            .import_anchors(vec![history])
            .await
            .expect_err("a subject that has moved on is not this batch's own row");
        assert!(
            matches!(
                &refused,
                ImportAnchorFailure::NotCommitted(AsyncStoreError::RevisionConflict {
                    subject,
                    ..
                }) if subject.id == "moved-on"
            ),
            "{refused:?}"
        );
        assert_eq!(
            second.calls().captures - taken,
            1,
            "the guard recognised the moved row as somebody else's and refused from its own \
             admission; a second capture here means it admitted the row and the group's \
             Expected::NoStream had to answer instead: {refused:?}"
        );
    });
}
