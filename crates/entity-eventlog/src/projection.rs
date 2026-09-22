use std::sync::OnceLock;

use entity_store::asynchronous::{AsyncStoreError, BatchKey, RecordKind, RecordedEntry, Subject};
use eventlog_core::{
    BoxFuture, EventLogError, ProjectionSpec, ProjectionStore, Projector, RecordedEvent, TenantId,
};
use serde_json::{Value, json};

use crate::encoding::{
    ANCHOR_BLOB_DOMAIN, Authority, BATCH_BLOB_DOMAIN, BINDING_BLOB_DOMAIN, ENTRY_BLOB_DOMAIN,
    PhysicalRef, RECORD_BLOB_DOMAIN, REQUEST_BLOB_DOMAIN, SubjectWire, authority_subject_value,
    decode_anchor, decode_batch, decode_binding, decode_entry, decode_record, framed_key,
    key_for_value,
};

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
    let entry = decode_record(&record_bytes).map_err(store_error)?;
    if entry.subject() != subject {
        return Err(invalid("record wrapper subject differs from its record"));
    }
    let request_bytes = required_blob(store, &wrapper.request_blob, REQUEST_BLOB_DOMAIN).await?;
    if entity_store::asynchronous::original_request_comparison_bytes(&entry).map_err(store_error)?
        != request_bytes
    {
        return Err(invalid("request blob differs from its complete record"));
    }
    let batch_bytes = required_blob(store, &wrapper.batch_blob, BATCH_BLOB_DOMAIN).await?;
    let (decoded_key, members) = decode_batch(&batch_bytes).map_err(store_error)?;
    let wrapper_key: BatchKey = wrapper.batch_key.clone().into();
    if decoded_key != wrapper_key {
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
    let record_row = json!(["er.eventlog.record-index/1", {"entry": {
        "kind":"committed", "authority":wrapper.authority, "record_id":entry.record_id(),
        "subject":SubjectWire::from(&subject), "record_kind":kind(entry.kind()), "revision":entry.revision(),
        "batch_key":wrapper.batch_key, "member_index":wrapper.member_index,
        "record_blob":wrapper.record_blob, "request_blob":wrapper.request_blob,
        "batch_blob":wrapper.batch_blob, "physical":physical,
    }}]);
    upsert_once(store, record_spec(), &event.tenant, &row_key, &record_row).await?;

    let subject_row_key = subject_key(&wrapper.authority, &subject).map_err(store_error)?;
    let prior = store
        .get(subject_spec(), &event.tenant, &subject_row_key)
        .await?;
    let (origin, state_source, prior_revision) =
        fold_subject_source(prior.as_ref(), &entry, &wrapper.record_blob)?;
    let subject_row = json!(["er.eventlog.subject-index/1", {
        "authority":wrapper.authority, "subject":SubjectWire::from(&subject), "origin":origin,
        "revision":prior_revision, "state_source":state_source, "physical_head":physical,
    }]);
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
            saved.push(json!({
                "member_index":index,"record_id":member.entry.record_id(),"record_key":key,
                "subject":SubjectWire::from(&member.entry.subject()),"record_kind":kind(member.entry.kind()),
                "revision":member.entry.revision(),"record_blob":body["record_blob"],
                "request_blob":body["request_blob"],"physical":body["physical"],
            }));
        }
        let key = batch_key(&wrapper.authority, &decoded_key).map_err(store_error)?;
        let row = json!(["er.eventlog.batch-index/1", {"authority":wrapper.authority,"batch_key":crate::encoding::BatchKeyWire::from(&decoded_key),"batch_blob":wrapper.batch_blob,"members":saved}]);
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
            let entry = decode_record(&record_bytes).map_err(store_error)?;
            if entry.subject() != subject {
                return Err(invalid("imported envelope names another subject"));
            }
            validate_revision(&entry)?;
            let key = record_key(&wrapper.authority, entry.record_id()).map_err(store_error)?;
            let row = json!(["er.eventlog.record-index/1", {"entry": {
                "kind":"imported","authority":wrapper.authority,"record_id":entry.record_id(),
                "subject":SubjectWire::from(&subject),"record_kind":kind(entry.kind()),"revision":entry.revision(),
                "anchor_blob":anchor_digest,"evidence_index":index,"record_blob":record_blob,"anchor_physical":physical(event),
            }}]);
            upsert_once(store, record_spec(), &event.tenant, &key, &row).await?;
        }
    }
    let key = subject_key(&wrapper.authority, &subject).map_err(store_error)?;
    let row = json!(["er.eventlog.subject-index/1", {
        "authority":wrapper.authority,"subject":SubjectWire::from(&subject),
        "origin":{"kind":"imported","anchor_blob":anchor_digest,"anchor_physical":physical(event)},
        "revision":wrapper.instance.revision,"state_source":{"kind":"anchor","anchor_blob":anchor_digest},
        "physical_head":physical(event),
    }]);
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
        (None, RecordedEntry::Decision(_)) if entry.revision() == 1 => Ok((
            json!({"kind":"genesis"}),
            json!({"kind":"decision","record_blob":record_blob}),
            1,
        )),
        (None, _) => Err(invalid(
            "subject history does not begin with creation or anchor",
        )),
        (Some(row), RecordedEntry::Decision(_)) => {
            let body = tagged_body(row, "er.eventlog.subject-index/1")?;
            Ok((
                body["origin"].clone(),
                json!({"kind":"decision","record_blob":record_blob}),
                entry.revision(),
            ))
        }
        (Some(row), RecordedEntry::Observation(_)) => {
            let body = tagged_body(row, "er.eventlog.subject-index/1")?;
            let revision = body["revision"]
                .as_u64()
                .ok_or_else(|| invalid("subject row has invalid revision"))?;
            if entry.revision() != revision {
                return Err(invalid("observation revision differs from subject row"));
            }
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
fn store_error(error: AsyncStoreError) -> EventLogError {
    EventLogError::Invalid(error.to_string())
}
