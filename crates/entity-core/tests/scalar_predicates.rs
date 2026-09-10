//! Scalar observations preserve Unknown and explicit scale authority through replay.

use entity_core::outcome::{self, Definition, Failure, Invocation, Record, Validated};
use entity_core::{CoreError, EntityDefinition, ValidatedDefinition};
use serde_json::{json, Value};

fn document(predicate: Value, fields: Value) -> Value {
    json!({
        "format":"entity-outcome-definition/6",
        "entity":{"entity":"sample.scalar","schema":{"fields":{}},
            "lifecycle":{"initial":"open","states":["open"]}},
        "commands":{"inspect":{"arguments":{"fields":fields},"observations":{},"outcomes":{
            "yes":{"condition":{"condition":"when","predicate":predicate},"effect":{"effect":"observe"}},
            "no":{"condition":{"condition":"otherwise"},"effect":{"effect":"observe"}}
        }}}
    })
}

fn model(predicate: Value, fields: Value) -> Validated {
    Validated::new(serde_json::from_value(document(predicate, fields)).unwrap()).unwrap()
}

fn call(arguments: Value) -> Invocation {
    Invocation {
        command: "inspect".into(),
        id: "scalar-a".into(),
        arguments: arguments.as_object().unwrap().clone(),
        observations: Default::default(),
    }
}

fn compare(op: &str, scales: Value) -> Value {
    json!({"scalar_compare":{"left":"$args.x","right":"$args.y","op":op,"scales":scales}})
}

fn result(model: &Validated, arguments: Value) -> Result<Record, Failure> {
    model.decide(None, call(arguments))
}

fn expected(model: &Validated, arguments: Value, answer: Option<bool>) {
    let actual = result(model, arguments.clone());
    match answer {
        Some(answer) => {
            let record = actual.unwrap_or_else(|error| panic!("{arguments}: {error:?}"));
            assert_eq!(
                record.outcome,
                if answer { "yes" } else { "no" },
                "{arguments}"
            );
            assert_eq!(
                record.result, None,
                "observations do not fabricate instances"
            );
            assert!(record.events.is_empty(), "observations emit no events");
            assert_eq!(outcome::replay(&[record]).unwrap(), None);
        }
        None => assert!(
            matches!(actual, Err(Failure::Unobservable { .. })),
            "{arguments}: {actual:?}"
        ),
    }
}

#[test]
fn scalar_truth_distinguishes_present_false_values_and_unreadable_observations() {
    let model = model(json!({"truthy":"$args.x"}), json!({"x":{"type":"json"}}));
    for value in [
        json!(false),
        json!(0),
        json!(-0.0),
        json!(""),
        json!("false"),
    ] {
        expected(&model, json!({"x":value}), Some(false));
    }
    for value in [
        json!(true),
        json!(1),
        json!(-1),
        json!(9007199254740993_i64),
        json!("False"),
        json!("0"),
        json!(" false "),
    ] {
        expected(&model, json!({"x":value}), Some(true));
    }
    for value in [Value::Null, json!([]), json!({})] {
        expected(&model, json!({"x":value}), None);
    }
    expected(&model, json!({}), None);
    // Preserve the ESS finite-conversion truthiness contract instead of exact nonzero truth.
    for (number, answer) in [
        ("1e-400", Some(false)),
        ("5e-324", Some(true)),
        ("1e400", None),
    ] {
        expected(
            &model,
            json!({"x":serde_json::from_str::<Value>(number).unwrap()}),
            answer,
        );
    }
}

#[test]
fn scale_order_requires_consensus_and_never_guesses_lexical_order() {
    let fields = json!({"x":{"type":"string"},"y":{"type":"string"}});
    for (scales, x, y, answer) in [
        (json!({}), "same", "same", None),
        (json!({"rank":["z","a"]}), "z", "a", Some(true)),
        (json!({"rank":["z","a"]}), "a", "z", Some(false)),
        (json!({"rank":["z","a"]}), "missing", "a", None),
        (json!({"a":["z","a"],"b":["a","z"]}), "z", "a", None),
        (
            json!({"a":["z","a"],"b":["z","middle","a"],"partial":["a"]}),
            "z",
            "a",
            Some(true),
        ),
        (
            json!({"empty":[],"duplicates":["z","a","z"]}),
            "z",
            "a",
            Some(true),
        ),
        (json!({"rank":["same"]}), "same", "same", Some(false)),
    ] {
        expected(
            &model(compare("lt", scales), fields.clone()),
            json!({"x":x,"y":y}),
            answer,
        );
    }
    for op in ["le", "ge"] {
        expected(
            &model(compare(op, json!({"rank":["same"]})), fields.clone()),
            json!({"x":"same","y":"same"}),
            Some(true),
        );
    }
}

#[test]
fn scalar_comparison_keeps_exact_numbers_type_sensitive_equality_and_unknown_order() {
    let fields = json!({"x":{"type":"json"},"y":{"type":"json"}});
    for (op, x, y, answer) in [
        (
            "gt",
            json!(9007199254740993_i64),
            json!(9007199254740992_i64),
            Some(true),
        ),
        ("eq", json!(100), json!(100.0), Some(true)),
        ("eq", json!(-0.0), json!(0), Some(true)),
        ("eq", json!("low"), json!("low"), Some(true)),
        ("eq", json!("1"), json!(1), Some(false)),
        ("ne", json!(false), json!(0), Some(true)),
        ("lt", json!(false), json!(true), None),
        ("ge", json!("1"), json!(1), None),
        ("eq", json!([]), json!([]), None),
        ("ne", Value::Null, json!(1), None),
    ] {
        expected(
            &model(compare(op, json!({})), fields.clone()),
            json!({"x":x,"y":y}),
            answer,
        );
    }
}

#[test]
fn negated_unknown_names_unreadable_addresses_without_selecting_otherwise() {
    let fields = json!({"x":{"type":"nullable","items":{"type":"string"}},"y":{"type":"string"}});
    let model = model(json!({"not":compare("lt", json!({}))}), fields);
    for arguments in [
        json!({}),
        json!({"x":null}),
        json!({"x":"left","y":"right"}),
    ] {
        assert_eq!(
            result(&model, arguments).unwrap_err(),
            Failure::Unobservable {
                outcomes: vec!["yes".into()],
                paths: vec!["$args.x".into(), "$args.y".into()]
            }
        );
    }
    assert_eq!(
        result(&model, json!({"x":"left"})).unwrap_err(),
        Failure::Unobservable {
            outcomes: vec!["yes".into()],
            paths: vec!["$args.y".into()]
        }
    );
}

#[test]
fn scalar_predicates_are_profile_gated_closed_and_schema_checked() {
    let fields = json!({"x":{"type":"integer"}});
    for profile in 1..=5 {
        for predicate in [
            json!({"truthy":"$args.x"}),
            json!({"scalar_compare":{"left":"$args.x","right":0,"op":"gt"}}),
        ] {
            let mut source = document(predicate, fields.clone());
            source["format"] = json!(format!("entity-outcome-definition/{profile}"));
            assert!(
                matches!(Validated::new(serde_json::from_value(source).unwrap()), Err(Failure::Definition { detail, .. }) if detail.contains("profile 6"))
            );
        }
    }
    let mut legacy: EntityDefinition =
        serde_json::from_value(document(json!(true), json!({}))["entity"].clone()).unwrap();
    legacy.invariants =
        serde_json::from_value(json!([{"name":"new-rule","assert":{"truthy":true}}])).unwrap();
    assert!(ValidatedDefinition::new(legacy)
        .unwrap_err()
        .to_string()
        .contains("profile 6"));

    for operand in [
        json!(null),
        json!([]),
        json!({}),
        json!("$args"),
        json!("$args.missing"),
        json!("$bound.lost"),
        json!("$state"),
    ] {
        let failure = Validated::new(
            serde_json::from_value(document(json!({"truthy":operand}), fields.clone())).unwrap(),
        )
        .unwrap_err();
        assert!(matches!(failure, Failure::Definition { .. }), "{failure:?}");
    }
    let source = document(
        json!({"truthy":"$args.x"}),
        json!({"x":{"type":"array","items":{"type":"integer"}}}),
    );
    assert!(
        matches!(Validated::new(serde_json::from_value(source).unwrap()), Err(Failure::Definition { detail, .. }) if detail.contains("scalar field"))
    );
    for condition in [
        json!({"truthy":true,"eq":[1,1]}),
        json!({"scalar_compare":{"left":1,"right":2,"op":"bad"}}),
        json!({"scalar_compare":{"left":1,"right":2,"op":"lt","extra":true}}),
        json!({"scalar_compare":{"left":1,"right":2,"op":"lt","scales":null}}),
    ] {
        let error =
            serde_json::from_value::<Definition>(document(condition, fields.clone())).unwrap_err();
        assert!(error.is_data(), "{error}");
    }
}

#[test]
fn scalar_rules_follow_lexical_value_scopes_and_replay_their_scale_authority() {
    let field = json!({"type":"array","items":{"type":"string"},"invariants":[{
        "name":"all-truthy","assert":{"forall":{"over":"$bound.value","bind":"item","body":{"truthy":"$bound.item"}}}
    }]});
    let model = model(json!(true), json!({"x":field}));
    expected(&model, json!({"x":["yes","0"]}), Some(true));
    let failure = result(&model, json!({"x":["yes","false"]})).unwrap_err();
    assert!(
        matches!(failure, Failure::Core(CoreError::Validation(errors)) if errors.iter().any(|e| e.path=="arguments.x" && e.message.contains("all-truthy"))),
        "local quantified rule is enforced"
    );

    let model = self::model(
        compare("lt", json!({"rank":["z","a"]})),
        json!({"x":{"type":"string"},"y":{"type":"string"}}),
    );
    let record = result(&model, json!({"x":"z","y":"a"})).unwrap();
    let bytes = serde_json::to_vec(&record).unwrap();
    let roundtrip: Record = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(serde_json::to_vec(&roundtrip).unwrap(), bytes);
    assert_eq!(outcome::replay(&[roundtrip]).unwrap(), None);
    for pointer in [
        "/invocation/arguments/x",
        "/outcome",
        "/format",
        "/definition/commands/inspect/outcomes/yes/condition/predicate/scalar_compare/scales/rank",
    ] {
        let mut tampered = serde_json::to_value(&record).unwrap();
        *tampered.pointer_mut(pointer).unwrap() = match pointer {
            "/outcome" => json!("no"),
            "/format" => json!("entity-outcome-record/5"),
            "/invocation/arguments/x" => json!("a"),
            _ => json!(["a", "z"]),
        };
        let tampered = serde_json::from_value(tampered).unwrap();
        assert!(
            matches!(
                outcome::replay(&[tampered]),
                Err(Failure::Replay { index: 0, .. })
            ),
            "{pointer}"
        );
    }
}

#[test]
fn scalar_state_and_payload_invariants_guard_create_change_and_replay() {
    let mut source = document(json!(true), json!({"x":{"type":"integer"}}));
    source["entity"]["schema"] = json!({"fields":{"x":{"type":"integer"}}});
    source["entity"]["invariants"] = json!([{"name":"nonzero","assert":{"truthy":"$fields.x"}}]);
    source["commands"]["inspect"]["outcomes"]["yes"]["effect"] =
        json!({"effect":"create","set":{"x":"$args.x"},"emits":[]});
    source["commands"]["replace"] = source["commands"]["inspect"].clone();
    source["commands"]["replace"]["outcomes"]["yes"]["effect"] = json!({"effect":"change","transitions":[{"from":"open","to":"open"}],"set":{"x":"$args.x"},"emits":[]});
    let model = Validated::new(serde_json::from_value(source.clone()).unwrap()).unwrap();
    assert!(
        matches!(model.decide(None, call(json!({"x":0}))), Err(Failure::Core(CoreError::InvariantViolation { rule:Some(rule), .. })) if rule=="nonzero")
    );
    let created = model.decide(None, call(json!({"x":1}))).unwrap();
    let mut replace = call(json!({"x":0}));
    replace.command = "replace".into();
    assert!(
        matches!(model.decide(created.result.as_ref(), replace.clone()), Err(Failure::Core(CoreError::InvariantViolation { rule:Some(rule), .. })) if rule=="nonzero")
    );
    replace.arguments.insert("x".into(), json!(2));
    let changed = model.decide(created.result.as_ref(), replace).unwrap();
    assert_eq!(changed.result.as_ref().unwrap().revision, 2);
    assert_eq!(
        outcome::replay(&[created, changed.clone()]).unwrap(),
        changed.result
    );

    let payload_schema = json!({"fields":{"x":{"type":"integer","invariants":[{"name":"payload-nonzero","assert":{"truthy":"$bound.value"}}]}}});
    source["commands"]["inspect"]["outcomes"]["yes"]["effect"]["emits"] =
        json!([{"template":{"type":"Stored","payload":{"x":0}},"schema":payload_schema}]);
    let model = Validated::new(serde_json::from_value(source.clone()).unwrap()).unwrap();
    assert!(
        matches!(model.decide(None, call(json!({"x":1}))), Err(Failure::Core(CoreError::Validation(errors))) if errors.iter().any(|e| e.path=="events[0].payload.x" && e.message.contains("payload-nonzero")))
    );
    source["commands"]["inspect"]["outcomes"]["yes"]["effect"] =
        json!({"effect":"refuse","error":"Rejected","payload":{"x":0},"schema":payload_schema});
    let model = Validated::new(serde_json::from_value(source).unwrap()).unwrap();
    assert!(
        matches!(model.decide(None, call(json!({"x":1}))), Err(Failure::Core(CoreError::Validation(errors))) if errors.iter().any(|e| e.path.ends_with(".x") && e.message.contains("payload-nonzero")))
    );
}
