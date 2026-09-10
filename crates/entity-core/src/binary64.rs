//! Finite numeric conversion and source-token preservation at an explicit typed decode boundary.
use std::collections::BTreeMap;

use serde::de::Error as _;
use serde_json::{value::RawValue, Map, Number, Value};

use crate::{FieldDefinition, FieldKind, ObjectSchema};

pub(crate) fn normalize(number: &Number) -> Option<Number> {
    let value = number.as_str().parse::<f64>().ok()?;
    Number::from_f64(value)
}

pub(crate) fn decode_object(
    schema: &ObjectSchema,
    text: &str,
) -> Result<Map<String, Value>, serde_json::Error> {
    decode_members(&schema.fields, text)
}

fn decode_members(
    fields: &BTreeMap<String, FieldDefinition>,
    text: &str,
) -> Result<Map<String, Value>, serde_json::Error> {
    let raw: BTreeMap<String, Box<RawValue>> = serde_json::from_str(text)?;
    raw.into_iter()
        .map(|(name, raw)| {
            let value = match fields.get(&name) {
                Some(field) => decode_field(field, &raw)?,
                None => serde_json::from_str(raw.get())?,
            };
            Ok((name, value))
        })
        .collect()
}

fn decode_field(field: &FieldDefinition, raw: &RawValue) -> Result<Value, serde_json::Error> {
    let text = raw.get().trim();
    if field.kind == FieldKind::Number && field.number_encoding.is_some() {
        if !matches!(text.as_bytes().first(), Some(b'-' | b'0'..=b'9')) {
            return Err(serde_json::Error::custom(
                "binary64 requires a JSON number token",
            ));
        }
        let value = text.parse::<f64>().map_err(serde_json::Error::custom)?;
        return Number::from_f64(value)
            .map(Value::Number)
            .ok_or_else(|| serde_json::Error::custom("binary64 requires a finite value"));
    }
    match field.kind {
        FieldKind::Nullable if text != "null" => {
            if let Some(inner) = &field.items {
                return decode_field(inner, raw);
            }
        }
        FieldKind::Object => return decode_members(&field.properties, text).map(Value::Object),
        FieldKind::Array => {
            if let Some(inner) = &field.items {
                let values: Vec<Box<RawValue>> = serde_json::from_str(text)?;
                return values
                    .iter()
                    .map(|raw| decode_field(inner, raw))
                    .collect::<Result<Vec<_>, _>>()
                    .map(Value::Array);
            }
        }
        FieldKind::Map => {
            if let Some(inner) = &field.items {
                let values: BTreeMap<String, Box<RawValue>> = serde_json::from_str(text)?;
                return values
                    .into_iter()
                    .map(|(name, raw)| Ok((name, decode_field(inner, &raw)?)))
                    .collect::<Result<Map<_, _>, _>>()
                    .map(Value::Object);
            }
        }
        FieldKind::Union => {
            if let Some(union) = &field.union {
                let values: BTreeMap<String, Box<RawValue>> = serde_json::from_str(text)?;
                let tag = values
                    .get(&union.tag)
                    .map(|raw| serde_json::from_str::<Value>(raw.get()))
                    .transpose()?;
                let variant = tag
                    .as_ref()
                    .and_then(Value::as_str)
                    .and_then(|tag| union.variants.get(tag));
                return values
                    .into_iter()
                    .map(|(name, raw)| {
                        let value = match variant.filter(|_| name == union.content) {
                            Some(variant) => decode_field(variant, &raw)?,
                            None => serde_json::from_str(raw.get())?,
                        };
                        Ok((name, value))
                    })
                    .collect::<Result<Map<_, _>, _>>()
                    .map(Value::Object);
            }
        }
        _ => {}
    }
    serde_json::from_str(text)
}
