//! The pre-load and typed-presence boundary used by service bindings.

use entity_core::{
    decide, decide_before_load, replay, CoreError, DefinitionError, EntityDefinition,
    EntityInstance, Evaluation, PreloadDecision, ValidatedDefinition,
};
use serde_json::{json, Value};

fn definition(value: Value) -> EntityDefinition {
    serde_json::from_value(value).expect("the fixture is a definition document")
}

fn validated(value: Value) -> ValidatedDefinition {
    ValidatedDefinition::new(definition(value)).expect("the fixture registers")
}

fn instance(entity: &str, id: &str, state: &str) -> EntityInstance {
    EntityInstance {
        entity: entity.to_owned(),
        version: 1,
        id: id.to_owned(),
        lifecycle_state: state.to_owned(),
        revision: 1,
        fields: serde_json::Map::new(),
    }
}

fn payment_definition() -> Value {
    json!({
        "entity": "invoice", "version": 1, "semantics": "service/1",
        "schema": { "fields": {} },
        "lifecycle": { "initial": "Draft", "states": ["Draft", "Issued", "Paid", "Cancelled"] },
        "operations": { "PayInvoice": {
            "arguments": { "fields": { "amount": { "type": "integer", "required": true } } },
            "outcomes": [
                { "name": "settled", "when": { "gt": ["$args.amount", 0] },
                  "effect": { "moves": { "from": "Issued", "to": "Paid" } },
                  "emits": [{ "type": "InvoicePaid", "payload": { "amount": "$args.amount" } }] },
                { "name": "rejected", "refuses": { "error": "InvalidAmount" } },
                { "name": "wrong-state", "wrong_state": true,
                  "refuses": { "error": "InvoiceStateConflict" } }
            ]
        }}
    })
}

fn presence_definition() -> Value {
    json!({
        "entity": "invoice", "version": 1, "semantics": "service/2",
        "schema": { "fields": {
            "fixed": { "type": "string", "required": true },
            "note": { "type": "string" }
        }},
        "lifecycle": { "initial": "Draft", "states": ["Draft"] },
        "create": {
            "arguments": { "fields": {
                "fixed": { "type": "string", "required": true },
                "bound": { "type": "object", "required": true, "properties": {
                    "note": { "type": "string" }
                }}
            }},
            "response": { "fields": {
                "fixed": { "type": "string", "required": true },
                "note": { "type": "string" }
            }},
            "outcomes": [{
                "name": "accepted", "effect": "creates",
                "set": { "fixed": "$args.fixed" },
                "set_if_present": { "note": { "argument": "bound.note" } },
                "emits": [{ "type": "InvoiceCreated", "payload": { "fixed": "$fields.fixed" },
                    "payload_if_present": { "note": { "argument": "bound.note" } } }],
                "responds": { "fixed": "$fields.fixed" },
                "responds_if_present": { "note": { "argument": "bound.note" } }
            }]
        }
    })
}

fn refusal(evaluation: PreloadDecision<'_>) -> entity_core::Refusal {
    match evaluation {
        PreloadDecision::Refused(refusal) => refusal,
        PreloadDecision::Load(_) => panic!("expected a complete pre-load refusal"),
    }
}

#[test]
fn a_nonpositive_payment_refuses_before_any_subject_load() {
    let definition = validated(payment_definition());
    for id in ["known", "unknown"] {
        for amount in [0, -1] {
            let refused = refusal(
                decide_before_load(&definition, id, "PayInvoice", json!({"amount": amount}))
                    .expect("the guard is answerable from input"),
            );
            assert_eq!(refused.outcome, "rejected");
            assert_eq!(refused.error, "InvalidAmount");
        }
    }
}

#[test]
fn a_positive_payment_requires_the_exact_subject_and_continues_in_er() {
    let definition = validated(payment_definition());
    let PreloadDecision::Load(prepared) =
        decide_before_load(&definition, "known", "PayInvoice", json!({"amount": 10})).unwrap()
    else {
        panic!("positive input needs the invoice")
    };
    assert_eq!(prepared.subject().entity(), "invoice");
    assert_eq!(prepared.subject().version(), 1);
    assert_eq!(prepared.subject().id(), "known");
    assert!(matches!(
        prepared
            .continue_with(&instance("invoice", "known", "Issued"))
            .unwrap(),
        Evaluation::Accepted(_)
    ));

    for state in ["Draft", "Paid", "Cancelled"] {
        let PreloadDecision::Load(prepared) =
            decide_before_load(&definition, "known", "PayInvoice", json!({"amount": 10})).unwrap()
        else {
            panic!("positive input needs the invoice")
        };
        let Evaluation::Refused(refused) = prepared
            .continue_with(&instance("invoice", "known", state))
            .expect("wrong-state is a named branch")
        else {
            panic!("wrong state refuses")
        };
        assert_eq!(refused.outcome, "wrong-state");
    }
}

#[test]
fn a_preload_scan_stops_at_the_first_subject_dependent_branch() {
    let definition = validated(json!({
        "entity": "probe", "version": 1, "semantics": "service/1",
        "schema": { "fields": { "flag": { "type": "boolean" } } },
        "lifecycle": { "initial": "Held", "states": ["Held"] },
        "operations": { "Check": { "outcomes": [
            { "name": "subject", "when": { "eq": ["$fields.flag", true] },
              "refuses": { "error": "Subject" } },
            { "name": "default", "refuses": { "error": "Default" } }
        ]}}
    }));
    assert!(matches!(
        decide_before_load(&definition, "p-1", "Check", json!({})).unwrap(),
        PreloadDecision::Load(_)
    ));
}

#[test]
fn implicit_kernel_and_branchless_service_operations_preload_then_match_direct_decide() {
    for semantics in [None, Some("service/1"), Some("service/2")] {
        let mut document = json!({
            "entity": "probe", "version": 1,
            "schema": { "fields": {} },
            "lifecycle": { "initial": "Held", "states": ["Held", "Other"] },
            "operations": { "Touch": { "transitions": [{ "from": "Held", "to": "Held" }] } }
        });
        if let Some(semantics) = semantics {
            document["semantics"] = json!(semantics);
        }
        let definition = validated(document);
        let held = instance("probe", "p-1", "Held");
        let direct = decide(&definition, &held, "Touch", json!({})).unwrap();
        let PreloadDecision::Load(prepared) =
            decide_before_load(&definition, "p-1", "Touch", json!({})).unwrap()
        else {
            panic!("implicit operation needs subject state")
        };
        let continued = prepared.continue_with(&held).unwrap();
        assert_eq!(continued, direct);
        let (Evaluation::Accepted(left), Evaluation::Accepted(right)) = (continued, direct) else {
            panic!("touch accepts")
        };
        assert_eq!(
            serde_json::to_vec(&left.record).unwrap(),
            serde_json::to_vec(&right.record).unwrap()
        );

        let other = instance("probe", "p-1", "Other");
        assert!(matches!(
            decide(&definition, &other, "Touch", json!({})),
            Err(CoreError::InvalidTransition { .. })
        ));
        let PreloadDecision::Load(prepared) =
            decide_before_load(&definition, "p-1", "Touch", json!({})).unwrap()
        else {
            panic!("implicit operation needs subject state")
        };
        assert!(matches!(
            prepared.continue_with(&other),
            Err(CoreError::InvalidTransition { .. })
        ));

        let mut precondition_document = serde_json::to_value(definition.as_definition()).unwrap();
        precondition_document["operations"]["Touch"]["preconditions"] =
            json!([{ "name": "never", "assert": false }]);
        let with_precondition = validated(precondition_document);
        let PreloadDecision::Load(prepared) =
            decide_before_load(&with_precondition, "p-1", "Touch", json!({})).unwrap()
        else {
            panic!("implicit operation needs subject facts before preconditions")
        };
        assert!(matches!(
            decide(&with_precondition, &held, "Touch", json!({})),
            Err(CoreError::PreconditionFailed { .. })
        ));
        assert!(matches!(
            prepared.continue_with(&held),
            Err(CoreError::PreconditionFailed { .. })
        ));
    }
}

fn guarded_definition(guard: Value) -> ValidatedDefinition {
    validated(json!({
        "entity": "probe", "version": 1, "semantics": "service/1",
        "schema": { "fields": { "note": { "type": "string" } } },
        "lifecycle": { "initial": "Held", "states": ["Held"] },
        "operations": { "Check": { "outcomes": [
            { "name": "first", "when": guard, "refuses": { "error": "First" } },
            { "name": "later", "refuses": { "error": "Later" } }
        ]}}
    }))
}

#[test]
fn kleene_dominating_guards_refuse_without_loading_an_unknown_subject() {
    for guard in [
        json!({"any": [true, {"eq": ["$fields.note", "x"]}]}),
        json!({"any": [{"eq": ["$fields.note", "x"]}, true]}),
    ] {
        assert_eq!(
            refusal(
                decide_before_load(&guarded_definition(guard), "missing", "Check", json!({}))
                    .unwrap()
            )
            .outcome,
            "first"
        );
    }
    for guard in [
        json!({"all": [false, {"eq": ["$fields.note", "x"]}]}),
        json!({"all": [{"eq": ["$fields.note", "x"]}, false]}),
    ] {
        assert_eq!(
            refusal(
                decide_before_load(&guarded_definition(guard), "missing", "Check", json!({}))
                    .unwrap()
            )
            .outcome,
            "later"
        );
    }
    assert_eq!(
        refusal(
            decide_before_load(
                &guarded_definition(json!({"not": false})),
                "missing",
                "Check",
                json!({})
            )
            .unwrap()
        )
        .outcome,
        "first"
    );
    assert!(matches!(
        decide_before_load(
            &guarded_definition(json!({"not": {"eq": ["$fields.note", "x"]}})),
            "missing",
            "Check",
            json!({})
        )
        .unwrap(),
        PreloadDecision::Load(_)
    ));
}

#[test]
fn preload_quantifiers_preserve_empty_collections_and_available_binders() {
    let definition = validated(json!({
        "entity": "probe", "version": 1, "semantics": "service/1",
        "schema": { "fields": { "note": { "type": "string" } } },
        "lifecycle": { "initial": "Held", "states": ["Held"] },
        "operations": { "Check": {
            "arguments": { "fields": { "items": { "type": "array", "required": true,
                "items": { "type": "integer" } } } },
            "outcomes": [
                { "name": "all", "when": { "for_all": { "in": "$args.items", "as": "item",
                    "that": { "any": [{ "gt": ["$item", 0] }, { "eq": ["$fields.note", "x"] }] } } },
                  "refuses": { "error": "All" } },
                { "name": "later", "refuses": { "error": "Later" } }
            ]
        }}
    }));
    for items in [json!([]), json!([1])] {
        assert_eq!(
            refusal(
                decide_before_load(&definition, "p-1", "Check", json!({"items": items})).unwrap()
            )
            .outcome,
            "all"
        );
    }
    assert!(matches!(
        decide_before_load(&definition, "p-1", "Check", json!({"items": [0]})).unwrap(),
        PreloadDecision::Load(_)
    ));

    let nested = validated(json!({
        "entity": "probe", "version": 1, "semantics": "service/1",
        "schema": { "fields": { "note": { "type": "string" } } },
        "lifecycle": { "initial": "Held", "states": ["Held"] },
        "operations": { "Check": {
            "arguments": { "fields": { "groups": { "type": "array", "required": true,
                "items": { "type": "array", "required": true, "items": { "type": "integer" } }
            } } },
            "outcomes": [
                { "name": "nested", "when": { "for_all": { "in": "$args.groups", "as": "group",
                    "that": { "for_any": { "in": "$group", "as": "item",
                        "that": { "any": [{ "gt": ["$item", 0] },
                                            { "eq": ["$fields.note", "x"] }] } } } } },
                  "refuses": { "error": "Nested" } },
                { "name": "later", "refuses": { "error": "Later" } }
            ]
        }}
    }));
    for groups in [json!([]), json!([[1], [2]])] {
        assert_eq!(
            refusal(
                decide_before_load(&nested, "p-1", "Check", json!({"groups": groups})).unwrap()
            )
            .outcome,
            "nested"
        );
    }

    let unknown_and_subject = validated(json!({
        "entity": "probe", "version": 1, "semantics": "service/1",
        "schema": { "fields": { "note": { "type": "string" } } },
        "lifecycle": { "initial": "Held", "states": ["Held"] },
        "operations": { "Check": {
            "arguments": { "fields": {
                "left": { "type": "string", "required": true },
                "right": { "type": "string", "required": true }
            } },
            "outcomes": [
                { "name": "combined", "when": { "any": [
                    { "compare": { "left": "$args.left", "op": "lt", "right": "$args.right" } },
                    { "eq": ["$fields.note", "x"] }
                ] }, "refuses": { "error": "Combined" } },
                { "name": "later", "refuses": { "error": "Later" } }
            ]
        }}
    }));
    let PreloadDecision::Load(prepared) = decide_before_load(
        &unknown_and_subject,
        "p-1",
        "Check",
        json!({"left": "a", "right": "b"}),
    )
    .unwrap() else {
        panic!("known unknown combined with a subject dependency still needs the subject")
    };
    assert!(matches!(
        prepared.continue_with(&instance("probe", "p-1", "Held")),
        Err(CoreError::OutcomeUnobservable { outcome, .. }) if outcome == "combined"
    ));
}

#[test]
fn unloaded_subject_existence_is_not_confused_with_an_absent_field() {
    assert!(matches!(
        decide_before_load(
            &guarded_definition(json!({"exists": "$fields.note"})),
            "missing",
            "Check",
            json!({})
        )
        .unwrap(),
        PreloadDecision::Load(_)
    ));
}

#[test]
fn a_state_guard_is_not_evaluated_before_the_subject_is_loaded() {
    let definition = validated(json!({
        "entity": "probe", "version": 1, "semantics": "service/1",
        "schema": { "fields": {} },
        "lifecycle": { "initial": "Held", "states": ["Held", "Other"] },
        "operations": { "Check": { "outcomes": [
            { "name": "guarded", "in_state": "Held", "when": false, "refuses": { "error": "Guarded" } },
            { "name": "later", "refuses": { "error": "Later" } }
        ]}}
    }));
    assert!(matches!(
        decide_before_load(&definition, "p-1", "Check", json!({})).unwrap(),
        PreloadDecision::Load(_)
    ));
}

#[test]
fn an_unknown_input_guard_refuses_without_trying_a_later_default() {
    let definition = validated(json!({
        "entity": "probe", "version": 1, "semantics": "service/1",
        "schema": { "fields": {} },
        "lifecycle": { "initial": "Held", "states": ["Held"] },
        "operations": { "Check": {
            "arguments": { "fields": { "input": { "type": "string" } } },
            "outcomes": [
                { "name": "unknown", "when": { "eq": ["$args.input", "x"] }, "refuses": { "error": "Unknown" } },
                { "name": "later", "refuses": { "error": "Later" } }
            ]
        }}
    }));
    assert!(matches!(
        decide_before_load(&definition, "p-1", "Check", json!({})),
        Err(CoreError::OutcomeUnobservable { outcome, unresolved, .. })
            if outcome == "unknown" && unresolved == vec!["$args.input"]
    ));
}

#[test]
fn a_prepared_operation_binds_definition_input_operation_and_identity() {
    let definition = validated(payment_definition());
    let prepare = |id: &str| {
        let PreloadDecision::Load(prepared) =
            decide_before_load(&definition, id, "PayInvoice", json!({"amount": 10})).unwrap()
        else {
            panic!("positive input needs a subject")
        };
        prepared
    };
    assert!(matches!(
        prepare("i-1").continue_with(&instance("other", "i-1", "Issued")),
        Err(CoreError::EntityMismatch { .. })
    ));
    assert!(matches!(
        prepare("i-1").continue_with(&instance("invoice", "i-2", "Missing")),
        Err(CoreError::SubjectMismatch { expected_id, actual_id, .. })
            if expected_id == "i-1" && actual_id == "i-2"
    ));
    assert!(matches!(
        prepare("i-1").continue_with(&instance("invoice", "i-1", "Missing")),
        Err(CoreError::UnknownState { .. })
    ));

    let subject = instance("invoice", "i-1", "Issued");
    let direct = decide(&definition, &subject, "PayInvoice", json!({"amount": 10})).unwrap();
    let prepared = prepare("i-1");
    assert_eq!(
        Value::Object(prepared.normalized_arguments().clone()),
        json!({"amount": 10})
    );
    assert_eq!(prepared.continue_with(&subject).unwrap(), direct);
}

#[test]
fn conditional_presence_distinguishes_absent_present_and_null_in_all_output_positions() {
    let presence = validated(presence_definition());
    let absent = entity_core::decide_create(
        &presence,
        "i-1".to_owned(),
        json!({"fixed": "fixed", "bound": {}}),
    )
    .unwrap()
    .into_decision()
    .unwrap();
    assert!(!absent.instance.fields.contains_key("note"));
    assert!(!absent.events[0]
        .payload
        .as_object()
        .unwrap()
        .contains_key("note"));
    assert!(!absent
        .record
        .response
        .as_ref()
        .unwrap()
        .contains_key("note"));

    let present = entity_core::decide_create(
        &presence,
        "i-2".to_owned(),
        json!({"fixed": "fixed", "bound": {"note": "hello"}}),
    )
    .unwrap()
    .into_decision()
    .unwrap();
    assert_eq!(present.instance.fields["note"], json!("hello"));
    assert_eq!(present.events[0].payload["note"], json!("hello"));
    assert_eq!(
        present.record.response.as_ref().unwrap()["note"],
        json!("hello")
    );

    for value in [Value::Null, json!(7)] {
        assert!(matches!(
            entity_core::decide_create(&presence, "i-3".to_owned(), json!({"fixed": "fixed", "bound": {"note": value}})),
            Err(CoreError::Validation(errors)) if errors.iter().any(|error| error.path == "arguments.bound.note")
        ));
    }

    let mut json_presence = presence_definition();
    json_presence["schema"]["fields"]["note"]["type"] = json!("json");
    json_presence["create"]["arguments"]["fields"]["bound"]["properties"]["note"]["type"] =
        json!("json");
    json_presence["create"]["response"]["fields"]["note"]["type"] = json!("json");
    let json_presence = validated(json_presence);
    let absent_json = entity_core::decide_create(
        &json_presence,
        "j-1".to_owned(),
        json!({"fixed": "fixed", "bound": {}}),
    )
    .unwrap()
    .into_decision()
    .unwrap();
    assert!(!absent_json.instance.fields.contains_key("note"));
    assert!(!absent_json.events[0]
        .payload
        .as_object()
        .unwrap()
        .contains_key("note"));
    assert!(!absent_json
        .record
        .response
        .as_ref()
        .unwrap()
        .contains_key("note"));
    let present_null = entity_core::decide_create(
        &json_presence,
        "j-2".to_owned(),
        json!({"fixed": "fixed", "bound": {"note": null}}),
    )
    .unwrap()
    .into_decision()
    .unwrap();
    assert_eq!(present_null.instance.fields["note"], Value::Null);
    assert_eq!(present_null.events[0].payload["note"], Value::Null);
    assert_eq!(
        present_null.record.response.as_ref().unwrap()["note"],
        Value::Null
    );
}

#[test]
fn one_optional_argument_reuses_one_presence_and_value_across_state_event_and_response() {
    let presence = validated(presence_definition());
    let decision = entity_core::decide_create(
        &presence,
        "i-1".to_owned(),
        json!({"fixed": "fixed", "bound": {"note": "same"}}),
    )
    .unwrap()
    .into_decision()
    .unwrap();
    assert_eq!(
        decision.instance.fields["note"],
        decision.events[0].payload["note"]
    );
    assert_eq!(
        decision.events[0].payload["note"],
        decision.record.response.as_ref().unwrap()["note"]
    );
}

#[test]
fn conditional_presence_refuses_wrong_types_and_missing_required_neighbors_before_selection() {
    let presence = validated(presence_definition());
    for input in [json!({"bound": {}}), json!({"fixed": "fixed"})] {
        assert!(matches!(
            entity_core::decide_create(&presence, "i-1".to_owned(), input),
            Err(CoreError::Validation(_))
        ));
    }
    let mut wrong_semantics = presence_definition();
    wrong_semantics["semantics"] = json!("service/1");
    let defects = ValidatedDefinition::new(definition(wrong_semantics)).unwrap_err();
    assert!(defects.iter().any(|defect| matches!(
        defect, DefinitionError::SemanticsKeyNotAvailable { key, .. } if key == "set_if_present"
    )));

    let mut required_leaf = presence_definition();
    required_leaf["create"]["arguments"]["fields"]["bound"]["properties"]["note"]["required"] =
        json!(true);
    let defects = ValidatedDefinition::new(definition(required_leaf)).unwrap_err();
    assert!(defects.iter().any(|defect| matches!(
        defect, DefinitionError::ConditionalArgumentInvalid { argument, .. } if argument == "bound.note"
    )));

    for bad_path in ["", "$args.bound.note", "bound..note", "bound.0"] {
        let mut document = presence_definition();
        document["create"]["outcomes"][0]["set_if_present"]["note"]["argument"] = json!(bad_path);
        let defects = ValidatedDefinition::new(definition(document)).unwrap_err();
        assert!(defects.iter().any(|defect| matches!(
            defect, DefinitionError::ConditionalArgumentInvalid { argument, .. } if argument == bad_path
        )));
    }

    for mutate in [
        "optional_parent",
        "defaulted_leaf",
        "open_parent",
        "open_root",
        "non_object_parent",
    ] {
        let mut document = presence_definition();
        match mutate {
            "optional_parent" => {
                document["create"]["arguments"]["fields"]["bound"]["required"] = json!(false)
            }
            "defaulted_leaf" => {
                document["create"]["arguments"]["fields"]["bound"]["properties"]["note"]
                    ["default"] = json!("default")
            }
            "open_parent" => {
                document["create"]["arguments"]["fields"]["bound"]["additional_properties"] =
                    json!(true)
            }
            "open_root" => document["create"]["arguments"]["additional_fields"] = json!(true),
            "non_object_parent" => {
                document["create"]["arguments"]["fields"]["bound"]["type"] = json!("array");
                document["create"]["arguments"]["fields"]["bound"]["items"] =
                    json!({"type": "string"});
                document["create"]["arguments"]["fields"]["bound"]
                    .as_object_mut()
                    .unwrap()
                    .remove("properties");
            }
            _ => unreachable!(),
        }
        let defects = ValidatedDefinition::new(definition(document)).unwrap_err();
        assert!(defects.iter().any(|defect| matches!(
            defect, DefinitionError::ConditionalArgumentInvalid { argument, .. } if argument == "bound.note"
        )), "{mutate}: {defects}");
    }

    for mutate in ["unknown", "required", "defaulted", "different_type"] {
        let mut document = presence_definition();
        match mutate {
            "unknown" => {
                document["create"]["outcomes"][0]["set_if_present"] =
                    json!({"unknown": {"argument": "bound.note"}})
            }
            "required" => document["schema"]["fields"]["note"]["required"] = json!(true),
            "defaulted" => document["schema"]["fields"]["note"]["default"] = json!("default"),
            "different_type" => document["schema"]["fields"]["note"]["max_length"] = json!(3),
            _ => unreachable!(),
        }
        let defects = ValidatedDefinition::new(definition(document)).unwrap_err();
        assert!(
            defects
                .iter()
                .any(|defect| matches!(defect, DefinitionError::ConditionalTargetInvalid { .. })),
            "{mutate}: {defects}"
        );
    }

    for map in ["set", "responds", "payload"] {
        let mut document = presence_definition();
        match map {
            "set" => document["create"]["outcomes"][0]["set"]["note"] = json!("$args.bound.note"),
            "responds" => {
                document["create"]["outcomes"][0]["responds"]["note"] = json!("$args.bound.note")
            }
            "payload" => {
                document["create"]["outcomes"][0]["emits"][0]["payload"]["note"] =
                    json!("$args.bound.note")
            }
            _ => unreachable!(),
        }
        let defects = ValidatedDefinition::new(definition(document)).unwrap_err();
        assert!(
            defects.iter().any(|defect| matches!(
                defect, DefinitionError::ConditionalTargetConflict { field, .. } if field == "note"
            )),
            "{map}: {defects}"
        );
    }

    let mut non_object_payload = presence_definition();
    non_object_payload["create"]["outcomes"][0]["emits"][0]["payload"] = json!("literal");
    let defects = ValidatedDefinition::new(definition(non_object_payload)).unwrap_err();
    assert!(defects.iter().any(|defect| matches!(
        defect, DefinitionError::ConditionalTargetInvalid { message, .. }
            if message.contains("must be an object")
    )));

    let mut operation_set = presence_definition();
    operation_set["operations"]["Update"] = json!({
        "arguments": operation_set["create"]["arguments"].clone(),
        "outcomes": [{
            "name": "updated",
            "set_if_present": {"note": {"argument": "bound.note"}}
        }]
    });
    let defects = ValidatedDefinition::new(definition(operation_set)).unwrap_err();
    assert!(defects.iter().any(|defect| matches!(
        defect, DefinitionError::ConditionalSetOnOperation { operation, field, .. }
            if operation == "Update" && field == "note"
    )));

    let mut refusing = presence_definition();
    refusing["create"]["outcomes"][0]["refuses"] = json!({"error": "No"});
    let defects = ValidatedDefinition::new(definition(refusing)).unwrap_err();
    assert!(defects
        .iter()
        .any(|defect| matches!(defect, DefinitionError::RefusalMutatesState { .. })));
}

#[test]
fn service_2_replay_recomputes_presence_and_refuses_every_tampered_position() {
    let definition = validated(presence_definition());
    let decision = entity_core::decide_create(
        &definition,
        "i-1".to_owned(),
        json!({"fixed": "fixed", "bound": {"note": "same"}}),
    )
    .unwrap()
    .into_decision()
    .unwrap();
    assert_eq!(
        decision.instance.fields["note"],
        decision.events[0].payload["note"]
    );
    assert_eq!(
        decision.events[0].payload["note"],
        decision.record.response.as_ref().unwrap()["note"]
    );
    assert_eq!(
        replay(std::slice::from_ref(&decision.record)).unwrap(),
        decision.instance
    );

    let mut records = Vec::new();
    let mut arguments = decision.record.clone();
    if let entity_core::DecisionCommand::Create { arguments, .. } = &mut arguments.command {
        arguments.get_mut("bound").unwrap()["note"] = json!("other");
    }
    records.push(arguments);
    let mut fields = decision.record.clone();
    fields
        .result
        .fields
        .insert("note".to_owned(), json!("other"));
    records.push(fields);
    let mut event = decision.record.clone();
    event.events[0].payload["note"] = json!("other");
    records.push(event);
    let mut response = decision.record.clone();
    response
        .response
        .as_mut()
        .unwrap()
        .insert("note".to_owned(), json!("other"));
    records.push(response);
    for record in records {
        assert!(matches!(replay(&[record]), Err(CoreError::Validation(_))));
    }
}

#[test]
fn kernel_1_and_service_1_keep_their_definitions_decisions_and_refusal_order() {
    let kernel = validated(json!({
        "entity": "probe", "version": 1,
        "schema": { "fields": {} },
        "lifecycle": { "initial": "Held", "states": ["Held"] },
        "operations": { "Touch": { "transitions": [{ "from": "Held", "to": "Held" }] } }
    }));
    assert!(!serde_json::to_value(kernel.as_definition())
        .unwrap()
        .as_object()
        .unwrap()
        .contains_key("semantics"));
    assert!(matches!(
        decide(
            &kernel,
            &instance("probe", "p-1", "Missing"),
            "Missing",
            json!({})
        ),
        Err(CoreError::UnknownState { .. })
    ));

    let service = validated(payment_definition());
    assert_eq!(
        serde_json::to_value(service.as_definition()).unwrap()["semantics"],
        json!("service/1")
    );
    assert_eq!(
        refusal(
            decide_before_load(&service, "unknown", "PayInvoice", json!({"amount": 0})).unwrap()
        )
        .outcome,
        "rejected"
    );
}
