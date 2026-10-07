//! Adversarial cases for `story:set-increments-a-numeric-field` (R-164).
//!
//! Each case asserts something the unit's own documents state, or a guard the unit's suite leaves
//! unpinned. None of them changes an implementation file.

use entity_core::{
    decide, rehydrate, CoreError, DefinitionError, EntityDefinition, EntityInstance, Evaluation,
    Registry, Runtime, ValidatedDefinition,
};
use serde_json::{json, Value};

fn definition(document: Value) -> Result<ValidatedDefinition, Vec<DefinitionError>> {
    let definition: EntityDefinition =
        serde_json::from_value(document).expect("a definition document");
    ValidatedDefinition::new(definition).map_err(|errors| errors.as_slice().to_vec())
}

fn instance(entity: &str, fields: &str) -> EntityInstance {
    serde_json::from_str(&format!(
        r#"{{"entity":"{entity}","version":1,"id":"x-1","lifecycle_state":"open","revision":1,"fields":{fields}}}"#
    ))
    .expect("an instance document")
}

fn repository_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates/")
        .parent()
        .expect("repository root")
        .to_path_buf()
}

// --- Contract drift: the normative kernel/1 evaluation order ---------------------------------------

/// `kernel-v0.1.md` § 6 is the twelve-step contract (R-70, AGENTS.md invariant 8), and it names the
/// refusal each step returns. A `kernel/1` `set` now also refuses as `IncrementOverflow`; the
/// runtime's step table and `service-semantics-v0.1.md` § 4.2 say so, the kernel page does not.
#[test]
fn the_kernel_1_evaluation_order_names_increment_overflow_at_the_set_step() {
    let counter = definition(json!({
        "entity": "counter",
        "version": 1,
        "schema": { "fields": { "hits": { "type": "integer", "required": true } } },
        "lifecycle": { "initial": "open", "states": ["open"] },
        "operations": { "bump": {
            "transitions": [{ "from": "open", "to": "open" }],
            "set": { "hits": { "increment": 1 } }
        }}
    }))
    .expect("a kernel/1 increment registers");
    let error = decide(
        &counter,
        &instance("counter", r#"{"hits":18446744073709551615}"#),
        "bump",
        json!({}),
    )
    .expect_err("u64::MAX + 1 is no kernel/1 integer");
    assert!(
        matches!(error, CoreError::IncrementOverflow { .. }),
        "{error}"
    );

    let design = std::fs::read_to_string(repository_root().join("docs/design/kernel-v0.1.md"))
        .expect("readable");
    let order = design
        .split("## 6. Evaluation order")
        .nth(1)
        .expect("the section exists");
    let set_step: Vec<&str> = order
        .lines()
        .skip_while(|line| !line.starts_with(" 6."))
        .take_while(|line| !line.starts_with(" 7."))
        .collect();
    assert!(
        set_step
            .iter()
            .any(|line| line.contains("IncrementOverflow")),
        "kernel-v0.1.md § 6 names only {set_step:?} at the set step, but a kernel/1 set now \
         refuses as IncrementOverflow there"
    );
}

// --- Contract drift: the CHANGELOG's refusal for the reserved shape --------------------------------

/// The CHANGELOG's `### Changed` entry tells a user whose `object` or `json` field was written with
/// the one-key `{increment: …}` literal that registration now refuses it as
/// `increment_target_invalid`, and on a creation outcome as `increment_on_create`: a creation
/// branch is refused before its target field is looked at.
#[test]
fn the_reserved_one_key_literal_on_a_json_field_of_a_creation_is_refused_as_increment_on_create() {
    let document = json!({
        "entity": "probe",
        "version": 1,
        "semantics": "service/1",
        "schema": { "fields": { "note": { "type": "json" } } },
        "lifecycle": { "initial": "open", "states": ["open"] },
        "create": { "outcomes": [{
            "name": "opened",
            "effect": "creates",
            "set": { "note": { "increment": 1 } }
        }]}
    });
    let changelog =
        std::fs::read_to_string(repository_root().join("CHANGELOG.md")).expect("readable");
    assert!(
        changelog.contains("`json` field can no longer be written with that one literal mapping")
            && changelog.contains("registration refuses it as `increment_target_invalid`")
            && changelog.contains("on a creation outcome as\n  `increment_on_create`"),
        "the CHANGELOG entry this case reads has moved or no longer names the creation refusal"
    );
    let errors = definition(document).expect_err("the shape is reserved");
    let kinds: Vec<&str> = errors.iter().map(DefinitionError::kind).collect();
    assert_eq!(
        kinds,
        ["increment_on_create"],
        "a json field written with the reserved literal on a creation branch: {errors:?}"
    );
}

// --- A guard the unit's suite does not pin ---------------------------------------------------------

/// `present_numeric_kind` fills defaults for an argument path only: a `$fields` member that is
/// optional with a default may be absent on an instance (a `service/3` branch's `fulfills` can
/// remove an optional field, and `{cleared: true}` is the next unit), so it is no always-present
/// amount. The unit's refusal list names `$fields.maybe` (optional, no default) and never a
/// defaulted optional field, so treating defaults as filling `$fields` too passes its suite.
#[test]
fn an_increment_amount_reading_an_optional_field_with_a_default_is_refused_at_registration() {
    let errors = definition(json!({
        "entity": "counter",
        "version": 1,
        "schema": { "fields": {
            "hits":  { "type": "integer", "required": true },
            "bonus": { "type": "integer", "default": 1 }
        }},
        "lifecycle": { "initial": "open", "states": ["open"] },
        "operations": { "bump": {
            "transitions": [{ "from": "open", "to": "open" }],
            "set": { "hits": { "increment": "$fields.bonus" } }
        }}
    }))
    .expect_err("an optional field may be absent, default or not");
    assert!(
        matches!(errors.as_slice(), [DefinitionError::IncrementAmountInvalid { path, .. }]
            if path == "operations.bump.set.hits.increment"),
        "{errors:?}"
    );
}

// --- Properties: the exact sum, its spelling, and the fold ----------------------------------------

/// A fixed-seed linear congruential generator, so every run draws the same cases.
struct Draw(u64);

impl Draw {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 11
    }

    fn below(&mut self, bound: u64) -> u64 {
        self.next() % bound
    }
}

/// `units × 10^-scale`, spelled positionally with no trailing fractional zero and no `-0`.
fn spelled(mut units: i128, mut scale: u32) -> String {
    if units == 0 {
        return "0".to_owned();
    }
    while scale > 0 && units % 10 == 0 {
        units /= 10;
        scale -= 1;
    }
    let sign = if units < 0 { "-" } else { "" };
    let digits = units.unsigned_abs().to_string();
    if scale == 0 {
        return format!("{sign}{digits}");
    }
    let scale = scale as usize;
    let digits = if digits.len() <= scale {
        format!("{}{digits}", "0".repeat(scale + 1 - digits.len()))
    } else {
        digits
    };
    let (whole, fraction) = digits.split_at(digits.len() - scale);
    format!("{sign}{whole}.{fraction}")
}

/// One JSON number token for `units × 10^-scale`, in one of several spellings a caller may hold:
/// positional with trailing zeros, an integer mantissa with an exponent, or scientific notation.
fn token(draw: &mut Draw, units: i128, scale: u32) -> String {
    let sign = if units < 0 || (units == 0 && draw.below(4) == 0) {
        "-"
    } else {
        ""
    };
    let magnitude = units.unsigned_abs();
    match draw.below(3) {
        0 => {
            let extra = draw.below(4) as u32;
            let digits = (magnitude * 10u128.pow(extra)).to_string();
            let scale = (scale + extra) as usize;
            if scale == 0 {
                return format!("{sign}{digits}");
            }
            let digits = if digits.len() <= scale {
                format!("{}{digits}", "0".repeat(scale + 1 - digits.len()))
            } else {
                digits
            };
            let (whole, fraction) = digits.split_at(digits.len() - scale);
            format!("{sign}{whole}.{fraction}")
        }
        1 => {
            let e = if draw.below(2) == 0 { "e" } else { "E" };
            if scale == 0 {
                format!("{sign}{magnitude}{e}+0")
            } else {
                format!("{sign}{magnitude}{e}-{scale}")
            }
        }
        _ => {
            if magnitude == 0 {
                return format!("{sign}0.0e5");
            }
            let digits = magnitude.to_string();
            let exponent = i64::try_from(digits.len()).unwrap() - 1 - i64::from(scale);
            let mantissa = if digits.len() > 1 {
                format!("{}.{}", &digits[..1], &digits[1..])
            } else {
                digits.clone()
            };
            let exponent = if exponent >= 0 && draw.below(2) == 0 {
                format!("E+{exponent}")
            } else {
                format!("e{exponent}")
            };
            format!("{sign}{mantissa}{exponent}")
        }
    }
}

fn operand(draw: &mut Draw) -> (i128, u32) {
    let digits = draw.below(19) as u32;
    let units = if digits == 0 {
        0
    } else {
        ((i128::from(draw.next()) << 53) | i128::from(draw.next())) % 10i128.pow(digits)
    };
    let units = if draw.below(2) == 0 { -units } else { units };
    (units, draw.below(13) as u32)
}

/// Every `kernel/1` `number` increment is the exact decimal sum of the two tokens, spelled with no
/// exponent and no trailing fractional zero, and the legacy event fold reproduces its bytes.
#[test]
fn a_number_increment_is_the_exact_sum_of_any_two_spellings_and_folds_to_the_same_bytes() {
    let mut registry = Registry::new();
    registry
        .register(
            serde_json::from_value(json!({
                "entity": "acc",
                "version": 1,
                "schema": { "fields": { "total": { "type": "number", "required": true } } },
                "lifecycle": { "initial": "open", "states": ["open"] },
                "create": { "emit": { "type": "Opened", "payload": {} } },
                "operations": { "add": {
                    "arguments": { "fields": { "by": { "type": "number", "required": true } } },
                    "transitions": [{ "from": "open", "to": "open" }],
                    "set": { "total": { "increment": "$args.by" } },
                    "emits": [{ "type": "Added", "payload": { "total": "$fields.total" } }]
                }}
            }))
            .expect("a definition document"),
        )
        .expect("registers");
    let validated = registry.get("acc", 1).expect("registered");
    let runtime = Runtime::new(&registry);
    let mut draw = Draw(0x5eed_1640);

    for case in 0..3000 {
        let (held_units, held_scale) = operand(&mut draw);
        let (by_units, by_scale) = operand(&mut draw);
        let held = token(&mut draw, held_units, held_scale);
        let by = token(&mut draw, by_units, by_scale);
        let common = held_scale.max(by_scale);
        let expected = spelled(
            held_units * 10i128.pow(common - held_scale) + by_units * 10i128.pow(common - by_scale),
            common,
        );

        let fields: Value = serde_json::from_str(&format!(r#"{{"total":{held}}}"#)).unwrap();
        let arguments: Value = serde_json::from_str(&format!(r#"{{"by":{by}}}"#)).unwrap();
        let created = runtime
            .create("acc", 1, "a-1", fields)
            .unwrap_or_else(|error| panic!("case {case}: create {held}: {error}"));
        let added = runtime
            .execute(&created.instance, "add", arguments)
            .unwrap_or_else(|error| panic!("case {case}: {held} + {by}: {error}"));
        let written = serde_json::to_string(&added.instance.fields["total"]).unwrap();
        assert_eq!(written, expected, "case {case}: {held} + {by}");

        if case % 10 == 0 {
            let events = [created.events, added.events].concat();
            let folded = rehydrate(validated, &events)
                .unwrap_or_else(|error| panic!("case {case}: fold of {held} + {by}: {error}"));
            assert_eq!(
                serde_json::to_string(&folded).unwrap(),
                serde_json::to_string(&added.instance).unwrap(),
                "case {case}: {held} + {by}"
            );
        }
    }
}

/// An integer increment is the exact sum when it lies in the semantics' span and
/// `IncrementOverflow` otherwise, for operands drawn at and around every span boundary.
#[test]
fn an_integer_increment_is_exact_inside_the_span_and_overflow_outside_it_under_both_semantics() {
    let boundaries: [i128; 9] = [
        i128::from(i64::MIN),
        i128::from(i64::MIN) + 1,
        -1,
        0,
        1,
        i128::from(i64::MAX) - 1,
        i128::from(i64::MAX),
        i128::from(i64::MAX) + 1,
        i128::from(u64::MAX),
    ];
    let mut draw = Draw(0x1a7e_6e85);
    for semantics in ["kernel/1", "service/1"] {
        let service = semantics != "kernel/1";
        let (low, high) = if service {
            (i128::from(i64::MIN), i128::from(i64::MAX))
        } else {
            (i128::from(i64::MIN), i128::from(u64::MAX))
        };
        let arguments = json!({ "fields": { "by": { "type": "integer", "required": true } } });
        let operation = if service {
            json!({
                "arguments": arguments,
                "outcomes": [{
                    "name": "added",
                    "effect": "updates",
                    "set": { "hits": { "increment": "$args.by" } }
                }]
            })
        } else {
            json!({
                "arguments": arguments,
                "transitions": [{ "from": "open", "to": "open" }],
                "set": { "hits": { "increment": "$args.by" } }
            })
        };
        let counter = definition(json!({
            "entity": "counter",
            "version": 1,
            "semantics": semantics,
            "schema": { "fields": { "hits": { "type": "integer", "required": true } } },
            "lifecycle": { "initial": "open", "states": ["open"] },
            "operations": { "add": operation }
        }))
        .expect("registers");
        for case in 0..400 {
            let pick = |draw: &mut Draw| {
                let base = boundaries[draw.below(boundaries.len() as u64) as usize];
                let offset = i128::from(draw.below(5)) - 2;
                (base + offset).clamp(low, high)
            };
            let held = pick(&mut draw);
            let by = pick(&mut draw);
            let before = instance("counter", &format!(r#"{{"hits":{held}}}"#));
            let arguments: Value = serde_json::from_str(&format!(r#"{{"by":{by}}}"#)).unwrap();
            let sum = held + by;
            match decide(&counter, &before, "add", arguments) {
                Ok(Evaluation::Accepted(decision)) => {
                    assert!(
                        (low..=high).contains(&sum),
                        "{semantics} case {case}: {held} + {by} accepted outside the span"
                    );
                    assert_eq!(
                        serde_json::to_string(&decision.instance.fields["hits"]).unwrap(),
                        sum.to_string(),
                        "{semantics} case {case}: {held} + {by}"
                    );
                }
                Ok(Evaluation::Refused(refusal)) => {
                    panic!("{semantics} case {case}: no branch refuses: {refusal:?}")
                }
                Err(error) => {
                    assert!(
                        !(low..=high).contains(&sum)
                            && matches!(error, CoreError::IncrementOverflow { .. }),
                        "{semantics} case {case}: {held} + {by}: {error}"
                    );
                }
            }
        }
    }
}
