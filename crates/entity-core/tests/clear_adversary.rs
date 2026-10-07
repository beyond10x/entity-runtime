//! Adversarial cases for `story:set-clears-an-optional-field` (R-165).
//!
//! Each case asserts something the unit's own documents state, or a combination the unit's suite
//! leaves unexercised. None of them changes an implementation file.

use std::collections::BTreeMap;

use entity_core::{
    rehydrate, replay, DefinitionError, DomainEvent, EntityDefinition, LoadedDecision,
    OperationFieldAction, PreloadDecision, Registry, Runtime, ValidatedDefinition,
};
use serde_json::{json, Value};

fn definition(document: Value) -> Result<ValidatedDefinition, Vec<DefinitionError>> {
    let definition: EntityDefinition =
        serde_json::from_value(document).expect("a definition document");
    ValidatedDefinition::new(definition).map_err(|errors| errors.as_slice().to_vec())
}

fn registry(document: Value) -> Registry {
    let mut registry = Registry::new();
    registry
        .register(serde_json::from_value(document).expect("a definition document"))
        .expect("registers");
    registry
}

fn text(value: &impl serde::Serialize) -> String {
    serde_json::to_string(value).expect("serializes")
}

fn repository_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates/")
        .parent()
        .expect("repository root")
        .to_path_buf()
}

// --- Contract drift: the compatibility rule the unit wrote --------------------------------------

/// `service-binding-boundary-v0.1.md` § 3, the rule this unit added, as narrowed after the first
/// adversary pass: only a `set` value that is a mapping made only of assignment keywords changes
/// meaning. A mapping of `cleared` and `increment` on a `json` field was an object template before
/// this unit and wrote that object; it is now two assignments of one field and is refused at
/// registration. A mapping with any other key is still the object template it was.
#[test]
fn a_mapping_of_both_assignment_keywords_that_wrote_an_object_is_now_refused_as_set_assignment_conflict(
) {
    let boundary = std::fs::read_to_string(
        repository_root().join("docs/design/service-binding-boundary-v0.1.md"),
    )
    .expect("the boundary design");
    let boundary = boundary.split_whitespace().collect::<Vec<_>>().join(" ");
    for stated in [
        "Every definition none of whose `set` values is a mapping made only of assignment keywords \
         (`increment`, `cleared`) decides the same bytes as before.",
        "a mapping of both keywords, which wrote that object into a `json` or `object` field \
         before, is now refused at registration as `SetAssignmentConflict`.",
    ] {
        assert!(
            boundary.contains(stated),
            "the rule this case holds the code to is still stated: {stated}"
        );
    }

    let probe = |value: Value| {
        json!({
            "entity": "probe",
            "version": 1,
            "schema": { "fields": { "extra": { "type": "json" } } },
            "lifecycle": { "initial": "open", "states": ["open"] },
            "operations": { "tag": {
                "transitions": [{ "from": "open", "to": "open" }],
                "set": { "extra": value }
            }}
        })
    };
    let refused = definition(probe(json!({ "cleared": true, "increment": 1 })))
        .expect_err("two assignments of one field");
    assert!(
        matches!(refused.as_slice(), [DefinitionError::SetAssignmentConflict { path, field, keywords }]
            if path == "operations.tag.set.extra"
                && field == "extra"
                && keywords == &["cleared".to_owned(), "increment".to_owned()]),
        "{refused:?}"
    );

    // One key that is not a keyword keeps the mapping the object template it was.
    let registry = registry(probe(json!({ "cleared": true, "increment": 1, "by": 2 })));
    let runtime = Runtime::new(&registry);
    let created = runtime
        .create("probe", 1, "p-1", json!({}))
        .expect("created");
    let tagged = runtime
        .execute(&created.instance, "tag", json!({}))
        .expect("the template writes");
    assert_eq!(
        text(&tagged.instance.fields),
        r#"{"extra":{"by":2,"cleared":true,"increment":1}}"#
    );
}

// --- Combinations the unit's suite does not exercise --------------------------------------------

/// A `service/3` outcome that clears one field in `set` and removes another through a fulfillment
/// names both in `removed`, on the record and on every event, and replays to the same bytes.
#[test]
fn a_clear_and_a_fulfillment_remove_on_one_outcome_are_both_named_and_replay() {
    let document = json!({
        "entity": "invoice", "version": 1, "semantics": "service/3",
        "schema": { "fields": {
            "issued_at": { "type": "string", "required": true },
            "note": { "type": "string" },
            "draft": { "type": "string" }
        }},
        "lifecycle": { "initial": "Draft", "states": ["Draft"] },
        "operations": { "Issue": { "outcomes": [{
            "name": "issued", "effect": "updates",
            "set": { "draft": { "cleared": true } },
            "fulfills": {
                "issued_at": { "actions": "required" },
                "note": { "actions": "optional" }
            },
            "emits": [
                { "type": "Issued", "payload": { "now": "$fields" } },
                { "type": "Filed", "payload": { "was": "$old_fields.draft" } }
            ]
        }]}}
    });
    let registry = registry(document);
    let runtime = Runtime::new(&registry);
    let created = runtime
        .create(
            "invoice",
            1,
            "i-1",
            json!({ "issued_at": "pending", "note": "remove", "draft": "v1" }),
        )
        .expect("created");
    let PreloadDecision::Load(prepared) = runtime
        .decide_before_load("invoice", 1, "i-1", "Issue", json!({}))
        .expect("prepared")
    else {
        panic!("the subject is required")
    };
    let LoadedDecision::NeedsFulfillment(prepared) =
        prepared.select_with(&created.instance).expect("selected")
    else {
        panic!("the outcome needs fulfillment")
    };
    let decision = prepared
        .complete(BTreeMap::from([
            (
                "issued_at".to_owned(),
                OperationFieldAction::Set {
                    value: json!("2026-10-07T00:00:00Z"),
                },
            ),
            ("note".to_owned(), OperationFieldAction::Remove),
        ]))
        .expect("completed")
        .into_decision()
        .expect("accepted");
    assert_eq!(
        text(&decision.instance.fields),
        r#"{"issued_at":"2026-10-07T00:00:00Z"}"#
    );
    assert_eq!(text(&decision.record.removed), r#"["draft","note"]"#);
    for event in &decision.events {
        assert_eq!(
            event.removed, decision.record.removed,
            "{}",
            event.event_type
        );
    }
    assert_eq!(
        text(&decision.events[0].payload),
        r#"{"now":{"issued_at":"2026-10-07T00:00:00Z"}}"#
    );
    assert_eq!(text(&decision.events[1].payload), r#"{"was":"v1"}"#);
    let replayed =
        replay(&[created.record, decision.record]).expect("the decision replays to its bytes");
    assert_eq!(text(&replayed), text(&decision.instance));
}

/// Two operations emit one event type on one transition; one clears a field, the other does not.
/// The honest history folds to what `execute` returned, and the events of one revision must agree
/// on their removal evidence.
#[test]
fn an_honest_history_of_two_emitters_of_one_event_folds_and_a_split_removal_is_refused() {
    let document = json!({
        "entity": "reminder", "version": 1,
        "schema": { "fields": {
            "title": { "type": "string", "required": true },
            "snoozed_until": { "type": "string" }
        }},
        "lifecycle": { "initial": "active", "states": ["active"] },
        "create": { "emit": { "type": "Opened", "payload": { "title": "$fields.title" } } },
        "operations": {
            "wake": {
                "transitions": [{ "from": "active", "to": "active" }],
                "set": { "snoozed_until": { "cleared": true } },
                "emits": [
                    { "type": "Touched", "payload": {} },
                    { "type": "Logged", "payload": {} }
                ]
            },
            "touch": {
                "transitions": [{ "from": "active", "to": "active" }],
                "emits": [
                    { "type": "Touched", "payload": {} },
                    { "type": "Logged", "payload": {} }
                ]
            }
        }
    });
    let registry = registry(document);
    let validated = registry.get("reminder", 1).expect("registered");
    let runtime = Runtime::new(&registry);
    let created = runtime
        .create(
            "reminder",
            1,
            "r-1",
            json!({ "title": "t", "snoozed_until": "2026-10-08T09:00:00Z" }),
        )
        .expect("created");
    let touched = runtime
        .execute(&created.instance, "touch", json!({}))
        .expect("touched");
    let woken = runtime
        .execute(&touched.instance, "wake", json!({}))
        .expect("woken");
    let touched_again = runtime
        .execute(&woken.instance, "touch", json!({}))
        .expect("touched again");
    assert_eq!(woken.events.len(), 2);
    for event in &woken.events {
        assert_eq!(text(&event.removed), r#"["snoozed_until"]"#);
    }

    let events: Vec<DomainEvent> = [
        created.events.clone(),
        touched.events.clone(),
        woken.events.clone(),
        touched_again.events.clone(),
    ]
    .concat();
    let folded = rehydrate(validated, &events).expect("an honest history folds");
    assert_eq!(text(&folded), text(&touched_again.instance));

    // One event of `wake`'s revision keeps the evidence and the other loses it: two accounts of
    // one decision, refused before any candidate is asked.
    let mut split = events.clone();
    split[4].removed.clear();
    let folded = rehydrate(validated, &split);
    assert!(
        matches!(&folded, Err(entity_core::CoreError::Validation(errors))
            if errors.len() == 1 && errors[0].message.contains("describe different decisions")),
        "a revision whose events disagree on removal folded: {:?}",
        folded.map(|instance| text(&instance))
    );
}
