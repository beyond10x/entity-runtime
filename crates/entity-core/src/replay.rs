//! Rebuilding an instance from the events that describe it.
//!
//! R-81 says the model is compatible with state persistence **and** event sourcing. This is the
//! second half: a shell that keeps the events as the record can fold them back into the instance,
//! and treat the stored instance as a cache it may throw away.
//!
//! # The rule this must not break
//!
//! R-34: `lifecycle_state` is written by `create` and `execute` and by nothing else. A fold plainly
//! *does* set a state, so the question is what stops it being a second way in — a caller who wants
//! an instance `closed` handing over an invented `TicketClosed` event.
//!
//! Two things, together:
//!
//! 1. **The state comes from the event, not from the caller's intent.** A `DomainEvent` carries
//!    `from_state` and `to_state` because the kernel wrote them when it permitted the operation.
//! 2. **Every step is re-checked against the definition.** A fold refuses an event whose transition
//!    the lifecycle does not declare, whose `from_state` is not where the fold had got to, or whose
//!    revision does not follow. An invented event has to be a transition the definition already
//!    permits, from the state the instance is already in — which is exactly what `execute` would
//!    have allowed anyway.
//!
//! So a fold is not a way to reach a state that could not have been reached. It is a slower way to
//! reach one that could.
//!
//! # Why a creation event is required
//!
//! A fold has to start somewhere, and the only honest start is the event that says the instance
//! came into being. A definition that emits nothing on creation cannot be event-sourced, and this
//! says so by name rather than inventing an empty instance to fold onto — an instance conjured from
//! no record would be the fold asserting something no event supports.

use serde_json::Map;

use crate::definition::EntityDefinition;
use crate::error::CoreError;
use crate::runtime::{DomainEvent, EntityInstance};
use crate::validation::validate_object;

/// Rebuilds an instance from its events, oldest first.
///
/// # Errors
///
/// [`CoreError::Validation`] naming the event that broke the chain: an empty history, a first event
/// that is not a creation, a revision that does not follow, a `from_state` that is not where the
/// fold had reached, or a transition the definition does not declare.
///
/// [`CoreError::EntityMismatch`] when an event belongs to another definition, and
/// [`CoreError::UnknownState`] when it names a state the definition does not have.
pub fn rehydrate(
    definition: &EntityDefinition,
    events: &[DomainEvent],
) -> Result<EntityInstance, CoreError> {
    let refuse = |detail: String| {
        Err(CoreError::Validation(vec![crate::ValidationError::new(
            "events", detail,
        )]))
    };

    let Some(first) = events.first() else {
        return refuse(
            "an instance cannot be rebuilt from no events; a fold has to start from the event that \
             says the instance came into being"
                .to_owned(),
        );
    };

    if first.from_state.is_some() {
        return refuse(format!(
            "the first event is `{}`, which moved from `{}` — a history must begin with a creation \
             event, and a definition that emits none on creation cannot be event-sourced", first.event_type, first.from_state.as_deref().unwrap_or_default()
        ));
    }

    let mut instance = EntityInstance {
        entity: definition.entity.clone(),
        version: definition.version,
        id: first.id.clone(),
        lifecycle_state: definition.lifecycle.initial.clone(),
        revision: 0,
        fields: Map::new(),
    };

    for (index, event) in events.iter().enumerate() {
        if event.entity != definition.entity || event.version != definition.version {
            return Err(CoreError::EntityMismatch {
                expected_entity: definition.entity.clone(),
                expected_version: definition.version,
                actual_entity: event.entity.clone(),
                actual_version: event.version,
            });
        }
        if event.id != instance.id {
            return refuse(format!(
                "event {index} (`{}`) is about `{}`, not `{}`; one history describes one instance",
                event.event_type, event.id, instance.id
            ));
        }
        if !definition.lifecycle.states.contains(&event.to_state) {
            return Err(CoreError::UnknownState {
                entity: definition.entity.clone(),
                state: event.to_state.clone(),
            });
        }

        // A revision is reached once and follows the one before it. A gap means events are missing,
        // and folding over a gap would produce an instance nothing ever was.
        let expected_revision = instance.revision + 1;
        if event.revision != expected_revision {
            return refuse(format!(
                "event {index} (`{}`) is at revision {}, but the fold had reached {}; a history \
                 with a gap rebuilds an instance that never existed",
                event.event_type, event.revision, instance.revision
            ));
        }

        match &event.from_state {
            // A creation event is not exempt from the lifecycle — it is held to the one state the
            // definition says an instance begins in. Without this, a fold accepts a forged
            // creation straight into any state `states` happens to contain, which is precisely the
            // thing `execute` cannot do: `create` always enters `lifecycle.initial`.
            None if index == 0 => {
                if event.to_state != definition.lifecycle.initial {
                    return refuse(format!(
                        "event 0 (`{}`) creates `{}` in `{}`, but an instance begins in `{}`; a creation that enters any other state rebuilds an instance `create` would never have produced", event.event_type, definition.entity, event.to_state, definition.lifecycle.initial
                    ));
                }
            }
            None => {
                return refuse(format!(
                    "event {index} (`{}`) claims no previous state, but only the first event of a \
                     history can be a creation",
                    event.event_type
                ))
            }
            Some(from) => {
                if from != &instance.lifecycle_state {
                    return refuse(format!(
                        "event {index} (`{}`) moves from `{from}`, but the fold is at `{}`",
                        event.event_type, instance.lifecycle_state
                    ));
                }
                if !permits(definition, from, &event.to_state) {
                    return refuse(format!(
                        "event {index} (`{}`) moves `{from}` -> `{}`, which no operation of \
                         `{}` declares; replaying it would reach a state the definition never \
                         permitted",
                        event.event_type, event.to_state, definition.entity
                    ));
                }
            }
        }

        instance.lifecycle_state = event.to_state.clone();
        instance.revision = event.revision;
        for (name, value) in &event.changed {
            instance.fields.insert(name.clone(), value.clone());
        }
    }

    // The fields an event carries are data like any other, and a fold that installed them
    // unchecked would rebuild an instance the schema refuses — a field of the wrong type, or one
    // the definition never declared. `execute` validates what it writes; so does replay.
    let defects = validate_object(&definition.schema, &instance.fields, "$fields");
    if !defects.is_empty() {
        let detail = defects
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("; ");
        return refuse(format!(
            "the events fold to an instance the schema refuses: {detail}"
        ));
    }

    Ok(instance)
}

/// Whether any operation of `definition` declares a transition from `from` to `to`.
fn permits(definition: &EntityDefinition, from: &str, to: &str) -> bool {
    definition.operations.values().any(|operation| {
        operation.transitions.iter().any(|transition| {
            transition.from.as_slice().iter().any(|state| state == from) && transition.to == to
        })
    })
}
