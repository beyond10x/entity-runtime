//! Independent source examination, pass 2: the YAML front door and the `service/1` literal door.
//!
//! § 10.2.1 retains an operand's **origin** while evaluating: a number reached through a reference
//! is read through the wire door, an authored numeric literal through the literal door, and the
//! two part company on exactly the decimals a binary64 cannot carry. That distinction lives in the
//! authored token, so a front door that normalises the token away answers a different question
//! from the one the author wrote.

use entity_core::{create, CoreError, EntityDefinition, Truth, ValidatedDefinition};
use serde_json::Value;

/// The same `service/1` probe, authored in YAML.
const YAML: &str = r#"
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
        right: 100000000000000000000000001
    message: the rule
operations: {}
"#;

/// The same `service/1` probe, authored in JSON.
const JSON: &str = r#"{
  "entity": "probe",
  "version": 1,
  "semantics": "service/1",
  "schema": { "fields": { "big": { "type": "number", "required": true } } },
  "lifecycle": { "initial": "held", "states": ["held"] },
  "invariants": [{
    "name": "rule",
    "assert": { "compare": {
      "left": "$fields.big", "op": "eq", "right": 100000000000000000000000001
    }},
    "message": "the rule"
  }],
  "operations": {}
}"#;

fn document(text: &str) -> Value {
    // `serde_json::from_str` retains the authored token, which is the whole subject of § 10.2.1.
    serde_json::from_str(text).expect("the fixture is a JSON document")
}

/// What the probe's invariant answers about one instance.
fn answer(definition: EntityDefinition, fields: Value) -> Truth {
    let validated = ValidatedDefinition::new(definition).expect("the probe registers");
    match create(&validated, "p-1".to_owned(), fields) {
        Ok(_) => Truth::True,
        Err(CoreError::InvariantViolation { .. }) => Truth::False,
        Err(CoreError::InvariantUnobservable { .. }) => Truth::Unknown,
        Err(other) => panic!("the probe refused for another reason: {other}"),
    }
}

/// § 10.2.1: *"Operand origin is retained while evaluating: a number reached through a reference
/// uses `of_number`; an authored numeric literal uses `of_literal`"*, and its stored half: *"the
/// authored token, byte for byte … `arbitrary_precision` is what keeps it"*.
///
/// The field value is one JSON document in both halves; only the **definition's** front door
/// differs. `serde_yaml_ng` resolves a plain scalar it cannot hold as an `i64` or a `u64` to an
/// `f64`, and `serde_json::Number::from_f64` then stores that binary64's shortest round-tripping
/// decimal, so `100000000000000000000000001` reaches the literal door spelled `1e26` — the same
/// value the wire door reads the field as. The literal door's whole purpose is that those two are
/// different values; a YAML-authored definition cannot express the difference, and `entity-cli`
/// reads definitions from YAML.
///
/// The YAML reader itself is unchanged from the base `b2d000e` — `crates/entity-yaml/src/lib.rs`
/// and `crates/entity-core/src/number.rs` are not in this submission's diff — so the lossy decoding
/// is older than this unit. What is new is the per-operand door the lossy decoding now defeats.
#[test]
fn a_service_1_numeric_literal_is_one_value_from_yaml_and_from_json() {
    let fields = document(r#"{"big": 100000000000000000000000001}"#);
    let from_yaml = entity_yaml::from_str(YAML).expect("the YAML document is a definition");
    let from_json: EntityDefinition =
        serde_json::from_str(JSON).expect("the JSON document is a definition");

    // The reference answer, which is the one root's F1 disposition made the implementation give:
    // the literal door keeps `100000000000000000000000001` at scale zero, the wire door has no
    // exact carrier for the field's token and reads the binary64's canonical decimal, so the two
    // are not one value and `compare` answers false.
    assert_eq!(
        answer(from_json.clone(), fields.clone()),
        Truth::False,
        "compare observes each operand through its own door"
    );

    // The measurement: the two front doors read one authored literal.
    assert_eq!(
        serde_json::to_value(&from_yaml).expect("the definition serializes"),
        serde_json::to_value(&from_json).expect("the definition serializes"),
        "one authored literal is one value, whichever front door read it"
    );

    // And therefore one answer.
    assert_eq!(
        answer(from_yaml, fields),
        Truth::False,
        "a YAML-authored definition asks the question its author wrote"
    );
}
