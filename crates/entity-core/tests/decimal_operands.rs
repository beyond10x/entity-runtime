//! Native decimal-wire expectations are independent of any floating-point literal parser.
use entity_core::{
    outcome::{self, Failure, Invocation, Record, Validated},
    CoreError,
};
use serde_json::{json, Value};

fn decimal() -> Value {
    json!({"type":"string","encoding":"decimal_text"})
}
fn compare(left: &str, right: Value, op: &str) -> Value {
    json!({"scalar_compare":{"left":left,"right":right,"op":op}})
}
fn document(predicate: Value) -> Value {
    json!({"format":"entity-outcome-definition/9","identity":{"field":"identity"},
        "entity":{"entity":"decimal.item","schema":{"fields":{"identity":{"type":"string","required":true}}},"lifecycle":{"initial":"open","states":["open"]}},
        "commands":{"inspect":{"arguments":{"fields":{"x":decimal(),"y":decimal()}},"observations":{},"outcomes":{
            "yes":{"condition":{"condition":"when","predicate":predicate},"effect":{"effect":"observe"}},
            "no":{"condition":{"condition":"otherwise"},"effect":{"effect":"observe"}}
        }}}
    })
}
fn checked(source: Value) -> Validated {
    Validated::new(serde_json::from_value(source).unwrap()).unwrap()
}
fn call(arguments: Value) -> Invocation {
    Invocation {
        command: "inspect".into(),
        id: outcome::identity_key(&json!("one")),
        arguments: arguments.as_object().unwrap().clone(),
        observations: Default::default(),
    }
}
fn expect(program: &Validated, args: Value, answer: bool) {
    let record = program.decide(None, call(args.clone())).unwrap();
    assert_eq!(record.outcome, if answer { "yes" } else { "no" }, "{args}");
    let bytes = serde_json::to_vec(&record).unwrap();
    let decoded: Record = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(serde_json::to_vec(&decoded).unwrap(), bytes);
    assert_eq!(outcome::replay(&[decoded]).unwrap(), None);
}

#[test]
fn decimal_operands_compare_all_digits_without_changing_wire_values() {
    for (x, y, op, answer) in [
        ("2", "10", "lt", true),
        ("10", "2", "lt", false),
        ("1.00", "1", "eq", true),
        ("-0.000", "0", "eq", true),
        ("9007199254740993", "9007199254740992", "gt", true),
        ("1.0000000000000000001", "1.0000000000000000000", "gt", true),
        ("-1.0000000000000000001", "-1", "lt", true),
        ("1.01", "1.0100", "ne", false),
    ] {
        let program = checked(document(compare(
            "$decimal.args.x",
            json!("$decimal.args.y"),
            op,
        )));
        expect(&program, json!({"x":x,"y":y}), answer);
    }
    let program = checked(document(compare("$decimal.args.x", json!(0), "gt")));
    for x in [
        format!("1{}", "0".repeat(400)),
        format!("0.{}1", "0".repeat(400)),
    ] {
        expect(&program, json!({"x":x}), true);
    }
    for predicate in [
        json!({"eq":["$decimal.args.x",1]}),
        json!({"in":["$decimal.args.x",[0,1,2]]}),
    ] {
        expect(&checked(document(predicate)), json!({"x":"1.00"}), true);
    }
    // Ordinary references remain text, even in profile 9; escaping is not conversion.
    expect(
        &checked(document(json!({"eq":["$args.x",1]}))),
        json!({"x":"1.00"}),
        false,
    );
    expect(
        &checked(document(
            json!({"eq":["$$decimal.args.x","$$decimal.args.x"]}),
        )),
        json!({}),
        true,
    );
}

#[test]
fn decimal_absence_grammar_and_truthiness_keep_their_distinct_contracts() {
    let mut source = document(compare("$decimal.args.x", json!(0), "gt"));
    source["commands"]["inspect"]["arguments"]["fields"]["x"] =
        json!({"type":"nullable","items":decimal()});
    let program = checked(source);
    for args in [json!({}), json!({"x":null})] {
        assert_eq!(
            program.decide(None, call(args)).unwrap_err(),
            Failure::Unobservable {
                outcomes: vec!["yes".into()],
                paths: vec!["$decimal.args.x".into()]
            }
        );
    }
    for x in [
        json!("1e3"),
        json!("+1"),
        json!("01"),
        json!("1."),
        json!("NaN"),
        json!(1),
        json!([]),
    ] {
        let error = program.decide(None, call(json!({"x":x}))).unwrap_err();
        assert!(
            matches!(error,Failure::Core(CoreError::Validation(ref errors)) if errors.iter().any(|e|e.path=="arguments.x")),
            "{error:?}"
        );
    }
    let truth = checked(document(json!({"truthy":"$decimal.args.x"})));
    expect(&truth, json!({"x":"-0.00"}), false);
    expect(&truth, json!({"x":"0.001"}), true);
    expect(&truth, json!({"x":format!("0.{}1","0".repeat(400))}), false);
    assert!(matches!(
        truth.decide(None, call(json!({"x":format!("1{}","0".repeat(400))}))),
        Err(Failure::Unobservable { .. })
    ));
}

#[test]
fn decimal_operands_require_profile_nine_declared_encoding_and_predicate_scope() {
    for profile in 1..=8 {
        let mut source = document(json!({"eq":["$decimal.args.x",1]}));
        source["format"] = json!(format!("entity-outcome-definition/{profile}"));
        if profile < 7 {
            source.as_object_mut().unwrap().remove("identity");
        }
        // Avoid unrelated old-profile encoding errors obscuring the operand refusal.
        source["commands"]["inspect"]["arguments"]["fields"] = json!({"x":{"type":"string"}});
        let error = Validated::new(serde_json::from_value(source).unwrap()).unwrap_err();
        assert!(
            matches!(error,Failure::Definition {ref detail,..} if detail.contains("profile 9")),
            "{profile}: {error:?}"
        );
    }
    for field in [
        json!({"type":"string"}),
        json!({"type":"json"}),
        json!({"type":"number"}),
        json!({"type":"string","encoding":"uuid_hyphenated"}),
    ] {
        let mut source = document(json!({"eq":["$decimal.args.x",1]}));
        source["commands"]["inspect"]["arguments"]["fields"]["x"] = field;
        let error = Validated::new(serde_json::from_value(source).unwrap()).unwrap_err();
        assert!(
            matches!(error,Failure::Definition {ref detail,..} if detail.contains("declared decimal-text field")),
            "{error:?}"
        );
    }
    for operand in [
        "$decimal.args.missing",
        "$decimal.bound.missing",
        "$decimal.state",
        "$decimal.decimal.args.x",
    ] {
        let error =
            Validated::new(serde_json::from_value(document(json!({"eq":[operand,1]}))).unwrap())
                .unwrap_err();
        assert!(
            matches!(error, Failure::Definition { .. }),
            "{operand}: {error:?}"
        );
    }
    for fields in [
        json!({"fields":{},"additional_fields":true}),
        json!({"fields":{"open":{"type":"json"}}}),
    ] {
        let operand = if fields["additional_fields"] == true {
            "$decimal.args.unknown"
        } else {
            "$decimal.args.open.unknown"
        };
        let mut source = document(json!({"eq":[operand,1]}));
        source["commands"]["inspect"]["arguments"] = fields;
        let error = Validated::new(serde_json::from_value(source).unwrap()).unwrap_err();
        assert!(
            matches!(error,Failure::Definition {ref detail,..} if detail.contains("declared decimal-text field")),
            "{error:?}"
        );
    }
    for payload in [
        json!("$decimal.args.x"),
        json!({"x":"$decimal.args.x"}),
        json!(["$decimal.args.x"]),
    ] {
        let mut source = document(json!(true));
        source["commands"]["inspect"]["outcomes"]["yes"]["effect"] =
            json!({"effect":"refuse","error":"bad","payload":payload,"schema":{}});
        let error = Validated::new(serde_json::from_value(source).unwrap()).unwrap_err();
        assert!(
            matches!(error,Failure::Definition {ref detail,..} if detail.contains("only available in predicates")),
            "{error:?}"
        );
    }
    let mut source = document(json!(true));
    source.as_object_mut().unwrap().remove("identity");
    assert!(
        matches!(Validated::new(serde_json::from_value(source).unwrap()),Err(Failure::Definition {path,..}) if path=="identity")
    );
}

#[test]
fn decimal_value_invariants_and_quantifiers_use_declared_lexical_bindings() {
    let mut source = document(
        json!({"forall":{"over":"$args.x","bind":"item","body":compare("$decimal.bound.item",json!(0),"gt")}}),
    );
    source["commands"]["inspect"]["arguments"]["fields"]["x"] =
        json!({"type":"array","items":decimal()});
    expect(&checked(source), json!({"x":["1.00","2","10"]}), true);
    let mut source = document(json!(true));
    source["commands"]["inspect"]["arguments"]["fields"]["x"]["invariants"] =
        json!([{"name":"positive","assert":compare("$decimal.bound.value",json!(0),"gt")}]);
    let program = checked(source);
    expect(&program, json!({"x":"0.0001"}), true);
    assert!(
        matches!(program.decide(None,call(json!({"x":"-0.00"}))),Err(Failure::Core(CoreError::Validation(errors))) if errors.iter().any(|e|e.path=="arguments.x"&&e.message.contains("positive")))
    );
}

#[test]
fn decimal_state_events_and_full_replay_preserve_spelling_and_predicate_authority() {
    let mut source = document(compare("$decimal.args.x", json!(0), "ge"));
    source["entity"]["schema"]["fields"]["x"] = decimal();
    source["entity"]["invariants"] =
        json!([{"name":"nonnegative","assert":compare("$decimal.fields.x",json!(0),"ge")}]);
    source["commands"]["inspect"]["outcomes"]["yes"]["effect"] = json!({"effect":"create","set":{"identity":"one","x":"$args.x"},"emits":[{"template":{"type":"Stored","payload":{"x":"$fields.x"}},"schema":{"fields":{"x":decimal()}}}]});
    source["commands"]["replace"] = source["commands"]["inspect"].clone();
    source["commands"]["replace"]["outcomes"]["yes"]["effect"]["effect"] = json!("change");
    source["commands"]["replace"]["outcomes"]["yes"]["effect"]["transitions"] =
        json!([{"from":"open","to":"open"}]);
    // A true selection cannot bypass an entity invariant, even for a valid decimal string.
    let mut invariant_only = source.clone();
    invariant_only["commands"]["inspect"]["outcomes"]["yes"]["condition"]["predicate"] =
        json!(true);
    assert!(
        matches!(checked(invariant_only).decide(None,call(json!({"x":"-1"}))),Err(Failure::Core(CoreError::InvariantViolation {rule:Some(rule),..})) if rule=="nonnegative")
    );
    let program = checked(source);
    let first = program
        .decide(None, call(json!({"x":"1.0000000000000000001"})))
        .unwrap();
    assert_eq!(
        first.result.as_ref().unwrap().fields["x"],
        json!("1.0000000000000000001")
    );
    assert_eq!(
        first.events[0].payload,
        json!({"x":"1.0000000000000000001"})
    );
    let mut replace = call(json!({"x":"-0.000"}));
    replace.command = "replace".into();
    let second = program.decide(first.result.as_ref(), replace).unwrap();
    assert_eq!(second.result.as_ref().unwrap().fields["x"], json!("-0.000"));
    assert_eq!(
        outcome::replay(&[first.clone(), second.clone()]).unwrap(),
        second.result
    );
    for (pointer, value) in [
        ("/invocation/arguments/x", json!("-1")),
        ("/result/fields/x", json!("1.0")),
        ("/events/0/payload/x", json!("1.0")),
        (
            "/definition/commands/inspect/outcomes/yes/condition/predicate/scalar_compare/op",
            json!("lt"),
        ),
        ("/format", json!("entity-outcome-record/8")),
    ] {
        let mut tampered = serde_json::to_value(&first).unwrap();
        *tampered.pointer_mut(pointer).unwrap() = value;
        assert!(
            matches!(
                outcome::replay(&[serde_json::from_value(tampered).unwrap()]),
                Err(Failure::Replay { index: 0, .. })
            ),
            "{pointer}"
        );
    }
}
