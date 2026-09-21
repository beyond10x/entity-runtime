//! Conformance check for the batch-import refusal contract on a non-file provider.
//!
//! `docs/design/eventlog-recorded-errors-import-v0.1.md` § "One capture and one group for a batch",
//! step 4, states without naming a provider:
//!
//! > Admission runs before any blob the provider binds, so a refused batch leaves no byte of
//! > itself behind — not even a bound orphan blob for a member the guard never objected to.
//!
//! and `CHANGELOG.md` repeats it to users as "a refused batch leaves no bound blob behind".
//!
//! `AtomicEventStore::append_group_guarded_with_blobs` promises that only of a provider that
//! *overrides* it: "Publishing nothing when the group refuses is the override's obligation; the
//! default may leave content-addressed orphan blobs". Of the three providers this crate has a
//! feature for, only `eventlog-file` overrides it.
//!
//! `tests/providers.rs::a_refused_member_leaves_no_part_of_the_batch_committed` asserts the
//! documented claim against `eventlog-file` only. This is the same assertion against the SQLite
//! provider, which the crate supports, provisions and imports into in the same file.
#![cfg(feature = "sqlite")]

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

fn imported_batch_history(label: &str) -> SubjectHistory {
    imported_batch_history_with(label, &format!("{label}-create"), "records/0")
}

/// Every blob the destination currently binds, by key.
async fn bound_digests(backend: &Arc<dyn EventlogBackend>, tenant: &TenantId) -> Vec<String> {
    backend
        .capture_tenant(tenant, projection_specs(), LIMITS)
        .await
        .expect("capture")
        .blobs
        .into_iter()
        .map(|blob| blob.digest)
        .collect()
}

#[test]
fn a_guard_refused_batch_binds_no_blob_on_the_sqlite_provider() {
    block_on(async {
        let tenant = TenantId::new("import-batch-sqlite-atomicity").expect("valid tenant");
        let backend = Arc::new(
            eventlog_sqlite::SqliteEventStore::in_memory("import_batch_atomicity")
                .await
                .expect("SQLite memory provider"),
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
            logical_scope: "import-batch-sqlite-atomicity-scope".to_owned(),
            tenant: tenant.as_str().to_owned(),
            stream_identity,
        };
        let erased: Arc<dyn EventlogBackend> = backend;
        EventlogBindingProvisioner::new(erased.clone(), LIMITS)
            .provision_binding(authority.clone(), context("sqlite-atomicity"))
            .await
            .expect("binding provisioned");

        let store = EventlogRecordedStore::open(erased.clone(), authority.clone(), LIMITS)
            .await
            .expect("bound store");
        let operation = store.operation(context("sqlite-atomicity-run"));

        // One subject imported alone, so that the record identity below is already taken.
        operation
            .import_anchor(imported_batch_history("taken"))
            .await
            .expect("first subject is imported alone");

        // A member the destination guard refuses: a record identity another subject already holds.
        let bound_before = bound_digests(&erased, &tenant).await;
        operation
            .import_anchors(vec![
                imported_batch_history("with-guard-a"),
                imported_batch_history_with("stolen", "taken-create", "records/0"),
            ])
            .await
            .expect_err("the destination guard refuses the batch");

        assert_eq!(
            bound_digests(&erased, &tenant).await,
            bound_before,
            "a refused batch binds no blob"
        );
    });
}
