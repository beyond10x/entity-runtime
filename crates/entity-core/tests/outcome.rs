//! Independent vectors for the opt-in outcome contract and legacy-reader separation.

use entity_core::outcome::{self, Definition, Failure, Invocation, Record, Validated};
use entity_core::{CoreError, DecisionRecord, EntityDefinition};
use serde_json::{json, Value};

fn document() -> Value {
    json!({
        "format":"entity-outcome-definition/1",
        "entity": {
            "entity":"sample.item", "version":1,
            "schema":{"fields":{"count":{"type":"integer","required":true,"min":0}}},
            "lifecycle":{"initial":"open","states":["open","closed"]},
            "invariants":[{"name":"bounded","assert":{"lte":["$fields.count",100]}}]
        },
        "commands": {
            "create": {
                "arguments":{"fields":{"count":{"type":"integer","required":true}}},
                "observations":{"fields":{"failed":{"type":"boolean"}}},
                "outcomes": {
                    "accepted": {
                        "condition":{"condition":"when","predicate":{"gt":["$args.count",0]}},
                        "effect":{"effect":"create","set":{"count":"$args.count"},"emits":[
                            {"template":{"type":"Created","payload":{"count":"$fields.count","arg":"$args.count","literal":"$$id"}},
                             "schema":{"fields":{"count":{"type":"integer","required":true},"arg":{"type":"integer","required":true},"literal":{"type":"string","required":true}}}},
                            {"template":{"type":"Audited","payload":{}},"schema":{}}
                        ]}
                    },
                    "rejected": {
                        "condition":{"condition":"otherwise"},
                        "effect":{"effect":"refuse","error":"InvalidCount","payload":{"why":"nonpositive"},"schema":{"fields":{"why":{"type":"string","required":true}}}}
                    },
                    "unavailable": {
                        "condition":{"condition":"external","field":"failed"},
                        "effect":{"effect":"refuse","error":"Unavailable","payload":{},"schema":{}}
                    }
                }
            },
            "close": {
                "arguments":{"fields":{"count":{"type":"integer","required":true}}},"observations":{},
                "outcomes": {
                    "closed": {"condition":{"condition":"otherwise"},"effect":{"effect":"change",
                        "transitions":[{"from":"open","to":"closed"}],"set":{"count":"$args.count"},
                        "emits":[{"template":{"type":"Closed","payload":{"count":"$fields.count"}},"schema":{"fields":{"count":{"type":"integer","required":true}}}}]}},
                    "already_closed": {"condition":{"condition":"wrong_state","states":["closed"]},"effect":{"effect":"observe"}}
                }
            },
            "probe": {"arguments":{"fields":{"flag":{"type":"boolean"}}},"observations":{},
                "outcomes":{
                    "yes":{"condition":{"condition":"when","predicate":{"eq":["$args.flag",true]}},"effect":{"effect":"observe"}},
                    "no":{"condition":{"condition":"otherwise"},"effect":{"effect":"observe"}}
                }}
        }
    })
}
fn checked(value: Value) -> Validated {
    Validated::new(serde_json::from_value(value).unwrap()).unwrap()
}
fn call(command: &str, arguments: Value, observations: Value) -> Invocation {
    Invocation {
        command: command.into(),
        id: "item-a".into(),
        arguments: arguments.as_object().unwrap().clone(),
        observations: observations.as_object().unwrap().clone(),
    }
}
fn create(model: &Validated, count: i64) -> Record {
    model
        .decide(
            None,
            call("create", json!({"count":count}), json!({"failed":false})),
        )
        .unwrap()
}

#[test]
fn creation_materializes_every_typed_event_at_one_revision_and_replays() {
    let model = checked(document());
    let record = create(&model, 7);
    let result = record.result.as_ref().unwrap();
    assert_eq!(record.outcome, "accepted");
    assert_eq!(result.revision, 1);
    assert_eq!(result.lifecycle_state, "open");
    assert_eq!(
        result.fields,
        json!({"count":7}).as_object().unwrap().clone()
    );
    assert_eq!(
        record
            .events
            .iter()
            .map(|e| e.event_type.as_str())
            .collect::<Vec<_>>(),
        ["Created", "Audited"]
    );
    assert!(record
        .events
        .iter()
        .all(|e| e.revision == 1 && e.from_state.is_none() && e.id == "item-a"));
    assert_eq!(
        record.events[0].payload,
        json!({"count":7,"arg":7,"literal":"$id"})
    );
    assert_eq!(
        record.events[0].args,
        json!({"count":7}).as_object().unwrap().clone()
    );
    let bytes = serde_json::to_string(&record).unwrap();
    let decoded: Record = serde_json::from_str(&bytes).unwrap();
    assert_eq!(outcome::replay(&[decoded]).unwrap(), record.result);
    assert_eq!(serde_json::to_string(&create(&model, 7)).unwrap(), bytes);
}

#[test]
fn refused_creation_then_change_and_wrong_state_observation_keep_exact_revisions() {
    let model = checked(document());
    let refused = create(&model, 0);
    assert_eq!(refused.outcome, "rejected");
    assert_eq!(refused.result, None);
    assert!(refused.events.is_empty());
    assert_eq!(refused.error.as_ref().unwrap().name, "InvalidCount");
    assert_eq!(
        refused.error.as_ref().unwrap().payload,
        json!({"why":"nonpositive"})
    );
    let created = create(&model, 7);
    let closed = model
        .decide(
            created.result.as_ref(),
            call("close", json!({"count":8}), json!({})),
        )
        .unwrap();
    assert_eq!(closed.outcome, "closed");
    assert_eq!(closed.result.as_ref().unwrap().revision, 2);
    assert_eq!(closed.result.as_ref().unwrap().lifecycle_state, "closed");
    assert_eq!(closed.events[0].payload, json!({"count":8}));
    let observed = model
        .decide(
            closed.result.as_ref(),
            call("close", json!({"count":99}), json!({})),
        )
        .unwrap();
    assert_eq!(observed.outcome, "already_closed");
    assert_eq!(observed.result, closed.result);
    assert!(observed.events.is_empty());
    assert_eq!(
        outcome::replay(&[refused, created, closed, observed.clone()]).unwrap(),
        observed.result
    );
}

#[test]
fn unobserved_conditions_never_become_a_default_or_a_known_other_branch() {
    let model = checked(document());
    for count in [0, 7] {
        assert_eq!(
            model
                .decide(None, call("create", json!({"count":count}), json!({})))
                .unwrap_err(),
            Failure::Unobservable {
                outcomes: vec!["unavailable".into()],
                paths: vec!["observations.failed".into()]
            }
        );
    }
    assert_eq!(
        model
            .decide(None, call("probe", json!({}), json!({})))
            .unwrap_err(),
        Failure::Unobservable {
            outcomes: vec!["yes".into()],
            paths: vec!["$args.flag".into()]
        }
    );
    assert_eq!(
        model
            .decide(None, call("probe", json!({"flag":false}), json!({})))
            .unwrap()
            .outcome,
        "no"
    );
}

#[test]
fn external_observations_are_separate_and_have_no_implicit_priority() {
    let model = checked(document());
    assert_eq!(
        model
            .decide(
                None,
                call("create", json!({"count":7}), json!({"failed":true}))
            )
            .unwrap_err(),
        Failure::Ambiguous {
            outcomes: vec!["accepted".into(), "unavailable".into()]
        }
    );
    let external = model
        .decide(
            None,
            call("create", json!({"count":0}), json!({"failed":true})),
        )
        .unwrap();
    assert_eq!(external.outcome, "unavailable");
    assert_eq!(external.error.unwrap().name, "Unavailable");
    let error = model
        .decide(
            None,
            call(
                "create",
                json!({"count":0,"failed":true}),
                json!({"failed":false}),
            ),
        )
        .unwrap_err();
    assert!(
        matches!(error,Failure::Core(CoreError::Validation(ref errors)) if errors.iter().any(|e|e.path == "arguments.failed")),
        "{error:?}"
    );
}

#[test]
fn changing_branches_preserve_kernel_invariants_and_refuse_missing_or_wrong_instances() {
    let model = checked(document());
    let created = create(&model, 7);
    let before = created.result.as_ref().unwrap();
    let saved = before.clone();
    let error = model
        .decide(Some(before), call("close", json!({"count":101}), json!({})))
        .unwrap_err();
    assert!(
        matches!(error,Failure::Core(CoreError::InvariantViolation{ref rule,..}) if rule.as_deref() == Some("bounded")),
        "{error:?}"
    );
    assert_eq!(before, &saved);
    assert_eq!(
        model
            .decide(None, call("close", json!({"count":1}), json!({})))
            .unwrap_err(),
        Failure::InstanceRequired
    );
    assert_eq!(
        model
            .decide(
                Some(before),
                call("create", json!({"count":1}), json!({"failed":false}))
            )
            .unwrap_err(),
        Failure::InstanceAlreadyExists
    );
    let mut other = call("probe", json!({"flag":true}), json!({}));
    other.id = "item-b".into();
    assert_eq!(
        model.decide(Some(before), other).unwrap_err(),
        Failure::IdentityMismatch
    );
}

#[test]
fn every_materialized_payload_is_validated_before_a_record_escapes() {
    let mut source = document();
    source["commands"]["create"]["outcomes"]["accepted"]["effect"]["emits"][1]["schema"] =
        json!({"fields":{"required":{"type":"string","required":true}}});
    let model = checked(source);
    let error = model
        .decide(
            None,
            call("create", json!({"count":7}), json!({"failed":false})),
        )
        .unwrap_err();
    assert!(
        matches!(error,Failure::Core(CoreError::Validation(ref errors)) if errors.iter().any(|e|e.path == "events[1].payload.required")),
        "{error:?}"
    );
    let mut source = document();
    source["commands"]["create"]["outcomes"]["rejected"]["effect"]["payload"]["why"] = json!(12);
    let model = checked(source);
    let error = model
        .decide(
            None,
            call("create", json!({"count":0}), json!({"failed":false})),
        )
        .unwrap_err();
    assert!(
        matches!(error,Failure::Core(CoreError::Validation(ref errors)) if errors.iter().any(|e|e.path == "error.payload.why")),
        "{error:?}"
    );
}

#[test]
fn dormant_branches_and_illegal_creation_scopes_refuse_registration() {
    let cases = [
        (
            "/commands/create/outcomes/accepted/effect/set/count",
            json!("$fields.count"),
            "command input",
        ),
        (
            "/commands/create/outcomes/accepted/effect/emits/0/template/payload/count",
            json!("$old_fields.count"),
            "command creation",
        ),
        (
            "/commands/create/outcomes/accepted/condition/predicate",
            json!({"eq":["$state","open"]}),
            "command input",
        ),
        (
            "/commands/close/outcomes/closed/effect/transitions",
            json!([]),
            "transition",
        ),
        (
            "/commands/create/observations/fields/failed/default",
            json!(false),
            "without defaults",
        ),
        (
            "/commands/close/outcomes/already_closed/condition/states",
            json!(["absent"]),
            "declared states",
        ),
    ];
    for (path, value, reason) in cases {
        let mut source = document();
        if path.ends_with("/default") {
            source["commands"]["create"]["observations"]["fields"]["failed"]["default"] = value;
        } else {
            *source.pointer_mut(path).unwrap() = value;
        }
        let error = Validated::new(serde_json::from_value(source).unwrap()).unwrap_err();
        assert!(
            matches!(error,Failure::Definition{ref detail,..} if detail.contains(reason)),
            "{path}: {error:?}"
        );
    }
}

#[test]
fn replay_recomputes_selection_events_results_and_definition_identity() {
    let model = checked(document());
    let original = create(&model, 7);
    let mut tampered = original.clone();
    tampered.outcome = "rejected".into();
    assert!(matches!(
        outcome::replay(&[tampered]),
        Err(Failure::Replay { index: 0, .. })
    ));
    let mut tampered = original.clone();
    tampered.events.reverse();
    assert!(matches!(
        outcome::replay(&[tampered]),
        Err(Failure::Replay { index: 0, .. })
    ));
    let mut tampered = original.clone();
    tampered.result.as_mut().unwrap().revision = 2;
    assert!(matches!(
        outcome::replay(&[tampered]),
        Err(Failure::Replay { index: 0, .. })
    ));
    let observed = model
        .decide(
            original.result.as_ref(),
            call("probe", json!({"flag":true}), json!({})),
        )
        .unwrap();
    let mut tampered = observed.clone();
    tampered.definition.entity.version = 2;
    assert!(matches!(
        outcome::replay(&[original.clone(), tampered]),
        Err(Failure::Replay { index: 1, .. })
    ));
    let mut other = create(&model, 0);
    other.invocation.id = "other".into();
    assert!(matches!(
        outcome::replay(&[create(&model, 0), other]),
        Err(Failure::Replay { index: 1, .. })
    ));
    assert_eq!(
        outcome::replay(&[original, observed.clone()]).unwrap(),
        observed.result
    );
}

#[test]
fn new_formats_are_closed_and_old_readers_refuse_them() {
    let mut source = document();
    let error = serde_json::from_value::<EntityDefinition>(source.clone()).unwrap_err();
    assert!(error.to_string().contains("unknown field"), "{error}");
    source["format"] = json!("entity-outcome-definition/6");
    let error = serde_json::from_value::<Definition>(source).unwrap_err();
    assert!(error.to_string().contains("unknown variant"), "{error}");
    let record = serde_json::to_value(create(&checked(document()), 7)).unwrap();
    let error = serde_json::from_value::<DecisionRecord>(record.clone()).unwrap_err();
    assert!(error.to_string().contains("unknown field"), "{error}");
    for path in [
        "/result",
        "/events/0",
        "/invocation",
        "/definition/commands/probe/outcomes/no/effect",
    ] {
        let mut value = record.clone();
        value
            .pointer_mut(path)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("extra".into(), json!(true));
        let error = serde_json::from_value::<Record>(value).unwrap_err();
        assert!(
            error.to_string().contains("unknown field"),
            "{path}: {error}"
        );
    }
}
