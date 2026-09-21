//! Conformance check: a batch must answer a global record-identity collision the way N singular
//! calls answer it.
//!
//! `story:recorded-store-imports-a-batch-with-one-capture` (ER canonical store, draft rev 3):
//!
//! > Byte identity: for the same input, `import_anchors` produces the same anchors, subject stream
//! > ids, blob keys and receipts as N calls of `import_anchor`.
//!
//! and `docs/design/eventlog-recorded-errors-import-v0.1.md` § "One capture and one group for a
//! batch", step 4:
//!
//! > One `ImportGuard` locks the binding row once and then, per member in the group's order, that
//! > member's global record keys in the selected total order and its subject key — **the same
//! > checks the singular guard performs**.
//!
//! `ImportGuard::check` runs at admission, which `AtomicEventStore::append_group_guarded` runs
//! "once, before entries". A member's own record key is therefore not in the store yet when a
//! later member of the *same group* is checked, so the guard cannot see a collision between two
//! members of one batch. N singular calls each commit before the next is admitted, so the second
//! is refused with the typed `RecordConflict { record_id }` — "one global record identity names
//! different complete bytes or request intent".
//!
//! Nothing of the batch is committed, so atomicity holds. What differs is the receipt the caller
//! gets: the collision falls through the guard to the inline projector's
//! `upsert_once` (`src/projection.rs:352`), whose `EventLogError::Invalid` `map_append_error`
//! turns into `ProviderIntegrity { detail: "projection identity already names different
//! authority" }` — an error whose own definition is "provider authority is internally
//! inconsistent", raised for an ordinary caller-input fault, naming an authority that did not
//! change.
//!
//! The two destinations below carry the same `Authority` — provisioned once, then copied before
//! the first import — which is the setup
//! `providers.rs::a_batch_import_writes_the_same_bytes_as_singular_imports_from_one_capture`
//! uses, so the only difference between the runs is singular against batch.
#![cfg(feature = "file")]

use std::sync::Arc;

use entity_core::{Registry, Runtime};
use entity_eventlog::{
    AsyncBindingProvisioner, AsyncImportedAnchorWriter, Authority, ErRecordedProjector,
    EventlogBackend, EventlogBindingProvisioner, EventlogOperationContext, EventlogRecordedStore,
};
use entity_store::{
    Recording,
    asynchronous::{
        AsyncStoreError, HistoryOrigin, ImportedRecordEvidence, KnownLegacyOrder, LegacyAnchor,
        LegacyCompleteness, LegacyEvidence, LegacyOrderDeclaration, RecordedEntry, Subject,
        SubjectHistory,
    },
};
use eventlog_core::{CaptureLimits, EventStore, InlineProjectionAdmin, TenantId};
use serde_json::json;
use time::OffsetDateTime;

const LIMITS: CaptureLimits = CaptureLimits {
    max_events: 128,
    max_blobs: 512,
    max_projection_rows: 512,
    max_payload_bytes: 4 * 1024 * 1024,
};

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
        correlation: Some("adapter-provider-contract".into()),
        causation: None,
        actor: None,
    }
}

fn imported_batch_history_with(label: &str, record_id: &str, locator: &str) -> SubjectHistory {
    let registry = registry();
    let decision = Runtime::new(&registry)
        .create("ticket", 1, label, json!({ "title": label }))
        .expect("legacy decision");
    let commit =
        entity_store::RecordedCommit::new(decision, &recording(record_id)).expect("legacy record");
    SubjectHistory {
        subject: Subject::new("ticket", label).expect("legacy subject"),
        origin: HistoryOrigin::Imported(LegacyAnchor {
            instance: commit.instance.clone(),
            completeness: LegacyCompleteness::AvailableEvidenceOnly,
            order: LegacyOrderDeclaration::PerKindOnly,
            evidence: vec![LegacyEvidence::Envelope(
                ImportedRecordEvidence::new(
                    RecordedEntry::Decision(commit),
                    "import-batch-source".to_owned(),
                    locator.to_owned(),
                    KnownLegacyOrder::PerKind(0),
                )
                .expect("imported evidence"),
            )],
        }),
        records: Vec::new(),
    }
}

fn copy_tree(from: &std::path::Path, to: &std::path::Path) {
    std::fs::create_dir_all(to).expect("destination directory");
    for entry in std::fs::read_dir(from).expect("readable source directory") {
        let entry = entry.expect("directory entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("entry type").is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).expect("copied file");
        }
    }
}

async fn open_import_destination(
    directory: &std::path::Path,
    authority: &Authority,
) -> EventlogRecordedStore {
    let backend = Arc::new(
        eventlog_file::FileEventStore::open(directory)
            .await
            .expect("file provider"),
    );
    backend
        .attach_inline_existing(Arc::new(ErRecordedProjector::new()))
        .await
        .expect("projection attachment");
    let erased: Arc<dyn EventlogBackend> = backend;
    EventlogRecordedStore::open(erased, authority.clone(), LIMITS)
        .await
        .expect("bound store")
}

async fn provisioned_file_destination(directory: &std::path::Path, label: &str) -> Authority {
    let tenant = TenantId::new(format!("import-batch-{label}")).expect("valid tenant");
    let backend = Arc::new(
        eventlog_file::FileEventStore::open(directory)
            .await
            .expect("file provider"),
    );
    let stream_identity = backend
        .stream_identity(&tenant)
        .await
        .expect("tenant identity");
    let projector = Arc::new(ErRecordedProjector::new());
    backend
        .create_projections(projector.clone())
        .await
        .expect("projection admission");
    backend
        .attach_inline_existing(projector)
        .await
        .expect("projection attachment");
    let authority = Authority {
        logical_scope: format!("import-batch-{label}-scope"),
        tenant: tenant.as_str().to_owned(),
        stream_identity,
    };
    let erased: Arc<dyn EventlogBackend> = backend;
    EventlogBindingProvisioner::new(erased, LIMITS)
        .provision_binding(authority.clone(), context(label))
        .await
        .expect("binding provisioned");
    authority
}

#[test]
fn a_batch_refuses_a_record_identity_two_of_its_members_share() {
    block_on(async {
        let root = tempfile::tempdir().expect("temporary directory");
        let singular_directory = root.path().join("singular");
        let batch_directory = root.path().join("batch");

        let authority = provisioned_file_destination(&singular_directory, "shared-record").await;
        copy_tree(&singular_directory, &batch_directory);

        // Two distinct subjects whose evidence claims the same global record identity.
        let histories = vec![
            imported_batch_history_with("shared-a", "shared-record", "records/0"),
            imported_batch_history_with("shared-b", "shared-record", "records/0"),
        ];

        // N singular calls: the first commits, the destination guard refuses the second because
        // the global record key is already taken.
        let singular_store = open_import_destination(&singular_directory, &authority).await;
        let singular_operation = singular_store.operation(context("shared-record-run"));
        singular_operation
            .import_anchor(histories[0].clone())
            .await
            .expect("the first singular import commits");
        let singular_second = singular_operation
            .import_anchor(histories[1].clone())
            .await
            .expect_err("the second singular import is refused by the destination guard");
        assert!(
            matches!(
                singular_second,
                entity_eventlog::ImportAnchorFailure::NotCommitted(
                    AsyncStoreError::RecordConflict { .. }
                )
            ),
            "{singular_second:?}"
        );

        // The batch over the same two histories must reach the same answer: one group that
        // establishes both would give the destination two subjects answering for one record
        // identity, which N singular calls refuse to produce.
        let batch_store = open_import_destination(&batch_directory, &authority).await;
        let batch_operation = batch_store.operation(context("shared-record-run"));
        let batch = batch_operation.import_anchors(histories.clone()).await;
        assert!(
            batch.is_err(),
            "the batch committed a record identity two of its members share, which N singular \
             calls refuse: {batch:?}"
        );
        // The singular pair reports `RecordConflict`. Pin whatever the batch reports, so that a
        // later change to the refusal a caller sees is visible rather than silent.
        let batch_error = batch.expect_err("refused");
        assert!(
            matches!(
                batch_error,
                entity_eventlog::ImportAnchorFailure::NotCommitted(_)
            ),
            "a batch that establishes nothing must say so, not leave the caller uncertain: \
             {batch_error:?}"
        );
        assert_eq!(
            format!("{batch_error:?}"),
            format!("{singular_second:?}"),
            "the batch reports the refusal N singular calls report"
        );
    });
}

/// Conformance check: an input naming one subject twice with identical bytes.
///
/// The acceptance statement of `story:recorded-store-imports-a-batch-with-one-capture` is that
/// `import_anchors` "produces the same anchors, subject stream ids, blob keys and **receipts** as
/// N calls of `import_anchor`". For the input `[X, X]` the singular path produces two receipts:
/// the first commits, and the second is settled against the destination's own capture and
/// reported `replayed: true`.
///
/// `docs/design/eventlog-recorded-errors-import-v0.1.md` step 1 refuses this input, reasoning:
///
/// > A batch naming one subject twice is refused as `InvalidInput` before any provider call,
/// > because one group holds at most one `Expected::NoStream` append per stream and the singular
/// > path's per-call capture is what would otherwise settle the second mention against the first.
///
/// For a second mention carrying *different* bytes that is the conservative answer. For a second
/// mention carrying the *same* bytes it inverts the stated goal: settling the second mention
/// against the first is exactly what N singular calls do, and refusing is what disagrees with
/// them.
#[test]
fn a_batch_naming_one_subject_twice_settles_as_two_singular_calls_settle() {
    block_on(async {
        let root = tempfile::tempdir().expect("temporary directory");
        let singular_directory = root.path().join("singular");
        let batch_directory = root.path().join("batch");

        let authority = provisioned_file_destination(&singular_directory, "repeat").await;
        copy_tree(&singular_directory, &batch_directory);

        let history = imported_batch_history_with("repeated", "repeated-create", "records/0");

        // Two singular calls over the identical history: the second is an exact replay.
        let singular_store = open_import_destination(&singular_directory, &authority).await;
        let singular_operation = singular_store.operation(context("repeat-run"));
        let first = singular_operation
            .import_anchor(history.clone())
            .await
            .expect("the first singular import commits");
        let second = singular_operation
            .import_anchor(history.clone())
            .await
            .expect("the second singular import is an exact replay, not a refusal");
        assert!(!first.replayed);
        assert!(second.replayed);

        // The batch over the same input must settle the same way.
        let batch_store = open_import_destination(&batch_directory, &authority).await;
        let batch_operation = batch_store.operation(context("repeat-run"));
        let batch = batch_operation
            .import_anchors(vec![history.clone(), history.clone()])
            .await;
        assert_eq!(
            batch.as_ref().map(|outcomes| outcomes
                .iter()
                .map(|outcome| outcome.replayed)
                .collect::<Vec<_>>()),
            Ok(vec![first.replayed, second.replayed]),
            "the batch refuses an input two singular calls settle: {batch:?}"
        );
    });
}
