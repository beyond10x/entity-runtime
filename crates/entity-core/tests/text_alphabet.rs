//! A `string` field or argument that declares its alphabet: membership per Unicode scalar value,
//! the refusals registration owes an alphabet it cannot honour, and the bytes a definition without
//! one keeps.

use entity_core::{
    create, decide, replay, CoreError, DefinitionError, EntityDefinition, EntityInstance,
    Evaluation, FieldDefinition, ValidatedDefinition, ValidationError,
};
use serde_json::{json, Value};

const KEYPAD: &str = "0123456789*#ABCD";

fn definition(value: Value) -> EntityDefinition {
    serde_json::from_value(value).expect("the fixture is a definition document")
}

fn validated(value: Value) -> ValidatedDefinition {
    ValidatedDefinition::new(definition(value)).expect("the fixture registers")
}

fn defects(value: Value) -> Vec<DefinitionError> {
    ValidatedDefinition::new(definition(value))
        .expect_err("the fixture is refused")
        .iter()
        .cloned()
        .collect()
}

/// A `service/1` definition whose schema is `fields` and whose one operation, `Dial`, takes
/// `arguments`.
fn probe(fields: Value, arguments: Value) -> Value {
    json!({
        "entity": "probe",
        "version": 1,
        "semantics": "service/1",
        "schema": { "fields": fields },
        "lifecycle": { "initial": "held", "states": ["held"] },
        "operations": { "Dial": {
            "transitions": [{ "from": "held", "to": "held" }],
            "arguments": { "fields": arguments }
        }}
    })
}

/// The probe with one required string field `code` over `alphabet`.
fn code_over(alphabet: &str) -> ValidatedDefinition {
    validated(probe(
        json!({ "code": { "type": "string", "required": true, "alphabet": alphabet } }),
        json!({}),
    ))
}

fn validation_errors(definition: &ValidatedDefinition, fields: Value) -> Vec<ValidationError> {
    match create(definition, "p-1".to_owned(), fields) {
        Err(CoreError::Validation(errors)) => errors,
        other => panic!("expected a validation refusal, got {other:?}"),
    }
}

fn outside(path: &str, character: char, position: usize) -> ValidationError {
    ValidationError::new(
        path,
        format!(
            "character {character:?} (U+{:04X}) at position {position} is not in the alphabet",
            u32::from(character)
        ),
    )
}

// --- Value validation ----------------------------------------------------------------------------

#[test]
fn a_text_made_only_of_its_alphabet_is_admitted() {
    let decision = create(
        &code_over(KEYPAD),
        "p-1".to_owned(),
        json!({ "code": "12#A*" }),
    )
    .expect("every character is in the alphabet");
    assert_eq!(decision.instance.fields["code"], json!("12#A*"));
}

#[test]
fn a_text_with_two_characters_outside_its_alphabet_is_one_error_naming_the_path_and_the_first() {
    // Two violations, `x` at position 2 and `y` at position 4: one error for the value, and it
    // names the first one, so a caller can say which character to remove without re-scanning.
    assert_eq!(
        validation_errors(&code_over(KEYPAD), json!({ "code": "1x2y" })),
        vec![outside("fields.code", 'x', 2)]
    );
}

#[test]
fn the_empty_text_satisfies_every_alphabet() {
    for alphabet in [KEYPAD, "a", "e\u{301}"] {
        create(
            &code_over(alphabet),
            "p-1".to_owned(),
            json!({ "code": "" }),
        )
        .unwrap_or_else(|error| panic!("{alphabet:?} refused the empty text: {error}"));
    }
}

#[test]
fn membership_is_per_unicode_scalar_value_with_no_normalization() {
    let definition = code_over("e\u{301}");
    create(&definition, "p-1".to_owned(), json!({ "code": "e\u{301}" }))
        .expect("the decomposed e-acute is two members of the alphabet");
    assert_eq!(
        validation_errors(&definition, json!({ "code": "\u{e9}" })),
        vec![outside("fields.code", '\u{e9}', 1)],
        "the composed e-acute is one scalar value the alphabet does not hold"
    );
    assert_eq!(
        validation_errors(&definition, json!({ "code": "e\u{301}\u{e9}" })),
        vec![outside("fields.code", '\u{e9}', 3)],
        "positions count scalar values, not bytes or graphemes"
    );
}

#[test]
fn membership_does_no_case_folding() {
    assert_eq!(
        validation_errors(&code_over(KEYPAD), json!({ "code": "12a" })),
        vec![outside("fields.code", 'a', 3)]
    );
    assert_eq!(
        validation_errors(&code_over("abc"), json!({ "code": "aBc" })),
        vec![outside("fields.code", 'B', 2)]
    );
}

#[test]
fn an_alphabet_violation_accumulates_with_the_other_errors_of_the_object() {
    let definition = validated(probe(
        json!({
            "code": { "type": "string", "required": true, "alphabet": KEYPAD, "max_length": 3 },
            "label": { "type": "string", "required": true, "alphabet": "ab" },
            "count": { "type": "integer", "required": true }
        }),
        json!({}),
    ));
    assert_eq!(
        validation_errors(
            &definition,
            json!({ "code": "12345z", "label": "abc", "count": "three" })
        ),
        vec![
            ValidationError::new("fields.code", "length 6 exceeds maximum 3"),
            outside("fields.code", 'z', 6),
            ValidationError::new("fields.count", "expected integer"),
            outside("fields.label", 'c', 3),
        ]
    );
}

#[test]
fn an_alphabet_is_checked_at_every_depth_a_string_is() {
    let definition = validated(probe(
        json!({
            "keys": { "type": "array", "items": { "type": "string", "alphabet": KEYPAD } },
            "dial": { "type": "object", "properties": {
                "prefix": { "type": "string", "alphabet": "+0123456789" }
            }},
            "named": { "type": "map", "key": "string", "items": { "type": "string", "alphabet": "01" } }
        }),
        json!({}),
    ));
    assert_eq!(
        validation_errors(
            &definition,
            json!({
                "keys": ["12", "1-2"],
                "dial": { "prefix": "+49 " },
                "named": { "bit": "2" }
            })
        ),
        vec![
            outside("fields.dial.prefix", ' ', 4),
            outside("fields.keys[1]", '-', 2),
            outside("fields.named.bit", '2', 1),
        ]
    );
}

#[test]
fn an_argument_declaring_an_alphabet_refuses_a_value_outside_it_with_the_argument_path() {
    let definition = validated(probe(
        json!({}),
        json!({ "keys": { "type": "string", "required": true, "alphabet": KEYPAD } }),
    ));
    let instance = EntityInstance {
        entity: "probe".into(),
        version: 1,
        id: "p-1".into(),
        lifecycle_state: "held".into(),
        revision: 1,
        fields: serde_json::Map::new(),
    };
    assert!(matches!(
        decide(&definition, &instance, "Dial", json!({ "keys": "12#" })),
        Ok(Evaluation::Accepted(_))
    ));
    assert_eq!(
        decide(&definition, &instance, "Dial", json!({ "keys": "1x2y" })),
        Err(CoreError::Validation(vec![outside(
            "arguments.keys",
            'x',
            2
        )]))
    );
}

#[test]
fn a_decision_recorded_under_an_alphabet_replays_and_keeps_it_in_the_snapshot() {
    let definition = code_over(KEYPAD);
    let decision = create(&definition, "p-1".to_owned(), json!({ "code": "#1" })).expect("decides");
    let snapshot = serde_json::to_value(&decision.record).expect("serializes");
    assert_eq!(
        snapshot["definition"]["schema"]["fields"]["code"]["alphabet"],
        json!(KEYPAD)
    );
    let rebuilt = replay(std::slice::from_ref(&decision.record)).expect("replays");
    assert_eq!(rebuilt, decision.instance);
}

// --- Registration --------------------------------------------------------------------------------

#[test]
fn an_alphabet_on_a_kind_other_than_string_is_refused_at_registration_with_its_path() {
    for (kind, field, path) in [
        (
            "integer",
            json!({ "type": "integer", "alphabet": "0123456789" }),
            "schema.f",
        ),
        (
            "enum",
            json!({ "type": "enum", "values": ["red"], "alphabet": "der" }),
            "schema.f",
        ),
        (
            "ref",
            json!({ "type": "ref", "entity": "probe", "alphabet": "ab" }),
            "schema.f",
        ),
        (
            "json",
            json!({ "type": "json", "alphabet": "ab" }),
            "schema.f",
        ),
        (
            "array",
            json!({ "type": "array", "items": { "type": "string" }, "alphabet": "ab" }),
            "schema.f",
        ),
        (
            "integer",
            json!({ "type": "array", "items": { "type": "integer", "alphabet": "01" } }),
            "schema.f[]",
        ),
    ] {
        assert_eq!(
            defects(probe(json!({ "f": field }), json!({}))),
            vec![DefinitionError::ConstraintNotApplicable {
                path: path.to_owned(),
                constraint: "alphabet",
                kind,
                applies_to: "a string field",
            }],
            "{kind} at {path}"
        );
    }
    assert_eq!(
        defects(probe(
            json!({}),
            json!({ "n": { "type": "boolean", "alphabet": "tf" } })
        )),
        vec![DefinitionError::ConstraintNotApplicable {
            path: "operations.Dial.arguments.n".to_owned(),
            constraint: "alphabet",
            kind: "boolean",
            applies_to: "a string field",
        }]
    );
}

#[test]
fn an_empty_alphabet_is_refused_at_registration() {
    assert_eq!(
        defects(probe(
            json!({ "code": { "type": "string", "alphabet": "" } }),
            json!({})
        )),
        vec![DefinitionError::InvalidField {
            path: "schema.code".to_owned(),
            message: "alphabet must declare at least one character".to_owned(),
        }]
    );
}

#[test]
fn an_alphabet_repeating_a_character_is_refused_naming_it_and_both_positions() {
    assert_eq!(
        defects(probe(
            json!({}),
            json!({ "keys": { "type": "string", "alphabet": "AB#A\u{301}\u{301}" } })
        )),
        vec![
            DefinitionError::InvalidField {
                path: "operations.Dial.arguments.keys".to_owned(),
                message: "alphabet writes 'A' twice, at positions 1 and 4".to_owned(),
            },
            DefinitionError::InvalidField {
                path: "operations.Dial.arguments.keys".to_owned(),
                message: "alphabet writes '\\u{301}' twice, at positions 5 and 6".to_owned(),
            },
        ]
    );
    // Two spellings of one letter are two scalar values, so neither repeats the other.
    validated(probe(
        json!({ "letter": { "type": "string", "alphabet": "\u{e9}e\u{301}" } }),
        json!({}),
    ));
}

#[test]
fn a_default_outside_its_alphabet_is_refused_at_registration() {
    assert_eq!(
        defects(probe(
            json!({ "code": { "type": "string", "alphabet": KEYPAD, "default": "12x" } }),
            json!({ "keys": { "type": "string", "alphabet": KEYPAD, "default": "#a" } }),
        )),
        vec![
            DefinitionError::InvalidField {
                path: "schema.code".to_owned(),
                message: "invalid default: character 'x' (U+0078) at position 3 is not in the \
                          alphabet"
                    .to_owned(),
            },
            DefinitionError::InvalidField {
                path: "operations.Dial.arguments.keys".to_owned(),
                message: "invalid default: character 'a' (U+0061) at position 2 is not in the \
                          alphabet"
                    .to_owned(),
            },
        ]
    );
    validated(probe(
        json!({ "code": { "type": "string", "alphabet": KEYPAD, "default": "" } }),
        json!({}),
    ));
}

#[test]
fn an_alphabet_is_a_service_key_that_a_kernel_1_definition_refuses() {
    let mut document = probe(
        json!({ "code": { "type": "string", "alphabet": KEYPAD } }),
        json!({}),
    );
    for semantics in ["service/1", "service/2", "service/3"] {
        document["semantics"] = json!(semantics);
        validated(document.clone());
    }
    document
        .as_object_mut()
        .expect("an object")
        .remove("semantics");
    assert_eq!(
        defects(document),
        vec![DefinitionError::SemanticsKeyNotAvailable {
            path: "schema.code.alphabet".to_owned(),
            key: "alphabet".to_owned(),
        }]
    );
}

// --- Document bytes ------------------------------------------------------------------------------

#[test]
fn a_field_without_an_alphabet_serializes_to_the_bytes_it_did_before() {
    // Decision records embed the definition, so a key that appeared as `"alphabet":null` would
    // change the bytes of every record ever written.
    assert_eq!(
        serde_json::to_string(&FieldDefinition::default()).expect("serializes"),
        r#"{"type":"string","required":false,"min_length":null,"max_length":null,"min":null,"max":null,"values":[],"items":null,"properties":{},"additional_properties":false,"entity":null,"inverse":null,"acyclic":null}"#
    );
}

#[test]
fn the_alphabet_key_is_closed_like_every_other_field_key() {
    let error = serde_json::from_value::<FieldDefinition>(json!({
        "type": "string", "alphabets": "ab"
    }))
    .expect_err("a misspelled key is refused");
    assert!(
        error.to_string().contains("unknown field `alphabets`"),
        "{error}"
    );
    let field: FieldDefinition =
        serde_json::from_value(json!({ "type": "string", "alphabet": "ab" })).expect("parses");
    assert_eq!(
        serde_json::to_value(&field).expect("serializes")["alphabet"],
        json!("ab")
    );
}
