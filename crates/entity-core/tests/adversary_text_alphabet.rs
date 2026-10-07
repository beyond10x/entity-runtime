//! Adversarial cases against a text's `alphabet` (R-162, R-163): every place a `string`
//! declaration is read — union variants, defaults at depth, `set` results, conditional targets,
//! `kernel/1` paths below the top level, a declared response (R-169) — and the cost of checking
//! many values against one large alphabet.

use std::time::{Duration, Instant};

use entity_core::{
    create, decide, CoreError, DefinitionError, EntityDefinition, EntityInstance, Evaluation,
    ValidatedDefinition, ValidationError,
};
use serde_json::{json, Value};

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

fn held(entity: &str) -> EntityInstance {
    EntityInstance {
        entity: entity.to_owned(),
        version: 1,
        id: "p-1".to_owned(),
        lifecycle_state: "held".to_owned(),
        revision: 1,
        fields: serde_json::Map::new(),
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

fn invalid_default(path: &str, character: char, position: usize) -> DefinitionError {
    DefinitionError::InvalidField {
        path: path.to_owned(),
        message: format!(
            "invalid default: {}",
            outside(path, character, position).message
        ),
    }
}

// --- A declared response -------------------------------------------------------------------------

/// A `service/1` keypad whose one operation answers the keys it was sent through the response
/// field `echo`, declared as `echo`.
fn echoing(echo: Value) -> ValidatedDefinition {
    validated(json!({
        "entity": "keypad",
        "version": 1,
        "semantics": "service/1",
        "schema": { "fields": {} },
        "lifecycle": { "initial": "held", "states": ["held"] },
        "operations": { "Dial": {
            "arguments": { "fields": { "keys": { "type": "string", "required": true } } },
            "response": { "fields": { "echo": echo } },
            "outcomes": [{
                "name": "dialled",
                "effect": { "moves": { "from": "held", "to": "held" } },
                "responds": { "echo": "$args.keys" }
            }]
        }}
    }))
}

/// The response the branch answered: exactly `echo` holding `keys`.
fn echoed(keys: &str) -> Option<serde_json::Map<String, Value>> {
    json!({ "echo": keys }).as_object().cloned()
}

/// Design `service-semantics-v0.1.md` § 5.3: the response is "a template map resolved at step 14
/// and validated against `response`". An alphabet on a response field holds the value the branch
/// answers, and a value outside it refuses the decision with the response path (R-169).
#[test]
fn a_response_outside_its_declared_alphabet_is_refused_with_its_response_path() {
    let definition =
        echoing(json!({ "type": "string", "required": true, "alphabet": "0123456789" }));
    assert_eq!(
        decide(
            &definition,
            &held("keypad"),
            "Dial",
            json!({ "keys": "12x" })
        ),
        Err(CoreError::Validation(vec![outside(
            "response.echo",
            'x',
            3
        )]))
    );
    match decide(
        &definition,
        &held("keypad"),
        "Dial",
        json!({ "keys": "12" }),
    ) {
        Ok(Evaluation::Accepted(decision)) => assert_eq!(decision.record.response, echoed("12")),
        other => panic!("a response inside its alphabet is answered: {other:?}"),
    }
}

/// The same check with a constraint the base already had: the alphabet inherits the check rather
/// than introducing it.
#[test]
fn a_response_longer_than_its_declared_max_length_is_refused_with_its_response_path() {
    let definition = echoing(json!({ "type": "string", "required": true, "max_length": 2 }));
    assert_eq!(
        decide(
            &definition,
            &held("keypad"),
            "Dial",
            json!({ "keys": "12345" })
        ),
        Err(CoreError::Validation(vec![ValidationError::new(
            "response.echo",
            "length 5 exceeds maximum 2"
        )]))
    );
}

// --- Cost ----------------------------------------------------------------------------------------

/// The first `count` ideographs of CJK Extension B (U+20000 onward), every one a distinct scalar
/// value and none a surrogate.
fn ideographs(count: u32) -> String {
    (0x2_0000..0x2_0000 + count)
        .map(|code| char::from_u32(code).expect("a scalar value"))
        .collect()
}

/// The fastest of `runs` creations of `values` one-character texts, each `U+20000`, in an array
/// whose items declare `alphabet`. Registration is outside the measured region.
fn fastest_creation(alphabet: &str, values: usize, runs: usize) -> Duration {
    let definition = validated(json!({
        "entity": "probe",
        "version": 1,
        "semantics": "service/1",
        "schema": { "fields": { "keys": {
            "type": "array", "required": true,
            "items": { "type": "string", "alphabet": alphabet }
        }}},
        "lifecycle": { "initial": "held", "states": ["held"] }
    }));
    let fields = json!({ "keys": vec!["\u{20000}"; values] });
    (0..runs)
        .map(|_| {
            let fields = fields.clone();
            let started = Instant::now();
            let decision = create(&definition, "p-1".to_owned(), fields)
                .expect("every value is in the alphabet");
            let elapsed = started.elapsed();
            drop(decision);
            elapsed
        })
        .min()
        .expect("at least one run")
}

/// An alphabet is the author's and has no length bound (ESS `string-alphabet-and-length.md` § 1);
/// the number of values is the caller's. Checking a value is one membership test per character,
/// so a thousand one-character values cost about the same whether the alphabet holds one character
/// or forty thousand, give or take building the set once. Rebuilding the alphabet's set for every
/// value makes the cost the product of the caller's count and the author's alphabet. The margin
/// covers one set build in an unoptimized build; the product exceeds it in an optimized one too.
#[test]
fn checking_many_values_against_a_large_alphabet_costs_about_what_a_small_one_does() {
    let small = fastest_creation(&ideographs(1), 1_000, 3);
    let large = fastest_creation(&ideographs(40_000), 1_000, 2);
    assert!(
        large <= small * 10 + Duration::from_millis(30),
        "1,000 one-character values took {large:?} against a 40,000-character alphabet and \
         {small:?} against a 1-character one"
    );
}

// --- Every place a string declaration is read ----------------------------------------------------

#[test]
fn a_union_variant_payload_is_checked_against_its_alphabet() {
    let definition = validated(json!({
        "entity": "probe",
        "version": 1,
        "semantics": "service/1",
        "schema": { "fields": { "signal": { "type": "union", "tag": "kind", "variants": {
            "tone": { "type": "string", "required": true, "alphabet": "0123456789*#" },
            "pause": { "type": "integer", "required": true }
        }}}},
        "lifecycle": { "initial": "held", "states": ["held"] }
    }));
    create(
        &definition,
        "p-1".to_owned(),
        json!({ "signal": { "kind": "tone", "value": "1#" } }),
    )
    .expect("every character is in the variant's alphabet");
    assert_eq!(
        create(
            &definition,
            "p-1".to_owned(),
            json!({ "signal": { "kind": "tone", "value": "1x" } })
        ),
        Err(CoreError::Validation(vec![outside(
            "fields.signal.value",
            'x',
            2
        )]))
    );

    let refused = defects(json!({
        "entity": "probe",
        "version": 1,
        "semantics": "service/1",
        "schema": { "fields": { "signal": { "type": "union", "tag": "kind", "variants": {
            "tone": { "type": "string", "alphabet": "0123456789", "default": "1x" },
            "pause": { "type": "integer", "alphabet": "01" }
        }}}},
        "lifecycle": { "initial": "held", "states": ["held"] }
    }));
    assert!(
        refused.contains(&invalid_default("schema.signal|tone", 'x', 2)),
        "{refused:?}"
    );
    assert!(
        refused.contains(&DefinitionError::ConstraintNotApplicable {
            path: "schema.signal|pause".to_owned(),
            constraint: "alphabet",
            kind: "integer",
            applies_to: "a string field",
        }),
        "{refused:?}"
    );
}

#[test]
fn a_default_at_any_depth_is_checked_against_the_alphabet_it_reaches() {
    let refused = defects(json!({
        "entity": "probe",
        "version": 1,
        "semantics": "service/1",
        "schema": { "fields": {
            "dial": { "type": "object", "default": { "prefix": "x" }, "properties": {
                "prefix": { "type": "string", "alphabet": "+0123456789" }
            }},
            "keys": { "type": "array", "default": ["1", "2x"],
                      "items": { "type": "string", "alphabet": "0123456789" } },
            "named": { "type": "map", "key": "string", "default": { "a": "0x" },
                       "items": { "type": "string", "alphabet": "01" } }
        }},
        "lifecycle": { "initial": "held", "states": ["held"] }
    }));
    for expected in [
        invalid_default("schema.dial.prefix", 'x', 1),
        invalid_default("schema.keys[1]", 'x', 2),
        invalid_default("schema.named.a", 'x', 2),
    ] {
        assert!(refused.contains(&expected), "{expected:?} in {refused:?}");
    }
}

#[test]
fn a_set_copying_an_unconstrained_argument_into_an_alphabet_field_is_refused_after_the_set() {
    let definition = validated(json!({
        "entity": "probe",
        "version": 1,
        "semantics": "service/1",
        "schema": { "fields": { "code": { "type": "string", "alphabet": "01" } } },
        "lifecycle": { "initial": "held", "states": ["held"] },
        "operations": { "Write": {
            "transitions": [{ "from": "held", "to": "held" }],
            "arguments": { "fields": { "code": { "type": "string", "required": true } } },
            "set": { "code": "$args.code" }
        }}
    }));
    assert!(matches!(
        decide(
            &definition,
            &held("probe"),
            "Write",
            json!({ "code": "0110" })
        ),
        Ok(Evaluation::Accepted(_))
    ));
    assert_eq!(
        decide(
            &definition,
            &held("probe"),
            "Write",
            json!({ "code": "012" })
        ),
        Err(CoreError::Validation(vec![outside("fields.code", '2', 3)]))
    );
}

/// `validate_conditional_target` compares the whole source leaf with the target, so an alphabet on
/// one side only is a different declaration.
#[test]
fn a_conditional_target_must_carry_its_source_leafs_alphabet() {
    let document = |target: Value| {
        json!({
            "entity": "probe",
            "version": 1,
            "semantics": "service/2",
            "schema": { "fields": { "code": target } },
            "lifecycle": { "initial": "held", "states": ["held"] },
            "create": {
                "arguments": { "fields": { "code": { "type": "string", "alphabet": "01" } } },
                "outcomes": [{
                    "name": "made",
                    "effect": "creates",
                    "set_if_present": { "code": { "argument": "code" } }
                }]
            }
        })
    };
    validated(document(json!({ "type": "string", "alphabet": "01" })));
    let refused = defects(document(json!({ "type": "string" })));
    assert!(
        refused.iter().any(|defect| matches!(
            defect,
            DefinitionError::ConditionalTargetInvalid { field, message, .. }
                if field == "code" && message.contains("complete source leaf definition")
        )),
        "{refused:?}"
    );
}

#[test]
fn a_kernel_1_alphabet_is_refused_at_every_depth_with_its_own_path() {
    let refused = defects(json!({
        "entity": "probe",
        "version": 1,
        "schema": { "fields": {
            "o": { "type": "object", "properties": { "p": { "type": "string", "alphabet": "ab" } } },
            "a": { "type": "array", "items": { "type": "string", "alphabet": "ab" } }
        }},
        "lifecycle": { "initial": "held", "states": ["held"] },
        "operations": { "Op": {
            "transitions": [{ "from": "held", "to": "held" }],
            "arguments": { "fields": { "x": { "type": "string", "alphabet": "ab" } } }
        }}
    }));
    for path in [
        "schema.a[].alphabet",
        "schema.o.p.alphabet",
        "operations.Op.arguments.x.alphabet",
    ] {
        assert!(
            refused.contains(&DefinitionError::SemanticsKeyNotAvailable {
                path: path.to_owned(),
                key: "alphabet".to_owned(),
            }),
            "{path} in {refused:?}"
        );
    }
}

// --- The per-call membership cache (correction round 1) ------------------------------------------

/// One `service/1` definition whose single validation call meets seven alphabet declarations, two
/// of them spelled alike: object properties repeated across array elements, union variants
/// repeated across array elements, a map's values, and a top-level field.
fn many_declarations() -> Value {
    json!({
        "entity": "probe",
        "version": 1,
        "semantics": "service/1",
        "schema": { "fields": {
            "rows": { "type": "array", "required": true, "items": {
                "type": "object", "properties": {
                    "a": { "type": "string", "alphabet": "01" },
                    "b": { "type": "string", "alphabet": "ab" }
                }
            }},
            "signals": { "type": "array", "required": true, "items": {
                "type": "union", "tag": "kind", "variants": {
                    "tone": { "type": "string", "required": true, "alphabet": "*#" },
                    "word": { "type": "string", "required": true, "alphabet": "xyz" }
                }
            }},
            "named": { "type": "map", "key": "string", "required": true,
                       "items": { "type": "string", "alphabet": "01" } },
            "plain": { "type": "string", "required": true, "alphabet": "ab" }
        }},
        "lifecycle": { "initial": "held", "states": ["held"] }
    })
}

fn crossed_values() -> Value {
    json!({
        "rows": [
            { "a": "0", "b": "a" },
            { "a": "a", "b": "0" },
            { "a": "1", "b": "b" },
            { "a": "b", "b": "1" }
        ],
        "signals": [
            { "kind": "tone", "value": "*x" },
            { "kind": "word", "value": "x*" },
            { "kind": "tone", "value": "#" }
        ],
        "named": { "k": "0a" },
        "plain": "ab0"
    })
}

/// Each value is answered from its own declaration's set however many declarations the call has
/// already cached, and in the order and number the uncached check produced.
#[test]
fn every_declaration_met_in_one_call_answers_from_its_own_alphabet() {
    let definition = validated(many_declarations());
    let errors = match create(&definition, "p-1".to_owned(), crossed_values()) {
        Err(CoreError::Validation(errors)) => errors,
        other => panic!("expected a validation refusal, got {other:?}"),
    };
    assert_eq!(
        errors,
        vec![
            outside("fields.named.k", 'a', 2),
            outside("fields.plain", '0', 3),
            outside("fields.rows[1].a", 'a', 1),
            outside("fields.rows[1].b", '0', 1),
            outside("fields.rows[3].a", 'b', 1),
            outside("fields.rows[3].b", '1', 1),
            outside("fields.signals[0].value", 'x', 2),
            outside("fields.signals[1].value", '*', 2),
        ]
    );
}

/// A default is validated in its own call; two sibling declarations met there keep their own sets.
#[test]
fn a_default_meeting_two_alphabets_names_each_offence_from_its_own() {
    let refused = defects(json!({
        "entity": "probe",
        "version": 1,
        "semantics": "service/1",
        "schema": { "fields": {
            "dial": { "type": "object", "default": { "a": "2", "b": "a1" }, "properties": {
                "a": { "type": "string", "alphabet": "01" },
                "b": { "type": "string", "alphabet": "ab" }
            }}
        }},
        "lifecycle": { "initial": "held", "states": ["held"] }
    }));
    assert_eq!(
        refused,
        vec![
            invalid_default("schema.dial.a", '2', 1),
            invalid_default("schema.dial.b", '1', 2),
        ]
    );
}

/// The cache is keyed by where a declaration sits in memory. A clone sits elsewhere, so it must
/// decide to the same record bytes and refuse with the same errors as the original, call after call.
#[test]
fn where_a_definition_sits_in_memory_changes_no_answer_and_no_byte() {
    let original = validated(many_declarations());
    let copy = original.clone();
    let rebuilt = validated(many_declarations());
    let refusal = |definition: &ValidatedDefinition| {
        create(definition, "p-1".to_owned(), crossed_values()).expect_err("refused")
    };
    let accepted = json!({
        "rows": [{ "a": "01", "b": "ba" }],
        "signals": [{ "kind": "word", "value": "zyx" }],
        "named": { "k": "10" },
        "plain": "b"
    });
    let bytes = |definition: &ValidatedDefinition| {
        let decision =
            create(definition, "p-1".to_owned(), accepted.clone()).expect("every value is inside");
        serde_json::to_vec(&decision.record).expect("serializes")
    };
    for other in [&copy, &rebuilt, &original] {
        assert_eq!(refusal(other), refusal(&original));
        assert_eq!(bytes(other), bytes(&original));
    }
}
