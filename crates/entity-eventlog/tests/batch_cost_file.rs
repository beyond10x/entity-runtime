//! Adversary case: the member-doubling bound of `batch_cost.rs`, held on the File provider.
//!
//! The File provider is the one `entity-cli` builds with (`eventlog-providers` enables `file`).
//! Its `ProjectionStore::get_blob` reads the blob file and hashes it in full on every call, and
//! the projector still calls `get_blob` for the batch blob once per member.
#![cfg(all(feature = "file", feature = "sync-bridge", target_os = "linux"))]

use std::{num::NonZeroU16, path::Path, time::Duration};

use entity_core::Registry;
use entity_eventlog::{
    EventlogOperationContext, RecordedProviderFacade,
    sync::{
        BridgeConfig, CallWait, EventlogRecordedStoreProvisioner, ProvisionAuthority, ShutdownMode,
        ShutdownOutcome,
    },
};
use entity_executor::{BatchAction, CreateRequest, ExecuteRequest};
use entity_store::{
    Recording,
    asynchronous::{AppendOutcome, BatchKey, Subject},
};
use eventlog_core::CaptureLimits;
use serde_json::json;
use time::OffsetDateTime;

const LIMITS: CaptureLimits = CaptureLimits {
    max_events: 20_000,
    max_blobs: 80_000,
    max_projection_rows: 80_000,
    max_payload_bytes: 512 * 1024 * 1024,
};
const RUNS: usize = 3;

fn registry() -> Registry {
    let definition = serde_json::from_value(json!({
        "entity": "ticket",
        "version": 1,
        "schema": { "fields": { "title": { "type": "string", "required": true } } },
        "lifecycle": { "initial": "open", "states": ["open"] },
        "operations": {
            "touch": { "transitions": [{ "from": "open", "to": "open" }], "emits": [] }
        }
    }))
    .expect("definition parses");
    let mut registry = Registry::new();
    registry.register(definition).expect("definition validates");
    registry
}

fn context(label: &str) -> EventlogOperationContext {
    EventlogOperationContext {
        subject: "batch-cost".to_owned(),
        actor: "entity-eventlog-test".to_owned(),
        request_id: format!("request-{label}"),
        trace_id: format!("trace-{label}"),
        causation_id: None,
        causation_depth: 0,
        occurred_at: OffsetDateTime::UNIX_EPOCH,
    }
}

fn recording(id: &str) -> Recording {
    Recording {
        record_id: id.to_owned(),
        recorded_at: "2026-10-08T00:00:00Z".to_owned(),
        correlation: None,
        causation: None,
        actor: None,
    }
}

fn ticket(id: &str) -> Subject {
    Subject::new("ticket", id).expect("subject")
}

fn create(id: &str) -> BatchAction {
    BatchAction::Create(CreateRequest {
        subject: ticket(id),
        definition_version: 1,
        fields: json!({ "title": id }),
        recording: recording(&format!("create-{id}")),
    })
}

fn touch(id: &str) -> BatchAction {
    BatchAction::Execute(ExecuteRequest {
        subject: ticket(id),
        expected_revision: 1,
        operation: "touch".to_owned(),
        arguments: json!({}),
        fulfillments: Default::default(),
        recording: recording(&format!("touch-{id}")),
    })
}

fn members(batch: usize) -> Vec<BatchAction> {
    (0..batch)
        .map(|index| {
            if index % 2 == 0 {
                create(&format!("new-{index}"))
            } else {
                touch(&format!("old-{}", index / 2))
            }
        })
        .collect()
}

fn cpu_time() -> Duration {
    let stat = std::fs::read_to_string("/proc/self/stat").expect("/proc/self/stat");
    let after = &stat[stat.rfind(')').expect("command name") + 2..];
    let fields: Vec<&str> = after.split(' ').collect();
    let ticks: u64 =
        fields[11].parse::<u64>().expect("utime") + fields[12].parse::<u64>().expect("stime");
    Duration::from_millis(ticks * 10)
}

fn batch_cost(directory: &Path, batch: usize, store: usize) -> Duration {
    let own = tempfile::tempdir_in(directory).expect("store directory");
    let mut facade = RecordedProviderFacade::provision(
        registry(),
        EventlogRecordedStoreProvisioner::File {
            path: own.path().join("file-store"),
            authority: ProvisionAuthority {
                logical_scope: "batch-cost-scope".to_owned(),
                tenant: "batch-cost-tenant".to_owned(),
                expected_stream_identity: None,
            },
            limits: LIMITS,
        },
        context("provision"),
        BridgeConfig {
            queue_capacity: NonZeroU16::new(8).expect("nonzero"),
        },
    )
    .expect("file authority provisioned");
    for chunk in 0..store.div_ceil(16) {
        let actions = (chunk * 16..store.min(chunk * 16 + 16))
            .map(|index| create(&format!("old-{index}")))
            .collect();
        facade
            .execute_batch(
                context(&format!("seed-{chunk}")),
                BatchKey::Named(format!("seed-{chunk}")),
                actions,
                CallWait::Forever,
            )
            .expect("seed batch");
    }
    let actions = members(batch);
    let before = cpu_time();
    let outcome = facade.execute_batch(
        context("measured"),
        BatchKey::Named("measured".to_owned()),
        actions,
        CallWait::Forever,
    );
    let cost = cpu_time() - before;
    let Ok(AppendOutcome::Committed {
        receipt,
        replayed: false,
    }) = outcome
    else {
        panic!("the measured batch must commit fresh: {outcome:?}")
    };
    assert_eq!(receipt.members().len(), batch);
    assert_eq!(
        facade.shutdown(ShutdownMode::Drain, CallWait::Forever),
        ShutdownOutcome::Joined { provider: Ok(()) }
    );
    cost
}

fn least_cost(directory: &Path, batch: usize, store: usize) -> Duration {
    let cost = (0..RUNS)
        .map(|_| batch_cost(directory, batch, store))
        .min()
        .expect("at least one run");
    eprintln!("file: {batch:>4} members on {store:>4} subjects: {cost:?} CPU");
    cost
}

/// The story's acceptance bound (at most about 2.5x per doubling of members at a fixed store
/// size), on the File provider rather than SQLite. Sizes reach the ~400 members the story names.
#[test]
#[ignore = "eventlog-file verifies the whole blob on each get_blob; linear on File needs an Eventlog read path"]
fn a_file_provider_batch_costs_time_linear_in_its_members() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("directory");
    let store = 256;
    let by_members: Vec<(usize, Duration)> = [128, 256, 512]
        .into_iter()
        .map(|batch| (batch, least_cost(directory.path(), batch, store)))
        .collect();
    for pair in by_members.windows(2) {
        let ratio =
            pair[1].1.as_secs_f64() / pair[0].1.max(Duration::from_millis(10)).as_secs_f64();
        assert!(
            ratio <= 2.5,
            "file provider: {} members cost {ratio:.2}x the CPU time of {} members; linear is \
             2x: {by_members:?}",
            pair[1].0,
            pair[0].0,
        );
    }
}
