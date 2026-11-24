//! Exact reading of the numbers a YAML reader would round.
//!
//! The YAML reader resolves a plain scalar such as `1.0000000000000001` to an `f64` before any
//! visitor sees it, so a definition's `serde_json::Number` — which keeps every digit — received
//! `1.0` and the kernel's exact comparison ran against a bound the author never wrote. This module
//! reads such a document a second time, guided by the shape the first reading found, and takes
//! each float scalar's own spelling where the `f64` would change its value.
//!
//! A float whose shortest `f64` spelling is the same decimal as the source keeps the reading it
//! always had, so the bytes of every definition that was already read exactly do not move: only a
//! bound that was silently rounded changes, and it changes to what the author wrote.

use serde::de::{DeserializeSeed, Error as _, MapAccess, SeqAccess, Visitor};
use serde::Deserialize;
use serde_json::{Map, Number, Value};
use serde_yaml_ng::Value as Yaml;
use std::fmt;

/// Reads `input` with every float scalar at its source value, or `None` when no float in it
/// would have been rounded, or the document holds a construct this reading does not follow (a
/// tagged value, which the definition reader refuses before this runs, or a key that is a mapping
/// or a sequence). `None` leaves the caller's reading as it was.
pub(super) fn exact_value(input: &str) -> Result<Option<Value>, serde_yaml_ng::Error> {
    let shape: Yaml = serde_yaml_ng::from_str(input)?;
    let mut rounded = false;
    let seed = Exact {
        shape: &shape,
        rounded: &mut rounded,
    };
    let value = match seed.deserialize(serde_yaml_ng::Deserializer::from_str(input)) {
        Ok(value) => value,
        Err(_) => return Ok(None),
    };
    Ok(rounded.then_some(value))
}

struct Exact<'s, 'r> {
    shape: &'s Yaml,
    rounded: &'r mut bool,
}

impl<'de> DeserializeSeed<'de> for Exact<'_, '_> {
    type Value = Value;

    fn deserialize<D>(self, deserializer: D) -> Result<Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        match self.shape {
            Yaml::Number(number) if number.is_f64() => {
                let text = String::deserialize(deserializer)?;
                let float = number.as_f64().unwrap_or(f64::NAN);
                let read = Number::from_f64(float)
                    .ok_or_else(|| D::Error::custom("a float that is not a JSON number"))?;
                match exact_decimal(&text) {
                    Some(source) if Some(&source) != canonical_f64(float).as_ref() => {
                        let exact: Number =
                            serde_json::from_str(&text).map_err(D::Error::custom)?;
                        *self.rounded = true;
                        Ok(Value::Number(exact))
                    }
                    _ => Ok(Value::Number(read)),
                }
            }
            Yaml::Mapping(mapping) => deserializer.deserialize_map(ExactMap {
                shape: mapping,
                rounded: self.rounded,
            }),
            Yaml::Sequence(sequence) => deserializer.deserialize_seq(ExactSeq {
                shape: sequence,
                rounded: self.rounded,
            }),
            Yaml::Tagged(_) => Err(D::Error::custom("a tagged node is read as written")),
            Yaml::Null | Yaml::Bool(_) | Yaml::Number(_) | Yaml::String(_) => {
                Value::deserialize(deserializer)
            }
        }
    }
}

struct ExactMap<'s, 'r> {
    shape: &'s serde_yaml_ng::Mapping,
    rounded: &'r mut bool,
}

impl<'de> Visitor<'de> for ExactMap<'_, '_> {
    type Value = Value;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("the mapping the first reading found")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut object = Map::new();
        // Duplicate and merge keys were refused before this reading, so the n-th key read here is
        // the n-th entry of the shape.
        for (key, shape) in self.shape {
            // A key is read as a string, and the string reader takes a tagged scalar's text and
            // ignores its tag, as the definition reader does: `!x rate:` names the field `rate`.
            let scalar = match key {
                Yaml::Tagged(tagged) => &tagged.value,
                untagged => untagged,
            };
            if !matches!(
                scalar,
                Yaml::Null | Yaml::Bool(_) | Yaml::Number(_) | Yaml::String(_)
            ) {
                return Err(A::Error::custom(
                    "a key that is not a scalar is read as written",
                ));
            }
            let Some(name) = map.next_key::<String>()? else {
                return Err(A::Error::custom("the mapping ended before its shape"));
            };
            let value = map.next_value_seed(Exact {
                shape,
                rounded: self.rounded,
            })?;
            object.insert(name, value);
        }
        if map.next_key::<serde::de::IgnoredAny>()?.is_some() {
            return Err(A::Error::custom("the mapping continues past its shape"));
        }
        Ok(Value::Object(object))
    }
}

struct ExactSeq<'s, 'r> {
    shape: &'s [Yaml],
    rounded: &'r mut bool,
}

impl<'de> Visitor<'de> for ExactSeq<'_, '_> {
    type Value = Value;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("the sequence the first reading found")
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut values = Vec::with_capacity(self.shape.len());
        for shape in self.shape {
            let Some(value) = sequence.next_element_seed(Exact {
                shape,
                rounded: self.rounded,
            })?
            else {
                return Err(A::Error::custom("the sequence ended before its shape"));
            };
            values.push(value);
        }
        if sequence.next_element::<serde::de::IgnoredAny>()?.is_some() {
            return Err(A::Error::custom("the sequence continues past its shape"));
        }
        Ok(Value::Array(values))
    }
}

/// A decimal as `(negative, significant digits, exponent)`: the value is `digits × 10^exponent`,
/// with no leading or trailing zero in `digits`, and zero written as `"0"` with exponent 0.
type Decimal = (bool, String, i64);

/// The decimal a JSON-grammar number spells, or `None` for any other spelling (`.5`, `+1.0`,
/// `.inf`), which this reading leaves to the `f64` it always had.
fn exact_decimal(text: &str) -> Option<Decimal> {
    let (negative, unsigned) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    let (mantissa, exponent) = match unsigned.find(['e', 'E']) {
        Some(at) => {
            let exponent = &unsigned[at + 1..];
            let digits = exponent.strip_prefix(['+', '-']).unwrap_or(exponent);
            if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
                return None;
            }
            (&unsigned[..at], exponent.parse::<i64>().ok()?)
        }
        None => (unsigned, 0),
    };
    let (whole, fraction) = match mantissa.split_once('.') {
        Some((whole, fraction)) if !fraction.is_empty() => (whole, fraction),
        Some(_) => return None,
        None => (mantissa, ""),
    };
    let leading_zero_ok = whole == "0" || !whole.starts_with('0');
    if whole.is_empty()
        || !leading_zero_ok
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    let fraction_len = i64::try_from(fraction.len()).ok()?;
    normalise(
        negative,
        &format!("{whole}{fraction}"),
        exponent.checked_sub(fraction_len)?,
    )
}

/// The decimal Rust's shortest round-trip spelling of `float` denotes, which is the spelling the
/// YAML reading has always produced for it.
fn canonical_f64(float: f64) -> Option<Decimal> {
    // `{:e}` is the shortest digits that read back as the same `f64`, in scientific form:
    // `1e0`, `1.0000000000000002e0`, `-2.5e-3`.
    exact_decimal(&format!("{float:e}"))
}

fn normalise(negative: bool, digits: &str, exponent: i64) -> Option<Decimal> {
    let significant = digits.trim_start_matches('0');
    if significant.is_empty() {
        return Some((negative, "0".to_owned(), 0));
    }
    let trimmed = significant.trim_end_matches('0');
    let dropped = i64::try_from(significant.len() - trimmed.len()).ok()?;
    Some((negative, trimmed.to_owned(), exponent.checked_add(dropped)?))
}
