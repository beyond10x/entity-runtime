use std::sync::OnceLock;

use entity_store::asynchronous::{AsyncStoreError, BatchKey, RecordKind, RecordedEntry, Subject};
use eventlog_core::{
    BoxFuture, EventLogError, ProjectionSpec, ProjectionStore, Projector, RecordedEvent, TenantId,
};
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};

use crate::encoding::{
    ANCHOR_BLOB_DOMAIN, Authority, BATCH_BLOB_DOMAIN, BINDING_BLOB_DOMAIN, ENTRY_BLOB_DOMAIN,
    PhysicalRef, RECORD_BLOB_DOMAIN, REQUEST_BLOB_DOMAIN, RecordedEntryWrapper, SubjectWire,
    authority_subject_value, decode_anchor, decode_batch, decode_binding, decode_entry,
    decode_record, framed_key, key_for_value, missing_definition, named_definitions, resolve_from,
    unresolvable, verify_definition,
};
use entity_core::EntityDefinition;
use std::collections::BTreeMap;

pub(crate) const PROJECTOR_NAME: &str = "er_recorded_v1";
pub(crate) const BINDING_NAME: &str = "er_binding_v1";
pub(crate) const RECORDS_NAME: &str = "er_records_v1";
pub(crate) const BATCHES_NAME: &str = "er_batches_v1";
pub(crate) const SUBJECTS_NAME: &str = "er_subjects_v1";

/// The exact four projection declarations owned by the recorded adapter.
#[must_use]
pub fn projection_specs() -> &'static [ProjectionSpec] {
    static SPECS: OnceLock<Vec<ProjectionSpec>> = OnceLock::new();
    SPECS.get_or_init(|| {
        [BINDING_NAME, RECORDS_NAME, BATCHES_NAME, SUBJECTS_NAME]
            .into_iter()
            .map(|name| {
                let specification = ProjectionSpec { name, indexed: &[] };
                specification
                    .validate()
                    .expect("fixed projection names are valid");
                specification
            })
            .collect()
    })
}

pub(crate) fn binding_spec() -> &'static ProjectionSpec {
    &projection_specs()[0]
}
pub(crate) fn record_spec() -> &'static ProjectionSpec {
    &projection_specs()[1]
}
pub(crate) fn batch_spec() -> &'static ProjectionSpec {
    &projection_specs()[2]
}
pub(crate) fn subject_spec() -> &'static ProjectionSpec {
    &projection_specs()[3]
}

pub(crate) fn record_key(
    authority: &Authority,
    record_id: &str,
) -> Result<String, AsyncStoreError> {
    key_for_value(
        "er.eventlog.record-index-key/1",
        json!({"authority":authority,"record_id":record_id}),
    )
}

pub(crate) fn batch_key(authority: &Authority, key: &BatchKey) -> Result<String, AsyncStoreError> {
    key_for_value(
        "er.eventlog.batch-index-key/1",
        json!({"authority":authority,"batch_key":crate::encoding::BatchKeyWire::from(key)}),
    )
}

pub(crate) fn subject_key(
    authority: &Authority,
    subject: &Subject,
) -> Result<String, AsyncStoreError> {
    key_for_value(
        "er.eventlog.subject-index-key/1",
        authority_subject_value(authority, subject),
    )
}

pub(crate) fn subject_stream_id(
    authority: &Authority,
    subject: &Subject,
) -> Result<String, AsyncStoreError> {
    key_for_value(
        "er.eventlog.subject-stream-key/1",
        authority_subject_value(authority, subject),
    )
}

/// The version of the rows [`ErRecordedProjector`] writes for a given history. Raise it in the
/// change that makes the projector write different rows for the same events, so a kept fold of the
/// earlier rows is replayed rather than served.
const PROJECTOR_ROWS_VERSION: u32 = 1;

/// SHA-256 over the projector's name, its tables and [`PROJECTOR_ROWS_VERSION`].
fn projector_fingerprint() -> String {
    let tables: Vec<Value> = projection_specs()
        .iter()
        .map(|spec| json!([spec.name, spec.indexed]))
        .collect();
    let described = json!({
        "format": "er.eventlog.recorded-projector-fingerprint/1",
        "name": PROJECTOR_NAME,
        "tables": tables,
        "rows": PROJECTOR_ROWS_VERSION,
    });
    let mut hasher = Sha256::new();
    hasher.update(described.to_string().as_bytes());
    format!("sha256:{:x}", hasher.finalize())
}

/// Deterministic inline projector for binding, records, batches, and current subjects.
#[derive(Debug, Default)]
pub struct ErRecordedProjector;

impl ErRecordedProjector {
    /// Constructs the fixed projector.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Projector for ErRecordedProjector {
    fn name(&self) -> &'static str {
        PROJECTOR_NAME
    }
    fn projections(&self) -> &'static [ProjectionSpec] {
        projection_specs()
    }

    /// The projector has no configuration: what it writes is decided by the tables it owns and
    /// the row formats it folds to, and the digest covers both. A store that keeps a folded state
    /// across opens (the Eventlog tree store) reuses it only while this is unchanged, so a new
    /// row format replays once instead of serving rows the earlier one wrote. A rebuilt program
    /// with the same formats is covered by that store too: it keys a kept state by the running
    /// executable as well.
    fn fingerprint(&self) -> Option<String> {
        static FINGERPRINT: OnceLock<String> = OnceLock::new();
        Some(FINGERPRINT.get_or_init(projector_fingerprint).clone())
    }

    fn apply<'a>(
        &'a self,
        event: &'a RecordedEvent,
        store: &'a mut dyn ProjectionStore,
    ) -> BoxFuture<'a, Result<(), EventLogError>> {
        Box::pin(async move {
            validate_reference_event(event)?;
            match event.name.as_str() {
                "er.binding" => apply_binding(event, store).await,
                "er.recorded_entry" => apply_recorded_entry(event, store).await,
                "er.import_anchor" => apply_import_anchor(event, store).await,
                // A refusal changes no subject, so it projects no row; the capture verifies it.
                "er.refused_request" if event.stream_type == "er.refusal" => Ok(()),
                _ => Err(EventLogError::Invalid(
                    "unknown event in ER-owned tenant".into(),
                )),
            }
        })
    }
}

async fn apply_binding(
    event: &RecordedEvent,
    store: &mut dyn ProjectionStore,
) -> Result<(), EventLogError> {
    if event.stream_type != "er.binding" || event.stream_id != "singleton" || event.version != 1 {
        return Err(invalid("binding event has wrong stream coordinates"));
    }
    let (digest, bytes) = reference_blob(event, store, BINDING_BLOB_DOMAIN).await?;
    let authority = decode_binding(&bytes).map_err(store_error)?;
    require_tenant(event, &authority)?;
    let row = json!(["er.eventlog.binding-index/1", {
        "authority": authority,
        "binding_blob": digest,
        "physical": physical(event),
    }]);
    upsert_once(store, binding_spec(), &event.tenant, "singleton", &row).await
}

async fn apply_recorded_entry(
    event: &RecordedEvent,
    store: &mut dyn ProjectionStore,
) -> Result<(), EventLogError> {
    if event.stream_type != "er.subject" {
        return Err(invalid("record reference has wrong stream type"));
    }
    let (entry_digest, bytes) = reference_blob(event, store, ENTRY_BLOB_DOMAIN).await?;
    let wrapper = decode_entry(&bytes).map_err(store_error)?;
    require_tenant(event, &wrapper.authority)?;
    let subject: Subject = wrapper.subject.clone().into();
    if event.stream_id != subject_stream_id(&wrapper.authority, &subject).map_err(store_error)? {
        return Err(invalid("record reference is in another subject stream"));
    }
    let record_bytes = required_blob(store, &wrapper.record_blob, RECORD_BLOB_DOMAIN).await?;
    let definitions = projected_definitions(store, &record_bytes).await?;
    let entry =
        decode_record(&record_bytes, &mut resolve_from(&definitions)).map_err(store_error)?;
    if entry.subject() != subject {
        return Err(invalid("record wrapper subject differs from its record"));
    }
    let request_bytes = required_blob(store, &wrapper.request_blob, REQUEST_BLOB_DOMAIN).await?;
    if entity_store::asynchronous::original_request_comparison_bytes(&entry).map_err(store_error)?
        != request_bytes
    {
        return Err(invalid("request blob differs from its complete record"));
    }
    let batch = member_batch(store, event, &wrapper).await?;
    let (decoded_key, members) = (&batch.key, &batch.members);
    let wrapper_key: BatchKey = wrapper.batch_key.clone().into();
    if *decoded_key != wrapper_key {
        return Err(invalid("record wrapper names another batch"));
    }
    let member_index = usize::try_from(wrapper.member_index)
        .map_err(|_| invalid("member index exceeds this platform"))?;
    let member = members
        .get(member_index)
        .ok_or_else(|| invalid("member index exceeds the batch"))?;
    if member.entry != entry || member.request_bytes != request_bytes {
        return Err(invalid("record wrapper differs from its batch member"));
    }
    if matches!(&wrapper_key, BatchKey::SingleRecord(id) if members.len() != 1 || id != entry.record_id())
    {
        return Err(invalid("single-record wrapper has invalid membership"));
    }
    validate_revision(&entry)?;
    let physical = physical(event);
    let row_key = record_key(&wrapper.authority, entry.record_id()).map_err(store_error)?;
    let record_row = committed_record_row(&wrapper, &entry, &physical);
    upsert_once(store, record_spec(), &event.tenant, &row_key, &record_row).await?;

    let subject_row_key = subject_key(&wrapper.authority, &subject).map_err(store_error)?;
    let prior = store
        .get(subject_spec(), &event.tenant, &subject_row_key)
        .await?;
    let (origin, state_source, prior_revision) =
        fold_subject_source(prior.as_ref(), &entry, &wrapper.record_blob)?;
    let subject_row = subject_row(
        &wrapper.authority,
        &subject,
        &origin,
        prior_revision,
        &state_source,
        &physical,
    );
    store
        .upsert(
            subject_spec(),
            &event.tenant,
            &subject_row_key,
            &subject_row,
        )
        .await?;

    if member_index + 1 == members.len() {
        let mut saved = Vec::with_capacity(members.len());
        for (index, member) in members.iter().enumerate() {
            let key =
                record_key(&wrapper.authority, member.entry.record_id()).map_err(store_error)?;
            let row = store
                .get(record_spec(), &event.tenant, &key)
                .await?
                .ok_or_else(|| invalid("final batch member cannot resolve an earlier member"))?;
            let body = committed_record_body(&row)?;
            if body.get("member_index").and_then(Value::as_u64)
                != Some(u64::try_from(index).map_err(|_| invalid("member index exhausted"))?)
                || body.get("batch_blob").and_then(Value::as_str)
                    != Some(wrapper.batch_blob.as_str())
            {
                return Err(invalid(
                    "batch member row does not reproduce its membership",
                ));
            }
            saved.push(batch_member_row(
                index,
                &member.entry,
                &key,
                &body["record_blob"],
                &body["request_blob"],
                &body["physical"],
            ));
        }
        let key = batch_key(&wrapper.authority, decoded_key).map_err(store_error)?;
        let row = batch_row(&wrapper.authority, decoded_key, &wrapper.batch_blob, saved);
        upsert_once(store, batch_spec(), &event.tenant, &key, &row).await?;
    }
    let _ = entry_digest;
    Ok(())
}

async fn apply_import_anchor(
    event: &RecordedEvent,
    store: &mut dyn ProjectionStore,
) -> Result<(), EventLogError> {
    if event.stream_type != "er.subject" || event.version == 0 {
        return Err(invalid("import anchor has wrong stream coordinates"));
    }
    let (anchor_digest, bytes) = reference_blob(event, store, ANCHOR_BLOB_DOMAIN).await?;
    let wrapper = decode_anchor(&bytes).map_err(store_error)?;
    require_tenant(event, &wrapper.authority)?;
    let subject: Subject = wrapper.subject.clone().into();
    if event.stream_id != subject_stream_id(&wrapper.authority, &subject).map_err(store_error)? {
        return Err(invalid("anchor is in another subject stream"));
    }
    if wrapper.instance.entity != subject.entity || wrapper.instance.id != subject.id {
        return Err(invalid("anchor instance names another subject"));
    }
    validate_er_revision(wrapper.instance.revision)?;
    for (index, evidence) in wrapper.evidence.iter().enumerate() {
        if let crate::encoding::EvidenceWire::Envelope { record_blob, .. } = evidence {
            let record_bytes = required_blob(store, record_blob, RECORD_BLOB_DOMAIN).await?;
            let entry = decode_record(&record_bytes, &mut unresolvable).map_err(store_error)?;
            if entry.subject() != subject {
                return Err(invalid("imported envelope names another subject"));
            }
            validate_revision(&entry)?;
            let key = record_key(&wrapper.authority, entry.record_id()).map_err(store_error)?;
            let row = imported_record_row(
                &wrapper.authority,
                &entry,
                &anchor_digest,
                index,
                record_blob,
                &physical(event),
            );
            upsert_once(store, record_spec(), &event.tenant, &key, &row).await?;
        }
    }
    let key = subject_key(&wrapper.authority, &subject).map_err(store_error)?;
    let row = imported_subject_row(
        &wrapper.authority,
        &subject,
        &anchor_digest,
        wrapper.instance.revision,
        &physical(event),
    );
    upsert_once(store, subject_spec(), &event.tenant, &key, &row).await
}

fn validate_reference_event(event: &RecordedEvent) -> Result<(), EventLogError> {
    if event.is_redacted() || event.schema_version != 1 {
        return Err(invalid("redacted or unknown reference event"));
    }
    let object = event
        .data
        .as_object()
        .ok_or_else(|| invalid("reference body is not an object"))?;
    if object.len() != 1 || object.get("blob").and_then(Value::as_str).is_none() {
        return Err(invalid("reference body is not exact {blob}"));
    }
    Ok(())
}

async fn reference_blob(
    event: &RecordedEvent,
    store: &mut dyn ProjectionStore,
    domain: &str,
) -> Result<(String, Vec<u8>), EventLogError> {
    let digest = event.data["blob"]
        .as_str()
        .ok_or_else(|| invalid("reference has no blob"))?
        .to_owned();
    let bytes = required_blob(store, &digest, domain).await?;
    Ok((digest, bytes))
}

async fn required_blob(
    store: &mut dyn ProjectionStore,
    digest: &str,
    domain: &str,
) -> Result<Vec<u8>, EventLogError> {
    let bytes = store
        .get_blob(digest)
        .await?
        .ok_or(EventLogError::NotFound)?;
    if framed_key(domain, &bytes).map_err(store_error)? != digest {
        return Err(invalid("referenced blob digest uses wrong bytes or domain"));
    }
    Ok(bytes)
}

/// A batch blob whose bytes were held to their digest and decoded.
struct VerifiedBatch {
    digest: String,
    bytes: Vec<u8>,
    key: BatchKey,
    members: Vec<entity_store::asynchronous::AppendMember>,
}

/// The largest batch blob [`LAST_BATCH`] keeps, in bytes: 32 MiB, the budget eventlog-sqlite's
/// verified-blob memory keeps too. A larger blob is still verified and decoded, for every member,
/// and is not kept.
const LAST_BATCH_LIMIT: usize = 32 << 20;

thread_local! {
    /// The last batch blob this thread verified and decoded, if it was at most
    /// [`LAST_BATCH_LIMIT`] bytes.
    ///
    /// Every member event of a group references the same batch blob, which holds every member, so
    /// hashing and decoding it once per member made a group cost the square of its members. Both
    /// are functions of the bytes alone, so a blob read again with exactly the bytes and digest
    /// verified last time reuses that answer. A later member of the same group does not read the
    /// blob again when the view proves its predecessor was projected from it ([`member_batch`]);
    /// every other read still reaches the provider, so a blob the store does not hold is refused
    /// exactly as before. A blob over the
    /// limit and a blob that does not verify both empty the slot, so it never holds more than the
    /// limit and never outlives a refusal.
    static LAST_BATCH: std::cell::RefCell<Option<std::sync::Arc<VerifiedBatch>>> =
        const { std::cell::RefCell::new(None) };
}

/// The verified batch blob one member event names.
///
/// Every member event of a group names the same batch blob, which holds every member, so reading
/// it from the provider for each member returned the whole blob once per member: K copies of a
/// blob that itself grows with K. Within one projection pass the blob is read, held to its digest
/// and decoded once, by the first member that needs it.
///
/// A later member reuses that answer only when this projection view proves the member before it
/// was projected from exactly that blob: its record row is present here, names this batch blob,
/// this batch key and the member index just before this one, and stands at the commit position
/// just before this event. That row is written only after its member's blob was read from this
/// view and verified, and the inline projector runs a group, and a rebuild, in one transaction, so
/// the availability and digest checks were made inside the same transaction. Anything else — the
/// first member, a single record, a gap, another blob or batch, a kept answer for another digest
/// or none — reads and verifies the blob from the provider as before.
async fn member_batch(
    store: &mut dyn ProjectionStore,
    event: &RecordedEvent,
    wrapper: &crate::encoding::RecordedEntryWrapper,
) -> Result<std::sync::Arc<VerifiedBatch>, EventLogError> {
    let last = LAST_BATCH.with(|last| last.borrow().clone());
    if let Some(last) = last
        && last.digest == wrapper.batch_blob
        && previous_member_projected(store, event, wrapper, &last).await?
    {
        return Ok(last);
    }
    verified_batch(store, &wrapper.batch_blob).await
}

/// Whether this view holds the record row of the member before `wrapper`'s, projected from
/// `batch` at the commit position directly before `event`.
async fn previous_member_projected(
    store: &mut dyn ProjectionStore,
    event: &RecordedEvent,
    wrapper: &crate::encoding::RecordedEntryWrapper,
    batch: &VerifiedBatch,
) -> Result<bool, EventLogError> {
    let (Some(previous_index), Some(previous_seq)) = (
        wrapper.member_index.checked_sub(1),
        event.global_seq.checked_sub(1),
    ) else {
        return Ok(false);
    };
    let Some(previous) = usize::try_from(previous_index)
        .ok()
        .and_then(|index| batch.members.get(index))
    else {
        return Ok(false);
    };
    let key = record_key(&wrapper.authority, previous.entry.record_id()).map_err(store_error)?;
    let Some(row) = store.get(record_spec(), &event.tenant, &key).await? else {
        return Ok(false);
    };
    let Ok(body) = committed_record_body(&row) else {
        return Ok(false);
    };
    let batch_key = serde_json::to_value(&wrapper.batch_key)
        .map_err(|_| invalid("batch key does not encode"))?;
    Ok(
        body.get("batch_blob").and_then(Value::as_str) == Some(wrapper.batch_blob.as_str())
            && body.get("batch_key") == Some(&batch_key)
            && body.get("member_index").and_then(Value::as_u64) == Some(previous_index)
            && body
                .get("physical")
                .and_then(|physical| physical.get("global_seq"))
                .and_then(Value::as_u64)
                == Some(previous_seq),
    )
}

/// The batch blob `digest` names, read from `store`, held to its digest and decoded.
async fn verified_batch(
    store: &mut dyn ProjectionStore,
    digest: &str,
) -> Result<std::sync::Arc<VerifiedBatch>, EventLogError> {
    let bytes = store
        .get_blob(digest)
        .await?
        .ok_or(EventLogError::NotFound)?;
    #[cfg(feature = "kpi-counters")]
    {
        crate::kpi_counters::add(&crate::kpi_counters::BATCH_BLOB_FETCHES, 1);
        crate::kpi_counters::add(&crate::kpi_counters::BATCH_BLOB_BYTES, bytes.len() as u64);
    }
    let last = LAST_BATCH.with(|last| last.borrow().clone());
    if let Some(last) = last
        && last.digest == digest
        && last.bytes == bytes
    {
        return Ok(last);
    }
    let definitions = projected_definitions(store, &bytes).await?;
    verify_batch(digest, bytes, LAST_BATCH_LIMIT, &definitions)
}

/// Holds `bytes` to `digest` and decodes them, reusing this thread's last answer for exactly
/// these bytes, and keeping this answer only when the bytes are at most `limit` long.
fn verify_batch(
    digest: &str,
    bytes: Vec<u8>,
    limit: usize,
    definitions: &BTreeMap<String, EntityDefinition>,
) -> Result<std::sync::Arc<VerifiedBatch>, EventLogError> {
    let last = LAST_BATCH.with(|last| last.borrow().clone());
    if let Some(last) = last
        && last.digest == digest
        && last.bytes == bytes
    {
        return Ok(last);
    }
    // Emptied first, so a refusal or a blob over the limit leaves nothing kept.
    LAST_BATCH.with(|last| last.borrow_mut().take());
    if framed_key(BATCH_BLOB_DOMAIN, &bytes).map_err(store_error)? != digest {
        return Err(invalid("referenced blob digest uses wrong bytes or domain"));
    }
    let (key, members) =
        decode_batch(&bytes, &mut resolve_from(definitions)).map_err(store_error)?;
    let keep = bytes.len() <= limit;
    let verified = std::sync::Arc::new(VerifiedBatch {
        digest: digest.to_owned(),
        bytes,
        key,
        members,
    });
    if keep {
        LAST_BATCH.with(|last| *last.borrow_mut() = Some(std::sync::Arc::clone(&verified)));
    }
    Ok(verified)
}

async fn upsert_once(
    store: &mut dyn ProjectionStore,
    spec: &ProjectionSpec,
    tenant: &TenantId,
    key: &str,
    row: &Value,
) -> Result<(), EventLogError> {
    match store.get(spec, tenant, key).await? {
        Some(found) if found == *row => Ok(()),
        Some(_) => Err(invalid(
            "projection identity already names different authority",
        )),
        None => store.upsert(spec, tenant, key, row).await,
    }
}

fn fold_subject_source(
    prior: Option<&Value>,
    entry: &RecordedEntry,
    record_blob: &str,
) -> Result<(Value, Value, u64), EventLogError> {
    match (prior, entry) {
        (None, RecordedEntry::Decision(_)) if entry.revision() == 1 => {
            Ok((genesis_origin(), decision_state_source(record_blob), 1))
        }
        (None, _) => Err(invalid(
            "subject history does not begin with creation or anchor",
        )),
        (Some(row), RecordedEntry::Decision(_)) => {
            let body = tagged_body(row, "er.eventlog.subject-index/1")?;
            Ok((
                body["origin"].clone(),
                decision_state_source(record_blob),
                entry.revision(),
            ))
        }
        (Some(row), RecordedEntry::Observation(_)) => {
            let body = tagged_body(row, "er.eventlog.subject-index/1")?;
            let revision = body["revision"]
                .as_u64()
                .ok_or_else(|| invalid("subject row has invalid revision"))?;
            // An observation never moves the row, which follows decisions. Its revision is not
            // held to the row here: a store that merges branches replays in causal order with
            // ties by recorded time, so an observation can replay after another branch's decision
            // of a lower or higher revision than the one it observed. Whether it observed the
            // decision it follows is decided by lineage verification of its history.
            Ok((
                body["origin"].clone(),
                body["state_source"].clone(),
                revision,
            ))
        }
    }
}

fn committed_record_body(row: &Value) -> Result<&serde_json::Map<String, Value>, EventLogError> {
    let body = tagged_body(row, "er.eventlog.record-index/1")?;
    let entry = body
        .get("entry")
        .and_then(Value::as_object)
        .ok_or_else(|| invalid("record row has no entry"))?;
    if entry.get("kind").and_then(Value::as_str) != Some("committed") {
        return Err(invalid("batch member resolves to imported record"));
    }
    Ok(entry)
}

pub(crate) fn tagged_body<'a>(
    row: &'a Value,
    tag: &str,
) -> Result<&'a serde_json::Map<String, Value>, EventLogError> {
    let array = row
        .as_array()
        .filter(|a| a.len() == 2)
        .ok_or_else(|| invalid("projection row is not a tagged pair"))?;
    if array[0].as_str() != Some(tag) {
        return Err(invalid("projection row has wrong tag"));
    }
    array[1]
        .as_object()
        .ok_or_else(|| invalid("projection row body is not an object"))
}

fn require_tenant(event: &RecordedEvent, authority: &Authority) -> Result<(), EventLogError> {
    if event.tenant.as_str() == authority.tenant {
        Ok(())
    } else {
        Err(invalid("authority tenant differs from event tenant"))
    }
}

// The row bodies the projector writes. The adapter builds the same bodies before a write, with
// the widest physical reference, to bound the payload bytes the write adds (see `admit_growth`).

pub(crate) fn committed_record_row(
    wrapper: &RecordedEntryWrapper,
    entry: &RecordedEntry,
    physical: &PhysicalRef,
) -> Value {
    json!(["er.eventlog.record-index/1", {"entry": {
        "kind":"committed", "authority":wrapper.authority, "record_id":entry.record_id(),
        "subject":SubjectWire::from(&entry.subject()), "record_kind":kind(entry.kind()),
        "revision":entry.revision(), "batch_key":wrapper.batch_key,
        "member_index":wrapper.member_index, "record_blob":wrapper.record_blob,
        "request_blob":wrapper.request_blob, "batch_blob":wrapper.batch_blob, "physical":physical,
    }}])
}

pub(crate) fn imported_record_row(
    authority: &Authority,
    entry: &RecordedEntry,
    anchor_blob: &str,
    evidence_index: usize,
    record_blob: &str,
    physical: &PhysicalRef,
) -> Value {
    json!(["er.eventlog.record-index/1", {"entry": {
        "kind":"imported","authority":authority,"record_id":entry.record_id(),
        "subject":SubjectWire::from(&entry.subject()),"record_kind":kind(entry.kind()),
        "revision":entry.revision(),"anchor_blob":anchor_blob,"evidence_index":evidence_index,
        "record_blob":record_blob,"anchor_physical":physical,
    }}])
}

pub(crate) fn subject_row(
    authority: &Authority,
    subject: &Subject,
    origin: &Value,
    revision: u64,
    state_source: &Value,
    physical: &PhysicalRef,
) -> Value {
    json!(["er.eventlog.subject-index/1", {
        "authority":authority, "subject":SubjectWire::from(subject), "origin":origin,
        "revision":revision, "state_source":state_source, "physical_head":physical,
    }])
}

pub(crate) fn imported_subject_row(
    authority: &Authority,
    subject: &Subject,
    anchor_blob: &str,
    revision: u64,
    physical: &PhysicalRef,
) -> Value {
    subject_row(
        authority,
        subject,
        &json!({"kind":"imported","anchor_blob":anchor_blob,"anchor_physical":physical}),
        revision,
        &json!({"kind":"anchor","anchor_blob":anchor_blob}),
        physical,
    )
}

/// The origin of a subject whose history begins with its creation.
pub(crate) fn genesis_origin() -> Value {
    json!({"kind":"genesis"})
}

/// The state source a decision leaves on its subject's row.
pub(crate) fn decision_state_source(record_blob: &str) -> Value {
    json!({"kind":"decision","record_blob":record_blob})
}

pub(crate) fn batch_member_row(
    index: usize,
    entry: &RecordedEntry,
    record_key: &str,
    record_blob: &Value,
    request_blob: &Value,
    physical: &Value,
) -> Value {
    json!({
        "member_index":index,"record_id":entry.record_id(),"record_key":record_key,
        "subject":SubjectWire::from(&entry.subject()),"record_kind":kind(entry.kind()),
        "revision":entry.revision(),"record_blob":record_blob,
        "request_blob":request_blob,"physical":physical,
    })
}

pub(crate) fn batch_row(
    authority: &Authority,
    key: &BatchKey,
    batch_blob: &str,
    members: Vec<Value>,
) -> Value {
    json!(["er.eventlog.batch-index/1", {
        "authority":authority,"batch_key":crate::encoding::BatchKeyWire::from(key),
        "batch_blob":batch_blob,"members":members,
    }])
}

pub(crate) fn physical(event: &RecordedEvent) -> PhysicalRef {
    PhysicalRef {
        event_id: event.event_id.clone(),
        global_seq: event.global_seq,
        stream_id: event.stream_id.clone(),
        stream_version: event.version,
    }
}

fn kind(value: RecordKind) -> &'static str {
    match value {
        RecordKind::Decision => "decision",
        RecordKind::Observation => "observation",
    }
}
fn validate_revision(entry: &RecordedEntry) -> Result<(), EventLogError> {
    validate_er_revision(entry.revision())
}
fn validate_er_revision(revision: u64) -> Result<(), EventLogError> {
    if revision == 0 || revision > i64::MAX as u64 {
        Err(invalid("ER revision is outside 1..=i64::MAX"))
    } else {
        Ok(())
    }
}
fn invalid(detail: impl Into<String>) -> EventLogError {
    EventLogError::Invalid(detail.into())
}
/// Every definition a record or batch document names, read from `store`, held to its digest and
/// decoded.
pub(crate) async fn projected_definitions(
    store: &mut dyn ProjectionStore,
    bytes: &[u8],
) -> Result<BTreeMap<String, EntityDefinition>, EventLogError> {
    let mut definitions = BTreeMap::new();
    for digest in named_definitions(bytes).map_err(store_error)? {
        let blob = store
            .get_blob(&digest)
            .await?
            .ok_or_else(|| store_error(missing_definition(&digest)))?;
        let definition = verify_definition(&digest, &blob).map_err(store_error)?;
        definitions.insert(digest, definition);
    }
    Ok(definitions)
}

fn store_error(error: AsyncStoreError) -> EventLogError {
    EventLogError::Invalid(error.to_string())
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, sync::Arc};

    use entity_store::asynchronous::{BatchKey, batch_comparison_bytes};

    use super::{BATCH_BLOB_DOMAIN, LAST_BATCH, LAST_BATCH_LIMIT, framed_key, verify_batch};

    fn blob(name: &str) -> (String, Vec<u8>) {
        let bytes =
            batch_comparison_bytes(&BatchKey::Named(name.to_owned()), &[]).expect("batch bytes");
        (
            framed_key(BATCH_BLOB_DOMAIN, &bytes).expect("digest"),
            bytes,
        )
    }

    fn kept() -> Option<String> {
        LAST_BATCH.with(|last| last.borrow().as_ref().map(|batch| batch.digest.clone()))
    }

    #[test]
    fn a_batch_within_the_limit_is_kept_and_reused_for_exactly_its_bytes() {
        let (digest, bytes) = blob("within");
        let first = verify_batch(&digest, bytes.clone(), LAST_BATCH_LIMIT, &BTreeMap::new())
            .expect("verifies");
        assert_eq!(kept(), Some(digest.clone()));
        let again = verify_batch(&digest, bytes, LAST_BATCH_LIMIT, &BTreeMap::new())
            .expect("verifies again");
        assert!(
            Arc::ptr_eq(&first, &again),
            "the same bytes reuse the kept answer"
        );
    }

    #[test]
    fn a_batch_over_the_limit_still_verifies_and_decodes_but_is_not_kept() {
        let (kept_digest, kept_bytes) = blob("kept");
        verify_batch(&kept_digest, kept_bytes, LAST_BATCH_LIMIT, &BTreeMap::new())
            .expect("verifies");
        let (digest, bytes) = blob("over");
        let limit = bytes.len() - 1;
        let verified = verify_batch(&digest, bytes, limit, &BTreeMap::new())
            .expect("an oversized batch verifies");
        assert_eq!(verified.key, BatchKey::Named("over".to_owned()));
        assert!(verified.members.is_empty());
        assert_eq!(kept(), None, "a batch over the limit empties the slot");
    }

    #[test]
    fn a_batch_that_does_not_verify_empties_the_slot() {
        let (digest, bytes) = blob("kept");
        verify_batch(&digest, bytes.clone(), LAST_BATCH_LIMIT, &BTreeMap::new()).expect("verifies");
        let (other, _) = blob("other");
        let refused = verify_batch(&other, bytes, LAST_BATCH_LIMIT, &BTreeMap::new());
        assert!(
            matches!(&refused, Err(eventlog_core::EventLogError::Invalid(detail))
                if detail == "referenced blob digest uses wrong bytes or domain"),
            "{:?}",
            refused.err()
        );
        assert_eq!(kept(), None, "a refusal empties the slot");
    }
}
