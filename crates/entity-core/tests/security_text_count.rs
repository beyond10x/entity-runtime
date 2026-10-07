//! Security review of the `service/1` text length address (R-160): every place registration
//! admits `<path>.count` has to be a place something resolves it (invariant 5), a length is read
//! only from a value that is a text, and every path registration admits untyped keeps the answer
//! it had before the address existed, so no recorded decision replays differently (R-97).

use entity_core::{
    create, execute, replay, CoreError, DefinitionError, DefinitionErrors, EntityDefinition,
    ValidatedDefinition,
};
use serde_json::{json, Value};

fn definition(value: Value) -> EntityDefinition {
    serde_json::from_value(value).expect("the fixture is a definition document")
}

fn validated(value: Value) -> ValidatedDefinition {
    ValidatedDefinition::new(definition(value)).expect("the fixture registers")
}

/// A `service/1` definition over `schema` with one invariant asserting `condition`.
fn probe(schema: Value, condition: Value) -> Value {
    json!({
        "entity": "probe", "version": 1, "semantics": "service/1",
        "schema": schema,
        "lifecycle": { "initial": "held", "states": ["held"] },
        "invariants": [{ "name": "rule", "assert": condition, "message": "the rule" }]
    })
}

/// How the invariant answered at creation: held, violated, or unobservable.
#[derive(Debug, PartialEq)]
enum Answer {
    Held,
    Violated,
    Unobservable(Vec<String>),
}

fn answer(schema: Value, condition: Value, fields: Value) -> Answer {
    match create(
        &validated(probe(schema, condition)),
        "p-1".to_owned(),
        fields,
    ) {
        Ok(_) => Answer::Held,
        Err(CoreError::InvariantViolation { .. }) => Answer::Violated,
        Err(CoreError::InvariantUnobservable { unresolved, .. }) => {
            Answer::Unobservable(unresolved)
        }
        Err(other) => panic!("the probe refused for another reason: {other}"),
    }
}

fn compare(left: &str, op: &str, right: u64) -> Value {
    json!({ "compare": { "left": left, "op": op, "right": right } })
}

/// A `service/1` definition whose only projection is keyed on `key`, over `schema`.
fn projection(schema: Value, key: &str) -> Value {
    json!({
        "entity": "probe", "version": 1, "semantics": "service/1",
        "schema": schema,
        "lifecycle": { "initial": "held", "states": ["held"] },
        "projections": { "by_key": { "key": key } }
    })
}

fn refused_projection(defects: &DefinitionErrors) -> bool {
    defects.iter().any(|defect| {
        matches!(defect, DefinitionError::InvalidTemplate { path, .. } if path == "projections.by_key")
    })
}

// --- invariant 5: a projection key registration admits is one the projection resolves ---------

/// `validation.rs` checks a projection key with `validate_reference` so that *"a projection naming
/// a field the schema does not have ... is refused where it is written rather than producing an
/// empty read model at run time"*, and `entity_store::project` resolves a key by walking objects
/// only (`crates/entity-store/src/projection.rs:77-81`), relying on registration to refuse
/// anything else. Before R-160 `$fields.<text>.count` was refused there; now it registers, and
/// every instance is left out of the read model.
#[test]
fn a_projection_keyed_on_a_text_length_is_refused_where_it_is_written() {
    let document = projection(
        json!({ "fields": { "name": { "type": "string", "required": true } } }),
        "$fields.name.count",
    );
    let Err(defects) = ValidatedDefinition::new(definition(document)) else {
        panic!(
            "a projection keyed on `$fields.name.count` registered, but the store's projection \
             walks objects only, so this read model files no instance"
        );
    };
    assert!(refused_projection(&defects), "{defects}");
}

/// The collection address forms R-148 added, which registration admits in a projection key under
/// `service/1`. The base registered them and replay re-validates every recorded definition, so
/// they keep registering unchanged; since R-167 the store resolves them as the kernel does, so the
/// key resolves where it is written. The kernel's reading of the address is asserted here, and the
/// store filing the instance under it in `entity-store`'s `tests/projections.rs`.
#[test]
fn a_projection_keyed_on_an_array_count_registers_where_it_is_written_and_reads_the_size() {
    let mut document = projection(
        json!({ "fields": { "tags": {
            "type": "array", "required": true, "items": { "type": "string" }
        }}}),
        "$fields.tags.count",
    );
    document["create"] =
        json!({ "emit": { "type": "Probed", "payload": { "read": "$fields.tags.count" } } });
    let document = definition(document);
    let registered = ValidatedDefinition::new(document.clone()).unwrap_or_else(|defects| {
        panic!(
            "a projection keyed on `$fields.tags.count` registered on the base, so a history \
             recorded under it must keep replaying: {defects}"
        )
    });
    assert_eq!(
        *registered, document,
        "the base registered this projection unchanged"
    );
    let decision = create(
        &registered,
        "p-1".to_owned(),
        json!({ "tags": ["x", "y", "z"] }),
    )
    .expect("the instance is created");
    assert_eq!(
        decision.events[0].payload["read"],
        json!(3),
        "the kernel reads the key's address as the array's size, the key the store files it under"
    );
}

// --- § 10.6: the only `count` a map has is its size, wherever the map is reached --------------

/// Registration admits `$m.count` on a quantifier element declared as a `map` (the element's
/// declaration is walked), but the run-time walk reads a binder's element with no declaration
/// (`runtime.rs` `resolve_expression_optional`, `walk(element, path, None, false, ..)`), so a
/// map element is walked as an ordinary object: `count` reads its own `count` member when it has
/// one, and nothing when it does not, instead of its size. The base answered it so, and replay
/// re-validates every recorded definition, so it keeps doing so until
/// story:binder-elements-carry-their-declaration.
#[test]
fn a_map_inside_a_quantifier_element_reads_its_count_member_until_binder_elements_carry_their_declaration(
) {
    let schema = json!({ "fields": { "maps": {
        "type": "array", "required": true,
        "items": { "type": "map", "key": "string", "items": { "type": "integer" } }
    }}});
    let one = json!({ "for_all": {
        "in": "$fields.maps", "as": "m", "that": compare("$m.count", "eq", 1)
    }});
    // Both answers in one comparison, so a run shows each. Each one-member map has size 1, so both
    // become `Held` when the element carries its declaration.
    assert_eq!(
        (
            answer(schema.clone(), one.clone(), json!({ "maps": [{ "a": 7 }] })),
            answer(schema, one, json!({ "maps": [{ "count": 5 }] })),
        ),
        (
            Answer::Unobservable(vec!["$m.count".to_owned()]),
            Answer::Violated
        ),
        "'$m.count' reads the element's `count` member (absent, then 5), as on the base; when \
         story:binder-elements-carry-their-declaration lands it reads the size and this case flips \
         to (Held, Held)"
    );
}

// --- integrity: a length is read only from a value that is a text --------------------------------

/// `execute` reads the stored instance's fields in its preconditions before revalidating them, and
/// which instance reaches the kernel is the shell's responsibility (AGENTS.md invariant 4, R-80).
/// A declared `string` holding an object is not a text, so it has no length: the rule is
/// unobservable. The walk instead falls through to the object's own `count` member after the
/// text arm declines, as the map arm prevents for a declared map (`runtime.rs` `walk`).
#[test]
fn a_stored_text_that_holds_no_text_answers_no_length() {
    let definition = validated(json!({
        "entity": "probe", "version": 1, "semantics": "service/1",
        "schema": { "fields": { "name": { "type": "string", "required": true } } },
        "lifecycle": { "initial": "held", "states": ["held"] },
        "operations": { "rename": {
            "transitions": [{ "from": "held", "to": "held" }],
            "arguments": { "fields": { "name": { "type": "string", "required": true } } },
            "preconditions": [{
                "name": "short",
                "assert": compare("$fields.name.count", "lte", 5),
                "message": "the current name is at most five characters"
            }],
            "set": { "name": "$args.name" }
        }}
    }));
    let mut instance = create(&definition, "p-1".to_owned(), json!({ "name": "Ann" }))
        .expect("creates")
        .instance;
    instance
        .fields
        .insert("name".to_owned(), json!({ "count": 1 }));

    let outcome = execute(&definition, &instance, "rename", json!({ "name": "Bo" }));
    assert!(
        matches!(
            &outcome,
            Err(CoreError::PreconditionUnobservable { unresolved, .. })
                if unresolved == &["$fields.name.count".to_owned()]
        ),
        "a declared text holding an object answered a length: {outcome:?}"
    );
}

// --- R-97: every path registration admits untyped keeps resolving to nothing ---------------------

/// The paths the implementor's test does not cover: a quantifier element of an untyped
/// collection, a union payload reached through an array element, and an undeclared argument of an
/// open argument schema. Each registered before R-160 and resolved to nothing then, so each must
/// still be unobservable. (An array with no declared item kind is refused as a schema, so it is no
/// such path.)
#[test]
fn a_text_length_through_an_untyped_element_or_argument_keeps_resolving_to_nothing() {
    let three = |address: &str| compare(address, "eq", 3);
    let unobservable = |address: &str| Answer::Unobservable(vec![address.to_owned()]);

    // A quantifier over a path inside a `json` field: the element kind is undeclared.
    assert_eq!(
        answer(
            json!({ "fields": { "blob": { "type": "json" } } }),
            json!({ "for_all": {
                "in": "$fields.blob.list", "as": "t", "that": three("$t.count")
            }}),
            json!({ "blob": { "list": ["Ann"] } })
        ),
        unobservable("$t.count"),
        "a quantifier element of an untyped collection"
    );

    // A union's string variant, reached through a declared array's element.
    assert_eq!(
        answer(
            json!({ "fields": { "payees": {
                "type": "array", "required": true,
                "items": { "type": "union", "tag": "kind", "required": true, "variants": {
                    "person": { "type": "string", "required": true }
                }}
            }}}),
            three("$fields.payees.0.value.count"),
            json!({ "payees": [{ "kind": "person", "value": "Ann" }] })
        ),
        unobservable("$fields.payees.0.value.count"),
        "a union payload inside an array element"
    );

    // An undeclared argument under an open argument schema, read by a precondition.
    let definition = validated(json!({
        "entity": "probe", "version": 1, "semantics": "service/1",
        "schema": { "fields": {} },
        "lifecycle": { "initial": "held", "states": ["held"] },
        "operations": { "touch": {
            "transitions": [{ "from": "held", "to": "held" }],
            "arguments": { "fields": {}, "additional_fields": true },
            "preconditions": [{ "name": "three", "assert": three("$args.note.count"), "message": "three" }]
        }}
    }));
    let instance = create(&definition, "p-1".to_owned(), json!({}))
        .expect("creates")
        .instance;
    let outcome = execute(&definition, &instance, "touch", json!({ "note": "Ann" }));
    assert!(
        matches!(
            &outcome,
            Err(CoreError::PreconditionUnobservable { unresolved, .. })
                if unresolved == &["$args.note.count".to_owned()]
        ),
        "an undeclared argument of an open schema: {outcome:?}"
    );
}

// --- invariant 2 and R-97 through templates ---------------------------------------------------

/// A `set` template and an event payload resolve the length through the same walk as a rule. The
/// decision is byte-identical when computed twice and replays to the instance it produced;
/// `kernel/1` refuses the same template at registration.
#[test]
fn a_text_length_in_a_template_is_deterministic_and_replays() {
    let document = |semantics: &str| {
        json!({
            "entity": "probe", "version": 1, "semantics": semantics,
            "schema": { "fields": {
                "name": { "type": "string", "required": true },
                "length": { "type": "integer" }
            }},
            "lifecycle": { "initial": "held", "states": ["held"] },
            "operations": { "rename": {
                "transitions": [{ "from": "held", "to": "held" }],
                "arguments": { "fields": { "name": { "type": "string", "required": true } } },
                "set": { "name": "$args.name", "length": "$args.name.count" },
                "emits": [{ "type": "renamed", "payload": {
                    "length": "$args.name.count", "previous": "$old_fields.name.count"
                }}]
            }}
        })
    };

    let definition = validated(document("service/1"));
    let created = create(&definition, "p-1".to_owned(), json!({ "name": "Ann" })).expect("creates");
    let run = || {
        execute(
            &definition,
            &created.instance,
            "rename",
            json!({ "name": "h\u{e9}llo" }),
        )
        .expect("renames")
    };
    let first = run();
    let second = run();
    assert_eq!(
        serde_json::to_vec(&first).expect("serialises"),
        serde_json::to_vec(&second).expect("serialises"),
        "the same inputs produced different decision bytes"
    );
    assert_eq!(first.instance.fields["length"], json!(5));
    assert_eq!(
        first.events[0].payload,
        json!({ "length": 5, "previous": 3 })
    );
    let rebuilt = replay(&[created.record.clone(), first.record.clone()]).expect("replays");
    assert_eq!(rebuilt, first.instance);

    let Err(defects) = ValidatedDefinition::new(definition_kernel(document("kernel/1"))) else {
        panic!("`kernel/1` registered a text length in a `set` template");
    };
    assert!(
        defects.iter().any(|defect| matches!(
            defect,
            DefinitionError::InvalidTemplate { path, .. } if path == "operations.rename.set.length"
        )),
        "{defects}"
    );
}

/// A `kernel/1` document carries no `semantics` key and no `emits` list.
fn definition_kernel(mut document: Value) -> EntityDefinition {
    let object = document.as_object_mut().expect("an object");
    object.remove("semantics");
    let rename = &mut object["operations"]["rename"];
    rename.as_object_mut().expect("an object").remove("emits");
    definition(document)
}
