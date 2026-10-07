//! A sequence of decisions produces the read model the definition declared.

use std::collections::BTreeSet;

use entity_core::{create, CoreError, EntityInstance, Registry, Runtime, ValidatedDefinition};
use entity_store::{project, Expect, Grouping, MemoryStore, StateProvider, Store};
use serde_json::{json, Value};

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

// --- R-167: a service projection key reads a collection address as the kernel reads it ---------

/// Every field a collection address case reads, each optional so one case sets only its own: an
/// array of texts, an array of objects, a declared map, an object with a property named `count`,
/// an untyped `json` value, and a union whose variants are a map and an array.
fn collection_schema() -> Value {
    json!({ "fields": {
        "tags": { "type": "array", "items": { "type": "string" } },
        "rows": { "type": "array", "items": { "type": "object", "properties": {
            "name": { "type": "string", "required": true }
        }}},
        "meta": { "type": "map", "key": "string", "items": { "type": "string" } },
        "stats": { "type": "object", "properties": {
            "count": { "type": "integer", "required": true }
        }},
        "payload": { "type": "json" },
        "choice": { "type": "union", "tag": "kind", "variants": {
            "bag": { "type": "map", "required": true, "key": "string", "items": { "type": "string" } },
            "list": { "type": "array", "required": true, "items": { "type": "string" } }
        }}
    }})
}

/// A definition under `semantics` whose one projection is keyed on `key`. With `reads`, its
/// creation event carries what the kernel resolves the same address to, under `read`.
fn keyed_on(semantics: &str, schema: Value, key: &str, reads: bool) -> ValidatedDefinition {
    let mut document = json!({
        "entity": "probe", "version": 1, "semantics": semantics,
        "schema": schema,
        "lifecycle": { "initial": "held", "states": ["held"] },
        "projections": { "by_key": { "key": key } }
    });
    if reads {
        document["create"] = json!({ "emit": { "type": "Probed", "payload": { "read": key } } });
    }
    let definition = serde_json::from_value(document).expect("the fixture is a definition");
    ValidatedDefinition::new(definition)
        .unwrap_or_else(|defects| panic!("a projection keyed on '{key}' registers: {defects}"))
}

/// The `service/1` read model keyed on `key` over one instance per `(id, fields)`, each created by
/// the kernel, so every instance is one the kernel accepted.
fn filed(key: &str, instances: &[(&str, Value)]) -> Grouping {
    let definition = keyed_on("service/1", collection_schema(), key, false);
    let held: Vec<EntityInstance> = instances
        .iter()
        .map(|(id, fields)| {
            create(&definition, (*id).to_owned(), fields.clone())
                .expect("the kernel accepts the instance")
                .instance
        })
        .collect();
    project(&definition, &held)
        .remove("by_key")
        .expect("the projection is declared")
}

fn group(entries: &[(&str, &[&str])]) -> Grouping {
    entries
        .iter()
        .map(|(key, ids)| {
            let ids: BTreeSet<String> = ids.iter().map(|id| (*id).to_owned()).collect();
            ((*key).to_owned(), ids)
        })
        .collect()
}

/// An array's `count` is its size under `service/1`, so an instance is filed under it — an empty
/// array under `0`, not left out. Before R-167 the key walked object members only and the read
/// model filed no instance, though registration admitted the key since R-148.
#[test]
fn a_service_projection_keyed_on_an_array_count_files_each_instance_under_its_size() {
    let grouping = filed(
        "$fields.tags.count",
        &[
            ("two", json!({ "tags": ["x", "y"] })),
            ("none", json!({ "tags": [] })),
        ],
    );
    assert_eq!(grouping, group(&[("0", &["none"]), ("2", &["two"])]));
}

/// `<array>.<n>` is the element at that index, and the walk continues under the array's declared
/// items; an index the array does not reach resolves to nothing, so that instance is left out
/// (R-100) rather than filed under an empty key.
#[test]
fn a_service_projection_keyed_on_an_array_element_files_each_instance_under_that_element() {
    let first = filed(
        "$fields.tags.0",
        &[
            ("two", json!({ "tags": ["x", "y"] })),
            ("none", json!({ "tags": [] })),
        ],
    );
    assert_eq!(first, group(&[("x", &["two"])]));

    let second_row = filed(
        "$fields.rows.1.name",
        &[
            ("short", json!({ "rows": [{ "name": "r1" }] })),
            (
                "long",
                json!({ "rows": [{ "name": "r1" }, { "name": "r2" }] }),
            ),
        ],
    );
    assert_eq!(second_row, group(&[("r2", &["long"])]));
}

/// § 10.6: the only `count` a declared map has is its size. A map holding a member named `count`
/// is filed under its size, where the object walk before R-167 filed it under that member's value.
#[test]
fn a_service_projection_keyed_on_a_map_count_files_each_instance_under_its_size_not_a_member_named_count(
) {
    let grouping = filed(
        "$fields.meta.count",
        &[
            ("one", json!({ "meta": { "a": "b" } })),
            ("member", json!({ "meta": { "a": "b", "count": "7" } })),
        ],
    );
    assert_eq!(grouping, group(&[("1", &["one"]), ("2", &["member"])]));
}

/// `kernel/1` resolves no collection address: its walk reads object members only, so a `json`
/// value's `count` is a member named `count` and an array under it files nothing. Every read model
/// a `kernel/1` definition produced before R-167 is the same bytes after it.
#[test]
fn a_kernel_1_projection_key_keeps_walking_object_members_only() {
    let schema = json!({ "fields": { "payload": { "type": "json" } } });
    let model = |key: &str| {
        let definition = keyed_on("kernel/1", schema.clone(), key, false);
        let held: Vec<EntityInstance> = [
            ("member", json!({ "payload": { "count": "m" } })),
            ("array", json!({ "payload": ["a", "b"] })),
        ]
        .into_iter()
        .map(|(id, fields)| {
            create(&definition, id.to_owned(), fields)
                .expect("the kernel accepts the instance")
                .instance
        })
        .collect();
        project(&definition, &held)
            .remove("by_key")
            .expect("the projection is declared")
    };
    assert_eq!(model("$fields.payload.count"), group(&[("m", &["member"])]));
    assert_eq!(model("$fields.payload.0"), Grouping::new());
}

/// How a read model spells a resolved key: a text as itself, a number or boolean as its JSON
/// spelling, and nothing for a null or a structural value (R-100).
fn spelled(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Number(_) | Value::Bool(_) => Some(value.to_string()),
        _ => None,
    }
}

/// The store's key walk is a copy of the kernel's (`entity-core` `runtime.rs` `lookup`/`walk`),
/// because the kernel's is private; this case is what keeps the copy from drifting. For every
/// collection address form — declared and untyped, through an object property, an array's items
/// and a union's selected variant — the key an instance is filed under is exactly the value the
/// kernel's creation event reads for the same address of the same instance, and an address the
/// kernel resolves to nothing files no instance.
#[test]
fn every_collection_address_key_files_an_instance_under_the_value_the_kernel_reads() {
    let service = [
        ("$fields.tags.count", json!({ "tags": ["x", "y"] })),
        ("$fields.tags.count", json!({ "tags": [] })),
        ("$fields.tags.0", json!({ "tags": ["x", "y"] })),
        ("$fields.tags.1", json!({ "tags": ["x", "y"] })),
        ("$fields.tags.0", json!({ "tags": [] })),
        ("$fields.rows.0.name", json!({ "rows": [{ "name": "r1" }] })),
        ("$fields.rows.0", json!({ "rows": [{ "name": "r1" }] })),
        (
            "$fields.meta.count",
            json!({ "meta": { "a": "b", "count": "7" } }),
        ),
        ("$fields.meta.count", json!({ "meta": {} })),
        ("$fields.stats.count", json!({ "stats": { "count": 5 } })),
        (
            "$fields.payload.count",
            json!({ "payload": { "count": "m" } }),
        ),
        (
            "$fields.payload.count",
            json!({ "payload": ["a", "b", "c"] }),
        ),
        (
            "$fields.payload.list.0",
            json!({ "payload": { "list": [true] } }),
        ),
        (
            "$fields.payload.list.01",
            json!({ "payload": { "list": ["a", "b"] } }),
        ),
        (
            "$fields.payload.list.+1",
            json!({ "payload": { "list": ["a", "b"] } }),
        ),
        (
            "$fields.payload.list.count.x",
            json!({ "payload": { "list": ["a"] } }),
        ),
        (
            "$fields.choice.value.count",
            json!({ "choice": { "kind": "bag", "value": { "a": "b", "count": "7" } } }),
        ),
        (
            "$fields.choice.value.count",
            json!({ "choice": { "kind": "list", "value": ["x"] } }),
        ),
        (
            "$fields.choice.value.0",
            json!({ "choice": { "kind": "list", "value": ["x"] } }),
        ),
        (
            "$fields.choice.kind",
            json!({ "choice": { "kind": "list", "value": ["x"] } }),
        ),
    ]
    .map(|(key, fields)| ("service/1", collection_schema(), key, fields));
    let kernel = [
        (
            "$fields.payload.count",
            json!({ "payload": { "count": "m" } }),
        ),
        ("$fields.payload.count", json!({ "payload": ["a"] })),
        ("$fields.payload.0", json!({ "payload": ["a"] })),
    ]
    .map(|(key, fields)| {
        let schema = json!({ "fields": { "payload": { "type": "json" } } });
        ("kernel/1", schema, key, fields)
    });

    let mut disagreements = Vec::new();
    for (semantics, schema, key, fields) in service.into_iter().chain(kernel) {
        let definition = keyed_on(semantics, schema.clone(), key, true);
        let (kernel_reads, instance) = match create(&definition, "p-1".to_owned(), fields.clone()) {
            Ok(decision) => (
                spelled(&decision.events[0].payload["read"]),
                decision.instance,
            ),
            Err(CoreError::Template {
                expression,
                message,
            }) if expression == key && message == "referenced value does not exist" => {
                // The kernel reads nothing here; the instance it would hold is the same fields.
                let definition = keyed_on(semantics, schema.clone(), key, false);
                let instance = create(&definition, "p-1".to_owned(), fields.clone())
                    .expect("the kernel accepts the instance")
                    .instance;
                (None, instance)
            }
            Err(other) => panic!("{semantics} {key} over {fields}: the kernel refused: {other}"),
        };
        let filed: Option<String> = project(&definition, [&instance])
            .remove("by_key")
            .expect("the projection is declared")
            .into_keys()
            .next();
        if filed != kernel_reads {
            disagreements.push(format!(
                "{semantics} {key} over {fields}: the store files it under {filed:?}, the kernel \
                 reads {kernel_reads:?}"
            ));
        }
    }
    assert!(
        disagreements.is_empty(),
        "the projection key walk drifted from the kernel's:\n{}",
        disagreements.join("\n")
    );
}
