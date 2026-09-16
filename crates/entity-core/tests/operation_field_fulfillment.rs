//! Complete `service/3` operation-field fulfillment and replay behavior.

use std::collections::{BTreeMap, BTreeSet};

use entity_core::{
    decide_before_load, replay, CoreError, DecisionCommand, DefinitionError, EntityDefinition,
    EntityInstance, Evaluation, LoadedDecision, OperationFieldAction, OperationFieldActions,
    PreloadDecision, ValidatedDefinition,
};
use serde_json::{json, Value};

fn document() -> Value {
    json!({
        "entity": "invoice", "version": 1, "semantics": "service/3",
        "identity": { "field": "invoice_id" },
        "schema": { "fields": {
            "invoice_id": { "type": "string", "required": true },
            "issued_at": { "type": "string", "required": true },
            "note": { "type": "string" }
        }},
        "lifecycle": { "initial": "Draft", "states": ["Draft", "Issued"] },
        "operations": { "Issue": {
            "arguments": { "fields": { "accept": { "type": "boolean", "required": true } } },
            "response": { "fields": { "issued_at": { "type": "string", "required": true } } },
            "outcomes": [
                { "name": "rejected", "when": { "eq": ["$args.accept", false] },
                  "refuses": { "error": "NotAccepted" } },
                { "name": "issued", "effect": { "moves": { "from": "Draft", "to": "Issued" } },
                  "fulfills": {
                    "issued_at": { "actions": "required" },
                    "note": { "actions": "optional" }
                  },
                  "emits": [{ "type": "InvoiceIssued", "payload": {
                    "issued_at": "$fields.issued_at"
                  }}],
                  "responds": { "issued_at": "$fields.issued_at" } }
            ]
        }}
    })
}

fn definition(value: Value) -> EntityDefinition {
    serde_json::from_value(value).expect("definition document parses")
}

fn validated() -> ValidatedDefinition {
    ValidatedDefinition::new(definition(document())).expect("service/3 definition validates")
}

fn draft(id: &str) -> EntityInstance {
    let identity = id.strip_prefix("s:").unwrap_or(id);
    EntityInstance {
        entity: "invoice".to_owned(),
        version: 1,
        id: id.to_owned(),
        lifecycle_state: "Draft".to_owned(),
        revision: 1,
        fields: serde_json::from_value(json!({
            "invoice_id": identity,
            "issued_at": "pending",
            "note": "remove me"
        }))
        .expect("fields are an object"),
    }
}

fn prepared<'a>(definition: &'a ValidatedDefinition) -> entity_core::PreparedOutcome<'a> {
    let PreloadDecision::Load(operation) =
        decide_before_load(definition, "s:inv-1", "Issue", json!({"accept": true}))
            .expect("input needs the subject")
    else {
        panic!("accepted input is not a pre-load refusal")
    };
    let LoadedDecision::NeedsFulfillment(prepared) = operation
        .select_with(&draft("s:inv-1"))
        .expect("selection succeeds")
    else {
        panic!("issued outcome advertises fulfillment")
    };
    prepared
}

#[test]
fn set_preserve_and_remove_apply_once_and_share_the_result_with_event_response_and_replay() {
    let validated_definition = validated();
    let created = entity_core::create(
        &validated_definition,
        "s:inv-1".to_owned(),
        Value::Object(draft("s:inv-1").fields),
    )
    .expect("branchless creation establishes replay genesis");
    let prepared = prepared(&validated_definition);
    assert_eq!(prepared.outcome(), "issued");
    assert_eq!(
        prepared
            .requirements()
            .iter()
            .map(|(name, requirement)| (name.as_str(), requirement.actions))
            .collect::<Vec<_>>(),
        vec![
            ("issued_at", OperationFieldActions::Required),
            ("note", OperationFieldActions::Optional),
        ]
    );

    let actions = BTreeMap::from([
        (
            "issued_at".to_owned(),
            OperationFieldAction::Set {
                value: json!("2026-09-16T10:00:00Z"),
            },
        ),
        ("note".to_owned(), OperationFieldAction::Remove),
    ]);
    let Evaluation::Accepted(decision) = prepared
        .complete(actions.clone())
        .expect("exact actions complete")
    else {
        panic!("issued accepts")
    };
    assert_eq!(
        decision.instance.fields["issued_at"],
        actions["issued_at"].set_value()
    );
    assert!(!decision.instance.fields.contains_key("note"));
    assert_eq!(
        decision.record.response.as_ref().unwrap()["issued_at"],
        json!("2026-09-16T10:00:00Z")
    );
    assert_eq!(
        decision.events[0].payload["issued_at"],
        json!("2026-09-16T10:00:00Z")
    );
    assert_eq!(decision.record.removed, BTreeSet::from(["note".to_owned()]));
    assert_eq!(decision.events[0].removed, decision.record.removed);
    assert!(!decision.record.changed.contains_key("note"));
    assert!(matches!(
        &decision.record.command,
        DecisionCommand::Execute { fulfillments, .. } if fulfillments == &actions
    ));
    let genesis = created.record;
    assert_eq!(
        replay(&[genesis.clone(), decision.record.clone()]).unwrap(),
        decision.instance
    );

    let mut tampered_action = decision.record.clone();
    let DecisionCommand::Execute { fulfillments, .. } = &mut tampered_action.command else {
        panic!("operation command")
    };
    fulfillments.insert("issued_at".to_owned(), OperationFieldAction::Preserve);
    assert!(replay(&[genesis.clone(), tampered_action]).is_err());

    let mut tampered_record_removal = decision.record.clone();
    tampered_record_removal.removed.clear();
    assert!(replay(&[genesis.clone(), tampered_record_removal]).is_err());

    let mut tampered_event_removal = decision.record.clone();
    tampered_event_removal.events[0].removed.clear();
    assert!(replay(&[genesis, tampered_event_removal]).is_err());
}

trait SetValue {
    fn set_value(&self) -> Value;
}

impl SetValue for OperationFieldAction {
    fn set_value(&self) -> Value {
        match self {
            OperationFieldAction::Set { value } => value.clone(),
            _ => panic!("expected set"),
        }
    }
}

#[test]
fn preserve_keeps_loaded_presence_while_every_wrong_action_coordinate_is_typed() {
    let definition = validated();
    let actions = BTreeMap::from([
        ("issued_at".to_owned(), OperationFieldAction::Preserve),
        ("note".to_owned(), OperationFieldAction::Preserve),
    ]);
    let Evaluation::Accepted(decision) = prepared(&definition).complete(actions).unwrap() else {
        panic!("issued accepts")
    };
    assert_eq!(decision.instance.fields["issued_at"], json!("pending"));
    assert_eq!(decision.instance.fields["note"], json!("remove me"));
    assert!(decision.record.removed.is_empty());

    let error = prepared(&definition)
        .complete(BTreeMap::from([(
            "issued_at".to_owned(),
            OperationFieldAction::Preserve,
        )]))
        .unwrap_err();
    assert!(matches!(
        error,
        CoreError::FulfillmentKeysMismatch { missing, extra, .. }
            if missing == vec!["note"] && extra.is_empty()
    ));

    let error = prepared(&definition)
        .complete(BTreeMap::from([
            ("issued_at".to_owned(), OperationFieldAction::Preserve),
            ("note".to_owned(), OperationFieldAction::Preserve),
            ("extra".to_owned(), OperationFieldAction::Preserve),
        ]))
        .unwrap_err();
    assert!(matches!(
        error,
        CoreError::FulfillmentKeysMismatch { missing, extra, .. }
            if missing.is_empty() && extra == vec!["extra"]
    ));

    let error = prepared(&definition)
        .complete(BTreeMap::from([
            ("issued_at".to_owned(), OperationFieldAction::Remove),
            ("note".to_owned(), OperationFieldAction::Preserve),
        ]))
        .unwrap_err();
    assert!(matches!(error, CoreError::RequiredFieldRemoval { field, .. } if field == "issued_at"));

    let error = prepared(&definition)
        .complete(BTreeMap::from([
            (
                "issued_at".to_owned(),
                OperationFieldAction::Set { value: json!(7) },
            ),
            ("note".to_owned(), OperationFieldAction::Preserve),
        ]))
        .unwrap_err();
    assert!(matches!(
        error,
        CoreError::Validation(errors)
            if errors[0].path == "fulfillments.issued_at.value"
    ));
}

#[test]
fn selection_refusal_preconditions_and_exact_subject_finish_before_any_fulfillment() {
    let validated_definition = validated();
    let prepare = |id: &str| {
        let PreloadDecision::Load(operation) =
            decide_before_load(&validated_definition, id, "Issue", json!({"accept": true}))
                .unwrap()
        else {
            panic!("subject facts are needed")
        };
        operation
    };
    let PreloadDecision::Refused(refusal) = decide_before_load(
        &validated_definition,
        "s:inv-1",
        "Issue",
        json!({"accept": false}),
    )
    .unwrap() else {
        panic!("refusal completes before load")
    };
    assert_eq!(refusal.error, "NotAccepted");
    assert!(matches!(
        prepare("s:inv-1").continue_with(&draft("s:inv-1")),
        Err(CoreError::FulfillmentRequired { fields, .. })
            if fields == vec!["issued_at", "note"]
    ));
    assert!(matches!(
        prepare("s:inv-1").select_with(&draft("s:other")),
        Err(CoreError::SubjectMismatch { .. })
    ));

    let mut with_precondition = document();
    with_precondition["operations"]["Issue"]["preconditions"] =
        json!([{ "name": "never", "assert": false }]);
    let definition = ValidatedDefinition::new(definition(with_precondition)).unwrap();
    let PreloadDecision::Load(operation) =
        decide_before_load(&definition, "s:inv-1", "Issue", json!({"accept": true})).unwrap()
    else {
        panic!("subject is loaded before preconditions")
    };
    assert!(matches!(
        operation.select_with(&draft("s:inv-1")),
        Err(CoreError::PreconditionFailed { .. })
    ));
}

#[test]
fn update_and_move_outcomes_record_removal_with_zero_one_and_many_events() {
    for (effect, event_count, expected_state) in [
        (json!("updates"), 0_usize, "Draft"),
        (
            json!({"moves": {"from": "Draft", "to": "Issued"}}),
            1,
            "Issued",
        ),
        (json!("updates"), 3, "Draft"),
    ] {
        let mut value = document();
        value["operations"]["Issue"]["outcomes"][1]["effect"] = effect;
        value["operations"]["Issue"]["outcomes"][1]["emits"] = Value::Array(
            (0..event_count)
                .map(|index| {
                    json!({
                        "type": format!("InvoiceIssued{index}"),
                        "payload": {"issued_at": "$fields.issued_at"}
                    })
                })
                .collect(),
        );
        let definition = ValidatedDefinition::new(definition(value)).unwrap();
        let PreloadDecision::Load(operation) =
            decide_before_load(&definition, "s:inv-1", "Issue", json!({"accept": true})).unwrap()
        else {
            panic!("subject is required")
        };
        let LoadedDecision::NeedsFulfillment(prepared) =
            operation.select_with(&draft("s:inv-1")).unwrap()
        else {
            panic!("accepted outcome requires actions")
        };
        let Evaluation::Accepted(decision) = prepared
            .complete(BTreeMap::from([
                (
                    "issued_at".to_owned(),
                    OperationFieldAction::Set {
                        value: json!("now"),
                    },
                ),
                ("note".to_owned(), OperationFieldAction::Remove),
            ]))
            .unwrap()
        else {
            panic!("accepted")
        };
        assert_eq!(decision.instance.lifecycle_state, expected_state);
        assert_eq!(decision.events.len(), event_count);
        assert_eq!(decision.record.removed, BTreeSet::from(["note".to_owned()]));
        assert!(decision
            .events
            .iter()
            .all(|event| event.removed == decision.record.removed));
    }
}

#[test]
fn invariants_judge_the_post_action_map_and_event_only_service_3_history_is_refused() {
    let mut value = document();
    value["invariants"] = json!([{
        "name": "issued_at_is_admitted",
        "assert": {"ne": ["$fields.issued_at", "forbidden"]}
    }]);
    let definition = ValidatedDefinition::new(definition(value)).unwrap();
    let PreloadDecision::Load(operation) =
        decide_before_load(&definition, "s:inv-1", "Issue", json!({"accept": true})).unwrap()
    else {
        panic!("subject is required")
    };
    let LoadedDecision::NeedsFulfillment(prepared) =
        operation.select_with(&draft("s:inv-1")).unwrap()
    else {
        panic!("accepted outcome requires actions")
    };
    assert!(matches!(
        prepared.complete(BTreeMap::from([
            (
                "issued_at".to_owned(),
                OperationFieldAction::Set {
                    value: json!("forbidden")
                },
            ),
            ("note".to_owned(), OperationFieldAction::Preserve),
        ])),
        Err(CoreError::InvariantViolation { .. })
    ));
    let error = entity_core::rehydrate(&definition, &[]).unwrap_err();
    assert!(error.to_string().contains("service/3"), "{error}");
}

#[test]
fn registration_accumulates_every_fulfillment_placement_and_presence_defect() {
    let mut wrong = document();
    wrong["semantics"] = json!("service/2");
    wrong["create"]["outcomes"] = json!([{
        "name": "created", "effect": "creates",
        "fulfills": { "note": { "actions": "optional" } }
    }]);
    wrong["operations"]["Issue"]["outcomes"][1]["set"] = json!({"note": "fixed"});
    wrong["operations"]["Issue"]["outcomes"][1]["fulfills"] = json!({
        "invoice_id": { "actions": "required" },
        "issued_at": { "actions": "optional" },
        "note": { "actions": "optional" },
        "unknown": { "actions": "optional" }
    });
    wrong["operations"]["Issue"]["outcomes"][0]["fulfills"] = json!({
        "note": { "actions": "optional" }
    });
    let defects = ValidatedDefinition::new(definition(wrong)).unwrap_err();
    let kinds = defects
        .iter()
        .map(DefinitionError::kind)
        .collect::<BTreeSet<_>>();
    for expected in [
        "semantics_key_not_available",
        "fulfillment_on_create",
        "fulfillment_identity_field",
        "fulfillment_presence_mismatch",
        "fulfillment_set_conflict",
        "fulfillment_field_unknown",
        "refusal_mutates_state",
    ] {
        assert!(kinds.contains(expected), "missing {expected}: {defects}");
    }
}
