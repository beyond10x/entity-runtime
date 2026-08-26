//! The shape around an event that a log needs and the kernel must not invent.
//!
//! A [`DomainEvent`] is the domain fact: *this ticket was closed, at this revision*. It carries no
//! event id, no time, no correlation, no causation and no actor — deliberately, because the kernel
//! has no clock and no id generator (R-01) and a kernel that invented either would produce a
//! different `Decision` for identical inputs.
//!
//! Those five belong to whoever recorded the event, which is the shell. This is the reference shape
//! for them, so two shells do not each invent a different one.
//!
//! # Correlation is not causation
//!
//! The single most common way this shape is got wrong is to keep one field and call it either name.
//! They answer different questions:
//!
//! | field | question | across a five-step flow |
//! |---|---|---|
//! | `correlation` | *what larger thing was this part of?* | the **same** value on all five |
//! | `causation` | *what immediately led to this?* | a **different** value on each — the step before it |
//!
//! With only correlation you can gather a flow but not order it or find where it forked. With only
//! causation you can walk one chain backwards but cannot ask *what else happened because of this
//! request*. Both, and "why did this happen?" is answerable; either alone and it is a guess from a
//! timestamp.
//!
//! # `actor` is written, never defaulted
//!
//! `None` means *nothing human caused this* — a scheduled job, a replay, a reconciliation — and it
//! is a real answer worth recording. What is refused is the field being **absent**: a key nobody
//! wrote would let an unattributed event read exactly like one whose actor was known and simply not
//! serialised, and the difference is the whole value of the field in an audit.

use serde::{Deserialize, Serialize};

use entity_core::DomainEvent;

/// What the shell knows about an event and the kernel cannot.
///
/// Generic over the event so a shell that has extended the domain fact still gets one envelope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Envelope<T> {
    /// This event's own identity, unique in the log it is appended to.
    pub event_id: String,
    /// When it was recorded, ISO-8601. The recorder's clock, read at the edge and passed in.
    pub recorded_at: String,
    /// The flow this was part of. The same value on every event of one flow.
    pub correlation: String,
    /// What immediately led to this. A different value per step; the event or command before it.
    pub causation: String,
    /// Who caused it, or `None` when nothing human did.
    ///
    /// Never skipped when serialising, and **required** when parsing: an absent key and an
    /// explicit `null` would otherwise read alike, and only one of them is a claim.
    ///
    /// `deserialize_with` is load-bearing rather than decoration. Serde's derive treats a missing
    /// `Option` field as `None` — so without this, a document that never mentioned an actor would
    /// parse as one asserting that nothing human caused the event. Forcing the call makes the key
    /// required, and the test `an_envelope_missing_a_field_is_refused_rather_than_defaulted` caught
    /// this the first time it ran.
    #[serde(deserialize_with = "Option::deserialize")]
    pub actor: Option<String>,
    /// The domain fact itself, exactly as the kernel produced it.
    pub event: T,
}

impl<T> Envelope<T> {
    /// Seals an event with everything the shell knows about it.
    pub fn new(
        event: T,
        event_id: impl Into<String>,
        recorded_at: impl Into<String>,
        correlation: impl Into<String>,
        causation: impl Into<String>,
        actor: Option<String>,
    ) -> Self {
        Self {
            event_id: event_id.into(),
            recorded_at: recorded_at.into(),
            correlation: correlation.into(),
            causation: causation.into(),
            actor,
            event,
        }
    }

    /// `true` when this event is the first of its flow — nothing before it caused it.
    ///
    /// The one case where the two values legitimately coincide, and worth being able to ask about
    /// rather than inferring from equality at each call site.
    #[must_use]
    pub fn starts_its_flow(&self) -> bool {
        self.correlation == self.causation
    }
}

/// What a shell knows about a whole `Decision`, before its events are sealed one by one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recording {
    /// When, ISO-8601. Read at the edge.
    pub recorded_at: String,
    /// The flow every event of this decision belongs to.
    pub correlation: String,
    /// What led to the operation that produced them.
    pub causation: String,
    /// Who asked, or `None`.
    pub actor: Option<String>,
}

impl Recording {
    /// Seals every event of a decision, giving each a derived identity.
    ///
    /// The identity is `<entity>:<id>@<revision>#<index>` — the event's own coordinates, which are
    /// already unique because a revision is reached once and an index is a position within it. It
    /// needs no random source and no clock, so two runs over the same decision produce the same
    /// ids, which is what lets a test assert on them and a replay recognise what it has already
    /// seen.
    ///
    /// A shell that needs opaque ids builds [`Envelope::new`] itself; this is the default, not the
    /// only way.
    #[must_use]
    pub fn seal(&self, events: &[DomainEvent]) -> Vec<Envelope<DomainEvent>> {
        events
            .iter()
            .enumerate()
            .map(|(index, event)| {
                Envelope::new(
                    event.clone(),
                    derived_id(event, index),
                    self.recorded_at.clone(),
                    self.correlation.clone(),
                    self.causation.clone(),
                    self.actor.clone(),
                )
            })
            .collect()
    }
}

/// `<entity>:<id>@<revision>#<index>` — an event's coordinates, which are already unique.
#[must_use]
pub fn derived_id(event: &DomainEvent, index: usize) -> String {
    format!("{}:{}@{}#{index}", event.entity, event.id, event.revision)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(revision: u64, event_type: &str) -> DomainEvent {
        DomainEvent {
            entity: "ticket".to_owned(),
            version: 1,
            id: "one".to_owned(),
            revision,
            event_type: event_type.to_owned(),
            from_state: Some("open".to_owned()),
            to_state: "closed".to_owned(),
            changed: serde_json::Map::new(),
            payload: serde_json::json!({}),
        }
    }

    fn recording() -> Recording {
        Recording {
            recorded_at: "2026-08-26T00:00:00Z".to_owned(),
            correlation: "flow-1".to_owned(),
            causation: "command-7".to_owned(),
            actor: Some("timo".to_owned()),
        }
    }

    #[test]
    fn every_event_of_one_decision_shares_a_correlation_and_gets_its_own_id() {
        let sealed = recording().seal(&[event(2, "TicketClosed"), event(2, "TicketArchived")]);

        assert_eq!(sealed.len(), 2);
        assert_eq!(sealed[0].correlation, sealed[1].correlation, "one flow");
        assert_ne!(sealed[0].event_id, sealed[1].event_id, "two events");
        assert_eq!(sealed[0].event_id, "ticket:one@2#0");
        assert_eq!(sealed[1].event_id, "ticket:one@2#1");
    }

    #[test]
    fn sealing_the_same_decision_twice_produces_the_same_ids() {
        // No clock and no random source, so a test can assert on an id and a replay can recognise
        // an event it has already seen.
        let events = [event(2, "TicketClosed")];
        assert_eq!(recording().seal(&events), recording().seal(&events));
    }

    #[test]
    fn correlation_and_causation_are_separate_fields_and_a_flow_start_says_so() {
        let mid_flow = recording().seal(&[event(2, "TicketClosed")]);
        assert_ne!(mid_flow[0].correlation, mid_flow[0].causation);
        assert!(!mid_flow[0].starts_its_flow());

        let first = Recording {
            causation: "flow-1".to_owned(),
            ..recording()
        }
        .seal(&[event(1, "TicketOpened")]);
        assert!(
            first[0].starts_its_flow(),
            "the one case where they coincide: nothing before it caused it"
        );
    }

    #[test]
    fn an_absent_actor_serialises_as_null_rather_than_disappearing() {
        // An absent key and an explicit null would otherwise read alike, and only one of them is a
        // claim. Nobody-caused-this is a real answer worth recording.
        let sealed = Recording {
            actor: None,
            ..recording()
        }
        .seal(&[event(2, "TicketClosed")]);

        let json = serde_json::to_value(&sealed[0]).expect("serialises");
        assert!(json.as_object().expect("an object").contains_key("actor"));
        assert!(json["actor"].is_null());
    }

    #[test]
    fn an_envelope_round_trips_and_refuses_a_field_it_does_not_know() {
        let sealed = recording().seal(&[event(2, "TicketClosed")]);
        let text = serde_json::to_string(&sealed[0]).expect("serialises");
        let back: Envelope<DomainEvent> = serde_json::from_str(&text).expect("parses");
        assert_eq!(back, sealed[0]);

        let mut value = serde_json::to_value(&sealed[0]).expect("serialises");
        value["invented"] = serde_json::json!("something");
        serde_json::from_value::<Envelope<DomainEvent>>(value)
            .expect_err("a key this build does not know is refused rather than dropped");
    }

    #[test]
    fn an_envelope_missing_a_field_is_refused_rather_than_defaulted() {
        // Including `actor`: a field somebody forgot must not be able to claim that nothing human
        // caused the event.
        for missing in [
            "event_id",
            "recorded_at",
            "correlation",
            "causation",
            "actor",
            "event",
        ] {
            let sealed = recording().seal(&[event(2, "TicketClosed")]);
            let mut value = serde_json::to_value(&sealed[0]).expect("serialises");
            value.as_object_mut().expect("an object").remove(missing);
            assert!(
                serde_json::from_value::<Envelope<DomainEvent>>(value).is_err(),
                "an envelope without `{missing}` must be refused, not defaulted"
            );
        }
    }
}
