//! Expected IEEE bit patterns are authored independently of the conversion implementation.
use entity_core::{
    outcome::{self, Failure, Invocation, Record, Validated},
    CoreError, ObjectSchema,
};
use serde_json::{json, Value};

const VECTORS: &[(&str, u64)] = &[
    ("0", 0),
    ("-0", 0x8000000000000000),
    ("-0.0", 0x8000000000000000),
    ("9007199254740993", 0x4340000000000000),
    ("9007199254740995", 0x4340000000000002),
    ("-9007199254740993", 0xc340000000000000),
    ("-9007199254740995", 0xc340000000000002),
    (
        "1.00000000000000011102230246251565404236316680908203125",
        0x3ff0000000000000,
    ),
    (
        "1.00000000000000011102230246251565404236316680908203126",
        0x3ff0000000000001,
    ),
    ("1.7976931348623157e308", 0x7fefffffffffffff),
    ("2.2250738585072014e-308", 0x0010000000000000),
    ("5e-324", 1),
    ("-5e-324", 0x8000000000000001),
    ("2.4703282292062327e-324", 0),
    ("2.4703282292062328e-324", 1),
    ("-2.4703282292062327e-324", 0x8000000000000000),
    ("1e-999", 0),
    ("-1e-999", 0x8000000000000000),
];
fn binary() -> Value {
    json!({"type":"number","number_encoding":"binary64"})
}
fn schema(field: Value) -> ObjectSchema {
    serde_json::from_value(json!({"fields":{"x":field}})).unwrap()
}
fn document() -> Value {
    let fields = json!({"identity":{"type":"string","required":true},"x":binary()});
    json!({"format":"entity-outcome-definition/10","identity":{"field":"identity"},
        "entity":{"entity":"finite.sample","schema":{"fields":fields},"lifecycle":{"initial":"open","states":["open"]}},
        "commands":{"check":{"arguments":{"fields":{"x":binary()}},"observations":{},"outcomes":{
            "positive":{"condition":{"condition":"when","predicate":{"scalar_compare":{"left":"$args.x","right":0,"op":"gt"}}},"effect":{"effect":"create","set":{"identity":"one","x":"$args.x"},"emits":[{"template":{"type":"Stored","payload":{"x":9007199254740993_i64}},"schema":{"fields":{"x":binary()}}}]}},
            "zero":{"condition":{"condition":"otherwise"},"effect":{"effect":"refuse","error":"Nonpositive","payload":{"x":"$args.x"},"schema":{"fields":{"x":binary()}}}}
        }}}
    })
}
fn checked(value: Value) -> Validated {
    Validated::new(serde_json::from_value(value).unwrap()).unwrap()
}
fn call(arguments: serde_json::Map<String, Value>) -> Invocation {
    Invocation {
        command: "check".into(),
        id: outcome::identity_key(&json!("one")),
        arguments,
        observations: Default::default(),
    }
}
fn assert_bits(value: &Value, bits: u64) {
    assert_eq!(value.as_f64().unwrap().to_bits(), bits, "{value}");
    let text = serde_json::to_string(value).unwrap();
    assert!(
        text.contains(['.', 'e', 'E']),
        "missing floating marker: {text}"
    );
    if bits == 0x8000000000000000 {
        assert_eq!(text, "-0.0");
    }
}

#[test]
fn raw_binary64_decoding_preserves_ieee_bits_through_every_typed_container() {
    let nullable = json!({"type":"nullable","items":binary()});
    for (field, template, pointer) in [
        (binary(), "TOKEN", "/x"),
        (nullable, "TOKEN", "/x"),
        (json!({"type":"array","items":binary()}), "[TOKEN]", "/x/0"),
        (
            json!({"type":"map","items":binary()}),
            "{\"entry\":TOKEN}",
            "/x/entry",
        ),
        (
            json!({"type":"object","properties":{"leaf":binary()}}),
            "{\"leaf\":TOKEN}",
            "/x/leaf",
        ),
        (
            json!({"type":"union","union":{"tag":"kind","content":"value","variants":{"number":binary(),"text":{"type":"string"}}}}),
            "{\"kind\":\"number\",\"value\":TOKEN}",
            "/x/value",
        ),
    ] {
        for &(token, bits) in VECTORS {
            let text = format!("{{\"x\":{}}}", template.replace("TOKEN", token));
            let value = Value::Object(schema(field.clone()).decode_json(&text).unwrap());
            assert_bits(value.pointer(pointer).unwrap(), bits);
        }
    }
    let nullable = schema(json!({"type":"nullable","items":binary()}));
    assert_eq!(
        nullable.decode_json("{\"x\":null}").unwrap()["x"],
        Value::Null
    );
    assert!(nullable.decode_json("{}").unwrap().is_empty());
}

#[test]
fn binary64_source_decoding_refuses_nonfinite_wrong_kinds_and_private_marker_objects() {
    let schema = schema(binary());
    for text in ["1e999", "-1e999", "1.7976931348623159e308"] {
        let error = schema
            .decode_json(&format!("{{\"x\":{text}}}"))
            .unwrap_err();
        assert!(error.to_string().contains("finite"), "{error}");
    }
    for text in [
        "null",
        "true",
        "\"1\"",
        "[]",
        "{}",
        "{\"$serde_json::private::Number\":\"-0\"}",
        "{\"$serde_json::private::RawValue\":\"-0\"}",
    ] {
        let error = schema
            .decode_json(&format!("{{\"x\":{text}}}"))
            .unwrap_err();
        assert!(
            error.to_string().contains("JSON number token"),
            "{text}: {error}"
        );
    }
    for text in ["NaN", "Inf", "+1", "01", "1.", "1e", "1 2", "/*x*/1"] {
        let error = schema
            .decode_json(&format!("{{\"x\":{text}}}"))
            .unwrap_err();
        assert!(error.is_syntax(), "{text}: {error}");
    }
    // Value conversion is honest about what a caller's earlier parser already discarded.
    let generic: Value = serde_json::from_str("-0").unwrap();
    assert_eq!(generic.as_f64().unwrap().to_bits(), 0);
    let decoded = schema.decode_json("{\"x\":-0}").unwrap();
    assert_bits(&decoded["x"], 0x8000000000000000);
}

#[test]
fn finite_values_are_normalized_before_selection_defaults_state_events_and_errors() {
    let source = document();
    let program = checked(source.clone());
    for &(token, bits) in VECTORS {
        let arguments = schema(binary())
            .decode_json(&format!("{{\"x\":{token}}}"))
            .unwrap();
        let record = program.decide(None, call(arguments)).unwrap();
        assert_bits(&record.invocation.arguments["x"], bits);
        if f64::from_bits(bits) > 0.0 {
            assert_eq!(record.outcome, "positive");
            assert_bits(&record.result.as_ref().unwrap().fields["x"], bits);
            assert_bits(&record.events[0].payload["x"], 0x4340000000000000);
        } else {
            assert_eq!(record.outcome, "zero");
            assert_bits(&record.error.as_ref().unwrap().payload["x"], bits);
        }
        let bytes = serde_json::to_vec(&record).unwrap();
        let decoded: Record = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(serde_json::to_vec(&decoded).unwrap(), bytes);
        assert_eq!(outcome::replay(&[decoded]).unwrap(), record.result);
    }
    let mut defaults = source.clone();
    defaults["commands"]["check"]["arguments"]["fields"]["x"]["default"] =
        json!(9007199254740993_i64);
    let record = checked(defaults)
        .decide(None, call(Default::default()))
        .unwrap();
    assert_bits(&record.invocation.arguments["x"], 0x4340000000000000);
    for bad in [
        json!("1"),
        json!(null),
        serde_json::from_str("1e999").unwrap(),
        json!({"$serde_json::private::Number":"1"}),
    ] {
        let error = program
            .decide(None, call(json!({"x":bad}).as_object().unwrap().clone()))
            .unwrap_err();
        assert!(
            matches!(error,Failure::Core(CoreError::Validation(ref errors)) if errors.iter().any(|e|e.path=="arguments.x")),
            "{error:?}"
        );
    }
    // Programmatic numeric input is rounded as well, before this equality is evaluated.
    let mut rounded = source;
    rounded["commands"]["check"]["outcomes"]["positive"]["condition"]["predicate"] =
        json!({"eq":["$args.x",9007199254740992_i64]});
    assert_eq!(
        checked(rounded)
            .decide(
                None,
                call(
                    json!({"x":9007199254740993_i64})
                        .as_object()
                        .unwrap()
                        .clone()
                )
            )
            .unwrap()
            .outcome,
        "positive"
    );
}

#[test]
fn binary64_changes_and_replay_refuse_unnormalized_state_and_changed_signs() {
    let mut source = document();
    source["commands"]["replace"] = source["commands"]["check"].clone();
    source["commands"]["replace"]["outcomes"]["positive"]["condition"]["predicate"] = json!(true);
    source["commands"]["replace"]["outcomes"]["positive"]["effect"]["effect"] = json!("change");
    source["commands"]["replace"]["outcomes"]["positive"]["effect"]["transitions"] =
        json!([{"from":"open","to":"open"}]);
    let program = checked(source);
    let first = program
        .decide(None, call(json!({"x":1}).as_object().unwrap().clone()))
        .unwrap();
    let mut next = call(schema(binary()).decode_json("{\"x\":-0}").unwrap());
    next.command = "replace".into();
    let second = program.decide(first.result.as_ref(), next.clone()).unwrap();
    assert_bits(
        &second.result.as_ref().unwrap().fields["x"],
        0x8000000000000000,
    );
    assert_eq!(
        outcome::replay(&[first.clone(), second.clone()]).unwrap(),
        second.result
    );
    let mut before = first.result.clone().unwrap();
    before.fields.insert("x".into(), json!(1));
    assert!(
        matches!(program.decide(Some(&before),next),Err(Failure::Core(CoreError::Validation(errors))) if errors.iter().any(|e| e.path=="before.fields.x"))
    );
    for pointer in ["/invocation/arguments/x", "/result/fields/x"] {
        let mut tampered = serde_json::to_value(&second).unwrap();
        *tampered.pointer_mut(pointer).unwrap() = json!(0.0);
        assert!(
            matches!(
                outcome::replay(&[first.clone(), serde_json::from_value(tampered).unwrap()]),
                Err(Failure::Replay { index: 1, .. })
            ),
            "{pointer}"
        );
    }
}

#[test]
fn binary64_encoding_is_closed_profile_gated_and_typed_identity_is_already_normalized() {
    for profile in 1..=9 {
        let mut source = document();
        source["format"] = json!(format!("entity-outcome-definition/{profile}"));
        if profile < 7 {
            source.as_object_mut().unwrap().remove("identity");
        }
        let error = Validated::new(serde_json::from_value(source).unwrap()).unwrap_err();
        assert!(
            matches!(error,Failure::Definition {ref detail,..} if detail.contains("profile 10")),
            "{profile}: {error:?}"
        );
    }
    for encoding in [json!(null), json!("unknown")] {
        let mut field = binary();
        field["number_encoding"] = encoding;
        let error = serde_json::from_value::<entity_core::FieldDefinition>(field).unwrap_err();
        assert!(error.is_data(), "{error}");
    }
    let mut wrong = document();
    wrong["commands"]["check"]["arguments"]["fields"]["x"]["type"] = json!("string");
    assert!(
        matches!(Validated::new(serde_json::from_value(wrong).unwrap()),Err(Failure::Definition {detail,..}) if detail.contains("number_encoding"))
    );
    let mut source = document();
    source["entity"]["schema"]["fields"]["identity"] = binary();
    source["entity"]["schema"]["fields"]["identity"]["required"] = json!(true);
    source["commands"]["check"]["outcomes"]["positive"]["effect"]["set"]["identity"] =
        json!("$args.x");
    let program = checked(source);
    let mut invocation = call(json!({"x":1}).as_object().unwrap().clone());
    invocation.id = outcome::identity_key(&json!(1));
    assert!(
        matches!(program.decide(None,invocation.clone()),Err(Failure::Core(CoreError::Validation(errors))) if errors.iter().any(|e|e.path=="identity.identity"))
    );
    invocation.id = outcome::identity_key(&json!(1.0));
    let record = program.decide(None, invocation).unwrap();
    assert_bits(
        &record.result.as_ref().unwrap().fields["identity"],
        0x3ff0000000000000,
    );
    assert_eq!(
        outcome::replay(std::slice::from_ref(&record)).unwrap(),
        record.result
    );
}
