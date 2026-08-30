//! Caller-supplied provenance around a durable record.

use entity_core::is_valid_timestamp;
use serde::{Deserialize, Serialize};
use std::fmt;

/// A durable record together with the provenance known only at the IO edge.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Envelope<T> {
    /// Caller-supplied idempotency identity.
    pub record_id: String,
    /// When the record was persisted, in a validated ISO-8601 form.
    pub recorded_at: String,
    /// The wider flow, when there is one.
    #[serde(deserialize_with = "Option::deserialize")]
    pub correlation: Option<String>,
    /// The immediately preceding record or command, when there is one.
    #[serde(deserialize_with = "Option::deserialize")]
    pub causation: Option<String>,
    /// Who caused it, or explicit `null` when no actor did.
    #[serde(deserialize_with = "Option::deserialize")]
    pub actor: Option<String>,
    /// The record itself.
    pub record: T,
}

impl<T> Envelope<T> {
    /// Builds and validates an envelope.
    ///
    /// # Errors
    ///
    /// [`EnvelopeError`] when the record id is blank, the timestamp is unreadable, or an optional
    /// identity was supplied as an empty string.
    pub fn new(
        record: T,
        record_id: impl Into<String>,
        recorded_at: impl Into<String>,
        correlation: Option<String>,
        causation: Option<String>,
        actor: Option<String>,
    ) -> Result<Self, EnvelopeError> {
        let envelope = Self {
            record_id: record_id.into(),
            recorded_at: recorded_at.into(),
            correlation,
            causation,
            actor,
            record,
        };
        envelope.validate()?;
        Ok(envelope)
    }

    /// Checks the provenance fields without inspecting the generic record.
    ///
    /// # Errors
    ///
    /// [`EnvelopeError`] naming the invalid field.
    pub fn validate(&self) -> Result<(), EnvelopeError> {
        if self.record_id.trim().is_empty() {
            return Err(EnvelopeError("record_id cannot be empty".to_owned()));
        }
        if !is_valid_timestamp(&self.recorded_at) {
            return Err(EnvelopeError(format!(
                "recorded_at {:?} is not a valid runtime timestamp",
                self.recorded_at
            )));
        }
        for (name, value) in [
            ("correlation", self.correlation.as_deref()),
            ("causation", self.causation.as_deref()),
            ("actor", self.actor.as_deref()),
        ] {
            if value.is_some_and(|value| value.trim().is_empty()) {
                return Err(EnvelopeError(format!(
                    "{name} cannot be empty; use null when it is absent"
                )));
            }
        }
        Ok(())
    }

    /// Whether this is explicitly the first record in a named flow.
    #[must_use]
    pub fn starts_its_flow(&self) -> bool {
        matches!(
            (&self.correlation, &self.causation),
            (Some(correlation), Some(causation)) if correlation == causation
        )
    }
}

/// Why caller-supplied recording metadata was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvelopeError(String);

impl fmt::Display for EnvelopeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for EnvelopeError {}

/// Provenance a shell supplies for one complete decision record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recording {
    /// Caller-supplied retry identity.
    pub record_id: String,
    /// When this was recorded.
    pub recorded_at: String,
    /// The wider flow, when any.
    pub correlation: Option<String>,
    /// The immediate cause, when any.
    pub causation: Option<String>,
    /// The actor, or no actor explicitly.
    pub actor: Option<String>,
}

impl Recording {
    /// Seals one complete record.
    ///
    /// # Errors
    ///
    /// Invalid provenance as described by [`Envelope::new`].
    pub fn seal<T>(&self, record: T) -> Result<Envelope<T>, EnvelopeError> {
        Envelope::new(
            record,
            self.record_id.clone(),
            self.recorded_at.clone(),
            self.correlation.clone(),
            self.causation.clone(),
            self.actor.clone(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recording() -> Recording {
        Recording {
            record_id: "record-1".to_owned(),
            recorded_at: "2026-08-31T12:00:00Z".to_owned(),
            correlation: Some("flow-1".to_owned()),
            causation: Some("command-1".to_owned()),
            actor: None,
        }
    }

    #[test]
    fn an_envelope_round_trips_with_explicit_absence() {
        let envelope = recording()
            .seal(serde_json::json!({"ok": true}))
            .expect("valid");
        let value = serde_json::to_value(&envelope).expect("serializes");
        assert!(value.get("actor").is_some_and(serde_json::Value::is_null));
        let back: Envelope<serde_json::Value> =
            serde_json::from_value(value).expect("deserializes");
        assert_eq!(back, envelope);
    }

    #[test]
    fn invalid_time_and_blank_identity_are_refused() {
        let mut invalid = recording();
        invalid.recorded_at = "2026-02-31T00:00:00Z".to_owned();
        assert_eq!(
            invalid.seal(()).expect_err("invalid"),
            EnvelopeError(
                "recorded_at \"2026-02-31T00:00:00Z\" is not a valid runtime timestamp".to_owned()
            )
        );
        invalid = recording();
        invalid.record_id = " ".to_owned();
        assert_eq!(
            invalid.seal(()).expect_err("invalid"),
            EnvelopeError("record_id cannot be empty".to_owned())
        );
    }

    #[test]
    fn an_envelope_missing_an_optional_key_is_refused_rather_than_defaulted() {
        let value = serde_json::json!({
            "record_id": "record-1",
            "recorded_at": "2026-08-31T12:00:00Z",
            "causation": null,
            "actor": null,
            "record": {"ok": true}
        });
        let error = serde_json::from_value::<Envelope<serde_json::Value>>(value)
            .expect_err("missing correlation is not an assertion of absence");
        assert!(error.to_string().contains("correlation"), "{error}");
    }
}
