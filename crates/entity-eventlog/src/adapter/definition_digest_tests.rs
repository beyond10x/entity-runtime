//! Issue 94: a committed record names its entity definition by digest instead of embedding it.
//!
//! A record written since 0.33.0 is framed `er.record/5` and carries `"definition":"sha256:…"`;
//! the definition is stored once per tenant as a content-addressed blob in the same append group
//! as the first record that names it. A store written before keeps its bytes: its records are
//! read and compared with their definitions embedded, so it opens, verifies and serves without a
//! rewrite, and a retry of one of its batches is still the same batch.
//!
//! In this module rather than in `tests/`: the refusals are produced by editing a capture and
//! building its model, which only this crate can do, and the crate's test binaries are not to grow
//! by one.

use std::{path::Path, sync::Arc};

use crate::{
    AsyncBindingProvisioner, Authority, CapturePolicy, ErRecordedProjector,
    EventlogBindingProvisioner, EventlogOperationContext, EventlogRecordedStore, projection_specs,
};
use entity_core::Registry;
use entity_executor::{BatchAction, CreateRequest, ExecuteRequest, Executor};
use entity_store::{
    Recording,
    asynchronous::{
        AsyncRecordedReader, AsyncStateReader, AsyncStoreError, BatchKey, RECORD_BY_DIGEST,
        RecordLookup, Subject, record_framing, verify_complete_store,
    },
};
use eventlog_core::{
    CaptureLimits, ConsistentTenantCapture, EventStore, InlineProjectionAdmin, TenantCapture,
    TenantId,
};
use serde_json::{Value, json};
use time::OffsetDateTime;

use super::{WRITES_EMBED_DEFINITIONS, build_model};
use crate::encoding::DEFINITION_BLOB_DOMAIN;

const PREFIX: &str = "definition_digest";
const TENANT: &str = "definition-tenant";
const WIDE: CaptureLimits = CaptureLimits {
    max_events: 512,
    max_blobs: 4_096,
    max_projection_rows: 4_096,
    max_payload_bytes: 64 * 1024 * 1024,
};

/// A `service/1` definition about the size of the ones a consumer stores: an invocation entity
/// with thirty optional text fields, two states and three operations.
fn definition() -> Value {
    let mut fields = serde_json::Map::new();
    fields.insert(
        "invocation_id".to_owned(),
        json!({ "type": "string", "required": true }),
    );
    fields.insert(
        "status".to_owned(),
        json!({ "type": "string", "required": false }),
    );
    for index in 0..30 {
        fields.insert(
            format!("attribute_{index:02}"),
            json!({ "type": "string", "required": false, "max_length": 512 }),
        );
    }
    json!({
        "entity": "invocation",
        "version": 1,
        "semantics": "service/1",
        "identity": { "field": "invocation_id" },
        "schema": { "fields": fields },
        "lifecycle": { "initial": "pending", "states": ["pending", "settled"] },
        "create": { "emit": { "type": "Invoked", "payload": { "id": "$fields.invocation_id" } } },
        "operations": {
            "settle": {
                "arguments": { "fields": { "status": { "type": "string", "required": true } } },
                "transitions": [{ "from": "pending", "to": "settled" }],
                "set": { "status": "$args.status" },
                "emits": [{ "type": "Settled", "payload": { "status": "$args.status" } }]
            },
            "annotate": {
                "arguments": { "fields": { "note": { "type": "string", "required": true } } },
                "transitions": [{ "from": "settled", "to": "settled" }],
                "set": { "attribute_00": "$args.note" },
                "emits": []
            },
            "retry": {
                "transitions": [{ "from": "pending", "to": "pending" }],
                "emits": []
            }
        }
    })
}

fn registry() -> Registry {
    let mut registry = Registry::new();
    registry
        .register(serde_json::from_value(definition()).expect("definition parses"))
        .expect("definition validates");
    registry
}

fn context(label: &str) -> EventlogOperationContext {
    EventlogOperationContext {
        subject: "definition-digest".to_owned(),
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
        recorded_at: "2026-11-23T00:00:00Z".to_owned(),
        correlation: None,
        causation: None,
        actor: None,
    }
}

fn block_on<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime")
        .block_on(future)
}

fn invocation(id: &str) -> Subject {
    Subject::new("invocation", format!("s:{id}")).expect("subject")
}

fn create(id: &str) -> BatchAction {
    let mut fields = serde_json::Map::new();
    fields.insert("invocation_id".to_owned(), json!(id));
    for index in 0..4 {
        fields.insert(
            format!("attribute_{index:02}"),
            json!(format!("value {index}")),
        );
    }
    BatchAction::Create(CreateRequest {
        subject: invocation(id),
        definition_version: 1,
        fields: Value::Object(fields),
        recording: recording(&format!("create-{id}")),
    })
}

fn settle(id: &str, revision: u64) -> BatchAction {
    BatchAction::Execute(ExecuteRequest {
        subject: invocation(id),
        expected_revision: revision,
        operation: "settle".to_owned(),
        arguments: json!({ "status": "ok" }),
        fulfillments: Default::default(),
        recording: recording(&format!("settle-{id}")),
    })
}

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

fn directory() -> tempfile::TempDir {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
    std::fs::create_dir_all(&root).expect("build directory");
    tempfile::tempdir_in(root).expect("directory")
}

async fn provisioned(path: &Path) -> Authority {
    let backend = Arc::new(
        eventlog_sqlite::SqliteEventStore::open(path.to_str().expect("utf-8"), PREFIX)
            .await
            .expect("store"),
    );
    let tenant = TenantId::new(TENANT).expect("tenant");
    let authority = Authority {
        logical_scope: "definition-scope".to_owned(),
        tenant: TENANT.to_owned(),
        stream_identity: backend.stream_identity(&tenant).await.expect("identity"),
    };
    let projector = Arc::new(ErRecordedProjector::new());
    backend
        .create_projections(projector.clone())
        .await
        .expect("projections");
    backend
        .attach_inline_existing(projector)
        .await
        .expect("attach");
    EventlogBindingProvisioner::new(backend, WIDE)
        .provision_binding(authority.clone(), context("binding"))
        .await
        .expect("binding");
    authority
}

async fn open(path: &Path, authority: &Authority, policy: CapturePolicy) -> EventlogRecordedStore {
    EventlogRecordedStore::open_with_policy(attached(path).await, authority.clone(), WIDE, policy)
        .await
        .expect("the store opens")
}

async fn write(store: &EventlogRecordedStore, key: &str, actions: Vec<BatchAction>) {
    let registry = registry();
    let operation = store.operation(context(key));
    Executor::new(&registry, &operation)
        .batch(BatchKey::Named(key.to_owned()), actions)
        .await
        .expect("the batch commits");
}

async fn capture(path: &Path) -> TenantCapture {
    attached(path)
        .await
        .capture_tenant(
            &TenantId::new(TENANT).expect("tenant"),
            projection_specs(),
            WIDE,
        )
        .await
        .expect("capture")
}

/// Every blob of a capture whose bytes begin with `framing`, by digest.
fn blobs_framed<'c>(capture: &'c TenantCapture, framing: &str) -> Vec<(&'c str, &'c [u8])> {
    capture
        .blobs
        .iter()
        .filter(|blob| record_framing(&blob.bytes).is_ok_and(|found| found == framing))
        .map(|blob| (blob.digest.as_str(), blob.bytes.as_slice()))
        .collect()
}

fn blob_bytes(capture: &TenantCapture) -> u64 {
    capture
        .blobs
        .iter()
        .map(|blob| blob.bytes.len() as u64)
        .sum()
}

/// Writes the format of every release before 0.33.0 for the duration of `body`.
async fn embedding<T>(body: impl std::future::Future<Output = T>) -> T {
    WRITES_EMBED_DEFINITIONS.with(|embed| embed.set(true));
    let value = body.await;
    WRITES_EMBED_DEFINITIONS.with(|embed| embed.set(false));
    value
}

/// A store written in the 0.32.0 format opens under both policies, verifies completely and serves
/// its records, histories and batches; a retry of one of its batches is answered as the batch it
/// is; new records added to it name their definition and the old ones keep their bytes.
#[test]
fn a_store_written_before_records_named_their_definitions_opens_verifies_and_serves() {
    block_on(async {
        let directory = directory();
        let path = directory.path().join("store.sqlite");
        let authority = provisioned(&path).await;
        let writer = open(&path, &authority, CapturePolicy::FullVerification).await;
        embedding(async {
            write(&writer, "old-one", vec![create("a")]).await;
            write(&writer, "old-two", vec![settle("a", 1), create("b")]).await;
        })
        .await;
        drop(writer);

        let before = capture(&path).await;
        let old_records: Vec<(String, Vec<u8>)> = blobs_framed(&before, "er.record/2")
            .into_iter()
            .map(|(digest, bytes)| (digest.to_owned(), bytes.to_vec()))
            .collect();
        assert_eq!(old_records.len(), 3, "three records, each in its own blob");
        assert!(
            old_records
                .iter()
                .all(|(_, bytes)| String::from_utf8_lossy(bytes).contains("\"definition\":{")),
            "a 0.32.0 record embeds its definition"
        );
        assert!(blobs_framed(&before, RECORD_BY_DIGEST).is_empty());
        assert!(blobs_framed(&before, "er.definition/1").is_empty());

        for policy in CapturePolicy::ALL {
            let store = open(&path, &authority, policy).await;
            let assurance = verify_complete_store(&store, "definition-scope")
                .await
                .expect("a 0.32.0 store verifies without a rewrite");
            assert_eq!(assurance.subjects.len(), 2, "{policy:?}");
            let state = store
                .load(&invocation("a"))
                .await
                .expect("reads")
                .expect("present");
            assert_eq!(state.lifecycle_state, "settled", "{policy:?}");
            let history = store.history(&invocation("a")).await.expect("history");
            assert_eq!(history.records.len(), 2, "{policy:?}");
            let Some(RecordLookup::Committed(record)) =
                store.lookup_record("settle-a").await.expect("lookup")
            else {
                panic!("the settle record is committed");
            };
            assert!(
                old_records
                    .iter()
                    .any(|(_, bytes)| *bytes == record.record_bytes),
                "{policy:?}: a 0.32.0 record is served with the bytes it was stored as"
            );
            let batch = store
                .lookup_batch(&BatchKey::Named("old-two".to_owned()))
                .await
                .expect("lookup")
                .expect("present");
            assert_eq!(batch.records.len(), 2, "{policy:?}");
            // The same command again: answered as the batch it already is, not as a conflict.
            write(&store, "old-two", vec![settle("a", 1), create("b")]).await;
        }

        let store = open(&path, &authority, CapturePolicy::FullVerification).await;
        write(&store, "new-one", vec![create("c")]).await;
        write(&store, "new-two", vec![settle("b", 1), create("d")]).await;
        drop(store);
        let after = capture(&path).await;
        for (digest, bytes) in &old_records {
            assert!(
                after
                    .blobs
                    .iter()
                    .any(|blob| blob.digest == *digest && blob.bytes == *bytes),
                "an old record blob keeps its key and its bytes"
            );
        }
        let named = blobs_framed(&after, RECORD_BY_DIGEST);
        assert_eq!(named.len(), 3, "the new records name their definition");
        assert_eq!(
            blobs_framed(&after, "er.definition/1").len(),
            1,
            "one definition, stored once for every record that names it"
        );
        for policy in CapturePolicy::ALL {
            let store = open(&path, &authority, policy).await;
            let assurance = verify_complete_store(&store, "definition-scope")
                .await
                .expect("a store holding both forms verifies");
            assert_eq!(assurance.subjects.len(), 4, "{policy:?}");
        }
    });
}

/// What one record of a `service/1` entity adds to a store, before and after: every blob a
/// single-record commit binds, with the definition blob the first record stores counted apart.
#[test]
fn a_new_record_holds_only_the_definition_digest_and_appends_far_fewer_bytes() {
    block_on(async {
        let measure = |embed: bool| async move {
            let directory = directory();
            let path = directory.path().join("store.sqlite");
            let authority = provisioned(&path).await;
            let store = open(&path, &authority, CapturePolicy::FullVerification).await;
            let write_all = async {
                write(&store, "first", vec![create("first")]).await;
                let after_first = blob_bytes(&capture(&path).await);
                write(&store, "second", vec![create("second")]).await;
                after_first
            };
            let after_first = if embed {
                embedding(write_all).await
            } else {
                write_all.await
            };
            let held = capture(&path).await;
            let second = blob_bytes(&held) - after_first;
            let definition: u64 = blobs_framed(&held, "er.definition/1")
                .iter()
                .map(|(_, bytes)| bytes.len() as u64)
                .sum();
            let record: u64 = blobs_framed(
                &held,
                if embed {
                    "er.record/2"
                } else {
                    RECORD_BY_DIGEST
                },
            )
            .iter()
            .map(|(_, bytes)| bytes.len() as u64)
            .max()
            .expect("a record blob");
            (second, definition, record)
        };
        let (before, before_definition, before_record) = measure(true).await;
        let (after, after_definition, after_record) = measure(false).await;
        eprintln!(
            "bytes one service/1 record appends: before {before} (record blob {before_record}), \
             after {after} (record blob {after_record}); definition blob {after_definition} once \
             per tenant"
        );
        assert_eq!(before_definition, 0, "0.32.0 stores no definition blob");
        assert!(after_definition > 0, "the definition is stored once");
        assert!(
            after_record + after_definition / 2 < before_record,
            "a record no longer carries its definition: {after_record} vs {before_record}"
        );
        assert!(
            after * 3 < before,
            "a second record appends a fraction of what it did: {after} vs {before}"
        );
    });
}

/// A record whose definition digest resolves to nothing, or to bytes that do not hash to it, is
/// refused by name; so is an `er.record/5` record whose definition is not a digest string.
#[test]
fn a_record_whose_definition_is_missing_or_does_not_match_is_refused() {
    block_on(async {
        let directory = directory();
        let path = directory.path().join("store.sqlite");
        let authority = provisioned(&path).await;
        let store = open(&path, &authority, CapturePolicy::FullVerification).await;
        write(&store, "one", vec![create("a")]).await;
        drop(store);
        let whole = capture(&path).await;
        build_model(&authority, &whole).expect("the untouched capture verifies");
        let definition = blobs_framed(&whole, "er.definition/1")
            .first()
            .map(|(digest, _)| (*digest).to_owned())
            .expect("a definition blob");
        assert_eq!(
            crate::encoding::framed_key(
                DEFINITION_BLOB_DOMAIN,
                blobs_framed(&whole, "er.definition/1")[0].1
            )
            .expect("key"),
            definition
        );

        let mut missing = whole.clone();
        missing.blobs.retain(|blob| blob.digest != definition);
        assert_eq!(
            refusal_of(build_model(&authority, &missing)),
            format!("a record names definition {definition}, which the store does not hold")
        );

        let mut mismatched = whole.clone();
        for blob in &mut mismatched.blobs {
            if blob.digest == definition {
                let text = String::from_utf8(blob.bytes.clone()).expect("utf-8");
                assert!(text.contains("Invoked"));
                blob.bytes = text.replace("Invoked", "Invoken").into_bytes();
            }
        }
        assert_eq!(
            refusal_of(build_model(&authority, &mismatched)),
            format!("definition blob {definition} does not hash to the digest the record names")
        );

        // A handle that has just verified the sound store remembers its records' decodes, and
        // still refuses the damaged definition exactly as a build from nothing does.
        for (damaged, expected) in [
            (
                &missing,
                format!("a record names definition {definition}, which the store does not hold"),
            ),
            (
                &mismatched,
                format!(
                    "definition blob {definition} does not hash to the digest the record names"
                ),
            ),
        ] {
            let mut memory = super::memory::VerifiedMemory::default();
            super::build_model_remembering(&authority, &whole, Some(&mut memory))
                .expect("the sound store builds");
            assert_eq!(
                refusal_of(super::build_model_remembering(
                    &authority,
                    damaged,
                    Some(&mut memory)
                )),
                expected
            );
        }

        // A record that names its definition is still held to its own canonical bytes, and its
        // definition slot must be a digest string.
        let definitions = std::collections::BTreeMap::from([(
            definition.clone(),
            crate::encoding::decode_definition(blobs_framed(&whole, "er.definition/1")[0].1)
                .expect("definition decodes"),
        )]);
        let record = blobs_framed(&whole, RECORD_BY_DIGEST)[0].1;
        crate::encoding::decode_record(record, &mut crate::encoding::resolve_from(&definitions))
            .expect("the stored record decodes");
        let spaced = String::from_utf8(record.to_vec())
            .expect("utf-8")
            .replacen("\",{", "\", {", 1);
        assert_ne!(spaced.as_bytes(), record, "the body was respaced");
        assert_eq!(
            refusal_of(crate::encoding::decode_record(
                spaced.as_bytes(),
                &mut crate::encoding::resolve_from(&definitions)
            )),
            "record bytes do not reproduce the complete record"
        );
        let inlined = String::from_utf8(record.to_vec()).expect("utf-8").replacen(
            &format!("\"{definition}\""),
            "{}",
            1,
        );
        assert_ne!(inlined.as_bytes(), record, "the digest was replaced");
        assert_eq!(
            refusal_of(crate::encoding::decode_record(
                inlined.as_bytes(),
                &mut crate::encoding::resolve_from(&definitions)
            )),
            "an er.record/5 record names its definition by a digest string"
        );
    });
}

fn refusal_of<T>(result: Result<T, AsyncStoreError>) -> String {
    match result {
        Err(AsyncStoreError::ProviderIntegrity { provider, detail }) if provider == "eventlog" => {
            detail
        }
        Err(other) => panic!("refused for another reason: {other:?}"),
        Ok(_) => panic!("damaged content was admitted"),
    }
}
