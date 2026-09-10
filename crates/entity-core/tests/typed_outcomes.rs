//! Version-2 values are exercised through public command execution and complete replay.

use entity_core::outcome::{self, Definition, Failure, Invocation, Record, Validated};
use entity_core::{CoreError, EntityDefinition, ValidatedDefinition};
use serde_json::{json, Value};

fn document(predicate: Value) -> Value {
    let shape = json!({"fields": {
        "payload": {"type":"nullable", "default":null, "items":{
            "type":"object", "properties":{"count":{"type":"integer","required":true,"default":7,"min":0}}
        }},
        "labels": {"type":"map", "required":true, "items":{"type":"nullable","items":{"type":"integer","min":0}}}
    }});
    let event = json!({"template":{"type":"Stored","payload":"$fields"},"schema":shape});
    json!({
        "format":"entity-outcome-definition/2",
        "entity":{"entity":"sample.typed", "schema":shape, "lifecycle":{"initial":"open","states":["open"]}},
        "commands":{
            "create":{"arguments":shape,"observations":{},"outcomes":{
                "accepted":{"condition":{"condition":"when","predicate":predicate},
                    "effect":{"effect":"create","set":{"payload":"$args.payload","labels":"$args.labels"},"emits":[event]}},
                "rejected":{"condition":{"condition":"otherwise"},"effect":{"effect":"refuse","error":"Rejected","payload":{},"schema":{}}}
            }},
            "replace":{"arguments":shape,"observations":{},"outcomes":{
                "changed":{"condition":{"condition":"otherwise"},"effect":{"effect":"change","transitions":[{"from":"open","to":"open"}],
                    "set":{"payload":"$args.payload","labels":"$args.labels"},"emits":[event]}}
            }}
        }
    })
}

fn model(document: Value) -> Validated {
    Validated::new(serde_json::from_value(document).unwrap()).unwrap()
}

fn call(command: &str, args: Value) -> Invocation {
    Invocation {
        command: command.into(),
        id: "typed-a".into(),
        arguments: args.as_object().unwrap().clone(),
        observations: Default::default(),
    }
}

fn quantifier(operator: &str, over: &str, bind: &str, body: Value) -> Value {
    json!({operator:{"over":over,"bind":bind,"body":body}})
}

#[test]
fn nullable_values_keep_defaults_types_events_and_replay_at_every_depth() {
    let model = model(document(json!(true)));
    let first = model
        .decide(
            None,
            call(
                "create",
                json!({"labels":{"empty":null,"exact":9223372036854775807_i64}}),
            ),
        )
        .unwrap();
    assert_eq!(first.invocation.arguments["payload"], Value::Null);
    assert_eq!(
        first.result.as_ref().unwrap().fields["payload"],
        Value::Null
    );
    assert_eq!(first.events[0].payload["labels"]["empty"], Value::Null);
    assert_eq!(
        first.events[0].payload["labels"]["exact"],
        json!(9223372036854775807_i64)
    );
    let second = model
        .decide(
            first.result.as_ref(),
            call("replace", json!({"payload":{},"labels":{"count":3}})),
        )
        .unwrap();
    assert_eq!(second.invocation.arguments["payload"], json!({"count":7}));
    assert_eq!(second.result.as_ref().unwrap().revision, 2);
    assert_eq!(second.events[0].payload["payload"], json!({"count":7}));
    let third = model
        .decide(
            second.result.as_ref(),
            call("replace", json!({"payload":null,"labels":{}})),
        )
        .unwrap();
    assert_eq!(
        third.result.as_ref().unwrap().fields["payload"],
        Value::Null
    );
    let history: Vec<Record> =
        serde_json::from_slice(&serde_json::to_vec(&vec![first, second, third.clone()]).unwrap())
            .unwrap();
    assert_eq!(outcome::replay(&history).unwrap(), third.result);
    assert_eq!(
        serde_json::to_value(&history[0]).unwrap()["format"],
        "entity-outcome-record/2"
    );
}

#[test]
fn required_nullable_omission_is_distinct_from_null_and_inner_defaults() {
    let mut doc = document(json!(true));
    let field = &mut doc["commands"]["create"]["arguments"]["fields"]["payload"];
    field["required"] = json!(true);
    field.as_object_mut().unwrap().remove("default");
    let model = model(doc);
    let Failure::Core(CoreError::Validation(errors)) = model
        .decide(None, call("create", json!({"labels":{}})))
        .unwrap_err()
    else {
        panic!("required omission must fail");
    };
    assert_eq!(errors[0].path, "arguments.payload");
    let record = model
        .decide(None, call("create", json!({"payload":null,"labels":{}})))
        .unwrap();
    assert_eq!(record.invocation.arguments["payload"], Value::Null);
    let Failure::Core(CoreError::Validation(errors)) = model
        .decide(
            None,
            call("create", json!({"payload":{"count":null},"labels":{}})),
        )
        .unwrap_err()
    else {
        panic!("nonnullable inner property must fail");
    };
    assert_eq!(errors[0].path, "arguments.payload.count");
}

#[test]
fn typed_maps_accumulate_value_errors_without_interpreting_keys_as_paths() {
    let model = model(document(json!(true)));
    let Failure::Core(CoreError::Validation(errors)) = model
        .decide(
            None,
            call(
                "create",
                json!({"payload":{"count":-2},"labels":{"a.b":false,"count":-1,"0":"bad"}}),
            ),
        )
        .unwrap_err()
    else {
        panic!("typed values must fail");
    };
    let paths: Vec<_> = errors.iter().map(|e| e.path.as_str()).collect();
    assert_eq!(
        paths,
        [
            "arguments.labels[\"0\"]",
            "arguments.labels[\"a.b\"]",
            "arguments.labels[\"count\"]",
            "arguments.payload.count"
        ]
    );
    let record = model
        .decide(
            None,
            call("create", json!({"labels":{"a.b":1,"count":null,"0":0}})),
        )
        .unwrap();
    assert_eq!(
        record.result.unwrap().fields["labels"],
        json!({"a.b":1,"count":null,"0":0})
    );
}

#[test]
fn quantified_maps_keep_empty_absent_null_and_kleene_results_distinct() {
    for (operator, values, expected) in [
        ("forall", json!({}), Some("accepted")),
        ("any_element", json!({}), Some("rejected")),
        ("forall", json!({"a":null,"b":3}), None),
        ("forall", json!({"a":null,"b":0}), Some("rejected")),
        ("any_element", json!({"a":null,"b":3}), Some("accepted")),
        ("any_element", json!({"a":null,"b":0}), None),
    ] {
        let model = model(document(quantifier(
            operator,
            "$args.labels",
            "value",
            json!({"gt":["$bound.value",1]}),
        )));
        let decision = model.decide(None, call("create", json!({"labels":values})));
        if let Some(expected) = expected {
            assert_eq!(decision.unwrap().outcome, expected, "{operator} {values}");
        } else {
            assert!(
                matches!(decision, Err(Failure::Unobservable { .. })),
                "{decision:?}"
            );
        }
    }
    for value in [None, Some(Value::Null)] {
        let mut doc = document(quantifier("forall", "$args.optional", "item", json!(true)));
        doc["commands"]["create"]["arguments"]["fields"]["optional"] =
            json!({"type":"nullable","items":{"type":"array","items":{"type":"integer"}}});
        let model = model(doc);
        let mut args = json!({"labels":{}});
        if let Some(value) = value {
            args["optional"] = value;
        }
        let error = model.decide(None, call("create", args)).unwrap_err();
        assert!(
            matches!(error, Failure::Unobservable { ref paths, .. } if paths == &["$args.optional".to_owned()]),
            "{error:?}"
        );
    }
}

fn nested_document(predicate: Value) -> Value {
    let mut doc = document(predicate);
    doc["commands"]["create"]["arguments"]["fields"]["groups"] = json!({"type":"array","required":true,"items":{"type":"nullable","items":{"type":"object","properties":{
        "minimum":{"type":"integer","required":true},
        "members":{"type":"map","required":true,"items":{"type":"nullable","items":{"type":"integer"}}}
    }}}});
    doc["commands"]["create"]["arguments"]["fields"]["limit"] =
        json!({"type":"integer","required":true});
    doc
}

#[test]
fn nested_quantifiers_resolve_outer_free_and_shadowed_bindings_lexically() {
    let outer = quantifier(
        "forall",
        "$args.groups",
        "group",
        quantifier(
            "any_element",
            "$bound.group.members",
            "member",
            json!({"all":[
                {"gt":["$bound.member","$bound.group.minimum"]}, {"gt":["$bound.member","$args.limit"]}
            ]}),
        ),
    );
    let shadowed = quantifier(
        "forall",
        "$args.groups",
        "group",
        quantifier(
            "any_element",
            "$bound.group.members",
            "group",
            json!({"gt":["$bound.group","$args.limit"]}),
        ),
    );
    for predicate in [outer, shadowed] {
        let model = model(nested_document(predicate));
        let record = model.decide(None, call("create", json!({"labels":{},"limit":2,"groups":[{"minimum":3,"members":{"none":null,"yes":4}},{"minimum":1,"members":{"yes":3}}]}))).unwrap();
        assert_eq!(record.outcome, "accepted");
        assert_eq!(
            outcome::replay(std::slice::from_ref(&record)).unwrap(),
            record.result
        );
        let empty = model
            .decide(
                None,
                call("create", json!({"labels":{},"limit":2,"groups":[]})),
            )
            .unwrap();
        assert_eq!(empty.outcome, "accepted");
        let missing = model
            .decide(
                None,
                call("create", json!({"labels":{},"limit":2,"groups":[null]})),
            )
            .unwrap_err();
        assert!(
            matches!(missing, Failure::Unobservable { .. }),
            "{missing:?}"
        );
    }
}

#[test]
fn legacy_profiles_reject_quantifiers_even_without_new_field_kinds() {
    let predicate = quantifier("forall", "$args.values", "value", json!(true));
    let mut doc = document(predicate);
    doc["format"] = json!("entity-outcome-definition/1");
    doc["entity"]["schema"] = json!({});
    doc["commands"].as_object_mut().unwrap().remove("replace");
    doc["commands"]["create"]["arguments"] =
        json!({"fields":{"values":{"type":"array","items":{"type":"integer"}}}});
    doc["commands"]["create"]["outcomes"]["accepted"]["effect"] = json!({"effect":"observe"});
    let error = Validated::new(serde_json::from_value(doc.clone()).unwrap()).unwrap_err();
    assert!(
        matches!(error, Failure::Definition { ref detail, .. } if detail.contains("element quantifiers require outcome profile 2")),
        "{error:?}"
    );
    let mut entity = doc["entity"].clone();
    entity["schema"] = doc["commands"]["create"]["arguments"].clone();
    entity["invariants"] = json!([{"name":"all","assert":quantifier("forall", "$fields.values", "value", json!(true))}]);
    let error = ValidatedDefinition::new(serde_json::from_value(entity).unwrap()).unwrap_err();
    assert!(error
        .to_string()
        .contains("element quantifiers require outcome profile 2"));
}

#[test]
fn nullable_error_payloads_stay_typed_non_mutating_records() {
    let mut doc = document(json!(false));
    let effect = &mut doc["commands"]["create"]["outcomes"]["rejected"]["effect"];
    effect["payload"] = json!({"reason":null});
    effect["schema"] = json!({"fields":{"reason":{"type":"nullable","required":true,"items":{"type":"string","min_length":1}}}});
    let record = model(doc.clone())
        .decide(None, call("create", json!({"labels":{}})))
        .unwrap();
    assert_eq!(
        record.error.as_ref().unwrap().payload,
        json!({"reason":null})
    );
    assert_eq!(record.result, None);
    assert!(record.events.is_empty());
    assert_eq!(outcome::replay(&[record]).unwrap(), None);
    doc["commands"]["create"]["outcomes"]["rejected"]["effect"]["payload"]["reason"] = json!(false);
    let error = model(doc)
        .decide(None, call("create", json!({"labels":{}})))
        .unwrap_err();
    assert!(
        matches!(error, Failure::Core(CoreError::Validation(ref errors)) if errors[0].path.ends_with("reason")),
        "{error:?}"
    );
}

#[test]
fn registration_checks_dormant_quantifiers_and_keeps_binding_scope_closed() {
    for (predicate, reason) in [
        (
            quantifier("forall", "$args.payload", "value", json!(true)),
            "array or map",
        ),
        (
            quantifier("forall", "$args.groups", "bad.name", json!(true)),
            "identifier",
        ),
        (
            quantifier(
                "forall",
                "$args.groups",
                "group",
                json!({"gt":["$bound.group.missing",0]}),
            ),
            "declares no property",
        ),
        (
            quantifier(
                "forall",
                "$args.groups",
                "group",
                json!({"eq":["$fields.labels",{}]}),
            ),
            "not available here",
        ),
        (json!({"eq":["$bound.group",1]}), "unknown lexical binder"),
    ] {
        let error = Validated::new(serde_json::from_value(nested_document(predicate)).unwrap())
            .unwrap_err();
        assert!(
            matches!(error, Failure::Definition { ref detail, .. } if detail.contains(reason)),
            "{error:?}"
        );
    }
    let mut doc = nested_document(quantifier("forall", "$args.groups", "group", json!(true)));
    doc["commands"]["create"]["outcomes"]["accepted"]["effect"]["set"]["payload"] =
        json!("$bound.group");
    let error = Validated::new(serde_json::from_value(doc).unwrap()).unwrap_err();
    assert!(
        matches!(error, Failure::Definition { ref detail, .. } if detail.contains("unknown lexical binder")),
        "{error:?}"
    );
}

#[test]
fn nullable_reference_validation_and_quantified_invariants_run_before_effects_escape() {
    let mut doc = document(json!({"gte":["$args.payload.count",0]}));
    doc["entity"]["invariants"] = json!([{"name":"positive-labels","assert":quantifier("forall", "$fields.labels", "item", json!({"gt":["$bound.item",0]}))}]);
    let model = model(doc);
    let record = model
        .decide(None, call("create", json!({"payload":{},"labels":{"a":1}})))
        .unwrap();
    assert_eq!(record.outcome, "accepted");
    let error = model
        .decide(
            record.result.as_ref(),
            call("replace", json!({"labels":{"a":0}})),
        )
        .unwrap_err();
    assert!(
        matches!(error, Failure::Core(CoreError::InvariantViolation { .. })),
        "{error:?}"
    );
    assert_eq!(record.result.unwrap().revision, 1);
}

#[test]
fn profile_two_payload_validation_and_format_downgrades_cannot_be_bypassed() {
    let mut doc = document(json!(true));
    doc["commands"]["create"]["outcomes"]["accepted"]["effect"]["emits"][0]["template"]
        ["payload"] = json!({"payload":{"count":-3},"labels":{}});
    let error = model(doc)
        .decide(None, call("create", json!({"labels":{}})))
        .unwrap_err();
    assert!(
        matches!(error, Failure::Core(CoreError::Validation(ref errors)) if errors[0].path.ends_with("payload.count")),
        "{error:?}"
    );
    let mut old = document(json!(true));
    old["format"] = json!("entity-outcome-definition/1");
    let error = Validated::new(serde_json::from_value(old).unwrap()).unwrap_err();
    assert!(
        matches!(error, Failure::Definition { ref detail, .. } if detail.contains("require outcome profile 2")),
        "{error:?}"
    );
    let legacy: EntityDefinition =
        serde_json::from_value(document(json!(true))["entity"].clone()).unwrap();
    let error = ValidatedDefinition::new(legacy).unwrap_err();
    assert!(error.to_string().contains("require outcome profile 2"));
    let model = model(document(json!(true)));
    let record = model
        .decide(None, call("create", json!({"labels":{}})))
        .unwrap();
    let mut downgraded = serde_json::to_value(record).unwrap();
    downgraded["format"] = json!("entity-outcome-record/1");
    let error = outcome::replay(&[serde_json::from_value(downgraded).unwrap()]).unwrap_err();
    assert!(matches!(error, Failure::Replay { .. }), "{error:?}");
}

#[test]
fn malformed_nested_contracts_and_quantifier_wire_keys_are_refused() {
    for shape in [
        json!({"type":"nullable"}),
        json!({"type":"map"}),
        json!({"type":"nullable","min":0,"items":{"type":"integer"}}),
        json!({"type":"map","items":{"type":"integer","default":"wrong"}}),
    ] {
        let mut doc = document(json!(true));
        doc["commands"]["replace"]["arguments"]["fields"]["bad"] = shape;
        let error = Validated::new(serde_json::from_value(doc).unwrap()).unwrap_err();
        assert!(matches!(error, Failure::Definition { .. }), "{error:?}");
    }
    let mut predicate = quantifier("forall", "$args.labels", "item", json!(true));
    predicate["forall"]["boddy"] = json!(false);
    let error = serde_json::from_value::<Definition>(document(predicate)).unwrap_err();
    assert!(error.to_string().contains("unknown field"), "{error}");
}
