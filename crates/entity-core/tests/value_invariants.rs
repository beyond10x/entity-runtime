//! Value-local rules remain enforced in every recursive decision boundary.

use entity_core::outcome::{self, Definition, Failure, Invocation, Validated};
use entity_core::{CoreError, EntityDefinition, ValidatedDefinition, ValidationError};
use serde_json::{json, Value};

fn document(field: Value) -> Value {
    let shape = json!({"fields":{"value":field}});
    let event = json!({"template":{"type":"Stored","payload":"$fields"},"schema":shape});
    json!({
        "format":"entity-outcome-definition/5",
        "entity":{"entity":"sample.value","schema":shape,"lifecycle":{"initial":"open","states":["open"]}},
        "commands":{
            "create":{"arguments":shape,"observations":{},"outcomes":{"created":{
                "condition":{"condition":"otherwise"},"effect":{"effect":"create","set":{"value":"$args.value"},"emits":[event]}}}},
            "replace":{"arguments":shape,"observations":{},"outcomes":{"replaced":{
                "condition":{"condition":"otherwise"},"effect":{"effect":"change",
                "transitions":[{"from":"open","to":"open"}],"set":{"value":"$args.value"},"emits":[event]}}}},
            "reject":{"arguments":shape,"observations":{},"outcomes":{"refused":{
                "condition":{"condition":"otherwise"},"effect":{"effect":"refuse","error":"Rejected",
                "payload":{"value":"$args.value"},"schema":shape}}}}
        }
    })
}

fn model(source: Value) -> Validated {
    Validated::new(serde_json::from_value(source).unwrap()).unwrap()
}

fn call(command: &str, value: Value) -> Invocation {
    Invocation {
        command: command.into(),
        id: "value-a".into(),
        arguments: json!({"value":value}).as_object().unwrap().clone(),
        observations: Default::default(),
    }
}

fn positive() -> Value {
    json!({"type":"integer","invariants":[{"name":"positive","assert":{"gt":["$bound.value",0]},"message":"must be positive"}]})
}

fn errors(failure: Failure) -> Vec<ValidationError> {
    let Failure::Core(CoreError::Validation(errors)) = failure else {
        panic!("expected typed value validation errors, got {failure:?}");
    };
    errors
}

#[test]
fn nested_value_rules_accumulate_siblings_and_distinguish_false_from_unknown() {
    let period = json!({"type":"object","properties":{
        "start":{"type":"integer","required":true},"end":{"type":"integer"}
    },"invariants":[
        {"name":"ordered","assert":{"lte":["$bound.value.start","$bound.value.end"]}},
        {"name":"positive-start","assert":{"gt":["$bound.value.start",0]}}
    ]});
    let model = model(document(
        json!({"type":"map","items":{"type":"array","items":period}}),
    ));
    let errors = errors(
        model
            .decide(
                None,
                call("create", json!({"a.b":[{"start":3,"end":2},{"start":0}]})),
            )
            .unwrap_err(),
    );
    assert_eq!(errors.len(), 3, "{errors:?}");
    assert_eq!(errors[0].path, "arguments.value[\"a.b\"][0]");
    assert!(
        errors[0].message.contains("ordered") && errors[0].message.contains("false"),
        "{errors:?}"
    );
    assert_eq!(errors[1].path, "arguments.value[\"a.b\"][1]");
    assert!(
        errors[1].message.contains("unobservable")
            && errors[1].message.contains("$bound.value.end"),
        "{errors:?}"
    );
    assert!(
        errors[2].message.contains("positive-start") && errors[2].message.contains("false"),
        "{errors:?}"
    );
}

#[test]
fn local_rules_cannot_read_entity_command_sibling_or_undeclared_bound_fields() {
    for reference in [
        "$id",
        "$entity",
        "$version",
        "$state",
        "$to_state",
        "$from_state",
        "$args",
        "$args.value",
        "$fields",
        "$fields.value",
        "$old_fields.value",
        "$bound.outer",
        "$bound.value.typo",
    ] {
        let field = json!({"type":"object","properties":{"known":{"type":"integer"}},
            "invariants":[{"assert":{"exists":reference}}]});
        let Failure::Definition { detail, .. } =
            Validated::new(serde_json::from_value(document(field)).unwrap()).unwrap_err()
        else {
            panic!("{reference}: invalid scope must refuse at registration");
        };
        assert!(
            detail.contains(reference) || detail.contains("typo") || detail.contains("outer"),
            "{reference}: {detail}"
        );
    }
    let dormant = json!({"type":"union","union":{"tag":"kind","content":"value","variants":{
        "valid":{"type":"integer"},"dormant":{"type":"integer","invariants":[{"assert":{"exists":"$args.secret"}}]}
    }}});
    let Failure::Definition { detail, .. } =
        Validated::new(serde_json::from_value(document(dormant)).unwrap()).unwrap_err()
    else {
        panic!("dormant value rules must be checked");
    };
    assert!(detail.contains("$args.secret"), "{detail}");
}

#[test]
fn local_quantifiers_capture_the_value_and_preserve_lexical_shadowing() {
    let field = json!({"type":"object","properties":{
        "limit":{"type":"integer","required":true},
        "items":{"type":"array","required":true,"items":{"type":"integer"}}
    },"invariants":[{"assert":{"forall":{"over":"$bound.value.items","bind":"item",
        "body":{"lte":["$bound.item","$bound.value.limit"]}}}}]});
    let model = model(document(field.clone()));
    for value in [
        json!({"limit":2,"items":[]}),
        json!({"limit":2,"items":[1,2]}),
    ] {
        model.decide(None, call("create", value)).unwrap();
    }
    let failure = errors(
        model
            .decide(None, call("create", json!({"limit":2,"items":[1,3]})))
            .unwrap_err(),
    );
    assert!(failure[0].message.contains("false"), "{failure:?}");
    let mut shadowed = field;
    shadowed["invariants"][0]["assert"]["forall"]["bind"] = json!("value");
    shadowed["invariants"][0]["assert"]["forall"]["body"] =
        json!({"lte":["$bound.value","$bound.value.limit"]});
    let Failure::Definition { detail, .. } =
        Validated::new(serde_json::from_value(document(shadowed)).unwrap()).unwrap_err()
    else {
        panic!("the inner scalar binder must shadow the outer object");
    };
    assert!(detail.contains("limit"), "{detail}");
}

#[test]
fn defaults_nullable_values_and_inactive_alternatives_keep_local_rule_boundaries() {
    let field = json!({"type":"object","properties":{
        "amount":{"type":"nullable","items":positive()},
        "count":{"type":"integer","default":2,"invariants":[{"assert":{"gt":["$bound.value",0]}}]}
    }});
    let model = model(document(field.clone()));
    let missing = model.decide(None, call("create", json!({}))).unwrap();
    assert_eq!(missing.result.unwrap().fields["value"], json!({"count":2}));
    let nullable = model
        .decide(None, call("create", json!({"amount":null})))
        .unwrap();
    assert_eq!(
        nullable.result.unwrap().fields["value"],
        json!({"amount":null,"count":2})
    );
    let failures = errors(
        model
            .decide(None, call("create", json!({"amount":0})))
            .unwrap_err(),
    );
    assert_eq!(failures[0].path, "arguments.value.amount");
    assert!(failures[0].message.contains("positive"), "{failures:?}");
    let mut bad_default = field;
    bad_default["properties"]["count"]["default"] = json!(0);
    let Failure::Definition { detail, .. } =
        Validated::new(serde_json::from_value(document(bad_default)).unwrap()).unwrap_err()
    else {
        panic!("defaults must satisfy local rules at registration");
    };
    assert!(
        detail.contains("invalid default") && detail.contains("false"),
        "{detail}"
    );
    let model = model_from_union();
    model
        .decide(None, call("create", json!({"kind":"other","value":0})))
        .unwrap();
    let failures = errors(
        model
            .decide(None, call("create", json!({"kind":"positive","value":0})))
            .unwrap_err(),
    );
    assert!(failures[0].message.contains("positive"), "{failures:?}");
}

fn model_from_union() -> Validated {
    model(document(
        json!({"type":"union","union":{"tag":"kind","content":"value","variants":{
            "positive":positive(),"other":{"type":"integer"}
        }}}),
    ))
}

#[test]
fn value_rules_protect_new_state_emitted_events_and_business_errors() {
    for boundary in ["state", "event", "error"] {
        let command = if boundary == "error" {
            "reject"
        } else {
            "create"
        };
        let mut source = document(positive());
        source["commands"][command]["arguments"]["fields"]["value"] = json!({"type":"integer"});
        if boundary != "state" {
            source["entity"]["schema"]["fields"]["value"] = json!({"type":"integer"});
        }
        let failures = errors(
            model(source)
                .decide(None, call(command, json!(0)))
                .unwrap_err(),
        );
        assert!(
            failures[0].message.contains("positive"),
            "{boundary}: {failures:?}"
        );
        assert!(
            failures[0].path.starts_with(match boundary {
                "state" => "fields",
                "event" => "events",
                _ => "error",
            }),
            "{boundary}: {failures:?}"
        );
    }
    let model = model(document(positive()));
    let first = model.decide(None, call("create", json!(1))).unwrap();
    let before = first.result.as_ref().unwrap();
    let snapshot = before.clone();
    let failure = errors(
        model
            .decide(Some(before), call("replace", json!(0)))
            .unwrap_err(),
    );
    assert!(failure[0].message.contains("positive"), "{failure:?}");
    assert_eq!(*before, snapshot);
    let next = model
        .decide(Some(before), call("replace", json!(2)))
        .unwrap();
    assert_eq!(
        outcome::replay(&[first.clone(), next.clone()]).unwrap(),
        next.result
    );
    let mut source = document(positive());
    source["commands"]["replace"]["outcomes"]["replaced"]["effect"]["set"]["value"] = json!(0);
    let assigning = Validated::new(serde_json::from_value(source).unwrap()).unwrap();
    let before = first.result.as_ref().unwrap();
    let failure = errors(
        assigning
            .decide(Some(before), call("replace", json!(1)))
            .unwrap_err(),
    );
    assert_eq!(failure[0].path, "fields.value");
    assert!(failure[0].message.contains("positive"), "{failure:?}");
    let mut changed = serde_json::to_value(next).unwrap();
    changed["definition"]["entity"]["schema"]["fields"]["value"]["invariants"] = json!([]);
    let Failure::Replay { detail, .. } =
        outcome::replay(&[first, serde_json::from_value(changed).unwrap()]).unwrap_err()
    else {
        panic!("value rules cannot change in the middle of a recorded definition history");
    };
    assert!(detail.contains("definition changed"), "{detail}");
}

#[test]
fn value_rule_profiles_metadata_and_replay_remain_closed() {
    for profile in 1..=4 {
        let mut source = document(json!({"type":"integer","invariants":[]}));
        source["format"] = json!(format!("entity-outcome-definition/{profile}"));
        let Failure::Definition { detail, .. } =
            Validated::new(serde_json::from_value(source).unwrap()).unwrap_err()
        else {
            panic!("even explicitly empty metadata needs the new profile");
        };
        assert!(detail.contains("profile 5"), "{detail}");
    }
    let entity: EntityDefinition =
        serde_json::from_value(document(positive())["entity"].clone()).unwrap();
    let error = ValidatedDefinition::new(entity).unwrap_err();
    assert!(error.to_string().contains("profile 5"), "{error}");
    for invariants in [Value::Null, json!({}), json!([{"assert":true,"unknown":0}])] {
        let error = serde_json::from_value::<Definition>(document(
            json!({"type":"integer","invariants":invariants}),
        ))
        .unwrap_err();
        assert!(error.is_data(), "{error}");
    }
    let model = model(document(positive()));
    let record = model.decide(None, call("create", json!(1))).unwrap();
    let mut forged = record.clone();
    forged.invocation.arguments["value"] = json!(0);
    let failure = errors(outcome::replay(&[forged]).unwrap_err());
    assert!(failure[0].message.contains("positive"), "{failure:?}");
    let mut downgraded = serde_json::to_value(record).unwrap();
    downgraded["format"] = json!("entity-outcome-record/4");
    let Failure::Replay { detail, .. } =
        outcome::replay(&[serde_json::from_value(downgraded).unwrap()]).unwrap_err()
    else {
        panic!("record profile must match re-decided profile");
    };
    assert!(detail.contains("differs"), "{detail}");
}
