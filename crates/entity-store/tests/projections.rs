//! A sequence of decisions produces the read model the definition declared.

use entity_core::{EntityInstance, Registry, Runtime};
use entity_store::{project, Expect, MemoryStore, StateProvider, Store};
use serde_json::json;

fn registry() -> Registry {
    let definition = serde_json::from_value(json!({
        "entity": "ticket",
        "version": 1,
        "schema": {
            "fields": {
                "title": { "type": "string", "required": true },
                "customer": { "type": "string", "required": true }
            }
        },
        "lifecycle": { "initial": "open", "states": ["open", "closed"] },
        "operations": {
            "close": { "transitions": [{ "from": "open", "to": "closed" }] }
        },
        "projections": {
            "by_status": { "key": "$state" },
            "open_per_customer": { "key": "$fields.customer", "in_state": "open" }
        }
    }))
    .expect("the definition parses");
    let mut registry = Registry::new();
    registry.register(definition).expect("it validates");
    registry
}

/// Creates three tickets, closes one, and returns the store holding them.
fn three_tickets(registry: &Registry) -> MemoryStore {
    let runtime = Runtime::new(registry);
    let mut store = MemoryStore::new();

    for (id, customer) in [("a", "acme"), ("b", "acme"), ("c", "globex")] {
        let created = runtime
            .create(
                "ticket",
                1,
                id,
                json!({ "title": "A ticket", "customer": customer }),
            )
            .expect("permitted");
        store.commit(&created, Expect::Absent).expect("accepted");
    }

    let held = store.load("ticket", "c").expect("answers").expect("held");
    let closed = runtime
        .execute(&held, "close", json!({}))
        .expect("permitted");
    store
        .commit(&closed, Expect::Revision(1))
        .expect("accepted");
    store
}

#[test]
fn a_sequence_of_decisions_produces_the_declared_read_model() {
    let registry = registry();
    let store = three_tickets(&registry);
    let definition = registry.get("ticket", 1).expect("registered");

    let instances: Vec<EntityInstance> = store.instances().cloned().collect();
    let models = project(definition, &instances);

    let by_status = &models["by_status"];
    assert_eq!(by_status["open"], ["a".to_owned(), "b".to_owned()].into());
    assert_eq!(by_status["closed"], ["c".to_owned()].into());

    // `in_state` is doing work: `c` belongs to globex and is closed, so globex has no open tickets
    // and does not appear at all — rather than appearing with an empty set, which reads as a
    // customer who exists and has nothing, and is a different fact.
    let per_customer = &models["open_per_customer"];
    assert_eq!(
        per_customer["acme"],
        ["a".to_owned(), "b".to_owned()].into()
    );
    assert!(!per_customer.contains_key("globex"));
}

#[test]
fn a_projection_is_the_same_bytes_every_run() {
    // A read model that reordered between runs makes every diff of one unreadable.
    let registry = registry();
    let store = three_tickets(&registry);
    let definition = registry.get("ticket", 1).expect("registered");
    let instances: Vec<EntityInstance> = store.instances().cloned().collect();

    let once = serde_json::to_string(&project(definition, &instances)).expect("serialises");
    let twice = serde_json::to_string(&project(definition, &instances)).expect("serialises");
    assert_eq!(once, twice);
}

#[test]
fn a_projection_naming_a_field_the_schema_does_not_have_is_refused_at_registration() {
    // Refused where it is written, rather than producing a read model that is silently always
    // empty — which is the hardest kind of wrong to notice, because nothing ever errors.
    let definition = serde_json::from_value(json!({
        "entity": "ticket",
        "version": 1,
        "schema": { "fields": { "title": { "type": "string" } } },
        "lifecycle": { "initial": "open", "states": ["open"] },
        "projections": { "by_owner": { "key": "$fields.owner" } }
    }))
    .expect("the definition parses");

    let mut registry = Registry::new();
    let error = registry
        .register(definition)
        .expect_err("a key naming a field nothing declares is refused");
    assert!(error.to_string().contains("owner"), "{error}");
}

#[test]
fn a_projection_naming_a_state_the_lifecycle_does_not_have_is_refused_at_registration() {
    let definition = serde_json::from_value(json!({
        "entity": "ticket",
        "version": 1,
        "schema": { "fields": { "title": { "type": "string" } } },
        "lifecycle": { "initial": "open", "states": ["open"] },
        "projections": { "archived": { "key": "$id", "in_state": "archived" } }
    }))
    .expect("the definition parses");

    let mut registry = Registry::new();
    let error = registry
        .register(definition)
        .expect_err("a state the lifecycle does not declare is refused");
    assert!(
        error.to_string().contains("the lifecycle does not declare"),
        "{error}"
    );
}

#[test]
fn an_instance_whose_key_resolves_to_nothing_is_left_out_rather_than_filed_under_an_empty_key() {
    // R-100's second clause, which had no test: the registry the other tests use declares
    // `customer` required, so a null key could never arise in them. A key that resolves to nothing
    // must drop the instance from the read model — filing it under `""` invents a group nobody
    // asked for, and it is the group everything broken ends up in.
    let definition = serde_json::from_value(json!({
        "entity": "ticket",
        "version": 1,
        "schema": {
            "fields": {
                "title": { "type": "string", "required": true },
                "customer": { "type": "string" }
            }
        },
        "lifecycle": { "initial": "open", "states": ["open", "closed"] },
        "operations": {
            "close": { "transitions": [{ "from": "open", "to": "closed" }] }
        },
        "projections": {
            "by_customer": { "key": "$fields.customer" }
        }
    }))
    .expect("parses");
    let mut registry = Registry::new();
    registry.register(definition).expect("validates");
    let runtime = Runtime::new(&registry);

    let with_key = runtime
        .create(
            "ticket",
            1,
            "has-one",
            json!({ "title": "A", "customer": "acme" }),
        )
        .expect("permitted");
    let without = runtime
        .create("ticket", 1, "has-none", json!({ "title": "B" }))
        .expect("permitted");

    let definition = registry.get("ticket", 1).expect("registered");
    let instances: Vec<EntityInstance> = vec![with_key.instance, without.instance];
    let projected = project(definition, &instances);
    let by_customer = projected.get("by_customer").expect("declared");

    assert_eq!(
        by_customer.keys().collect::<Vec<_>>(),
        vec!["acme"],
        "only the instance whose key resolved appears"
    );
    assert!(
        !by_customer.contains_key(""),
        "and nothing was filed under an empty key"
    );
}
