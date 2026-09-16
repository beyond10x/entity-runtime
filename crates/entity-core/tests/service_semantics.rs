//! The `service/1` document rules: byte preservation, the numbered order, branch selection, named
//! outcomes, refusals, responses and the update effect.
//!
//! Every case here is named for the behaviour it protects and asserts a variant rather than
//! `is_err`, which is what makes a renamed message a documentation change rather than a silent one.

use entity_core::{
    create, decide, execute, rehydrate, replay, CoreError, DecisionCommand, DecisionEffect,
    DefinitionError, DefinitionErrors, EntityDefinition, EntityInstance, Evaluation, Refusal,
    Registry, ValidatedDefinition,
};
use serde_json::{json, Value};

// --- shared fixtures ------------------------------------------------------------------------------

fn definition(value: Value) -> EntityDefinition {
    serde_json::from_value(value).expect("the fixture is a definition document")
}

#[allow(dead_code)]
fn validated(value: Value) -> ValidatedDefinition {
    ValidatedDefinition::new(definition(value)).expect("the fixture registers")
}

fn refused(value: Value) -> DefinitionErrors {
    ValidatedDefinition::new(definition(value)).expect_err("the fixture is refused")
}

fn carries(defects: &DefinitionErrors, expected: &DefinitionError) -> bool {
    defects.iter().any(|defect| defect == expected)
}

/// The keys one serialized value carries, in name order.
fn keys(value: &Value) -> Vec<&str> {
    value
        .as_object()
        .expect("an object")
        .keys()
        .map(String::as_str)
        .collect()
}

/// The `kernel/1` ticket every byte-preservation case is measured against.
fn kernel_ticket() -> Value {
    json!({
        "entity": "ticket",
        "version": 1,
        "schema": { "fields": {
            "title": { "type": "string", "required": true },
            "resolution": { "type": "string" }
        }},
        "lifecycle": { "initial": "open", "states": ["open", "closed"] },
        "create": { "emit": { "type": "TicketOpened", "payload": { "id": "$id" } } },
        "operations": { "close": {
            "arguments": { "fields": { "resolution": { "type": "string", "required": true } } },
            "transitions": [{ "from": "open", "to": "closed" }],
            "set": { "resolution": "$args.resolution" },
            "emits": [{ "type": "TicketClosed", "payload": { "resolution": "$fields.resolution" } }]
        }}
    })
}

/// The witness lifecycle and command of § 4.4, whose one unanswered (state, input) pair the source
/// neither validates, synthesizes, runs nor answers in a reference target.
fn close_ticket_witness() -> Value {
    json!({
        "entity": "witness_ticket",
        "version": 1,
        "semantics": "service/1",
        "schema": { "fields": { "note": { "type": "string" } } },
        "lifecycle": { "initial": "Open", "states": ["Open", "Held", "Closed", "Archived"] },
        "operations": { "CloseTicket": {
            "arguments": { "fields": { "urgent": { "type": "boolean", "required": true } } },
            "outcomes": [
                {
                    "name": "closed-fast",
                    "when": { "eq": ["$args.urgent", true] },
                    "effect": { "moves": { "to": "Closed", "from": "Open" } },
                    "set": { "note": "fast" }
                },
                {
                    "name": "closed-slow",
                    "effect": { "moves": { "to": "Closed", "from": "Held" } },
                    "set": { "note": "slow" }
                },
                {
                    "name": "wrong-state",
                    "wrong_state": true,
                    "refuses": { "error": "TicketStateConflict" }
                }
            ]
        }}
    })
}

fn instance(entity: &str, state: &str, fields: Value) -> EntityInstance {
    at(entity, "x-1", state, fields)
}

/// An instance at an explicit storage address, which a `service/1` identity field has to mirror.
fn at(entity: &str, id: &str, state: &str, fields: Value) -> EntityInstance {
    EntityInstance {
        entity: entity.to_owned(),
        version: 1,
        id: id.to_owned(),
        lifecycle_state: state.to_owned(),
        revision: 1,
        fields: fields.as_object().expect("fields are an object").clone(),
    }
}

// --- § 1.1 and § 1.2: bytes that must not move ----------------------------------------------------

/// Every key this contract adds is `#[serde(default)]` with a `skip_serializing_if` that is true
/// for the value a `kernel/1` definition has, so the document round-trips to the bytes it had.
#[test]
fn a_kernel_1_definition_and_record_serialize_to_the_bytes_they_serialized_to_before_branches() {
    let parsed = definition(kernel_ticket());
    let document = serde_json::to_value(&parsed).expect("serializes");

    // The exact key set a `kernel/1` definition serialized before this contract, at every level.
    // A new key that did not skip itself would show up here as an extra name.
    assert_eq!(
        keys(&document),
        vec![
            "create",
            "entity",
            "invariants",
            "lifecycle",
            "operations",
            "projections",
            "schema",
            "version"
        ]
    );
    assert_eq!(
        keys(&document["schema"]),
        vec!["additional_fields", "fields"]
    );
    assert_eq!(
        keys(&document["schema"]["fields"]["title"]),
        vec![
            "acyclic",
            "additional_properties",
            "entity",
            "inverse",
            "items",
            "max",
            "max_length",
            "min",
            "min_length",
            "properties",
            "required",
            "type",
            "values"
        ],
        "a field grew a key: `key`, `tag` and `variants` must skip themselves"
    );
    assert_eq!(keys(&document["create"]), vec!["emit"]);
    assert_eq!(
        keys(&document["operations"]["close"]),
        vec!["arguments", "emits", "preconditions", "set", "transitions"]
    );
    // `create`'s existing spelling is untouched, including the `{"emit":null}` a definition that
    // emits nothing on creation has always written.
    let mut silent = kernel_ticket();
    silent["create"] = json!({});
    assert_eq!(
        serde_json::to_value(definition(silent)).expect("serializes")["create"],
        json!({ "emit": null })
    );

    let registry = registry_of(kernel_ticket());
    let decision = entity_core::Runtime::new(&registry)
        .create("ticket", 1, "t-1", json!({ "title": "Login fails" }))
        .expect("creates");
    let record = serde_json::to_value(&decision.record).expect("serializes");
    let keys: Vec<&String> = record.as_object().expect("object").keys().collect();
    assert_eq!(
        keys,
        vec![
            "changed",
            "command",
            "definition",
            "entity",
            "events",
            "from_state",
            "id",
            "result",
            "revision",
            "to_state"
        ],
        "a kernel/1 record carries no outcome, effect or response key"
    );
    assert_eq!(
        record["command"],
        json!({ "command": "create", "fields": { "title": "Login fails" } }),
        "a kernel/1 creation records no arguments key"
    );
    assert_eq!(
        record["definition"]["create"],
        json!({ "emit": { "type": "TicketOpened", "payload": { "id": "$id" } } })
    );
    assert!(record["definition"].get("semantics").is_none());
}

/// A `service/1` decision names its branch, its effect and its response, and its original request
/// is the caller's arguments — reconstructing it as the fields the branch produced would hand a
/// retry a request the caller never sent.
#[test]
fn a_service_1_decision_frames_as_er_record_2_and_its_request_carries_arguments_not_fields() {
    let registry = registry_of(service_invoice());
    let decision = entity_core::Runtime::new(&registry)
        .create(
            "invoice",
            1,
            "s:INV-1",
            json!({ "input": { "invoice_id": "INV-1", "amount": 10, "customer_email": "a@b.c" } }),
        )
        .expect("creates");
    assert_eq!(decision.record.outcome.as_deref(), Some("accepted"));
    assert_eq!(decision.record.effect, Some(DecisionEffect::Created));
    let DecisionCommand::Create { fields, arguments } = &decision.record.command else {
        panic!("a creation records a create command");
    };
    assert_eq!(arguments["input"]["amount"], json!(10));
    assert_eq!(fields["total"], json!(10));
    assert!(
        !fields.contains_key("input"),
        "the fields are what the branch produced, not the request"
    );
    let record = serde_json::to_value(&decision.record).expect("serializes");
    assert_eq!(record["definition"]["semantics"], json!("service/1"));
    assert_eq!(record["command"]["arguments"]["input"]["amount"], json!(10));
}

// --- § 1.3: what a build predating this refuses ---------------------------------------------------

/// A struct mirroring the `kernel/1` shape refuses a `service/1` document by naming the key it does
/// not know, rather than reading a definition whose branches it would silently not evaluate.
#[test]
fn a_pre_service_reader_refuses_a_service_1_definition_by_naming_its_unknown_field() {
    #[derive(Debug, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    #[allow(dead_code)]
    struct PreService {
        entity: String,
        #[serde(default)]
        version: u32,
        schema: Value,
        lifecycle: Value,
        #[serde(default)]
        invariants: Value,
        #[serde(default)]
        create: Value,
        #[serde(default)]
        operations: Value,
        #[serde(default)]
        projections: Value,
    }

    let document = service_invoice();
    let error = serde_json::from_value::<PreService>(document).expect_err("refused");
    let message = error.to_string();
    assert!(
        message.starts_with("unknown field")
            && [
                "semantics",
                "identity",
                "relations",
                "scales",
                "number_observation"
            ]
            .iter()
            .any(|key| message.contains(key)),
        "the refusal names the key it does not know: {message}"
    );
}

/// The hand-written `Condition` reader admits exactly one known operator and names what it found,
/// so a build predating `service/1` refuses `for_all`, `for_any`, `truthy` and `compare` by name.
#[test]
fn a_kernel_1_definition_using_a_service_operator_is_refused_at_registration() {
    for operator in ["compare", "truthy", "for_all", "for_any"] {
        let condition = match operator {
            "compare" => {
                json!({ "compare": { "left": "$fields.title", "op": "eq", "right": "x" } })
            }
            "truthy" => json!({ "truthy": "$fields.title" }),
            _ => json!({ operator: { "in": "$fields.lines", "as": "line", "that": true } }),
        };
        let mut document = kernel_ticket();
        document["schema"]["fields"]["lines"] =
            json!({ "type": "array", "items": { "type": "string" } });
        document["invariants"] = json!([{ "assert": condition }]);
        let defects = refused(document);
        assert!(
            carries(
                &defects,
                &DefinitionError::SemanticsKeyNotAvailable {
                    path: "invariants[0].assert".to_owned(),
                    key: operator.to_owned(),
                }
            ),
            "{operator} was not refused by name: {defects}"
        );
    }
}

// --- § 4.1 and § 4.2: the numbered order ----------------------------------------------------------

/// The code checks the instance's type before its state, and both documents now say so.
#[test]
fn the_kernel_checks_entity_mismatch_before_unknown_state_and_both_documents_say_so() {
    let registry = registry_of(kernel_ticket());
    let definition = registry.get("ticket", 1).expect("registered");
    let wrong = EntityInstance {
        entity: "order".to_owned(),
        version: 1,
        id: "t-1".to_owned(),
        lifecycle_state: "nowhere".to_owned(),
        revision: 1,
        fields: json!({ "title": "x" }).as_object().expect("object").clone(),
    };
    // Both are wrong. The type mismatch is the one reported, which is step 0.
    assert!(matches!(
        execute(
            definition,
            &wrong,
            "close",
            json!({ "resolution": "fixed" })
        ),
        Err(CoreError::EntityMismatch { .. })
    ));

    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates/")
        .parent()
        .expect("repository root");
    let kernel_design =
        std::fs::read_to_string(root.join("docs/design/kernel-v0.1.md")).expect("readable");
    let order = kernel_design
        .split("## 6. Evaluation order")
        .nth(1)
        .expect("the section exists");
    let numbered: Vec<&str> = order
        .lines()
        .skip_while(|line| !line.starts_with(" 0."))
        .take(2)
        .collect();
    assert!(
        numbered[0].contains("EntityMismatch") && numbered[1].contains("UnknownState"),
        "kernel-v0.1.md § 6 no longer numbers the two identity checks in the code's order: {numbered:?}"
    );
    let agents = std::fs::read_to_string(root.join("AGENTS.md")).expect("readable");
    assert!(
        agents.contains("twelve-step evaluation order"),
        "AGENTS.md invariant 8 no longer states the corrected count"
    );
    assert!(!agents.contains("eleven-step evaluation order"));
}

/// The sixteen steps, as the sequence of observable refusals a `service/1` operation produces when
/// each step in turn is the first one that can fail.
#[test]
fn a_service_1_operation_runs_the_sixteen_steps_in_the_numbered_order() {
    let registry = registry_of(service_invoice());
    let definition = registry.get("invoice", 1).expect("registered");
    let held = at(
        "invoice",
        "s:INV-1",
        "pending",
        json!({ "invoice_id": "INV-1", "total": 10 }),
    );

    // 0: the instance was created under another definition.
    let mut other = held.clone();
    other.entity = "order".to_owned();
    assert!(matches!(
        decide(
            definition,
            &other,
            "pay",
            json!({ "input": { "amount": 5 } })
        ),
        Err(CoreError::EntityMismatch { .. })
    ));
    // 1: a state the definition does not declare, reported after the type matches.
    let mut nowhere = held.clone();
    nowhere.lifecycle_state = "nowhere".to_owned();
    assert!(matches!(
        decide(
            definition,
            &nowhere,
            "pay",
            json!({ "input": { "amount": 5 } })
        ),
        Err(CoreError::UnknownState { .. })
    ));
    // 2: no such operation.
    assert!(matches!(
        decide(definition, &held, "refund", json!({})),
        Err(CoreError::OperationNotFound { .. })
    ));
    // 3: the arguments are validated before any branch is selected.
    assert!(matches!(
        decide(
            definition,
            &held,
            "pay",
            json!({ "input": { "amount": "ten" } })
        ),
        Err(CoreError::Validation(_))
    ));
    // 4: no branch applies.
    assert!(matches!(
        decide(definition, &held, "audit", json!({ "input": { "level": 9 } })),
        Err(CoreError::NoOutcomeSelected { operation }) if operation == "audit"
    ));
    // 6: the selected branch refuses, and produces nothing.
    let Ok(Evaluation::Refused(refusal)) = decide(
        definition,
        &held,
        "pay",
        json!({ "input": { "amount": 0 } }),
    ) else {
        panic!("a non-positive amount takes the refusing branch");
    };
    assert_eq!(
        refusal,
        Refusal {
            outcome: "rejected".to_owned(),
            error: "AmountNotPositive".to_owned(),
            message: Some("an invoice is paid with a positive amount".to_owned()),
        }
    );
    // 12: an invariant judges the next state.
    let overpaid = at(
        "invoice",
        "s:INV-1",
        "pending",
        json!({ "invoice_id": "INV-1", "total": 10 }),
    );
    assert!(matches!(
        decide(
            definition,
            &overpaid,
            "pay",
            json!({ "input": { "amount": 99999 } })
        ),
        Err(CoreError::InvariantViolation { .. })
    ));
    // 15: the decision.
    let Ok(Evaluation::Accepted(decision)) = decide(
        definition,
        &held,
        "pay",
        json!({ "input": { "amount": 10 } }),
    ) else {
        panic!("a positive amount is accepted");
    };
    assert_eq!(decision.record.outcome.as_deref(), Some("settled"));
    assert_eq!(decision.record.effect, Some(DecisionEffect::Moved));
    assert_eq!(decision.instance.lifecycle_state, "paid");
    assert_eq!(decision.instance.revision, 2);
}

/// Steps 11, 12, 13 and 14 have one order and it is this one. Each half fails if the pair is
/// swapped: a broken mirror is reported before the invariant that would also have failed, and a
/// response is materialised from the post-`set` fields the events were built from.
#[test]
fn the_identity_mirror_runs_before_the_invariants_and_the_response_after_the_events() {
    let mut document = service_invoice();
    // An invariant that also fails, so only the order decides which refusal is returned.
    document["invariants"] = json!([{
        "name": "identity_is_not_empty",
        "assert": { "ne": ["$fields.invoice_id", ""] },
        "message": "an invoice states its identity"
    }]);
    document["operations"]["rename"] = json!({
        "arguments": { "fields": {} },
        "outcomes": [{
            "name": "renamed",
            "effect": "updates",
            "set": { "invoice_id": "" }
        }]
    });
    let registry = registry_of(document);
    let definition = registry.get("invoice", 1).expect("registered");
    let held = at(
        "invoice",
        "s:INV-1",
        "pending",
        json!({ "invoice_id": "INV-1", "total": 10 }),
    );
    // The mirror is step 11 and the invariant is step 12, so the mirror is what answers.
    assert!(matches!(
        decide(definition, &held, "rename", json!({})),
        Err(CoreError::IdentityMismatch { ref field, .. }) if field == "invoice_id"
    ));

    // The response reads the post-`set` fields the events read, which is only true where both are
    // materialised after the `set` and the response after the events.
    let Ok(Evaluation::Accepted(decision)) = decide(
        definition,
        &held,
        "pay",
        json!({ "input": { "amount": 10 } }),
    ) else {
        panic!("accepted");
    };
    let response = decision
        .record
        .response
        .expect("a service/1 decision responds");
    assert_eq!(response["paid_total"], json!(10));
    assert_eq!(decision.events[0].payload["total"], json!(10));
}

/// Renumbering a `kernel/1` run against the legacy column recovers the twelve-step list unchanged:
/// steps 4, 6, 11 and 14 are absent and nothing else moves.
#[test]
fn renumbering_a_service_1_run_against_the_legacy_column_recovers_the_twelve_step_order() {
    let registry = registry_of(kernel_ticket());
    let definition = registry.get("ticket", 1).expect("registered");
    let open = instance("ticket", "open", json!({ "title": "Login fails" }));
    let closed = instance("ticket", "closed", json!({ "title": "Login fails" }));

    // Step 5 answers `InvalidTransition` and only `InvalidTransition`: no branch selection, no
    // wrong-state branch and no unspecified move source can arise.
    assert!(matches!(
        decide(
            definition,
            &closed,
            "close",
            json!({ "resolution": "again" })
        ),
        Err(CoreError::InvalidTransition { .. })
    ));
    let Ok(Evaluation::Accepted(decision)) =
        decide(definition, &open, "close", json!({ "resolution": "fixed" }))
    else {
        panic!("accepted");
    };
    // Steps 6, 11 and 14 are absent, so the record carries none of their products.
    assert_eq!(decision.record.outcome, None);
    assert_eq!(decision.record.effect, None);
    assert_eq!(decision.record.response, None);
}

// --- § 4.3: selection, exactly --------------------------------------------------------------------

/// The source's own reference target answers `rejected` for a non-positive amount **before** it
/// looks at the invoice's state, and `wrong-state` only for an amount that passed.
#[test]
fn an_input_guard_answers_before_the_held_state_is_tested() {
    let registry = registry_of(service_invoice());
    let definition = registry.get("invoice", 1).expect("registered");
    let paid = at(
        "invoice",
        "s:INV-1",
        "paid",
        json!({ "invoice_id": "INV-1", "total": 10 }),
    );

    // Already paid, and the amount is not positive. The input guard is what answers.
    let Ok(Evaluation::Refused(refusal)) = decide(
        definition,
        &paid,
        "pay",
        json!({ "input": { "amount": 0 } }),
    ) else {
        panic!("the input guard answers first");
    };
    assert_eq!(refusal.error, "AmountNotPositive");

    // The same state with an amount that passed answers the wrong-state branch instead.
    let Ok(Evaluation::Refused(refusal)) = decide(
        definition,
        &paid,
        "pay",
        json!({ "input": { "amount": 5 } }),
    ) else {
        panic!("the held state answers second");
    };
    assert_eq!(refusal.error, "InvoiceStateConflict");
}

/// The wrong-state set is the complement of **every** move source of the command, not of the
/// selected branch's own `from`.
#[test]
fn the_wrong_state_set_is_the_complement_of_every_move_source_not_of_the_selected_branch() {
    let registry = registry_of(close_ticket_witness());
    let definition = registry.get("witness_ticket", 1).expect("registered");
    // `move_sources = {Open} ∪ {Held}`, so `wrong_states = {Closed, Archived}`.
    for state in ["Closed", "Archived"] {
        let resting = instance("witness_ticket", state, json!({}));
        let Ok(Evaluation::Refused(refusal)) = decide(
            definition,
            &resting,
            "CloseTicket",
            json!({ "urgent": true }),
        ) else {
            panic!("{state} is a wrong state");
        };
        assert_eq!(refusal.error, "TicketStateConflict");
    }
    // `Held` is a source of some move of this command, so it is **not** a wrong state — which is
    // the whole content of § 4.4.
    let held = instance("witness_ticket", "Held", json!({}));
    assert!(matches!(
        decide(definition, &held, "CloseTicket", json!({ "urgent": true })),
        Err(CoreError::UnspecifiedMoveSource { .. })
    ));
}

/// A selected move whose `from` excludes the state answers the wrong-state branch where the state
/// is one no move of the command starts from.
#[test]
fn a_selected_move_branch_whose_from_excludes_the_state_answers_the_wrong_state_branch() {
    let registry = registry_of(close_ticket_witness());
    let definition = registry.get("witness_ticket", 1).expect("registered");
    let archived = instance("witness_ticket", "Archived", json!({}));
    let Ok(Evaluation::Refused(refusal)) = decide(
        definition,
        &archived,
        "CloseTicket",
        json!({ "urgent": false }),
    ) else {
        panic!("Archived is a wrong state");
    };
    assert_eq!(refusal.outcome, "wrong-state");
}

/// With no wrong-state branch the same position is `InvalidTransition`, which is what `kernel/1`
/// returns today.
#[test]
fn a_selected_move_branch_with_no_wrong_state_branch_still_answers_invalid_transition() {
    let mut document = close_ticket_witness();
    document["operations"]["CloseTicket"]["outcomes"]
        .as_array_mut()
        .expect("outcomes")
        .pop();
    let registry = registry_of(document);
    let definition = registry.get("witness_ticket", 1).expect("registered");
    let archived = instance("witness_ticket", "Archived", json!({}));
    assert!(matches!(
        decide(definition, &archived, "CloseTicket", json!({ "urgent": true })),
        Err(CoreError::InvalidTransition { ref state, .. }) if state == "Archived"
    ));
}

/// The one (state, input) pair the source leaves open, and only it: the same command answers
/// normally for `Open` + urgent and for `Held` + not urgent.
#[test]
fn the_close_ticket_witness_registers_and_refuses_only_the_state_input_pair_the_source_leaves_open()
{
    let registry = registry_of(close_ticket_witness());
    let definition = registry.get("witness_ticket", 1).expect("registered");

    let open = instance("witness_ticket", "Open", json!({}));
    let Ok(Evaluation::Accepted(decision)) =
        decide(definition, &open, "CloseTicket", json!({ "urgent": true }))
    else {
        panic!("Open with urgent is an ordinary move");
    };
    assert_eq!(decision.record.outcome.as_deref(), Some("closed-fast"));

    let held = instance("witness_ticket", "Held", json!({}));
    let Ok(Evaluation::Accepted(decision)) =
        decide(definition, &held, "CloseTicket", json!({ "urgent": false }))
    else {
        panic!("Held with not-urgent is an ordinary move");
    };
    assert_eq!(decision.record.outcome.as_deref(), Some("closed-slow"));

    // And the pair, with the four facts a reader needs to repair the specification.
    let Err(CoreError::UnspecifiedMoveSource {
        operation,
        outcome,
        state,
        from,
    }) = decide(definition, &held, "CloseTicket", json!({ "urgent": true }))
    else {
        panic!("the unanswered pair is named");
    };
    assert_eq!(operation, "CloseTicket");
    assert_eq!(outcome, "closed-fast");
    assert_eq!(state, "Held");
    assert_eq!(from, vec!["Open".to_owned()]);

    // The mirror case.
    assert!(matches!(
        decide(definition, &open, "CloseTicket", json!({ "urgent": false })),
        Err(CoreError::UnspecifiedMoveSource { ref outcome, .. }) if outcome == "closed-slow"
    ));
}

/// A bare state guard is a `True` selector: selected in exactly the states it names, for every
/// admitted input.
#[test]
fn a_matching_in_state_branch_with_no_when_is_taken() {
    let registry = registry_of(state_guarded());
    let definition = registry.get("guarded", 1).expect("registered");
    let draft = instance("guarded", "draft", json!({ "note": "n" }));
    let Ok(Evaluation::Accepted(decision)) =
        decide(definition, &draft, "act", json!({ "flag": false }))
    else {
        panic!("a bare state guard selects");
    };
    assert_eq!(decision.record.outcome.as_deref(), Some("from-draft"));
}

/// A matching state guard whose own `when` is false is skipped, which is what keeps a present guard
/// running rather than made redundant by the state test.
#[test]
fn a_matching_in_state_branch_whose_when_is_false_is_skipped() {
    let registry = registry_of(state_guarded());
    let definition = registry.get("guarded", 1).expect("registered");
    let review = instance("guarded", "review", json!({ "note": "n" }));
    let Ok(Evaluation::Accepted(decision)) =
        decide(definition, &review, "act", json!({ "flag": false }))
    else {
        panic!("the default is reached");
    };
    assert_eq!(decision.record.outcome.as_deref(), Some("otherwise"));

    let Ok(Evaluation::Accepted(decision)) =
        decide(definition, &review, "act", json!({ "flag": true }))
    else {
        panic!("the guarded branch is reached");
    };
    assert_eq!(decision.record.outcome.as_deref(), Some("from-review"));
}

/// A branch the held state has already excluded cannot make the command unobservable, so its guard
/// is not evaluated at all.
#[test]
fn a_non_matching_in_state_branch_is_skipped_without_evaluating_its_when() {
    let mut document = state_guarded();
    // An unobservable guard on the branch the held state excludes.
    document["operations"]["act"]["outcomes"][1]["when"] = json!({ "eq": ["$fields.missing", 1] });
    document["schema"]["fields"]["missing"] = json!({ "type": "integer" });
    let registry = registry_of(document);
    let definition = registry.get("guarded", 1).expect("registered");
    let draft = instance("guarded", "draft", json!({ "note": "n" }));
    let Ok(Evaluation::Accepted(decision)) =
        decide(definition, &draft, "act", json!({ "flag": true }))
    else {
        panic!("the excluded branch's guard is never asked");
    };
    assert_eq!(decision.record.outcome.as_deref(), Some("from-draft"));
}

/// A bare state guard is not a second default and does not have to be last.
#[test]
fn a_selector_free_default_is_still_separate_from_a_bare_state_guard() {
    // The state guard is declared first and the selector-free default last: admitted.
    assert!(ValidatedDefinition::new(definition(state_guarded())).is_ok());

    // Two selector-free branches, or one that is not last, is the refusal.
    let mut document = state_guarded();
    document["operations"]["act"]["outcomes"]
        .as_array_mut()
        .expect("outcomes")
        .push(json!({ "name": "second-default", "effect": "updates", "set": { "note": "b" } }));
    let defects = refused(document);
    assert!(carries(
        &defects,
        &DefinitionError::AmbiguousDefaultOutcome {
            command: "act".to_owned()
        }
    ));
}

/// An unanswerable guard refuses the command and names every address nothing was observed at;
/// selection stops there and no later branch is tried.
#[test]
fn an_unobservable_guard_refuses_the_command_instead_of_taking_the_next_branch() {
    let mut document = state_guarded();
    document["schema"]["fields"]["missing"] = json!({ "type": "integer" });
    document["operations"]["act"]["outcomes"][0]["in_state"] = Value::Null;
    document["operations"]["act"]["outcomes"][0]["when"] = json!({ "eq": ["$fields.missing", 1] });
    let registry = registry_of(document);
    let definition = registry.get("guarded", 1).expect("registered");
    let draft = instance("guarded", "draft", json!({ "note": "n" }));
    let Err(CoreError::OutcomeUnobservable {
        operation,
        outcome,
        unresolved,
    }) = decide(definition, &draft, "act", json!({ "flag": true }))
    else {
        panic!("an unanswerable guard refuses the command");
    };
    assert_eq!(operation, "act");
    assert_eq!(outcome, "from-draft");
    assert_eq!(unresolved, vec!["$fields.missing".to_owned()]);
}

/// Declared order is the tie-break where two guards both hold, which is the rule `kernel/1` already
/// applies to transitions.
#[test]
fn branches_are_tried_in_declared_order_and_the_first_applicable_one_is_taken() {
    let mut document = state_guarded();
    document["operations"]["act"]["outcomes"] = json!([
        { "name": "first", "when": { "eq": ["$args.flag", true] }, "effect": "updates", "set": { "note": "first" } },
        { "name": "second", "when": { "eq": ["$args.flag", true] }, "effect": "updates", "set": { "note": "second" } },
        { "name": "otherwise", "effect": "updates", "set": { "note": "otherwise" } }
    ]);
    let registry = registry_of(document);
    let definition = registry.get("guarded", 1).expect("registered");
    let draft = instance("guarded", "draft", json!({ "note": "n" }));
    let Ok(Evaluation::Accepted(decision)) =
        decide(definition, &draft, "act", json!({ "flag": true }))
    else {
        panic!("accepted");
    };
    assert_eq!(decision.record.outcome.as_deref(), Some("first"));
    assert_eq!(decision.instance.fields["note"], json!("first"));
}

// --- § 2.1: the registration refusals -------------------------------------------------------------

#[test]
fn an_operation_mixing_a_state_guard_and_a_wrong_state_branch_is_refused_at_registration() {
    let mut document = state_guarded();
    document["operations"]["act"]["outcomes"]
        .as_array_mut()
        .expect("outcomes")
        .push(json!({ "name": "wrong", "wrong_state": true, "refuses": { "error": "Conflict" } }));
    let defects = refused(document);
    assert!(carries(
        &defects,
        &DefinitionError::WrongStateWithStateGuard {
            operation: "act".to_owned()
        }
    ));
}

#[test]
fn a_state_guard_naming_a_state_its_move_does_not_start_from_is_refused() {
    let mut document = close_ticket_witness();
    document["operations"]["CloseTicket"]["outcomes"] = json!([
        {
            "name": "closed-fast",
            "in_state": "Held",
            "effect": { "moves": { "to": "Closed", "from": "Open" } },
            "set": { "note": "fast" }
        },
        { "name": "otherwise", "effect": "updates", "set": { "note": "none" } }
    ]);
    let defects = refused(document);
    assert!(carries(
        &defects,
        &DefinitionError::GuardStateOutsideMove {
            operation: "CloseTicket".to_owned(),
            outcome: "closed-fast".to_owned(),
            state: "Held".to_owned(),
        }
    ));
}

#[test]
fn a_wrong_state_branch_whose_moves_cover_every_state_is_refused_at_registration() {
    let mut document = close_ticket_witness();
    document["lifecycle"] = json!({ "initial": "Open", "states": ["Open", "Held"] });
    document["operations"]["CloseTicket"]["outcomes"] = json!([
        {
            "name": "closed-fast",
            "when": { "eq": ["$args.urgent", true] },
            "effect": { "moves": { "to": "Held", "from": ["Open", "Held"] } },
            "set": { "note": "fast" }
        },
        { "name": "wrong-state", "wrong_state": true, "refuses": { "error": "Conflict" } }
    ]);
    let defects = refused(document);
    assert!(carries(
        &defects,
        &DefinitionError::WrongStateUnreachable {
            operation: "CloseTicket".to_owned()
        }
    ));
}

#[test]
fn a_selector_free_branch_that_is_not_last_is_refused() {
    let mut document = state_guarded();
    let outcomes = document["operations"]["act"]["outcomes"]
        .as_array_mut()
        .expect("outcomes");
    outcomes.swap(0, 2);
    let defects = refused(document);
    assert!(carries(
        &defects,
        &DefinitionError::AmbiguousDefaultOutcome {
            command: "act".to_owned()
        }
    ));
}

#[test]
fn a_branch_that_leaves_a_required_response_field_undetermined_is_refused_at_registration() {
    let mut document = service_invoice();
    document["operations"]["pay"]["outcomes"][0]["responds"] = json!({});
    let defects = refused(document);
    assert!(carries(
        &defects,
        &DefinitionError::OutcomeResponseIncomplete {
            command: "pay".to_owned(),
            outcome: "settled".to_owned(),
            field: "paid_total".to_owned(),
        }
    ));
}

#[test]
fn a_refusing_branch_that_declares_responds_is_refused_at_registration() {
    let mut document = service_invoice();
    document["operations"]["pay"]["outcomes"][1]["responds"] = json!({ "paid_total": 0 });
    let defects = refused(document);
    assert!(carries(
        &defects,
        &DefinitionError::RefusalMutatesState {
            command: "pay".to_owned(),
            outcome: "rejected".to_owned(),
        }
    ));
}

// --- § 2.2 and § 2.3: the reference scopes --------------------------------------------------------

#[test]
fn an_outcome_selector_may_not_read_to_state() {
    let mut document = service_invoice();
    document["operations"]["pay"]["outcomes"][0]["when"] = json!({ "eq": ["$to_state", "paid"] });
    let defects = refused(document);
    assert!(
        defects.iter().any(|defect| matches!(
            defect,
            DefinitionError::InvalidRule { path, message }
                if path == "operations.pay.outcomes.settled.when.eq[0]"
                    && message.contains("an outcome selector")
        )),
        "{defects}"
    );
}

#[test]
fn a_creation_set_may_not_read_fields() {
    let mut document = service_invoice();
    document["create"]["outcomes"][0]["set"]["total"] = json!("$fields.total");
    let defects = refused(document);
    assert!(
        defects.iter().any(|defect| matches!(
            defect,
            DefinitionError::InvalidTemplate { path, message }
                if path == "create.outcomes.accepted.set.total"
                    && message.contains("a creation outcome assignment")
        )),
        "{defects}"
    );
}

/// The billing regression: a creation event publishes an argument no field stores.
#[test]
fn a_creation_branch_event_payload_reads_a_creation_argument_no_field_stores() {
    let registry = registry_of(service_invoice());
    let decision = entity_core::Runtime::new(&registry)
        .create(
            "invoice",
            1,
            "s:INV-1",
            json!({ "input": { "invoice_id": "INV-1", "amount": 10, "customer_email": "a@b.c" } }),
        )
        .expect("creates");
    assert_eq!(
        decision.events[0].payload["customer_email"],
        json!("a@b.c"),
        "the payload publishes an argument the schema stores nowhere"
    );
    assert!(
        !decision.instance.fields.contains_key("customer_email"),
        "no field was invented to hold it"
    );
}

#[test]
fn a_creation_branch_response_reads_a_creation_argument_and_a_post_set_field() {
    let registry = registry_of(service_invoice());
    let decision = entity_core::Runtime::new(&registry)
        .create(
            "invoice",
            1,
            "s:INV-1",
            json!({ "input": { "invoice_id": "INV-1", "amount": 10, "customer_email": "a@b.c" } }),
        )
        .expect("creates");
    let response = decision
        .record
        .response
        .expect("a service/1 creation responds");
    assert_eq!(response["notified"], json!("a@b.c"));
    assert_eq!(response["opening_total"], json!(10));
}

/// The `kernel/1` spelling is untouched: its scope admits no `$args` at all.
#[test]
fn a_kernel_1_creation_event_payload_still_cannot_read_args() {
    let mut document = kernel_ticket();
    document["create"]["emit"]["payload"] = json!({ "who": "$args.title" });
    let defects = refused(document);
    assert!(
        defects.iter().any(|defect| matches!(
            defect,
            DefinitionError::InvalidTemplate { path, message }
                if path == "create.emit.payload.who" && message.contains("a creation event payload")
        )),
        "{defects}"
    );
}

// --- § 5 and § 6: refusals, responses, effects ----------------------------------------------------

#[test]
fn a_refusing_branch_produces_no_record_no_event_no_response_and_no_revision() {
    let registry = registry_of(service_invoice());
    let definition = registry.get("invoice", 1).expect("registered");
    let held = at(
        "invoice",
        "s:INV-1",
        "pending",
        json!({ "invoice_id": "INV-1", "total": 10 }),
    );
    let Ok(Evaluation::Refused(refusal)) = decide(
        definition,
        &held,
        "pay",
        json!({ "input": { "amount": 0 } }),
    ) else {
        panic!("refused");
    };
    assert_eq!(refusal.outcome, "rejected");
    // Nothing durable: `Evaluation::Refused` carries no decision at all, and the entry point that
    // returns a `Decision` answers the typed refusal instead.
    assert!(decide(
        definition,
        &held,
        "pay",
        json!({ "input": { "amount": 0 } })
    )
    .expect("evaluates")
    .accepted()
    .is_none());
    assert_eq!(
        execute(
            definition,
            &held,
            "pay",
            json!({ "input": { "amount": 0 } })
        ),
        Err(CoreError::Refused {
            outcome: "rejected".to_owned(),
            error: "AmountNotPositive".to_owned(),
            message: Some("an invoice is paid with a positive amount".to_owned()),
        })
    );
    assert_eq!(held.revision, 1, "the caller's instance is untouched");
}

#[test]
fn an_accepted_branch_that_changes_nothing_still_advances_one_revision_with_zero_events() {
    let mut document = service_invoice();
    document["operations"]["pay"]["outcomes"][2] =
        json!({ "name": "wrong-state", "wrong_state": true, "responds": { "paid_total": 0 } });
    let registry = registry_of(document);
    let definition = registry.get("invoice", 1).expect("registered");
    let paid = at(
        "invoice",
        "s:INV-1",
        "paid",
        json!({ "invoice_id": "INV-1", "total": 10 }),
    );
    let Ok(Evaluation::Accepted(decision)) = decide(
        definition,
        &paid,
        "pay",
        json!({ "input": { "amount": 5 } }),
    ) else {
        panic!("an accepting wrong-state branch is an ordinary decision");
    };
    assert_eq!(decision.record.effect, Some(DecisionEffect::Unchanged));
    assert_eq!(decision.instance.revision, 2);
    assert_eq!(decision.instance.lifecycle_state, "paid");
    assert!(decision.events.is_empty());
    assert!(decision.record.changed.is_empty());
}

#[test]
fn a_creation_branch_with_the_creates_effect_and_no_events_is_admitted_and_records_created() {
    let mut document = service_invoice();
    document["create"]["outcomes"][0]["emits"] = json!([]);
    let registry = registry_of(document);
    let decision = entity_core::Runtime::new(&registry)
        .create(
            "invoice",
            1,
            "s:INV-1",
            json!({ "input": { "invoice_id": "INV-1", "amount": 1, "customer_email": "a@b.c" } }),
        )
        .expect("a zero-event creation is admitted");
    assert!(decision.events.is_empty());
    assert_eq!(decision.record.effect, Some(DecisionEffect::Created));
}

#[test]
fn a_kernel_1_creation_that_emits_nothing_still_validates_and_still_creates() {
    let mut document = kernel_ticket();
    document["create"] = json!({});
    let registry = registry_of(document);
    let decision = entity_core::Runtime::new(&registry)
        .create("ticket", 1, "t-1", json!({ "title": "x" }))
        .expect("creates");
    assert!(decision.events.is_empty());
    assert_eq!(decision.record.effect, None);
}

#[test]
fn an_update_branch_keeps_its_state_and_records_updated_without_a_transition() {
    let registry = registry_of(service_invoice());
    let definition = registry.get("invoice", 1).expect("registered");
    let held = at(
        "invoice",
        "s:INV-1",
        "pending",
        json!({ "invoice_id": "INV-1", "total": 10 }),
    );
    let Ok(Evaluation::Accepted(decision)) = decide(
        definition,
        &held,
        "note",
        json!({ "input": { "text": "chased" } }),
    ) else {
        panic!("accepted");
    };
    assert_eq!(decision.record.effect, Some(DecisionEffect::Updated));
    assert_eq!(decision.record.from_state.as_deref(), Some("pending"));
    assert_eq!(decision.record.to_state, "pending");
    assert!(
        definition.operations["note"].transitions.is_empty(),
        "no self-transition was synthesized at registration"
    );
}

#[test]
fn a_declared_response_is_materialised_from_the_selected_branch_and_replays_byte_for_byte() {
    let registry = registry_of(service_invoice());
    let runtime = entity_core::Runtime::new(&registry);
    let created = runtime
        .create(
            "invoice",
            1,
            "s:INV-1",
            json!({ "input": { "invoice_id": "INV-1", "amount": 10, "customer_email": "a@b.c" } }),
        )
        .expect("creates");
    let paid = runtime
        .execute(
            &created.instance,
            "pay",
            json!({ "input": { "amount": 10 } }),
        )
        .expect("pays");
    assert_eq!(
        paid.record.response.as_ref().expect("responds")["paid_total"],
        json!(10)
    );
    let rebuilt = replay(&[created.record.clone(), paid.record.clone()]).expect("replays");
    assert_eq!(rebuilt, paid.instance);
}

#[test]
fn a_creation_branch_emits_zero_one_or_many_events_in_declaration_order() {
    let mut document = service_invoice();
    document["create"]["outcomes"][0]["emits"] = json!([
        { "type": "InvoiceCreated", "payload": { "customer_email": "$args.input.customer_email" } },
        { "type": "InvoiceLogged", "payload": { "total": "$fields.total" } },
        { "type": "InvoiceCreated", "payload": { "customer_email": "$args.input.customer_email" } }
    ]);
    let registry = registry_of(document);
    let decision = entity_core::Runtime::new(&registry)
        .create(
            "invoice",
            1,
            "s:INV-1",
            json!({ "input": { "invoice_id": "INV-1", "amount": 3, "customer_email": "a@b.c" } }),
        )
        .expect("creates");
    let types: Vec<&str> = decision
        .events
        .iter()
        .map(|event| event.event_type.as_str())
        .collect();
    assert_eq!(
        types,
        vec!["InvoiceCreated", "InvoiceLogged", "InvoiceCreated"],
        "declaration order, duplicates preserved"
    );
}

/// An event-only fold cannot see which branch ran, so it refuses by name before it reads an event.
#[test]
fn a_service_1_history_is_refused_by_rehydrate_before_any_event_arg_is_read() {
    let registry = registry_of(service_invoice());
    let definition = registry.get("invoice", 1).expect("registered");
    let decision = create(
        definition,
        "s:INV-1".to_owned(),
        json!({ "input": { "invoice_id": "INV-1", "amount": 10, "customer_email": "a@b.c" } }),
    )
    .expect("creates");
    let Err(CoreError::Validation(errors)) = rehydrate(definition, &decision.events) else {
        panic!("a service/1 history is refused");
    };
    assert_eq!(errors.len(), 1);
    assert!(errors[0].message.contains("service/1"));
    // And with no events at all, so nothing was read: the refusal is the same one.
    let Err(CoreError::Validation(empty)) = rehydrate(definition, &[]) else {
        panic!("refused");
    };
    assert_eq!(empty[0].message, errors[0].message);
}

#[test]
fn every_complete_branch_decision_replays_byte_for_byte_from_its_record() {
    let registry = registry_of(service_invoice());
    let runtime = entity_core::Runtime::new(&registry);
    let created = runtime
        .create(
            "invoice",
            1,
            "s:INV-1",
            json!({ "input": { "invoice_id": "INV-1", "amount": 7, "customer_email": "a@b.c" } }),
        )
        .expect("creates");
    let noted = runtime
        .execute(
            &created.instance,
            "note",
            json!({ "input": { "text": "chased" } }),
        )
        .expect("notes");
    let paid = runtime
        .execute(&noted.instance, "pay", json!({ "input": { "amount": 7 } }))
        .expect("pays");
    let records = vec![
        created.record.clone(),
        noted.record.clone(),
        paid.record.clone(),
    ];
    assert_eq!(replay(&records).expect("replays"), paid.instance);

    // Changing one recorded product is refused: the record is evidence, never an instruction.
    let mut tampered = records.clone();
    tampered[2].outcome = Some("rejected".to_owned());
    assert!(matches!(replay(&tampered), Err(CoreError::Validation(_))));
    let mut tampered = records.clone();
    tampered[2].effect = Some(DecisionEffect::Unchanged);
    assert!(matches!(replay(&tampered), Err(CoreError::Validation(_))));
    let mut tampered = records;
    tampered[2]
        .response
        .as_mut()
        .expect("responds")
        .insert("paid_total".to_owned(), json!(99));
    assert!(matches!(replay(&tampered), Err(CoreError::Validation(_))));
}

// --- fixtures used above --------------------------------------------------------------------------

fn registry_of(document: Value) -> Registry {
    let mut registry = Registry::new();
    registry
        .register(definition(document))
        .expect("the fixture registers");
    registry
}

/// The billing shape: a creation whose payload and response read an argument no field stores, an
/// input-guarded settlement, a refusing default and a wrong-state branch.
fn service_invoice() -> Value {
    json!({
        "entity": "invoice",
        "version": 1,
        "semantics": "service/1",
        "identity": { "field": "invoice_id" },
        "schema": { "fields": {
            "invoice_id": { "type": "string", "required": true },
            "total": { "type": "integer", "required": true },
            "note": { "type": "string" }
        }},
        "lifecycle": { "initial": "pending", "states": ["pending", "paid"] },
        "invariants": [{
            "name": "total_is_sane",
            "assert": { "compare": { "left": "$fields.total", "op": "lte", "right": 1000 } },
            "message": "an invoice total is at most a thousand"
        }],
        "create": {
            "arguments": { "fields": { "input": { "type": "object", "required": true, "properties": {
                "invoice_id": { "type": "string", "required": true },
                "amount": { "type": "integer", "required": true },
                "customer_email": { "type": "string", "required": true }
            }}}},
            "response": { "fields": {
                "opening_total": { "type": "integer", "required": true },
                "notified": { "type": "string" }
            }},
            "outcomes": [{
                "name": "accepted",
                "effect": "creates",
                "set": {
                    "invoice_id": "$args.input.invoice_id",
                    "total": "$args.input.amount"
                },
                "emits": [{
                    "type": "InvoiceCreated",
                    "payload": {
                        "invoice_id": "$fields.invoice_id",
                        "customer_email": "$args.input.customer_email"
                    }
                }],
                "responds": {
                    "opening_total": "$fields.total",
                    "notified": "$args.input.customer_email"
                }
            }]
        },
        "operations": {
            "pay": {
                "arguments": { "fields": { "input": { "type": "object", "required": true, "properties": {
                    "amount": { "type": "integer", "required": true }
                }}}},
                "response": { "fields": { "paid_total": { "type": "integer", "required": true } } },
                "outcomes": [
                    {
                        "name": "settled",
                        "when": { "compare": { "left": "$args.input.amount", "op": "gt", "right": 0 } },
                        "effect": { "moves": { "to": "paid", "from": "pending" } },
                        "set": { "total": "$args.input.amount" },
                        "emits": [{ "type": "InvoicePaid", "payload": { "total": "$fields.total" } }],
                        "responds": { "paid_total": "$fields.total" }
                    },
                    {
                        "name": "rejected",
                        "refuses": {
                            "error": "AmountNotPositive",
                            "message": "an invoice is paid with a positive amount"
                        }
                    },
                    {
                        "name": "wrong-state",
                        "wrong_state": true,
                        "refuses": { "error": "InvoiceStateConflict" }
                    }
                ]
            },
            "note": {
                "arguments": { "fields": { "input": { "type": "object", "required": true, "properties": {
                    "text": { "type": "string", "required": true }
                }}}},
                "outcomes": [{
                    "name": "noted",
                    "effect": "updates",
                    "set": { "note": "$args.input.text" }
                }]
            },
            "audit": {
                "arguments": { "fields": { "input": { "type": "object", "required": true, "properties": {
                    "level": { "type": "integer", "required": true }
                }}}},
                "outcomes": [{
                    "name": "audited",
                    "when": { "compare": { "left": "$args.input.level", "op": "lt", "right": 5 } },
                    "effect": "updates",
                    "set": { "note": "audited" }
                }]
            }
        }
    })
}

/// A bare state guard declared first, a guarded branch with its own `when`, and a selector-free
/// default declared last.
fn state_guarded() -> Value {
    json!({
        "entity": "guarded",
        "version": 1,
        "semantics": "service/1",
        "schema": { "fields": { "note": { "type": "string", "required": true } } },
        "lifecycle": { "initial": "draft", "states": ["draft", "review", "done"] },
        "operations": { "act": {
            "arguments": { "fields": { "flag": { "type": "boolean", "required": true } } },
            "outcomes": [
                { "name": "from-draft", "in_state": "draft", "effect": "updates", "set": { "note": "draft" } },
                {
                    "name": "from-review",
                    "in_state": "review",
                    "when": { "eq": ["$args.flag", true] },
                    "effect": "updates",
                    "set": { "note": "review" }
                },
                { "name": "otherwise", "effect": "updates", "set": { "note": "otherwise" } }
            ]
        }}
    })
}
