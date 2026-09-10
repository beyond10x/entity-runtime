use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::{check_object, defect, Definition, DefinitionFormat, Failure};
use crate::{FieldDefinition, FieldKind, ObjectSchema};

/// A required entity field holding the logical identity, independently of its storage key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Identity {
    /// Exact top-level entity field name; its complete schema is the identity admission contract.
    pub field: String,
}

// Typed nodes keep JSON-library numeric carrier keys ordinary object keys and preserve numeric
// token spelling. This codec has its own vocabulary; domain values never select a codec variant.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
enum KeyValue {
    Null,
    Boolean(bool),
    Number(String),
    String(String),
    Array(Vec<KeyValue>),
    Object(BTreeMap<String, KeyValue>),
}

impl From<&Value> for KeyValue {
    fn from(value: &Value) -> Self {
        match value {
            Value::Null => Self::Null,
            Value::Bool(value) => Self::Boolean(*value),
            Value::Number(value) => Self::Number(value.to_string()),
            Value::String(value) => Self::String(value.clone()),
            Value::Array(values) => Self::Array(values.iter().map(Self::from).collect()),
            Value::Object(values) => Self::Object(
                values
                    .iter()
                    .map(|(key, value)| (key.clone(), Self::from(value)))
                    .collect(),
            ),
        }
    }
}

impl KeyValue {
    fn value(self) -> Result<Value, Failure> {
        Ok(match self {
            Self::Null => Value::Null,
            Self::Boolean(value) => Value::Bool(value),
            Self::Number(value) => Value::Number(value.parse().map_err(|_| Failure::IdentityKey)?),
            Self::String(value) => Value::String(value),
            Self::Array(values) => Value::Array(
                values
                    .into_iter()
                    .map(Self::value)
                    .collect::<Result<_, _>>()?,
            ),
            Self::Object(values) => Value::Object(
                values
                    .into_iter()
                    .map(|(key, value)| Ok((key, value.value()?)))
                    .collect::<Result<_, Failure>>()?,
            ),
        })
    }
}

/// Encode a logical identity as a deterministic, type-preserving profile-7 instance key.
///
/// This performs no schema admission, lookup or identity generation. Object keys are ordered;
/// number spelling, array order and string contents are preserved without coercion.
#[must_use]
pub fn identity_key(value: &Value) -> String {
    format!(
        "identity/1:{}",
        serde_json::to_string(&KeyValue::from(value)).expect("typed JSON identity is serializable")
    )
}

/// Decode a canonical profile-7 key without interpreting domain object keys as codec metadata.
///
/// # Errors
/// Returns [`Failure::IdentityKey`] for an unknown format, malformed or noncanonical key.
pub fn identity_value(key: &str) -> Result<Value, Failure> {
    let encoded = key
        .strip_prefix("identity/1:")
        .ok_or(Failure::IdentityKey)?;
    let value: KeyValue = serde_json::from_str(encoded).map_err(|_| Failure::IdentityKey)?;
    let value = value.value()?;
    if identity_key(&value) != key {
        return Err(Failure::IdentityKey);
    }
    Ok(value)
}

pub(super) fn read<'de, D: serde::Deserializer<'de>>(
    reader: D,
) -> Result<Option<Identity>, D::Error> {
    Identity::deserialize(reader).map(Some)
}

pub(super) fn validate(definition: &Definition) -> Result<(), Failure> {
    let identity = match (&definition.identity, definition.format) {
        (None, DefinitionFormat::V7 | DefinitionFormat::V8) => {
            return Err(defect(
                "identity",
                "profile 7 or later requires a typed identity field",
            ))
        }
        (Some(identity), DefinitionFormat::V7 | DefinitionFormat::V8) => identity,
        (Some(_), _) => {
            return Err(defect(
                "identity",
                "typed identity requires outcome profile 7 or later",
            ))
        }
        (None, _) => return Ok(()),
    };
    let field = definition
        .entity
        .schema
        .fields
        .get(&identity.field)
        .ok_or_else(|| defect("identity.field", "identity names no declared entity field"))?;
    if !field.required {
        return Err(defect("identity.field", "identity field must be required"));
    }
    typed(field, &format!("entity.schema.{}", identity.field))
}

fn typed(field: &FieldDefinition, path: &str) -> Result<(), Failure> {
    if field.kind == FieldKind::Json
        || field.additional_properties
        || field.default.as_value().is_some()
    {
        return Err(defect(
            path,
            "identity must be fully typed without defaults or untyped additional properties",
        ));
    }
    for (name, child) in &field.properties {
        typed(child, &format!("{path}.{name}"))?;
    }
    if let Some(item) = &field.items {
        typed(item, &format!("{path}.items"))?;
    }
    if let Some(union) = &field.union {
        for (name, variant) in &union.variants {
            typed(variant, &format!("{path}.variants.{name}"))?;
        }
    }
    Ok(())
}

pub(super) fn admit(definition: &Definition, key: &str) -> Result<Option<Value>, Failure> {
    let Some(identity) = &definition.identity else {
        return Ok(None);
    };
    let value = identity_value(key)?;
    let schema = ObjectSchema {
        fields: BTreeMap::from([(
            identity.field.clone(),
            definition.entity.schema.fields[&identity.field].clone(),
        )]),
        additional_fields: false,
    };
    check_object(
        &schema,
        &Map::from_iter([(identity.field.clone(), value.clone())]),
        "identity",
    )?;
    Ok(Some(value))
}

pub(super) fn matches(
    definition: &Definition,
    fields: &Map<String, Value>,
    value: Option<&Value>,
) -> Result<(), Failure> {
    if let (Some(identity), Some(value)) = (&definition.identity, value) {
        // Compare the canonical key, not numeric equality that could alias distinct token spellings.
        if fields.get(&identity.field).map(identity_key).as_deref()
            != Some(identity_key(value).as_str())
        {
            return Err(Failure::IdentityMismatch);
        }
    }
    Ok(())
}
