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
        args: serde_json::Map::new(),
        payload: json!({}),
    }];

    let error = rehydrate(definition, &forged).expect_err("`invented` is not a declared field");
    assert!(error.to_string().contains("invented"), "{error}");
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
        args: serde_json::Map::new(),
        payload: json!({}),
    }];

    rehydrate(definition, &forged).expect_err("`title` is a string");
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
    assert!(rehydrate(definition, &unobserved).is_err());
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
