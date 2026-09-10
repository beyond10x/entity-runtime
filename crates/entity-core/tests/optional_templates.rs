//! Expected absence/null/value shapes are authored independently of template materialization.
use entity_core::{
    outcome::{self, Definition, Failure, Invocation, Record, Validated},
    CoreError,
};
use serde_json::{json, Value};

fn document() -> Value {
    let optional = json!({"type":"nullable","items":{"type":"string"}});
    let identity = json!({"type":"string","required":true});
    let data = json!({"type":"object","required":true,"properties":{
        "note":optional,"from_parent":optional,
        "list":{"type":"array","required":true,"items":{"type":"object","properties":{"note":optional}}},
        "literal":{"type":"string","required":true}
    }});
    let arguments = json!({"fields":{"identity":identity,"note":optional,"allow":{"type":"boolean","required":true},
        "parent":{"type":"nullable","items":{"type":"object","properties":{"note":optional}}}
    }});
    let set = json!({"identity":"$args.identity","note":"$optional.args.note","data":{
        "note":"$optional.args.note","from_parent":"$optional.args.parent.note",
        "list":[{"note":"$optional.args.note"}],"literal":"$$optional.args.note"
    }});
    let events = json!([
        {"template":{"type":"Changed","payload":{"note":"$optional.fields.note","data":"$fields.data"}},"schema":{"fields":{"note":optional,"data":data}}},
        {"template":{"type":"Audited","payload":{}},"schema":{}}
    ]);
    json!({"format":"entity-outcome-definition/8","identity":{"field":"identity"},
        "entity":{"entity":"optional.item","schema":{"fields":{"identity":identity,"note":optional,"data":data}},"lifecycle":{"initial":"open","states":["open"]}},
        "commands":{
            "create":{"arguments":arguments,"observations":{},"outcomes":{
                "created":{"condition":{"condition":"when","predicate":{"eq":["$args.allow",true]}},"effect":{"effect":"create","set":set,"emits":events}},
                "refused":{"condition":{"condition":"otherwise"},"effect":{"effect":"refuse","error":"Declined","payload":{"note":"$optional.args.note"},"schema":{"fields":{"note":optional}}}}
            }},
            "change":{"arguments":arguments,"observations":{},"outcomes":{"changed":{"condition":{"condition":"otherwise"},"effect":{"effect":"change","transitions":[{"from":"open","to":"open"}],"set":set,"emits":events}}}}
        }
    })
}

fn checked(value: Value) -> Validated {
    Validated::new(serde_json::from_value(value).unwrap()).unwrap()
}

fn call(command: &str, extra: Value) -> Invocation {
    let Value::Object(mut arguments) = extra else {
        panic!("fixture arguments must be objects");
    };
    arguments.insert("identity".into(), json!("one"));
    arguments.entry("allow").or_insert(json!(true));
    Invocation {
        command: command.into(),
        id: outcome::identity_key(&json!("one")),
        arguments,
        observations: serde_json::Map::new(),
    }
}

fn roundtrip(records: &[Record]) -> Vec<Record> {
    serde_json::from_slice(&serde_json::to_vec(records).unwrap()).unwrap()
}

#[test]
fn optional_templates_preserve_absent_null_and_value_through_effects_and_replay() {
    let program = checked(document());
    let vectors = [
        (
            json!({"note":"value","parent":{"note":"nested"}}),
            json!({"identity":"one","note":"value","data":{"note":"value","from_parent":"nested","list":[{"note":"value"}],"literal":"$optional.args.note"}}),
        ),
        (
            json!({"note":null,"parent":{"note":null}}),
            json!({"identity":"one","note":null,"data":{"note":null,"from_parent":null,"list":[{"note":null}],"literal":"$optional.args.note"}}),
        ),
        (
            json!({"parent":null}),
            json!({"identity":"one","data":{"list":[{}],"literal":"$optional.args.note"}}),
        ),
        (
            json!({"parent":{}}),
            json!({"identity":"one","data":{"list":[{}],"literal":"$optional.args.note"}}),
        ),
        (
            json!({}),
            json!({"identity":"one","data":{"list":[{}],"literal":"$optional.args.note"}}),
        ),
        (
            json!({"note":"$optional.args.note"}),
            json!({"identity":"one","note":"$optional.args.note","data":{"note":"$optional.args.note","list":[{"note":"$optional.args.note"}],"literal":"$optional.args.note"}}),
        ),
    ];
    let mut history = Vec::new();
    for (index, (arguments, expected)) in vectors.into_iter().enumerate() {
        let before = history
            .last()
            .and_then(|record: &Record| record.result.as_ref());
        let record = program
            .decide(
                before,
                call(if index == 0 { "create" } else { "change" }, arguments),
            )
            .unwrap();
        let state = record.result.as_ref().unwrap();
        assert_eq!(Value::Object(state.fields.clone()), expected);
        assert_eq!(state.revision, u64::try_from(index).unwrap() + 1);
        assert_eq!(
            record
                .events
                .iter()
                .map(|e| e.event_type.as_str())
                .collect::<Vec<_>>(),
            ["Changed", "Audited"]
        );
        assert_eq!(record.events[0].payload.get("note"), expected.get("note"));
        assert_eq!(record.events[0].payload["data"], expected["data"]);
        assert!(record
            .events
            .iter()
            .all(|event| event.revision == state.revision));
        history.push(record);
    }
    assert_eq!(
        outcome::replay(&roundtrip(&history)).unwrap(),
        history.last().unwrap().result
    );
    let mut tampered = history;
    tampered[2]
        .result
        .as_mut()
        .unwrap()
        .fields
        .insert("note".into(), Value::Null);
    assert!(matches!(
        outcome::replay(&tampered),
        Err(Failure::Replay { .. })
    ));
}

#[test]
fn optional_error_properties_do_not_hide_required_outputs_or_strict_reference_failures() {
    let program = checked(document());
    for (arguments, payload) in [
        (json!({"allow":false}), json!({})),
        (json!({"allow":false,"note":null}), json!({"note":null})),
        (json!({"allow":false,"note":"why"}), json!({"note":"why"})),
    ] {
        let refused = program.decide(None, call("create", arguments)).unwrap();
        assert_eq!(refused.error.as_ref().unwrap().payload, payload);
        assert_eq!(refused.result, None);
        assert!(refused.events.is_empty());
        assert_eq!(outcome::replay(&roundtrip(&[refused])).unwrap(), None);
    }
    let mut required = document();
    required["entity"]["schema"]["fields"]["note"]["required"] = json!(true);
    let program = checked(required);
    let created = program
        .decide(None, call("create", json!({"note":"kept"})))
        .unwrap();
    let snapshot = created.result.clone();
    assert!(matches!(
        program.decide(created.result.as_ref(), call("change", json!({}))),
        Err(Failure::Core(CoreError::Validation(_)))
    ));
    assert_eq!(created.result, snapshot);
    let mut strict = document();
    strict["commands"]["create"]["outcomes"]["created"]["effect"]["set"]["note"] =
        json!("$args.note");
    assert!(matches!(
        checked(strict).decide(None, call("create", json!({}))),
        Err(Failure::Core(CoreError::Template { .. }))
    ));
    let mut required_error = document();
    required_error["commands"]["create"]["outcomes"]["refused"]["effect"]["schema"]["fields"]
        ["note"]["required"] = json!(true);
    assert!(matches!(
        checked(required_error).decide(None, call("create", json!({"allow":false}))),
        Err(Failure::Core(CoreError::Validation(_)))
    ));
}

#[test]
fn optional_references_require_profile_eight_declared_paths_and_object_property_positions() {
    for reference in [
        "$optional.args.missing",
        "$optional.observations.note",
        "$optional.optional.args.note",
        "$optional.bound.missing",
    ] {
        let mut source = document();
        source["commands"]["create"]["outcomes"]["created"]["effect"]["set"]["note"] =
            json!(reference);
        assert!(
            matches!(
                Validated::new(serde_json::from_value(source).unwrap()),
                Err(Failure::Definition { .. })
            ),
            "{reference}"
        );
    }
    for payload in [json!("$optional.args.note"), json!(["$optional.args.note"])] {
        let mut source = document();
        source["commands"]["create"]["outcomes"]["refused"]["effect"]["payload"] = payload;
        assert!(matches!(
            Validated::new(serde_json::from_value(source).unwrap()),
            Err(Failure::Definition { .. })
        ));
    }
    for profile in 1..=7 {
        let mut source = document();
        source["format"] = json!(format!("entity-outcome-definition/{profile}"));
        if profile < 7 {
            source.as_object_mut().unwrap().remove("identity");
        }
        // These older shapes are deliberately reduced to their admitted string vocabulary.
        source["entity"]["schema"] =
            json!({"fields":{"identity":{"type":"string","required":true}}});
        source["commands"] = json!({"create":{"arguments":{"fields":{"note":{"type":"string"}}},"observations":{},"outcomes":{"created":{"condition":{"condition":"otherwise"},"effect":{"effect":"create","set":{"identity":"fixed","note":"$optional.args.note"},"emits":[]}}}}});
        source["entity"]["schema"]["fields"]["note"] = json!({"type":"string"});
        assert!(
            matches!(Validated::new(serde_json::from_value(source).unwrap()),Err(Failure::Definition {detail,..}) if detail.contains("profile 8")),
            "{profile}"
        );
    }
    let mut source = document();
    source.as_object_mut().unwrap().remove("identity");
    assert!(
        matches!(Validated::new(serde_json::from_value::<Definition>(source).unwrap()),Err(Failure::Definition {path,..}) if path == "identity")
    );
}
