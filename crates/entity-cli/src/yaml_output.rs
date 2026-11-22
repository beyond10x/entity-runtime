//! YAML output in which every number is a plain YAML number.
//!
//! The kernel's exact numbers come from `serde_json`'s `arbitrary_precision` feature, which
//! serializes a number as a private one-entry map, `$serde_json::private::Number: '3.75'`. The JSON
//! writer knows that map; the YAML writer wrote it literally, so a reader other than this runtime's
//! own decoded every number of a decision as a mapping. The YAML writer spells a plain number only
//! from a native integer or an `f64`, and an `f64` changes digits it cannot hold, so each number is
//! written as a placeholder word and the word is then replaced by the number's own digits — the
//! technique `entity-surface` uses for the generated contracts. The value is built as a
//! `serde_yaml_ng::Value`, whose mappings keep the order the types declare their fields in.

use serde_yaml_ng::{Mapping, Value};

const PRIVATE_NUMBER: &str = "$serde_json::private::Number";
const NUMBER_MARKER: &str = "entity-runtime-number-";

/// `value` as YAML with exact numbers.
pub(crate) fn to_string<T: serde::Serialize + ?Sized>(value: &T) -> Result<String, String> {
    let value = serde_yaml_ng::to_value(value).map_err(|error| error.to_string())?;
    value_to_string(&value)
}

/// `value` as YAML with exact numbers, for a value already built (by a serializer with its own
/// enum spelling, for instance).
pub(crate) fn value_to_string(value: &Value) -> Result<String, String> {
    let mut generation = 0_u64;
    loop {
        let marker = format!("{NUMBER_MARKER}{generation}-");
        let mut digits = Vec::new();
        let placeheld = numbers_as_placeholders(value, &marker, &mut digits);
        let text = serde_yaml_ng::to_string(&placeheld).map_err(|error| error.to_string())?;
        // A placeholder is trusted only when the text holds the marker exactly once per number;
        // any other occurrence comes from a string in the value, and the next marker is tried.
        if text.matches(marker.as_str()).count() == digits.len() {
            return digits_for_placeholders(&text, &marker, &digits);
        }
        generation += 1;
    }
}

/// The digits of `value` when it is the private map of an exact number.
fn exact_number(value: &Mapping) -> Option<&str> {
    if value.len() != 1 {
        return None;
    }
    let (key, digits) = value.iter().next()?;
    match (key, digits) {
        (Value::String(key), Value::String(digits))
            if key == PRIVATE_NUMBER && digits.parse::<serde_json::Number>().is_ok() =>
        {
            Some(digits)
        }
        _ => None,
    }
}

fn numbers_as_placeholders(value: &Value, marker: &str, digits: &mut Vec<String>) -> Value {
    match value {
        Value::Mapping(members) => {
            if let Some(number) = exact_number(members) {
                let placeholder = format!("{marker}{}", digits.len());
                digits.push(number.to_owned());
                return Value::String(placeholder);
            }
            Value::Mapping(
                members
                    .iter()
                    .map(|(key, member)| {
                        (key.clone(), numbers_as_placeholders(member, marker, digits))
                    })
                    .collect(),
            )
        }
        Value::Sequence(items) => Value::Sequence(
            items
                .iter()
                .map(|item| numbers_as_placeholders(item, marker, digits))
                .collect(),
        ),
        Value::Tagged(tagged) => {
            let mut tagged = tagged.clone();
            tagged.value = numbers_as_placeholders(&tagged.value, marker, digits);
            Value::Tagged(tagged)
        }
        other => other.clone(),
    }
}

fn digits_for_placeholders(text: &str, marker: &str, digits: &[String]) -> Result<String, String> {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(marker) {
        out.push_str(&rest[..at]);
        rest = &rest[at + marker.len()..];
        let end = rest
            .find(|character: char| !character.is_ascii_digit())
            .unwrap_or(rest.len());
        let number = rest[..end]
            .parse::<usize>()
            .ok()
            .and_then(|index| digits.get(index))
            .ok_or_else(|| {
                format!(
                    "YAML number placeholder {marker}{} is not one written",
                    &rest[..end]
                )
            })?;
        out.push_str(number);
        rest = &rest[end..];
    }
    out.push_str(rest);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn exact_numbers_are_written_as_yaml_numbers_with_their_own_digits() {
        let fields: serde_json::Value =
            serde_json::from_str(r#"{"name":"City","spent":3.75,"views":0,"tiny":1.50,"precise":0.1000000000000000000001}"#)
                .expect("exact JSON");
        let text = to_string(&fields).expect("YAML");
        assert!(!text.contains(PRIVATE_NUMBER), "{text}");
        let read: Value = serde_yaml_ng::from_str(&text).expect("plain YAML");
        assert_eq!(read["spent"], Value::Number(3.75.into()), "{text}");
        assert_eq!(read["views"], Value::Number(0.into()), "{text}");
        assert!(text.contains("tiny: 1.50\n"), "digits kept: {text}");
        assert!(
            text.contains("precise: 0.1000000000000000000001\n"),
            "digits kept: {text}"
        );
        assert_eq!(read["name"], Value::String("City".into()));
    }

    #[test]
    fn a_string_that_looks_like_the_placeholder_stays_a_string() {
        let value = json!({ "note": format!("{NUMBER_MARKER}0-0"), "count": 2 });
        let text = to_string(&value).expect("YAML");
        let read: Value = serde_yaml_ng::from_str(&text).expect("plain YAML");
        assert_eq!(read["note"], Value::String(format!("{NUMBER_MARKER}0-0")));
        assert_eq!(read["count"], Value::Number(2.into()));
    }

    #[test]
    fn field_order_is_the_declared_order_not_sorted() {
        #[derive(serde::Serialize)]
        struct Instance {
            entity: &'static str,
            id: &'static str,
            amount: serde_json::Value,
        }
        let text = to_string(&Instance {
            entity: "counter",
            id: "sample",
            amount: serde_json::from_str("2.5").expect("number"),
        })
        .expect("YAML");
        assert_eq!(text, "entity: counter\nid: sample\namount: 2.5\n");
    }
}
