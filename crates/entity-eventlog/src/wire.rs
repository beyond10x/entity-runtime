use entity_core::EntityInstance;
use entity_store::{Expect, RecordedCommit, RecordedObservation, StoreError};
use eventlog_core::{RecordedEvent, StreamId};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, sync::Arc};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
enum Format {
    #[serde(rename = "entity-eventlog/1")]
    V1,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StoredRecord {
    format: Format,
    pub payload: Payload,
}

impl StoredRecord {
    pub fn new(payload: Payload) -> Self {
        Self {
            format: Format::V1,
            payload,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "record",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub(crate) enum Payload {
    Decision(Box<RecordedCommit>),
    Observation(RecordedObservation),
}

impl Payload {
    pub fn subject(&self) -> (&str, &str) {
        match self {
            Self::Decision(c) => (&c.instance.entity, &c.instance.id),
            Self::Observation(o) => (&o.entity, &o.id),
        }
    }
    pub fn record_id(&self) -> &str {
        match self {
            Self::Decision(c) => &c.envelope.record_id,
            Self::Observation(o) => &o.envelope.record_id,
        }
    }
    pub fn recorded_at(&self) -> &str {
        match self {
            Self::Decision(c) => &c.envelope.recorded_at,
            Self::Observation(o) => &o.envelope.recorded_at,
        }
    }
    pub fn validate(&self) -> Result<(), StoreError> {
        match self {
            Self::Decision(c) => c.validate(),
            Self::Observation(o) => o.validate(),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct IdentityClaim {
    pub digest: String,
    pub stream: StreamId,
    pub version: u64,
}

#[derive(Clone)]
pub(crate) struct Subject {
    pub head: u64,
    pub encoded_bytes: usize,
    pub history: Arc<entity_store::VerifiedHistory>,
    pub observations: Arc<Vec<RecordedObservation>>,
    identities: Arc<BTreeSet<String>>,
}

impl Subject {
    pub fn new(entity: &str, id: &str) -> Self {
        Self {
            head: 0,
            encoded_bytes: 0,
            history: Arc::new(entity_store::VerifiedHistory::new(entity, id)),
            observations: Arc::new(Vec::new()),
            identities: Arc::new(BTreeSet::new()),
        }
    }
    pub fn instance(&self) -> Option<&EntityInstance> {
        self.history.instance()
    }
    pub fn fold(&mut self, payload: Payload) -> Result<(), StoreError> {
        payload.validate()?;
        if !Arc::make_mut(&mut self.identities).insert(payload.record_id().into()) {
            return Err(invalid("history repeats a record id"));
        }
        match payload {
            Payload::Decision(commit) => {
                Arc::make_mut(&mut self.history).append(commit.envelope)?;
            }
            Payload::Observation(observation) => {
                entity_store::check(
                    &observation.entity,
                    &observation.id,
                    Expect::Revision(observation.revision),
                    self.instance().map(|i| i.revision),
                )?;
                Arc::make_mut(&mut self.observations).push(observation);
            }
        }
        Ok(())
    }
}

pub(crate) fn decode(event: &RecordedEvent) -> Result<StoredRecord, StoreError> {
    if event.is_redacted() || event.name != "entity.record" || event.schema_version != 1 {
        return Err(invalid(
            "entity history has a redacted or unsupported record",
        ));
    }
    serde_json::from_value(event.data.clone()).map_err(invalid)
}

pub(crate) fn invalid(error: impl std::fmt::Display) -> StoreError {
    StoreError::Backend(error.to_string())
}
