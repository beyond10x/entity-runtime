//! Folding an instance back out of its events.
//!
//! The two claims worth pinning are the story's own: a create plus n operations, folded from their
//! events, equals the instance the operations returned — and an invented event whose transition the
//! lifecycle does not permit is refused rather than replayed.

use entity_core::{rehydrate, replay as replay_records, DomainEvent, Registry, Runtime};
use serde_json::json;

/// A ticket that opens, closes, and can be reopened — with an event on every step including create.
fn registry() -> Registry {
    let definition = serde_json::from_value(json!({
        "entity": "ticket",
        "version": 1,
        "schema": {
            "fields": {
                "title": { "type": "string", "required": true },
                "closed_by": { "type": "string" }
            }
        },
        "lifecycle": { "initial": "open", "states": ["open", "closed"] },
        "create": { "emit": { "type": "TicketOpened", "payload": { "ticket": "$id" } } },
        "operations": {
            "close": {
                "arguments": { "fields": { "who": { "type": "string", "required": true } } },
                "transitions": [{ "from": "open", "to": "closed" }],
                "set": { "closed_by": "$args.who" },
                "emits": [{ "type": "TicketClosed", "payload": { "ticket": "$id" } }]
            },
            "reopen": {
                "transitions": [{ "from": "closed", "to": "open" }],
                "emits": [{ "type": "TicketReopened", "payload": { "ticket": "$id" } }]
            }
        }
    }))
    .expect("the definition parses");
    let mut registry = Registry::new();
    registry.register(definition).expect("it validates");
    registry
}

#[test]
fn a_create_and_two_operations_fold_back_into_the_instance_they_produced() {
    let registry = registry();
    let runtime = Runtime::new(&registry);

    let created = runtime
        .create("ticket", 1, "one", json!({ "title": "A ticket" }))
        .expect("permitted");
    let closed = runtime
        .execute(&created.instance, "close", json!({ "who": "timo" }))
        .expect("permitted");
    let reopened = runtime
        .execute(&closed.instance, "reopen", json!({}))
        .expect("permitted");

    let mut history = Vec::new();
    history.extend(created.events.clone());
    history.extend(closed.events.clone());
    history.extend(reopened.events.clone());
    assert_eq!(
        history.len(),
        3,
        "one event per step, including the creation"
    );

    let definition = registry.get("ticket", 1).expect("registered");
    let folded = rehydrate(definition, &history).expect("the history rebuilds the instance");

    assert_eq!(
        folded, reopened.instance,
        "state, revision and fields all come back — including `closed_by`, which only a `set:` \
         assignment ever wrote"
    );
}

#[test]
fn complete_decision_records_recompute_the_instance_from_genesis() {
    let registry = registry();
    let runtime = Runtime::new(&registry);
    let created = runtime
        .create("ticket", 1, "recorded", json!({ "title": "A ticket" }))
        .expect("permitted");
    let closed = runtime
        .execute(&created.instance, "close", json!({ "who": "timo" }))
        .expect("permitted");
    let records = [created.record, closed.record];
    assert_eq!(
        replay_records(&records).expect("verified replay"),
        closed.instance
    );
}

#[test]
fn changing_any_recorded_result_is_refused_instead_of_becoming_state() {
    let registry = registry();
    let runtime = Runtime::new(&registry);
    let created = runtime
        .create("ticket", 1, "forged", json!({ "title": "A ticket" }))
        .expect("permitted");
    let closed = runtime
        .execute(&created.instance, "close", json!({ "who": "timo" }))
        .expect("permitted");
    let mut records = [created.record, closed.record];
    records[1]
        .result
        .fields
        .insert("closed_by".to_owned(), json!("mallory"));
    let error = replay_records(&records).expect_err("forged output is comparison evidence");
    assert!(
        error
            .to_string()
            .contains("differs from the decision recomputed"),
        "{error}"
    );
}

#[test]
fn an_event_whose_transition_the_lifecycle_does_not_permit_is_refused() {
    // The R-34 guard, stated as an attack: hand the fold an event that says the ticket went
    // straight from open to a state no operation reaches that way. If the fold trusted the event,
    // replay would be a second way to set a lifecycle state — and every ladder in every adopter
    // would be advisory.
    let registry = registry();
    let runtime = Runtime::new(&registry);
    let created = runtime
        .create("ticket", 1, "one", json!({ "title": "A ticket" }))
        .expect("permitted");

    let mut invented = created.events[0].clone();
    invented.revision = 2;
    invented.from_state = Some("closed".to_owned());
    invented.to_state = "open".to_owned();
    invented.event_type = "TicketReopened".to_owned();

    let definition = registry.get("ticket", 1).expect("registered");
    let mut history = created.events.clone();
    history.push(invented);

    let error = rehydrate(definition, &history).expect_err("the fold must refuse it");
    assert!(
        error.to_string().contains("the fold is at `open`"),
        "the refusal says where the fold actually was: {error}"
    );
}

#[test]
fn an_event_naming_a_transition_no_operation_declares_is_refused() {
    let registry = registry();
    let runtime = Runtime::new(&registry);
    let created = runtime
        .create("ticket", 1, "one", json!({ "title": "A ticket" }))
        .expect("permitted");

    // `open -> open` is a state the definition has and a transition no operation declares.
    let mut invented = created.events[0].clone();
    invented.revision = 2;
    invented.from_state = Some("open".to_owned());
    invented.to_state = "open".to_owned();

    let definition = registry.get("ticket", 1).expect("registered");
    let mut history = created.events.clone();
    history.push(invented);

    let error = rehydrate(definition, &history).expect_err("the fold must refuse it");
    assert!(
        error.to_string().contains("which no operation of"),
        "the refusal names the missing declaration: {error}"
    );
}

#[test]
fn an_event_whose_type_no_operation_emits_on_its_transition_is_refused() {
    // The last way into the fold that answered to nothing. `open -> closed` is declared and
    // `TicketArchived` is a type no operation emits, so there was no `set:` to hold the event's
    // `changed` to and no argument schema to hold its `args` to: the fields rode in on the
    // transition alone, leaving the schema to decide what an event the definition never emits may
    // write. An event nothing emits describes no decision the kernel took, so there is nothing for
    // a fold to reconstruct from it.
    let registry = registry();
    let runtime = Runtime::new(&registry);
    let definition = registry.get("ticket", 1).expect("registered");
    let created = runtime
        .create("ticket", 1, "one", json!({ "title": "A ticket" }))
        .expect("permitted");
    let closed = runtime
        .execute(&created.instance, "close", json!({ "who": "timo" }))
        .expect("permitted");

    let mut forged = [created.events[0].clone(), closed.events[0].clone()];
    forged[1].event_type = "TicketArchived".to_owned();

    let error = rehydrate(definition, &forged).expect_err("no operation emits `TicketArchived`");
    assert!(
        matches!(error, entity_core::CoreError::Validation(ref errors)
            if errors[0].path == "events"
                && errors[0].message.contains("event 1 (`TicketArchived`)")
                && errors[0].message.contains("`open` -> `closed`")
                && errors[0]
                    .message
                    .contains("no operation of `ticket` emits `TicketArchived` on that transition")),
        "the refusal names the event, its transition and what no operation emits there: {error}"
    );
}

#[test]
fn a_creation_event_of_a_type_the_definition_does_not_emit_on_creation_is_refused() {
    // The same hole on the first event. `create` emits at most one event, so a creation of any
    // other type is a record no `create` call left behind — and one whose fields nothing but the
    // schema would have questioned.
    let registry = registry();
    let runtime = Runtime::new(&registry);
    let definition = registry.get("ticket", 1).expect("registered");
    let created = runtime
        .create("ticket", 1, "one", json!({ "title": "A ticket" }))
        .expect("permitted");

    let mut forged = created.events.clone();
    forged[0].event_type = "TicketRaised".to_owned();

    let error = rehydrate(definition, &forged).expect_err("`create` emits `TicketOpened`");
    assert!(
        matches!(error, entity_core::CoreError::Validation(ref errors)
            if errors[0].path == "events"
                && errors[0].message.contains("event 0 (`TicketRaised`)")
                && errors[0].message.contains("it emits `TicketOpened`")),
        "the refusal names the type recorded and the one the definition emits: {error}"
    );
}

/// A note whose creation emits nothing: a definition that cannot be event-sourced at all.
fn silent_registry() -> Registry {
    let definition = serde_json::from_value(json!({
        "entity": "note",
        "version": 1,
        "schema": { "fields": { "text": { "type": "string", "required": true } } },
        "lifecycle": { "initial": "draft", "states": ["draft", "filed"] },
        "operations": {
            "file": {
                "transitions": [{ "from": "draft", "to": "filed" }],
                "emits": [{ "type": "NoteFiled", "payload": { "note": "$id" } }]
            }
        }
    }))
    .expect("the definition parses");
    let mut registry = Registry::new();
    registry.register(definition).expect("it validates");
    registry
}

#[test]
fn a_history_cannot_begin_for_a_definition_that_emits_nothing_on_creation() {
    // The design says a definition emitting no creation event cannot be event-sourced, and this is
    // where saying so has teeth: with no `create.emit` to compare against, any first event at all
    // was accepted as the creation and the fold invented an instance no record supports.
    let registry = silent_registry();
    let definition = registry.get("note", 1).expect("registered");

    let forged = vec![DomainEvent {
        entity: "note".to_owned(),
        version: 1,
        id: "n-1".to_owned(),
        revision: 1,
        event_type: "NoteWritten".to_owned(),
        from_state: None,
        to_state: "draft".to_owned(),
        changed: serde_json::from_value(json!({ "text": "invented" })).expect("an object"),
        args: serde_json::from_value(json!({ "text": "invented" })).expect("an object"),
        payload: json!({}),
    }];

    let error = rehydrate(definition, &forged).expect_err("`note` emits nothing on creation");
    assert!(
        matches!(error, entity_core::CoreError::Validation(ref errors)
            if errors[0].path == "events"
                && errors[0].message.contains("event 0 (`NoteWritten`)")
                && errors[0].message.contains("the definition emits nothing on creation")),
        "the refusal says the definition emits no creation event: {error}"
    );
}

#[test]
fn a_history_with_a_gap_is_refused_rather_than_folded_over() {
    // Folding over a gap produces an instance nothing ever was, and nothing downstream could tell.
    let registry = registry();
    let runtime = Runtime::new(&registry);
    let created = runtime
        .create("ticket", 1, "one", json!({ "title": "A ticket" }))
        .expect("permitted");
    let closed = runtime
        .execute(&created.instance, "close", json!({ "who": "timo" }))
        .expect("permitted");

    let definition = registry.get("ticket", 1).expect("registered");
    let error = rehydrate(definition, &closed.events).expect_err("no creation event to start from");
    assert!(
        error
            .to_string()
            .contains("must begin with a creation event"),
        "{error}"
    );
}

#[test]
fn an_empty_history_is_refused_rather_than_producing_an_empty_instance() {
    let registry = registry();
    let definition = registry.get("ticket", 1).expect("registered");
    let error = rehydrate(definition, &[]).expect_err("nothing to fold");
    assert!(
        error
            .to_string()
            .contains("cannot be rebuilt from no events"),
        "{error}"
    );
}

#[test]
fn a_history_belonging_to_another_instance_is_refused() {
    let registry = registry();
    let runtime = Runtime::new(&registry);
    let one = runtime
        .create("ticket", 1, "one", json!({ "title": "One" }))
        .expect("permitted");
    let two = runtime
        .create("ticket", 1, "two", json!({ "title": "Two" }))
        .expect("permitted");
    let closed = runtime
        .execute(&two.instance, "close", json!({ "who": "timo" }))
        .expect("permitted");

    let definition = registry.get("ticket", 1).expect("registered");
    let mut history = one.events.clone();
    history.extend(closed.events);

    let error = rehydrate(definition, &history).expect_err("one history describes one instance");
    assert!(
        error
            .to_string()
            .contains("one history describes one instance"),
        "{error}"
    );
}

#[test]
fn a_creation_event_into_a_state_that_is_not_the_initial_one_is_refused() {
    // The hole this closes: the first event used to be exempt from every lifecycle check, so a
    // forged creation walked straight into any state `states` happened to list. `create` always
    // enters `lifecycle.initial`, so a fold that reached anything else rebuilt an instance the
    // kernel could not have produced.
    let registry = registry();
    let definition = registry.get("ticket", 1).expect("registered");

    let forged = vec![DomainEvent {
        entity: "ticket".to_owned(),
        version: 1,
        id: "one".to_owned(),
        revision: 1,
        event_type: "TicketOpened".to_owned(),
        from_state: None,
        to_state: "closed".to_owned(),
        changed: serde_json::from_value(json!({ "title": "A ticket" })).expect("an object"),
        args: serde_json::Map::new(),
        payload: json!({}),
    }];

    let error = rehydrate(definition, &forged).expect_err("a creation may only enter `open`");
    let message = error.to_string();
    assert!(message.contains("closed"), "{message}");
    assert!(message.contains("open"), "{message}");
}

#[test]
fn an_event_carrying_a_field_the_schema_does_not_declare_is_refused() {
    // An event's `changed` is data like any other. Installed unchecked, a fold rebuilds an
    // instance the schema refuses — a field of the wrong type, or one nobody declared.
    let registry = registry();
    let definition = registry.get("ticket", 1).expect("registered");

    let forged = vec![DomainEvent {
        entity: "ticket".to_owned(),
        version: 1,
        id: "one".to_owned(),
        revision: 1,
        event_type: "TicketOpened".to_owned(),
        from_state: None,
        to_state: "open".to_owned(),
        changed: serde_json::from_value(json!({ "title": "A ticket", "invented": "nope" }))
            .expect("an object"),
        // The same fields as `changed`: a creation records one set of fields as both, and a
        // disagreement between them is refused before the schema is ever consulted.
        args: serde_json::from_value(json!({ "title": "A ticket", "invented": "nope" }))
            .expect("an object"),
        payload: json!({}),
    }];

    let error = rehydrate(definition, &forged).expect_err("`invented` is not a declared field");
    assert!(
        matches!(error, entity_core::CoreError::Validation(ref errors)
            if errors[0].path == "events"
                && errors[0].message.contains("event 0 (`TicketOpened`)")
                && errors[0].message.contains("invented")),
        "the refusal names the event and the undeclared field: {error}"
    );
}

#[test]
fn an_event_carrying_a_field_of_the_wrong_type_is_refused() {
    let registry = registry();
    let definition = registry.get("ticket", 1).expect("registered");

    let forged = vec![DomainEvent {
        entity: "ticket".to_owned(),
        version: 1,
        id: "one".to_owned(),
        revision: 1,
        event_type: "TicketOpened".to_owned(),
        from_state: None,
        to_state: "open".to_owned(),
        changed: serde_json::from_value(json!({ "title": 12345 })).expect("an object"),
        args: serde_json::from_value(json!({ "title": 12345 })).expect("an object"),
        payload: json!({}),
    }];

    let error = rehydrate(definition, &forged).expect_err("`title` is a string");
    assert!(
        matches!(error, entity_core::CoreError::Validation(ref errors)
            if errors[0].path == "events"
                && errors[0].message.contains("$fields.title")
                && errors[0].message.contains("expected string")),
        "the refusal names the field and the type the schema declares: {error}"
    );
}

#[test]
fn a_history_whose_revisions_skip_a_number_is_refused() {
    // R-97's revision-gap clause. It was pinned by a test that asserts a different branch — the
    // one about a history that does not begin with a creation — so the gap itself was checked by
    // nothing.
    let registry = registry();
    let definition = registry.get("ticket", 1).expect("registered");
    let runtime = Runtime::new(&registry);

    let created = runtime
        .create("ticket", 1, "one", json!({ "title": "A ticket" }))
        .expect("permitted");
    let closed = runtime
        .execute(&created.instance, "close", json!({ "who": "timo" }))
        .expect("permitted");

    let mut gapped = created.events.clone();
    let mut later = closed.events[0].clone();
    later.revision = 7;
    gapped.push(later);

    let error = rehydrate(definition, &gapped).expect_err("revision 7 does not follow revision 1");
    let message = error.to_string();
    assert!(
        message.contains('7'),
        "the message names the revision: {message}"
    );
    assert!(message.contains("gap"), "{message}");
}

#[test]
fn a_second_creation_event_partway_through_a_history_is_refused() {
    // The other branch R-97 claims and nothing asserted: only the first event of a history may be
    // a creation.
    let registry = registry();
    let definition = registry.get("ticket", 1).expect("registered");
    let runtime = Runtime::new(&registry);

    let created = runtime
        .create("ticket", 1, "one", json!({ "title": "A ticket" }))
        .expect("permitted");

    let mut twice = created.events.clone();
    let mut second = created.events[0].clone();
    second.revision = 2;
    twice.push(second);

    let error = rehydrate(definition, &twice).expect_err("only the first event may be a creation");
    assert!(error.to_string().contains("creation"), "{error}");
}

/// A story whose `implement` costs evidence — the adopter's own ladder, reduced to one rung.
fn gated_registry() -> Registry {
    let definition = serde_json::from_value(json!({
        "entity": "story",
        "version": 1,
        "schema": { "fields": { "title": { "type": "string", "required": true } } },
        "lifecycle": { "initial": "active", "states": ["active", "implemented"] },
        "create": { "emit": { "type": "StoryCreated", "payload": { "story": "$id" } } },
        "operations": {
            "implement": {
                "arguments": { "fields": { "evidence": { "type": "json" } } },
                "preconditions": [{
                    "name": "evidence: test_result",
                    "message": "an implemented story carries at least one test result",
                    "assert": { "gte": ["$args.evidence.test_result", 1] }
                }],
                "transitions": [{ "from": "active", "to": "implemented" }],
                "emits": [{ "type": "StoryImplemented", "payload": { "story": "$id" } }]
            }
        }
    }))
    .expect("the definition parses");
    let mut registry = Registry::new();
    registry.register(definition).expect("it validates");
    registry
}

#[test]
fn an_event_records_the_arguments_it_was_decided_on_and_a_creation_records_its_fields() {
    let registry = registry();
    let runtime = Runtime::new(&registry);
    let created = runtime
        .create("ticket", 1, "one", json!({ "title": "A ticket" }))
        .expect("permitted");
    assert_eq!(
        created.events[0].args,
        json!({ "title": "A ticket" })
            .as_object()
            .cloned()
            .expect("an object"),
        "a creation event carries the creation's fields as its arguments"
    );
    let closed = runtime
        .execute(&created.instance, "close", json!({ "who": "timo" }))
        .expect("permitted");
    assert_eq!(
        closed.events[0].args,
        json!({ "who": "timo" })
            .as_object()
            .cloned()
            .expect("an object"),
        "an operation's event carries the arguments it was decided on, verbatim"
    );
}

#[test]
fn a_replayed_event_whose_arguments_would_not_have_satisfied_the_preconditions_is_refused() {
    // The forgery R-97 did not yet catch: a transition the definition declares, from the state the
    // fold is at, at the next revision — decided on evidence that would have been refused.
    let registry = gated_registry();
    let runtime = Runtime::new(&registry);
    let created = runtime
        .create("story", 1, "s-1", json!({ "title": "One" }))
        .expect("permitted");
    let implemented = runtime
        .execute(
            &created.instance,
            "implement",
            json!({ "evidence": { "test_result": 1 } }),
        )
        .expect("one test result is enough");

    let honest = [created.events[0].clone(), implemented.events[0].clone()];
    let definition = registry.get("story", 1).expect("registered");
    assert_eq!(
        rehydrate(definition, &honest).expect("the honest history folds"),
        implemented.instance
    );

    let mut forged = honest.clone();
    forged[1].args = json!({ "evidence": { "test_result": 0 } })
        .as_object()
        .cloned()
        .expect("an object");
    let error = rehydrate(definition, &forged).expect_err("zero test results do not implement");
    let message = error.to_string();
    assert!(
        message.contains("evidence: test_result"),
        "the refusal names the rule: {message}"
    );
    assert!(
        message.contains("decided on arguments"),
        "and says what kind of forgery it is: {message}"
    );

    // And the unobservable case is refused too: no evidence at all is not a count of zero, but it
    // is not a count of one either.
    let mut unobserved = honest;
    unobserved[1].args = serde_json::Map::new();
    let error = rehydrate(definition, &unobserved).expect_err("no evidence is not one test result");
    assert!(
        matches!(error, entity_core::CoreError::Validation(ref errors)
            if errors[0].path == "events"
                && errors[0].message.contains("evidence: test_result")),
        "the refusal names the rule nobody could answer: {error}"
    );
}

/// A ticket whose invariant forbids being `closed` without a resolution.
///
/// `assert` is a parameter because the two branches of a refused invariant need different
/// conditions: `exists` always answers, so it yields a violation, while a comparison against an
/// absent field cannot be answered at all and yields an unobservable.
fn guarded_registry(assert: serde_json::Value) -> Registry {
    let definition = serde_json::from_value(json!({
        "entity": "ticket",
        "version": 1,
        "schema": {
            "fields": {
                "title": { "type": "string", "required": true },
                "resolution": { "type": "string" }
            }
        },
        "lifecycle": { "initial": "open", "states": ["open", "closed"] },
        "invariants": [{
            "name": "closed_requires_resolution",
            "message": "a closed ticket carries the resolution it was closed with",
            "assert": assert
        }],
        "create": { "emit": { "type": "TicketOpened", "payload": { "ticket": "$id" } } },
        "operations": {
            "close": {
                "arguments": {
                    "fields": { "resolution": { "type": "string", "required": true } }
                },
                "transitions": [{ "from": "open", "to": "closed" }],
                "set": { "resolution": "$args.resolution" },
                "emits": [{ "type": "TicketClosed", "payload": { "ticket": "$id" } }]
            }
        }
    }))
    .expect("the definition parses");
    let mut registry = Registry::new();
    registry.register(definition).expect("it validates");
    registry
}

/// The ticket of the guarded story in the revision that came before it: `close` records no
/// `resolution`, and `invariants` is whatever the definition of the day declared.
///
/// Two calls give the same `(entity, version)` in two revisions — the definition a history was
/// recorded under, and the definition it is later folded under. That difference is what reaches the
/// per-event invariant check now that every event answers to an emitting operation's `set:`.
fn unresolved_registry(invariants: serde_json::Value) -> Registry {
    let definition = serde_json::from_value(json!({
        "entity": "ticket",
        "version": 1,
        "schema": {
            "fields": {
                "title": { "type": "string", "required": true },
                "resolution": { "type": "string" }
            }
        },
        "lifecycle": { "initial": "open", "states": ["open", "closed"] },
        "invariants": invariants,
        "create": { "emit": { "type": "TicketOpened", "payload": { "ticket": "$id" } } },
        "operations": {
            "close": {
                "arguments": {
                    "fields": { "resolution": { "type": "string", "required": true } }
                },
                "transitions": [{ "from": "open", "to": "closed" }],
                "emits": [{ "type": "TicketClosed", "payload": { "ticket": "$id" } }]
            }
        }
    }))
    .expect("the definition parses");
    let mut registry = Registry::new();
    registry.register(definition).expect("it validates");
    registry
}

#[test]
fn a_history_folding_to_an_instance_a_declared_invariant_refuses_is_refused() {
    // The history every earlier check waves through, and the honest one: `close` never recorded a
    // `resolution`, and the invariant that a closed ticket carries one was added after these events
    // were written. The transition is declared, `TicketClosed` is what `close` emits on it, the
    // `changed` is exactly what that `set:`-less `close` wrote, and the schema is happy
    // (`resolution` is optional) — so the per-event invariant check is the only thing standing
    // between this history and a `closed` ticket with no resolution: exactly the instance
    // `execute` refuses to produce.
    for assert in [
        // Answerable, and false: a violation.
        json!({ "any": [{ "ne": ["$state", "closed"] }, { "exists": "$fields.resolution" }] }),
        // Unanswerable: a comparison against a field that is not there. An invariant nobody can
        // check is not an invariant that held, so this must refuse the fold too.
        json!({ "any": [
            { "ne": ["$state", "closed"] },
            { "eq": ["$fields.resolution", "fixed"] }
        ] }),
    ] {
        let recorded_under = unresolved_registry(json!([]));
        let runtime = Runtime::new(&recorded_under);
        let created = runtime
            .create("ticket", 1, "t-1", json!({ "title": "x" }))
            .expect("permitted");
        let closed = runtime
            .execute(&created.instance, "close", json!({ "resolution": "fixed" }))
            .expect("permitted");
        let history = [created.events[0].clone(), closed.events[0].clone()];

        // The control, and the claim that what refuses below is the invariant and not a forgery:
        // under the definition that wrote these events the fold reproduces the instance `execute`
        // returned, byte for byte.
        assert_eq!(
            rehydrate(
                recorded_under.get("ticket", 1).expect("registered"),
                &history
            )
            .expect("the recorded history folds under the definition that wrote it"),
            closed.instance,
            "the history is honest: every check but the invariant accepts it"
        );

        let folded_under = unresolved_registry(json!([{
            "name": "closed_requires_resolution",
            "message": "a closed ticket carries the resolution it was closed with",
            "assert": assert
        }]));
        let error = rehydrate(folded_under.get("ticket", 1).expect("registered"), &history)
            .expect_err("a closed ticket without a resolution is what the invariant forbids");
        assert!(
            matches!(error, entity_core::CoreError::Validation(ref errors)
                if errors[0].path == "events"
                    && errors[0].message.contains("closed_requires_resolution")),
            "the refusal names the invariant: {error}"
        );
        let message = error.to_string();
        assert!(
            message.contains("a declared invariant refuses"),
            "and says which check refused it: {message}"
        );
        assert!(
            message.contains("event 1 (`TicketClosed`)"),
            "and which event it was: {message}"
        );
    }
}

#[test]
fn an_event_missing_its_arguments_is_refused_when_parsed() {
    // The same rule as the envelope (R-87): a key nobody wrote must not read as "decided on
    // nothing". An event written by an older kernel, or by hand, is refused rather than defaulted.
    let text = json!({
        "entity": "ticket", "version": 1, "id": "one", "revision": 1,
        "type": "TicketOpened", "from_state": null, "to_state": "open",
        "changed": { "title": "A ticket" }, "payload": {}
    });
    let parsed: Result<DomainEvent, _> = serde_json::from_value(text);
    let error = parsed.expect_err("an event with no `args` is not one this kernel wrote");
    assert!(error.to_string().contains("args"), "{error}");
}

#[test]
fn a_forged_event_whose_changed_contradicts_what_its_own_arguments_would_have_set_is_refused() {
    // The hole the argument check left open. `args` was re-asked of the preconditions and `changed`
    // was installed, but nothing compared the two: a `close` decided on `fixed` could leave the
    // ticket resolved `not-fixed`. Schema-valid, invariant-satisfying, and a value no `execute` on
    // that command could ever produce.
    let registry = guarded_registry(
        json!({ "any": [{ "ne": ["$state", "closed"] }, { "exists": "$fields.resolution" }] }),
    );
    let runtime = Runtime::new(&registry);
    let definition = registry.get("ticket", 1).expect("registered");

    let created = runtime
        .create("ticket", 1, "t-1", json!({ "title": "x" }))
        .expect("permitted");
    let closed = runtime
        .execute(&created.instance, "close", json!({ "resolution": "fixed" }))
        .expect("permitted");

    let mut forged = [created.events[0].clone(), closed.events[0].clone()];
    forged[1].changed =
        serde_json::from_value(json!({ "resolution": "not-fixed" })).expect("an object");

    let error = rehydrate(definition, &forged).expect_err("`close` writes what its arguments say");
    assert!(
        matches!(error, entity_core::CoreError::Validation(ref errors)
            if errors[0].path == "events"
                && errors[0].message.contains("event 1 (`TicketClosed`)")
                && errors[0].message.contains("`close`")
                && errors[0].message.contains("`resolution` (recorded \"not-fixed\", not \"fixed\")")),
        "the refusal names the event, the operation tried and the field with both values: {error}"
    );
    assert!(
        !error.to_string().contains("closed_requires_resolution"),
        "`not-fixed` satisfies the invariant, so it is the `set:` comparison that refuses: {error}"
    );
}

#[test]
fn a_forged_event_carrying_an_argument_its_operation_does_not_declare_is_refused() {
    // `execute` checks the arguments against the operation's schema before any rule reads them.
    // An event whose `args` carry a key `close` never declared satisfies the preconditions, resolves
    // every `set:` and reproduces `changed` exactly — and still describes a call `execute` would
    // have refused at step 3. The fold asks the same question in the same place.
    let registry = registry();
    let runtime = Runtime::new(&registry);
    let definition = registry.get("ticket", 1).expect("registered");
    let created = runtime
        .create("ticket", 1, "t-1", json!({ "title": "x" }))
        .expect("permitted");
    let closed = runtime
        .execute(&created.instance, "close", json!({ "who": "ops" }))
        .expect("permitted");

    let mut forged = [created.events[0].clone(), closed.events[0].clone()];
    forged[1].args.insert("bogus".to_owned(), json!(1));

    let error = rehydrate(definition, &forged).expect_err("an undeclared argument is refused");
    assert!(
        matches!(error, entity_core::CoreError::Validation(ref errors)
            if errors[0].path == "events"
                && errors[0].message.contains("event 1 (`TicketClosed`)")
                && errors[0].message.contains("`close`")
                && errors[0].message.contains("arguments.bogus: field is not declared in the schema")),
        "the refusal names the event, the operation and the undeclared argument: {error}"
    );
}

#[test]
fn a_creation_event_whose_changed_differs_from_its_recorded_fields_is_refused() {
    // `create` writes one set of fields — the caller's, after defaults — and records it twice, as
    // the event's `changed` and as the arguments it was decided on. A forgery that alters one and
    // not the other describes no `create` call, so there is nothing for the fold to reconstruct.
    let registry = registry();
    let runtime = Runtime::new(&registry);
    let definition = registry.get("ticket", 1).expect("registered");
    let created = runtime
        .create("ticket", 1, "one", json!({ "title": "A ticket" }))
        .expect("permitted");

    let mut forged = created.events.clone();
    forged[0]
        .changed
        .insert("title".to_owned(), json!("Tampered"));

    let error = rehydrate(definition, &forged).expect_err("a creation records one set of fields");
    assert!(
        matches!(error, entity_core::CoreError::Validation(ref errors)
            if errors[0].path == "events"
                && errors[0].message.contains("event 0 (`TicketOpened`)")
                && errors[0].message.contains("`title` (recorded \"Tampered\", not \"A ticket\")")),
        "the refusal names the event and the field the two halves differ on: {error}"
    );
}

/// A counter whose invariant reads the very field a forged event gets wrong.
///
/// The two rules disagree about what the defect is: the schema says `count` is an integer, while
/// the invariant reading `$fields.count` merely computes `false` for a string and reports a broken
/// invariant. Which of them answers is what the test below pins.
fn counted_registry() -> Registry {
    let definition = serde_json::from_value(json!({
        "entity": "counter",
        "version": 1,
        "schema": { "fields": { "count": { "type": "integer", "required": true } } },
        "lifecycle": { "initial": "counting", "states": ["counting", "done"] },
        "invariants": [{
            "name": "count_is_not_negative",
            "message": "a counter never counts below zero",
            "assert": { "gte": ["$fields.count", 0] }
        }],
        "create": { "emit": { "type": "CounterStarted", "payload": { "counter": "$id" } } },
        "operations": {
            "finish": {
                "transitions": [{ "from": "counting", "to": "done" }],
                "emits": [{ "type": "CounterFinished", "payload": { "counter": "$id" } }]
            }
        }
    }))
    .expect("the definition parses");
    let mut registry = Registry::new();
    registry.register(definition).expect("it validates");
    registry
}

#[test]
fn a_forged_field_of_the_wrong_type_is_refused_by_the_schema_not_the_invariant_that_reads_it() {
    // Fields before rules, per event, exactly as `execute` orders them: a wrong-typed field is a
    // schema defect and must be reported as one. Checking the invariants first would answer
    // `count_is_not_negative` instead — the wrong defect, and one that only appears in the
    // definitions where some invariant happens to read the field.
    let registry = counted_registry();
    let definition = registry.get("counter", 1).expect("registered");

    let forged = vec![DomainEvent {
        entity: "counter".to_owned(),
        version: 1,
        id: "c-1".to_owned(),
        revision: 1,
        event_type: "CounterStarted".to_owned(),
        from_state: None,
        to_state: "counting".to_owned(),
        changed: serde_json::from_value(json!({ "count": "many" })).expect("an object"),
        args: serde_json::from_value(json!({ "count": "many" })).expect("an object"),
        payload: json!({}),
    }];

    let error = rehydrate(definition, &forged).expect_err("`count` is an integer");
    assert!(
        matches!(error, entity_core::CoreError::Validation(ref errors)
            if errors[0].path == "events"
                && errors[0].message.contains("event 0 (`CounterStarted`)")
                && errors[0].message.contains("$fields.count: expected integer")),
        "the schema names the defect and the event that carried it: {error}"
    );
    assert!(
        !error.to_string().contains("count_is_not_negative"),
        "the invariant never sees a value the schema has already refused: {error}"
    );
}

/// A task with a field default the caller never sends, an argument default it never sends either, a
/// `set:` on both operations, a precondition reading what the earlier `set:` wrote, and invariants
/// over the result.
fn defaulted_registry() -> Registry {
    let definition = serde_json::from_value(json!({
        "entity": "task",
        "version": 1,
        "schema": {
            "fields": {
                "title": { "type": "string", "required": true },
                "attempts": { "type": "integer", "required": true, "default": 0 },
                "owner": { "type": "string" },
                "resolution": { "type": "string" }
            }
        },
        "lifecycle": { "initial": "open", "states": ["open", "assigned", "closed"] },
        "invariants": [
            {
                "name": "closed_requires_resolution",
                "message": "a closed task carries the resolution it was closed with",
                "assert": {
                    "any": [
                        { "ne": ["$state", "closed"] },
                        { "exists": "$fields.resolution" }
                    ]
                }
            },
            {
                "name": "attempts_is_not_negative",
                "message": "a task cannot have been attempted a negative number of times",
                "assert": { "gte": ["$fields.attempts", 0] }
            }
        ],
        "create": { "emit": { "type": "TaskCreated", "payload": { "task": "$id" } } },
        "operations": {
            "assign": {
                "arguments": { "fields": { "owner": { "type": "string", "required": true } } },
                "transitions": [{ "from": "open", "to": "assigned" }],
                "set": { "owner": "$args.owner" },
                "emits": [{ "type": "TaskAssigned", "payload": { "owner": "$args.owner" } }]
            },
            "close": {
                "arguments": {
                    "fields": {
                        "resolution": { "type": "string", "required": true, "default": "fixed" }
                    }
                },
                "preconditions": [{
                    "name": "owner: assigned",
                    "message": "a task is closed by the owner it was assigned to",
                    "assert": { "exists": "$fields.owner" }
                }],
                "transitions": [{ "from": "assigned", "to": "closed" }],
                "set": { "resolution": "$args.resolution" },
                "emits": [{ "type": "TaskClosed", "payload": { "task": "$id" } }]
            }
        }
    }))
    .expect("the definition parses");
    let mut registry = Registry::new();
    registry.register(definition).expect("it validates");
    registry
}

#[test]
fn an_honest_history_through_set_defaults_and_invariants_folds_to_the_executed_instance() {
    // The other half of a tightened fold: what the kernel actually wrote still folds, byte for
    // byte. Every step here has something the fold has to reproduce rather than trust — a field
    // default, an argument default, two `set:` assignments, a precondition reading an earlier one.
    let registry = defaulted_registry();
    let runtime = Runtime::new(&registry);
    let definition = registry.get("task", 1).expect("registered");

    let created = runtime
        .create("task", 1, "t-1", json!({ "title": "Write it down" }))
        .expect("permitted");
    let assigned = runtime
        .execute(&created.instance, "assign", json!({ "owner": "timo" }))
        .expect("permitted");
    let closed = runtime
        .execute(&assigned.instance, "close", json!({}))
        .expect("permitted");

    assert_eq!(
        created.instance.fields["attempts"],
        json!(0),
        "the field default is part of what the creation event records"
    );
    assert_eq!(
        closed.events[0].args["resolution"],
        json!("fixed"),
        "and the argument default is on the event, which is what the fold resolves `set:` from"
    );

    let history = [
        created.events[0].clone(),
        assigned.events[0].clone(),
        closed.events[0].clone(),
    ];
    assert_eq!(
        rehydrate(definition, &history).expect("the honest history folds"),
        closed.instance,
        "state, revision and every field come back — including the two only a `set:` wrote and the \
         one only a default did"
    );
}

// --- One revision, one decision, every event it emitted ------------------------------------------

/// An order whose `submit` emits two events in one decision, each with a payload the fold can check.
fn two_emit_registry() -> Registry {
    let definition = serde_json::from_value(json!({
        "entity": "order",
        "version": 1,
        "schema": { "fields": {
            "total": { "type": "integer", "required": true },
            "submitted_by": { "type": "string", "default": "nobody" }
        }},
        "lifecycle": { "initial": "draft", "states": ["draft", "submitted"] },
        "create": { "emit": { "type": "OrderDrafted", "payload": { "order": "$id", "total": "$fields.total" } } },
        "operations": {
            "submit": {
                "arguments": { "fields": { "who": { "type": "string", "required": true } } },
                "transitions": [{ "from": "draft", "to": "submitted" }],
                "set": { "submitted_by": "$args.who" },
                "emits": [
                    { "type": "OrderSubmitted", "payload": { "order": "$id", "by": "$args.who" } },
                    { "type": "OrderAudited", "payload": { "order": "$id", "was": "$old_fields.submitted_by", "now": "$fields.submitted_by" } }
                ]
            }
        }
    }))
    .expect("the definition parses");
    let mut registry = Registry::new();
    registry.register(definition).expect("it validates");
    registry
}

fn drafted_and_submitted(registry: &Registry) -> (entity_core::EntityInstance, Vec<DomainEvent>) {
    let runtime = Runtime::new(registry);
    let drafted = runtime
        .create("order", 1, "o-1", json!({ "total": 10 }))
        .expect("permitted");
    let submitted = runtime
        .execute(&drafted.instance, "submit", json!({ "who": "ops" }))
        .expect("permitted");
    assert_eq!(
        submitted.events.len(),
        2,
        "one decision, two events, one revision"
    );
    let mut history = drafted.events.clone();
    history.extend(submitted.events.clone());
    (submitted.instance, history)
}

#[test]
fn an_honest_history_of_an_operation_emitting_two_events_folds_to_the_executed_instance() {
    // Both events carry revision 2. A fold that treated each event as its own decision refused the
    // second as "a history with a gap"; a revision is one decision and holds every event it emitted.
    let registry = two_emit_registry();
    let definition = registry.get("order", 1).expect("registered");
    let (executed, history) = drafted_and_submitted(&registry);
    let folded = rehydrate(definition, &history).expect("an honest two-event revision folds");
    assert_eq!(folded, executed);
}

#[test]
fn a_forged_payload_on_an_operation_event_is_refused() {
    // `changed` and `args` are intact; only the payload says something the template would not have
    // resolved to. In a definition without `set:`, the payload is the only record of who acted.
    let registry = two_emit_registry();
    let definition = registry.get("order", 1).expect("registered");
    let (_, mut history) = drafted_and_submitted(&registry);
    history[1].payload = json!({ "order": "o-1", "by": "somebody-else" });

    let error = rehydrate(definition, &history).expect_err("a payload the template would not emit");
    assert!(
        matches!(error, entity_core::CoreError::Validation(ref errors)
            if errors[0].path == "events"
                && errors[0].message.contains("event 1 (`OrderSubmitted`)")
                && errors[0].message.contains("a payload `submit` would not have emitted")
                && errors[0].message.contains("\"by\":\"ops\"")
                && errors[0].message.contains("\"by\":\"somebody-else\"")),
        "the refusal names the event, the operation and both payloads: {error}"
    );
}

#[test]
fn a_forged_payload_on_the_creation_event_is_refused() {
    let registry = two_emit_registry();
    let definition = registry.get("order", 1).expect("registered");
    let (_, mut history) = drafted_and_submitted(&registry);
    history[0].payload = json!({ "order": "o-1", "total": 999 });

    let error = rehydrate(definition, &history).expect_err("`create` resolves its own payload");
    assert!(
        matches!(error, entity_core::CoreError::Validation(ref errors)
            if errors[0].path == "events"
                && errors[0].message.contains("event 0 (`OrderDrafted`)")
                && errors[0].message.contains("a payload `create` would not have emitted")
                && errors[0].message.contains("\"total\":10")
                && errors[0].message.contains("\"total\":999")),
        "the refusal names the creation event and both payloads: {error}"
    );
}

#[test]
fn two_events_at_one_revision_that_describe_different_decisions_are_refused() {
    // One `execute` call materialises every event of a revision from one context. Two events at
    // one revision that disagree on the arguments were not written by one call.
    let registry = two_emit_registry();
    let definition = registry.get("order", 1).expect("registered");
    let (_, mut history) = drafted_and_submitted(&registry);
    history[2].args.insert("who".to_owned(), json!("impostor"));

    let error = rehydrate(definition, &history).expect_err("one revision, one decision");
    assert!(
        matches!(error, entity_core::CoreError::Validation(ref errors)
            if errors[0].path == "events"
                && errors[0].message.contains("events 1 (`OrderSubmitted`) and 2 (`OrderAudited`)")
                && errors[0].message.contains("share revision 2 but describe different decisions")),
        "the refusal names both events and the revision: {error}"
    );
}

#[test]
fn a_history_missing_one_of_the_two_events_a_decision_emitted_is_refused() {
    // `submit` emits two events; a history carrying only the first was not written by `submit`, and
    // no other operation emits `OrderSubmitted` on that transition.
    let registry = two_emit_registry();
    let definition = registry.get("order", 1).expect("registered");
    let (_, mut history) = drafted_and_submitted(&registry);
    history.truncate(2);

    let error = rehydrate(definition, &history).expect_err("half a decision is no decision");
    assert!(
        matches!(error, entity_core::CoreError::Validation(ref errors)
            if errors[0].path == "events"
                && errors[0].message.contains("event 1 (`OrderSubmitted`)")
                && errors[0].message.contains("`submit`: emits [OrderSubmitted, OrderAudited] on that transition, and the history carries [OrderSubmitted]")),
        "the refusal names the operation's emits and what the history carries: {error}"
    );
}

// --- Cases an adversarial review of the regrouped fold added ------------------------------------

fn register(value: serde_json::Value) -> Registry {
    let definition = serde_json::from_value(value).expect("the definition parses");
    let mut registry = Registry::new();
    registry.register(definition).expect("it validates");
    registry
}

// ---------------------------------------------------------------------------------------------
// 1. Two operations sharing one transition and one event type, distinguished only by the payload.
// ---------------------------------------------------------------------------------------------

/// Two ways to close a ticket. Same transition, same emitted type, same (empty) `set:`, same
/// argument schema — the payload is the only record of which one was taken, which is the case the
/// design says the payload check exists for.
fn two_closers() -> Registry {
    register(json!({
        "entity": "ticket",
        "version": 1,
        "schema": { "fields": { "title": { "type": "string", "required": true } } },
        "lifecycle": { "initial": "open", "states": ["open", "closed"] },
        "create": { "emit": { "type": "TicketOpened", "payload": { "ticket": "$id" } } },
        "operations": {
            "close_as_duplicate": {
                "arguments": { "fields": { "who": { "type": "string", "required": true } } },
                "transitions": [{ "from": "open", "to": "closed" }],
                "emits": [{ "type": "TicketClosed", "payload": {
                    "ticket": "$id", "reason": "duplicate", "by": "$args.who"
                }}]
            },
            "close_as_fixed": {
                "arguments": { "fields": { "who": { "type": "string", "required": true } } },
                "transitions": [{ "from": "open", "to": "closed" }],
                "emits": [{ "type": "TicketClosed", "payload": {
                    "ticket": "$id", "reason": "fixed", "by": "$args.who"
                }}]
            }
        }
    }))
}

fn closed_through(
    registry: &Registry,
    operation: &str,
) -> (entity_core::EntityInstance, Vec<DomainEvent>) {
    let runtime = Runtime::new(registry);
    let opened = runtime
        .create("ticket", 1, "t-1", json!({ "title": "a ticket" }))
        .expect("create is permitted");
    let closed = runtime
        .execute(&opened.instance, operation, json!({ "who": "ops" }))
        .unwrap_or_else(|error| panic!("{operation} is permitted: {error}"));
    let mut history = opened.events.clone();
    history.extend(closed.events.clone());
    (closed.instance, history)
}

#[test]
fn an_honest_history_of_the_first_of_two_payload_distinguished_operations_folds() {
    let registry = two_closers();
    let definition = registry.get("ticket", 1).expect("registered");
    let (executed, history) = closed_through(&registry, "close_as_duplicate");
    let folded = rehydrate(definition, &history).expect("an honest history folds");
    assert_eq!(folded, executed);
}

/// The same definition, the same shape of history, through the operation whose name sorts second.
/// Every candidate that answers for the transition, arguments and fields is kept, and the payloads
/// decide between them — so an honest history of `close_as_fixed` is not measured against
/// `close_as_duplicate`'s template.
#[test]
fn an_honest_history_of_the_second_of_two_payload_distinguished_operations_folds() {
    let registry = two_closers();
    let definition = registry.get("ticket", 1).expect("registered");
    let (executed, history) = closed_through(&registry, "close_as_fixed");
    let folded = rehydrate(definition, &history)
        .expect("an honest history of the second operation folds too");
    assert_eq!(folded, executed);
}

// ---------------------------------------------------------------------------------------------
// 2. An operation that emits nothing, in the middle of a history and at the end of one.
// ---------------------------------------------------------------------------------------------

fn silent_middle() -> Registry {
    register(json!({
        "entity": "order",
        "version": 1,
        "schema": { "fields": { "memo": { "type": "string", "default": "none" } } },
        "lifecycle": { "initial": "draft", "states": ["draft", "checked", "sent"] },
        "create": { "emit": { "type": "OrderDrafted", "payload": { "order": "$id" } } },
        "operations": {
            "check": { "transitions": [{ "from": "draft", "to": "checked" }] },
            "touch": { "transitions": [{ "from": "draft", "to": "draft" }] },
            "submit": {
                "transitions": [{ "from": "draft", "to": "sent" }],
                "emits": [{ "type": "OrderSubmitted", "payload": { "order": "$id" } }]
            },
            "send": {
                "transitions": [{ "from": "checked", "to": "sent" }],
                "emits": [{ "type": "OrderSent", "payload": { "order": "$id" } }]
            }
        }
    }))
}

/// A decision that emits nothing leaves no event. When a later decision did emit, its revision no
/// longer follows the one the fold reached, and the fold refuses it as a gap — the history is
/// unfoldable from the silent decision on, and the refusal says where it stopped.
#[test]
fn a_silent_decision_followed_by_an_emitting_one_makes_the_history_unfoldable_from_there() {
    let registry = silent_middle();
    let definition = registry.get("order", 1).expect("registered");
    let runtime = Runtime::new(&registry);

    let drafted = runtime
        .create("order", 1, "o-1", json!({}))
        .expect("create is permitted");
    let checked = runtime
        .execute(&drafted.instance, "check", json!({}))
        .expect("check is permitted");
    assert!(checked.events.is_empty(), "`check` emits nothing");
    let sent = runtime
        .execute(&checked.instance, "send", json!({}))
        .expect("send is permitted");

    let mut history = drafted.events.clone();
    history.extend(sent.events.clone());
    let error = rehydrate(definition, &history).expect_err("the silent decision left a gap");
    assert!(
        matches!(error, entity_core::CoreError::Validation(ref errors)
            if errors[0].path == "events"
                && errors[0].message.contains("event 1 (`OrderSent`) is at revision 3, but the fold had reached 1")),
        "the refusal names the revision the fold stopped at: {error}"
    );
}

/// The other half of what the fold cannot see: a silent decision that was the last one leaves the
/// fold one decision short, and nothing in the events says so.
#[test]
fn a_trailing_silent_decision_leaves_the_fold_one_decision_short_with_nothing_to_refuse() {
    let registry = silent_middle();
    let definition = registry.get("order", 1).expect("registered");
    let runtime = Runtime::new(&registry);

    let drafted = runtime
        .create("order", 1, "o-3", json!({}))
        .expect("create is permitted");
    let touched = runtime
        .execute(&drafted.instance, "touch", json!({}))
        .expect("touch is permitted");

    let folded = rehydrate(definition, &drafted.events).expect("a trailing silent decision");
    assert_eq!(folded.revision, 1, "one decision short");
    assert_eq!(touched.instance.revision, 2, "the instance really advanced");
}
fn three_emit() -> Registry {
    register(json!({
        "entity": "batch",
        "version": 1,
        "schema": { "fields": {
            "owner": { "type": "string", "default": "nobody" },
            "count": { "type": "integer", "default": 0 }
        }},
        "lifecycle": { "initial": "open", "states": ["open", "run"] },
        "create": { "emit": { "type": "BatchOpened", "payload": {
            "batch": "$id", "owner": "$fields.owner", "count": "$fields.count"
        }}},
        "operations": {
            "run": {
                "arguments": { "fields": {
                    "who": { "type": "string", "required": true },
                    "n": { "type": "integer", "required": true }
                }},
                "transitions": [{ "from": "open", "to": "run" }],
                "set": { "owner": "$args.who", "count": "$args.n" },
                "emits": [
                    { "type": "BatchStarted", "payload": { "by": "$args.who" } },
                    { "type": "BatchCounted", "payload": {
                        "was": "$old_fields.count", "now": "$fields.count"
                    }},
                    { "type": "BatchFinished", "payload": {
                        "state": "$state", "from": "$from_state", "entity": "$entity",
                        "version": "$version", "id": "$id",
                        "literal": "$$fields.owner", "nothing": null,
                        "nested": { "deep": { "who": "$args.who" } },
                        "list": ["$args.who", 100, 100.0, ["$fields.count"]]
                    }}
                ]
            }
        }
    }))
}

fn ran(registry: &Registry) -> (entity_core::EntityInstance, Vec<DomainEvent>) {
    let runtime = Runtime::new(registry);
    let opened = runtime
        .create("batch", 1, "b-1", json!({}))
        .expect("create is permitted");
    let done = runtime
        .execute(&opened.instance, "run", json!({ "who": "ops", "n": 3 }))
        .expect("run is permitted");
    assert_eq!(done.events.len(), 3, "one decision, three events");
    let mut history = opened.events.clone();
    history.extend(done.events.clone());
    (done.instance, history)
}

#[test]
fn an_honest_history_of_three_events_at_one_revision_folds() {
    let registry = three_emit();
    let definition = registry.get("batch", 1).expect("registered");
    let (executed, history) = ran(&registry);
    let folded = rehydrate(definition, &history).expect("three events at one revision fold");
    assert_eq!(folded, executed);
}

#[test]
fn three_events_at_one_revision_in_the_wrong_order_are_refused() {
    let registry = three_emit();
    let definition = registry.get("batch", 1).expect("registered");
    let (_, mut history) = ran(&registry);
    history.swap(1, 3);
    let error = rehydrate(definition, &history).expect_err("a reordering was written by no call");
    assert!(
        format!("{error}").contains("emits [BatchStarted, BatchCounted, BatchFinished]"),
        "the refusal names the operation's emit order: {error}"
    );
}

#[test]
fn a_duplicated_event_at_one_revision_is_refused() {
    let registry = three_emit();
    let definition = registry.get("batch", 1).expect("registered");
    let (_, mut history) = ran(&registry);
    let duplicate = history[1].clone();
    history.insert(2, duplicate);
    let error = rehydrate(definition, &history).expect_err("a superset was written by no call");
    assert!(
        format!("{error}").contains("the history carries [BatchStarted, BatchStarted"),
        "the refusal names what the history carries: {error}"
    );
}

#[test]
fn a_forged_payload_on_the_last_event_of_a_revision_is_refused() {
    let registry = three_emit();
    let definition = registry.get("batch", 1).expect("registered");
    let (_, mut history) = ran(&registry);
    history[3].payload = json!({ "state": "run" });
    let error = rehydrate(definition, &history).expect_err("the third payload is checked too");
    assert!(
        format!("{error}").contains("event 3 (`BatchFinished`) carries a payload"),
        "the refusal names the third event of the revision: {error}"
    );
}

#[test]
fn changing_the_to_state_of_the_second_event_of_a_revision_is_refused() {
    let registry = three_emit();
    let definition = registry.get("batch", 1).expect("registered");
    let (_, mut history) = ran(&registry);
    history[2].to_state = "open".to_owned();
    let error = rehydrate(definition, &history).expect_err("one revision, one transition");
    assert!(
        format!("{error}").contains("share revision 2 but describe different decisions"),
        "the refusal names the disagreement: {error}"
    );
}

#[test]
fn two_creation_events_at_revision_one_are_refused() {
    let registry = three_emit();
    let definition = registry.get("batch", 1).expect("registered");
    let (_, history) = ran(&registry);
    let mut forged = vec![history[0].clone(), history[0].clone()];
    forged.extend(history[1..].iter().cloned());
    let error = rehydrate(definition, &forged).expect_err("`create` emits at most one event");
    assert!(
        format!("{error}").contains("`create` emits at most one"),
        "the refusal names the rule: {error}"
    );
}

// ---------------------------------------------------------------------------------------------
// 6. The per-event identity checks, at a slot no existing case reaches: the second event of a
//    revision. Green here; red under the mutation that stops the loop after the first event.
// ---------------------------------------------------------------------------------------------

#[test]
fn an_event_of_another_instance_at_the_second_slot_of_a_revision_is_refused() {
    let registry = three_emit();
    let definition = registry.get("batch", 1).expect("registered");
    let (_, mut history) = ran(&registry);
    history[2].id = "b-2".to_owned();
    let error = rehydrate(definition, &history).expect_err("one history describes one instance");
    assert!(
        format!("{error}").contains("event 2 (`BatchCounted`) is about `b-2`, not `b-1`"),
        "the refusal names the event and both identities: {error}"
    );
}

#[test]
fn an_event_of_another_entity_at_the_second_slot_of_a_revision_is_refused() {
    let registry = three_emit();
    let definition = registry.get("batch", 1).expect("registered");
    let (_, mut history) = ran(&registry);
    history[2].entity = "other".to_owned();
    let error = rehydrate(definition, &history).expect_err("another definition's event");
    assert!(
        matches!(error, entity_core::CoreError::EntityMismatch { ref actual_entity, .. }
            if actual_entity == "other"),
        "{error}"
    );
}

#[test]
fn an_event_naming_an_unknown_state_at_the_second_slot_of_a_revision_is_refused() {
    let registry = three_emit();
    let definition = registry.get("batch", 1).expect("registered");
    let (_, mut history) = ran(&registry);
    history[2].to_state = "nowhere".to_owned();
    let error = rehydrate(definition, &history).expect_err("a state the definition does not have");
    assert!(
        matches!(error, entity_core::CoreError::UnknownState { ref state, .. }
            if state == "nowhere"),
        "{error}"
    );
}
