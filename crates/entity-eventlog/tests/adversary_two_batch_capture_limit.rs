//! Adversary pass 2, unit 10: the batch is not bounded by the capture limit it must read itself
//! back with.
//!
//! `EventlogRecordedStore::open` takes one `CaptureLimits`, and `entity-cli` exposes all four as
//! caller-supplied flags (`crates/entity-cli/src/main.rs:887-906`, every value must be nonzero and
//! is otherwise the caller's). The same limits bound *every* capture the handle takes, including
//! the pre-capture and post-capture `import_anchors` makes around its group
//! (`crates/entity-eventlog/src/adapter.rs:2052`, `:2201`).
//!
//! `docs/design/eventlog-recorded-errors-import-v0.1.md` § "One capture and one group for a batch"
//! step 5 says the batch takes "one post-capture and verif[ies] every member's anchor bytes and
//! returned `PhysicalRef` before returning one outcome per input". It does not say what bounds the
//! group's size, and nothing does: `import_anchors` freezes one `AppendGroup` holding one append
//! per pending member and commits it without ever comparing the group it is about to write against
//! the limits the very next line has to read it back with.
//!
//! The singular `import_anchor` recaptures after every single append, so it can overshoot its own
//! reader by at most the one event it just wrote and the next call refuses. `import_anchors`
//! overshoots by the whole batch, in one commit, and then reports
//! `ImportAnchorFailure::Uncertain { cause: RecoveryUnavailable }` for a batch that committed
//! completely — leaving a destination the handle that wrote it can no longer read at all.
//!
//! This case measures both paths over the same six histories, at the same limits, on two
//! destinations carrying the same `Authority`. The assertion it fails on names both numbers, so
//! the singular path's own overshoot is in the failure output rather than asserted away.
#![cfg(feature = "file")]

use std::sync::Arc;

use entity_core::{Registry, Runtime};
use entity_eventlog::{
    AsyncBindingProvisioner, AsyncImportedAnchorWriter, Authority, ErRecordedProjector,
    EventlogBackend, EventlogBindingProvisioner, EventlogOperationContext, EventlogRecordedStore,
    projection_specs,
};
use entity_store::{
    Recording,
    asynchronous::{
        HistoryOrigin, ImportedRecordEvidence, KnownLegacyOrder, LegacyAnchor, LegacyCompleteness,
        LegacyEvidence, LegacyOrderDeclaration, RecordedEntry, Subject, SubjectHistory,
    },
};
use eventlog_core::{CaptureLimits, EventStore, InlineProjectionAdmin, TenantId};
use serde_json::json;
use time::OffsetDateTime;

/// The limits the importing handle is opened with. One binding event plus one `er.import_anchor`
/// event per imported subject, so four events is three imported subjects and the binding.
const TIGHT: CaptureLimits = CaptureLimits {
    max_events: 4,
    max_blobs: 512,
    max_projection_rows: 512,
    max_payload_bytes: 4 * 1024 * 1024,
};

/// What an observer outside the importing handle reads with, so that what was committed can be
/// counted whatever the importing handle can or cannot see.
const WIDE: CaptureLimits = CaptureLimits {
    max_events: 4096,
    max_blobs: 4096,
    max_projection_rows: 4096,
    max_payload_bytes: 4 * 1024 * 1024,
};

const MEMBERS: usize = 6;

fn block_on<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime")
        .block_on(future)
}

fn context(label: &str) -> EventlogOperationContext {
    EventlogOperationContext {
        subject: "adversary-two-capture-limit".into(),
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
        correlation: Some("adversary-two-capture-limit".into()),
        causation: None,
        actor: None,
    }
}

fn imported_history(label: &str) -> SubjectHistory {
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

fn copy_tree(from: &std::path::Path, to: &std::path::Path) {
    std::fs::create_dir_all(to).expect("destination directory");
    for entry in std::fs::read_dir(from).expect("readable source") {
        let entry = entry.expect("directory entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("entry type").is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).expect("copied file");
        }
    }
}

async fn provisioned(directory: &std::path::Path, label: &str) -> (Authority, TenantId) {
    let tenant = TenantId::new(format!("adversary-two-{label}")).expect("valid tenant");
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
        logical_scope: format!("adversary-two-{label}-scope"),
        tenant: tenant.as_str().to_owned(),
        stream_identity,
    };
    let erased: Arc<dyn EventlogBackend> = backend;
    EventlogBindingProvisioner::new(erased.clone(), TIGHT)
        .provision_binding(authority.clone(), context(label))
        .await
        .expect("binding provisioned");
    (authority, tenant)
}

async fn open_destination(
    directory: &std::path::Path,
    authority: &Authority,
) -> (Arc<dyn EventlogBackend>, EventlogRecordedStore) {
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
    let store = EventlogRecordedStore::open(erased.clone(), authority.clone(), TIGHT)
        .await
        .expect("bound store");
    (erased, store)
}

/// Every event the destination actually holds, counted with limits wide enough to see them all.
async fn committed_events(backend: &Arc<dyn EventlogBackend>, tenant: &TenantId) -> usize {
    backend
        .capture_tenant(tenant, projection_specs(), WIDE)
        .await
        .expect("observer capture")
        .events
        .len()
}

#[test]
fn a_batch_commits_no_more_than_its_own_handle_can_read_back() {
    block_on(async {
        let root = tempfile::tempdir().expect("temporary directory");
        let singular_directory = root.path().join("singular");
        let batch_directory = root.path().join("batch");

        // One provisioned destination, copied before the first import, so both runs carry the same
        // `Authority` and the only difference between them is singular against batch.
        let (authority, tenant) = provisioned(&singular_directory, "limit").await;
        copy_tree(&singular_directory, &batch_directory);

        let histories: Vec<SubjectHistory> = (0..MEMBERS)
            .map(|ordinal| imported_history(&format!("limit-subject-{ordinal}")))
            .collect();

        // The singular path, one at a time, until it refuses. It recaptures after every append, so
        // the first call that puts the destination past `TIGHT.max_events` is also the last one
        // that gets to write.
        let (singular_backend, singular_store) =
            open_destination(&singular_directory, &authority).await;
        let singular_operation = singular_store.operation(context("limit-singular"));
        let mut singular_accepted = 0usize;
        for history in &histories {
            match singular_operation.import_anchor(history.clone()).await {
                Ok(_) => singular_accepted += 1,
                Err(_) => break,
            }
        }
        let singular_events = committed_events(&singular_backend, &tenant).await;

        // The batch path, one call, same six histories, same limits.
        let (batch_backend, batch_store) = open_destination(&batch_directory, &authority).await;
        let batch_operation = batch_store.operation(context("limit-batch"));
        let batch_outcome = batch_operation.import_anchors(histories.clone()).await;
        let batch_events = committed_events(&batch_backend, &tenant).await;

        // What the caller was told, for the record: the batch does not report a commit.
        let batch_reported_failure = batch_outcome.is_err();

        // A writer must not commit its destination into a state its own reader cannot read. The
        // singular path's own violation of this is one event wide and is named in the message, so
        // that the two numbers are compared rather than one of them asserted away.
        assert!(
            batch_events <= usize::try_from(TIGHT.max_events).expect("limit fits"),
            "import_anchors committed {batch_events} events into a destination whose own handle \
             reads at max_events={}, after a pre-capture that fitted; the singular path over the \
             same {MEMBERS} histories accepted {singular_accepted} and stopped at \
             {singular_events} events. The batch call reported failure: {batch_reported_failure}, \
             so every receipt for what it did commit is lost.",
            TIGHT.max_events
        );
    });
}
