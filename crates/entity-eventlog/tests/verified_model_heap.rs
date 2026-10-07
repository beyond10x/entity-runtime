//! Release probe: the resident memory one verified handle keeps, per seeded store size.
//!
//! Run explicitly with `cargo +1.91.0 test --release -p entity-eventlog
//! --features sqlite,sync-bridge --test verified_model_heap -- --ignored --nocapture`.
//!
//! The stores are the shared-clock shapes `shared_clock_cost.rs` seeds — 55, 601 and 1,203
//! events of the 96-field `metadata` definition — seeded here by a copy of its seeding, because
//! the evidence of that probe pins its file's digest. The seeding process measures nothing. Each
//! measurement runs in a process of its own, this test binary re-run with `ER_MODEL_HEAP_STORE`
//! naming one seeded file, so that what one store's open left with the allocator is not part of
//! the next one's numbers. `ER_MODEL_HEAP_EVENTS` selects shapes (`601` or `55,1203`) and
//! `ER_MODEL_HEAP_RUNS` the runs per shape (3).
//!
//! The workspace forbids the `unsafe` a counting allocator needs, so the measure is the kernel's:
//! `VmRSS` and `VmHWM` from `/proc/self/status`, with the peak reset by writing `5` to
//! `/proc/self/clear_refs` just before the open. `live` is resident bytes with the handle alive
//! minus resident bytes just before the open; `peak` is the high-water mark minus that same
//! baseline; `dropped` is resident bytes after the handle is dropped minus the baseline, which is
//! what the allocator kept of freed memory and therefore how far `live` can overstate the heap.
#![cfg(all(feature = "sqlite", feature = "sync-bridge"))]

use entity_core::{Registry, Runtime};
use entity_eventlog::{
    AsyncBindingProvisioner, Authority, CapturePolicy, ErRecordedProjector,
    EventlogBindingProvisioner, EventlogOperationContext, EventlogRecordedStore, projection_specs,
};
use entity_store::{
    Expect, RecordedCommit, Recording,
    asynchronous::{
        AppendMember, BatchKey, RecordedEntry, batch_comparison_bytes, canonical_domain_bytes,
        original_request_comparison_bytes, record_comparison_bytes,
    },
};
use eventlog_core::{
    AppendGroup, AtomicEventStore, CaptureLimits, CommandMeta, ConsistentTenantCapture, EventStore,
    Expected, InlineProjectionAdmin, NewEvent, NoGuard, StreamAppend, StreamId, TenantId,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{path::Path, process::Command, sync::Arc};
use time::OffsetDateTime;

const PREFIX: &str = "shared_clock_cost";
const TENANT: &str = "shared-clock";
const SCOPE: &str = "shared-clock-scope";
const LIMITS: CaptureLimits = CaptureLimits {
    max_events: 10_000,
    max_blobs: 40_000,
    max_projection_rows: 40_000,
    max_payload_bytes: 512 * 1024 * 1024,
};
/// The shapes `shared_clock_cost.rs` seeds, in events including the authority binding.
const SHAPES: [usize; 3] = [55, 601, 1203];
/// The name this test is re-run under, which the measuring process is selected by.
const PROBE: &str = "one_verified_handle_reports_the_resident_bytes_it_keeps_per_event";

fn registry() -> Registry {
    let mut fields = serde_json::Map::new();
    fields.insert("title".into(), json!({"type":"string", "required":true}));
    for index in 0..96 {
        fields.insert(
            format!("metadata_field_{index:04}_carrying_a_long_declared_name"),
            json!({"type":"string", "max_length":64}),
        );
    }
    let mut registry = Registry::new();
    registry
        .register(
            serde_json::from_value(json!({
                "entity":"metadata", "version":1, "schema":{"fields":fields},
                "lifecycle":{"initial":"open", "states":["open"]},
                "operations":{"touch":{"transitions":[{"from":"open", "to":"open"}], "emits":[]}}
            }))
            .unwrap(),
        )
        .unwrap();
    registry
}

fn recording(id: &str) -> Recording {
    Recording {
        record_id: id.into(),
        recorded_at: "2026-10-03T00:00:00Z".into(),
        correlation: None,
        causation: None,
        actor: None,
    }
}
fn context(id: &str) -> EventlogOperationContext {
    EventlogOperationContext {
        subject: "cost-probe".into(),
        actor: "entity-eventlog-test".into(),
        request_id: format!("request-{id}"),
        trace_id: format!("trace-{id}"),
        causation_id: None,
        causation_depth: 0,
        occurred_at: OffsetDateTime::UNIX_EPOCH,
    }
}
fn framed(domain: &str, bytes: &[u8]) -> String {
    let mut hash = Sha256::new();
    hash.update(domain.as_bytes());
    hash.update([0]);
    hash.update((bytes.len() as u64).to_be_bytes());
    hash.update(bytes);
    format!("sha256:{:x}", hash.finalize())
}
fn key(domain: &str, value: Value) -> String {
    let wrapped = canonical_domain_bytes("_", value).unwrap();
    framed(domain, &wrapped[5..wrapped.len() - 1])
}

// The fixture's wire envelopes, exactly as `shared_clock_cost.rs` writes them. The measured open
// verifies every byte of them with the real projector rows, so a fixture shortcut is refused there.
async fn seed_group(
    backend: &eventlog_sqlite::SqliteEventStore,
    authority: &Authority,
    index: usize,
    members: Vec<AppendMember>,
) {
    let batch_key = BatchKey::Named(format!("seed-{index}"));
    let batch_wire = json!(["named", format!("seed-{index}")]);
    let batch_bytes = batch_comparison_bytes(&batch_key, &members).unwrap();
    let batch_digest = framed("er.eventlog.batch-blob-key/1", &batch_bytes);
    let mut blobs = vec![(batch_digest.clone(), batch_bytes)];
    let tenant = TenantId::new(&authority.tenant).unwrap();
    let mut appends = Vec::new();
    for (member_index, member) in members.iter().enumerate() {
        let subject = member.entry.subject();
        let record_bytes = record_comparison_bytes(&member.entry).unwrap();
        let record_digest = framed("er.eventlog.record-blob-key/1", &record_bytes);
        let request_digest = framed("er.eventlog.request-blob-key/1", &member.request_bytes);
        let wrapper = canonical_domain_bytes(
            "er.eventlog.recorded-entry/1",
            json!({
                "authority":authority, "batch_blob":batch_digest, "batch_key":batch_wire,
                "member_index":member_index, "record_blob":record_digest,
                "request_blob":request_digest, "subject":[subject.entity, subject.id],
            }),
        )
        .unwrap();
        let wrapper_digest = framed("er.eventlog.recorded-entry-blob-key/1", &wrapper);
        blobs.extend([
            (record_digest, record_bytes),
            (request_digest, member.request_bytes.clone()),
            (wrapper_digest.clone(), wrapper),
        ]);
        let stream_key = key(
            "er.eventlog.subject-stream-key/1",
            json!({"authority":authority, "subject":[subject.entity, subject.id]}),
        );
        appends.push(StreamAppend {
            stream: StreamId::new(tenant.clone(), "er.subject", stream_key).unwrap(),
            expected: match member.expect {
                Expect::Absent => Expected::NoStream,
                Expect::Revision(revision) => Expected::Exact(revision),
            },
            events: vec![
                NewEvent::new("er.recorded_entry", 1, json!({"blob":wrapper_digest})).unwrap(),
            ],
        });
    }
    let ctx = context(&format!("seed-{index}"));
    let group = AppendGroup {
        tenant,
        appends,
        meta: CommandMeta {
            idempotency_key: key(
                "er.eventlog.batch-command-key/1",
                json!({"authority":authority,"batch_key":batch_wire}),
            ),
            request_hash: batch_digest,
            subject: ctx.subject,
            actor: ctx.actor,
            request_id: ctx.request_id,
            trace_id: ctx.trace_id,
            causation_id: None,
            causation_depth: 0,
            occurred_at: ctx.occurred_at,
            claim: None,
        },
    };
    let result = backend
        .append_group_guarded_with_blobs(&group, Arc::new(NoGuard), &blobs)
        .await
        .expect("fixture group/projector commits");
    assert!(!result.deduplicated);
    assert_eq!(result.appends.len(), 2);
}

/// Seeds `total_events` shared-clock events, and returns the bytes of every blob they bind.
async fn seed(path: &Path, registry: &Registry, total_events: usize) -> usize {
    assert_eq!(total_events % 2, 1);
    let backend = Arc::new(
        eventlog_sqlite::SqliteEventStore::open(path.to_str().unwrap(), PREFIX)
            .await
            .unwrap(),
    );
    let tenant = TenantId::new(TENANT).unwrap();
    let authority = Authority {
        logical_scope: SCOPE.into(),
        tenant: tenant.as_str().into(),
        stream_identity: backend.stream_identity(&tenant).await.unwrap(),
    };
    let projector = Arc::new(ErRecordedProjector::new());
    backend.create_projections(projector.clone()).await.unwrap();
    backend.attach_inline_existing(projector).await.unwrap();
    EventlogBindingProvisioner::new(backend.clone(), LIMITS)
        .provision_binding(authority.clone(), context("binding"))
        .await
        .unwrap();
    let runtime = Runtime::new(registry);
    let mut clock = None;
    for index in 0..(total_events - 1) / 2 {
        let decision = match clock.as_ref() {
            None => runtime.create("metadata", 1, "clock", json!({"title":"clock"})),
            Some(instance) => runtime.execute(instance, "touch", json!({})),
        }
        .unwrap();
        let expected = clock
            .as_ref()
            .map_or(Expect::Absent, |state: &entity_core::EntityInstance| {
                Expect::Revision(state.revision)
            });
        clock = Some(decision.instance.clone());
        let clock_entry = RecordedEntry::Decision(
            RecordedCommit::new(decision, &recording(&format!("clock-{index}"))).unwrap(),
        );
        let peer_id = format!("peer-{index}");
        let peer_entry = RecordedEntry::Decision(
            RecordedCommit::new(
                runtime
                    .create("metadata", 1, &peer_id, json!({"title":peer_id}))
                    .unwrap(),
                &recording(&format!("peer-{index}")),
            )
            .unwrap(),
        );
        let members = [(clock_entry, expected), (peer_entry, Expect::Absent)]
            .into_iter()
            .map(|(entry, expect)| {
                let bytes = original_request_comparison_bytes(&entry).unwrap();
                AppendMember::new(expect, entry, bytes)
            })
            .collect();
        seed_group(&backend, &authority, index, members).await;
    }
    let capture = backend
        .capture_tenant(&tenant, projection_specs(), LIMITS)
        .await
        .unwrap();
    assert_eq!(
        capture.events.len(),
        total_events,
        "includes one authority-binding event"
    );
    capture.blobs.iter().map(|blob| blob.bytes.len()).sum()
}

/// One field of `/proc/self/status`, in bytes.
fn status_bytes(field: &str) -> usize {
    let status = std::fs::read_to_string("/proc/self/status").expect("/proc/self/status");
    let line = status
        .lines()
        .find(|line| line.starts_with(field))
        .unwrap_or_else(|| panic!("{field} is not in /proc/self/status"));
    let kib: usize = line[field.len()..]
        .trim()
        .trim_end_matches("kB")
        .trim()
        .parse()
        .unwrap_or_else(|_| panic!("{field} is not a kB count: {line}"));
    kib * 1024
}

/// Opens one verified handle over the seeded file `store` names and prints what it keeps.
fn measure(store: &str, events: usize) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let tenant = TenantId::new(TENANT).unwrap();
    // The provider handle, its attached projector and the authority are opened before the
    // baseline: they are what a consumer holds before it asks for a verified handle.
    let (backend, authority) = runtime.block_on(async {
        let backend = Arc::new(
            eventlog_sqlite::SqliteEventStore::open_existing(store, PREFIX)
                .await
                .unwrap(),
        );
        backend
            .attach_inline_existing(Arc::new(ErRecordedProjector::new()))
            .await
            .unwrap();
        let stream_identity = backend
            .stored_stream_identity(&tenant)
            .await
            .unwrap()
            .expect("the seeded tenant has a stream identity");
        let authority = Authority {
            logical_scope: SCOPE.into(),
            tenant: TENANT.into(),
            stream_identity,
        };
        (backend, authority)
    });
    std::fs::write("/proc/self/clear_refs", "5").expect("reset the resident high-water mark");
    let baseline = status_bytes("VmRSS:");
    let handle = runtime
        .block_on(EventlogRecordedStore::open_with_policy(
            backend,
            authority,
            LIMITS,
            CapturePolicy::ProviderTracked,
        ))
        .expect("cold ER verification validates every fixture byte and history");
    let alive = status_bytes("VmRSS:");
    let peak = status_bytes("VmHWM:");
    let calls = std::hint::black_box(&handle).calls();
    assert_eq!(
        (calls.captures, calls.model_builds, calls.records_decoded),
        (1, 1, events - 1),
        "the measured handle is one complete capture and one whole-model build: {calls:?}"
    );
    drop(handle);
    let dropped = status_bytes("VmRSS:");
    let live = alive.saturating_sub(baseline);
    println!(
        "model-heap-run events={events} live_bytes={live} live_per_event={} peak_bytes={} dropped_bytes={} baseline_bytes={baseline}",
        live / events,
        peak.saturating_sub(baseline),
        dropped.saturating_sub(baseline),
    );
}

fn median(values: &mut [usize]) -> usize {
    values.sort_unstable();
    values[values.len() / 2]
}

/// The value of `name=` in one `model-heap-run` line.
fn field(line: &str, name: &str) -> usize {
    line.split_whitespace()
        .find_map(|pair| pair.strip_prefix(name)?.strip_prefix('='))
        .unwrap_or_else(|| panic!("{name} is missing from {line}"))
        .parse()
        .unwrap_or_else(|_| panic!("{name} is not a count in {line}"))
}

/// Reports, per seeded size, the resident bytes one `ProviderTracked` verified handle keeps after
/// its open — the median of several runs, each in a process of its own — and per event.
///
/// It asserts only that each measured handle is the cold open it claims to be: one capture, one
/// whole-model build, every recorded entry decoded. The numbers are the evidence; the ratio
/// between two revisions is read off two runs of this probe, not asserted inside one.
#[test]
#[ignore = "release memory probe; run explicitly on a quiet host"]
fn one_verified_handle_reports_the_resident_bytes_it_keeps_per_event() {
    assert!(!cfg!(debug_assertions), "this probe requires --release");
    if let Ok(store) = std::env::var("ER_MODEL_HEAP_STORE") {
        let events = std::env::var("ER_MODEL_HEAP_EVENTS")
            .expect("a measuring run names its shape")
            .parse()
            .expect("one event count");
        measure(&store, events);
        return;
    }
    let shapes: Vec<usize> = std::env::var("ER_MODEL_HEAP_EVENTS").map_or_else(
        |_| SHAPES.to_vec(),
        |listed| {
            listed
                .split(',')
                .map(|count| count.trim().parse().expect("an event count"))
                .collect()
        },
    );
    let runs: usize =
        std::env::var("ER_MODEL_HEAP_RUNS").map_or(3, |runs| runs.parse().expect("a run count"));
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let registry = registry();
    let seeding = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    for events in shapes {
        let path = directory.path().join(format!("heap-{events}.sqlite3"));
        let blob_bytes = seeding.block_on(seed(&path, &registry, events));
        println!("model-heap-seed events={events} blob_bytes={blob_bytes}");
        let (mut live, mut peak, mut dropped) = (Vec::new(), Vec::new(), Vec::new());
        for _ in 0..runs {
            let output = Command::new(std::env::current_exe().expect("this test binary"))
                .args([
                    "--ignored",
                    "--exact",
                    PROBE,
                    "--nocapture",
                    "--test-threads=1",
                ])
                .env("ER_MODEL_HEAP_STORE", &path)
                .env("ER_MODEL_HEAP_EVENTS", events.to_string())
                .output()
                .expect("the measuring process runs");
            let stdout = String::from_utf8_lossy(&output.stdout);
            assert!(
                output.status.success(),
                "the measuring process failed: {}\n{stdout}\n{}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            );
            // libtest prints the test's name on the line the run line starts on.
            let line = stdout
                .lines()
                .find_map(|line| line.find("model-heap-run ").map(|at| &line[at..]))
                .unwrap_or_else(|| panic!("the measuring process printed no run line:\n{stdout}"));
            println!("{line}");
            live.push(field(line, "live_bytes"));
            peak.push(field(line, "peak_bytes"));
            dropped.push(field(line, "dropped_bytes"));
        }
        let live_median = median(&mut live);
        println!(
            "model-heap events={events} runs={runs} live_median_bytes={live_median} live_median_per_event={} peak_median_bytes={} dropped_median_bytes={} live_bytes={live:?}",
            live_median / events,
            median(&mut peak),
            median(&mut dropped),
        );
    }
}
