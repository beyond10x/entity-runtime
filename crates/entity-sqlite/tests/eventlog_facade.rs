//! The public SQLite compatibility facade over the complete recorded Eventlog authority.
#![cfg(feature = "eventlog-facade")]

use std::{num::NonZeroU16, path::Path};

use entity_core::{Registry, Runtime};
use entity_eventlog::{
    sync::{BridgeConfig, CallWait, ProvisionAuthority, ShutdownMode, ShutdownOutcome},
    EventlogOperationContext,
};
use entity_query::{DocumentQuery, DocumentQueryProvider};
use entity_sqlite::EventlogSqliteStore;
use entity_store::{asynchronous::AppendOutcome, Expect, RecordedCommit, Recording, StateProvider};
use eventlog_core::CaptureLimits;
use serde_json::json;

const LIMITS: CaptureLimits = CaptureLimits {
    max_events: 512,
    max_blobs: 2_048,
    max_projection_rows: 2_048,
    max_payload_bytes: 8 * 1024 * 1024,
};

fn scratch() -> std::path::PathBuf {
    let path = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("sqlite-eventlog-facade")
        .join(std::process::id().to_string());
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).expect("scratch directory");
    path.join("authority.sqlite3")
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

fn context(label: &str) -> EventlogOperationContext {
    EventlogOperationContext {
        subject: "sqlite-facade-test".to_owned(),
        actor: "entity-sqlite-test".to_owned(),
        request_id: format!("request-{label}"),
        trace_id: format!("trace-{label}"),
        causation_id: None,
        causation_depth: 0,
        occurred_at: time::OffsetDateTime::UNIX_EPOCH,
    }
}

fn bridge() -> BridgeConfig {
    BridgeConfig {
        queue_capacity: NonZeroU16::new(8).expect("nonzero"),
    }
}

#[test]
fn sqlite_eventlog_facade_entrypoints_preserve_receipts_queries_and_exact_reopen() {
    let path = scratch();
    let definitions = registry();
    let decision = Runtime::new(&definitions)
        .create("ticket", 1, "one", json!({"title":"SQLite facade"}))
        .expect("creation");
    let commit = RecordedCommit::new(
        decision,
        &Recording {
            record_id: "sqlite-facade-create".to_owned(),
            recorded_at: "2026-09-16T00:00:00Z".to_owned(),
            correlation: Some("sqlite-facade".to_owned()),
            causation: None,
            actor: None,
        },
    )
    .expect("recorded creation");
    let mut store = EventlogSqliteStore::provision(
        path.to_string_lossy(),
        "recorded",
        definitions,
        ProvisionAuthority {
            logical_scope: "scope-sqlite-facade".to_owned(),
            tenant: "tenant-sqlite-facade".to_owned(),
            expected_stream_identity: None,
        },
        context("provision"),
        LIMITS,
        bridge(),
    )
    .expect("SQLite facade provisioned");
    let authority = store.recorded().authority().clone();
    let first = store
        .recorded()
        .commit_recorded(
            context("create"),
            commit.clone(),
            Expect::Absent,
            CallWait::Forever,
        )
        .expect("recorded creation");
    let retry = store
        .recorded()
        .commit_recorded(context("retry"), commit, Expect::Absent, CallWait::Forever)
        .expect("exact retry");
    let (
        AppendOutcome::Committed {
            receipt: first_receipt,
            replayed: false,
        },
        AppendOutcome::Committed {
            receipt: retry_receipt,
            replayed: true,
        },
    ) = (first, retry)
    else {
        panic!("creation and retry must retain the committed receipt")
    };
    assert_eq!(retry_receipt, first_receipt);
    assert_eq!(
        store
            .query_documents(&DocumentQuery::for_entity("ticket"))
            .expect("query")
            .items
            .len(),
        1
    );
    assert_eq!(
        store.shutdown(ShutdownMode::Drain, CallWait::Forever),
        ShutdownOutcome::Joined { provider: Ok(()) }
    );

    let mut altered = authority.clone();
    altered.stream_identity.push_str("-altered");
    assert!(
        EventlogSqliteStore::open(
            path.to_string_lossy(),
            "recorded",
            registry(),
            altered,
            LIMITS,
            bridge(),
        )
        .is_err(),
        "an altered physical authority must be refused"
    );
    let mut reopened = EventlogSqliteStore::open(
        path.to_string_lossy(),
        "recorded",
        registry(),
        authority,
        LIMITS,
        bridge(),
    )
    .expect("exact SQLite authority reopens");
    assert_eq!(
        reopened
            .load("ticket", "one")
            .expect("reopened state")
            .expect("retained state")
            .revision,
        1
    );
    assert_eq!(
        reopened.shutdown(ShutdownMode::Drain, CallWait::Forever),
        ShutdownOutcome::Joined { provider: Ok(()) }
    );
}
