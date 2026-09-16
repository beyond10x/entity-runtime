//! The YAML front door and an authored exact numeric literal.
//!
//! § 10.2.1 retains an operand's **origin** while evaluating: a number reached through a reference
//! is read through the wire door, an authored numeric literal through the literal door, and the two
//! part company on exactly the decimals a binary64 cannot carry. That distinction lives in the
//! authored digits, so a front door that refuses them — or normalises them away — answers a
//! different question from the one the author wrote. `entity-cli` reads definitions from YAML, so
//! this is the door an authored `service/1` literal actually enters through.
//!
//! The duplicate-key pre-pass implemented `visit_i64`, `visit_u64` and `visit_f64` and not
//! `visit_i128`/`visit_u128`, and serde's default for those two answers `invalid_type`. Every
//! definition carrying an integer past the `u64` span was therefore refused — with the pre-pass's
//! own *"expected unambiguous YAML"*, which names nothing the author can act on.

use entity_core::EntityDefinition;
use serde_json::Value;

/// A definition whose one invariant compares `big` against an authored integer literal.
fn yaml(literal: &str) -> String {
    format!(
        r#"
entity: probe
version: 1
semantics: "service/1"
schema:
  fields:
    big:
      type: number
      required: true
lifecycle:
  initial: held
  states: [held]
invariants:
  - name: rule
    assert:
      compare:
        left: $fields.big
        op: eq
        right: {literal}
    message: the rule
operations: {{}}
"#
    )
}

/// The same definition as JSON, where `serde_json::from_str` retains the authored token.
fn json(literal: &str) -> String {
    format!(
        r#"{{
  "entity": "probe",
  "version": 1,
  "semantics": "service/1",
  "schema": {{ "fields": {{ "big": {{ "type": "number", "required": true }} }} }},
  "lifecycle": {{ "initial": "held", "states": ["held"] }},
  "invariants": [{{
    "name": "rule",
    "assert": {{ "compare": {{ "left": "$fields.big", "op": "eq", "right": {literal} }} }},
    "message": "the rule"
  }}],
  "operations": {{}}
}}"#
    )
}

fn from_yaml(literal: &str) -> EntityDefinition {
    entity_yaml::from_str(&yaml(literal)).expect("the YAML document is a definition")
}

fn from_json(literal: &str) -> EntityDefinition {
    serde_json::from_str(&json(literal)).expect("the JSON document is a definition")
}

fn serialized(definition: &EntityDefinition) -> Value {
    serde_json::to_value(definition).expect("the definition serializes")
}

/// What the two front doors made of one authored literal, as values that can be compared.
fn both_doors(literal: &str) -> (Value, Value) {
    (
        serialized(&from_yaml(literal)),
        serialized(&from_json(literal)),
    )
}

/// The authored literal, as the definition carries it.
fn literal_of(definition: &Value) -> String {
    definition
        .pointer("/invariants/0/assert/compare/right")
        .expect("the literal is where it was written")
        .to_string()
}

/// An authored integer the source carries exactly reaches the literal door with its own digits,
/// whichever front door read it — including the `i128`/`u128` span, which is where the reader used
/// to refuse the document outright.
#[test]
fn an_authored_integer_is_one_value_from_yaml_and_from_json_at_every_admitted_width() {
    for literal in [
        // inside the `u64`/`i64` span the reader already handled, as the control
        "0",
        "-1",
        "9007199254740993",
        "9223372036854775807",
        "-9223372036854775808",
        "18446744073709551615",
        // the first value past each of those spans, which is where `visit_u128`/`visit_i128` begin
        "18446744073709551616",
        "-9223372036854775809",
        // and the reviewer's own case: an integer no binary64 carries, whose last digit is the
        // whole subject of § 10.2.1's two doors
        "100000000000000000000000001",
        "-100000000000000000000000001",
        // the widest exact integers the two 128-bit carriers spell
        "170141183460469231731687303715884105727",
        "-170141183460469231731687303715884105728",
    ] {
        let (yaml_side, json_side) = both_doors(literal);
        assert_eq!(
            yaml_side, json_side,
            "one authored literal is one value, whichever front door read it: {literal}"
        );
        assert_eq!(
            literal_of(&yaml_side),
            literal,
            "the authored digits reach the literal door unrounded"
        );
    }
}

/// What the new arms must not take with them: the pre-pass exists to refuse mapping ambiguity, and
/// a scalar it now walks past is still only a leaf.
#[test]
fn the_duplicate_and_merge_key_refusals_still_hold_beside_a_wide_integer() {
    const WIDE: &str = "100000000000000000000000001";

    let duplicated = yaml(WIDE).replace("version: 1", "version: 1\nversion: 1");
    let error = entity_yaml::from_str(&duplicated).expect_err("a repeated key is ambiguous");
    assert!(
        error.to_string().contains("duplicate mapping key"),
        "{error}"
    );

    let merged = format!("{}<<: {{ operations: {{}} }}\n", yaml(WIDE));
    let error = entity_yaml::from_str(&merged).expect_err("a merge key is still refused");
    assert!(error.to_string().contains("merge keys"), "{error}");
}

/// An integer wider than either 128-bit carrier is still resolved to a binary64 by the YAML reader,
/// which is that reader's own limit and not this seam's. Pinned so the boundary is a measured fact
/// rather than an assumption: past it, the two front doors are **not** one value.
#[test]
fn an_integer_past_the_128_bit_carriers_is_still_read_as_a_float_and_says_so() {
    let literal = "1701411834604692317316873037158841057270";
    let (yaml_side, json_side) = both_doors(literal);
    assert_ne!(
        yaml_side, json_side,
        "past the carriers the YAML reader resolves the scalar to a binary64"
    );
    assert_eq!(literal_of(&json_side), literal);
}
