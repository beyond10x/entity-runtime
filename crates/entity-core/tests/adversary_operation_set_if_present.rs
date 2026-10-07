//! Adversarial cases against an operation outcome's `set_if_present` (issue 54).
//!
//! Each case states the contract it holds the code to: the design page's registration rules
//! (`docs/design/service-binding-boundary-v0.1.md` § 2.1) and runtime meaning (§ 2.2).

use std::collections::{BTreeMap, BTreeSet};

use entity_core::{
    decide, decide_before_load, decide_create, replay, DefinitionError, EntityDefinition,
    EntityInstance, LoadedDecision, OperationFieldAction, PreloadDecision, ValidatedDefinition,
};
use serde_json::{json, Value};

fn definition(value: Value) -> EntityDefinition {
    serde_json::from_value(value).expect("the fixture is a definition document")
}

fn validated(value: Value) -> ValidatedDefinition {
    ValidatedDefinition::new(definition(value)).expect("the fixture registers")
}

/// A `service/2` memo whose `annotate` operation copies the optional leaf `bound.note` into the
/// optional field `note`.
fn memo_definition() -> Value {
    json!({
        "entity": "memo", "version": 1, "semantics": "service/2",
        "schema": { "fields": {
            "title": { "type": "string", "required": true },
            "note": { "type": "string" }
        }},
        "lifecycle": { "initial": "Open", "states": ["Open"] },
        "create": {
            "arguments": { "fields": {
                "title": { "type": "string", "required": true },
                "bound": { "type": "object", "required": true, "properties": {
                    "note": { "type": "string" }
                }}
            }},
            "outcomes": [{
                "name": "opened", "effect": "creates",
                "set": { "title": "$args.title" },
                "set_if_present": { "note": { "argument": "bound.note" } }
            }]
        },
        "operations": { "annotate": {
            "arguments": { "fields": {
                "bound": { "type": "object", "required": true, "properties": {
                    "note": { "type": "string" }
                }}
            }},
            "outcomes": [{
                "name": "annotated", "effect": "updates",
                "set_if_present": { "note": { "argument": "bound.note" } },
                "emits": [{ "type": "Annotated", "payload": { "after": "$fields" } }]
            }]
        }}
    })
}

fn memo(fields: Value) -> EntityInstance {
    EntityInstance {
        entity: "memo".to_owned(),
        version: 1,
        id: "m-1".to_owned(),
        lifecycle_state: "Open".to_owned(),
        revision: 1,
        fields: serde_json::from_value(fields).expect("the fields are an object"),
    }
}

/// Design § 2.1 rule 3: "every parent on the path is required, so exactly the leaf controls
/// presence"; rule 4 refuses a defaulted leaf because it "is always materialized after
/// normalization". A required parent whose declared default carries the leaf materializes the leaf
/// for every caller who omits the parent, so presence is no longer the caller's: on an operation
/// the field is overwritten although the caller sent no argument at all.
#[test]
fn a_parent_default_cannot_make_an_operation_set_if_present_leaf_present_for_a_caller_who_sent_none(
) {
    let mut document = memo_definition();
    document["operations"]["annotate"]["arguments"]["fields"]["bound"]["default"] =
        json!({"note": "from-default"});
    match ValidatedDefinition::new(definition(document)) {
        Err(defects) => assert!(
            defects.iter().any(|defect| matches!(
                defect,
                DefinitionError::ConditionalArgumentInvalid { argument, .. }
                    if argument == "bound.note"
            )),
            "refused, but not for the defaulted parent: {defects}"
        ),
        Ok(registered) => {
            let decision = decide(
                &registered,
                &memo(json!({"note": "kept", "title": "T"})),
                "annotate",
                json!({}),
            )
            .expect("the operation evaluates")
            .into_decision()
            .expect("the operation accepts");
            panic!(
                "registered a set_if_present leaf whose parent default makes it present: a caller \
                 that sent no argument had note 'kept' overwritten with {} (changed = {})",
                decision.instance.fields["note"],
                Value::Object(decision.record.changed),
            );
        }
    }
}

/// The same registration rule on a creation outcome, which the base commit already admitted: this
/// case separates what the unit introduced from what it inherited.
///
/// It asserts today's behaviour, which is the defect: the creation registers and a caller who sent
/// no `bound` gets `note` from the parent's default. Refusing it would refuse definitions an
/// earlier release admitted, so it is `story:creation-set-if-present-ignores-parent-defaults`, not
/// this unit. When that story lands this assertion flips to the refusal the operation case holds.
#[test]
fn a_parent_default_makes_a_creation_set_if_present_leaf_present_for_a_caller_who_sent_none_until_creation_set_if_present_ignores_parent_defaults(
) {
    let mut document = memo_definition();
    document["create"]["arguments"]["fields"]["bound"]["default"] = json!({"note": "from-default"});
    document["operations"]
        .as_object_mut()
        .expect("operations is an object")
        .clear();
    let registered = ValidatedDefinition::new(definition(document)).expect(
        "a creation set_if_present under a defaulted parent still registers until \
         story:creation-set-if-present-ignores-parent-defaults lands; then this flips to a refusal",
    );
    let decision = decide_create(&registered, "m-1".to_owned(), json!({"title": "T"}))
        .expect("the creation evaluates")
        .into_decision()
        .expect("the creation accepts");
    assert_eq!(
        decision.instance.fields.get("note"),
        Some(&json!("from-default")),
        "a caller that sent no `bound` gets note from the parent default until \
         story:creation-set-if-present-ignores-parent-defaults lands; then this assertion flips"
    );
}

/// CHANGELOG: "`kernel/1` and `service/1` still refuse the key." The unit's tests show
/// `service/1`; this is the `kernel/1` half.
#[test]
fn kernel_1_refuses_an_operation_outcome_carrying_set_if_present() {
    let document = json!({
        "entity": "memo", "version": 1,
        "schema": { "fields": { "note": { "type": "string" } } },
        "lifecycle": { "initial": "Open", "states": ["Open"] },
        "operations": { "annotate": {
            "arguments": { "fields": {
                "bound": { "type": "object", "required": true, "properties": {
                    "note": { "type": "string" }
                }}
            }},
            "transitions": [{ "from": "Open", "to": "Open" }],
            "outcomes": [{
                "name": "annotated", "effect": "updates",
                "set_if_present": { "note": { "argument": "bound.note" } }
            }]
        }}
    });
    let defects = ValidatedDefinition::new(definition(document)).unwrap_err();
    assert!(
        defects.iter().any(|defect| matches!(
            defect,
            DefinitionError::SemanticsKeyNotAvailable { path, key }
                if path == "operations.annotate.outcomes" && key == "outcomes"
        )),
        "{defects}"
    );
}

/// Design § 2.2: an operation's present leaf "inserts or replaces the field". The empty text is a
/// present value, not an absence.
#[test]
fn an_empty_text_argument_is_present_and_replaces_the_field() {
    let registered = validated(memo_definition());
    let decision = decide(
        &registered,
        &memo(json!({"note": "old", "title": "T"})),
        "annotate",
        json!({"bound": {"note": ""}}),
    )
    .unwrap()
    .into_decision()
    .unwrap();
    assert_eq!(decision.instance.fields["note"], json!(""));
    assert_eq!(
        Value::Object(decision.record.changed.clone()),
        json!({"note": ""})
    );
}

/// The pre-load route (`decide_before_load` then `continue_with`) is the one a host takes; it must
/// produce the record `decide` produces, byte for byte, present and absent.
#[test]
fn the_prepared_continuation_writes_the_bytes_decide_writes_for_an_operation_set_if_present() {
    let registered = validated(memo_definition());
    let subject = memo(json!({"note": "kept", "title": "T"}));
    for arguments in [json!({"bound": {"note": "new"}}), json!({"bound": {}})] {
        let direct = decide(&registered, &subject, "annotate", arguments.clone())
            .unwrap()
            .into_decision()
            .unwrap();
        let PreloadDecision::Load(prepared) =
            decide_before_load(&registered, "m-1", "annotate", arguments.clone()).unwrap()
        else {
            panic!("the operation needs its subject")
        };
        let continued = prepared
            .continue_with(&subject)
            .unwrap()
            .into_decision()
            .unwrap();
        assert_eq!(
            serde_json::to_vec(&continued.record).unwrap(),
            serde_json::to_vec(&direct.record).unwrap(),
            "{arguments}"
        );
    }
}

/// Under `service/3` a `Remove` action on one field and a present leaf on another produce one
/// result: the removal evidence names only the removed field, `changed` names only the written one,
/// and replay reproduces both.
#[test]
fn a_service_3_removal_beside_an_operation_set_if_present_records_one_change_and_one_removal() {
    let mut document = memo_definition();
    document["semantics"] = json!("service/3");
    document["schema"]["fields"]["stamp"] = json!({ "type": "string" });
    document["operations"]["annotate"]["outcomes"][0]["fulfills"] =
        json!({ "stamp": { "actions": "optional" } });
    let registered = validated(document);
    let created = decide_create(
        &registered,
        "m-1".to_owned(),
        json!({"title": "T", "bound": {}}),
    )
    .unwrap()
    .into_decision()
    .unwrap();
    // Give the subject a stamp to remove: the creation map cannot write it, so a first annotate
    // sets it through the host.
    let stamped = {
        let PreloadDecision::Load(prepared) =
            decide_before_load(&registered, "m-1", "annotate", json!({"bound": {}})).unwrap()
        else {
            panic!("the operation needs its subject")
        };
        let LoadedDecision::NeedsFulfillment(outcome) =
            prepared.select_with(&created.instance).unwrap()
        else {
            panic!("the outcome requests its stamp")
        };
        outcome
            .complete(BTreeMap::from([(
                "stamp".to_owned(),
                OperationFieldAction::Set {
                    value: json!("s-0"),
                },
            )]))
            .unwrap()
            .into_decision()
            .unwrap()
    };
    let PreloadDecision::Load(prepared) = decide_before_load(
        &registered,
        "m-1",
        "annotate",
        json!({"bound": {"note": "n"}}),
    )
    .unwrap() else {
        panic!("the operation needs its subject")
    };
    let LoadedDecision::NeedsFulfillment(outcome) =
        prepared.select_with(&stamped.instance).unwrap()
    else {
        panic!("the outcome requests its stamp")
    };
    let decision = outcome
        .complete(BTreeMap::from([(
            "stamp".to_owned(),
            OperationFieldAction::Remove,
        )]))
        .unwrap()
        .into_decision()
        .unwrap();
    assert_eq!(
        Value::Object(decision.instance.fields.clone()),
        json!({"note": "n", "title": "T"})
    );
    assert_eq!(
        Value::Object(decision.record.changed.clone()),
        json!({"note": "n"})
    );
    assert_eq!(
        decision.record.removed,
        BTreeSet::from(["stamp".to_owned()])
    );
    assert_eq!(decision.events[0].removed, decision.record.removed);
    assert_eq!(
        Value::Object(decision.events[0].changed.clone()),
        json!({"note": "n"})
    );
    assert_eq!(
        replay(&[created.record, stamped.record, decision.record]).unwrap(),
        decision.instance
    );
}
