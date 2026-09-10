//! Exact wire admission through complete decisions, with no value normalization.

use entity_core::outcome::{self, Definition, Failure, Invocation, Record, Validated};
use entity_core::{CoreError, EntityDefinition, ValidatedDefinition};
use serde_json::{json, Value};

fn document(field: Value) -> Value {
    let shape = json!({"fields":{"value":field}});
    json!({
        "format":"entity-outcome-definition/4",
        "entity":{"entity":"sample.encoded","schema":shape,"lifecycle":{"initial":"open","states":["open"]}},
        "commands":{
            "create":{"arguments":shape,"observations":{},"outcomes":{"created":{
                "condition":{"condition":"otherwise"},"effect":{"effect":"create","set":{"value":"$args.value"},
                "emits":[{"template":{"type":"Stored","payload":"$fields"},"schema":shape}]}}}},
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
        id: "encoded-a".into(),
        arguments: json!({"value":value}).as_object().unwrap().clone(),
        observations: Default::default(),
    }
}

fn field(encoding: &str) -> Value {
    json!({"type":"string","encoding":encoding,"required":true})
}

#[test]
fn encoded_strings_preserve_wire_spelling_and_refuse_malformed_values() {
    for (encoding, valid, invalid) in [
        (
            "uuid_hyphenated",
            vec![
                "00000000-0000-0000-0000-000000000000",
                "ABCDEFAB-1234-ABCD-9876-ABCDEF012345",
            ],
            vec![
                "",
                "abcdefab1234abcd9876abcdef012345",
                "urn:uuid:abcdefab-1234-abcd-9876-abcdef012345",
                "abcdefab-1234-abcd-9876-abcdef01234g",
                "00000000-0000-0000-0000-000000000000\n",
            ],
        ),
        (
            "base64_padded",
            vec!["", "AA==", "AB==", "AAA=", "////", "YWJj", "YWJjZA=="],
            vec![
                "A", "AA", "AAA", "A===", "=AAA", "AA=A", "AA--", "AA__", "AA==\n", "éé",
            ],
        ),
        (
            "decimal_text",
            vec![
                "0",
                "-0",
                "-0.00",
                "1.00",
                "9007199254740993123456789.0000000000001",
                "-12.50",
            ],
            vec![
                "", "-", "+1", "01", "-01", ".1", "1.", "1e2", "NaN", " 1", "1\n", "１", "1.2.3",
            ],
        ),
    ] {
        let model = model(document(field(encoding)));
        for text in valid {
            let record = model.decide(None, call("create", json!(text))).unwrap();
            assert_eq!(record.result.as_ref().unwrap().fields["value"], text);
            assert_eq!(record.events[0].payload["value"], text);
            let copy: Record =
                serde_json::from_slice(&serde_json::to_vec(&record).unwrap()).unwrap();
            assert_eq!(
                outcome::replay(std::slice::from_ref(&copy)).unwrap(),
                record.result
            );
            let refused = model.decide(None, call("reject", json!(text))).unwrap();
            assert_eq!(refused.error.unwrap().payload["value"], text);
        }
        for value in
            invalid
                .into_iter()
                .map(|s| json!(s))
                .chain([json!(null), json!(1), json!(true)])
        {
            let Failure::Core(CoreError::Validation(errors)) = model
                .decide(None, call("create", value.clone()))
                .unwrap_err()
            else {
                panic!("{encoding} {value}: expected field validation refusal");
            };
            assert_eq!(errors.len(), 1, "{encoding} {value}");
            assert_eq!(errors[0].path, "arguments.value");
            assert!(
                errors[0].message.contains("match")
                    || errors[0].message.contains("expected string"),
                "{errors:?}"
            );
        }
    }
}

#[test]
fn encoded_map_keys_and_nested_values_accumulate_independent_errors() {
    let model = model(document(
        json!({"type":"map","key_encoding":"uuid_hyphenated",
        "items":{"type":"array","items":{"type":"nullable","items":field("decimal_text")}}}),
    ));
    let value = json!({"bad.key":["01",null,"1e1"],"00000000-0000-0000-0000-000000000000":["1.00"],"quote\"":[".1"]});
    let Failure::Core(CoreError::Validation(errors)) =
        model.decide(None, call("create", value)).unwrap_err()
    else {
        panic!("both map key and value failures must accumulate");
    };
    assert_eq!(errors.len(), 5, "{errors:?}");
    assert_eq!(
        errors
            .iter()
            .filter(|e| e.message.contains("map key"))
            .count(),
        2
    );
    assert!(
        errors
            .iter()
            .any(|e| e.path == "arguments.value[\"bad.key\"][0]"),
        "{errors:?}"
    );
    let value = json!({"00000000-0000-0000-0000-000000000000":["-0.00",null]});
    let record = model.decide(None, call("create", value.clone())).unwrap();
    assert_eq!(record.result.unwrap().fields["value"], value);
}

#[test]
fn encoding_admission_requires_profile_four_and_applicable_nonnull_metadata() {
    for profile in 1..=3 {
        let mut source = document(field("decimal_text"));
        source["format"] = json!(format!("entity-outcome-definition/{profile}"));
        let Failure::Definition { detail, .. } =
            Validated::new(serde_json::from_value(source).unwrap()).unwrap_err()
        else {
            panic!("old profiles must refuse encoded field vocabulary");
        };
        assert!(detail.contains("profile 4"), "{detail}");
        let mut source = document(
            json!({"type":"map","key_encoding":"uuid_hyphenated","items":{"type":"boolean"}}),
        );
        source["format"] = json!(format!("entity-outcome-definition/{profile}"));
        let Failure::Definition { detail, .. } =
            Validated::new(serde_json::from_value(source).unwrap()).unwrap_err()
        else {
            panic!("map key metadata must also require profile 4");
        };
        assert!(detail.contains("profile 4"), "{detail}");
    }
    let entity: EntityDefinition =
        serde_json::from_value(document(field("decimal_text"))["entity"].clone()).unwrap();
    let error = ValidatedDefinition::new(entity).unwrap_err();
    assert!(error.to_string().contains("profile 4"), "{error}");
    for metadata in ["encoding", "key_encoding"] {
        for supplied in [json!(null), json!("unknown"), json!(false)] {
            let mut f = json!({"type":"string"});
            f[metadata] = supplied;
            let error = serde_json::from_value::<Definition>(document(f)).unwrap_err();
            assert!(error.is_data(), "{error}");
        }
    }
    for f in [
        json!({"type":"integer","encoding":"decimal_text"}),
        json!({"type":"string","key_encoding":"uuid_hyphenated"}),
        json!({"type":"map","items":{"type":"string"},"encoding":"base64_padded"}),
    ] {
        let Failure::Definition { detail, .. } =
            Validated::new(serde_json::from_value(document(f)).unwrap()).unwrap_err()
        else {
            panic!("inapplicable metadata must refuse at registration");
        };
        assert!(detail.contains("applies"), "{detail}");
    }
}

#[test]
fn encoded_defaults_and_dormant_union_alternatives_are_checked_at_registration() {
    let mut encoded = field("decimal_text");
    encoded["default"] = json!("01");
    let Failure::Definition { detail, .. } =
        Validated::new(serde_json::from_value(document(encoded.clone())).unwrap()).unwrap_err()
    else {
        panic!("malformed encoded default must refuse");
    };
    assert!(detail.contains("invalid default"), "{detail}");
    encoded["default"] = json!("-0.00");
    let union = json!({"type":"union","union":{"tag":"kind","content":"value","variants":{
        "data":{"type":"object","properties":{"amount":encoded}},
        "other":{"type":"boolean"}
    }}});
    let mut dormant = union.clone();
    dormant["union"]["variants"]["other"] =
        json!({"type":"string","encoding":"decimal_text","default":"01"});
    let Failure::Definition { detail, .. } =
        Validated::new(serde_json::from_value(document(dormant)).unwrap()).unwrap_err()
    else {
        panic!("even a dormant alternative must reject its invalid encoded default");
    };
    assert!(detail.contains("invalid default"), "{detail}");
    let model = model(document(union));
    let record = model
        .decide(None, call("create", json!({"kind":"data","value":{}})))
        .unwrap();
    assert_eq!(
        record.result.unwrap().fields["value"]["value"]["amount"],
        "-0.00"
    );
}

#[test]
fn replay_refuses_changed_encoded_spelling_and_downgraded_profile() {
    let model = model(document(field("decimal_text")));
    let record = model.decide(None, call("create", json!("1.00"))).unwrap();
    let mut changed = record.clone();
    changed.events[0].payload["value"] = json!("1.0");
    let Failure::Replay { detail, .. } = outcome::replay(&[changed]).unwrap_err() else {
        panic!("different wire spelling must not be treated as the same decision");
    };
    assert!(detail.contains("differ"), "{detail}");
    let mut downgraded = serde_json::to_value(record).unwrap();
    downgraded["format"] = json!("entity-outcome-record/3");
    let Failure::Replay { detail, .. } =
        outcome::replay(&[serde_json::from_value(downgraded).unwrap()]).unwrap_err()
    else {
        panic!("record profile must match its definition");
    };
    assert!(detail.contains("differ"), "{detail}");
}

#[test]
fn encoded_event_and_error_payloads_refuse_even_when_arguments_are_plain_strings() {
    for command in ["create", "reject"] {
        let mut source = document(field("decimal_text"));
        source["commands"][command]["arguments"]["fields"]["value"] = json!({"type":"string"});
        if command == "create" {
            source["entity"]["schema"]["fields"]["value"] = json!({"type":"string"});
        }
        let Failure::Core(CoreError::Validation(errors)) = model(source)
            .decide(None, call(command, json!("01")))
            .unwrap_err()
        else {
            panic!("typed output must enforce encoding after valid plain-string input");
        };
        assert!(
            errors.iter().any(|e| e.message.contains("DecimalText")),
            "{errors:?}"
        );
    }
}
