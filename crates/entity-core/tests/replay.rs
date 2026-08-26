//! Folding an instance back out of its events.
//!
//! The two claims worth pinning are the story's own: a create plus n operations, folded from their
//! events, equals the instance the operations returned — and an invented event whose transition the
//! lifecycle does not permit is refused rather than replayed.

use entity_core::{rehydrate, Registry, Runtime};
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
