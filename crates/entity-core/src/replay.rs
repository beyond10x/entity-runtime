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
//! Five things, together:
//!
//! 1. **The state comes from the event, not from the caller's intent.** A `DomainEvent` carries
//!    `from_state` and `to_state` because the kernel wrote them when it permitted the operation.
//! 2. **Every step is re-checked against the definition.** A fold refuses an event whose transition
//!    the lifecycle does not declare, whose `from_state` is not where the fold had got to, whose
//!    revision does not follow, or whose type no operation of the definition emits on the
//!    transition it took. An invented event has to be a transition the definition already permits,
//!    from the state the instance is already in, carrying a type something there actually emits —
//!    which is exactly what `execute` would have allowed anyway.
//! 3. **Every step is held to the entity's invariants.** After each event's state, revision and
//!    fields are installed — the creation event included, because `create` checks them too — the
//!    definition's invariants are evaluated against what the fold now holds, exactly as `create`
//!    and `execute` evaluate them against what they wrote. Without this an honest history recorded
//!    before an invariant was added folds to a `closed` ticket with no resolution: an instance the
//!    invariant exists to forbid.
//! 4. **Every event's fields are the ones its own operation would have written, and an event
//!    nothing would have written is refused.** An operation event carries both the arguments it was
//!    decided on and the fields it wrote, and the two have to agree: the fold resolves the emitting
//!    operation's `set:` against the fields as they stood and the arguments the event records, and
//!    refuses an event whose `changed` is anything else. Without this a `close` decided on
//!    `resolution: fixed` folds to a ticket resolved `not-fixed` — a value no `execute` on that
//!    command could have produced, and one no precondition, schema or invariant has any reason to
//!    object to. An event no operation emits on its transition has no such operation to answer for
//!    it, so it is refused outright rather than admitted on the strength of the transition alone.
//! 5. **A revision is one decision and holds every event it emitted, payloads included.** One
//!    `execute` call materialises the whole `emits` list at one revision, from one context, so the
//!    fold walks the history a revision at a time: the events of a revision must agree on transition,
//!    arguments and fields, must be exactly the emitting operation's `emits` in order — a subset, a
//!    superset or a reordering was written by no single call — and each payload must be what its
//!    template resolves to against the fields afterwards. In a definition whose operations have no
//!    `set:`, the payload is the only place a decision's particulars are written down.
//!
//! So a fold is not a way to reach a state that could not have been reached. It is a slower way to
//! reach one that could.
//!
//! # The arguments and the fields are checked together
//!
//! An event carries `args` — what the rules read when the operation was permitted — and `changed`,
//! the fields the operation wrote. A fold re-asks both questions of **one** operation: among those
//! that declare the event's transition *and* emit its type, one has to both accept the event's
//! arguments under its preconditions and write exactly the event's `changed` when its `set:` is
//! resolved against those arguments. Asking the two questions separately would accept a history no
//! single `execute` call could have written — one operation's arguments beside another's fields.
//!
//! Without the first half a forged event carries `test_result: 0` into `implemented`. Without the
//! second, `changed` answers to nothing but the schema, so a `close` decided on
//! `resolution: fixed` folds to a ticket resolved `not-fixed`.
//!
//! A creation event is the same question with no operation in it: `create` records the instance's
//! fields after defaults as both `args` and `changed`, so a fold requires the two to be equal, and
//! its type has to be the one `create.emit` names.
//!
//! Where no operation both declares the transition and emits the event's type there is no operation
//! to put the question to, and the fold refuses the event rather than admitting it on the strength
//! of the transition alone: an event nothing emits describes no decision the kernel took, so there
//! is nothing to hold its fields to and the schema alone would decide what they may be. A creation
//! event is refused the same way when its type is not the one `create.emit` names — and when the
//! definition emits nothing on creation, every creation event of it is invented and no history of
//! it can begin.
//!
//! # What the fold cannot see
//!
//! An operation that emits nothing leaves no event, so a history folded from events alone does not
//! know it ran. When that silent decision was the last one, the fold returns the instance as the
//! events describe it — one decision short, with nothing to refuse. When a later decision did emit,
//! its revision no longer follows the one the fold reached, and the fold refuses it as a gap: the
//! history is unfoldable from that point, and the refusal names the revision it stopped at. That is
//! a property of event-only history, not of this fold — `replay` over complete decision records has
//! no such gap — and the guide says so beside `emits`.
//!
//! # Why a creation event is required
//!
//! A fold has to start somewhere, and the only honest start is the event that says the instance
//! came into being. A definition that emits nothing on creation cannot be event-sourced, and this
//! says so by name rather than inventing an empty instance to fold onto — an instance conjured from
//! no record would be the fold asserting something no event supports.

use serde_json::{Map, Value};

use crate::definition::OperationDefinition;
use crate::error::CoreError;
use crate::runtime::{
    canonical_object, canonicalize, changed_fields, check_invariants, check_preconditions, create,
    execute, resolve_template, DecisionCommand, DecisionRecord, DomainEvent, EntityInstance,
    TemplateContext,
};
use crate::validation::validate_object;
use crate::ValidatedDefinition;

/// Replays complete decision records and verifies every byte-producing kernel choice.
///
/// Unlike legacy event folding, this reruns defaults, schemas, transition selection,
/// preconditions, `set`, invariants and event templates. Recorded `changed` fields and event types
/// are comparison evidence, never instructions trusted to mutate state.
///
/// # Errors
///
/// A typed kernel refusal when the recorded command no longer evaluates, or
/// [`CoreError::Validation`] naming the first record that differs from recomputation.
pub fn replay(records: &[DecisionRecord]) -> Result<EntityInstance, CoreError> {
    let mut verified = VerifiedReplay::default();
    for record in records {
        verified.advance(record)?;
    }
    verified.instance.ok_or_else(|| {
        CoreError::Validation(vec![crate::ValidationError::new(
            "records[0]",
            "an instance cannot be replayed from no decision records",
        )])
    })
}

/// An incrementally verified decision prefix, constructed only through ordinary kernel replay.
///
/// Its state cannot be supplied or changed directly. A caller may reuse this value for an
/// unchanged history prefix and verify only appended records. Storage owns proof that the prefix
/// is unchanged; this value deliberately knows nothing about persistence or caches.
#[derive(Debug, Clone, Default)]
pub struct VerifiedReplay {
    instance: Option<EntityInstance>,
    count: usize,
}

impl VerifiedReplay {
    /// The state recomputed from the accepted prefix, absent before its creation record.
    #[must_use]
    pub fn instance(&self) -> Option<&EntityInstance> {
        self.instance.as_ref()
    }

    /// Verifies one next record, preserving this prefix unchanged on refusal.
    ///
    /// # Errors
    /// The same indexed refusal as [`replay`] for a malformed or altered decision.
    pub fn advance(&mut self, record: &DecisionRecord) -> Result<(), CoreError> {
        let index = self.count;
        let instance = &self.instance;
        let refuse = |index: usize, detail: &str| {
            CoreError::Validation(vec![crate::ValidationError::new(
                format!("records[{index}]"),
                detail,
            )])
        };
        let definition = record
            .definition
            .clone()
            .ok_or_else(|| {
                refuse(
                    index,
                    "legacy import has no definition snapshot and cannot be replayed from genesis",
                )
            })
            .and_then(|definition| ValidatedDefinition::new(definition).map_err(CoreError::from))?;
        let decision =
            match &record.command {
                DecisionCommand::Create { fields } if instance.is_none() => create(
                    &definition,
                    record.id.clone(),
                    serde_json::Value::Object(fields.clone()),
                )?,
                DecisionCommand::Create { .. } => {
                    return Err(refuse(index, "creation may only be the first record"))
                }
                DecisionCommand::Execute {
                    operation,
                    arguments,
                } => {
                    let before = instance.as_ref().ok_or_else(|| {
                        refuse(index, "history begins with an operation, not creation")
                    })?;
                    execute(
                        &definition,
                        before,
                        operation,
                        serde_json::Value::Object(arguments.clone()),
                    )?
                }
                DecisionCommand::LegacyImport => return Err(refuse(
                    index,
                    "legacy import marks a snapshot boundary and cannot be replayed from genesis",
                )),
            };
        if decision.record != *record {
            return Err(refuse(
                index,
                "record differs from the decision recomputed from its definition and command",
            ));
        }
        self.instance = Some(decision.instance);
        self.count += 1;
        Ok(())
    }
}

/// Rebuilds an instance from its events, oldest first.
///
/// # Errors
///
/// [`CoreError::Validation`] naming the event that broke the chain: an empty history, a first event
/// that is not a creation, a revision that does not follow, a `from_state` that is not where the
/// fold had reached, a transition the definition does not declare, a type no operation emits on
/// that transition, arguments the emitting operation's argument schema or preconditions would have
/// refused, a `changed` that is not what that operation's `set:` would have written from those
/// arguments — on a creation event, a type the definition does not emit on creation, any creation
/// event at all when it emits none, or a `changed` that is not its own recorded fields — fields the
/// schema refuses, or a step the entity's invariants refuse.
///
/// [`CoreError::EntityMismatch`] when an event belongs to another definition, and
/// [`CoreError::UnknownState`] when it names a state the definition does not have.
pub fn rehydrate(
    definition: &ValidatedDefinition,
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

    // What an invariant may not read, and therefore what it is given: no arguments, no previous
    // fields. Hoisted so every event's check borrows the same empty map.
    let empty = Map::new();

    // One decision writes every event of its revision at once, in the order the operation's
    // `emits` lists them (`create` emits at most one), so the fold walks the history one revision
    // at a time: a group is one decision, and it is held to one operation as a whole — transition,
    // arguments, fields, and every event the operation would have emitted, payloads included.
    let mut start = 0;
    while start < events.len() {
        let revision = events[start].revision;
        let end = start
            + events[start..]
                .iter()
                .take_while(|event| event.revision == revision)
                .count();
        let group = &events[start..end];
        let index = start;
        let first = &events[start];

        for (offset, event) in group.iter().enumerate() {
            let at = index + offset;
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
                    "event {at} (`{}`) is about `{}`, not `{}`; one history describes one instance",
                    event.event_type, event.id, instance.id
                ));
            }
            if !definition.lifecycle.states.contains(&event.to_state) {
                return Err(CoreError::UnknownState {
                    entity: definition.entity.clone(),
                    state: event.to_state.clone(),
                });
            }
        }

        // A revision is reached once and follows the one before it. A gap means events are missing,
        // and folding over a gap would produce an instance nothing ever was.
        let expected_revision = instance
            .revision
            .checked_add(1)
            .filter(|revision| *revision <= i64::MAX as u64)
            .ok_or_else(|| CoreError::RevisionExhausted {
                entity: instance.entity.clone(),
                id: instance.id.clone(),
                revision: instance.revision,
            })?;
        if revision != expected_revision {
            return refuse(format!(
                "event {index} (`{}`) is at revision {}, but the fold had reached {}; a history \
                 with a gap rebuilds an instance that never existed",
                first.event_type, revision, instance.revision
            ));
        }

        // Every event of one revision came out of one `execute` call, which materialised them all
        // from one context: the same transition, the same arguments, the same fields written. Two
        // events at one revision that disagree on any of those describe two decisions, and a
        // revision holds one.
        for (offset, event) in group.iter().enumerate().skip(1) {
            if event.from_state != first.from_state
                || event.to_state != first.to_state
                || event.args != first.args
                || event.changed != first.changed
            {
                return refuse(format!(
                    "events {index} (`{}`) and {} (`{}`) share revision {revision} but describe \
                     different decisions — one `execute` call writes every event of a revision \
                     with one transition, one set of arguments and one set of fields",
                    first.event_type,
                    index + offset,
                    event.event_type
                ));
            }
        }

        // Who wrote this revision, and the event templates it materialised — held until the fields
        // are installed and checked, because `execute` materialises events last (§ 6 step 10) and
        // the fold compares payloads in the same place.
        // Who could have written this revision — every operation that answers for the transition,
        // the arguments and the fields — with the event templates each materialised. Held until the
        // fields are installed and checked, because `execute` materialises events last (§ 6 step
        // 10) and the fold compares payloads in the same place; where two operations differ only
        // in their payload templates, the payloads are what tells them apart.
        let writers: Vec<(&str, &[crate::EventDefinition])> = match &first.from_state {
            // A creation event is not exempt from the lifecycle — it is held to the one state the
            // definition says an instance begins in. Without this, a fold accepts a forged
            // creation straight into any state `states` happens to contain, which is precisely the
            // thing `execute` cannot do: `create` always enters `lifecycle.initial`.
            None if index == 0 => {
                if first.to_state != definition.lifecycle.initial {
                    return refuse(format!(
                        "event 0 (`{}`) creates `{}` in `{}`, but an instance begins in `{}`; a creation that enters any other state rebuilds an instance `create` would never have produced", first.event_type, definition.entity, first.to_state, definition.lifecycle.initial
                    ));
                }
                // The creation event's own emitter check, and the one place a definition can rule
                // event sourcing out entirely: `create` emits at most one event, so a creation of
                // any other type — or any creation event at all when the definition emits none —
                // is a record no `create` call left behind.
                let Some(emit) = definition.create.emit.as_ref() else {
                    return refuse(format!(
                        "event 0 (`{}`) claims to create `{}`, but the definition emits nothing \
                         on creation: every creation event of it is invented, so no history of it \
                         can begin",
                        first.event_type, definition.entity
                    ));
                };
                if first.event_type != emit.event_type {
                    return refuse(format!(
                        "event 0 (`{}`) is a creation of a type `{}` does not emit on creation — \
                         it emits `{}`; a creation event of any other type describes no `create` \
                         call",
                        first.event_type, definition.entity, emit.event_type
                    ));
                }
                if group.len() > 1 {
                    return refuse(format!(
                        "events 0 to {} all claim revision 1, but `create` emits at most one \
                         event; a creation that left more than one is one no `create` call made",
                        end - 1
                    ));
                }
                // `create` writes one set of fields — the caller's, after defaults — and records
                // it twice: as the event's `changed` and as the arguments it was decided on. An
                // event whose two halves disagree describes no `create` call, so there is no
                // creation for the fold to reconstruct.
                if first.changed != first.args {
                    return refuse(format!(
                        "event 0 (`{}`) records fields it did not decide on — a creation carries \
                         one set of fields as both its `changed` and its `args`, and these differ \
                         on {}",
                        first.event_type,
                        disagreeing_fields(&first.args, &first.changed)
                    ));
                }
                vec![("create", std::slice::from_ref(emit))]
            }
            None => {
                return refuse(format!(
                    "event {index} (`{}`) claims no previous state, but only the first event of a \
                     history can be a creation",
                    first.event_type
                ))
            }
            Some(from) => {
                if from != &instance.lifecycle_state {
                    return refuse(format!(
                        "event {index} (`{}`) moves from `{from}`, but the fold is at `{}`",
                        first.event_type, instance.lifecycle_state
                    ));
                }
                if !permits(definition, from, &first.to_state) {
                    return refuse(format!(
                        "event {index} (`{}`) moves `{from}` -> `{}`, which no operation of \
                         `{}` declares; replaying it would reach a state the definition never \
                         permitted",
                        first.event_type, first.to_state, definition.entity
                    ));
                }
                // An emitter has to exist before there is any point asking what it would have
                // produced. Without this the event rides in on the transition alone: nothing holds
                // its `changed` to a `set:`, nothing holds its `args` to an argument schema, and
                // the schema is left deciding what an event of a type the definition never emits
                // may write.
                for (offset, event) in group.iter().enumerate() {
                    if !emits_on(definition, from, &event.to_state, &event.event_type) {
                        return refuse(format!(
                            "event {} (`{}`) moves `{from}` -> `{}`, but no operation of `{}` \
                             emits `{}` on that transition; an event nothing emits describes no \
                             decision, so there is nothing to hold its fields to",
                            index + offset,
                            event.event_type,
                            event.to_state,
                            definition.entity,
                            event.event_type
                        ));
                    }
                }
                match operations_that_would_have_produced(definition, &instance, group, from) {
                    Ok(candidates) => candidates
                        .into_iter()
                        .map(|(name, operation)| (name.as_str(), operation.emits.as_slice()))
                        .collect(),
                    Err(failure) => {
                        return refuse(format!(
                            "event {index} (`{}`) is not what any operation that emits it on \
                             `{from}` -> `{}` would have produced — {failure}; replaying it would \
                             reach a state `execute` would not have permitted",
                            first.event_type, first.to_state
                        ))
                    }
                }
            }
        };

        // The payload templates read the fields as they stood before as `$old_fields`, so those
        // are kept across the install.
        let before_fields = instance.fields.clone();
        instance.lifecycle_state = first.to_state.clone();
        instance.revision = revision;
        for (name, value) in &first.changed {
            instance.fields.insert(name.clone(), value.clone());
        }

        // Fields before rules, per revision, in the order `execute` uses (design § 6: the fields
        // are validated at step 7 and the invariants evaluated at step 9). A field of the wrong
        // type is a schema defect, and checking the invariants first would report it as a broken
        // invariant — the wrong defect, and only in the definitions where some invariant happens
        // to read that field. Validating here rather than once at the end also names the event
        // that carried it.
        let defects = validate_object(&definition.schema, &instance.fields, "$fields");
        if !defects.is_empty() {
            let detail = defects
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("; ");
            return refuse(format!(
                "event {index} (`{}`) folds to an instance the schema refuses: {detail}",
                first.event_type
            ));
        }

        // `create` and `execute` both check the definition's invariants against what they just
        // wrote, so a fold that skipped them would rebuild an instance the kernel refused to
        // produce. What still reaches this check, now that every event answers to an operation's
        // `set:`, is an honest history recorded under an earlier definition: an invariant added, or
        // tightened, after the events were written is satisfied by nothing in them, and a `closed`
        // ticket with no resolution is refused here or handed back. The creation event is checked
        // too, because `create` checks them as well.
        //
        // Registration guarantees an invariant reads only `$id`, `$entity`, `$version`, `$state`
        // and `$fields…` (R-52), so the empty arguments and previous fields below are what an
        // invariant is allowed to see, not a narrowing of it.
        let context = TemplateContext {
            bindings: &[],
            definition,
            id: &first.id,
            args: &empty,
            old_fields: &empty,
            new_fields: &instance.fields,
            from_state: first.from_state.as_deref(),
            to_state: &first.to_state,
        };
        if let Err(failure) = check_invariants(definition, &context) {
            return refuse(format!(
                "event {index} (`{}`) folds to an instance a declared invariant refuses — \
                 {failure}; replaying it would reach a state `execute` would not have permitted",
                first.event_type
            ));
        }

        // Last, as `execute` does it: every payload against the template that emitted it, resolved
        // in the context `materialize_event` used — the fields afterwards, the fields before, the
        // arguments (none on creation). In a definition whose operations have no `set:`, the payload
        // is the only place a decision's particulars (who acted, why) are written down, so a fold
        // that skipped it would accept any account of the decision that kept the transition.
        let (payload_args, payload_old) = match first.from_state {
            None => (&empty, &empty),
            Some(_) => (&first.args, &before_fields),
        };
        let materialised = TemplateContext {
            bindings: &[],
            definition,
            id: &first.id,
            args: payload_args,
            old_fields: payload_old,
            new_fields: &instance.fields,
            from_state: first.from_state.as_deref(),
            to_state: &first.to_state,
        };
        let mut payload_refusals = Vec::new();
        let answered =
            writers.iter().any(|(writer, templates)| {
                match templates.iter().zip(group).enumerate().find_map(
                    |(offset, (template, event))| {
                        payload_disagrees(&template.payload, &materialised, &event.payload).map(
                            |detail| {
                                format!(
                                    "event {} (`{}`) carries a payload `{writer}` would not have \
                                 emitted — {detail}",
                                    index + offset,
                                    event.event_type
                                )
                            },
                        )
                    },
                ) {
                    Some(refusal) => {
                        payload_refusals.push(refusal);
                        false
                    }
                    None => true,
                }
            });
        if !answered {
            return refuse(payload_refusals.join("; "));
        }

        start = end;
    }

    Ok(instance)
}

/// Every operation that could have produced this revision's events, or why none could have.
///
/// The caller has already refused an event no operation emits on its transition, so there is at
/// least one candidate to ask.
///
/// One operation has to answer for the whole revision, because one `execute` call did: the
/// arguments it was decided on, the fields its `set:` wrote and every event its `emits` lists came
/// from the same decision, in that order. Accepting the arguments from one candidate and the fields
/// from another — or an event list shorter or longer than the candidate's `emits` — would accept a
/// history no single call could have written. The payloads are the caller's to compare, after the
/// fields are installed and checked, because that is when `execute` materialises them — and they
/// are compared against every candidate returned here, because two operations may differ in nothing
/// but the payload they emit.
///
/// Each candidate is asked what it was asked when it ran: the preconditions see the arguments the
/// events carry, the fields as they stood before and the transition taken, and the `set:` templates
/// resolve against exactly that context — which is how `execute` builds it, so a disagreement here
/// is a disagreement about the decision and not about the two code paths.
fn operations_that_would_have_produced<'a>(
    definition: &'a ValidatedDefinition,
    before: &EntityInstance,
    group: &[DomainEvent],
    from: &str,
) -> Result<Vec<(&'a String, &'a OperationDefinition)>, String> {
    let first = &group[0];
    let mut refusals = Vec::new();
    let mut candidates = Vec::new();
    for (name, operation) in &definition.operations {
        if !declares(operation, from, &first.to_state)
            || !operation.emits.iter().any(|emitted| {
                group
                    .iter()
                    .any(|event| event.event_type == emitted.event_type)
            })
        {
            continue;
        }
        // Every event the operation emits, in its order, and nothing else: `execute` materialises
        // the whole `emits` list in one decision, so a history carrying a subset, a superset or a
        // reordering of it was not written by this operation.
        let emitted: Vec<&str> = operation
            .emits
            .iter()
            .map(|emitted| emitted.event_type.as_str())
            .collect();
        let carried: Vec<&str> = group
            .iter()
            .map(|event| event.event_type.as_str())
            .collect();
        if emitted != carried {
            refusals.push(format!(
                "`{name}`: emits [{}] on that transition, and the history carries [{}] at this \
                 revision",
                emitted.join(", "),
                carried.join(", ")
            ));
            continue;
        }
        // `execute` validates the arguments against the operation's schema before a precondition
        // reads them (step 3 of the eleven), so the fold asks the same question first: an undeclared
        // or wrong-typed argument is a decision no `execute` call made, whatever the rules say.
        let argument_defects = validate_object(&operation.arguments, &first.args, "arguments");
        if !argument_defects.is_empty() {
            refusals.push(format!(
                "`{name}`: decided on arguments its argument schema refuses — {}",
                argument_defects
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("; ")
            ));
            continue;
        }
        let context = TemplateContext {
            bindings: &[],
            definition,
            id: &first.id,
            args: &first.args,
            old_fields: &before.fields,
            new_fields: &before.fields,
            from_state: Some(from),
            to_state: &first.to_state,
        };
        if let Err(error) = check_preconditions(name, &operation.preconditions, &context) {
            refusals.push(format!(
                "`{name}`: decided on arguments its preconditions would have refused — {error}"
            ));
            continue;
        }

        let mut written = canonical_object(before.fields.clone());
        let mut unresolved = None;
        for (field, template) in &operation.set {
            match resolve_template(template, &context) {
                Ok(value) => {
                    written.insert(field.clone(), value);
                }
                // A `set:` this candidate cannot resolve from the event's arguments is this
                // candidate's refusal, not the fold's: another emitter may still answer for the
                // event.
                Err(error) => {
                    unresolved = Some(format!(
                        "`{name}`: its `set:` for `{field}` does not resolve from those \
                         arguments — {error}"
                    ));
                    break;
                }
            }
        }
        if let Some(refusal) = unresolved {
            refusals.push(refusal);
            continue;
        }

        // `changed` through the same function `execute` records it with, so the comparison is
        // between two decisions rather than between two ways of describing one.
        let after = TemplateContext {
            bindings: &[],
            definition,
            id: &first.id,
            args: &first.args,
            old_fields: &before.fields,
            new_fields: &written,
            from_state: Some(from),
            to_state: &first.to_state,
        };
        let would_have_written = changed_fields(&after);
        if would_have_written == first.changed {
            candidates.push((name, operation));
        } else {
            refusals.push(format!(
                "`{name}`: accepts those arguments but would have written {}",
                disagreeing_fields(&would_have_written, &first.changed)
            ));
        }
    }
    if candidates.is_empty() {
        // Non-empty while the caller checks that an emitter exists: a candidate that answers for
        // the revision was collected above, and one that does not left its reason behind.
        Err(refusals.join("; "))
    } else {
        Ok(candidates)
    }
}

/// How a recorded payload differs from what `template` resolves to in `context`, or `None` when
/// they agree. An unresolvable template is a disagreement too: `execute` would have refused.
fn payload_disagrees(
    template: &Value,
    context: &TemplateContext<'_>,
    recorded: &Value,
) -> Option<String> {
    match resolve_template(template, context) {
        Ok(resolved) => {
            let resolved = canonicalize(resolved);
            (resolved != *recorded).then(|| format!("resolves to {resolved}, not {recorded}"))
        }
        Err(error) => Some(format!("does not resolve — {error}")),
    }
}

/// The fields two sets of writes disagree on, each with both values.
///
/// Named one by one rather than by printing both objects: a refusal a reader has to diff by eye is
/// a refusal that gets skimmed, and the field in dispute is the whole finding.
fn disagreeing_fields(expected: &Map<String, Value>, recorded: &Map<String, Value>) -> String {
    let mut names: Vec<&String> = expected.keys().chain(recorded.keys()).collect();
    names.sort_unstable();
    names.dedup();
    let describe = |value: Option<&Value>| match value {
        Some(value) => value.to_string(),
        None => "nothing".to_owned(),
    };
    names
        .into_iter()
        .filter(|name| expected.get(*name) != recorded.get(*name))
        .map(|name| {
            format!(
                "`{name}` (recorded {}, not {})",
                describe(recorded.get(name)),
                describe(expected.get(name))
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Whether any operation of `definition` declares a transition from `from` to `to`.
fn permits(definition: &ValidatedDefinition, from: &str, to: &str) -> bool {
    definition
        .operations
        .values()
        .any(|operation| declares(operation, from, to))
}

/// Whether any operation of `definition` both declares `from` -> `to` and emits `event_type` on it.
///
/// Both halves of one operation, not two: an operation that declares the transition while another
/// emits the type is not an operation that ever wrote this event, and admitting the event on that
/// pairing would accept a record no single `execute` call could have left.
fn emits_on(definition: &ValidatedDefinition, from: &str, to: &str, event_type: &str) -> bool {
    definition.operations.values().any(|operation| {
        declares(operation, from, to)
            && operation
                .emits
                .iter()
                .any(|emitted| emitted.event_type == event_type)
    })
}

/// Whether `operation` declares a transition from `from` to `to`.
fn declares(operation: &OperationDefinition, from: &str, to: &str) -> bool {
    operation.transitions.iter().any(|transition| {
        transition.from.as_slice().iter().any(|state| state == from) && transition.to == to
    })
}
