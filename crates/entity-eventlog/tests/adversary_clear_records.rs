//! Adversarial probe of R-165 through the Eventlog recorded adapter: a `kernel/1` decision whose
//! `set` clears a field carries a non-empty `removed` under `er.record/1`, a framing that never
//! carried one before. The adapter commits it, retries it exactly, and a cold reopen verifies the
//! whole history from genesis and replays it to the instance `execute` returned.
#![cfg(all(feature = "sync-bridge", feature = "file"))]

use std::{collections::BTreeMap, sync::Arc};

use entity_core::{Registry, replay};
use entity_eventlog::{
    AsyncBindingProvisioner, Authority, ErRecordedProjector, EventlogBackend,
    EventlogBindingProvisioner, EventlogOperationContext, EventlogRecordedStore,
};
use entity_executor::{CreateRequest, ExecuteRequest, Executor};
use entity_store::{
    Recording,
    asynchronous::{
        AsyncRecordedReader, AsyncStateReader, RecordedEntry, Subject, record_comparison_bytes,
        record_domain, verify_subject_history,
    },
};
use eventlog_core::{CaptureLimits, EventStore, InlineProjectionAdmin, TenantId};
use serde_json::json;
use time::OffsetDateTime;

const LIMITS: CaptureLimits = CaptureLimits {
    max_events: 256,
    max_blobs: 1_024,
    max_projection_rows: 1_024,
    max_payload_bytes: 8 * 1024 * 1024,
};

fn block_on<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime")
        .block_on(future)
}

fn registry() -> Registry {
    let definition = serde_json::from_value(json!({
        "entity": "reminder",
        "version": 1,
        "schema": { "fields": {
            "title": { "type": "string", "required": true },
            "snoozed_until": { "type": "string" }
        }},
        "lifecycle": { "initial": "active", "states": ["active"] },
        "operations": { "wake": {
            "transitions": [{ "from": "active", "to": "active" }],
            "set": { "snoozed_until": { "cleared": true } },
            "emits": [{ "type": "Woken", "payload": { "title": "$fields.title" } }]
        }}
    }))
    .expect("definition parses");
    let mut registry = Registry::new();
    registry.register(definition).expect("definition validates");
    registry
}

fn context(label: &str) -> EventlogOperationContext {
    EventlogOperationContext {
        subject: "adversary-clear-records".into(),
        actor: "entity-eventlog-test".into(),
        request_id: format!("request-{label}"),
        trace_id: format!("trace-{label}"),
        causation_id: None,
        causation_depth: 0,
        occurred_at: OffsetDateTime::UNIX_EPOCH,
    }
}

fn recording(record_id: &str) -> Recording {
    Recording {
        record_id: record_id.into(),
        recorded_at: "2026-10-07T00:00:00Z".into(),
        correlation: None,
        causation: None,
        actor: None,
    }
}

fn wake(expected_revision: u64, record_id: &str) -> ExecuteRequest {
    ExecuteRequest {
        subject: Subject::new("reminder", "r-1").expect("subject"),
        expected_revision,
        operation: "wake".into(),
        arguments: json!({}),
        fulfillments: BTreeMap::new(),
        recording: recording(record_id),
    }
}

#[test]
fn a_kernel_1_clear_record_commits_retries_and_verifies_after_a_cold_reopen() {
    block_on(async {
        let directory = tempfile::tempdir().expect("temporary directory");
        let backend = Arc::new(
            eventlog_file::FileEventStore::open(directory.path())
                .await
                .expect("file provider"),
        );
        let tenant = TenantId::new("adversary-clear").expect("tenant");
        let stream_identity = backend
            .stream_identity(&tenant)
            .await
            .expect("stream identity");
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
            logical_scope: "adversary-clear-scope".into(),
            tenant: tenant.as_str().into(),
            stream_identity,
        };
        let backend: Arc<dyn EventlogBackend> = backend;
        EventlogBindingProvisioner::new(backend.clone(), LIMITS)
            .provision_binding(authority.clone(), context("binding"))
            .await
            .expect("binding provisioned");

        let store = EventlogRecordedStore::open(backend.clone(), authority.clone(), LIMITS)
            .await
            .expect("adapter opens");
        let registry = registry();
        let operation = store.operation(context("write"));
        Executor::new(&registry, &operation)
            .create(CreateRequest {
                subject: Subject::new("reminder", "r-1").expect("subject"),
                definition_version: 1,
                fields: json!({ "title": "call back", "snoozed_until": "2026-10-08T09:00:00Z" }),
                recording: recording("create"),
            })
            .await
            .expect("creation commits");
        Executor::new(&registry, &operation)
            .execute(wake(1, "wake-1"))
            .await
            .expect("the clear commits");
        Executor::new(&registry, &operation)
            .execute(wake(2, "wake-2"))
            .await
            .expect("the clear of an absent field commits");
        let retry = store.operation(context("retry"));
        let replayed = Executor::new(&registry, &retry)
            .execute(wake(1, "wake-1"))
            .await
            .expect("an exact retry replays");
        assert!(replayed.replayed());

        let reopened = EventlogRecordedStore::open(backend, authority, LIMITS)
            .await
            .expect("adapter reopens after clear records");
        let subject = Subject::new("reminder", "r-1").expect("subject");
        let terminal = reopened
            .load(&subject)
            .await
            .expect("state read")
            .expect("state exists");
        assert_eq!(
            serde_json::to_string(&terminal.fields).expect("fields"),
            r#"{"title":"call back"}"#
        );
        let history = reopened.history(&subject).await.expect("history read");
        verify_subject_history(&history, &terminal).expect("history verifies from genesis");
        let mut records = Vec::new();
        for stored in &history.records {
            assert_eq!(record_domain(&stored.entry), "er.record/1");
            assert_eq!(
                stored.record_bytes,
                record_comparison_bytes(&stored.entry).expect("record/1 bytes")
            );
            let RecordedEntry::Decision(commit) = &stored.entry else {
                panic!("every entry is a decision");
            };
            records.push(commit.envelope.record.clone());
        }
        assert_eq!(records.len(), 3);
        assert_eq!(
            serde_json::to_string(&records[1].removed).expect("removed"),
            r#"["snoozed_until"]"#
        );
        assert_eq!(
            serde_json::to_string(&records[2].removed).expect("removed"),
            r#"["snoozed_until"]"#
        );
        let replayed = replay(&records).expect("the stored records replay");
        assert_eq!(replayed, terminal);
    });
}
