//! Independent source examination, pass 1: cases driven from
//! `docs/design/service-semantics-v0.1.md` against the implementation the same commit wrote.
//!
//! Each case names the contract line it is driven from. Nothing here is a second opinion about the
//! implementation's own suite; each one is a claim the admitted document makes that a program can
//! decide.

use entity_core::{
    create, CoreError, DefinitionError, EntityDefinition, Registry, Truth, ValidatedDefinition,
};
use serde_json::{json, Value};

fn definition(value: Value) -> EntityDefinition {
    serde_json::from_value(value).expect("the fixture is a definition document")
}

fn document(value: &str) -> Value {
    // `serde_json::from_str` retains the authored token, which is the whole subject of § 10.2.1;
    // `json!` cannot carry an integer past the `i64`/`u64` span at all.
    serde_json::from_str(value).expect("the fixture is a JSON document")
}

/// A `service/1` definition whose only rule is `condition`, over `schema`.
fn probe(schema: Value, condition: Value) -> Value {
    json!({
        "entity": "probe",
        "version": 1,
        "semantics": "service/1",
        "schema": schema,
        "lifecycle": { "initial": "held", "states": ["held"] },
        "invariants": [{ "name": "rule", "assert": condition, "message": "the rule" }]
    })
}

/// What one condition answers about one instance, read through the invariant it is written as.
fn answer(schema: Value, condition: Value, fields: Value) -> Truth {
    let validated = ValidatedDefinition::new(definition(probe(schema, condition)))
        .expect("the probe registers");
    match create(&validated, "p-1".to_owned(), fields) {
        Ok(_) => Truth::True,
        Err(CoreError::InvariantViolation { .. }) => Truth::False,
        Err(CoreError::InvariantUnobservable { .. }) => Truth::Unknown,
        Err(other) => panic!("the probe refused for another reason: {other}"),
    }
}

/// § 10.2.1: *"Operand origin is retained while evaluating: a number reached through a reference
/// uses `of_number`; an authored numeric literal uses `of_literal` … **Membership preserves that
/// distinction for each operand**"*, and § 10.4: *"`eq`, `ne`, `in` and `contains` over two numbers
/// use the same rule … a membership test that disagreed with `compare` would be a second numeric
/// semantics inside one document."*
///
/// The two doors part company on an authored integer past the `u64` span: the literal door keeps it
/// at scale zero, the wire door has no exact carrier for it and reads the binary64's canonical
/// decimal instead. `compare` observes each operand through its own door and answers **false**;
/// `eq`, `ne`, `in` and `contains` resolve both operands and then read both through the wire door,
/// so they answer the opposite.
#[test]
fn service_equality_and_membership_read_an_authored_literal_through_the_literal_door() {
    let schema = json!({ "fields": { "big": { "type": "number", "required": true } } });
    let fields = document(r#"{"big": 100000000000000000000000001}"#);

    // The reference reads `100000000000000000000000000` — the canonical decimal of the binary64
    // the token collapses to — and the literal reads `100000000000000000000000001` exactly, so the
    // two are not one value.
    let compared = answer(
        schema.clone(),
        document(
            r#"{"compare": {"left": "$fields.big", "op": "eq",
                            "right": 100000000000000000000000001}}"#,
        ),
        fields.clone(),
    );
    assert_eq!(
        compared,
        Truth::False,
        "compare observes each operand through its own door"
    );

    for (operator, condition) in [
        (
            "eq",
            r#"{"eq": ["$fields.big", 100000000000000000000000001]}"#,
        ),
        (
            "ne",
            r#"{"not": {"ne": ["$fields.big", 100000000000000000000000001]}}"#,
        ),
        (
            "in",
            r#"{"in": ["$fields.big", [100000000000000000000000001]]}"#,
        ),
        (
            "contains",
            r#"{"contains": [[100000000000000000000000001], "$fields.big"]}"#,
        ),
    ] {
        assert_eq!(
            answer(schema.clone(), document(condition), fields.clone()),
            compared,
            "{operator} answers what compare answers"
        );
    }

    // The control: a literal both doors read alike, so the divergence above is located at the door
    // and not at equality in general.
    let one = json!({ "fields": { "amount": { "type": "number", "required": true } } });
    assert_eq!(
        answer(
            one.clone(),
            json!({ "compare": { "left": "$fields.amount", "op": "eq", "right": 1 } }),
            json!({ "amount": 1 })
        ),
        Truth::True
    );
    assert_eq!(
        answer(
            one,
            json!({ "eq": ["$fields.amount", 1] }),
            json!({ "amount": 1 })
        ),
        Truth::True
    );
}

/// § 2.1: *"`RelationViaUnknown` / `RelationViaWrongShape` | for a `References` relation: `via` is
/// not a declared field of **this** definition, **or its kind is not the one § 8.1's row admits**"*,
/// and § 8.1's `References`/`Many` row: *"`{type: array, items: <the target's identity kind>}`"*.
///
/// `item` declares an `integer` logical identity. `basket` carries the relation in an array of
/// `string`, which is not the target's identity kind, and neither `EntityDefinition::validate` nor
/// `Registry::validate_all` looks at the element kind at all — only at whether the carrier is an
/// array.
#[test]
fn a_references_many_carrier_whose_elements_are_not_the_targets_identity_kind_is_refused() {
    let item = json!({
        "entity": "item",
        "version": 1,
        "semantics": "service/1",
        "identity": { "field": "sku" },
        "schema": { "fields": { "sku": { "type": "integer", "required": true } } },
        "lifecycle": { "initial": "listed", "states": ["listed"] }
    });
    let basket = json!({
        "entity": "basket",
        "version": 1,
        "semantics": "service/1",
        "relations": { "items": {
            "kind": "references", "target": "item", "cardinality": "many", "via": "item_ids"
        }},
        "schema": { "fields": { "item_ids": {
            "type": "array", "required": true, "items": { "type": "string" }
        }}},
        "lifecycle": { "initial": "open", "states": ["open"] }
    });

    let mut registry = Registry::new();
    registry.register(definition(item)).expect("item registers");
    registry
        .register(definition(basket))
        .expect("the carrier's array-ness is all one definition can answer");

    let defects = registry
        .validate_all()
        .expect_err("the whole registry can compare the carrier against the target's identity");
    assert!(
        defects.iter().any(|defect| matches!(
            defect,
            DefinitionError::RelationViaWrongShape { via, .. } if via == "item_ids"
        )),
        "the carrier's element kind is not the target's identity kind: {defects}"
    );
}

/// The `References`/`One` half of the same row: *"the target's identity kind"*. A `boolean` carrier
/// for an `integer` identity is admitted because the only shape test on this row is *not an array*.
#[test]
fn a_references_one_carrier_of_the_wrong_kind_is_refused() {
    let item = json!({
        "entity": "item",
        "version": 1,
        "semantics": "service/1",
        "identity": { "field": "sku" },
        "schema": { "fields": { "sku": { "type": "integer", "required": true } } },
        "lifecycle": { "initial": "listed", "states": ["listed"] }
    });
    let basket = json!({
        "entity": "basket",
        "version": 1,
        "semantics": "service/1",
        "relations": { "head": {
            "kind": "references", "target": "item", "cardinality": "one", "via": "head_id"
        }},
        "schema": { "fields": { "head_id": { "type": "boolean", "required": true } } },
        "lifecycle": { "initial": "open", "states": ["open"] }
    });

    let mut registry = Registry::new();
    registry.register(definition(item)).expect("item registers");
    registry.register(definition(basket)).expect("registers");

    let defects = registry
        .validate_all()
        .expect_err("a boolean carrier cannot hold an integer identity");
    assert!(
        defects.iter().any(|defect| matches!(
            defect,
            DefinitionError::RelationViaWrongShape { via, .. } if via == "head_id"
        )),
        "the carrier's kind is not the target's identity kind: {defects}"
    );
}

/// § 4.3's first selection line: *"**The state test.** If `in_state` is declared and is not the
/// instance's current state, **skip the branch**"*, and § 4.2's step 10 for a creation: *"the state
/// is the lifecycle's `initial`"*.
///
/// `select_outcome` is handed `state: None` at creation
/// (`crates/entity-core/src/runtime.rs:541-546`), so its state test cannot fire
/// (`:987-991`) and a branch guarded on `closed` is selected while the instance is being created in
/// `held`. The branch is also not a default, so `AmbiguousDefaultOutcome`'s last-position rule never
/// looks at it.
#[test]
fn a_creation_branch_guarded_on_a_state_the_creation_cannot_be_in_is_skipped() {
    let document = json!({
        "entity": "probe",
        "version": 1,
        "semantics": "service/1",
        "schema": { "fields": { "title": { "type": "string", "required": true } } },
        "lifecycle": { "initial": "held", "states": ["held", "closed"] },
        "create": {
            "arguments": { "fields": { "title": { "type": "string", "required": true } } },
            "outcomes": [
                { "name": "guarded", "in_state": "closed", "effect": "creates",
                  "set": { "title": "$args.title" } },
                { "name": "default", "effect": "creates",
                  "set": { "title": "$args.title" } }
            ]
        }
    });
    let validated = ValidatedDefinition::new(definition(document)).expect("registers");
    let decision =
        create(&validated, "p-1".to_owned(), json!({ "title": "t" })).expect("creation succeeds");
    assert_eq!(
        decision.record.outcome.as_deref(),
        Some("default"),
        "a branch guarded on `closed` cannot be the branch that creates an instance in `held`"
    );
}

/// **The one expectation this copy corrects, on root's explicit disposition of F6**
/// (`service-source-correction-1/coordinator-disposition.md`, the F6 paragraph): the measurement
/// the reviewer made is right and the expectation it was written against is not, so the
/// *expectation* moves and the behaviour stays. The reviewer's own file and its raw red output are
/// immutable in its separate tree; this is the implementation's copy.
///
/// § 2.2 said *"Nothing else changes"*, which read as the fixed roots taking precedence over a
/// binder that names one. They do not. The authoritative source predicate `Element::rebind`
/// rewrites **any** matching first namespace and passes every other namespace through
/// (`ESS/crates/specify/ess-primitives/src/predicate.rs`, around `:435`), and § 10.4 says the same:
/// `$<bind>` is the element, every other address passes through, and an inner same-name binder
/// wins. Giving `$id` precedence — or refusing a binder named after a fixed root — would be
/// narrower than the source. § 2.2 is clarified in the same correction to say that *"nothing else
/// changes"* governs addresses the binder does **not** match.
///
/// So `as: id` makes `$id` the bound element `"x"` inside the body, `eq: ["$id", "p-1"]` is false,
/// and the invariant does not hold. The pass-through half is pinned beside this one in
/// `service_semantics.rs`.
#[test]
fn a_quantifier_binder_named_after_a_fixed_root_shadows_it_inside_the_body() {
    let schema = json!({ "fields": { "tags": {
        "type": "array", "required": true, "items": { "type": "string" }
    }}});
    let condition = json!({ "for_all": {
        "in": "$fields.tags", "as": "id", "that": { "eq": ["$id", "p-1"] }
    }});
    assert_eq!(
        answer(schema, condition, json!({ "tags": ["x"] })),
        Truth::False,
        "`$id` is the bound element inside the body, which is the source's own rewrite"
    );
}

/// § 10.2.1: *"definition admission rejects unobservable numeric literals and bounds."*
///
/// `unobservable_literal` is reached from the `compare` arm only
/// (`crates/entity-core/src/validation.rs:1199-1201`), so the same literal in `eq`, `ne`, `in`,
/// `contains` or an ordering operator is admitted and its comparison is unevaluable at every
/// evaluation.
#[test]
fn an_unobservable_numeric_literal_is_refused_wherever_it_is_written() {
    let schema = json!({ "fields": { "x": { "type": "number", "required": true } } });
    let carries_unobservable = |defects: &entity_core::DefinitionErrors| {
        defects.iter().any(|defect| {
            matches!(defect, DefinitionError::InvalidRule { message, .. }
                if message.contains("source observation domain"))
        })
    };

    // The control: the mechanism exists and `compare` reaches it.
    let compared = ValidatedDefinition::new(definition(probe(
        schema.clone(),
        document(r#"{"compare": {"left": "$fields.x", "op": "eq", "right": 1e400}}"#),
    )))
    .expect_err("an unobservable literal is refused where it is written");
    assert!(
        carries_unobservable(&compared),
        "the compare arm names the domain: {compared}"
    );

    let equated = ValidatedDefinition::new(definition(probe(
        schema,
        document(r#"{"eq": ["$fields.x", 1e400]}"#),
    )))
    .expect_err("the same literal is the same defect in `eq`");
    assert!(
        carries_unobservable(&equated),
        "the eq arm names the domain: {equated}"
    );
}

/// § 10.6: an array ordinal is *"`0` or a digit string with no leading zero"*.
///
/// The run-time walk parses an ordinal with `str::parse::<usize>`
/// (`crates/entity-core/src/runtime.rs:2018`), which accepts a leading `+` that § 10.6's grammar
/// does not. This asserts that registration refuses the address first, so no registered definition
/// can reach the looser run-time reading — which is what makes the two readers' disagreement
/// unreachable rather than a defect.
#[test]
fn a_leading_plus_ordinal_is_refused_at_registration_so_the_runtime_reading_is_unreachable() {
    let schema = json!({ "fields": { "lines": {
        "type": "array", "required": true, "items": { "type": "integer" }
    }}});
    let defects = ValidatedDefinition::new(definition(probe(
        schema,
        json!({ "eq": ["$fields.lines.+1", 1] }),
    )))
    .expect_err("`+1` is not the ordinal grammar");
    assert!(
        defects.iter().any(|defect| matches!(
            defect,
            DefinitionError::InvalidTemplate { message, .. } | DefinitionError::InvalidRule { message, .. }
                if message.contains("+1")
        )),
        "the refusal names the segment: {defects}"
    );
}
