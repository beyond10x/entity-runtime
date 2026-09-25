//! What one handle open and one small batch cost on a small store whose records embed large
//! definitions — the shape an adopter's local metadata store has: a dozen subjects, a handful of
//! entity kinds each declared in tens of kilobytes, and batches of one or two actions.

use super::*;
use entity_executor::{CreateRequest, ExecuteRequest};
use entity_store::{
    Recording,
    asynchronous::{LegacyAnchor, LegacyCompleteness, LegacyOrderDeclaration},
};

const LIMITS: CaptureLimits = CaptureLimits {
    max_events: 4_096,
    max_blobs: 16_384,
    max_projection_rows: 16_384,
    max_payload_bytes: 256 * 1024 * 1024,
};

const KINDS: [&str; 3] = ["alpha", "beta", "gamma"];
const PREFIX: &str = "small_store_cost";

fn context(label: &str) -> EventlogOperationContext {
    EventlogOperationContext {
        subject: "small-store-cost".into(),
        actor: "entity-eventlog-test".into(),
        request_id: format!("request-{label}"),
        trace_id: format!("trace-{label}"),
        causation_id: None,
        causation_depth: 0,
        occurred_at: OffsetDateTime::UNIX_EPOCH,
    }
}

/// Three kinds whose every definition is `fields` long-named optional fields, so each decision
/// record embeds a definition of tens of kilobytes.
fn registry(fields: usize) -> Registry {
    let mut registry = Registry::new();
    for kind in KINDS {
        let mut schema = serde_json::Map::new();
        schema.insert(
            "title".into(),
            json!({ "type": "string", "required": true }),
        );
        for index in 0..fields {
            schema.insert(
                format!("{kind}_field_{index:04}_carrying_a_long_declared_name"),
                json!({ "type": "string", "max_length": 64 }),
            );
        }
        let definition = serde_json::from_value(json!({
            "entity": kind,
            "version": 1,
            "schema": { "fields": schema },
            "lifecycle": { "initial": "open", "states": ["open", "closed"] },
            "operations": {
                "touch": {
                    "transitions": [{ "from": "open", "to": "open" }],
                    "arguments": { "fields": { "note": { "type": "string" } } },
                    "set": { "title": "$args.note" },
                    "emits": []
                }
            }
        }))
        .expect("definition parses");
        registry.register(definition).expect("definition validates");
    }
    registry
}

fn recording(id: &str) -> Recording {
    Recording {
        record_id: id.into(),
        recorded_at: "2026-09-25T00:00:00Z".into(),
        correlation: None,
        causation: None,
        actor: None,
    }
}

fn create(kind: &str, id: &str, record_id: &str) -> BatchAction {
    BatchAction::Create(CreateRequest {
        subject: Subject::new(kind, id).expect("subject"),
        definition_version: 1,
        fields: json!({ "title": id }),
        recording: recording(record_id),
    })
}

fn touch(kind: &str, id: &str, revision: u64, record_id: &str) -> BatchAction {
    BatchAction::Execute(ExecuteRequest {
        subject: Subject::new(kind, id).expect("subject"),
        expected_revision: revision,
        operation: "touch".into(),
        arguments: json!({ "note": record_id }),
        fulfillments: BTreeMap::new(),
        recording: recording(record_id),
    })
}

fn imported(kind: &str, id: &str) -> SubjectHistory {
    SubjectHistory {
        subject: Subject::new(kind, id).expect("subject"),
        origin: HistoryOrigin::Imported(LegacyAnchor {
            instance: EntityInstance {
                entity: kind.into(),
                version: 1,
                id: id.into(),
                lifecycle_state: "open".into(),
                revision: 3,
                fields: serde_json::from_value(json!({ "title": id })).expect("fields"),
            },
            completeness: LegacyCompleteness::AvailableEvidenceOnly,
            order: LegacyOrderDeclaration::PerKindOnly,
            evidence: Vec::new(),
        }),
        records: Vec::new(),
    }
}

async fn sqlite(path: &str, existing: bool) -> Arc<eventlog_sqlite::SqliteEventStore> {
    let concrete = if existing {
        eventlog_sqlite::SqliteEventStore::open_existing(path, PREFIX).await
    } else {
        eventlog_sqlite::SqliteEventStore::open(path, PREFIX).await
    }
    .expect("SQLite provider");
    Arc::new(concrete)
}

async fn reopen(path: &str, authority: &Authority) -> EventlogRecordedStore {
    let concrete = sqlite(path, true).await;
    concrete
        .attach_inline_existing(Arc::new(crate::ErRecordedProjector::new()))
        .await
        .expect("projection attachment");
    let backend: Arc<dyn EventlogBackend> = concrete;
    EventlogRecordedStore::open(backend, authority.clone(), LIMITS)
        .await
        .expect("bound store reopens")
}

/// A provisioned store holding `subjects` subjects — a third imported, the rest created — and a
/// short history of one- and two-action batches over them. Returns the path, the authority and
/// every subject's current revision.
async fn seeded(
    directory: &std::path::Path,
    registry: &Registry,
    subjects: usize,
) -> (String, Authority, Vec<(String, String, u64)>) {
    let path = directory
        .join("eventlog.sqlite3")
        .to_string_lossy()
        .into_owned();
    let concrete = sqlite(&path, false).await;
    let tenant = TenantId::new("small-store-cost").expect("tenant");
    let stream_identity = concrete.stream_identity(&tenant).await.expect("generation");
    let projector = Arc::new(crate::ErRecordedProjector::new());
    concrete
        .create_projections(projector.clone())
        .await
        .expect("projection admission");
    concrete
        .attach_inline_existing(projector)
        .await
        .expect("projection attachment");
    let authority = Authority {
        logical_scope: "small-store-cost-scope".into(),
        tenant: tenant.as_str().into(),
        stream_identity,
    };
    let backend: Arc<dyn EventlogBackend> = concrete;
    EventlogBindingProvisioner::new(backend.clone(), LIMITS)
        .provision_binding(authority.clone(), context("binding"))
        .await
        .expect("binding");
    let store = EventlogRecordedStore::open(backend, authority.clone(), LIMITS)
        .await
        .expect("bound store");
    let mut revisions = Vec::new();
    let imports = subjects / 3;
    for index in 0..imports {
        let kind = KINDS[index % KINDS.len()];
        let id = format!("imported-{index:02}");
        store
            .operation(context(&format!("import-{index}")))
            .import_source_anchor("legacy/small-store".into(), imported(kind, &id))
            .await
            .expect("import");
        revisions.push((kind.to_owned(), id, 3));
    }
    let created: Vec<_> = (imports..subjects)
        .map(|index| (KINDS[index % KINDS.len()], format!("created-{index:02}")))
        .collect();
    store
        .operation(context("seed"))
        .execute_batch(
            registry,
            BatchKey::Named("seed".into()),
            created
                .iter()
                .map(|(kind, id)| create(kind, id, &format!("seed-{id}")))
                .collect(),
        )
        .await
        .expect("seed batch commits");
    revisions.extend(
        created
            .into_iter()
            .map(|(kind, id)| (kind.to_owned(), id, 1)),
    );
    for round in 0..4 {
        run_batch(
            &store,
            registry,
            &mut revisions,
            &format!("history-{round}"),
            round,
        )
        .await;
    }
    (path, authority, revisions)
}

/// One batch of two touches over subjects chosen by `round`.
async fn run_batch(
    store: &EventlogRecordedStore,
    registry: &Registry,
    revisions: &mut [(String, String, u64)],
    label: &str,
    round: usize,
) {
    let count = revisions.len();
    let chosen = [round % count, (round + count / 2) % count];
    let actions = chosen
        .iter()
        .enumerate()
        .map(|(member, index)| {
            let (kind, id, revision) = &revisions[*index];
            touch(kind, id, *revision, &format!("{label}-{member}"))
        })
        .collect();
    store
        .operation(context(label))
        .execute_batch(registry, BatchKey::Named(label.into()), actions)
        .await
        .expect("batch commits");
    for index in chosen {
        revisions[index].2 += 1;
    }
}

/// Typed record decodes and record replays `work` costs on this thread.
///
/// Every provider call a handle makes returns to the thread that awaits it, and the model a read
/// is verified into is built there, so what a handle decodes and replays is charged here.
async fn counted<T>(work: impl std::future::Future<Output = T>) -> (T, u64, u64) {
    crate::encoding::RECORD_DECODES.with(|charged| charged.set(0));
    super::REPLAYED_RECORDS.with(|charged| charged.set(0));
    let value = work.await;
    (
        value,
        crate::encoding::RECORD_DECODES.with(std::cell::Cell::get),
        super::REPLAYED_RECORDS.with(std::cell::Cell::get),
    )
}

/// Records `store` holds, read through a complete snapshot.
async fn stored_records(store: &EventlogRecordedStore) -> usize {
    store
        .complete_snapshot("small-store-cost-scope")
        .await
        .expect("snapshot")
        .histories
        .iter()
        .map(|subject| subject.history.records.len())
        .sum()
}

/// A batch on a handle that has already verified the store costs the records that batch adds,
/// not the store. Each of its per-entity reads — the preflight, the append's read and the
/// post-commit check — used to decode and replay every record of every subject it reached, and a
/// shared seed batch reaches every subject created in it, so a two-action batch on a sixteen-
/// record store decoded some fifty records. What a handle has verified, byte for byte, it does
/// not verify again. The two records the batch commits are replayed once each and decoded twice:
/// once from their own record blobs, and once as members of the group blob the post-commit read
/// discovers their subjects through, which that read meets before it has verified it.
#[tokio::test]
async fn a_warm_handle_decodes_and_replays_only_the_records_a_batch_adds() {
    let registry = registry(8);
    let directory = tempfile::tempdir().expect("store directory");
    let (path, authority, mut revisions) = seeded(directory.path(), &registry, 12).await;
    let store = reopen(&path, &authority).await;
    run_batch(&store, &registry, &mut revisions, "warm", 1).await;
    for round in 0..3 {
        let before = store.calls();
        let ((), decoded, replayed) = counted(run_batch(
            &store,
            &registry,
            &mut revisions,
            &format!("counted-{round}"),
            round + 2,
        ))
        .await;
        assert_eq!(
            (decoded, replayed),
            (4, 2),
            "batch {round}: a two-action batch decoded {decoded} and replayed {replayed} records \
             on a handle that had verified everything else; calls {before:?} -> {:?}",
            store.calls()
        );
    }
}

/// A cold open verifies every record, and decodes each exactly once.
///
/// A named batch's blob holds every member record again. It is held to the member records the
/// open has just decoded — byte for byte, which is what the member comparison established — rather
/// than decoded a second time.
#[tokio::test]
async fn a_cold_open_decodes_and_replays_each_record_once() {
    let registry = registry(8);
    let directory = tempfile::tempdir().expect("store directory");
    let (path, authority, _) = seeded(directory.path(), &registry, 12).await;
    let (store, decoded, replayed) = counted(reopen(&path, &authority)).await;
    let records = stored_records(&store).await as u64;
    assert_eq!(records, 16, "the seed and four two-action batches");
    assert_eq!(
        (decoded, replayed),
        (records, records),
        "a cold open of {records} records decoded {decoded} and replayed {replayed}"
    );
}

fn measured_number(name: &str, default: usize) -> usize {
    std::env::var(name).map_or(default, |value| value.parse().expect("a count"))
}

/// The timing for the small-store unit. Prints; asserts nothing about time, which is noisy on a
/// shared machine. Asks for nothing unless `ENTITY_EVENTLOG_SMALL_STORE` is set.
#[tokio::test]
async fn small_store_open_and_batch_measurement() {
    if std::env::var("ENTITY_EVENTLOG_SMALL_STORE").is_err() {
        return;
    }
    let fields = measured_number("ENTITY_EVENTLOG_SMALL_STORE_FIELDS", 250);
    let subjects = measured_number("ENTITY_EVENTLOG_SMALL_STORE_SUBJECTS", 12);
    let rounds = measured_number("ENTITY_EVENTLOG_SMALL_STORE_ROUNDS", 5);
    let open_count = measured_number("ENTITY_EVENTLOG_SMALL_STORE_OPENS", rounds);
    let registry = registry(fields);
    let directory = tempfile::tempdir().expect("store directory");
    let (path, authority, mut revisions) = seeded(directory.path(), &registry, subjects).await;
    let mut opens = Vec::new();
    let mut store = None;
    for _ in 0..open_count.max(1) {
        let start = std::time::Instant::now();
        let opened = reopen(&path, &authority).await;
        opens.push(start.elapsed());
        store = Some(opened);
    }
    let store = store.expect("opened");
    let snapshot = store
        .complete_snapshot("small-store-cost-scope")
        .await
        .expect("snapshot");
    let record = snapshot
        .histories
        .iter()
        .flat_map(|subject| subject.history.records.iter())
        .next()
        .map(|record| record.record_bytes.len());
    println!(
        "store: {} subjects, {fields} fields per definition, record blob {record:?} bytes",
        snapshot.histories.len()
    );
    let mut batches = Vec::new();
    for round in 0..rounds {
        let before = store.calls();
        let start = std::time::Instant::now();
        run_batch(
            &store,
            &registry,
            &mut revisions,
            &format!("measured-{round}"),
            round + 7,
        )
        .await;
        let took = start.elapsed();
        batches.push(took);
        println!(
            "batch {round}: {took:?}; calls {before:?} -> {:?}",
            store.calls()
        );
    }
    opens.sort();
    batches.sort();
    println!(
        "open median {:?} (all {opens:?}); two-action batch median {:?} (all {batches:?})",
        opens[opens.len() / 2],
        batches[batches.len() / 2]
    );
}

/// What a build answers, to the field a read exposes: the verified histories, terminals and forks,
/// or the refusal exactly as it is worded.
fn adversary_answer(result: Result<CapturedModel, AsyncStoreError>) -> String {
    match result {
        Ok(model) => format!(
            "Ok {:?} {:?} {:?}",
            model.histories, model.terminals, model.forked
        ),
        Err(error) => format!("Err {error:?}"),
    }
}

/// Every structural rewrite of `capture` that keeps every digest consistent: an event dropped, an
/// event's subject position moved, two events' store positions swapped, and two records of one
/// stream exchanged under each other's positions.
fn adversary_rewrites(capture: &TenantCapture) -> Vec<(String, TenantCapture)> {
    let mut rewrites = Vec::new();
    let events = capture.events.len();
    for index in 1..events {
        let mut dropped = capture.clone();
        dropped.events.remove(index);
        rewrites.push((format!("event {index} dropped"), dropped));
        for delta in [-1i64, 1] {
            let mut moved = capture.clone();
            let version = moved.events[index].version as i64 + delta;
            if version < 1 {
                continue;
            }
            moved.events[index].version = version as u64;
            rewrites.push((format!("event {index} version {delta:+}"), moved));
        }
        for other in index + 1..events {
            let mut swapped = capture.clone();
            let (left, right) = (
                swapped.events[index].global_seq,
                swapped.events[other].global_seq,
            );
            swapped.events[index].global_seq = right;
            swapped.events[other].global_seq = left;
            swapped.events.sort_by_key(|event| event.global_seq);
            rewrites.push((format!("events {index} and {other} store-swapped"), swapped));
            if capture.events[index].stream_id == capture.events[other].stream_id {
                let mut exchanged = capture.clone();
                let data = exchanged.events[index].data.clone();
                exchanged.events[index].data = exchanged.events[other].data.clone();
                exchanged.events[other].data = data;
                rewrites.push((format!("records {index} and {other} exchanged"), exchanged));
            }
        }
    }
    rewrites
}

/// Adversary, attack 1 and 5: a handle that verified the store, and then a rival handle's honest
/// appends, answers every consistent rewrite of the resulting observation — below its remembered
/// prefix and inside the unremembered suffix alike — exactly as a cold build of the same
/// observation answers it: the same model, or the same refusal in the same words.
#[tokio::test]
async fn adversary_a_warm_handle_answers_every_rewritten_capture_as_a_cold_build() {
    let registry = registry(2);
    let directory = tempfile::tempdir().expect("store directory");
    let (path, authority, mut revisions) = seeded(directory.path(), &registry, 6).await;
    let warm = reopen(&path, &authority).await;
    let rival = reopen(&path, &authority).await;
    run_batch(&rival, &registry, &mut revisions, "rival-0", 0).await;
    run_batch(&rival, &registry, &mut revisions, "rival-1", 0).await;
    let capture = warm.capture().await.expect("capture");
    let rewrites = adversary_rewrites(&capture);
    assert!(rewrites.len() > 100, "{} rewrites", rewrites.len());
    let mut differed = Vec::new();
    let (mut replayed_cold, mut replayed_warm, mut refused) = (0u64, 0u64, 0usize);
    for (what, rewritten) in
        std::iter::once(("the honest capture".to_owned(), capture.clone())).chain(rewrites)
    {
        super::REPLAYED_RECORDS.with(|charged| charged.set(0));
        let cold = adversary_answer(build_model(&authority, &rewritten));
        replayed_cold += super::REPLAYED_RECORDS.with(std::cell::Cell::get);
        super::REPLAYED_RECORDS.with(|charged| charged.set(0));
        let hot = {
            // One memory across every rewrite: what one rewritten build remembers is in the
            // memory the next is answered from, as it would be on a handle read repeatedly.
            let mut memory = warm.memory.lock().expect("memory");
            adversary_answer(build_model_remembering(
                &authority,
                &rewritten,
                Some(&mut memory),
            ))
        };
        replayed_warm += super::REPLAYED_RECORDS.with(std::cell::Cell::get);
        refused += usize::from(cold.starts_with("Err"));
        if cold != hot {
            differed.push(format!("{what}:\n  cold {cold:.300}\n  warm {hot:.300}"));
        }
    }
    eprintln!(
        "parity probe: replayed cold {replayed_cold} warm {replayed_warm}, refused {refused}"
    );
    // The probe is only a probe of the memory if the memory answered: the warm builds must have
    // replayed less than the cold ones, and the rewrites must have been refused somewhere.
    assert!(
        replayed_warm < replayed_cold && refused > 0,
        "the memory was not exercised: replayed cold {replayed_cold} warm {replayed_warm}, \
         refused {refused}"
    );
    assert!(
        differed.is_empty(),
        "{} rewrites answered differently warm:\n{}",
        differed.len(),
        differed.join("\n")
    );
}

/// JSON with every object's keys in bytewise order, written without `serde_json::Map`, so that
/// what it writes does not depend on which map backend the build unified.
fn adversary_sorted_json(value: &Value, out: &mut Vec<u8>) {
    match value {
        Value::Object(object) => {
            let mut entries: Vec<(&String, &Value)> = object.iter().collect();
            entries.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
            out.push(b'{');
            for (index, (key, value)) in entries.into_iter().enumerate() {
                if index > 0 {
                    out.push(b',');
                }
                out.extend(serde_json::to_vec(key).expect("key"));
                out.push(b':');
                adversary_sorted_json(value, out);
            }
            out.push(b'}');
        }
        Value::Array(values) => {
            out.push(b'[');
            for (index, value) in values.iter().enumerate() {
                if index > 0 {
                    out.push(b',');
                }
                adversary_sorted_json(value, out);
            }
            out.push(b']');
        }
        scalar => out.extend(serde_json::to_vec(scalar).expect("scalar")),
    }
}

/// Adversary, attack 4: every blob a writer binds — binding, records, requests, wrappers, group
/// blobs, import anchors — is sorted-key compact JSON, whichever map backend the build unified.
/// Run it with `--features serde_json/preserve_order` as well as without: base wrote sorted keys
/// in both, and the reordering pass is now skipped in one of them.
#[tokio::test]
async fn adversary_every_bound_blob_is_sorted_key_json_under_either_map_backend() {
    let registry = registry(4);
    let directory = tempfile::tempdir().expect("store directory");
    let (path, authority, _) = seeded(directory.path(), &registry, 6).await;
    let store = reopen(&path, &authority).await;
    let capture = store.capture().await.expect("capture");
    assert!(capture.blobs.len() > 20, "{} blobs", capture.blobs.len());
    for blob in &capture.blobs {
        let value: Value = serde_json::from_slice(&blob.bytes).expect("a JSON blob");
        let mut sorted = Vec::new();
        adversary_sorted_json(&value, &mut sorted);
        assert_eq!(
            String::from_utf8_lossy(&sorted),
            String::from_utf8_lossy(&blob.bytes),
            "blob {} is not sorted-key JSON",
            blob.digest
        );
    }
}
