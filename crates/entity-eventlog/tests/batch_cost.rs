//! A recorded batch costs work linear in its members and independent of the store's size.
//!
//! The work measured is the CPU time this process spends in one
//! `RecordedProviderFacade::execute_batch` on a SQLite store opened `ProviderTracked`. CPU time,
//! unlike elapsed time, does not grow while the machine runs other work; each point is the least
//! of three runs on fresh stores, and the bounds leave room for what noise remains. Two factors
//! are held apart: the batch size at a fixed store size, and the store size at a fixed batch size.
//!
//! The workspace forbids the `unsafe` a counting allocator needs, and the hashing and decoding
//! that grew are inside the provider's append transaction, where no public counter reaches.
#![cfg(all(feature = "sqlite", feature = "sync-bridge", target_os = "linux"))]

use std::{num::NonZeroU16, path::Path, time::Duration};

use entity_core::Registry;
use entity_eventlog::{
    CapturePolicy, EventlogOperationContext, RecordedProviderFacade,
    sync::{
        BridgeConfig, CallWait, EventlogRecordedStoreOwner, EventlogRecordedStoreProvisioner,
        ProvisionAuthority, ShutdownMode, ShutdownOutcome,
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

const PREFIX: &str = "batch_cost";
const LIMITS: CaptureLimits = CaptureLimits {
    max_events: 20_000,
    max_blobs: 80_000,
    max_projection_rows: 80_000,
    max_payload_bytes: 512 * 1024 * 1024,
};
/// Runs per point; the least is kept, because noise only ever adds time.
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

fn bridge() -> BridgeConfig {
    BridgeConfig {
        queue_capacity: NonZeroU16::new(8).expect("nonzero"),
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

/// Members alternate between creating a new subject and executing on a distinct one the store
/// already holds, as a consumer's mixed batch does.
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

/// User plus system CPU time of this whole process, every thread included: the facade decides
/// and appends on its own worker. Read from `/proc/self/stat`, fields 14 and 15, in clock ticks
/// of 10 ms (`USER_HZ` is 100 on every Linux ABI this workspace builds for).
fn cpu_time() -> Duration {
    let stat = std::fs::read_to_string("/proc/self/stat").expect("/proc/self/stat");
    // The command name, field 2, is parenthesised and may hold spaces; fields count after it.
    let after = &stat[stat.rfind(')').expect("command name") + 2..];
    let fields: Vec<&str> = after.split(' ').collect();
    let ticks: u64 =
        fields[11].parse::<u64>().expect("utime") + fields[12].parse::<u64>().expect("stime");
    Duration::from_millis(ticks * 10)
}

/// CPU time of one batch of `batch` members on a fresh store already holding `store` subjects.
fn batch_cost(directory: &Path, batch: usize, store: usize) -> Duration {
    assert!(
        store >= batch / 2,
        "every touched subject is a distinct seeded one"
    );
    let own = tempfile::tempdir_in(directory).expect("store directory");
    let path = own.path().join("store.sqlite3");
    let mut provisioned = RecordedProviderFacade::provision(
        registry(),
        EventlogRecordedStoreProvisioner::Sqlite {
            path: path.to_string_lossy().into_owned(),
            prefix: PREFIX.to_owned(),
            authority: ProvisionAuthority {
                logical_scope: "batch-cost-scope".to_owned(),
                tenant: "batch-cost-tenant".to_owned(),
                expected_stream_identity: None,
            },
            limits: LIMITS,
        },
        context("provision"),
        bridge(),
    )
    .expect("SQLite authority provisioned");
    let authority = provisioned.authority().clone();
    assert_eq!(
        provisioned.shutdown(ShutdownMode::Drain, CallWait::Forever),
        ShutdownOutcome::Joined { provider: Ok(()) }
    );
    let mut facade = RecordedProviderFacade::start_with_read_policy(
        registry(),
        EventlogRecordedStoreOwner::Sqlite {
            path: path.to_string_lossy().into_owned(),
            prefix: PREFIX.to_owned(),
            authority,
            limits: LIMITS,
        },
        bridge(),
        CapturePolicy::ProviderTracked,
    )
    .expect("tracked facade starts");
    // Seeded in small batches, so seeding stays cheap whatever a large batch costs.
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

/// The least CPU time of `RUNS` batches of one shape.
fn least_cost(directory: &Path, batch: usize, store: usize) -> Duration {
    let cost = (0..RUNS)
        .map(|_| batch_cost(directory, batch, store))
        .min()
        .expect("at least one run");
    eprintln!("{batch:>4} members on {store:>4} subjects: {cost:?} CPU");
    cost
}

fn ratio(larger: Duration, smaller: Duration) -> f64 {
    // A point under one clock tick would make any ratio meaningless.
    larger.as_secs_f64() / smaller.max(Duration::from_millis(10)).as_secs_f64()
}

/// Doubling a batch's members at a fixed store size at most about doubles its cost, and doubling
/// the store at a fixed batch size leaves it about where it was. Each factor is measured at three
/// points. A batch's blob holds every member, and the projector hashed and decoded it once per
/// member, so 0.30.1 cost the square of the members: 3.7x per doubling here.
#[test]
fn a_recorded_batch_costs_time_linear_in_its_members_and_flat_in_the_store() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("directory");
    let store = 64;
    let by_members: Vec<(usize, Duration)> = [32, 64, 128]
        .into_iter()
        .map(|batch| (batch, least_cost(directory.path(), batch, store)))
        .collect();
    let batch = 32;
    let by_store: Vec<(usize, Duration)> = [64, 128, 256]
        .into_iter()
        .map(|store| (store, least_cost(directory.path(), batch, store)))
        .collect();
    for pair in by_members.windows(2) {
        let ratio = ratio(pair[1].1, pair[0].1);
        assert!(
            ratio <= 2.5,
            "{} members on {store} subjects cost {ratio:.2}x the CPU time of {} members; linear \
             is 2x: {by_members:?}",
            pair[1].0,
            pair[0].0,
        );
    }
    for pair in by_store.windows(2) {
        let ratio = ratio(pair[1].1, pair[0].1);
        assert!(
            ratio <= 1.75,
            "{batch} members on {} subjects cost {ratio:.2}x the CPU time on {} subjects; flat \
             is 1x: {by_store:?}",
            pair[1].0,
            pair[0].0,
        );
    }
}
