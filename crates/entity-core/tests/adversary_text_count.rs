//! Adversarial cases against `<text>.count` (R-160): where registration admits a length address
//! that nothing resolves, where a sibling collection address answers the wrong value, and the
//! registration guards the unit's own suite leaves unpinned.

use entity_core::{
    create, replay, CoreError, DefinitionError, DefinitionErrors, EntityDefinition, Registry,
    Runtime, Truth, ValidatedDefinition,
};
use serde_json::{json, Value};

fn definition(value: Value) -> EntityDefinition {
    serde_json::from_value(value).expect("the fixture is a definition document")
}

fn refused(value: Value) -> DefinitionErrors {
    ValidatedDefinition::new(definition(value)).expect_err("the fixture is refused")
}

/// A `service/1` definition whose only rule is the invariant `condition`, over `schema`.
fn probe(schema: Value, condition: Value) -> Value {
    json!({
        "entity": "probe",
        "version": 1,
        "semantics": "service/1",
        "schema": schema,
        "lifecycle": { "initial": "held", "states": ["held"] },
        "invariants": [{ "name": "rule", "assert": condition, "message": "the rule" }]
    })
}

/// What one invariant answers about one instance: a decision is `True`, a violation `False`, an
/// unobservable refusal `Unknown`.
fn answer(schema: Value, condition: Value, fields: Value) -> Truth {
    let validated = ValidatedDefinition::new(definition(probe(schema, condition)))
        .expect("the probe registers");
    match create(&validated, "p-1".to_owned(), fields) {
        Ok(_) => Truth::True,
        Err(CoreError::InvariantViolation { .. }) => Truth::False,
        Err(CoreError::InvariantUnobservable { .. }) => Truth::Unknown,
        Err(other) => panic!("the probe refused for another reason: {other}"),
    }
}

/// A `service/1` definition with one projection keyed on `key`.
fn projected(schema: Value, key: &str) -> Value {
    json!({
        "entity": "probe",
        "version": 1,
        "semantics": "service/1",
        "schema": schema,
        "lifecycle": { "initial": "held", "states": ["held"] },
        "projections": { "by_key": { "key": key } }
    })
}

fn refused_as_projection_key(defects: &DefinitionErrors, key: &str) -> bool {
    defects.iter().any(|defect| {
        matches!(
            defect,
            DefinitionError::InvalidTemplate { path, message }
                if path == "projections.by_key" && message.contains(key)
        )
    })
}

/// A projection key is validated with the rule walk (`validation.rs:167-183`), whose stated
/// purpose is that a key resolving to nothing is "refused where it is written rather than producing
/// an empty read model at run time". The shell resolves the key with an object-only walk
/// (`entity-store/src/projection.rs:70-95`) that has no length form, so `$fields.title.count`
/// files no instance under any key. The base refused this key at registration.
#[test]
fn a_projection_keyed_on_a_text_length_is_refused_at_registration() {
    let document = projected(
        json!({ "fields": { "title": { "type": "string", "required": true } } }),
        "$fields.title.count",
    );
    let outcome = ValidatedDefinition::new(definition(document));
    let Err(defects) = outcome else {
        panic!(
            "a projection keyed on '$fields.title.count' registered, and the shell's key walk \
             resolves it to nothing for every instance"
        );
    };
    assert!(
        refused_as_projection_key(&defects, "$fields.title.count"),
        "{defects}"
    );
}

/// The two R-148 collection forms, which the base already admitted, keep registering exactly as on
/// the base, because replay re-validates every recorded definition. Since R-167 the store reads
/// such a key as the kernel reads the same address of the same instance, so the instance is filed
/// under its array's size: the kernel's half is asserted here, and the store's in `entity-store`'s
/// `tests/projections.rs`, which this crate cannot reach (`purity.rs` pins its dependencies).
#[test]
fn a_projection_keyed_on_an_array_count_registers_as_on_the_base_and_reads_the_size() {
    let mut document = projected(
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
            "a projection keyed on '$fields.tags.count' registered on the base, so a history \
             recorded under it must keep replaying: {defects}"
        )
    });
    assert_eq!(
        *registered, document,
        "the base registered this projection unchanged"
    );
    let decision = create(&registered, "p-1".to_owned(), json!({ "tags": ["x", "y"] }))
        .expect("the instance is created");
    assert_eq!(
        decision.events[0].payload["read"],
        json!(2),
        "the kernel reads the key's address as the array's size, the key the store files it under"
    );
}

/// § 10.6: "the only `count` a `map` has is its size". Registration admits `$g.count` on a
/// quantifier element declared `map` (`walk_field_path`'s map arm), but the run-time walk reads a
/// binder's element without its declaration (`runtime.rs:2803`), so `count` reads the member named
/// `count`, or nothing. The base registered and answered it that way, and replay re-validates every
/// recorded definition, so it keeps doing so until story:binder-elements-carry-their-declaration.
#[test]
fn a_map_count_inside_a_quantifier_reads_the_member_until_binder_elements_carry_their_declaration()
{
    let map = json!({ "type": "map", "key": "string", "items": { "type": "integer" } });
    let schema =
        json!({ "fields": { "groups": { "type": "array", "required": true, "items": map } } });
    let at_most_one = json!({ "for_all": { "in": "$fields.groups", "as": "g", "that": {
        "compare": { "left": "$g.count", "op": "lte", "right": 1 }
    }}});
    let until = "as on the base; when story:binder-elements-carry-their-declaration lands this \
                 reads the size instead, and this case flips to the size's answer";

    // One entry whose key is `count`: the member (5) is read, not the size (1), so the rule fails.
    assert_eq!(
        answer(
            schema.clone(),
            at_most_one.clone(),
            json!({ "groups": [{ "count": 5 }] })
        ),
        Truth::False,
        "'$g.count' reads the element's `count` member, {until}"
    );
    // Two entries and no `count` member: nothing is read, not the size (2).
    assert_eq!(
        answer(
            schema,
            at_most_one,
            json!({ "groups": [{ "a": 1, "b": 2 }] })
        ),
        Truth::Unknown,
        "'$g.count' on an element with no `count` member reads nothing, {until}"
    );

    // Outside a quantifier the walk has the declaration, so the same map is addressed by its size.
    let mut declared = map;
    declared["required"] = json!(true);
    assert_eq!(
        answer(
            json!({ "fields": { "group": declared } }),
            json!({ "compare": { "left": "$fields.group.count", "op": "lte", "right": 1 } }),
            json!({ "group": { "count": 5 } })
        ),
        Truth::True,
        "a declared map of one entry has size 1, not the value of its `count` member"
    );
}

/// The unit's binder guard is pinned only for `$t.count` directly on a `List<String>` element.
/// The `typed` flag also has to survive the object-property and array-index arms of
/// `walk_field_path`, or a length one level inside the element is admitted and resolves to nothing
/// at every evaluation.
#[test]
fn a_text_length_one_level_inside_a_quantifier_element_is_refused_at_registration() {
    for (items, address) in [
        (
            json!({ "type": "object", "required": true, "properties": {
                "name": { "type": "string", "required": true }
            }}),
            "$t.name.count",
        ),
        (
            json!({ "type": "array", "required": true, "items": { "type": "string" } }),
            "$t.0.count",
        ),
    ] {
        let defects = refused(probe(
            json!({ "fields": { "rows": { "type": "array", "required": true, "items": items } } }),
            json!({ "for_all": { "in": "$fields.rows", "as": "t", "that": {
                "compare": { "left": address, "op": "lte", "right": 8 }
            }}}),
        ));
        let expected =
            format!("'{address}' reads the length of a text inside a quantifier element");
        assert!(
            defects.iter().any(|defect| matches!(
                defect,
                DefinitionError::QuantifierBodyScope { detail, .. } if detail.contains(&expected)
            )),
            "{address}: {defects}"
        );
    }
}

/// Unclaimed by the unit: a `set` template and an event payload resolve the length through the
/// same walk, and the decision that wrote it replays.
#[test]
fn a_text_length_in_a_set_template_and_an_event_payload_resolves_and_replays() {
    let document = json!({
        "entity": "probe", "version": 1, "semantics": "service/1",
        "schema": { "fields": {
            "name": { "type": "string", "required": true },
            "length": { "type": "integer" }
        }},
        "lifecycle": { "initial": "held", "states": ["held"] },
        "operations": {
            "rename": {
                "transitions": [{ "from": "held", "to": "held" }],
                "arguments": { "fields": { "name": { "type": "string", "required": true } } },
                "set": { "name": "$args.name", "length": "$args.name.count" },
                "emits": [{ "type": "Renamed", "payload": { "length": "$fields.name.count" } }]
            }
        }
    });
    let mut registry = Registry::new();
    registry
        .register(definition(document))
        .expect("a text length registers in a set template and an event payload");
    let runtime = Runtime::new(&registry);
    let created = runtime
        .create("probe", 1, "p-1", json!({ "name": "Ann" }))
        .expect("creates");
    let renamed = runtime
        .execute(
            &created.instance,
            "rename",
            json!({ "name": "e\u{301}\u{1F600}" }),
        )
        .expect("renames");
    assert_eq!(renamed.instance.fields["length"], json!(3));
    assert_eq!(renamed.events.len(), 1);
    assert_eq!(renamed.events[0].payload, json!({ "length": 3 }));
    let rebuilt = replay(&[created.record.clone(), renamed.record.clone()]).expect("replays");
    assert_eq!(rebuilt, renamed.instance);
}

// --- pass 2: what the correction's new refusals do to decisions already recorded ----------------

/// The creation record a kernel that admitted `snapshot` wrote for `fields` — base `b7362882` and
/// every release since 0.19.0, which shipped the collection address forms.
///
/// `snapshot` no longer registers, so the decision is taken under `registrable` — the same document
/// without the refused part, which takes the same decision — and the record then carries `snapshot`,
/// as the old kernel's record did. `replay` rebuilds every record from the snapshot it carries
/// (`replay.rs:121-131`), so the snapshot is what decides whether the history still replays.
fn recorded_under(
    snapshot: Value,
    registrable: Value,
    fields: Value,
) -> (entity_core::DecisionRecord, entity_core::EntityInstance) {
    let validated =
        ValidatedDefinition::new(definition(registrable)).expect("the registrable half registers");
    let decision = create(&validated, "p-1".to_owned(), fields).expect("the decision is taken");
    let mut record = decision.record;
    record.definition = Some(definition(snapshot));
    (record, decision.instance)
}

/// The unit claims "recorded decisions replay unchanged" (CHANGELOG, R-160). A projection is
/// performed by the shell and takes no part in any decision, yet a stored decision whose definition
/// snapshot declares a projection keyed on an R-148 collection address — which every release since
/// 0.19.0 registered — no longer replays: `replay` re-validates the snapshot and the correction's
/// projection-key refusal refuses it.
#[test]
fn a_decision_recorded_under_a_projection_keyed_on_a_collection_address_still_replays() {
    let schema = json!({ "fields": {
        "tags": { "type": "array", "required": true, "items": { "type": "string" } },
        "meta": { "type": "map", "required": true, "key": "string", "items": { "type": "string" } }
    }});
    let document = |projections: Value| {
        json!({
            "entity": "probe", "version": 1, "semantics": "service/1",
            "schema": schema.clone(),
            "lifecycle": { "initial": "held", "states": ["held"] },
            "projections": projections
        })
    };
    let mut stranded = Vec::new();
    for key in ["$fields.tags.count", "$fields.tags.0", "$fields.meta.count"] {
        let (record, instance) = recorded_under(
            document(json!({ "by_key": { "key": key } })),
            document(json!({})),
            json!({ "tags": ["x"], "meta": { "a": "b" } }),
        );
        match replay(&[record]) {
            Ok(rebuilt) if rebuilt == instance => {}
            other => stranded.push(format!("{key}: {other:?}")),
        }
    }
    assert!(
        stranded.is_empty(),
        "a recorded decision no longer replays:\n{}",
        stranded.join("\n")
    );
}

/// The same for the map-size refusal under a binder: base registered `$g.count` on a map element,
/// and a creation whose invariant held under it — `{count: 1}` is at most five whether `count` is
/// read as the member or as the size — was recorded. That record no longer replays.
#[test]
fn a_decision_recorded_under_a_quantifier_reading_a_map_count_still_replays() {
    let schema = json!({ "fields": { "groups": {
        "type": "array",
        "required": true,
        "items": { "type": "map", "key": "string", "items": { "type": "integer" } }
    }}});
    let snapshot = probe(
        schema.clone(),
        json!({ "for_all": { "in": "$fields.groups", "as": "g", "that": {
            "compare": { "left": "$g.count", "op": "lte", "right": 5 }
        }}}),
    );
    let registrable = probe(schema, json!(true));
    let (record, instance) =
        recorded_under(snapshot, registrable, json!({ "groups": [{ "count": 1 }] }));
    match replay(&[record]) {
        Ok(rebuilt) => assert_eq!(rebuilt, instance),
        Err(error) => panic!("a recorded decision no longer replays: {error:?}"),
    }
}
