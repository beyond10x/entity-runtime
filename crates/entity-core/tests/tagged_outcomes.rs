//! Adjacent tags select typed payloads through real command decisions and replay.

use entity_core::outcome::{self, Definition, Failure, Invocation, Record, Validated};
use entity_core::{CoreError, EntityDefinition, ValidatedDefinition};
use serde_json::{json, Value};

fn union(variants: Value) -> Value {
    json!({"type":"union", "required":true, "union":{
        "tag":"kind", "content":"value", "variants":variants
    }})
}

fn parameter() -> Value {
    union(json!({
        "input":{"type":"object","properties":{"name":{"type":"string","required":true}}},
        "binding":{"type":"object","properties":{"name":{"type":"string","required":true}}},
        "constant":{"type":"object","properties":{"value":{"type":"string","required":true}}}
    }))
}

fn document(field: Value) -> Value {
    let shape = json!({"fields":{"choice":field}});
    let event = json!({"template":{"type":"Stored","payload":"$fields"},"schema":shape});
    json!({
        "format":"entity-outcome-definition/3",
        "entity":{"entity":"sample.union", "schema":shape,"lifecycle":{"initial":"open","states":["open"]}},
        "commands":{
            "create":{"arguments":shape,"observations":{},"outcomes":{
                "created":{"condition":{"condition":"otherwise"},"effect":{
                    "effect":"create","set":{"choice":"$args.choice"},"emits":[event]}}
            }},
            "replace":{"arguments":shape,"observations":{},"outcomes":{
                "changed":{"condition":{"condition":"otherwise"},"effect":{
                    "effect":"change","transitions":[{"from":"open","to":"open"}],
                    "set":{"choice":"$args.choice"},"emits":[event]}}
            }},
            "reject":{"arguments":shape,"observations":{},"outcomes":{
                "refused":{"condition":{"condition":"otherwise"},"effect":{
                    "effect":"refuse","error":"Rejected","payload":{"choice":"$args.choice"},"schema":shape}}
            }}
        }
    })
}

fn model(document: Value) -> Validated {
    Validated::new(serde_json::from_value(document).unwrap()).unwrap()
}

fn call(command: &str, choice: Value) -> Invocation {
    Invocation {
        command: command.into(),
        id: "union-a".into(),
        arguments: json!({"choice":choice}).as_object().unwrap().clone(),
        observations: Default::default(),
    }
}

#[test]
fn tags_select_closed_payloads_and_errors_accumulate_without_a_decision() {
    let model = model(document(parameter()));
    for value in [
        json!({"kind":"input","value":{"name":"$id"}}),
        json!({"kind":"binding","value":{"name":"project"}}),
        json!({"kind":"constant","value":{"value":"literal"}}),
    ] {
        let record = model.decide(None, call("create", value.clone())).unwrap();
        assert_eq!(record.result.unwrap().fields["choice"], value);
        assert_eq!(record.events[0].payload["choice"], value);
    }
    for (value, paths) in [
        (
            json!({}),
            vec!["arguments.choice.kind", "arguments.choice.value"],
        ),
        (json!({"kind":7,"value":{}}), vec!["arguments.choice.kind"]),
        (
            json!({"kind":"missing","value":{}}),
            vec!["arguments.choice.kind"],
        ),
        (
            json!({"kind":"input","value":{"value":"wrong"},"extra":true}),
            vec![
                "arguments.choice.extra",
                "arguments.choice.value.name",
                "arguments.choice.value.value",
            ],
        ),
        (
            json!({"kind":"constant","value":null}),
            vec!["arguments.choice.value"],
        ),
        (json!(null), vec!["arguments.choice"]),
    ] {
        let Failure::Core(CoreError::Validation(errors)) =
            model.decide(None, call("create", value)).unwrap_err()
        else {
            panic!("invalid union must return accumulated validation errors");
        };
        let mut actual: Vec<_> = errors.iter().map(|error| error.path.as_str()).collect();
        actual.sort_unstable();
        assert_eq!(actual, paths);
    }
}

#[test]
fn defaults_follow_only_the_selected_payload_and_keep_explicit_null_and_missing_distinct() {
    let nested = union(json!({
        "data":{"type":"object","properties":{"count":{"type":"integer","required":true,"default":7}}},
        "empty":{"type":"nullable","items":{"type":"string"}}
    }));
    let field = union(json!({"batch":{"type":"array","items":{
        "type":"map","items":nested
    }}}));
    let model = model(document(field));
    let value = json!({"kind":"batch","value":[{
        "a.b":{"kind":"data","value":{}}, "null":{"kind":"empty","value":null}
    }]});
    let record = model.decide(None, call("create", value)).unwrap();
    let expected = json!({"kind":"batch","value":[{
        "a.b":{"kind":"data","value":{"count":7}}, "null":{"kind":"empty","value":null}
    }]});
    assert_eq!(record.invocation.arguments["choice"], expected);
    assert_eq!(record.result.as_ref().unwrap().fields["choice"], expected);
    let missing = json!({"kind":"batch","value":[{"null":{"kind":"empty"}}]});
    let Failure::Core(CoreError::Validation(errors)) =
        model.decide(None, call("create", missing)).unwrap_err()
    else {
        panic!("nullable payload still requires its envelope property");
    };
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].path, "arguments.choice.value[0][\"null\"].value");
}

#[test]
fn every_dormant_variant_and_the_envelope_are_validated_at_registration() {
    for (field, expected) in [
        (json!({"type":"union"}), "must declare 'union'"),
        (
            json!({"type":"union","union":{"tag":"kind","content":"kind","variants":{"a":{"type":"string"}}}}),
            "distinct property names",
        ),
        (
            json!({"type":"union","union":{"tag":"","content":"value","variants":{"a":{"type":"string"}}}}),
            "nonempty distinct",
        ),
        (union(json!({})), "at least one variant"),
        (
            union(json!({" ":{"type":"string"}})),
            "variant name cannot be blank",
        ),
        (
            union(json!({"valid":{"type":"string"},"dormant":{"type":"array"}})),
            "must declare 'items'",
        ),
        (
            json!({"type":"string","union":{"tag":"kind","content":"value","variants":{"a":{"type":"string"}}}}),
            "union",
        ),
    ] {
        let Failure::Definition { detail, .. } =
            Validated::new(serde_json::from_value(document(field)).unwrap()).unwrap_err()
        else {
            panic!("malformed union must be a definition failure");
        };
        assert!(detail.contains(expected), "{detail}");
    }
    let mut field = parameter();
    field["default"] = json!({"kind":"missing","value":{}});
    let Failure::Definition { detail, .. } =
        Validated::new(serde_json::from_value(document(field)).unwrap()).unwrap_err()
    else {
        panic!("union defaults must be validated");
    };
    assert!(
        detail.contains("invalid default") && detail.contains("missing"),
        "{detail}"
    );
}

#[test]
fn tag_references_and_common_payload_paths_stay_checked_in_quantified_bodies() {
    let field = union(json!({
        "left":{"type":"array","items":{"type":"integer"}},
        "right":{"type":"array","items":{"type":"integer"}}
    }));
    let mut source = document(field);
    source["commands"]["create"]["outcomes"]["accepted"] =
        source["commands"]["create"]["outcomes"]["created"].take();
    source["commands"]["create"]["outcomes"]
        .as_object_mut()
        .unwrap()
        .remove("created");
    source["commands"]["create"]["outcomes"]["accepted"]["condition"] = json!({"condition":"when","predicate":{"all":[
        {"eq":["$args.choice.kind","left"]},
        {"forall":{"over":"$args.choice.value","bind":"item","body":{"gt":["$bound.item",0]}}}
    ]}});
    source["commands"]["create"]["outcomes"]["rejected"] =
        source["commands"]["reject"]["outcomes"]["refused"].clone();
    let accepted = model(source.clone())
        .decide(None, call("create", json!({"kind":"left","value":[1,2]})))
        .unwrap();
    assert_eq!(accepted.outcome, "accepted");
    let rejected = model(source.clone())
        .decide(None, call("create", json!({"kind":"right","value":[1,2]})))
        .unwrap();
    assert_eq!(rejected.outcome, "rejected");
    assert_eq!(rejected.result, None);
    for reference in ["$args.choice.undeclared", "$args.choice.kind.nested"] {
        let mut bad = source.clone();
        bad["commands"]["create"]["outcomes"]["accepted"]["condition"] =
            json!({"condition":"when","predicate":{"exists":reference}});
        let Failure::Definition { detail, .. } =
            Validated::new(serde_json::from_value(bad).unwrap()).unwrap_err()
        else {
            panic!("unknown reference admitted")
        };
        assert!(detail.contains("union"), "{detail}");
    }
    let mut bad = source;
    bad["commands"]["create"]["arguments"]["fields"]["choice"]["union"]["variants"]["right"] =
        json!({"type":"string"});
    let Failure::Definition { detail, .. } =
        Validated::new(serde_json::from_value(bad).unwrap()).unwrap_err()
    else {
        panic!("mixed payload collection admitted")
    };
    assert!(detail.contains("not a typed collection"), "{detail}");
}

#[test]
fn typed_events_refusals_and_replay_preserve_selected_union_values() {
    let model = model(document(parameter()));
    let created = model
        .decide(
            None,
            call("create", json!({"kind":"input","value":{"name":"project"}})),
        )
        .unwrap();
    let changed = model
        .decide(
            created.result.as_ref(),
            call(
                "replace",
                json!({"kind":"constant","value":{"value":"text"}}),
            ),
        )
        .unwrap();
    let refused = model
        .decide(
            changed.result.as_ref(),
            call(
                "reject",
                json!({"kind":"binding","value":{"name":"credential"}}),
            ),
        )
        .unwrap();
    assert_eq!(refused.result, changed.result);
    assert_eq!(
        refused.error.as_ref().unwrap().payload["choice"]["kind"],
        "binding"
    );
    assert!(refused.events.is_empty());
    let records: Vec<Record> = serde_json::from_value(json!([created, changed, refused])).unwrap();
    assert_eq!(outcome::replay(&records).unwrap(), records[2].result);
    assert_eq!(
        serde_json::to_value(&records[0]).unwrap()["format"],
        "entity-outcome-record/3"
    );
    for index in 0..3 {
        let mut tampered = records.clone();
        tampered[index].invocation.arguments["choice"]["kind"] = json!("other");
        assert!(matches!(
            outcome::replay(&tampered),
            Err(Failure::Core(CoreError::Validation(_)))
        ));
    }
    let mut tampered = records;
    tampered[1].events[0].payload["choice"]["kind"] = json!("input");
    assert!(matches!(
        outcome::replay(&tampered),
        Err(Failure::Replay { index: 1, .. })
    ));

    for (command, path) in [
        ("create", "events[0].payload.choice.kind"),
        ("reject", "error.payload.choice.kind"),
    ] {
        let mut source = document(parameter());
        let outcomes = &mut source["commands"][command]["outcomes"];
        let effect = if command == "create" {
            &mut outcomes["created"]["effect"]
        } else {
            &mut outcomes["refused"]["effect"]
        };
        if command == "create" {
            effect["emits"][0]["template"]["payload"] =
                json!({"choice":{"kind":"invalid","value":{}}});
        } else {
            effect["payload"] = json!({"choice":{"kind":"invalid","value":{}}});
        }
        let Failure::Core(CoreError::Validation(errors)) = model_with_bad_payload(source, command)
        else {
            panic!("typed materialized payload admitted")
        };
        assert_eq!(errors[0].path, path);
    }
}

fn model_with_bad_payload(source: Value, command: &str) -> Failure {
    model(source)
        .decide(
            None,
            call(command, json!({"kind":"input","value":{"name":"project"}})),
        )
        .unwrap_err()
}

#[test]
fn old_profiles_refuse_unions_and_closed_readers_refuse_unknown_union_fields() {
    for version in [1, 2, 3] {
        let mut source = document(json!({"type":"string", "union":null}));
        source["format"] = json!(format!("entity-outcome-definition/{version}"));
        let error = serde_json::from_value::<Definition>(source).unwrap_err();
        assert!(error.to_string().contains("invalid type: null"), "{error}");
    }
    for version in [1, 2] {
        let mut source = document(parameter());
        source["format"] = json!(format!("entity-outcome-definition/{version}"));
        let Failure::Definition { detail, .. } =
            Validated::new(serde_json::from_value(source).unwrap()).unwrap_err()
        else {
            panic!("union admitted under old profile")
        };
        assert!(detail.contains("outcome profile 3"), "{detail}");
    }
    let legacy: EntityDefinition =
        serde_json::from_value(document(parameter())["entity"].clone()).unwrap();
    assert!(ValidatedDefinition::new(legacy)
        .unwrap_err()
        .to_string()
        .contains("outcome profile 3"));
    let mut source = document(parameter());
    source["entity"]["schema"]["fields"]["choice"]["union"]["fallback"] = json!("input");
    let error = serde_json::from_value::<Definition>(source).unwrap_err();
    assert!(
        error.to_string().contains("unknown field `fallback`"),
        "{error}"
    );
}
