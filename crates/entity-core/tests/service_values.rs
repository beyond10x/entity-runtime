//! The `service/1` value domain: the three new field kinds, the source number observation, the
//! scale context, the four new operators and the two collection address forms.

use entity_core::{
    create, CoreError, DefinitionError, DefinitionErrors, EntityDefinition, Registry, Truth,
    ValidatedDefinition,
};
use serde_json::{json, Value};

fn definition(value: Value) -> EntityDefinition {
    serde_json::from_value(value).expect("the fixture is a definition document")
}

fn refused(value: Value) -> DefinitionErrors {
    ValidatedDefinition::new(definition(value)).expect_err("the fixture is refused")
}

fn carries(defects: &DefinitionErrors, expected: &DefinitionError) -> bool {
    defects.iter().any(|defect| defect == expected)
}

/// A definition whose only rule is `condition`, over `schema`, under `semantics`.
fn probe(semantics: &str, schema: Value, scales: Value, condition: Value) -> Value {
    let mut document = json!({
        "entity": "probe",
        "version": 1,
        "schema": schema,
        "lifecycle": { "initial": "held", "states": ["held"] },
        "invariants": [{ "name": "rule", "assert": condition, "message": "the rule" }]
    });
    if semantics == "service/1" {
        document["semantics"] = json!("service/1");
        if !scales.as_object().expect("an object").is_empty() {
            document["scales"] = scales;
        }
    }
    document
}

/// What one condition answers about one instance, read through the invariant it is written as.
///
/// A decision means `True`, a violation means `False`, and an unobservable refusal means `Unknown`
/// — which is the distinction the three-valued rules exist to keep, and the one every `compare`,
/// `truthy` and quantifier row below turns on.
fn answer(semantics: &str, schema: Value, scales: Value, condition: Value, fields: Value) -> Truth {
    let document = probe(semantics, schema, scales, condition);
    let validated = ValidatedDefinition::new(definition(document)).expect("the probe registers");
    match create(&validated, "p-1".to_owned(), fields) {
        Ok(_) => Truth::True,
        Err(CoreError::InvariantViolation { .. }) => Truth::False,
        Err(CoreError::InvariantUnobservable { .. }) => Truth::Unknown,
        Err(other) => panic!("the probe refused for another reason: {other}"),
    }
}

/// The same, where the schema is one optional `json` field named `value`.
fn about(semantics: &str, condition: Value, value: Value) -> Truth {
    answer(
        semantics,
        json!({ "fields": { "value": { "type": "json" }, "other": { "type": "json" } } }),
        json!({}),
        condition,
        json!({ "value": value }),
    )
}

// --- § 10.1: the three new field kinds ------------------------------------------------------------

#[test]
fn a_map_field_types_its_values_and_refuses_an_untyped_member() {
    let schema = json!({ "fields": { "metadata": {
        "type": "map", "key": "string", "items": { "type": "string" }, "required": true
    }}});
    assert_eq!(
        answer(
            "service/1",
            schema.clone(),
            json!({}),
            true.into(),
            json!({ "metadata": { "a": "one", "b": "two" } })
        ),
        Truth::True
    );

    let validated = ValidatedDefinition::new(definition(probe(
        "service/1",
        schema,
        json!({}),
        true.into(),
    )))
    .expect("registers");
    let Err(CoreError::Validation(errors)) = create(
        &validated,
        "p-1".to_owned(),
        json!({ "metadata": { "a": 1 } }),
    ) else {
        panic!("a map's values are typed by `items`");
    };
    assert_eq!(errors[0].path, "fields.metadata.a");
    assert_eq!(errors[0].message, "expected string");

    // A map that declares no value definition, or no key spelling, is refused where it is written.
    let defects = refused(probe(
        "service/1",
        json!({ "fields": { "metadata": { "type": "map" } } }),
        json!({}),
        true.into(),
    ));
    assert!(carries(
        &defects,
        &DefinitionError::MapValueMissing {
            path: "schema.metadata".to_owned()
        }
    ));
    assert!(carries(
        &defects,
        &DefinitionError::MapKeyNotText {
            path: "schema.metadata".to_owned()
        }
    ));
}

#[test]
fn a_map_with_an_integer_key_refuses_a_key_that_is_not_integer_text() {
    let schema = json!({ "fields": { "counts": {
        "type": "map", "key": "integer", "items": { "type": "integer" }, "required": true
    }}});
    let validated = ValidatedDefinition::new(definition(probe(
        "service/1",
        schema,
        json!({}),
        true.into(),
    )))
    .expect("registers");
    assert!(create(
        &validated,
        "p-1".to_owned(),
        json!({ "counts": { "7": 1, "-2": 3 } })
    )
    .is_ok());
    let Err(CoreError::Validation(errors)) = create(
        &validated,
        "p-1".to_owned(),
        json!({ "counts": { "seven": 1 } }),
    ) else {
        panic!("a key is checked by its spelling");
    };
    assert_eq!(errors[0].path, "fields.counts.seven");
    assert!(errors[0].message.contains("integer text"));
}

#[test]
fn a_union_field_accepts_the_adjacent_tagged_form_and_refuses_an_unknown_tag() {
    let validated = payee_probe("kind");
    assert!(create(
        &validated,
        "p-1".to_owned(),
        json!({ "payee": { "kind": "person", "value": "a@b.c" } })
    )
    .is_ok());
    let Err(CoreError::Validation(errors)) = create(
        &validated,
        "p-1".to_owned(),
        json!({ "payee": { "kind": "alien", "value": "a@b.c" } }),
    ) else {
        panic!("an unknown variant is refused");
    };
    assert_eq!(errors[0].path, "fields.payee");
    assert!(errors[0]
        .message
        .contains("'alien' is not one of [company, person]"));

    // A key that is neither the tag nor the content is refused by name.
    let Err(CoreError::Validation(errors)) = create(
        &validated,
        "p-1".to_owned(),
        json!({ "payee": { "kind": "person", "value": "a@b.c", "extra": 1 } }),
    ) else {
        panic!("a union carries exactly two keys");
    };
    assert_eq!(errors[0].path, "fields.payee.extra");
}

#[test]
fn a_union_tagged_value_reads_its_content_under_content() {
    // The content key is derived, not declared: `value`, or `content` where the tag is `value`.
    let validated = payee_probe("value");
    assert!(create(
        &validated,
        "p-1".to_owned(),
        json!({ "payee": { "value": "person", "content": "a@b.c" } })
    )
    .is_ok());
    let Err(CoreError::Validation(errors)) = create(
        &validated,
        "p-1".to_owned(),
        json!({ "payee": { "value": "person", "value2": "a@b.c" } }),
    ) else {
        panic!("the derived content key is where the payload is read");
    };
    assert_eq!(errors[0].path, "fields.payee.value2");

    // And a definition-time read of the content resolves under `service/1`.
    let condition = json!({ "eq": ["$fields.payee.content", "a@b.c"] });
    assert_eq!(
        answer(
            "service/1",
            payee_schema("value"),
            json!({}),
            condition,
            json!({ "payee": { "value": "person", "content": "a@b.c" } })
        ),
        Truth::True
    );
}

/// The precondition for the billing acceptance fixture to exist at all: nothing in that entity is
/// refused by type.
#[test]
fn the_billing_invoice_entity_lowers_with_every_field_typed_and_none_refused() {
    let document = json!({
        "entity": "billing_invoice",
        "version": 1,
        "semantics": "service/1",
        "identity": { "field": "invoice_id" },
        "schema": { "fields": {
            "invoice_id": { "type": "string", "required": true },
            "account_id": { "type": "string", "required": true },
            "total": { "type": "number", "required": true },
            "currency": { "type": "enum", "values": ["EUR", "USD"], "required": true },
            "issued_at": { "type": "string", "required": true },
            "payee": { "type": "union", "tag": "kind", "required": true, "variants": {
                "person": { "type": "string", "required": true },
                "company": { "type": "object", "required": true, "properties": {
                    "name": { "type": "string", "required": true },
                    "registration": { "type": "string" }
                }}
            }},
            "metadata": { "type": "map", "key": "string", "items": { "type": "string" }, "required": true },
            "lines": { "type": "array", "required": true, "items": { "type": "object", "properties": {
                "sku": { "type": "string", "required": true },
                "quantity": { "type": "integer", "required": true },
                "unit_price": { "type": "number", "required": true }
            }}},
            "weight": { "type": "binary64" }
        }},
        "lifecycle": { "initial": "draft", "states": ["draft", "issued"] },
        "invariants": [{
            "name": "every_unit_price_is_not_negative",
            "assert": { "for_all": { "in": "$fields.lines", "as": "line", "that": {
                "compare": { "left": "$line.unit_price", "op": "gte", "right": 0 }
            }}},
            "message": "a line's unit price is not negative"
        }]
    });
    let validated = ValidatedDefinition::new(definition(document)).expect("registers");
    let decision = create(
        &validated,
        "s:INV-1".to_owned(),
        json!({
            "invoice_id": "INV-1",
            "account_id": "ACC-1",
            "total": "10.50",
            "currency": "EUR",
            "issued_at": "2026-09-16",
            "payee": { "kind": "company", "value": { "name": "Acme" } },
            "metadata": { "source": "portal" },
            "lines": [{ "sku": "A", "quantity": 2, "unit_price": 5 }],
            "weight": 1.5
        }),
    );
    // `total` is a string here only to prove the typed refusal is the one that fires.
    let Err(CoreError::Validation(errors)) = decision else {
        panic!("the typed refusal names the field");
    };
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].path, "fields.total");

    assert!(create(
        &validated,
        "s:INV-1".to_owned(),
        json!({
            "invoice_id": "INV-1",
            "account_id": "ACC-1",
            "total": 10.5,
            "currency": "EUR",
            "issued_at": "2026-09-16",
            "payee": { "kind": "company", "value": { "name": "Acme" } },
            "metadata": { "source": "portal" },
            "lines": [{ "sku": "A", "quantity": 2, "unit_price": 5 }],
            "weight": 1.5
        })
    )
    .is_ok());
}

// --- § 10.2 and § 10.2.1: the numeric domain ------------------------------------------------------

#[test]
fn a_binary64_predicate_is_admitted_and_negative_zero_equals_zero() {
    let schema = json!({ "fields": { "amount": { "type": "binary64", "required": true } } });
    for token in ["-0.0", "0", "0.0"] {
        let value: Value = serde_json::from_str(token).expect("a number");
        assert_eq!(
            answer(
                "service/1",
                schema.clone(),
                json!({}),
                json!({ "compare": { "left": "$fields.amount", "op": "eq", "right": 0 } }),
                json!({ "amount": value })
            ),
            Truth::True,
            "{token} is not one value with 0"
        );
    }
    assert_eq!(
        answer(
            "service/1",
            schema.clone(),
            json!({}),
            json!({ "compare": { "left": "$fields.amount", "op": "gt", "right": 0 } }),
            json!({ "amount": 1.5 })
        ),
        Truth::True
    );
    // A value the source's own constructor refuses has no place in the field at all.
    let validated = ValidatedDefinition::new(definition(probe(
        "service/1",
        schema,
        json!({}),
        true.into(),
    )))
    .expect("registers");
    let huge: Value = serde_json::from_str("{\"amount\": 1e400}").expect("a document");
    let Err(CoreError::Validation(errors)) = create(&validated, "p-1".to_owned(), huge) else {
        panic!("an infinite binary64 is refused at the field kind");
    };
    assert_eq!(errors[0].path, "fields.amount");
    assert!(errors[0].message.contains("source observation domain"));
}

#[test]
fn a_binary64_field_keeps_the_sign_of_its_zero_through_creation_event_and_replay() {
    let document = json!({
        "entity": "weighed",
        "version": 1,
        "semantics": "service/1",
        "schema": { "fields": { "amount": { "type": "binary64", "required": true } } },
        "lifecycle": { "initial": "held", "states": ["held"] },
        "create": { "emit": { "type": "Weighed", "payload": { "amount": "$fields.amount" } } }
    });
    let validated = ValidatedDefinition::new(definition(document)).expect("registers");
    let fields: Value = serde_json::from_str("{\"amount\": -0.0}").expect("a document");
    let decision = create(&validated, "p-1".to_owned(), fields).expect("creates");
    let stored = serde_json::to_string(&decision.instance.fields).expect("serializes");
    assert!(
        stored.contains("-0.0"),
        "the sign survives in the bytes: {stored}"
    );
    let published = serde_json::to_string(&decision.events[0].payload).expect("serializes");
    assert!(published.contains("-0.0"), "{published}");
    let rebuilt = entity_core::replay(std::slice::from_ref(&decision.record)).expect("replays");
    assert_eq!(
        serde_json::to_string(&rebuilt.fields).expect("serializes"),
        stored
    );
}

#[test]
fn an_exact_decimal_survives_creation_set_event_and_replay_unrounded() {
    let token = "1.0000000000000000001";
    let document = json!({
        "entity": "exact",
        "version": 1,
        "semantics": "service/1",
        "schema": { "fields": { "amount": { "type": "number", "required": true } } },
        "lifecycle": { "initial": "held", "states": ["held", "seen"] },
        "create": { "emit": { "type": "Seen", "payload": { "amount": "$fields.amount" } } },
        "operations": { "look": {
            "arguments": { "fields": {} },
            "outcomes": [{
                "name": "looked",
                "effect": { "moves": { "to": "seen", "from": "held" } },
                "set": { "amount": "$fields.amount" },
                "emits": [{ "type": "Looked", "payload": { "amount": "$fields.amount" } }]
            }]
        }}
    });
    let validated = ValidatedDefinition::new(definition(document)).expect("registers");
    let fields: Value =
        serde_json::from_str(&format!("{{\"amount\": {token}}}")).expect("document");
    let created = create(&validated, "p-1".to_owned(), fields).expect("creates");
    assert_eq!(created.instance.fields["amount"].to_string(), token);
    assert_eq!(created.events[0].payload["amount"].to_string(), token);
    let looked =
        entity_core::execute(&validated, &created.instance, "look", json!({})).expect("looks");
    assert_eq!(looked.instance.fields["amount"].to_string(), token);
    let rebuilt =
        entity_core::replay(&[created.record.clone(), looked.record.clone()]).expect("replays");
    assert_eq!(rebuilt.fields["amount"].to_string(), token);
}

#[test]
fn a_service_integer_beyond_the_i64_range_is_refused_while_kernel_1_still_admits_it() {
    let schema = json!({ "fields": { "count": { "type": "integer", "required": true } } });
    let beyond: Value =
        serde_json::from_str("{\"count\": 18446744073709551615}").expect("a document");

    let service = ValidatedDefinition::new(definition(probe(
        "service/1",
        schema.clone(),
        json!({}),
        true.into(),
    )))
    .expect("registers");
    let Err(CoreError::Validation(errors)) = create(&service, "p-1".to_owned(), beyond.clone())
    else {
        panic!("the source integer range is the admitted one");
    };
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].path, "fields.count");
    assert_eq!(
        errors[0].message,
        "value 18446744073709551615 is outside the source integer range \
         [-9223372036854775808, 9223372036854775807]"
    );

    let kernel = ValidatedDefinition::new(definition(probe(
        "kernel/1",
        schema,
        json!({}),
        true.into(),
    )))
    .expect("registers");
    assert!(
        create(&kernel, "p-1".to_owned(), beyond).is_ok(),
        "kernel/1 is untouched"
    );
}

#[test]
fn a_decimal_binary64_does_not_carry_answers_ne_false_and_its_negation_true() {
    let schema = json!({ "fields": { "amount": { "type": "number", "required": true } } });
    let fields: Value =
        serde_json::from_str("{\"amount\": 1.0000000000000000001}").expect("a document");
    assert_eq!(
        answer(
            "service/1",
            schema.clone(),
            json!({}),
            json!({ "compare": { "left": "$fields.amount", "op": "ne", "right": 1 } }),
            fields.clone()
        ),
        Truth::False,
        "the source observes exactly 1"
    );
    assert_eq!(
        answer(
            "service/1",
            schema,
            json!({}),
            json!({ "not": { "compare": { "left": "$fields.amount", "op": "ne", "right": 1 } } }),
            fields
        ),
        Truth::True,
        "the negation of one answer, not a second rule"
    );
}

#[test]
fn that_same_token_is_stored_and_replayed_byte_for_byte_after_answering_equal() {
    let token = "1.0000000000000000001";
    let document = probe(
        "service/1",
        json!({ "fields": { "amount": { "type": "number", "required": true } } }),
        json!({}),
        json!({ "compare": { "left": "$fields.amount", "op": "eq", "right": 1 } }),
    );
    let validated = ValidatedDefinition::new(definition(document)).expect("registers");
    let fields: Value =
        serde_json::from_str(&format!("{{\"amount\": {token}}}")).expect("document");
    let decision = create(&validated, "p-1".to_owned(), fields).expect("the guard answers equal");
    assert_eq!(decision.instance.fields["amount"].to_string(), token);
    let rebuilt = entity_core::replay(std::slice::from_ref(&decision.record)).expect("replays");
    assert_eq!(rebuilt.fields["amount"].to_string(), token);
}

#[test]
fn a_binary64_underflow_token_is_falsy_and_is_still_stored_unrounded() {
    let token = "1e-400";
    let document = probe(
        "service/1",
        json!({ "fields": { "amount": { "type": "number", "required": true } } }),
        json!({}),
        json!({ "not": { "truthy": "$fields.amount" } }),
    );
    let validated = ValidatedDefinition::new(definition(document)).expect("registers");
    let fields: Value =
        serde_json::from_str(&format!("{{\"amount\": {token}}}")).expect("document");
    let decision = create(&validated, "p-1".to_owned(), fields).expect("the token is falsy");
    assert_eq!(decision.instance.fields["amount"].to_string(), "1e-400");
}

#[test]
fn two_adjacent_integers_past_2_53_are_distinguished_on_the_exact_integer_path() {
    let schema = json!({ "fields": { "count": { "type": "integer", "required": true } } });
    let fields: Value = serde_json::from_str("{\"count\": 9007199254740993}").expect("a document");
    assert_eq!(
        answer(
            "service/1",
            schema.clone(),
            json!({}),
            json!({ "compare": { "left": "$fields.count", "op": "gt", "right": 9007199254740992i64 } }),
            fields.clone()
        ),
        Truth::True
    );
    assert_eq!(
        answer(
            "service/1",
            schema,
            json!({}),
            json!({ "compare": { "left": "$fields.count", "op": "eq", "right": 9007199254740992i64 } }),
            fields
        ),
        Truth::False
    );
}

/// The measured profile of the actual service-producing source tool, read end to end through a
/// definition rather than through the module's own unit test.
#[test]
fn the_versioned_observation_matches_the_actual_source_cli_profile() {
    let schema = json!({ "fields": { "amount": { "type": "number", "required": true } } });
    let fields: Value =
        serde_json::from_str("{\"amount\": 946.3702156715110866946}").expect("a document");
    // The CLI profile reads this token as binary64 bits 408d92f633a24cdb, whose shortest
    // round-tripping decimal is 946.3702156715111; the default-feature library profile reads
    // 408d92f633a24cda, whose decimal is 946.370215671511. The two are different profiles and are
    // not conflated: this asserts the one the contract fixes.
    assert_eq!(
        answer(
            "service/1",
            schema.clone(),
            json!({}),
            json!({ "compare": { "left": "$fields.amount", "op": "eq", "right": 946.3702156715111f64 } }),
            fields.clone()
        ),
        Truth::True
    );
    assert_eq!(
        answer(
            "service/1",
            schema.clone(),
            json!({}),
            json!({ "compare": { "left": "$fields.amount", "op": "eq", "right": 946.370215671511f64 } }),
            fields.clone()
        ),
        Truth::False,
        "the default-feature profile's reading is not this rule's answer"
    );
    // Signed zero is one value, the literal and wire doors stay separate, and a wire token outside
    // the domain refuses with a path rather than panicking.
    let validated = ValidatedDefinition::new(definition(probe(
        "service/1",
        schema,
        json!({}),
        true.into(),
    )))
    .expect("registers");
    let huge: Value = serde_json::from_str("{\"amount\": 1e400}").expect("a document");
    assert!(matches!(
        create(&validated, "p-1".to_owned(), huge),
        Err(CoreError::Validation(_))
    ));
}

#[test]
fn service_numeric_equality_membership_and_bounds_all_use_the_observation_rule() {
    let schema = json!({ "fields": { "amount": { "type": "number", "required": true } } });
    let fields: Value =
        serde_json::from_str("{\"amount\": 1.0000000000000000001}").expect("a document");
    for (operator, condition) in [
        ("eq", json!({ "eq": ["$fields.amount", 1] })),
        ("ne", json!({ "not": { "ne": ["$fields.amount", 1] } })),
        ("in", json!({ "in": ["$fields.amount", [1]] })),
        ("contains", json!({ "contains": [[1], "$fields.amount"] })),
    ] {
        assert_eq!(
            answer(
                "service/1",
                schema.clone(),
                json!({}),
                condition,
                fields.clone()
            ),
            Truth::True,
            "{operator} answers what compare answers"
        );
    }
    // A schema bound is an authored literal, so the source reads it through the literal door and
    // the stored value through the wire door — and a value the source observes as exactly 1 is
    // inside `max: 1`.
    let bounded =
        json!({ "fields": { "amount": { "type": "number", "required": true, "max": 1 } } });
    let validated = ValidatedDefinition::new(definition(probe(
        "service/1",
        bounded,
        json!({}),
        true.into(),
    )))
    .expect("registers");
    assert!(create(&validated, "p-1".to_owned(), fields).is_ok());
}

#[test]
fn kernel_1_number_operators_and_bounds_answer_exactly_what_they_answer_today() {
    let schema = json!({ "fields": { "amount": { "type": "number", "required": true } } });
    let fields: Value =
        serde_json::from_str("{\"amount\": 1.0000000000000000001}").expect("a document");
    // The exact token, which is what `kernel/1` has always compared.
    assert_eq!(
        answer(
            "kernel/1",
            schema.clone(),
            json!({}),
            json!({ "ne": ["$fields.amount", 1] }),
            fields.clone()
        ),
        Truth::True
    );
    assert_eq!(
        answer(
            "kernel/1",
            schema.clone(),
            json!({}),
            json!({ "gt": ["$fields.amount", 1] }),
            fields.clone()
        ),
        Truth::True
    );
    // And a bound refuses the exact value, as it always has.
    let bounded =
        json!({ "fields": { "amount": { "type": "number", "required": true, "max": 1 } } });
    let validated = ValidatedDefinition::new(definition(probe(
        "kernel/1",
        bounded,
        json!({}),
        true.into(),
    )))
    .expect("registers");
    let Err(CoreError::Validation(errors)) = create(&validated, "p-1".to_owned(), fields) else {
        panic!("the kernel/1 bound reads the exact token");
    };
    assert_eq!(errors[0].path, "fields.amount");

    // `gt` over two non-numbers is two-valued `false`, which is why `compare` is its own operator.
    assert_eq!(
        answer(
            "kernel/1",
            json!({ "fields": { "name": { "type": "string", "required": true } } }),
            json!({}),
            json!({ "gt": ["$fields.name", "a"] }),
            json!({ "name": "b" })
        ),
        Truth::False
    );
}

#[test]
fn a_record_replays_under_the_number_observation_its_definition_snapshot_names() {
    let document = probe(
        "service/1",
        json!({ "fields": { "amount": { "type": "number", "required": true } } }),
        json!({}),
        json!({ "compare": { "left": "$fields.amount", "op": "eq", "right": 1 } }),
    );
    let validated = ValidatedDefinition::new(definition(document)).expect("registers");
    let fields: Value =
        serde_json::from_str("{\"amount\": 1.0000000000000000001}").expect("a document");
    let decision = create(&validated, "p-1".to_owned(), fields).expect("creates");
    let snapshot = decision
        .record
        .definition
        .as_ref()
        .expect("a snapshot")
        .number_observation;
    assert_eq!(snapshot, entity_core::NumberObservation::SourceNumber1);
    assert!(entity_core::replay(&[decision.record]).is_ok());
}

// --- § 10.3: the scale context --------------------------------------------------------------------

#[test]
fn a_timestamp_comparison_with_no_declared_scale_is_unknown_and_not_false() {
    assert_eq!(
        answer(
            "service/1",
            json!({ "fields": { "issued_at": { "type": "string", "required": true } } }),
            json!({}),
            json!({ "compare": { "left": "$fields.issued_at", "op": "lt", "right": "2026-09-17" } }),
            json!({ "issued_at": "2026-09-16" })
        ),
        Truth::Unknown,
        "the absence of a scale is a value of the context, never false"
    );
}

#[test]
fn a_timestamp_comparison_inside_a_declared_scale_answers_by_rank() {
    let scales = json!({ "days": ["2026-09-16", "2026-09-17"] });
    let schema = json!({ "fields": { "issued_at": { "type": "string", "required": true } } });
    assert_eq!(
        answer(
            "service/1",
            schema.clone(),
            scales.clone(),
            json!({ "compare": { "left": "$fields.issued_at", "op": "lt", "right": "2026-09-17" } }),
            json!({ "issued_at": "2026-09-16" })
        ),
        Truth::True
    );
    assert_eq!(
        answer(
            "service/1",
            schema,
            scales,
            json!({ "compare": { "left": "$fields.issued_at", "op": "gt", "right": "2026-09-17" } }),
            json!({ "issued_at": "2026-09-16" })
        ),
        Truth::False
    );
}

#[test]
fn two_declared_scales_that_disagree_about_one_pair_answer_unknown() {
    let scales = json!({ "rising": ["low", "high"], "falling": ["high", "low"] });
    assert_eq!(
        answer(
            "service/1",
            json!({ "fields": { "level": { "type": "string", "required": true } } }),
            scales,
            json!({ "compare": { "left": "$fields.level", "op": "lt", "right": "high" } }),
            json!({ "level": "low" })
        ),
        Truth::Unknown
    );
}

#[test]
fn the_declared_scales_are_snapshotted_into_the_record_and_replay_reads_the_snapshot() {
    let document = probe(
        "service/1",
        json!({ "fields": { "level": { "type": "string", "required": true } } }),
        json!({ "rising": ["low", "high"] }),
        json!({ "compare": { "left": "$fields.level", "op": "lt", "right": "high" } }),
    );
    let validated = ValidatedDefinition::new(definition(document)).expect("registers");
    let decision =
        create(&validated, "p-1".to_owned(), json!({ "level": "low" })).expect("creates");
    let snapshot = decision.record.definition.as_ref().expect("a snapshot");
    assert_eq!(
        snapshot.scales["rising"],
        vec!["low".to_owned(), "high".to_owned()]
    );
    // Replay reads the snapshot, so the rule is answered under the scales that decided it — no
    // lookup, no IO, no clock.
    assert!(entity_core::replay(&[decision.record]).is_ok());
}

// --- § 10.4: the four new operators ---------------------------------------------------------------

#[test]
fn compare_answers_unknown_where_gt_answers_false_for_two_non_numbers() {
    let schema = json!({ "fields": { "flag": { "type": "boolean", "required": true } } });
    assert_eq!(
        answer(
            "service/1",
            schema.clone(),
            json!({}),
            json!({ "compare": { "left": "$fields.flag", "op": "gt", "right": false } }),
            json!({ "flag": true })
        ),
        Truth::Unknown,
        "a boolean against a boolean has no ordering"
    );
    assert_eq!(
        answer(
            "kernel/1",
            schema,
            json!({}),
            json!({ "gt": ["$fields.flag", false] }),
            json!({ "flag": true })
        ),
        Truth::False,
        "and the existing operator keeps its two-valued answer"
    );
}

#[test]
fn compare_over_a_resolved_array_operand_is_unknown() {
    assert_eq!(
        about(
            "service/1",
            json!({ "compare": { "left": "$fields.value", "op": "eq", "right": "$fields.other" } }),
            json!([1, 2])
        ),
        Truth::Unknown,
        "a list has no scalar spelling, so this is unevaluable by construction"
    );
    // A literal list or mapping operand never reaches run time: it is refused where it is written.
    let defects = refused(probe(
        "service/1",
        json!({ "fields": { "value": { "type": "json" } } }),
        json!({}),
        json!({ "compare": { "left": "$fields.value", "op": "eq", "right": [1, 2] } }),
    ));
    assert!(carries(
        &defects,
        &DefinitionError::CompareOperandNotAddressable {
            path: "invariants[0].assert".to_owned(),
            side: "right",
        }
    ));
}

#[test]
fn truthy_over_text_is_true_unless_empty_or_the_word_false() {
    for (value, expected) in [
        (json!("yes"), Truth::True),
        (json!("0"), Truth::True),
        (json!(""), Truth::False),
        (json!("false"), Truth::False),
        (json!("False"), Truth::True),
    ] {
        assert_eq!(
            about(
                "service/1",
                json!({ "truthy": "$fields.value" }),
                value.clone()
            ),
            expected,
            "{value}"
        );
    }
}

#[test]
fn truthy_over_a_number_is_the_zero_test_and_over_nothing_is_unknown() {
    for (token, expected) in [
        ("1", Truth::True),
        ("0", Truth::False),
        ("-0.0", Truth::False),
        ("1e-400", Truth::False),
        ("0.5", Truth::True),
    ] {
        let value: Value = serde_json::from_str(token).expect("a number");
        assert_eq!(
            about("service/1", json!({ "truthy": "$fields.value" }), value),
            expected,
            "{token}"
        );
    }
    assert_eq!(
        answer(
            "service/1",
            json!({ "fields": { "value": { "type": "json" } } }),
            json!({}),
            json!({ "truthy": "$fields.value" }),
            json!({})
        ),
        Truth::Unknown
    );
    assert_eq!(
        about(
            "service/1",
            json!({ "truthy": "$fields.value" }),
            json!(true)
        ),
        Truth::True
    );
    assert_eq!(
        about(
            "service/1",
            json!({ "truthy": "$fields.value" }),
            json!([1])
        ),
        Truth::Unknown
    );
}

/// A declared optional array of untyped elements, so the same rule meets an empty collection, a
/// non-empty one and no collection at all.
fn quantified_schema() -> Value {
    json!({ "fields": { "value": { "type": "array", "items": { "type": "json" } } } })
}

#[test]
fn for_all_over_an_empty_collection_is_true_and_for_any_is_false() {
    let quantifier = |operator: &str| json!({ operator: { "in": "$fields.value", "as": "e", "that": { "truthy": "$e" } } });
    let over = |operator: &str, elements: Value| {
        answer(
            "service/1",
            quantified_schema(),
            json!({}),
            quantifier(operator),
            json!({ "value": elements }),
        )
    };
    assert_eq!(over("for_all", json!([])), Truth::True, "vacuously true");
    assert_eq!(over("for_any", json!([])), Truth::False);
    assert_eq!(over("for_all", json!([1, 2])), Truth::True);
    assert_eq!(over("for_any", json!([0, 0])), Truth::False);
    assert_eq!(over("for_any", json!([0, 3])), Truth::True);
}

#[test]
fn for_all_over_an_unobserved_collection_is_unknown_rather_than_vacuously_true() {
    let condition =
        json!({ "for_all": { "in": "$fields.value", "as": "e", "that": { "truthy": "$e" } } });
    assert_eq!(
        answer(
            "service/1",
            quantified_schema(),
            json!({}),
            condition.clone(),
            json!({})
        ),
        Truth::Unknown,
        "nobody looked and there was nothing to look at are different"
    );
    // A value that is not a collection at all is the same answer. An open schema is what lets the
    // rule register at all: a declared array would have refused the value before the rule ran.
    assert_eq!(
        answer(
            "service/1",
            json!({ "fields": {}, "additional_fields": true }),
            json!({}),
            condition,
            json!({ "value": 7 })
        ),
        Truth::Unknown
    );
}

#[test]
fn a_nested_quantifier_body_reaches_the_outer_element_and_an_inner_binder_shadows_it() {
    let schema = json!({ "fields": { "groups": { "type": "array", "required": true, "items": {
        "type": "object",
        "properties": {
            "limit": { "type": "integer", "required": true },
            "items": { "type": "array", "required": true, "items": { "type": "integer" } }
        }
    }}}});
    // The inner body reads the outer element's `limit` and the inner element itself.
    let reaching = json!({ "for_all": { "in": "$fields.groups", "as": "group", "that": {
        "for_all": { "in": "$group.items", "as": "item", "that": {
            "compare": { "left": "$item", "op": "lte", "right": "$group.limit" }
        }}
    }}});
    let fields =
        json!({ "groups": [{ "limit": 5, "items": [1, 5] }, { "limit": 2, "items": [2] }] });
    assert_eq!(
        answer(
            "service/1",
            schema.clone(),
            json!({}),
            reaching.clone(),
            fields
        ),
        Truth::True
    );
    let over = json!({ "groups": [{ "limit": 5, "items": [1, 9] }] });
    assert_eq!(
        answer("service/1", schema.clone(), json!({}), reaching, over),
        Truth::False
    );

    // A nested `as` equal to an enclosing one is admitted and the inner one wins.
    let shadowing = json!({ "for_all": { "in": "$fields.groups", "as": "e", "that": {
        "for_all": { "in": "$e.items", "as": "e", "that": { "truthy": "$e" } }
    }}});
    assert_eq!(
        answer(
            "service/1",
            schema.clone(),
            json!({}),
            shadowing.clone(),
            json!({ "groups": [{ "limit": 1, "items": [1, 2] }] })
        ),
        Truth::True
    );
    assert_eq!(
        answer(
            "service/1",
            schema,
            json!({}),
            shadowing,
            json!({ "groups": [{ "limit": 1, "items": [1, 0] }] })
        ),
        Truth::False,
        "the inner binder is the element, not the outer group"
    );
}

#[test]
fn for_all_over_a_map_walks_its_values_in_canonical_key_order() {
    let schema = json!({ "fields": { "counts": {
        "type": "map", "key": "string", "items": { "type": "integer" }, "required": true
    }}});
    let condition =
        json!({ "for_all": { "in": "$fields.counts", "as": "n", "that": { "truthy": "$n" } } });
    assert_eq!(
        answer(
            "service/1",
            schema.clone(),
            json!({}),
            condition.clone(),
            json!({ "counts": { "b": 1, "a": 2 } })
        ),
        Truth::True
    );
    // The values are what is walked, and the fold settles on the first zero in canonical key order.
    assert_eq!(
        answer(
            "service/1",
            schema,
            json!({}),
            condition,
            json!({ "counts": { "b": 1, "a": 0 } })
        ),
        Truth::False
    );
}

#[test]
fn a_quantifier_body_reading_an_address_outside_its_scope_is_refused_at_registration() {
    let schema = json!({ "fields": { "lines": { "type": "array", "required": true, "items": {
        "type": "object", "properties": { "quantity": { "type": "integer", "required": true } }
    }}}});
    let defects = refused(probe(
        "service/1",
        schema.clone(),
        json!({}),
        json!({ "for_all": { "in": "$fields.lines", "as": "line", "that": {
            "compare": { "left": "$line.nope", "op": "gt", "right": 0 }
        }}}),
    ));
    assert!(
        defects
            .iter()
            .any(|defect| matches!(defect, DefinitionError::QuantifierBodyScope { .. })),
        "{defects}"
    );

    // `as` is one non-empty path segment, and `in` names a collection.
    let defects = refused(probe(
        "service/1",
        schema.clone(),
        json!({}),
        json!({ "for_all": { "in": "$fields.lines", "as": "a.b", "that": true } }),
    ));
    assert!(defects
        .iter()
        .any(|defect| matches!(defect, DefinitionError::QuantifierBindInvalid { .. })));

    let defects = refused(probe(
        "service/1",
        json!({ "fields": { "name": { "type": "string", "required": true } } }),
        json!({}),
        json!({ "for_all": { "in": "$fields.name", "as": "e", "that": true } }),
    ));
    assert!(defects
        .iter()
        .any(|defect| matches!(defect, DefinitionError::QuantifierOverNotCollection { .. })));
    let _ = schema;
}

#[test]
fn a_condition_nested_past_thirty_two_is_refused_with_its_limit() {
    let mut condition = json!(true);
    for _ in 0..32 {
        condition = json!({ "not": condition });
    }
    // 33 levels counting the innermost literal.
    let defects = refused(probe(
        "service/1",
        json!({ "fields": { "value": { "type": "json" } } }),
        json!({}),
        condition,
    ));
    assert!(carries(
        &defects,
        &DefinitionError::ConditionTooDeep {
            path: "invariants[0].assert".to_owned(),
            depth: 33,
            limit: 32,
        }
    ));

    let mut shallow = json!(true);
    for _ in 0..31 {
        shallow = json!({ "not": shallow });
    }
    assert!(ValidatedDefinition::new(definition(probe(
        "service/1",
        json!({ "fields": { "value": { "type": "json" } } }),
        json!({}),
        shallow
    )))
    .is_ok());
}

#[test]
fn a_nominal_invariant_inside_a_list_lowers_to_a_for_all_over_that_path() {
    // `Money.amount >= 0` reaching `LineItem.unit_price` inside a `List<LineItem>`.
    let schema = json!({ "fields": { "lines": { "type": "array", "required": true, "items": {
        "type": "object", "properties": { "unit_price": { "type": "number", "required": true } }
    }}}});
    let condition = json!({ "for_all": { "in": "$fields.lines", "as": "e", "that": {
        "compare": { "left": "$e.unit_price", "op": "gte", "right": 0 }
    }}});
    assert_eq!(
        answer(
            "service/1",
            schema.clone(),
            json!({}),
            condition.clone(),
            json!({ "lines": [{ "unit_price": 0 }, { "unit_price": 5 }] })
        ),
        Truth::True
    );
    assert_eq!(
        answer(
            "service/1",
            schema,
            json!({}),
            condition,
            json!({ "lines": [{ "unit_price": -1 }] })
        ),
        Truth::False
    );
}

#[test]
fn an_empty_any_of_is_unknown_when_unobserved_and_false_when_observed() {
    assert_eq!(
        answer(
            "service/1",
            json!({ "fields": { "value": { "type": "json" } } }),
            json!({}),
            json!({ "in": ["$fields.value", []] }),
            json!({})
        ),
        Truth::Unknown,
        "the needle resolved to nothing"
    );
    assert_eq!(
        about(
            "service/1",
            json!({ "in": ["$fields.value", []] }),
            json!("x")
        ),
        Truth::False,
        "the needle resolved and the list is empty"
    );
}

// --- § 10.6: collection addressing ----------------------------------------------------------------

#[test]
fn a_collection_count_resolves_under_service_1_and_resolves_to_nothing_under_kernel_1() {
    // An open schema, so the same address registers under both document rule sets and only the
    // evaluation differs.
    let schema = json!({ "fields": {}, "additional_fields": true });
    let condition = json!({ "compare": { "left": "$fields.lines.count", "op": "eq", "right": 2 } });
    assert_eq!(
        answer(
            "service/1",
            schema.clone(),
            json!({}),
            condition.clone(),
            json!({ "lines": ["a", "b"] })
        ),
        Truth::True
    );
    assert_eq!(
        answer(
            "kernel/1",
            schema.clone(),
            json!({}),
            json!({ "eq": ["$fields.lines.count", 2] }),
            json!({ "lines": ["a", "b"] })
        ),
        Truth::Unknown,
        "kernel/1 walks objects only, so the address resolves to nothing"
    );
    // A declared map addresses its size and nothing else it holds.
    let declared = json!({ "fields": { "metadata": {
        "type": "map", "key": "string", "items": { "type": "string" }, "required": true
    }}});
    assert_eq!(
        answer(
            "service/1",
            declared.clone(),
            json!({}),
            json!({ "compare": { "left": "$fields.metadata.count", "op": "eq", "right": 1 } }),
            json!({ "metadata": { "count": "7" } })
        ),
        Truth::True,
        "the only count a map has is its size"
    );
    let defects = refused(probe(
        "service/1",
        declared,
        json!({}),
        json!({ "eq": ["$fields.metadata.source", "portal"] }),
    ));
    assert!(
        defects.iter().any(|defect| matches!(
            defect,
            DefinitionError::InvalidRule { message, .. } if message.contains("keys are not addressable")
        )),
        "{defects}"
    );
}

#[test]
fn an_array_ordinal_address_resolves_under_service_1_only() {
    let schema = json!({ "fields": {}, "additional_fields": true });
    assert_eq!(
        answer(
            "service/1",
            schema.clone(),
            json!({}),
            json!({ "compare": { "left": "$fields.lines.0", "op": "eq", "right": "a" } }),
            json!({ "lines": ["a", "b"] })
        ),
        Truth::True
    );
    assert_eq!(
        answer(
            "kernel/1",
            schema.clone(),
            json!({}),
            json!({ "eq": ["$fields.lines.0", "a"] }),
            json!({ "lines": ["a", "b"] })
        ),
        Truth::Unknown
    );
    // A leading zero is not an index, so it addresses nothing.
    assert_eq!(
        answer(
            "service/1",
            schema,
            json!({}),
            json!({ "compare": { "left": "$fields.lines.01", "op": "eq", "right": "b" } }),
            json!({ "lines": ["a", "b"] })
        ),
        Truth::Unknown
    );
    // And the address is checked at registration against a declared array.
    let declared = json!({ "fields": { "lines": {
        "type": "array", "required": true, "items": { "type": "string" }
    }}});
    assert!(ValidatedDefinition::new(definition(probe(
        "service/1",
        declared.clone(),
        json!({}),
        json!({ "compare": { "left": "$fields.lines.1", "op": "eq", "right": "b" } })
    )))
    .is_ok());
    let defects = refused(probe(
        "service/1",
        declared,
        json!({}),
        json!({ "compare": { "left": "$fields.lines.nope", "op": "eq", "right": "b" } }),
    ));
    assert!(
        defects.iter().any(|defect| matches!(
            defect,
            DefinitionError::InvalidRule { message, .. }
                if message.contains("addresses `count` or an element index")
        )),
        "{defects}"
    );
}

// --- helpers ---------------------------------------------------------------------------------------

fn payee_schema(tag: &str) -> Value {
    json!({ "fields": { "payee": {
        "type": "union",
        "tag": tag,
        "required": true,
        "variants": {
            "person": { "type": "string", "required": true },
            "company": { "type": "object", "required": true, "properties": {
                "name": { "type": "string", "required": true }
            }}
        }
    }}})
}

fn payee_probe(tag: &str) -> ValidatedDefinition {
    let mut registry = Registry::new();
    let document = probe("service/1", payee_schema(tag), json!({}), true.into());
    registry
        .register(definition(document))
        .expect("the probe registers");
    registry.get("probe", 1).expect("registered").clone()
}

// --- § 10.2.1: each operand through its own door, and the literals admission refuses --------------

/// `serde_json::from_str` retains the authored token, which is the whole subject of § 10.2.1;
/// `json!` cannot carry an integer past the `i64`/`u64` span at all.
fn document(value: &str) -> Value {
    serde_json::from_str(value).expect("the fixture is a JSON document")
}

/// An integer past the `u64` span: the literal door keeps it at scale zero, and the wire door,
/// having no exact carrier for it, reads the binary64's canonical decimal — which ends in `…000`.
/// Everything below turns on that one disagreement, which is the only thing that makes the two
/// doors observable from outside.
const PAST_U64: &str = "100000000000000000000000001";

fn origin_schema() -> Value {
    let number = json!({ "type": "number", "required": true });
    let pair = json!({
        "type": "object",
        "required": true,
        "properties": { "a": { "type": "number", "required": true } }
    });
    let list = json!({ "type": "array", "required": true, "items": { "type": "number" } });
    json!({ "fields": {
        "big": number.clone(),
        "same": number,
        "list": list.clone(),
        "list_same": list,
        "pair": pair.clone(),
        "pair_same": pair
    }})
}

fn origin_fields() -> Value {
    document(&format!(
        r#"{{"big": {PAST_U64}, "same": {PAST_U64},
            "list": [{PAST_U64}], "list_same": [{PAST_U64}],
            "pair": {{"a": {PAST_U64}}}, "pair_same": {{"a": {PAST_U64}}}}}"#
    ))
}

fn origin_answer(semantics: &str, condition: &str) -> Truth {
    answer(
        semantics,
        origin_schema(),
        json!({}),
        document(condition),
        origin_fields(),
    )
}

#[test]
fn service_equality_and_membership_read_each_operand_through_its_own_door() {
    // The reference point: `compare` reads each operand through its own door, so a reference and an
    // authored literal spelling the same digits are not one value.
    assert_eq!(
        origin_answer(
            "service/1",
            &format!(
                r#"{{"compare": {{"left": "$fields.big", "op": "eq", "right": {PAST_U64}}}}}"#
            )
        ),
        Truth::False
    );

    // Every combination of the two origins, in both operand positions, for all four operators.
    // `false` is *one literal, one reference*; `true` is *both the same door*.
    for (condition, expected) in [
        // eq — a literal against a reference, written either way round.
        (
            format!(r#"{{"eq": ["$fields.big", {PAST_U64}]}}"#),
            Truth::False,
        ),
        (
            format!(r#"{{"eq": [{PAST_U64}, "$fields.big"]}}"#),
            Truth::False,
        ),
        // and the same door on both sides, which is what says the divergence is the door.
        (
            r#"{"eq": ["$fields.big", "$fields.same"]}"#.to_owned(),
            Truth::True,
        ),
        (
            format!(r#"{{"eq": [{PAST_U64}, {PAST_U64}]}}"#),
            Truth::True,
        ),
        // ne is the negation of the same answer, not a second rule.
        (
            format!(r#"{{"ne": ["$fields.big", {PAST_U64}]}}"#),
            Truth::True,
        ),
        (
            r#"{"ne": ["$fields.big", "$fields.same"]}"#.to_owned(),
            Truth::False,
        ),
        // in — the collection's **elements** carry the origin, not the collection.
        (
            format!(r#"{{"in": ["$fields.big", [{PAST_U64}]]}}"#),
            Truth::False,
        ),
        (
            format!(r#"{{"in": [{PAST_U64}, "$fields.list"]}}"#),
            Truth::False,
        ),
        (
            r#"{"in": ["$fields.big", "$fields.list"]}"#.to_owned(),
            Truth::True,
        ),
        (
            format!(r#"{{"in": [{PAST_U64}, [{PAST_U64}]]}}"#),
            Truth::True,
        ),
        // contains — the same four, with the operands the other way round.
        (
            format!(r#"{{"contains": [[{PAST_U64}], "$fields.big"]}}"#),
            Truth::False,
        ),
        (
            format!(r#"{{"contains": ["$fields.list", {PAST_U64}]}}"#),
            Truth::False,
        ),
        (
            r#"{"contains": ["$fields.list", "$fields.big"]}"#.to_owned(),
            Truth::True,
        ),
        (
            format!(r#"{{"contains": [[{PAST_U64}], {PAST_U64}]}}"#),
            Truth::True,
        ),
        // and the origin travels to every depth, not only to a top-level operand — through a
        // mapping's members and through a list's elements alike, which are two separate walks.
        (
            format!(r#"{{"eq": [{{"a": {PAST_U64}}}, "$fields.pair"]}}"#),
            Truth::False,
        ),
        (
            r#"{"eq": ["$fields.pair", "$fields.pair_same"]}"#.to_owned(),
            Truth::True,
        ),
        (
            format!(r#"{{"eq": [[{PAST_U64}], "$fields.list"]}}"#),
            Truth::False,
        ),
        (
            format!(r#"{{"eq": ["$fields.list", [{PAST_U64}]]}}"#),
            Truth::False,
        ),
        (
            r#"{"eq": ["$fields.list", "$fields.list_same"]}"#.to_owned(),
            Truth::True,
        ),
        (
            format!(r#"{{"eq": [[{PAST_U64}], [{PAST_U64}]]}}"#),
            Truth::True,
        ),
    ] {
        assert_eq!(
            origin_answer("service/1", &condition),
            expected,
            "{condition}"
        );
    }

    // `kernel/1` is untouched: its equality compares the tokens exactly, so the same pair that
    // `service/1` calls two values is one value there, and it stays one.
    for (condition, expected) in [
        (
            format!(r#"{{"eq": ["$fields.big", {PAST_U64}]}}"#),
            Truth::True,
        ),
        (
            format!(r#"{{"ne": ["$fields.big", {PAST_U64}]}}"#),
            Truth::False,
        ),
        (
            format!(r#"{{"in": ["$fields.big", [{PAST_U64}]]}}"#),
            Truth::True,
        ),
        (
            format!(r#"{{"contains": [[{PAST_U64}], "$fields.big"]}}"#),
            Truth::True,
        ),
    ] {
        assert_eq!(
            origin_answer("kernel/1", &condition),
            expected,
            "{condition}"
        );
    }
}

#[test]
fn an_unobservable_numeric_literal_is_refused_in_every_service_1_operator_and_at_every_depth() {
    let schema = json!({ "fields": {
        "x": { "type": "number", "required": true },
        "list": { "type": "array", "required": true, "items": { "type": "number" } },
        "pair": { "type": "object", "required": true, "properties": {
            "a": { "type": "number", "required": true }
        }}
    }});
    let names_the_domain = |defects: &DefinitionErrors| {
        defects.iter().any(|defect| {
            matches!(defect, DefinitionError::InvalidRule { message, .. }
                if message.contains("source observation domain"))
        })
    };

    for condition in [
        r#"{"compare": {"left": "$fields.x", "op": "eq", "right": 1e400}}"#,
        r#"{"compare": {"left": 1e400, "op": "eq", "right": "$fields.x"}}"#,
        r#"{"eq": ["$fields.x", 1e400]}"#,
        r#"{"eq": [1e400, "$fields.x"]}"#,
        r#"{"ne": ["$fields.x", 1e400]}"#,
        r#"{"gt": ["$fields.x", 1e400]}"#,
        r#"{"gte": ["$fields.x", 1e400]}"#,
        r#"{"lt": ["$fields.x", 1e400]}"#,
        r#"{"lte": ["$fields.x", 1e400]}"#,
        r#"{"truthy": 1e400}"#,
        // nested one level below the operand, which is where a membership literal lives
        r#"{"in": ["$fields.x", [1, 1e400]]}"#,
        r#"{"in": [1e400, "$fields.list"]}"#,
        r#"{"contains": [[1, 1e400], "$fields.x"]}"#,
        r#"{"contains": ["$fields.list", 1e400]}"#,
        r#"{"eq": [{"a": 1e400}, "$fields.pair"]}"#,
        // and below a connective and inside a fold
        r#"{"not": {"eq": ["$fields.x", 1e400]}}"#,
        r#"{"all": [true, {"any": [{"eq": ["$fields.x", 1e400]}]}]}"#,
        r#"{"for_all": {"in": "$fields.list", "as": "n", "that": {"eq": ["$n", 1e400]}}}"#,
    ] {
        let defects = refused(probe(
            "service/1",
            schema.clone(),
            json!({}),
            document(condition),
        ));
        assert!(names_the_domain(&defects), "{condition}: {defects}");
    }

    // `kernel/1` reads every token this runtime can hold through `number::compare`, so nothing it
    // admits today stops being admitted. The four service-only operators cannot appear here at all.
    let kernel = json!({ "fields": { "x": { "type": "number", "required": true } } });
    for condition in [
        r#"{"eq": ["$fields.x", 1e400]}"#,
        r#"{"ne": ["$fields.x", 1e400]}"#,
        r#"{"gt": ["$fields.x", 1e400]}"#,
        r#"{"in": ["$fields.x", [1e400]]}"#,
        r#"{"contains": [[1e400], "$fields.x"]}"#,
    ] {
        assert!(
            ValidatedDefinition::new(definition(probe(
                "kernel/1",
                kernel.clone(),
                json!({}),
                document(condition)
            )))
            .is_ok(),
            "{condition}"
        );
    }
}

#[test]
fn a_quantifier_body_reads_the_fixed_roots_its_binder_does_not_name() {
    // The other half of § 2.2's rule, and the half the shadowing case in `service_review_one.rs`
    // does not measure: an address the binder does **not** match passes through to the enclosing
    // scope untouched, which is what lets a body mix element facts with free ones.
    let schema = json!({ "fields": {
        "tags": { "type": "array", "required": true, "items": { "type": "string" } }
    }});
    let free = json!({ "for_all": { "in": "$fields.tags", "as": "line", "that": { "all": [
        { "eq": ["$id", "p-1"] },
        { "eq": ["$entity", "probe"] },
        { "eq": ["$line", "x"] }
    ]}}});
    assert_eq!(
        answer(
            "service/1",
            schema.clone(),
            json!({}),
            free,
            json!({ "tags": ["x"] })
        ),
        Truth::True
    );

    // And a binder shadows only the name it declares: with `as: id` outside and `as: line` inside,
    // `$id` is the outer element, `$line` is the inner one, and `$entity` is still the definition's.
    let nested = json!({ "for_all": { "in": "$fields.tags", "as": "id", "that": {
        "for_all": { "in": "$fields.tags", "as": "line", "that": { "all": [
            { "eq": ["$id", "x"] },
            { "eq": ["$line", "x"] },
            { "eq": ["$entity", "probe"] }
        ]}}
    }}});
    assert_eq!(
        answer(
            "service/1",
            schema,
            json!({}),
            nested,
            json!({ "tags": ["x"] })
        ),
        Truth::True
    );
}

// --- § 10.2.1: a schema bound is an authored literal, read at admission through the literal door --

/// Whether any defect names the source observation domain, in either variant a definition-admission
/// refusal of an unreadable number can carry.
fn admission_names_the_domain(defects: &DefinitionErrors) -> bool {
    defects.iter().any(|defect| {
        matches!(
            defect,
            DefinitionError::InvalidField { message, .. }
                | DefinitionError::InvalidRule { message, .. }
                if message.contains("source observation domain")
        )
    })
}

/// A `service/1` field of the given shape, named `x`, in the entity schema.
fn bounded_schema(field: Value) -> Value {
    json!({ "fields": { "x": field } })
}

/// § 10.2.1: *"Under `service/1`, numeric schema admission refuses a value outside this
/// source-observation domain with a path-bearing `ValidationError`; **definition admission rejects
/// unobservable numeric literals and bounds**"*, and *"schema bounds use the literal door"*.
///
/// A `default` was already read where it is written. A `min`/`max` was read only when a value
/// arrived, so the definition registered: on a **required** field that adds a second, redundant
/// error to every value, and on an **optional** field with no value ever supplied the bound answers
/// nothing at any evaluation and is never reported at all — which is the case the sentence exists
/// to refuse.
#[test]
fn an_unobservable_schema_bound_is_refused_at_definition_admission_on_both_limits() {
    // Both limits, both signs. The one the reviewer's case measured is the first row.
    for bound in [
        r#"{"type": "number", "required": true, "min": 1e400}"#,
        r#"{"type": "number", "required": true, "max": 1e400}"#,
        r#"{"type": "number", "required": true, "min": -1e400}"#,
        r#"{"type": "number", "required": true, "max": -1e400}"#,
        r#"{"type": "number", "required": true, "min": -1e400, "max": 1e400}"#,
        // and on an optional field, which is the row that answered nothing at every evaluation.
        r#"{"type": "number", "min": 1e400}"#,
        // the other two numeric kinds carry the same door.
        r#"{"type": "integer", "required": true, "min": 1e400}"#,
        r#"{"type": "binary64", "required": true, "max": -1e400}"#,
    ] {
        let defects = refused(probe(
            "service/1",
            bounded_schema(document(bound)),
            json!({}),
            true.into(),
        ));
        assert!(admission_names_the_domain(&defects), "{bound}: {defects}");
    }

    // The control the reviewer's case paired it with: the same unreadable number written as a
    // `default` is refused where it is written, and has been.
    let defects = refused(probe(
        "service/1",
        bounded_schema(document(
            r#"{"type": "number", "required": true, "default": 1e400}"#,
        )),
        json!({}),
        true.into(),
    ));
    assert!(admission_names_the_domain(&defects), "{defects}");
}

/// The same refusal at every depth a schema reaches, and on every surface a schema occurs on:
/// the entity schema, the creation's arguments and response, and an operation's.
#[test]
fn an_unobservable_bound_is_refused_at_every_depth_and_on_every_admitted_schema_surface() {
    let bound = || document(r#"{"type": "number", "required": true, "min": 1e400}"#);
    for nested in [
        json!({ "type": "object", "required": true, "properties": { "a": bound() } }),
        json!({ "type": "array", "required": true, "items": bound() }),
        json!({ "type": "map", "required": true, "key": "string", "items": bound() }),
        json!({ "type": "union", "required": true, "tag": "kind", "variants": { "n": bound() } }),
        json!({ "type": "object", "required": true, "properties": {
            "rows": { "type": "array", "required": true, "items": {
                "type": "object", "required": true, "properties": { "a": bound() }
            }}
        }}),
    ] {
        let defects = refused(probe(
            "service/1",
            bounded_schema(nested.clone()),
            json!({}),
            true.into(),
        ));
        assert!(admission_names_the_domain(&defects), "{nested}: {defects}");
    }

    // The argument, response and operation surfaces, each of which `validate_schema_definition`
    // already walks. Other defects may accompany the bound — an undetermined response field is one
    // — and the assertion is that the bound is among them, not that it is alone.
    let surfaces = [
        json!({ "create": { "arguments": { "fields": { "x": bound() } } } }),
        json!({ "create": { "response": { "fields": { "x": bound() } } } }),
        json!({ "operations": { "touch": {
            "arguments": { "fields": { "x": bound() } },
            "transitions": [{ "from": "held", "to": "held" }]
        }}}),
        json!({ "operations": { "touch": {
            "response": { "fields": { "x": bound() } },
            "transitions": [{ "from": "held", "to": "held" }]
        }}}),
    ];
    for surface in surfaces {
        let mut fixture = probe("service/1", json!({ "fields": {} }), json!({}), true.into());
        for (key, value) in surface.as_object().expect("an object") {
            fixture[key] = value.clone();
        }
        let defects = refused(fixture);
        assert!(admission_names_the_domain(&defects), "{surface}: {defects}");
    }
}

/// What the new refusal must **not** take with it: a readable bound, the underflow class the
/// reading rule deliberately admits, the bound-order check, the run-time bound itself, and every
/// `kernel/1` answer.
#[test]
fn a_readable_bound_the_order_check_and_every_kernel_1_bound_are_untouched() {
    for bound in [
        r#"{"type": "number", "required": true, "min": 0, "max": 100}"#,
        r#"{"type": "integer", "required": true, "min": -9223372036854775808}"#,
        // `1e-400` underflows to a finite binary64, so the source observes it and the bound is
        // readable. § 10.2.1's underflow row is about the value it observes, not a refusal.
        r#"{"type": "number", "required": true, "min": 1e-400}"#,
        r#"{"type": "binary64", "required": true, "min": -1.5, "max": 1.5}"#,
    ] {
        assert!(
            ValidatedDefinition::new(definition(probe(
                "service/1",
                bounded_schema(document(bound)),
                json!({}),
                true.into()
            )))
            .is_ok(),
            "{bound}"
        );
    }

    // The order check still answers, and it answers on its own terms rather than through the door.
    let defects = refused(probe(
        "service/1",
        bounded_schema(json!({ "type": "number", "required": true, "min": 2, "max": 1 })),
        json!({}),
        true.into(),
    ));
    assert!(
        defects.iter().any(|defect| matches!(
            defect,
            DefinitionError::InvalidField { message, .. } if message == "min cannot exceed max"
        )),
        "{defects}"
    );

    // And the bound is still enforced where a value arrives, through the literal door.
    let validated = ValidatedDefinition::new(definition(probe(
        "service/1",
        bounded_schema(json!({ "type": "number", "required": true, "min": 10 })),
        json!({}),
        true.into(),
    )))
    .expect("registers");
    let Err(CoreError::Validation(errors)) =
        create(&validated, "p-1".to_owned(), json!({ "x": 9 }))
    else {
        panic!("a value below the minimum is refused where it arrives");
    };
    assert_eq!(errors[0].path, "fields.x");
    assert!(errors[0].message.contains("is below minimum"));
    assert!(create(&validated, "p-1".to_owned(), json!({ "x": 10 })).is_ok());

    // `kernel/1` reads a bound through `number::compare`, which holds any token this runtime holds,
    // so no `kernel/1` definition stops registering.
    for bound in [
        r#"{"type": "number", "required": true, "min": 1e400}"#,
        r#"{"type": "number", "max": -1e400}"#,
        r#"{"type": "integer", "required": true, "min": 1e400}"#,
    ] {
        assert!(
            ValidatedDefinition::new(definition(probe(
                "kernel/1",
                bounded_schema(document(bound)),
                json!({}),
                true.into()
            )))
            .is_ok(),
            "{bound}"
        );
    }
}

// --- § 10.5 and § 10.6: a union's selected variant keeps its declared kind --------------------------

/// A `service/1` probe over one union field named `payee`, with the given tag and variants.
fn variant_schema(tag: &str, variants: Value) -> Value {
    json!({ "fields": { "payee": {
        "type": "union",
        "required": true,
        "tag": tag,
        "variants": variants
    }}})
}

fn variant_answer(tag: &str, variants: Value, condition: Value, value: Value) -> Truth {
    answer(
        "service/1",
        variant_schema(tag, variants),
        json!({}),
        condition,
        json!({ "payee": value }),
    )
}

/// § 10.6: *"a `map`'s keys stay unaddressable (§ 10.4), so `{metadata: {count: "7"}}` cannot be
/// read as its own `count` key: the only `count` a `map` has is its size"* — **whichever field
/// declares the map**, § 10.1 typing a union's `variants` with a full `FieldDefinition` and § 10.5
/// reaching a variant at `$fields.payee.value` under the tag test.
///
/// The walk continued under the union's content key with `variants.get(<wire key>)`, which looks a
/// variant **label** up by the **wire key**. No variant is called `value`, so a declared `map` was
/// lost and the payload was walked as an untyped object: `count` read the map's own `count` member
/// and answered a wrong value rather than a refusal.
#[test]
fn a_declared_map_is_addressed_by_its_size_whichever_field_declares_it() {
    let variants = json!({ "meta": {
        "type": "map", "required": true, "key": "string", "items": { "type": "string" }
    }});
    let count =
        json!({ "compare": { "left": "$fields.payee.value.count", "op": "eq", "right": 1 } });
    assert_eq!(
        variant_answer(
            "kind",
            variants.clone(),
            count.clone(),
            json!({ "kind": "meta", "value": { "count": "7" } })
        ),
        Truth::True,
        "a declared map's only `count` is its size"
    );
    // Two members, so the size and the shadowing key cannot be confused by coincidence.
    assert_eq!(
        variant_answer(
            "kind",
            variants.clone(),
            json!({ "compare": { "left": "$fields.payee.value.count", "op": "eq", "right": 2 } }),
            json!({ "kind": "meta", "value": { "count": "7", "source": "portal" } })
        ),
        Truth::True
    );
    // And the map's keys stay unaddressable inside a variant exactly as they are outside one.
    assert_eq!(
        variant_answer(
            "kind",
            variants,
            json!({ "eq": ["$fields.payee.value.source", "portal"] }),
            json!({ "kind": "meta", "value": { "source": "portal" } })
        ),
        Truth::Unknown
    );

    // The derived content key moves with the tag, and the size is read under it either way.
    assert_eq!(
        variant_answer(
            "value",
            json!({ "meta": {
                "type": "map", "required": true, "key": "string", "items": { "type": "string" }
            }}),
            json!({ "compare": { "left": "$fields.payee.content.count", "op": "eq", "right": 1 } }),
            json!({ "value": "meta", "content": { "count": "7" } })
        ),
        Truth::True
    );
}

/// The general form behind the map case: the walk continues under the kind the **tag** names, so
/// one declared path answers by the variant the value selected — never by the spelling of the
/// content key.
#[test]
fn the_variant_the_tag_names_is_the_kind_the_walk_continues_under() {
    let variants = json!({
        "meta": { "type": "map", "required": true, "key": "string", "items": { "type": "string" } },
        "lines": { "type": "array", "required": true, "items": { "type": "string" } },
        "company": { "type": "object", "required": true, "properties": {
            "name": { "type": "string", "required": true }
        }}
    });
    let count =
        json!({ "compare": { "left": "$fields.payee.value.count", "op": "eq", "right": 2 } });

    // One address, three variants, three declared kinds: a map's size, an array's length, and an
    // object that has no `count` at all.
    assert_eq!(
        variant_answer(
            "kind",
            variants.clone(),
            count.clone(),
            json!({ "kind": "meta", "value": { "a": "1", "b": "2" } })
        ),
        Truth::True
    );
    assert_eq!(
        variant_answer(
            "kind",
            variants.clone(),
            count.clone(),
            json!({ "kind": "lines", "value": ["a", "b"] })
        ),
        Truth::True
    );
    assert_eq!(
        variant_answer(
            "kind",
            variants.clone(),
            count,
            json!({ "kind": "company", "value": { "name": "Acme" } })
        ),
        Truth::Unknown,
        "an object declares no `count`, so the address resolves to nothing"
    );

    // An array variant's ordinal address, and an object variant's property, both still resolve.
    assert_eq!(
        variant_answer(
            "kind",
            variants.clone(),
            json!({ "eq": ["$fields.payee.value.0", "a"] }),
            json!({ "kind": "lines", "value": ["a", "b"] })
        ),
        Truth::True
    );
    assert_eq!(
        variant_answer(
            "kind",
            variants.clone(),
            json!({ "eq": ["$fields.payee.value.name", "Acme"] }),
            json!({ "kind": "company", "value": { "name": "Acme" } })
        ),
        Truth::True
    );

    // The tag segment is a label, not a variant: it resolves to its own text and continues under no
    // declared field.
    assert_eq!(
        variant_answer(
            "kind",
            variants.clone(),
            json!({ "eq": ["$fields.payee.kind", "company"] }),
            json!({ "kind": "company", "value": { "name": "Acme" } })
        ),
        Truth::True
    );

    // A variant named after the content key is selected by the tag like any other, rather than by
    // sharing that key's spelling.
    assert_eq!(
        variant_answer(
            "kind",
            json!({
                "value": { "type": "array", "required": true, "items": { "type": "string" } },
                "other": { "type": "object", "required": true, "properties": {
                    "count": { "type": "string", "required": true }
                }}
            }),
            json!({ "compare": { "left": "$fields.payee.value.count", "op": "eq", "right": 2 } }),
            json!({ "kind": "value", "value": ["a", "b"] })
        ),
        Truth::True
    );
}

/// The same resolution inside the two contexts a lowered invariant actually puts it in: a nested
/// union, and a quantifier body.
#[test]
fn a_variants_declared_kind_survives_nesting_and_a_quantifier_body() {
    // A union whose variant is a union: the inner tag selects the inner variant's kind.
    let nested = json!({ "inner": {
        "type": "union", "required": true, "tag": "shape", "variants": {
            "meta": {
                "type": "map", "required": true, "key": "string", "items": { "type": "string" }
            }
        }
    }});
    assert_eq!(
        variant_answer(
            "kind",
            nested,
            json!({ "compare": {
                "left": "$fields.payee.value.value.count", "op": "eq", "right": 1
            }}),
            json!({ "kind": "inner", "value": { "shape": "meta", "value": { "count": "7" } } })
        ),
        Truth::True
    );

    // A quantifier over a variant walks a JSON object's values whether or not the walk typed the
    // variant, so this row measures preservation rather than the defect: the quantifier context
    // keeps the answer it had.
    assert_eq!(
        variant_answer(
            "kind",
            json!({ "meta": {
                "type": "map", "required": true, "key": "string", "items": { "type": "string" }
            }}),
            json!({ "for_all": {
                "in": "$fields.payee.value", "as": "v", "that": { "eq": ["$v", "x"] }
            }}),
            json!({ "kind": "meta", "value": { "a": "x", "b": "x" } })
        ),
        Truth::True
    );

    // And registration still admits the tag-guarded path it admitted before, rather than refusing
    // a variant address it cannot type until run time.
    assert!(ValidatedDefinition::new(definition(probe(
        "service/1",
        variant_schema(
            "kind",
            json!({ "meta": {
                "type": "map", "required": true, "key": "string", "items": { "type": "string" }
            }})
        ),
        json!({}),
        json!({ "compare": { "left": "$fields.payee.value.count", "op": "eq", "right": 1 } })
    )))
    .is_ok());
}
