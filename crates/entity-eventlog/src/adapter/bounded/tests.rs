//! The suffix verifier without a whole model, and the open checkpoint's write rules
//! (`docs/design/recorded-open-checkpoint-v0.1.md` § *When it is written*, *What invalidates it*).
use super::*;
use crate::adapter::checkpoint::{self as persisted, Loaded};
use entity_executor::{CreateRequest, ExecuteRequest};
use entity_store::Recording;
use eventlog_core::{CaptureLimits, CapturedProjectionDelta, TenantCapture};

const LIMITS: CaptureLimits = CaptureLimits {
    max_events: 256,
    max_blobs: 2048,
    max_projection_rows: 2048,
    max_payload_bytes: 16 * 1024 * 1024,
};
const PREFIX: &str = "bounded_unit";

fn context(id: &str) -> EventlogOperationContext {
    EventlogOperationContext {
        subject: "bounded-unit".into(),
        actor: "entity-eventlog-test".into(),
        request_id: id.into(),
        trace_id: id.into(),
        causation_id: None,
        causation_depth: 0,
        occurred_at: OffsetDateTime::UNIX_EPOCH,
    }
}

fn recording(id: &str) -> Recording {
    Recording {
        record_id: id.into(),
        recorded_at: "2026-10-07T00:00:00Z".into(),
        correlation: None,
        causation: None,
        actor: None,
    }
}

fn registry() -> Registry {
    let mut registry = Registry::new();
    registry
        .register(
            serde_json::from_value(json!({
                "entity":"ticket","version":1,
                "schema":{"fields":{"title":{"type":"string","required":true}}},
                "lifecycle":{"initial":"Open","states":["Open","Closed"]},
                "operations":{
                    "Touch":{"transitions":[{"from":"Open","to":"Open"}],"emits":[]},
                    "Close":{"transitions":[{"from":"Open","to":"Closed"}],"emits":[]}
                }
            }))
            .expect("definition parses"),
        )
        .expect("definition validates");
    registry
}

fn create(id: &str) -> BatchAction {
    BatchAction::Create(CreateRequest {
        subject: Subject::new("ticket", id).expect("subject"),
        definition_version: 1,
        fields: json!({"title": id}),
        recording: recording(&format!("create-{id}")),
    })
}

fn touch(id: &str, revision: u64, record: &str) -> BatchAction {
    BatchAction::Execute(ExecuteRequest {
        subject: Subject::new("ticket", id).expect("subject"),
        expected_revision: revision,
        operation: "Touch".into(),
        arguments: json!({}),
        fulfillments: BTreeMap::new(),
        recording: recording(record),
    })
}

fn block_on<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime")
        .block_on(future)
}

/// A file-backed SQLite store with durable continuity enabled, kept inside the build directory.
struct Store {
    _directory: tempfile::TempDir,
    path: String,
    authority: Authority,
}

impl Store {
    async fn new() -> Self {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
        std::fs::create_dir_all(&root).expect("build directory");
        let directory = tempfile::tempdir_in(root).expect("directory");
        let path = directory
            .path()
            .join("store.sqlite3")
            .to_string_lossy()
            .into_owned();
        let backend = Arc::new(
            eventlog_sqlite::SqliteEventStore::open(&path, PREFIX)
                .await
                .expect("store"),
        );
        let tenant = TenantId::new("bounded-unit").expect("tenant");
        let authority = Authority {
            logical_scope: "bounded-scope".into(),
            tenant: tenant.as_str().into(),
            stream_identity: backend.stream_identity(&tenant).await.expect("identity"),
        };
        let projector = Arc::new(crate::ErRecordedProjector::new());
        backend
            .create_projections(projector.clone())
            .await
            .expect("projections");
        backend
            .attach_inline_existing(projector)
            .await
            .expect("attach");
        EventlogBindingProvisioner::new(backend.clone(), LIMITS)
            .provision_binding(authority.clone(), context("binding"))
            .await
            .expect("binding");
        backend
            .enable_durable_continuity()
            .await
            .expect("durable continuity");
        Self {
            _directory: directory,
            path,
            authority,
        }
    }

    async fn backend(&self) -> Arc<eventlog_sqlite::SqliteEventStore> {
        let backend = Arc::new(
            eventlog_sqlite::SqliteEventStore::open_existing(&self.path, PREFIX)
                .await
                .expect("store opens"),
        );
        backend
            .attach_inline_existing(Arc::new(crate::ErRecordedProjector::new()))
            .await
            .expect("attach");
        backend
    }

    async fn open(&self, policy: CapturePolicy) -> Result<EventlogRecordedStore, AsyncStoreError> {
        EventlogRecordedStore::open_with_policy(
            self.backend().await,
            self.authority.clone(),
            LIMITS,
            policy,
        )
        .await
    }

    async fn tracked(&self) -> EventlogRecordedStore {
        self.open(CapturePolicy::ProviderTracked)
            .await
            .expect("tracked open")
    }

    async fn persisted(&self) -> Loaded {
        let tenant = TenantId::new(&self.authority.tenant).expect("tenant");
        persisted::load(
            self.backend().await.as_ref(),
            &tenant,
            &self.authority,
            LIMITS,
        )
        .await
        .expect("checkpoint read")
    }

    async fn persist(&self, state: Value) {
        let tenant = TenantId::new(&self.authority.tenant).expect("tenant");
        assert!(
            persisted::save(
                self.backend().await.as_ref(),
                &tenant,
                &self.authority,
                state
            )
            .await
            .expect("checkpoint write"),
            "the snapshot generation admits the write"
        );
    }

    async fn capture(&self) -> TenantCapture {
        let tenant = TenantId::new(&self.authority.tenant).expect("tenant");
        self.backend()
            .await
            .capture_tenant(&tenant, projection_specs(), LIMITS)
            .await
            .expect("capture")
    }
}

async fn run(store: &EventlogRecordedStore, key: &str, actions: Vec<BatchAction>) {
    store
        .operation(context(key))
        .execute_batch(&registry(), BatchKey::Named(key.into()), actions)
        .await
        .expect("batch commits");
}

fn usage(capture: &TenantCapture) -> CaptureUsage {
    CaptureUsage {
        events: capture.events.len() as u64,
        blobs: capture.blobs.len() as u64,
        projection_rows: projection_rows(capture),
        payload_bytes: 0,
    }
}

/// The delta a provider reports between two complete captures.
fn difference(before: &TenantCapture, after: &TenantCapture) -> TenantCaptureDelta {
    let old: BTreeSet<_> = before.blobs.iter().map(|b| b.digest.as_str()).collect();
    TenantCaptureDelta {
        tenant: after.tenant.clone(),
        stream_identity: after.stream_identity.clone(),
        events: after.events[before.events.len()..].to_vec(),
        blobs: after
            .blobs
            .iter()
            .filter(|b| !old.contains(b.digest.as_str()))
            .cloned()
            .collect(),
        projections: before
            .projections
            .iter()
            .zip(&after.projections)
            .map(|(before, after)| {
                let old: BTreeMap<_, _> = before.rows.iter().cloned().collect();
                let new: BTreeMap<_, _> = after.rows.iter().cloned().collect();
                let keys: BTreeSet<_> = old.keys().chain(new.keys()).cloned().collect();
                CapturedProjectionDelta {
                    specification: after.specification,
                    rows: keys
                        .into_iter()
                        .filter(|k| old.get(k) != new.get(k))
                        .map(|key| CapturedRowChange {
                            before: old.get(&key).cloned(),
                            after: new.get(&key).cloned(),
                            key,
                        })
                        .collect(),
                }
            })
            .collect(),
        resulting_usage: usage(after),
    }
}

/// A forged suffix is refused as integrity, as corrupt history, or, where the forgery is a prior
/// state, as the conflict the record's expectation has with the state the forged row names.
/// One forged coordinate of a provider suffix, by what it forges.
type Forgery = (&'static str, Box<dyn Fn(&mut TenantCaptureDelta)>);

fn refused<T>(result: &Result<T, AsyncStoreError>) -> bool {
    matches!(
        result,
        Err(AsyncStoreError::ProviderIntegrity { .. }
            | AsyncStoreError::CorruptHistory { .. }
            | AsyncStoreError::RevisionConflict { .. })
    )
}

/// The suffix verifier accepts exactly the suffix a whole build accepts, and refuses every
/// coordinate a provider could get wrong: a row omitted, added, forged or rebinding an identity,
/// a prior state the subject never had, a position that does not advance, a usage that does not
/// add up, an event it cannot verify, and blob bytes that are not their digest.
#[test]
fn a_suffix_verified_from_rows_is_accepted_whole_and_refused_for_every_forged_coordinate() {
    block_on(async {
        let fixture = Store::new().await;
        let store = fixture.tracked().await;
        run(&store, "first", vec![create("a"), create("b")]).await;
        let before = fixture.capture().await;
        run(
            &store,
            "second",
            vec![touch("a", 1, "touch-a"), create("c")],
        )
        .await;
        run(&store, "third", vec![touch("a", 2, "touch-a-again")]).await;
        let after = fixture.capture().await;
        build_model(&fixture.authority, &after).expect("a whole build accepts the store");
        let from = before.events.last().expect("events").global_seq;
        let suffix = store
            .verify_suffix(usage(&before), from, &difference(&before, &after))
            .await
            .expect("the suffix verifies");
        assert_eq!(suffix.events, 3);
        assert_eq!(suffix.usage, usage(&after));
        assert_eq!(
            suffix.last_position,
            after.events.last().expect("events").global_seq
        );
        let forgeries: Vec<Forgery> = vec![
            (
                "a record row omitted",
                Box::new(|d| drop(d.projections[1].rows.pop())),
            ),
            (
                "a batch row omitted",
                Box::new(|d| drop(d.projections[2].rows.pop())),
            ),
            (
                "a subject row omitted",
                Box::new(|d| drop(d.projections[3].rows.pop())),
            ),
            (
                "a subject's prior row withheld",
                Box::new(|d| {
                    for change in &mut d.projections[3].rows {
                        change.before = None;
                    }
                }),
            ),
            (
                "a subject row forged",
                Box::new(|d| d.projections[3].rows[0].after = Some(json!(["forged"]))),
            ),
            (
                "a record id already used",
                Box::new(|d| d.projections[1].rows[0].before = Some(json!(["used"]))),
            ),
            (
                "a row the events do not explain",
                Box::new(|d| {
                    d.projections[1].rows.push(CapturedRowChange {
                        key: "unexplained".into(),
                        before: None,
                        after: Some(json!(["unexplained"])),
                    })
                }),
            ),
            (
                "the binding row changed",
                Box::new(|d| {
                    d.projections[0].rows.push(CapturedRowChange {
                        key: "singleton".into(),
                        before: None,
                        after: Some(json!(["binding"])),
                    })
                }),
            ),
            (
                "a position that does not advance",
                Box::new(move |d| d.events[0].global_seq = from),
            ),
            (
                "usage that does not add up",
                Box::new(|d| d.resulting_usage.events += 1),
            ),
            (
                "an event the rows cannot verify",
                Box::new(|d| d.events[0].name = "er.refused_request".into()),
            ),
            (
                "blob bytes that are not their digest",
                Box::new(|d| d.blobs[0].bytes.push(b' ')),
            ),
        ];
        for (forgery, forge) in forgeries {
            let mut delta = difference(&before, &after);
            forge(&mut delta);
            let result = store.verify_suffix(usage(&before), from, &delta).await;
            assert!(
                refused(&result),
                "{forgery}: {:?}",
                result.map(|s| s.events)
            );
        }
    });
}

/// A forged prior state: the subject row the provider journaled before the suffix must name the
/// state the suffix's records continue from.
#[test]
fn a_suffix_continuing_another_state_than_its_subject_row_names_is_refused() {
    block_on(async {
        let fixture = Store::new().await;
        let store = fixture.tracked().await;
        run(&store, "first", vec![create("a"), create("b")]).await;
        let before = fixture.capture().await;
        run(&store, "second", vec![touch("a", 1, "touch-a")]).await;
        let after = fixture.capture().await;
        let mut delta = difference(&before, &after);
        let rows: BTreeMap<_, _> = before.projections[3].rows.iter().cloned().collect();
        let other =
            subject_key(&fixture.authority, &Subject::new("ticket", "b").expect("b")).expect("key");
        // The row of another subject, at the same revision, offered as a's prior row.
        delta.projections[3].rows[0].before = Some(rows[&other].clone());
        let result = store
            .verify_suffix(
                usage(&before),
                before.events.last().expect("events").global_seq,
                &delta,
            )
            .await;
        assert!(
            matches!(
                &result,
                Err(AsyncStoreError::ProviderIntegrity { detail, .. })
                    if detail == "a subject row names another authority or subject"
            ),
            "{:?}",
            result.map(|s| s.events)
        );
    });
}

/// Design rule: "a read-only drain after a `verifier` change writes a valid checkpoint at the
/// unchanged head, and the next open is bounded".
#[test]
fn a_read_only_drain_after_a_verifier_change_writes_a_valid_checkpoint_at_the_unchanged_head() {
    block_on(async {
        let fixture = Store::new().await;
        let first = fixture.tracked().await;
        run(&first, "first", vec![create("a")]).await;
        assert!(first.write_open_checkpoint().await.expect("write"));
        let Loaded::Valid(record) = fixture.persisted().await else {
            panic!("a valid checkpoint was written");
        };
        drop(first);
        let older = persisted::Record {
            verifier: "0.0.0-older".into(),
            ..record.clone()
        };
        fixture.persist(older.state().expect("seals")).await;
        let reader = fixture.tracked().await;
        assert_eq!(
            reader.open_verification(),
            OpenVerification::Complete,
            "another verifier's checkpoint is not applied"
        );
        assert!(reader.write_open_checkpoint().await.expect("write"));
        assert_eq!(fixture.persisted().await, Loaded::Valid(record.clone()));
        drop(reader);
        assert_eq!(
            fixture.tracked().await.open_verification(),
            OpenVerification::Checkpoint
        );
    });
}

/// Design rule: "a handle whose held position is below the persisted valid one writes nothing".
#[test]
fn a_handle_held_below_the_persisted_checkpoint_writes_nothing() {
    block_on(async {
        let fixture = Store::new().await;
        let stale = fixture.tracked().await;
        let newer = fixture.tracked().await;
        run(&newer, "first", vec![create("a")]).await;
        assert!(newer.write_open_checkpoint().await.expect("write"));
        let persisted = fixture.persisted().await;
        assert!(
            !stale.write_open_checkpoint().await.expect("write"),
            "a handle at the older position leaves the newer checkpoint"
        );
        assert_eq!(fixture.persisted().await, persisted);
    });
}

/// Design rule: "a drain over a damaged record above the head writes".
#[test]
fn a_drain_over_a_damaged_record_above_the_head_writes() {
    block_on(async {
        let fixture = Store::new().await;
        let first = fixture.tracked().await;
        run(&first, "first", vec![create("a")]).await;
        assert!(first.write_open_checkpoint().await.expect("write"));
        let Loaded::Valid(record) = fixture.persisted().await else {
            panic!("a valid checkpoint was written");
        };
        drop(first);
        let mut damaged = record.state().expect("seals");
        damaged["position"] = json!(record.position + 1_000);
        fixture.persist(damaged).await;
        assert_eq!(fixture.persisted().await, Loaded::Damaged);
        let handle = fixture.tracked().await;
        assert_eq!(handle.open_verification(), OpenVerification::Complete);
        assert!(handle.write_open_checkpoint().await.expect("write"));
        assert_eq!(fixture.persisted().await, Loaded::Valid(record));
    });
}

/// Design rule: "a handle opened before a discard leaves the tombstone in place at its drain",
/// and the handle that loaded the tombstone replaces it.
#[test]
fn a_handle_opened_before_a_discard_leaves_the_tombstone_in_place() {
    block_on(async {
        let fixture = Store::new().await;
        let first = fixture.tracked().await;
        run(&first, "first", vec![create("a")]).await;
        assert!(first.write_open_checkpoint().await.expect("write"));
        drop(first);
        let live = fixture.tracked().await;
        assert_eq!(live.open_verification(), OpenVerification::Checkpoint);
        run(&live, "second", vec![create("b")]).await;
        assert!(
            EventlogRecordedStore::discard_open_checkpoint(
                fixture.backend().await.as_ref(),
                &fixture.authority
            )
            .await
            .expect("discard")
        );
        let tombstone = fixture.persisted().await;
        assert!(matches!(tombstone, Loaded::Tombstone(_)), "{tombstone:?}");
        assert!(
            !live.write_open_checkpoint().await.expect("write"),
            "a handle that loaded a checkpoint never writes over a tombstone"
        );
        assert_eq!(fixture.persisted().await, tombstone);
        let after = fixture.tracked().await;
        assert_eq!(after.open_verification(), OpenVerification::Complete);
        assert!(after.write_open_checkpoint().await.expect("write"));
        assert!(matches!(fixture.persisted().await, Loaded::Valid(_)));
        assert!(
            EventlogRecordedStore::discard_open_checkpoint(
                fixture.backend().await.as_ref(),
                &fixture.authority
            )
            .await
            .expect("discard")
        );
        let second = fixture.persisted().await;
        assert!(matches!(second, Loaded::Tombstone(_)), "{second:?}");
        assert_ne!(
            second, tombstone,
            "a tombstone names what it replaced, so two discards never write equal ones"
        );
    });
}

/// "A checkpoint ahead of the provider's head refuses `ProviderTracked` opens with
/// `ProviderIntegrity` until an explicit discard call; `FullVerification` opens are unaffected."
/// The record is valid but its provider bytes restore to nothing, so the open verifies completely
/// and only the position comparison can refuse it.
#[test]
fn a_checkpoint_beyond_the_head_refuses_tracked_opens_until_discarded() {
    block_on(async {
        let fixture = Store::new().await;
        let first = fixture.tracked().await;
        run(&first, "first", vec![create("a")]).await;
        assert!(first.write_open_checkpoint().await.expect("write"));
        let Loaded::Valid(record) = fixture.persisted().await else {
            panic!("a valid checkpoint was written");
        };
        drop(first);
        let ahead = persisted::Record {
            provider: b"not the provider's bytes".to_vec(),
            position: record.position + 2,
            ..record
        };
        fixture.persist(ahead.state().expect("seals")).await;
        let refused = fixture.open(CapturePolicy::ProviderTracked).await;
        let head = record.position;
        let position = head + 2;
        assert!(
            matches!(
                &refused,
                Err(AsyncStoreError::ProviderIntegrity { detail, .. })
                    if detail.starts_with(&format!(
                        "the open checkpoint names tenant position {position}, beyond the provider's head {head}"
                    ))
            ),
            "a checkpoint beyond the head was opened instead of refused: {:?}",
            refused.map(|store| store.open_verification())
        );
        fixture
            .open(CapturePolicy::FullVerification)
            .await
            .expect("FullVerification never reads the checkpoint");
        assert!(
            EventlogRecordedStore::discard_open_checkpoint(
                fixture.backend().await.as_ref(),
                &fixture.authority
            )
            .await
            .expect("discard")
        );
        assert_eq!(
            fixture.tracked().await.open_verification(),
            OpenVerification::Complete
        );
    });
}

/// "`authority` ... and the binding `PhysicalRef`: a checkpoint for another generation is never
/// applied." A record whose digest holds but which names another binding event is not started
/// from, and its position does not refuse the open.
#[test]
fn a_checkpoint_naming_another_binding_event_is_not_applied() {
    block_on(async {
        let fixture = Store::new().await;
        let first = fixture.tracked().await;
        run(&first, "first", vec![create("a")]).await;
        assert!(first.write_open_checkpoint().await.expect("write"));
        let Loaded::Valid(record) = fixture.persisted().await else {
            panic!("a valid checkpoint was written");
        };
        drop(first);
        let foreign = persisted::Record {
            binding: PhysicalRef {
                event_id: "another-binding".into(),
                ..record.binding.clone()
            },
            position: record.position + 5,
            ..record
        };
        fixture.persist(foreign.state().expect("seals")).await;
        let handle = fixture.tracked().await;
        assert_eq!(
            handle.open_verification(),
            OpenVerification::Complete,
            "a checkpoint naming another binding event was started from"
        );
        assert_eq!(handle.calls().model_builds, 1);
    });
}

/// "The handle has not refused with `ProviderIntegrity` since its last verification": a handle
/// whose verification refused persists nothing until a complete verification succeeds.
#[test]
fn a_handle_that_refused_since_its_last_complete_verification_persists_nothing() {
    block_on(async {
        let fixture = Store::new().await;
        let handle = fixture.tracked().await;
        run(&handle, "first", vec![create("a")]).await;
        handle.note_refusal(&integrity("a verification refused"));
        assert!(
            !handle.write_open_checkpoint().await.expect("write"),
            "a handle that refused writes nothing"
        );
        assert_eq!(fixture.persisted().await, Loaded::Absent);
        handle.reverify().await.expect("a complete verification");
        assert!(
            handle.write_open_checkpoint().await.expect("write"),
            "a complete verification after the refusal admits the write again"
        );
    });
}

/// The floor: history is append-only, so a complete capture behind a position the handle already
/// verified is a store that lost its tail under it.
#[test]
fn a_complete_capture_behind_the_verified_position_is_refused() {
    block_on(async {
        let fixture = Store::new().await;
        let handle = fixture.tracked().await;
        let capture = fixture.capture().await;
        let head = capture.events.last().expect("binding event").global_seq;
        let result = handle.verify_complete(capture, None, head + 1);
        assert!(
            matches!(
                &result,
                Err(AsyncStoreError::ProviderIntegrity { detail, .. })
                    if detail == &format!(
                        "the provider's head {head} is behind position {} this handle already verified",
                        head + 1
                    )
            ),
            "{:?}",
            result.map(|held| held.model.decoded)
        );
    });
}
