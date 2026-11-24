//! The CPU of one connectors-shaped read invoke, by call kind.
//!
//! Connectors measured a read invoke at 138-143 CPU ms on Entity Runtime 0.32.0 and 171-177 on
//! 0.33.1 (2026-11-23), on an Eventlog SQLite store read with `ProviderTracked`, nine definitions
//! of about 137 KB together and 50 recorded events. One invoke is three single-action
//! `execute_batch` calls, one `lookup_batch` and one `read_history`. This reproduces that shape
//! with definitions of its own.
//!
//! CPU, not wall time: the process's CPU clock counts every thread, the bridge worker included,
//! and does not grow with other load on the host the way wall time does. Its numbers are still
//! only compared with numbers from the same machine (`ab`).

use std::path::Path;

use entity_core::Registry;
use entity_eventlog::{
    sync::{CallWait, EventlogRecordedStoreOwner, ShutdownMode, ShutdownOutcome},
    CapturePolicy, RecordedProviderFacade,
};
use entity_executor::{BatchAction, CreateRequest, ExecuteRequest};
use entity_store::asynchronous::{AppendOutcome, BatchKey};
use rustix::time::{clock_gettime, ClockId};
use serde_json::{json, Value};

use crate::{
    fixture::{bridge, context, definition, recording, subject, LIMITS},
    stores::{authority, fresh, path_string},
};

/// Definitions in the registry, as connectors registers.
const DEFINITIONS: usize = 9;
/// String fields per definition: what makes each about 15 KB.
const FIELDS: usize = 64;
/// Filler operations per definition, each setting one field and emitting one event.
const FILLER_OPERATIONS: usize = 24;
/// Seeded subjects, each created and touched `SEED_TOUCHES` times: 50 recorded events.
const SEED_SUBJECTS: usize = 10;
const SEED_TOUCHES: u64 = 4;
/// Measured invokes.
const INVOKES: usize = 30;
/// Complete snapshots and fresh provider-tracked opens of the grown store, measured after the
/// invokes and outside their total: work a connectors invoke may add that the three batches do
/// not show.
const SNAPSHOTS: usize = 5;
const OPENS: usize = 5;
/// The entity every invoke records.
const INVOCATION: &str = "invocation_0";

/// One definition of the probe's registry, as JSON text.
fn definition_text(index: usize) -> String {
    let mut fields = serde_json::Map::new();
    for field in 0..FIELDS {
        fields.insert(
            format!("field_{field:03}"),
            json!({ "type": "string", "required": false, "max_length": 4096 }),
        );
    }
    let mut operations = serde_json::Map::new();
    operations.insert(
        "dispatch".to_owned(),
        json!({ "transitions": [{ "from": "admitted", "to": "dispatched" }] }),
    );
    operations.insert(
        "release".to_owned(),
        json!({ "transitions": [{ "from": "dispatched", "to": "released" }] }),
    );
    operations.insert(
        "touch".to_owned(),
        json!({ "transitions": [
            { "from": "admitted", "to": "admitted" },
            { "from": "dispatched", "to": "dispatched" },
            { "from": "released", "to": "released" }
        ] }),
    );
    for operation in 0..FILLER_OPERATIONS {
        operations.insert(
            format!("record_{operation:03}"),
            json!({
                "arguments": { "fields": { "value": { "type": "string", "required": true } } },
                "transitions": [{ "from": "released", "to": "released" }],
                "set": { format!("field_{operation:03}"): "$args.value" },
                "emits": [{
                    "type": format!("Recorded{operation:03}"),
                    "payload": { "value": "$args.value", "before": format!("$old_fields.field_{operation:03}") }
                }]
            }),
        );
    }
    json!({
        "entity": format!("invocation_{index}"),
        "version": 1,
        "schema": { "fields": Value::Object(fields) },
        "lifecycle": { "initial": "admitted", "states": ["admitted", "dispatched", "released"] },
        "operations": Value::Object(operations)
    })
    .to_string()
}

/// The probe's registry and the bytes its definitions take as JSON.
fn registry() -> (Registry, usize) {
    let mut registry = Registry::new();
    let mut bytes = 0;
    for index in 0..DEFINITIONS {
        let text = definition_text(index);
        bytes += text.len();
        registry
            .register(definition(&text))
            .expect("probe definition registers");
    }
    (registry, bytes)
}

fn create(id: &str, record: &str) -> BatchAction {
    BatchAction::Create(CreateRequest {
        subject: subject(INVOCATION, id),
        definition_version: 1,
        fields: json!({ "field_000": id }),
        recording: recording(record),
    })
}

fn execute(id: &str, revision: u64, operation: &str, record: &str) -> BatchAction {
    BatchAction::Execute(ExecuteRequest {
        subject: subject(INVOCATION, id),
        expected_revision: revision,
        operation: operation.to_owned(),
        arguments: json!({}),
        fulfillments: Default::default(),
        recording: recording(record),
    })
}

/// The process's CPU time so far, every thread, in microseconds.
fn cpu_us() -> f64 {
    let now = clock_gettime(ClockId::ProcessCPUTime);
    now.tv_sec as f64 * 1e6 + now.tv_nsec as f64 / 1e3
}

/// CPU per call kind, summed over the invokes.
#[derive(Default)]
struct Spent {
    execute_batch: f64,
    lookup_batch: f64,
    read_history: f64,
}

/// `operation`'s CPU, added to `slot`.
fn spend<T>(slot: &mut f64, operation: impl FnOnce() -> T) -> T {
    let start = cpu_us();
    let output = operation();
    *slot += cpu_us() - start;
    output
}

fn committed(facade: &RecordedProviderFacade, key: &str, action: BatchAction) {
    let outcome = facade.execute_batch(
        context(key),
        BatchKey::Named(key.to_owned()),
        vec![action],
        CallWait::Forever,
    );
    assert!(
        matches!(
            outcome,
            Ok(AppendOutcome::Committed {
                replayed: false,
                ..
            })
        ),
        "{key} must commit fresh: {outcome:?}"
    );
}

/// CPU per read invoke on a provider-tracked Eventlog SQLite store of 50 recorded events, in
/// total and by call kind: `eventlog_sqlite.read_invoke.{cpu_us, execute_batch.cpu_us,
/// lookup_batch.cpu_us, read_history.cpu_us}`, each the mean over the invokes.
pub fn read_invoke(work: &Path) -> Vec<(String, f64)> {
    let (registry, bytes) = registry();
    let directory = fresh(work, "read-invoke");
    let path = path_string(&directory.join("eventlog.sqlite"));
    let mut seeding = entity_sqlite::EventlogSqliteStore::provision(
        path.clone(),
        "er_kpi",
        registry.clone(),
        authority("read-invoke"),
        context("provision"),
        LIMITS,
        bridge(),
    )
    .expect("provisioned");
    for at in 0..SEED_SUBJECTS {
        let id = format!("seed-{at:02}");
        committed(
            seeding.recorded(),
            &format!("{id}-create"),
            create(&id, &format!("{id}-create")),
        );
        for revision in 1..=SEED_TOUCHES {
            let record = format!("{id}-touch-{revision}");
            committed(
                seeding.recorded(),
                &record,
                execute(&id, revision, "touch", &record),
            );
        }
    }
    let bound = seeding.recorded().authority().clone();
    assert_eq!(
        seeding.shutdown(ShutdownMode::Drain, CallWait::Forever),
        ShutdownOutcome::Joined { provider: Ok(()) }
    );

    let open = || {
        RecordedProviderFacade::start_with_read_policy(
            registry.clone(),
            EventlogRecordedStoreOwner::Sqlite {
                path: path.clone(),
                prefix: "er_kpi".to_owned(),
                authority: bound.clone(),
                limits: LIMITS,
            },
            bridge(),
            CapturePolicy::ProviderTracked,
        )
        .expect("provider-tracked open")
    };
    let mut facade = open();
    let mut spent = Spent::default();
    let start = cpu_us();
    for at in 0..INVOKES {
        let id = format!("invoke-{at:03}");
        let admitted = format!("{id}-admit");
        spend(&mut spent.execute_batch, || {
            committed(&facade, &admitted, create(&id, &admitted));
        });
        let dispatched = format!("{id}-dispatch");
        spend(&mut spent.execute_batch, || {
            committed(
                &facade,
                &dispatched,
                execute(&id, 1, "dispatch", &dispatched),
            );
        });
        let released = format!("{id}-release");
        spend(&mut spent.execute_batch, || {
            committed(&facade, &released, execute(&id, 2, "release", &released));
        });
        let batch = spend(&mut spent.lookup_batch, || {
            facade
                .lookup_batch(&BatchKey::Named(admitted.clone()), CallWait::Forever)
                .expect("lookup")
        });
        assert!(batch.is_some(), "the admitted batch is found");
        let history = spend(&mut spent.read_history, || {
            facade
                .read_history(&subject(INVOCATION, &id), CallWait::Forever)
                .expect("history")
        });
        assert_eq!(history.records.len(), 3, "the invocation has three records");
    }
    let total = cpu_us() - start;
    let mut snapshot = 0.0;
    for _ in 0..SNAPSHOTS {
        let complete = spend(&mut snapshot, || {
            facade
                .complete_snapshot(CallWait::Forever)
                .expect("complete snapshot")
        });
        assert!(!complete.histories.is_empty(), "the store holds subjects");
    }
    assert_eq!(
        facade.shutdown(ShutdownMode::Drain, CallWait::Forever),
        ShutdownOutcome::Joined { provider: Ok(()) }
    );
    let mut opened = 0.0;
    for _ in 0..OPENS {
        let mut fresh = spend(&mut opened, open);
        assert_eq!(
            fresh.shutdown(ShutdownMode::Drain, CallWait::Forever),
            ShutdownOutcome::Joined { provider: Ok(()) }
        );
    }
    eprintln!(
        "er-kpi: read invoke: {DEFINITIONS} definitions of {bytes} bytes, {} seeded events, \
         {INVOKES} invokes",
        SEED_SUBJECTS as u64 * (1 + SEED_TOUCHES)
    );
    let per = |value: f64| value / INVOKES as f64;
    vec![
        ("eventlog_sqlite.read_invoke.cpu_us".to_owned(), per(total)),
        (
            "eventlog_sqlite.read_invoke.execute_batch.cpu_us".to_owned(),
            per(spent.execute_batch),
        ),
        (
            "eventlog_sqlite.read_invoke.lookup_batch.cpu_us".to_owned(),
            per(spent.lookup_batch),
        ),
        (
            "eventlog_sqlite.read_invoke.read_history.cpu_us".to_owned(),
            per(spent.read_history),
        ),
        (
            "eventlog_sqlite.read_invoke.after.complete_snapshot.cpu_us".to_owned(),
            snapshot / SNAPSHOTS as f64,
        ),
        (
            "eventlog_sqlite.read_invoke.after.open.cpu_us".to_owned(),
            opened / OPENS as f64,
        ),
    ]
}
