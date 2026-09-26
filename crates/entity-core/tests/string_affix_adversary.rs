//! Adversarial cases for `starts_with` / `ends_with` on the paths the unit's own tests do not
//! reach: the pre-load outcome selection a `service/N` routing rule runs through, a quantifier
//! binder as the haystack, and the `$$` literal escape.

use entity_core::{decide_before_load, DefinitionError, PreloadDecision, ValidatedDefinition};
use serde_json::{json, Value};

fn validated(value: Value) -> ValidatedDefinition {
    ValidatedDefinition::new(serde_json::from_value(value).expect("a definition document"))
        .expect("the fixture registers")
}

fn routed(guard: Value) -> ValidatedDefinition {
    validated(json!({
        "entity": "call", "version": 1, "semantics": "service/1",
        "schema": { "fields": { "note": { "type": "string" } } },
        "lifecycle": { "initial": "Held", "states": ["Held"] },
        "operations": { "Route": {
            "arguments": { "fields": {
                "caller": { "type": "string", "required": true },
                "phones": { "type": "array", "required": true, "items": { "type": "string" } }
            } },
            "outcomes": [
                { "name": "first", "when": guard, "refuses": { "error": "First" } },
                { "name": "later", "refuses": { "error": "Later" } }
            ]
        }}
    }))
}

fn outcome(decision: PreloadDecision<'_>) -> String {
    match decision {
        PreloadDecision::Refused(refusal) => refusal.outcome,
        PreloadDecision::Load(_) => "<load>".to_owned(),
    }
}

/// A guard over the subject cannot be answered before the subject is loaded, on either operand.
/// Kills the mutants `StartsWith/EndsWith => false` and `.any` → `.all` in
/// `condition_needs_subject` (`crates/entity-core/src/runtime.rs:2006-2011`): with either, the
/// guard is evaluated against empty fields, answers `Unknown`, and the pre-load decision is
/// `OutcomeUnobservable` instead of `Load`.
#[test]
fn a_prefix_or_suffix_guard_over_the_subject_waits_for_the_load() {
    for guard in [
        json!({ "starts_with": ["$fields.note", "+44"] }),
        json!({ "ends_with": ["$fields.note", "0"] }),
        json!({ "starts_with": ["+44 20", "$fields.note"] }),
        json!({ "ends_with": ["+44 20", "$fields.note"] }),
    ] {
        let definition = routed(guard.clone());
        let decision = decide_before_load(
            &definition,
            "c-1",
            "Route",
            json!({ "caller": "+44 20", "phones": [] }),
        );
        assert!(
            matches!(decision, Ok(PreloadDecision::Load(_))),
            "{guard}: expected Load, got {:?}",
            decision.map(outcome)
        );
    }
}

/// The ESS routing rule the operators exist for — "caller number starts with +44" — is decided
/// from the arguments alone, before any load.
#[test]
fn a_prefix_guard_over_the_arguments_routes_before_any_load() {
    let rows = [
        ("starts_with", "+44 20 7946", "+44", "first"),
        ("starts_with", "+49 30", "+44", "later"),
        ("starts_with", "0044 20", "+44", "later"),
        ("ends_with", "+44 20 7946", "7946", "first"),
        ("ends_with", "+44 20 7946", "+44", "later"),
    ];
    for (operator, caller, affix, expected) in rows {
        let definition = routed(json!({ operator: ["$args.caller", affix] }));
        let decision = decide_before_load(
            &definition,
            "c-1",
            "Route",
            json!({ "caller": caller, "phones": [] }),
        )
        .unwrap_or_else(|error| panic!("{operator} {caller}: {error}"));
        assert_eq!(outcome(decision), expected, "{operator} {caller} {affix}");
    }
}

/// A quantifier binder is an operand like any other: `for_any` over the argument list with a
/// prefix test on the bound element registers and decides before load.
#[test]
fn a_prefix_test_on_a_quantifier_binder_registers_and_decides() {
    let guard = json!({ "for_any": { "in": "$args.phones", "as": "p",
        "that": { "starts_with": ["$p", "+44"] } } });
    for (phones, expected) in [
        (json!([]), "later"),
        (json!(["+49 1"]), "later"),
        (json!(["+49 1", "+44 2"]), "first"),
    ] {
        let definition = routed(guard.clone());
        let decision = decide_before_load(
            &definition,
            "c-1",
            "Route",
            json!({ "caller": "x", "phones": phones }),
        )
        .unwrap_or_else(|error| panic!("{phones}: {error}"));
        assert_eq!(outcome(decision), expected, "{phones}");
    }
}

/// `$$` escapes a literal `$`, so a needle may itself begin with a dollar sign.
#[test]
fn an_escaped_dollar_is_a_literal_prefix_not_a_reference() {
    for (caller, expected) in [("$44 off", "first"), ("44 off", "later")] {
        let definition = routed(json!({ "starts_with": ["$args.caller", "$$44"] }));
        let decision = decide_before_load(
            &definition,
            "c-1",
            "Route",
            json!({ "caller": caller, "phones": [] }),
        )
        .unwrap_or_else(|error| panic!("{caller}: {error}"));
        assert_eq!(outcome(decision), expected, "{caller}");
    }
}

/// A literal operand that is not a string makes `starts_with`/`ends_with` `false` at every
/// evaluation — a rule that could never hold. `docs/design/kernel-v0.1.md:261` refuses such an
/// operand for `before`/`after` "like every other defect that could never work". It is reached by
/// the operator's own motivating rule written in YAML: `starts_with: [$fields.caller, +44]` reads
/// `+44` as the integer `44` (serde_yaml_ng), and the definition registers and refuses every
/// caller. Expected: `InvalidRule` at the literal's path.
#[test]
fn a_prefix_or_suffix_literal_that_is_not_a_string_is_refused_at_registration() {
    for semantics in ["kernel/1", "service/1"] {
        for (operator, literal) in [
            ("starts_with", json!(44)),
            ("ends_with", json!(44)),
            ("starts_with", json!(true)),
            ("ends_with", json!(null)),
        ] {
            let document = json!({
                "entity": "call", "version": 1, "semantics": semantics,
                "schema": { "fields": { "caller": { "type": "string", "required": true } } },
                "lifecycle": { "initial": "Held", "states": ["Held"] },
                "invariants": [{ "assert": { operator: ["$fields.caller", literal] } }]
            });
            let errors = ValidatedDefinition::new(
                serde_json::from_value(document).expect("a definition document"),
            )
            .err()
            .unwrap_or_else(|| {
                panic!("{semantics} {operator} {literal}: registered a rule that can never hold")
            });
            let expected = format!("invariants[0].assert.{operator}[1]");
            assert!(
                errors.iter().any(|defect| matches!(
                    defect,
                    DefinitionError::InvalidRule { path, .. } if path == &expected
                )),
                "{semantics} {operator} {literal}: {errors}"
            );
        }
    }
}
