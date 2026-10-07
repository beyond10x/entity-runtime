//! The persisted checkpoint a later `ProviderTracked` open starts from.
//!
//! A checkpoint records one observation a handle verified: the provider's durable proof that
//! nothing but its own acknowledged appends changed the tenant since, and the position it reached.
//! It is an Eventlog snapshot of one stream the bound authority owns, so it is never captured
//! material, never enters a model, and is copied, restored and forgotten with the store. It is a
//! cache: a record that is absent, unreadable, of another format or verifier, or whose digest does
//! not hold costs the next open a complete verification and nothing else. The one record an open
//! refuses is one whose digest and authority hold and whose position is beyond the provider's head:
//! only the checkpoint knows that a self-consistent store lost its tail.
//!
//! Wire form (`docs/design/recorded-open-checkpoint-v0.1.md` § *The wire form*): the snapshot's
//! state is one JSON object; `digest` is the framed SHA-256 of every other field's canonical
//! bytes. A discard writes a tombstone in place of a record, because the snapshot port has no
//! delete.

use entity_store::asynchronous::AsyncStoreError;
use eventlog_core::{CaptureLimits, Snapshot, StreamId, TenantId};
use serde_json::{Map, Value, json};
use time::OffsetDateTime;

use super::{EventlogBackend, input_eventlog, map_read_error};
use crate::{
    encoding::{Authority, PhysicalRef, key_for_value},
    projection::projection_specs,
};

/// The only format this verifier interprets.
pub(super) const FORMAT: &str = "er.eventlog.open-checkpoint/1";
/// The domain the record's own digest is framed in.
const DIGEST_DOMAIN: &str = "er.eventlog.open-checkpoint-digest/1";
/// The domain a tombstone names the record it replaced in.
const REPLACED_DOMAIN: &str = "er.eventlog.open-checkpoint-replaced/1";
/// The stream type of the one stream a bound authority keeps its checkpoint on.
pub(super) const STREAM_TYPE: &str = "er.open-checkpoint";
/// The stream id is the authority's framed key, built as a subject stream key is.
const STREAM_KEY_DOMAIN: &str = "er.eventlog.open-checkpoint-stream-key/1";
/// The verifier that wrote a record: a newer one re-verifies completely what an older one accepted.
pub(super) const VERIFIER: &str = env!("CARGO_PKG_VERSION");
/// The snapshot schema version of the state object.
const STATE_SCHEMA: u32 = 1;

/// One checkpoint: everything it binds except its own digest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Record {
    /// The `entity-eventlog` version that verified the observation.
    pub(super) verifier: String,
    /// The bound authority, stream identity included.
    pub(super) authority: Authority,
    /// The physical coordinates of the authority's binding event.
    pub(super) binding: PhysicalRef,
    /// The projection names the observation captured, in request order.
    pub(super) projections: Vec<String>,
    /// The handle's capture limits; the provider binds them to its checkpoint too.
    pub(super) limits: CaptureLimits,
    /// The provider's durable form of the checkpoint it issued with the verified observation.
    pub(super) provider: Vec<u8>,
    /// The last tenant position the observation verified.
    pub(super) position: u64,
}

/// A discard's marker in place of a record: never a checkpoint, and never equal to another.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Tombstone {
    authority: Authority,
    /// The framed digest of the snapshot state this tombstone replaced, so that two discards in a
    /// row, or of different records, never write equal tombstones.
    replaces: String,
    digest: String,
}

/// What an open found persisted, or what this handle last wrote.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Loaded {
    /// Nothing is persisted, or nothing was read: a `FullVerification` handle never reads it.
    Absent,
    /// Unreadable, of an unknown format or kind, or failing its digest or authority.
    Damaged,
    /// A discard's tombstone, which the record persisted at a drain must still be.
    Tombstone(Tombstone),
    /// Digest and authority hold, but another verifier, projection set or limits wrote it: not
    /// applied, but its position still says how far the store reached.
    Foreign {
        /// The position the record names.
        position: u64,
        /// The binding event the record names.
        binding: PhysicalRef,
    },
    /// A checkpoint this handle may start from.
    Valid(Record),
}

impl Loaded {
    /// The position and binding of a record whose digest and authority hold, if this is one.
    pub(super) fn reach(&self) -> Option<(u64, &PhysicalRef)> {
        match self {
            Self::Valid(record) => Some((record.position, &record.binding)),
            Self::Foreign { position, binding } => Some((*position, binding)),
            Self::Absent | Self::Damaged | Self::Tombstone(_) => None,
        }
    }
}

impl Record {
    /// A record of this handle's current observation.
    pub(super) fn new(
        authority: &Authority,
        binding: PhysicalRef,
        limits: CaptureLimits,
        provider: Vec<u8>,
        position: u64,
    ) -> Self {
        Self {
            verifier: VERIFIER.to_owned(),
            authority: authority.clone(),
            binding,
            projections: projection_names(),
            limits,
            provider,
            position,
        }
    }

    fn body(&self) -> Value {
        json!({
            "format": FORMAT,
            "kind": "checkpoint",
            "verifier": self.verifier,
            "authority": self.authority,
            "binding": self.binding,
            "projections": self.projections,
            "limits": limits_value(self.limits),
            "provider": hex(&self.provider),
            "position": self.position,
        })
    }

    /// The snapshot state: the body and its digest.
    pub(super) fn state(&self) -> Result<Value, AsyncStoreError> {
        seal(self.body())
    }

    /// Whether this record was written by this verifier for exactly this handle's projections
    /// and limits.
    fn applies(&self, limits: CaptureLimits) -> bool {
        self.verifier == VERIFIER && self.projections == projection_names() && self.limits == limits
    }
}

impl Tombstone {
    fn body(&self) -> Value {
        json!({
            "format": FORMAT,
            "kind": "discarded",
            "authority": self.authority,
            "replaces": self.replaces,
        })
    }
}

fn projection_names() -> Vec<String> {
    projection_specs()
        .iter()
        .map(|spec| spec.name.to_owned())
        .collect()
}

fn limits_value(limits: CaptureLimits) -> Value {
    json!({
        "max_events": limits.max_events,
        "max_blobs": limits.max_blobs,
        "max_projection_rows": limits.max_projection_rows,
        "max_payload_bytes": limits.max_payload_bytes,
    })
}

/// A body with its digest added.
fn seal(body: Value) -> Result<Value, AsyncStoreError> {
    let digest = key_for_value(DIGEST_DOMAIN, body.clone())?;
    let Value::Object(mut fields) = body else {
        return Err(AsyncStoreError::Encoding(
            "an open checkpoint body is an object".into(),
        ));
    };
    fields.insert("digest".to_owned(), Value::String(digest));
    Ok(Value::Object(fields))
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        text.push(char::from(DIGITS[usize::from(byte >> 4)]));
        text.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    text
}

fn unhex(text: &str) -> Option<Vec<u8>> {
    let digit = |c: u8| match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        _ => None,
    };
    let bytes = text.as_bytes();
    if bytes.len() % 2 != 0 {
        return None;
    }
    bytes
        .chunks(2)
        .map(|pair| Some(digit(pair[0])? << 4 | digit(pair[1])?))
        .collect()
}

/// Interprets one persisted state for `authority` and `limits`.
///
/// Every field is read by exact key and the digest is recomputed from the typed record, so a
/// state carrying an unknown field, a field of the wrong type or a digest of other content is
/// damaged rather than partly interpreted.
pub(super) fn classify(state: &Value, authority: &Authority, limits: CaptureLimits) -> Loaded {
    let Some(fields) = state.as_object() else {
        return Loaded::Damaged;
    };
    if fields.get("format").and_then(Value::as_str) != Some(FORMAT) {
        return Loaded::Damaged;
    }
    match fields.get("kind").and_then(Value::as_str) {
        Some("checkpoint") => match decode_record(fields) {
            Some((record, digest)) => {
                if seal(record.body())
                    .ok()
                    .and_then(|sealed| sealed.get("digest").cloned())
                    != Some(Value::String(digest))
                    || record.authority != *authority
                {
                    Loaded::Damaged
                } else if record.applies(limits) {
                    Loaded::Valid(record)
                } else {
                    Loaded::Foreign {
                        position: record.position,
                        binding: record.binding,
                    }
                }
            }
            None => Loaded::Damaged,
        },
        Some("discarded") => match decode_tombstone(fields) {
            Some(tombstone)
                if tombstone.authority == *authority
                    && seal(tombstone.body())
                        .ok()
                        .and_then(|sealed| sealed.get("digest").cloned())
                        == Some(Value::String(tombstone.digest.clone())) =>
            {
                Loaded::Tombstone(tombstone)
            }
            _ => Loaded::Damaged,
        },
        _ => Loaded::Damaged,
    }
}

fn exact<'a>(fields: &'a Map<String, Value>, keys: &[&str]) -> Option<Vec<&'a Value>> {
    if fields.len() != keys.len() {
        return None;
    }
    keys.iter().map(|key| fields.get(*key)).collect()
}

fn decode_record(fields: &Map<String, Value>) -> Option<(Record, String)> {
    let values = exact(
        fields,
        &[
            "format",
            "kind",
            "verifier",
            "authority",
            "binding",
            "projections",
            "limits",
            "provider",
            "position",
            "digest",
        ],
    )?;
    let limits = values[6].as_object()?;
    if limits.len() != 4 {
        return None;
    }
    let limit = |name: &str| limits.get(name).and_then(Value::as_u64);
    Some((
        Record {
            verifier: values[2].as_str()?.to_owned(),
            authority: serde_json::from_value(values[3].clone()).ok()?,
            binding: serde_json::from_value(values[4].clone()).ok()?,
            projections: values[5]
                .as_array()?
                .iter()
                .map(|name| name.as_str().map(str::to_owned))
                .collect::<Option<_>>()?,
            limits: CaptureLimits {
                max_events: limit("max_events")?,
                max_blobs: limit("max_blobs")?,
                max_projection_rows: limit("max_projection_rows")?,
                max_payload_bytes: limit("max_payload_bytes")?,
            },
            provider: unhex(values[7].as_str()?)?,
            position: values[8].as_u64()?,
        },
        values[9].as_str()?.to_owned(),
    ))
}

fn decode_tombstone(fields: &Map<String, Value>) -> Option<Tombstone> {
    let values = exact(
        fields,
        &["format", "kind", "authority", "replaces", "digest"],
    )?;
    Some(Tombstone {
        authority: serde_json::from_value(values[2].clone()).ok()?,
        replaces: values[3].as_str()?.to_owned(),
        digest: values[4].as_str()?.to_owned(),
    })
}

/// The one stream `authority` keeps its checkpoint on, in its own tenant.
pub(super) fn stream(
    tenant: &TenantId,
    authority: &Authority,
) -> Result<StreamId, AsyncStoreError> {
    let id = key_for_value(STREAM_KEY_DOMAIN, json!({ "authority": authority }))?;
    StreamId::new(tenant.clone(), STREAM_TYPE, id).map_err(input_eventlog)
}

/// What is persisted for `authority` now. A provider that cannot read the snapshot answers
/// [`Loaded::Damaged`]: an unreadable checkpoint is a damaged cache, never a refusal.
pub(super) async fn load(
    backend: &dyn EventlogBackend,
    tenant: &TenantId,
    authority: &Authority,
    limits: CaptureLimits,
) -> Result<Loaded, AsyncStoreError> {
    let stream = stream(tenant, authority)?;
    Ok(match backend.load_snapshot(&stream).await {
        Ok(None) => Loaded::Absent,
        Ok(Some(snapshot)) if snapshot.state_schema_version == STATE_SCHEMA => {
            classify(&snapshot.state, authority, limits)
        }
        Ok(Some(_)) | Err(_) => Loaded::Damaged,
    })
}

/// Persists `state` for `authority`, if the provider still holds the stream generation it read.
pub(super) async fn save(
    backend: &dyn EventlogBackend,
    tenant: &TenantId,
    authority: &Authority,
    state: Value,
) -> Result<bool, AsyncStoreError> {
    let stream = stream(tenant, authority)?;
    let Some(generation) = backend
        .snapshot_generation(&stream)
        .await
        .map_err(map_read_error)?
    else {
        return Ok(false);
    };
    backend
        .save_snapshot_checked(
            &stream,
            &Snapshot {
                version: 0,
                state_schema_version: STATE_SCHEMA,
                state,
                // No library here reads a clock: a checkpoint's meaning is its position.
                recorded_at: OffsetDateTime::UNIX_EPOCH,
            },
            &generation,
        )
        .await
        .map_err(map_read_error)
}

/// Replaces whatever is persisted for `authority` with a tombstone; `false` when nothing was.
pub(super) async fn discard(
    backend: &dyn EventlogBackend,
    tenant: &TenantId,
    authority: &Authority,
) -> Result<bool, AsyncStoreError> {
    let stream = stream(tenant, authority)?;
    let Some(snapshot) = backend
        .load_snapshot(&stream)
        .await
        .map_err(map_read_error)?
    else {
        return Ok(false);
    };
    let replaces = key_for_value(REPLACED_DOMAIN, snapshot.state)?;
    let body = Tombstone {
        authority: authority.clone(),
        replaces,
        digest: String::new(),
    }
    .body();
    if save(backend, tenant, authority, seal(body)?).await? {
        Ok(true)
    } else {
        Err(AsyncStoreError::Backend(
            "the open checkpoint's snapshot generation moved while it was discarded".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIMITS: CaptureLimits = CaptureLimits {
        max_events: 10,
        max_blobs: 20,
        max_projection_rows: 30,
        max_payload_bytes: 40,
    };

    fn authority() -> Authority {
        Authority {
            logical_scope: "scope".into(),
            tenant: "tenant".into(),
            stream_identity: "identity".into(),
        }
    }

    fn record() -> Record {
        Record::new(
            &authority(),
            PhysicalRef {
                event_id: "binding-event".into(),
                global_seq: 1,
                stream_id: "singleton".into(),
                stream_version: 1,
            },
            LIMITS,
            vec![0x00, 0x7f, 0xff, 0x10],
            42,
        )
    }

    /// The reference vector of the wire form, rendered canonically: a change to any field, its
    /// spelling or the digest construction changes these bytes. The verifier is fixed here so that
    /// a release does not move the vector; a record this verifier writes carries its own version.
    #[test]
    fn a_checkpoint_record_has_one_wire_form_and_digest() {
        let reference = Record {
            verifier: "reference".into(),
            ..record()
        };
        let state = reference.state().expect("a record seals");
        assert_eq!(
            String::from_utf8(
                crate::encoding::canonical_value_bytes(state.clone()).expect("canonical")
            )
            .expect("utf-8"),
            concat!(
                r#"{"authority":{"logical_scope":"scope","stream_identity":"identity","tenant":"tenant"},"#,
                r#""binding":{"event_id":"binding-event","global_seq":1,"stream_id":"singleton","stream_version":1},"#,
                r#""digest":"sha256:1a1fd81af5a7e6932efad0d0a69d5fdeccd0b4def6c739ea0d28c2140cfe90c0","format":"er.eventlog.open-checkpoint/1","kind":"checkpoint","#,
                r#""limits":{"max_blobs":20,"max_events":10,"max_payload_bytes":40,"max_projection_rows":30},"#,
                r#""position":42,"projections":["er_binding_v1","er_records_v1","er_batches_v1","er_subjects_v1"],"#,
                r#""provider":"007fff10","verifier":"reference"}"#
            )
        );
        assert_eq!(
            classify(&state, &authority(), LIMITS),
            Loaded::Foreign {
                position: 42,
                binding: record().binding
            },
            "a record of another verifier is never applied"
        );
        let current = record().state().expect("a record seals");
        assert_eq!(
            classify(&current, &authority(), LIMITS),
            Loaded::Valid(record())
        );
    }

    #[test]
    fn a_record_changed_without_its_digest_is_damaged_and_another_verifier_is_foreign() {
        let state = record().state().expect("a record seals");
        let mut moved = state.clone();
        moved["position"] = json!(43);
        assert_eq!(classify(&moved, &authority(), LIMITS), Loaded::Damaged);
        let mut extra = state.clone();
        extra["note"] = json!("unknown field");
        assert_eq!(classify(&extra, &authority(), LIMITS), Loaded::Damaged);
        let mut other = authority();
        other.stream_identity = "another-generation".into();
        assert_eq!(classify(&state, &other, LIMITS), Loaded::Damaged);
        let older = Record {
            verifier: "0.0.0-older".into(),
            ..record()
        };
        assert_eq!(
            classify(&older.state().expect("seals"), &authority(), LIMITS),
            Loaded::Foreign {
                position: 42,
                binding: record().binding
            }
        );
        assert_eq!(
            classify(
                &state,
                &authority(),
                CaptureLimits {
                    max_events: 11,
                    ..LIMITS
                }
            ),
            Loaded::Foreign {
                position: 42,
                binding: record().binding
            }
        );
    }

    #[test]
    fn tombstones_of_different_replaced_states_differ() {
        let seal_tombstone = |replaced: Value| {
            let body = Tombstone {
                authority: authority(),
                replaces: key_for_value(REPLACED_DOMAIN, replaced).expect("digest"),
                digest: String::new(),
            }
            .body();
            classify(&seal(body).expect("seals"), &authority(), LIMITS)
        };
        let first = seal_tombstone(record().state().expect("seals"));
        let second = seal_tombstone(json!({"anything":"else"}));
        assert!(matches!(first, Loaded::Tombstone(_)), "{first:?}");
        assert!(matches!(second, Loaded::Tombstone(_)), "{second:?}");
        assert_ne!(first, second);
    }

    #[test]
    fn provider_bytes_round_trip_through_hex_and_refuse_other_text() {
        let bytes: Vec<u8> = (0..=255).collect();
        assert_eq!(unhex(&hex(&bytes)), Some(bytes));
        assert_eq!(unhex("0"), None);
        assert_eq!(unhex("0G"), None);
        assert_eq!(unhex("AB"), None);
    }
}
