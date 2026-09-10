//! Independently stated identity values and correspondence failures, not generated expectations.
use entity_core::outcome::{
    self, identity_key, identity_value, Definition, Failure, Invocation, Record, Validated,
};
use entity_core::CoreError;
use serde_json::{json, Value};

fn document(shape: Value) -> Value {
    json!({
        "format":"entity-outcome-definition/7", "identity":{"field":"identity"},
        "entity":{
            "entity":"typed.item", "version":1,
            "schema":{"fields":{"identity":shape, "count":{"type":"integer","required":true}}},
            "lifecycle":{"initial":"open","states":["open"]}
        },
        "commands":{
            "create":{
                "arguments":{"fields":{"identity":shape,"allow":{"type":"boolean","required":true}}},
                "observations":{}, "outcomes":{
                    "created":{"condition":{"condition":"when","predicate":{"eq":["$args.allow",true]}},
                        "effect":{"effect":"create","set":{"identity":"$args.identity","count":0},"emits":[
                            {"template":{"type":"Created","payload":{"identity":"$fields.identity"}}, "schema":{"fields":{"identity":shape}}},
                            {"template":{"type":"Audited","payload":{}},"schema":{}}
                        ]}},
                    "refused":{"condition":{"condition":"otherwise"},"effect":{"effect":"refuse","error":"Disabled","payload":{},"schema":{}}}
                }
            },
            "change":{
                "arguments":{"fields":{"identity":shape,"count":{"type":"integer","required":true}}},
                "observations":{},"outcomes":{"changed":{
                    "condition":{"condition":"otherwise"},"effect":{"effect":"change",
                        "transitions":[{"from":"open","to":"open"}],"set":{"identity":"$args.identity","count":"$args.count"},"emits":[]}
                }}
            },
            "observe":{"arguments":{},"observations":{},"outcomes":{"observed":{
                "condition":{"condition":"otherwise"},"effect":{"effect":"observe"}
            }}}
        }
    })
}

fn checked(document: Value) -> Validated {
    Validated::new(serde_json::from_value(document).unwrap()).unwrap()
}

fn call(command: &str, identity: &Value, arguments: Value) -> Invocation {
    Invocation {
        command: command.into(),
        id: identity_key(identity),
        arguments: arguments.as_object().unwrap().clone(),
        observations: serde_json::Map::new(),
    }
}

#[test]
fn typed_identity_keys_preserve_exact_values_and_refuse_noncanonical_aliases() {
    let examples = [
        (json!(null), r#"identity/1:"null""#),
        (json!(true), r#"identity/1:{"boolean":true}"#),
        (json!(1), r#"identity/1:{"number":"1"}"#),
        (json!("1"), r#"identity/1:{"string":"1"}"#),
        (
            json!(9007199254740993_i64),
            r#"identity/1:{"number":"9007199254740993"}"#,
        ),
        (json!("$id/é"), r#"identity/1:{"string":"$id/é"}"#),
        (
            json!([false, "x"]),
            r#"identity/1:{"array":[{"boolean":false},{"string":"x"}]}"#,
        ),
        (
            json!({"z":1,"a":true}),
            r#"identity/1:{"object":{"a":{"boolean":true},"z":{"number":"1"}}}"#,
        ),
    ];
    let mut keys = std::collections::BTreeSet::new();
    for (value, expected) in examples {
        assert_eq!(identity_key(&value), expected);
        assert_eq!(identity_value(expected).unwrap(), value);
        assert!(keys.insert(expected));
    }
    for number in ["-0", "-0.0", "1.0", "1e400", "9007199254740993"] {
        let value: Value = serde_json::from_str(number).unwrap();
        let decoded = identity_value(&identity_key(&value)).unwrap();
        assert_eq!(
            serde_json::to_string(&decoded).unwrap(),
            serde_json::to_string(&value).unwrap()
        );
    }
    let private_key = Value::Object(serde_json::Map::from_iter([(
        "$serde_json::private::Number".into(),
        json!("7"),
    )]));
    assert_eq!(
        identity_value(&identity_key(&private_key)).unwrap(),
        private_key
    );
    for invalid in [
        "1",
        "identity/2:null",
        "identity/1:null",
        "identity/1:{}",
        r#"identity/1: {"string":"x"}"#,
        r#"identity/1:{"number":"01"}"#,
        r#"identity/1:{"string":"\u0078"}"#,
        r#"identity/1:{"object":{"z":{"number":"1"},"a":{"boolean":true}}}"#,
        r#"identity/1:{"object":{"x":{"number":"1"},"x":{"number":"1"}}}"#,
        r#"identity/1:{"unknown":"x"}"#,
    ] {
        assert_eq!(
            identity_value(invalid),
            Err(Failure::IdentityKey),
            "{invalid}"
        );
    }
}

#[test]
fn typed_identity_is_validated_on_refusal_creation_change_and_replay() {
    for (shape, identity) in [
        (
            json!({"type":"integer","required":true}),
            json!(9007199254740993_i64),
        ),
        (
            json!({"type":"string","required":true,"encoding":"uuid_hyphenated"}),
            json!("00000000-0000-0000-0000-000000000001"),
        ),
        (
            json!({"type":"nullable","required":true,"items":{"type":"boolean"}}),
            Value::Null,
        ),
        (
            json!({"type":"object","required":true,"properties":{"tenant":{"type":"string","required":true},"ordinal":{"type":"integer","required":true}}}),
            json!({"tenant":"é","ordinal":3}),
        ),
    ] {
        let runtime = checked(document(shape));
        let refused = runtime
            .decide(
                None,
                call(
                    "create",
                    &identity,
                    json!({"identity":identity,"allow":false}),
                ),
            )
            .unwrap();
        assert_eq!(refused.result, None);
        assert_eq!(refused.error.as_ref().unwrap().name, "Disabled");
        assert!(refused.events.is_empty());
        let created = runtime
            .decide(
                None,
                call(
                    "create",
                    &identity,
                    json!({"identity":identity,"allow":true}),
                ),
            )
            .unwrap();
        let instance = created.result.as_ref().unwrap();
        assert_eq!(instance.fields["identity"], identity);
        assert_eq!(instance.revision, 1);
        assert_eq!(
            created
                .events
                .iter()
                .map(|e| e.event_type.as_str())
                .collect::<Vec<_>>(),
            ["Created", "Audited"]
        );
        assert_eq!(created.events[0].payload, json!({"identity":identity}));
        assert!(created
            .events
            .iter()
            .all(|e| e.id == identity_key(&identity) && e.revision == 1));
        let changed = runtime
            .decide(
                Some(instance),
                call("change", &identity, json!({"identity":identity,"count":2})),
            )
            .unwrap();
        assert_eq!(changed.result.as_ref().unwrap().revision, 2);
        assert_eq!(changed.result.as_ref().unwrap().fields["count"], json!(2));
        assert!(changed.events.is_empty());
        let observed = runtime
            .decide(
                changed.result.as_ref(),
                call("observe", &identity, json!({})),
            )
            .unwrap();
        assert_eq!(observed.result, changed.result);
        let records: Vec<Record> = serde_json::from_slice(
            &serde_json::to_vec(&[refused, created, changed, observed.clone()]).unwrap(),
        )
        .unwrap();
        assert_eq!(outcome::replay(&records).unwrap(), observed.result);
    }
}

#[test]
fn identity_correspondence_cannot_be_changed_in_input_previous_state_or_effects() {
    let runtime = checked(document(json!({"type":"integer","required":true,"min":1})));
    let key = json!(7);
    for command in ["create", "observe"] {
        let args = if command == "create" {
            json!({"identity":7,"allow":false})
        } else {
            json!({})
        };
        let failure = runtime
            .decide(None, call(command, &json!("7"), args))
            .unwrap_err();
        assert!(
            matches!(failure, Failure::Core(CoreError::Validation(ref errors)) if errors.iter().any(|e| e.path == "identity.identity")),
            "{failure:?}"
        );
    }
    assert_eq!(
        runtime.decide(
            None,
            call("create", &key, json!({"identity":8,"allow":true}))
        ),
        Err(Failure::IdentityMismatch)
    );
    let created = runtime
        .decide(
            None,
            call("create", &key, json!({"identity":7,"allow":true})),
        )
        .unwrap();
    let original = created.result.as_ref().unwrap();
    assert_eq!(
        runtime.decide(
            Some(original),
            call("change", &key, json!({"identity":8,"count":2}))
        ),
        Err(Failure::IdentityMismatch)
    );
    let mut corrupted = original.clone();
    corrupted.fields.insert("identity".into(), json!(8));
    assert_eq!(
        runtime.decide(Some(&corrupted), call("observe", &key, json!({}))),
        Err(Failure::IdentityMismatch)
    );
    assert_eq!(original.fields["identity"], json!(7));
    assert_eq!(original.revision, 1);

    let mut changed_id = created.clone();
    changed_id.invocation.id = identity_key(&json!(8));
    assert_eq!(
        outcome::replay(&[changed_id]),
        Err(Failure::IdentityMismatch)
    );
    let mut changed_result = created.clone();
    changed_result
        .result
        .as_mut()
        .unwrap()
        .fields
        .insert("identity".into(), json!(8));
    assert!(matches!(
        outcome::replay(&[changed_result]),
        Err(Failure::Replay { index: 0, .. })
    ));
    let mut downgrade = created;
    downgrade.format = outcome::RecordFormat::V6;
    assert!(matches!(
        outcome::replay(&[downgrade]),
        Err(Failure::Replay { index: 0, .. })
    ));
}

#[test]
fn typed_identity_contract_is_closed_required_and_has_no_implicit_value_rules() {
    let base = document(json!({"type":"integer","required":true}));
    for identity in [Value::Null, json!({"field":"identity","extra":false})] {
        let mut source = base.clone();
        source["identity"] = identity;
        let error = serde_json::from_value::<Definition>(source).unwrap_err();
        assert!(error.is_data(), "{error}");
    }
    let mut absent = base.clone();
    absent.as_object_mut().unwrap().remove("identity");
    assert!(
        matches!(Validated::new(serde_json::from_value(absent).unwrap()), Err(Failure::Definition {path, ..}) if path == "identity")
    );
    for profile in 1..=6 {
        let mut source = base.clone();
        source["format"] = json!(format!("entity-outcome-definition/{profile}"));
        assert!(
            matches!(Validated::new(serde_json::from_value(source).unwrap()), Err(Failure::Definition {detail, ..}) if detail.contains("profile 7"))
        );
    }
    for shape in [
        json!({"type":"integer"}),
        json!({"type":"json","required":true}),
        json!({"type":"integer","required":true,"default":7}),
        json!({"type":"object","required":true,"additional_properties":true}),
        json!({"type":"array","required":true,"items":{"type":"json"}}),
        json!({"type":"object","required":true,"properties":{"x":{"type":"integer","default":0}}}),
    ] {
        let error = Validated::new(serde_json::from_value(document(shape)).unwrap()).unwrap_err();
        assert!(matches!(error, Failure::Definition { .. }), "{error:?}");
    }
    let mut missing_set = base.clone();
    missing_set["commands"]["create"]["outcomes"]["created"]["effect"]["set"]
        .as_object_mut()
        .unwrap()
        .remove("identity");
    assert!(
        matches!(Validated::new(serde_json::from_value(missing_set).unwrap()), Err(Failure::Definition {detail, ..}) if detail.contains("explicitly set"))
    );
    let mut wrong = base;
    wrong["identity"]["field"] = json!("absent");
    assert!(
        matches!(Validated::new(serde_json::from_value(wrong).unwrap()), Err(Failure::Definition {path, ..}) if path == "identity.field")
    );
}

#[test]
fn identity_predicates_read_typed_values_and_retained_identity_is_immutable() {
    let mut source = document(
        json!({"type":"integer","required":true,"invariants":[{"assert":{"scalar_compare":{"left":"$bound.value","op":"gt","right":0}}}]}),
    );
    source["entity"]["invariants"] = json!([{"assert":{"scalar_compare":{"left":"$fields.identity","op":"gt","right":9007199254740992_i64}}}]);
    source["commands"]["change"]["outcomes"]["changed"]["effect"]["set"]
        .as_object_mut()
        .unwrap()
        .remove("identity");
    let runtime = checked(source);
    let valid = json!(9007199254740993_i64);
    let failure = runtime
        .decide(None, call("observe", &json!(-1), json!({})))
        .unwrap_err();
    assert!(
        matches!(failure, Failure::Core(CoreError::Validation(_))),
        "{failure:?}"
    );
    let failure = runtime
        .decide(
            None,
            call(
                "create",
                &json!(9007199254740992_i64),
                json!({"identity":9007199254740992_i64,"allow":true}),
            ),
        )
        .unwrap_err();
    assert!(
        matches!(failure, Failure::Core(CoreError::InvariantViolation { .. })),
        "{failure:?}"
    );
    let created = runtime
        .decide(
            None,
            call("create", &valid, json!({"identity":valid,"allow":true})),
        )
        .unwrap();
    let changed = runtime
        .decide(
            created.result.as_ref(),
            call("change", &valid, json!({"identity":valid,"count":2})),
        )
        .unwrap();
    assert_eq!(changed.result.as_ref().unwrap().fields["identity"], valid);
    assert_eq!(
        outcome::replay(&[created, changed.clone()]).unwrap(),
        changed.result
    );
}
