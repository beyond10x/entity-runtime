//! A `ProviderTracked` open of a SQLite store with durable open checkpoints enabled verifies the
//! persisted checkpoint and the suffix after it, not the whole history
//! (`docs/design/recorded-open-checkpoint-v0.1.md`, GitHub #55).
//!
//! Each test names where a verification an open used to do now happens, or shows that it still
//! happens: a `FullVerification` open, a complete read, the provider's continuity proof, the suffix
//! verifier, or the read that reads a row and the blobs it names.
#![cfg(all(feature = "sqlite", feature = "sync-bridge", feature = "file"))]

use std::{num::NonZeroU16, path::Path, sync::Arc};

use entity_core::Registry;
use entity_eventlog::{
    Authority, CapturePolicy, ErRecordedProjector, EventlogOperationContext, EventlogRecordedStore,
    OpenVerification, RecordedProviderFacade, StoreCalls,
    sync::{
        BridgeConfig, CallWait, EventlogRecordedStoreOwner, EventlogRecordedStoreProvisioner,
        ProvisionAuthority, ShutdownMode, ShutdownOutcome, SyncReadError,
    },
};
use entity_executor::{BatchAction, CreateRequest, ExecuteRequest, ExecutionError, Executor};
use entity_store::{
    RecordedObservation, Recording,
    asynchronous::{
        AsyncRecordedReader, AsyncStateReader, AsyncStoreError, BatchKey, Subject, WriteFailure,
    },
};
use eventlog_core::{CaptureLimits, EventStore, InlineProjectionAdmin, TenantId};
use serde_json::json;
use time::OffsetDateTime;

const PREFIX: &str = "bounded_open";
const LIMITS: CaptureLimits = CaptureLimits {
    max_events: 512,
    max_blobs: 4_096,
    max_projection_rows: 4_096,
    max_payload_bytes: 16 * 1024 * 1024,
};
/// Written into one ticket's title so the raw-edit test can find that record's bytes in the file.
const MARKER: &str = "raw-edit-marker-7f3a";

fn registry() -> Registry {
    let definition = serde_json::from_value(json!({
        "entity": "ticket",
        "version": 1,
        "schema": { "fields": { "title": { "type": "string", "required": true } } },
        "lifecycle": { "initial": "open", "states": ["open", "closed"] },
        "operations": {
            "touch": { "transitions": [{ "from": "open", "to": "open" }], "emits": [] },
            "close": { "transitions": [{ "from": "open", "to": "closed" }], "emits": [] }
        }
    }))
    .expect("definition parses");
    let mut registry = Registry::new();
    registry.register(definition).expect("definition validates");
    registry
}

fn context(label: &str) -> EventlogOperationContext {
    EventlogOperationContext {
        subject: "bounded-open".to_owned(),
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
        recorded_at: "2026-10-07T00:00:00Z".to_owned(),
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

fn block_on<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime")
        .block_on(future)
}

fn ticket(id: &str) -> Subject {
    Subject::new("ticket", id).expect("subject")
}

fn create(id: &str, title: &str) -> BatchAction {
    BatchAction::Create(CreateRequest {
        subject: ticket(id),
        definition_version: 1,
        fields: json!({ "title": title }),
        recording: recording(&format!("create-{id}")),
    })
}

fn execute(id: &str, revision: u64, operation: &str, record: &str) -> BatchAction {
    BatchAction::Execute(ExecuteRequest {
        subject: ticket(id),
        expected_revision: revision,
        operation: operation.to_owned(),
        arguments: json!({}),
        fulfillments: Default::default(),
        recording: recording(record),
    })
}

fn observe(id: &str, revision: u64, record: &str) -> BatchAction {
    BatchAction::Observe(RecordedObservation {
        entity: "ticket".into(),
        id: id.into(),
        revision,
        envelope: recording(record)
            .seal(json!({ "evidence": record }))
            .expect("observation envelope"),
    })
}

/// A provisioned SQLite file with every handle on it closed.
fn provisioned(path: &Path, limits: CaptureLimits) -> Authority {
    let mut facade = RecordedProviderFacade::provision(
        registry(),
        EventlogRecordedStoreProvisioner::Sqlite {
            path: path.to_string_lossy().into_owned(),
            prefix: PREFIX.to_owned(),
            authority: ProvisionAuthority {
                logical_scope: "bounded-scope".to_owned(),
                tenant: "bounded-tenant".to_owned(),
                expected_stream_identity: None,
            },
            limits,
        },
        context("provision"),
        bridge(),
    )
    .expect("SQLite authority provisioned");
    let authority = facade.authority().clone();
    assert_eq!(
        facade.shutdown(ShutdownMode::Drain, CallWait::Forever),
        ShutdownOutcome::Joined { provider: Ok(()) }
    );
    authority
}

/// The provider handle an owner opens: existing file, ER projector attached.
async fn attached(path: &Path) -> Arc<eventlog_sqlite::SqliteEventStore> {
    let backend = Arc::new(
        eventlog_sqlite::SqliteEventStore::open_existing(path.to_str().expect("utf-8"), PREFIX)
            .await
            .expect("provisioned SQLite file opens"),
    );
    backend
        .attach_inline_existing(Arc::new(ErRecordedProjector::new()))
        .await
        .expect("ER projector attaches");
    backend
}

async fn open(
    path: &Path,
    authority: &Authority,
    limits: CaptureLimits,
    policy: CapturePolicy,
) -> Result<EventlogRecordedStore, AsyncStoreError> {
    EventlogRecordedStore::open_with_policy(attached(path).await, authority.clone(), limits, policy)
        .await
}

async fn enable(path: &Path) {
    eventlog_sqlite::SqliteEventStore::open_existing(path.to_str().expect("utf-8"), PREFIX)
        .await
        .expect("store opens")
        .enable_durable_continuity()
        .await
        .expect("durable continuity installs");
}

async fn run(
    store: &EventlogRecordedStore,
    key: &str,
    actions: Vec<BatchAction>,
) -> Result<(), ExecutionError> {
    let registry = registry();
    let operation = store.operation(context(key));
    Executor::new(&registry, &operation)
        .batch(BatchKey::Named(key.to_owned()), actions)
        .await
        .map(|_| ())
}

/// Writes through a `FullVerification` handle, which never touches the open checkpoint.
async fn write(path: &Path, authority: &Authority, key: &str, actions: Vec<BatchAction>) {
    let store = open(path, authority, LIMITS, CapturePolicy::FullVerification)
        .await
        .expect("full open");
    run(&store, key, actions).await.expect("batch commits");
}

/// Opens tracked, persists the observation it verified, and closes.
async fn checkpoint(path: &Path, authority: &Authority) -> OpenVerification {
    let store = open(path, authority, LIMITS, CapturePolicy::ProviderTracked)
        .await
        .expect("tracked open");
    let opened = store.open_verification();
    store
        .write_open_checkpoint()
        .await
        .expect("checkpoint write");
    opened
}

/// A store holding tickets a..d, a closed one, a named batch, an observation, and one single
/// record written after the checkpoint is not, so every row kind and state source is exercised.
async fn populated(path: &Path) -> Authority {
    let authority = provisioned(path, LIMITS);
    enable(path).await;
    write(path, &authority, "first", vec![create("a", "alpha")]).await;
    write(
        path,
        &authority,
        "pair",
        vec![
            execute("a", 1, "touch", "touch-a"),
            create("b", MARKER),
            create("c", "gamma"),
        ],
    )
    .await;
    write(path, &authority, "seen", vec![observe("a", 2, "observe-a")]).await;
    write(
        path,
        &authority,
        "closing",
        vec![execute("c", 1, "close", "close-c")],
    )
    .await;
    authority
}

fn subjects() -> Vec<Subject> {
    ["a", "b", "c", "absent"].into_iter().map(ticket).collect()
}

fn records() -> Vec<&'static str> {
    vec![
        "create-a",
        "touch-a",
        "create-b",
        "create-c",
        "observe-a",
        "close-c",
        "absent",
    ]
}

fn keys() -> Vec<BatchKey> {
    ["first", "pair", "seen", "closing", "absent"]
        .into_iter()
        .map(|key| BatchKey::Named(key.into()))
        .collect()
}

/// Every state, history, record and batch the store's readers and an operation's per-entity
/// readers answer, rendered for comparison.
async fn answers(store: &EventlogRecordedStore) -> Vec<String> {
    let operation = store.operation(context("reads"));
    let mut answers = Vec::new();
    for subject in subjects() {
        answers.push(format!("{:?}", store.load(&subject).await));
        answers.push(format!("{:?}", store.history(&subject).await));
        answers.push(format!("{:?}", operation.load(&subject).await));
        answers.push(format!("{:?}", operation.history(&subject).await));
    }
    for record in records() {
        answers.push(format!("{:?}", store.lookup_record(record).await));
        answers.push(format!("{:?}", operation.lookup_record(record).await));
    }
    for key in keys() {
        answers.push(format!("{:?}", store.lookup_batch(&key).await));
        answers.push(format!("{:?}", operation.lookup_batch(&key).await));
    }
    answers
}

/// Acceptance 2 and 5 at the store level: an enabled store's tracked open after a checkpoint at
/// the head asks the provider one continuity question and verifies no event, blob or row.
#[test]
fn a_tracked_open_at_the_checkpoint_verifies_no_event_before_it() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("directory");
    let path = directory.path().join("store.sqlite3");
    block_on(async {
        let authority = populated(&path).await;
        assert_eq!(
            checkpoint(&path, &authority).await,
            OpenVerification::Complete,
            "no checkpoint was persisted yet, so the first open verified completely"
        );
        let store = open(&path, &authority, LIMITS, CapturePolicy::ProviderTracked)
            .await
            .expect("bounded open");
        assert_eq!(store.open_verification(), OpenVerification::Checkpoint);
        assert_eq!(
            store.calls(),
            StoreCalls::default(),
            "the open took no capture, built no model and decoded no record"
        );
    });
}

/// Acceptance 2: appends after the checkpoint are verified one by one and nothing before them is.
#[test]
fn an_open_after_appends_verifies_only_the_suffix() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("directory");
    let path = directory.path().join("store.sqlite3");
    block_on(async {
        let authority = populated(&path).await;
        checkpoint(&path, &authority).await;
        write(
            &path,
            &authority,
            "later",
            vec![create("d", "delta"), execute("b", 1, "touch", "touch-b")],
        )
        .await;
        let store = open(&path, &authority, LIMITS, CapturePolicy::ProviderTracked)
            .await
            .expect("suffix open");
        assert_eq!(
            store.open_verification(),
            OpenVerification::Suffix { events: 2 }
        );
        let calls = store.calls();
        assert_eq!(
            (
                calls.captures,
                calls.model_builds,
                calls.model_advances,
                calls.records_decoded
            ),
            (0, 0, 1, 2),
            "one advance decoding exactly the two appended records: {calls:?}"
        );
        assert_eq!(
            store
                .load(&ticket("b"))
                .await
                .expect("load")
                .expect("b")
                .revision,
            2
        );
    });
}

/// Acceptance 4: `FullVerification` never reads the checkpoint, so its open verifies the whole
/// history even when a valid checkpoint would let a tracked open skip it.
#[test]
fn full_verification_verifies_the_whole_history_beside_a_valid_checkpoint() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("directory");
    let path = directory.path().join("store.sqlite3");
    block_on(async {
        let authority = populated(&path).await;
        checkpoint(&path, &authority).await;
        let full = open(&path, &authority, LIMITS, CapturePolicy::FullVerification)
            .await
            .expect("full open");
        let calls = full.calls();
        assert_eq!(full.open_verification(), OpenVerification::Complete);
        assert_eq!(
            (calls.captures, calls.model_builds, calls.records_decoded),
            (1, 1, 6),
            "one complete capture and one whole-model build decoding all six records: {calls:?}"
        );
        assert!(
            !full.write_open_checkpoint().await.expect("write call"),
            "a FullVerification handle never writes the checkpoint"
        );
        let tracked = open(&path, &authority, LIMITS, CapturePolicy::ProviderTracked)
            .await
            .expect("tracked open");
        assert_eq!(
            tracked.open_verification(),
            OpenVerification::Checkpoint,
            "the checkpoint the FullVerification open ignored was valid"
        );
    });
}

/// "Every answer a bounded handle gives equals what a complete handle gives": states, records,
/// batches and histories, through the store's readers and an operation's per-entity readers, with
/// no complete verification on the bounded handle.
#[test]
fn every_answer_of_a_bounded_handle_equals_a_complete_handles() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("directory");
    let path = directory.path().join("store.sqlite3");
    block_on(async {
        let authority = populated(&path).await;
        checkpoint(&path, &authority).await;
        let bounded = open(&path, &authority, LIMITS, CapturePolicy::ProviderTracked)
            .await
            .expect("bounded open");
        let full = open(&path, &authority, LIMITS, CapturePolicy::FullVerification)
            .await
            .expect("full open");
        let bounded_answers = answers(&bounded).await;
        let full_answers = answers(&full).await;
        assert_eq!(bounded_answers.len(), full_answers.len());
        for (index, (bounded, full)) in bounded_answers.iter().zip(&full_answers).enumerate() {
            assert_eq!(bounded, full, "answer {index} differs");
        }
        assert!(
            bounded_answers
                .iter()
                .all(|answer| answer.starts_with("Ok(")),
            "every read answered: {bounded_answers:?}"
        );
        let calls = bounded.calls();
        assert_eq!(
            (calls.captures, calls.model_builds),
            (0, 0),
            "the bounded handle answered without a complete verification: {calls:?}"
        );
        assert!(
            calls.scoped_reads > 0,
            "its histories were read per entity: {calls:?}"
        );
    });
}

/// "A complete snapshot ... a complete capture": a bounded handle's complete read verifies the
/// whole store once, after which it holds a whole model like any tracked handle.
#[test]
fn a_complete_read_of_a_bounded_handle_verifies_the_whole_store_once() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("directory");
    let path = directory.path().join("store.sqlite3");
    block_on(async {
        let authority = populated(&path).await;
        checkpoint(&path, &authority).await;
        let bounded = open(&path, &authority, LIMITS, CapturePolicy::ProviderTracked)
            .await
            .expect("bounded open");
        let full = open(&path, &authority, LIMITS, CapturePolicy::FullVerification)
            .await
            .expect("full open");
        let snapshot = bounded
            .complete_snapshot(&authority.logical_scope)
            .await
            .expect("complete read");
        assert_eq!(
            snapshot,
            full.complete_snapshot(&authority.logical_scope)
                .await
                .expect("complete read")
        );
        let calls = bounded.calls();
        assert_eq!((calls.captures, calls.model_builds), (1, 1), "{calls:?}");
        bounded.load(&ticket("a")).await.expect("load");
        bounded
            .complete_snapshot(&authority.logical_scope)
            .await
            .expect("complete read");
        assert_eq!(
            bounded.calls(),
            calls,
            "an unchanged provider reuses the whole model the complete read built"
        );
    });
}

/// "What a bounded open stops detecting: edits that bypass SQL while no handle is open." The
/// provider cannot see a raw file edit, so the open starts from the checkpoint; the read of the
/// edited blob holds it to its digest and refuses it, and a FullVerification open refuses it too.
#[test]
fn a_raw_file_edit_is_refused_by_the_read_of_the_edited_blob_and_by_full_verification() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("directory");
    let path = directory.path().join("store.sqlite3");
    // The blobs holding the marker, as the store bound them: the record, its request and its batch.
    let (authority, blobs) = block_on(async {
        let authority = populated(&path).await;
        checkpoint(&path, &authority).await;
        let store = open(&path, &authority, LIMITS, CapturePolicy::FullVerification)
            .await
            .expect("full open");
        let Some(entity_store::asynchronous::RecordLookup::Committed(record)) =
            store.lookup_record("create-b").await.expect("lookup")
        else {
            panic!("create-b is committed");
        };
        let batch = store
            .lookup_batch(&BatchKey::Named("pair".into()))
            .await
            .expect("lookup")
            .expect("pair is committed");
        (
            authority,
            [
                record.record_bytes,
                record.request_bytes,
                batch.comparison_bytes,
            ],
        )
    });
    let marker = MARKER.as_bytes();
    let rewrite = |bytes: &mut Vec<u8>, from: &[u8], to: &[u8]| {
        let mut count = 0;
        let mut at = 0;
        while let Some(found) = bytes[at..]
            .windows(from.len())
            .position(|window| window == from)
        {
            let start = at + found;
            bytes[start..start + to.len()].copy_from_slice(to);
            at = start + from.len();
            count += 1;
        }
        count
    };
    let hash = |bytes: &[u8]| format!("{:x}", <sha2::Sha256 as sha2::Digest>::digest(bytes));
    let mut edited = (0, 0);
    for file in [path.clone(), path.with_extension("sqlite3-wal")] {
        let Ok(mut bytes) = std::fs::read(&file) else {
            continue;
        };
        // The provider re-hashes a stored blob on every read against the hash it stored beside
        // it, so a raw edit that leaves that hash refuses below Entity Runtime. This one keeps
        // the provider's hash consistent, which only Entity Runtime's framed digests catch.
        for blob in &blobs {
            let mut changed = blob.clone();
            assert!(
                rewrite(&mut changed, marker, b"R") > 0,
                "the blob holds the marker"
            );
            edited.1 += rewrite(&mut bytes, hash(blob).as_bytes(), hash(&changed).as_bytes());
        }
        edited.0 += rewrite(&mut bytes, marker, b"R");
        std::fs::write(&file, bytes).expect("raw edit");
    }
    assert!(
        edited.0 >= 3 && edited.1 == 3,
        "the marker, and the provider's hash of each of the three blobs, were rewritten in place: {edited:?}"
    );
    block_on(async {
        let bounded = open(&path, &authority, LIMITS, CapturePolicy::ProviderTracked)
            .await
            .expect("the provider sees no SQL write, so the open starts from the checkpoint");
        assert_eq!(bounded.open_verification(), OpenVerification::Checkpoint);
        assert!(
            matches!(
                bounded.load(&ticket("a")).await,
                Ok(Some(ref instance)) if instance.revision == 2
            ),
            "a state the edit did not touch is answered"
        );
        // A write the edit does not touch moves this handle past the persisted checkpoint, so only
        // the refusal below keeps it from persisting a new one.
        run(&bounded, "after-edit", vec![create("e", "epsilon")])
            .await
            .expect("a write beside the edit commits");
        let refused = bounded.load(&ticket("b")).await;
        assert!(
            matches!(
                &refused,
                Err(AsyncStoreError::ProviderIntegrity { detail, .. })
                    if detail == "blob digest/domain mismatch"
            ),
            "the edited state blob is refused by the read that reads it: {refused:?}"
        );
        assert!(
            !bounded.write_open_checkpoint().await.expect("write call"),
            "a handle that refused since its last complete verification persists nothing"
        );
        drop(bounded);
        let reopened = open(&path, &authority, LIMITS, CapturePolicy::ProviderTracked)
            .await
            .expect("tracked open");
        assert_eq!(
            reopened.open_verification(),
            OpenVerification::Suffix { events: 1 },
            "the persisted checkpoint is still the one before the refusal"
        );
        drop(reopened);
        let refused = open(&path, &authority, LIMITS, CapturePolicy::FullVerification).await;
        assert!(
            matches!(refused, Err(AsyncStoreError::ProviderIntegrity { .. })),
            "a FullVerification open refuses the edited store: {refused:?}"
        );
    });
}

/// "A write's preflight and read-back: the rows its scope names; its own append as a delta": a
/// bounded handle's own write is verified as a suffix, not by a complete verification.
#[test]
fn a_bounded_handle_verifies_its_own_write_as_a_suffix() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("directory");
    let path = directory.path().join("store.sqlite3");
    block_on(async {
        let authority = populated(&path).await;
        checkpoint(&path, &authority).await;
        let bounded = open(&path, &authority, LIMITS, CapturePolicy::ProviderTracked)
            .await
            .expect("bounded open");
        run(
            &bounded,
            "own",
            vec![create("e", "epsilon"), execute("b", 1, "touch", "touch-b")],
        )
        .await
        .expect("the write commits");
        assert_eq!(
            bounded
                .load(&ticket("b"))
                .await
                .expect("load")
                .expect("b")
                .revision,
            2
        );
        let calls = bounded.calls();
        assert_eq!(
            (calls.captures, calls.model_builds, calls.model_advances),
            (0, 0, 1),
            "the handle's own append was verified as one suffix: {calls:?}"
        );
        assert!(bounded.write_open_checkpoint().await.expect("write"));
        drop(bounded);
        let next = open(&path, &authority, LIMITS, CapturePolicy::ProviderTracked)
            .await
            .expect("next open");
        assert_eq!(next.open_verification(), OpenVerification::Checkpoint);
    });
}

/// "Tenant totals for admit_growth": a bounded handle holds the provider's usage, not its digest
/// set, and refuses a write past its read bound with exactly the totals a whole-model handle
/// reaches on the same store.
#[test]
fn a_bounded_handle_refuses_a_write_past_its_read_bound_as_a_whole_handle_does() {
    let tight = CaptureLimits {
        max_events: 4,
        ..LIMITS
    };
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("directory");
    let path = directory.path().join("store.sqlite3");
    block_on(async {
        let authority = provisioned(&path, tight);
        enable(&path).await;
        let first = open(&path, &authority, tight, CapturePolicy::ProviderTracked)
            .await
            .expect("first open");
        run(
            &first,
            "first",
            vec![create("a", "alpha"), create("b", "beta")],
        )
        .await
        .expect("three events fit four");
        assert!(first.write_open_checkpoint().await.expect("write"));
        drop(first);
        let attempt = || vec![create("c", "gamma"), create("d", "delta")];
        let bounded = open(&path, &authority, tight, CapturePolicy::ProviderTracked)
            .await
            .expect("bounded open");
        assert_eq!(bounded.open_verification(), OpenVerification::Checkpoint);
        let from_rows = run(&bounded, "over", attempt()).await;
        let whole = open(&path, &authority, tight, CapturePolicy::FullVerification)
            .await
            .expect("whole open");
        let from_model = run(&whole, "over", attempt()).await;
        let refusal = |result: &Result<(), ExecutionError>| match result {
            Err(ExecutionError::Write(WriteFailure::NotCommitted(
                AsyncStoreError::BatchExceedsReadBounds {
                    bound,
                    limit,
                    would_hold,
                },
            ))) => Some((bound.clone(), *limit, *would_hold)),
            _ => None,
        };
        assert_eq!(
            refusal(&from_rows),
            Some(("max_events".to_owned(), 4, 5)),
            "{from_rows:?}"
        );
        assert_eq!(refusal(&from_rows), refusal(&from_model));
    });
}

/// A foreign write while a bounded handle is live ends the provider's proof; the handle's next
/// read is a complete verification, as a whole-model tracked handle's is.
#[test]
fn a_foreign_write_under_a_live_bounded_handle_makes_its_next_read_complete() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("directory");
    let path = directory.path().join("store.sqlite3");
    block_on(async {
        let authority = populated(&path).await;
        checkpoint(&path, &authority).await;
        let bounded = open(&path, &authority, LIMITS, CapturePolicy::ProviderTracked)
            .await
            .expect("bounded open");
        bounded.load(&ticket("a")).await.expect("load");
        assert_eq!(bounded.calls().captures, 0);
        // A standalone blob write is not an acknowledged atomic group: the provider journals none.
        let other = attached(&path).await;
        let tenant = TenantId::new(&authority.tenant).expect("tenant");
        let bytes = b"an unreferenced blob";
        let digest = format!("sha256:{:x}", <sha2::Sha256 as sha2::Digest>::digest(bytes));
        other
            .put_blob(&tenant, &digest, bytes)
            .await
            .expect("foreign write");
        bounded
            .load(&ticket("a"))
            .await
            .expect("load after the write");
        let calls = bounded.calls();
        assert_eq!(
            (calls.captures, calls.model_builds),
            (1, 1),
            "the write ended continuity, so the read verified completely: {calls:?}"
        );
    });
}

/// The facade's drain persists the checkpoint; a cancelled shutdown does not.
#[test]
fn a_drain_shutdown_persists_the_checkpoint_and_a_cancelling_one_does_not() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("directory");
    let path = directory.path().join("store.sqlite3");
    let authority = provisioned(&path, LIMITS);
    let owner = || EventlogRecordedStoreOwner::Sqlite {
        path: path.to_string_lossy().into_owned(),
        prefix: PREFIX.to_owned(),
        authority: authority.clone(),
        limits: LIMITS,
    };
    let start = || {
        RecordedProviderFacade::start_with_read_policy(
            registry(),
            owner(),
            bridge(),
            CapturePolicy::ProviderTracked,
        )
        .expect("facade starts")
    };
    let mut facade = start();
    facade
        .enable_durable_open_checkpoints(CallWait::Forever)
        .expect("enable");
    facade
        .execute_batch(
            context("a"),
            BatchKey::Named("a".into()),
            vec![create("a", "alpha")],
            CallWait::Forever,
        )
        .expect("create");
    assert_eq!(
        facade.shutdown(ShutdownMode::CancelQueued, CallWait::Forever),
        ShutdownOutcome::Joined { provider: Ok(()) }
    );
    let mut facade = start();
    assert_eq!(
        facade.open_verification(),
        OpenVerification::Complete,
        "a cancelled shutdown persisted nothing"
    );
    assert_eq!(
        facade.shutdown(ShutdownMode::Drain, CallWait::Forever),
        ShutdownOutcome::Joined { provider: Ok(()) }
    );
    let mut facade = start();
    assert_eq!(facade.open_verification(), OpenVerification::Checkpoint);
    assert!(
        !facade
            .write_open_checkpoint(CallWait::Forever)
            .expect("explicit write"),
        "a handle holding exactly the persisted record writes nothing"
    );
    facade
        .disable_durable_open_checkpoints(CallWait::Forever)
        .expect("disable");
    assert_eq!(
        facade.shutdown(ShutdownMode::Drain, CallWait::Forever),
        ShutdownOutcome::Joined { provider: Ok(()) }
    );
    let facade = start();
    assert_eq!(
        facade.open_verification(),
        OpenVerification::Complete,
        "after disabling, the persisted checkpoint restores to nothing"
    );
}

/// The discard call is the recovery from a checkpoint ahead of the head, and an owner whose
/// provider never holds a checkpoint has nothing to discard.
#[test]
fn a_discard_through_the_owner_replaces_the_checkpoint_and_the_next_open_is_complete() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("directory");
    let path = directory.path().join("store.sqlite3");
    let authority = block_on(async {
        let authority = populated(&path).await;
        checkpoint(&path, &authority).await;
        authority
    });
    let owner = EventlogRecordedStoreOwner::Sqlite {
        path: path.to_string_lossy().into_owned(),
        prefix: PREFIX.to_owned(),
        authority: authority.clone(),
        limits: LIMITS,
    };
    assert_eq!(owner.discard_open_checkpoint(), Ok(true));
    block_on(async {
        let store = open(&path, &authority, LIMITS, CapturePolicy::ProviderTracked)
            .await
            .expect("open after discard");
        assert_eq!(store.open_verification(), OpenVerification::Complete);
        assert!(
            store.write_open_checkpoint().await.expect("write"),
            "the handle that loaded the tombstone replaces it"
        );
    });
    let memory = EventlogRecordedStoreOwner::SqliteMemory {
        prefix: PREFIX.to_owned(),
        authority,
        limits: LIMITS,
    };
    assert_eq!(memory.discard_open_checkpoint(), Ok(false));
}

/// Durable open checkpoints are a SQLite capability: another provider refuses the enable call.
#[test]
fn enabling_durable_open_checkpoints_on_another_provider_is_refused() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("directory");
    let mut facade = RecordedProviderFacade::provision(
        registry(),
        EventlogRecordedStoreProvisioner::File {
            path: directory.path().join("file-store"),
            authority: ProvisionAuthority {
                logical_scope: "file-scope".to_owned(),
                tenant: "file-tenant".to_owned(),
                expected_stream_identity: None,
            },
            limits: LIMITS,
        },
        context("provision"),
        bridge(),
    )
    .expect("file authority provisioned");
    let refused = facade.enable_durable_open_checkpoints(CallWait::Forever);
    assert!(
        matches!(
            &refused,
            Err(SyncReadError::Store(AsyncStoreError::InvalidInput(detail)))
                if detail == "durable open checkpoints need the SQLite provider"
        ),
        "{refused:?}"
    );
    assert_eq!(
        facade.shutdown(ShutdownMode::Drain, CallWait::Forever),
        ShutdownOutcome::Joined { provider: Ok(()) }
    );
}

/// An open of a store nobody enabled is complete, as before: the drain writes nothing because the
/// provider gives the held checkpoint no durable form.
#[test]
fn a_store_nobody_enabled_opens_completely_every_time() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("directory");
    let path = directory.path().join("store.sqlite3");
    block_on(async {
        let authority = provisioned(&path, LIMITS);
        write(&path, &authority, "first", vec![create("a", "alpha")]).await;
        let store = open(&path, &authority, LIMITS, CapturePolicy::ProviderTracked)
            .await
            .expect("tracked open");
        assert!(!store.write_open_checkpoint().await.expect("write call"));
        drop(store);
        let store = open(&path, &authority, LIMITS, CapturePolicy::ProviderTracked)
            .await
            .expect("tracked open");
        assert_eq!(store.open_verification(), OpenVerification::Complete);
        assert_eq!((store.calls().captures, store.calls().model_builds), (1, 1));
        let tenant = TenantId::new(&authority.tenant).expect("tenant");
        assert!(
            attached(&path)
                .await
                .read_feed(&tenant, 0, 16)
                .await
                .expect("feed")
                .events
                .iter()
                .all(|event| event.stream_type != "er.open-checkpoint"),
            "a checkpoint is a snapshot, never an event"
        );
    });
}
