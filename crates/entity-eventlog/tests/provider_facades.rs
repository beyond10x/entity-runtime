//! Complete synchronous facades over native File and SQLite providers.
#![cfg(all(feature = "sync-bridge", feature = "file", feature = "sqlite"))]

use std::num::NonZeroU16;

use entity_core::Registry;
use entity_eventlog::{
    Authority, EventlogFileStore, EventlogOperationContext, LegacyImportError,
    RecordedProviderFacade,
    sync::{
        BridgeConfig, CallWait, EventlogRecordedStoreProvisioner, ProvisionAuthority, ShutdownMode,
        ShutdownOutcome,
    },
};
use entity_executor::{BatchAction, CreateRequest, ExecuteRequest};
use entity_query::{DocumentQuery, DocumentQueryProvider};
use entity_store::{
    EventProvider, HistoryProvider, LegacyStoreSnapshot, LegacyStoreSource, Recording,
    StateProvider, Store,
    asynchronous::{AppendOutcome, BatchKey, Subject},
};
use eventlog_core::{CaptureLimits, EventStore, TenantId};
use serde_json::json;
use time::OffsetDateTime;

const LIMITS: CaptureLimits = CaptureLimits {
    max_events: 512,
    max_blobs: 2_048,
    max_projection_rows: 2_048,
    max_payload_bytes: 8 * 1024 * 1024,
};

fn bridge() -> BridgeConfig {
    BridgeConfig {
        queue_capacity: NonZeroU16::new(8).expect("nonzero"),
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
                "emits": []
            }
        }
    }))
    .expect("definition parses");
    let mut registry = Registry::new();
    registry.register(definition).expect("definition validates");
    registry
}

fn context(label: &str) -> EventlogOperationContext {
    EventlogOperationContext {
        subject: "facade-test".to_owned(),
        actor: "entity-eventlog-test".to_owned(),
        request_id: format!("request-{label}"),
        trace_id: format!("trace-{label}"),
        causation_id: None,
        causation_depth: 0,
        occurred_at: OffsetDateTime::UNIX_EPOCH,
    }
}

fn recording(record_id: &str) -> Recording {
    Recording {
        record_id: record_id.to_owned(),
        recorded_at: "2026-09-16T00:00:00Z".to_owned(),
        correlation: Some("provider-facade".to_owned()),
        causation: None,
        actor: None,
    }
}

fn create(id: &str, record_id: &str) -> CreateRequest {
    CreateRequest {
        subject: Subject::new("ticket", id).expect("subject"),
        definition_version: 1,
        fields: json!({"title":id}),
        recording: recording(record_id),
    }
}

fn touch(id: &str, revision: u64, record_id: &str) -> ExecuteRequest {
    ExecuteRequest {
        subject: Subject::new("ticket", id).expect("subject"),
        expected_revision: revision,
        operation: "touch".to_owned(),
        arguments: json!({}),
        fulfillments: Default::default(),
        recording: recording(record_id),
    }
}

fn block_on<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime")
        .block_on(future)
}

fn file_authority(path: &std::path::Path, label: &str) -> Authority {
    let tenant = TenantId::new(format!("facade-{label}")).expect("tenant");
    let backend = block_on(eventlog_file::FileEventStore::open(path)).expect("File Eventlog");
    let stream_identity = block_on(backend.stream_identity(&tenant)).expect("stream identity");
    Authority {
        logical_scope: format!("scope-{label}"),
        tenant: tenant.as_str().to_owned(),
        stream_identity,
    }
}

#[test]
fn file_facade_preserves_atomic_groups_queries_retries_and_restart() {
    let directory = tempfile::tempdir().expect("temporary File authority");
    let authority = file_authority(directory.path(), "file");
    let mut facade = EventlogFileStore::provision(
        directory.path(),
        registry(),
        ProvisionAuthority {
            logical_scope: authority.logical_scope.clone(),
            tenant: authority.tenant.clone(),
            expected_stream_identity: Some(authority.stream_identity.clone()),
        },
        context("provision-file"),
        LIMITS,
        bridge(),
    )
    .expect("File facade provisioned");
    let key = BatchKey::Named("group-create-and-touch".to_owned());
    let actions = vec![
        BatchAction::Create(create("one", "one-create")),
        BatchAction::Execute(touch("one", 1, "one-touch")),
        BatchAction::Create(create("two", "two-create")),
    ];
    let first = facade
        .recorded()
        .execute_batch(
            context("group-first"),
            key.clone(),
            actions.clone(),
            CallWait::Forever,
        )
        .expect("ordered repeated-subject group");
    let AppendOutcome::Committed {
        receipt,
        replayed: false,
    } = first
    else {
        panic!("fresh group must return its committed receipt")
    };
    assert_eq!(receipt.members().len(), 3);
    let replay = facade
        .recorded()
        .execute_batch(context("group-retry"), key, actions, CallWait::Forever)
        .expect("exact retry");
    assert!(matches!(
        replay,
        AppendOutcome::Committed { replayed: true, .. }
    ));
    assert_eq!(
        facade
            .load("ticket", "one")
            .expect("state")
            .unwrap()
            .revision,
        2
    );
    assert!(facade.events("ticket", "one").expect("events").is_empty());
    let observation = entity_store::RecordedObservation {
        entity: "ticket".to_owned(),
        id: "one".to_owned(),
        revision: 2,
        envelope: recording("one-observation")
            .seal(json!({"seen":true}))
            .expect("observation envelope"),
    };
    facade
        .recorded()
        .observe(
            context("one-observation"),
            observation.clone(),
            CallWait::Forever,
        )
        .expect("observation recorded");
    assert_eq!(
        facade.observations("ticket", "one").expect("observations"),
        vec![observation]
    );
    let page = facade
        .query_documents(&DocumentQuery {
            entity: "ticket".to_owned(),
            matching: Default::default(),
            limit: Some(1),
            after: None,
        })
        .expect("query");
    assert_eq!(page.items.len(), 1);
    assert!(page.next.is_some());

    let conflict = facade.recorded().execute_batch(
        context("rollback"),
        BatchKey::Named("rollback".to_owned()),
        vec![
            BatchAction::Create(create("three", "three-create")),
            BatchAction::Create(create("one", "duplicate-one")),
        ],
        CallWait::Forever,
    );
    assert!(conflict.is_err());
    assert!(facade.load("ticket", "three").expect("state").is_none());
    assert_eq!(
        facade.shutdown(ShutdownMode::Drain, CallWait::Forever),
        ShutdownOutcome::Joined { provider: Ok(()) }
    );

    let mut reopened =
        EventlogFileStore::open(directory.path(), registry(), authority, LIMITS, bridge())
            .expect("File facade reopens");
    assert_eq!(reopened.ids("ticket").expect("ids"), vec!["one", "two"]);
    assert_eq!(
        reopened.shutdown(ShutdownMode::Drain, CallWait::Forever),
        ShutdownOutcome::Joined { provider: Ok(()) }
    );
}

#[test]
fn legacy_file_import_is_exact_resumable_and_provider_verified() {
    let registry = registry();
    let source_dir = tempfile::tempdir().expect("legacy source");
    let mut source = entity_store::FileStore::open(source_dir.path());
    let created = entity_core::Runtime::new(&registry)
        .create("ticket", 1, "legacy", json!({"title":"legacy"}))
        .expect("creation");
    let recorded = entity_store::RecordedCommit::new(created, &recording("legacy-create"))
        .expect("recorded legacy commit");
    source
        .commit_recorded(&recorded, entity_store::Expect::Absent)
        .expect("legacy write");
    let snapshot = source
        .acquire_legacy("file/legacy-source")
        .expect("source acquisition");

    let destination = tempfile::tempdir().expect("destination");
    let authority = file_authority(destination.path(), "import");
    let mut facade = EventlogFileStore::provision(
        destination.path(),
        registry,
        ProvisionAuthority {
            logical_scope: authority.logical_scope,
            tenant: authority.tenant,
            expected_stream_identity: Some(authority.stream_identity),
        },
        context("provision-import"),
        LIMITS,
        bridge(),
    )
    .expect("destination provisioned");
    let first = facade
        .recorded()
        .import_legacy(snapshot.clone(), context("import-first"), CallWait::Forever)
        .expect("legacy import");
    assert_eq!(first.source_id, "file/legacy-source");
    assert_eq!(first.subjects.len(), 1);
    assert_eq!(first.replayed, 0);
    let retry = facade
        .recorded()
        .import_legacy(snapshot, context("import-retry"), CallWait::Forever)
        .expect("exact import retry");
    assert_eq!(retry.replayed, 1);
    assert_eq!(
        facade.load("ticket", "legacy").expect("state"),
        Some(recorded.instance)
    );
    let suffix = ExecuteRequest {
        subject: Subject::new("ticket", "legacy").expect("subject"),
        expected_revision: 1,
        operation: "touch".to_owned(),
        arguments: json!({}),
        fulfillments: Default::default(),
        recording: recording("legacy-suffix"),
    };
    let appended = facade
        .recorded()
        .execute(context("legacy-suffix"), suffix.clone(), CallWait::Forever)
        .expect("post-import suffix");
    assert!(!appended.replayed());
    let retried = facade
        .recorded()
        .execute(context("legacy-suffix-retry"), suffix, CallWait::Forever)
        .expect("post-import exact retry");
    assert!(retried.replayed());
    let history = facade
        .recorded()
        .read_history(
            &Subject::new("ticket", "legacy").expect("subject"),
            CallWait::Forever,
        )
        .expect("mixed imported history");
    assert!(matches!(
        history.origin,
        entity_store::asynchronous::HistoryOrigin::Imported(_)
    ));
    assert_eq!(history.records.len(), 1);
    assert_eq!(
        facade.shutdown(ShutdownMode::Drain, CallWait::Forever),
        ShutdownOutcome::Joined { provider: Ok(()) }
    );
}

#[test]
fn legacy_import_retains_settled_progress_and_refuses_a_changed_source_boundary() {
    let registry = registry();
    let source_id = "file/partial-source";
    let mut acquired = Vec::new();
    for (id, title, record_id) in [
        ("first", "first", "partial-first"),
        ("second", "second", "partial-second"),
    ] {
        let source_dir = tempfile::tempdir().expect("legacy source");
        let mut source = entity_store::FileStore::open(source_dir.path());
        let created = entity_core::Runtime::new(&registry)
            .create("ticket", 1, id, json!({"title":title}))
            .expect("creation");
        let recorded = entity_store::RecordedCommit::new(created, &recording(record_id))
            .expect("recorded legacy commit");
        source
            .commit_recorded(&recorded, entity_store::Expect::Absent)
            .expect("legacy write");
        acquired.extend(
            source
                .acquire_legacy(source_id)
                .expect("source acquisition")
                .histories,
        );
    }
    let snapshot = LegacyStoreSnapshot::new(source_id, acquired).expect("combined acquisition");

    let changed_dir = tempfile::tempdir().expect("changed legacy source");
    let mut changed = entity_store::FileStore::open(changed_dir.path());
    let changed_decision = entity_core::Runtime::new(&registry)
        .create(
            "ticket",
            1,
            "second",
            json!({"title":"changed after acquisition"}),
        )
        .expect("changed creation");
    let changed_record =
        entity_store::RecordedCommit::new(changed_decision, &recording("partial-second-changed"))
            .expect("changed legacy commit");
    changed
        .commit_recorded(&changed_record, entity_store::Expect::Absent)
        .expect("changed legacy write");
    let changed_snapshot = changed
        .acquire_legacy(source_id)
        .expect("changed source acquisition");

    let destination = tempfile::tempdir().expect("destination");
    let authority = file_authority(destination.path(), "partial-import");
    let mut facade = EventlogFileStore::provision(
        destination.path(),
        registry,
        ProvisionAuthority {
            logical_scope: authority.logical_scope,
            tenant: authority.tenant,
            expected_stream_identity: Some(authority.stream_identity),
        },
        context("provision-partial-import"),
        LIMITS,
        bridge(),
    )
    .expect("destination provisioned");
    facade
        .recorded()
        .import_legacy(
            changed_snapshot,
            context("changed-boundary"),
            CallWait::Forever,
        )
        .expect("changed boundary is independently valid before conflict");

    for label in ["partial-first-attempt", "partial-retry"] {
        let error = facade
            .recorded()
            .import_legacy(snapshot.clone(), context(label), CallWait::Forever)
            .expect_err("unequal second-subject boundary must be refused");
        let LegacyImportError::Subject {
            subject, settled, ..
        } = error
        else {
            panic!("the exact conflicting subject must be reported")
        };
        assert_eq!(subject.entity, "ticket");
        assert_eq!(subject.id, "second");
        assert_eq!(
            settled.len(),
            1,
            "the earlier exact boundary remains settled and resumable"
        );
    }
    assert_eq!(
        facade
            .load("ticket", "first")
            .expect("settled first subject")
            .expect("first imported state")
            .fields["title"],
        json!("first")
    );
    assert_eq!(
        facade
            .load("ticket", "second")
            .expect("conflicting second subject")
            .expect("existing changed boundary")
            .fields["title"],
        json!("changed after acquisition")
    );
    assert_eq!(
        facade.shutdown(ShutdownMode::Drain, CallWait::Forever),
        ShutdownOutcome::Joined { provider: Ok(()) }
    );
}

#[test]
fn legacy_import_revalidates_the_complete_source_before_any_destination_write() {
    let registry = registry();
    let source_dir = tempfile::tempdir().expect("legacy source");
    let mut source = entity_store::FileStore::open(source_dir.path());
    for (id, record_id) in [("first", "validate-first"), ("second", "validate-second")] {
        let created = entity_core::Runtime::new(&registry)
            .create("ticket", 1, id, json!({"title":id}))
            .expect("creation");
        let recorded = entity_store::RecordedCommit::new(created, &recording(record_id))
            .expect("recorded legacy commit");
        source
            .commit_recorded(&recorded, entity_store::Expect::Absent)
            .expect("legacy write");
    }
    let mut snapshot = source
        .acquire_legacy("file/revalidate-source")
        .expect("valid acquisition");
    let entity_store::asynchronous::HistoryOrigin::Imported(anchor) =
        &mut snapshot.histories[1].origin
    else {
        panic!("legacy acquisition has an imported boundary")
    };
    anchor.instance.fields["title"] = json!("tampered after acquisition");

    let destination = tempfile::tempdir().expect("destination");
    let authority = file_authority(destination.path(), "source-validation");
    let mut facade = EventlogFileStore::provision(
        destination.path(),
        registry,
        ProvisionAuthority {
            logical_scope: authority.logical_scope,
            tenant: authority.tenant,
            expected_stream_identity: Some(authority.stream_identity),
        },
        context("provision-source-validation"),
        LIMITS,
        bridge(),
    )
    .expect("destination provisioned");
    assert!(matches!(
        facade.recorded().import_legacy(
            snapshot,
            context("invalid-complete-source"),
            CallWait::Forever,
        ),
        Err(LegacyImportError::Source(_))
    ));
    assert!(
        facade.ids("ticket").expect("destination ids").is_empty(),
        "whole-source validation precedes the first imported subject"
    );
    assert_eq!(
        facade.shutdown(ShutdownMode::Drain, CallWait::Forever),
        ShutdownOutcome::Joined { provider: Ok(()) }
    );
}

#[test]
fn sqlite_memory_facade_uses_the_same_complete_surface() {
    let prefix = "facade_sqlite_memory";
    let authority = ProvisionAuthority {
        logical_scope: "scope-sqlite-memory".to_owned(),
        tenant: "facade-sqlite-memory".to_owned(),
        expected_stream_identity: None,
    };
    let mut facade = RecordedProviderFacade::provision(
        registry(),
        EventlogRecordedStoreProvisioner::SqliteMemory {
            prefix: prefix.to_owned(),
            authority,
            limits: LIMITS,
        },
        context("provision-sqlite"),
        bridge(),
    )
    .expect("SQLite facade provisioned");
    facade
        .create(
            context("sqlite-create"),
            create("one", "sqlite-one"),
            CallWait::Forever,
        )
        .expect("SQLite facade creation");
    assert_eq!(facade.ids("ticket").expect("ids"), vec!["one"]);
    assert_eq!(
        facade.shutdown(ShutdownMode::Drain, CallWait::Forever),
        ShutdownOutcome::Joined { provider: Ok(()) }
    );
}

#[test]
fn sqlite_file_provision_returns_the_exact_reopen_authority() {
    let directory = tempfile::tempdir().expect("SQLite authority directory");
    let path = directory.path().join("eventlog.sqlite3");
    let selection = ProvisionAuthority {
        logical_scope: "scope-sqlite-file".to_owned(),
        tenant: "facade-sqlite-file".to_owned(),
        expected_stream_identity: None,
    };
    let mut facade = RecordedProviderFacade::provision(
        registry(),
        EventlogRecordedStoreProvisioner::Sqlite {
            path: path.to_string_lossy().into_owned(),
            prefix: "facade_sqlite_file".to_owned(),
            authority: selection,
            limits: LIMITS,
        },
        context("provision-sqlite-file"),
        bridge(),
    )
    .expect("SQLite file facade provisioned");
    let authority = facade.authority().clone();
    assert_eq!(authority.logical_scope, "scope-sqlite-file");
    assert_eq!(authority.tenant, "facade-sqlite-file");
    assert!(!authority.stream_identity.is_empty());
    facade
        .create(
            context("sqlite-file-create"),
            create("kept", "sqlite-file-kept"),
            CallWait::Forever,
        )
        .expect("SQLite file creation");
    assert_eq!(
        facade.shutdown(ShutdownMode::Drain, CallWait::Forever),
        ShutdownOutcome::Joined { provider: Ok(()) }
    );

    let mut reopened = RecordedProviderFacade::start(
        registry(),
        entity_eventlog::sync::EventlogRecordedStoreOwner::Sqlite {
            path: path.to_string_lossy().into_owned(),
            prefix: "facade_sqlite_file".to_owned(),
            authority,
            limits: LIMITS,
        },
        bridge(),
    )
    .expect("SQLite file facade reopens with returned authority");
    assert_eq!(reopened.ids("ticket").expect("ids"), vec!["kept"]);
    assert_eq!(
        reopened.shutdown(ShutdownMode::Drain, CallWait::Forever),
        ShutdownOutcome::Joined { provider: Ok(()) }
    );
}

#[test]
fn file_facade_groups_are_process_atomic_and_survive_a_publication_crash() {
    use std::{
        process::{Command, Stdio},
        thread,
        time::{Duration, Instant},
    };

    const ROOT: &str = "ENTITY_EVENTLOG_FACADE_PROCESS_ROOT";
    const AUTHORITY: &str = "ENTITY_EVENTLOG_FACADE_PROCESS_AUTHORITY";
    const MODE: &str = "ENTITY_EVENTLOG_FACADE_PROCESS_MODE";
    const TEST: &str = "file_facade_groups_are_process_atomic_and_survive_a_publication_crash";

    if let (Ok(root), Ok(authority), Ok(mode)) = (
        std::env::var(ROOT),
        std::env::var(AUTHORITY),
        std::env::var(MODE),
    ) {
        let root = std::path::PathBuf::from(root);
        let authority: Authority = serde_json::from_str(&authority).expect("child authority");
        let mut facade = EventlogFileStore::open(&root, registry(), authority, LIMITS, bridge())
            .expect("child facade opens");
        if let Some(label) = mode.strip_prefix("compete-") {
            std::fs::write(root.join(format!("ready-{label}")), "ready").expect("child ready");
            while !root.join("go").exists() {
                thread::sleep(Duration::from_millis(2));
            }
            let result = facade.recorded().execute_batch(
                context(label),
                BatchKey::Named(format!("competing-{label}")),
                vec![
                    BatchAction::Execute(touch("one", 1, &format!("{label}-one"))),
                    BatchAction::Execute(touch("two", 1, &format!("{label}-two"))),
                ],
                CallWait::Forever,
            );
            std::fs::write(
                root.join(format!("result-{label}")),
                if result.is_ok() {
                    "committed"
                } else {
                    "refused"
                },
            )
            .expect("child result");
            assert_eq!(
                facade.shutdown(ShutdownMode::Drain, CallWait::Forever),
                ShutdownOutcome::Joined { provider: Ok(()) }
            );
            return;
        }
        assert_eq!(mode, "crash-group");
        let actions = (0..192)
            .map(|index| {
                BatchAction::Create(create(
                    &format!("crash-{index:03}"),
                    &format!("crash-record-{index:03}"),
                ))
            })
            .collect();
        std::fs::write(root.join("crash-ready"), "ready").expect("crash child ready");
        let _ = facade.recorded().execute_batch(
            context("crash-group"),
            BatchKey::Named("crash-group".to_owned()),
            actions,
            CallWait::Forever,
        );
        let _ = facade.shutdown(ShutdownMode::Drain, CallWait::Forever);
        return;
    }

    let directory = tempfile::tempdir().expect("process File authority");
    let authority = file_authority(directory.path(), "process");
    let mut prepared = EventlogFileStore::provision(
        directory.path(),
        registry(),
        ProvisionAuthority {
            logical_scope: authority.logical_scope.clone(),
            tenant: authority.tenant.clone(),
            expected_stream_identity: Some(authority.stream_identity.clone()),
        },
        context("process-provision"),
        LIMITS,
        bridge(),
    )
    .expect("process authority provisioned");
    prepared
        .recorded()
        .execute_batch(
            context("process-seed"),
            BatchKey::Named("process-seed".to_owned()),
            vec![
                BatchAction::Create(create("one", "process-create-one")),
                BatchAction::Create(create("two", "process-create-two")),
            ],
            CallWait::Forever,
        )
        .expect("seed subjects");
    assert_eq!(
        prepared.shutdown(ShutdownMode::Drain, CallWait::Forever),
        ShutdownOutcome::Joined { provider: Ok(()) }
    );

    let authority_json = serde_json::to_string(&authority).expect("authority JSON");
    let child = |mode: &str| {
        Command::new(std::env::current_exe().expect("test executable"))
            .args(["--exact", TEST, "--nocapture"])
            .env(ROOT, directory.path())
            .env(AUTHORITY, &authority_json)
            .env(MODE, mode)
            .stdout(Stdio::null())
            .spawn()
            .expect("child starts")
    };
    let mut first = child("compete-first");
    let mut second = child("compete-second");
    let deadline = Instant::now() + Duration::from_secs(30);
    while !(directory.path().join("ready-first").exists()
        && directory.path().join("ready-second").exists())
    {
        assert!(
            Instant::now() < deadline,
            "competing children did not become ready"
        );
        thread::sleep(Duration::from_millis(5));
    }
    std::fs::write(directory.path().join("go"), "go").expect("release children");
    assert!(first.wait().expect("first child").success());
    assert!(second.wait().expect("second child").success());
    let results = [
        std::fs::read_to_string(directory.path().join("result-first")).expect("first result"),
        std::fs::read_to_string(directory.path().join("result-second")).expect("second result"),
    ];
    assert_eq!(
        results
            .iter()
            .filter(|result| result.as_str() == "committed")
            .count(),
        1,
        "exactly one complete cross-subject group commits: {results:?}"
    );

    assert!(
        !directory.path().join("append.json").exists(),
        "settled competing groups leave no publication intent"
    );
    let mut crashing = child("crash-group");
    let deadline = Instant::now() + Duration::from_secs(30);
    while !directory.path().join("crash-ready").exists() {
        assert!(
            Instant::now() < deadline,
            "crash child did not become ready"
        );
        thread::sleep(Duration::from_millis(2));
    }
    while !directory.path().join("append.json").exists() {
        if let Some(status) = crashing.try_wait().expect("crash child status") {
            panic!("crash child completed before the publication intent was observed: {status}");
        }
        assert!(
            Instant::now() < deadline,
            "publication intent was not observed"
        );
        thread::yield_now();
    }
    crashing.kill().expect("kill at publication intent");
    let _ = crashing.wait().expect("crash child reaped");

    let mut reopened =
        EventlogFileStore::open(directory.path(), registry(), authority, LIMITS, bridge())
            .expect("File authority recovers after killed publisher");
    let ids = reopened.ids("ticket").expect("complete recovered ids");
    let crash_ids = ids.iter().filter(|id| id.starts_with("crash-")).count();
    assert!(
        crash_ids == 0 || crash_ids == 192,
        "a killed named group is wholly absent or wholly committed, found {crash_ids}/192"
    );
    let batch = reopened
        .recorded()
        .lookup_batch(
            &BatchKey::Named("crash-group".to_owned()),
            CallWait::Forever,
        )
        .expect("batch lookup");
    assert_eq!(batch.is_some(), crash_ids == 192);
    if let Some(batch) = batch {
        assert_eq!(batch.records.len(), 192);
    }
    assert_eq!(
        reopened.shutdown(ShutdownMode::Drain, CallWait::Forever),
        ShutdownOutcome::Joined { provider: Ok(()) }
    );
}
