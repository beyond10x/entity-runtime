//! Document-local expansion, deliberately outside the kernel's serialized model.

use entity_core::EntityDefinition;
use serde::de::Error as _;
use serde_json::Value;

use crate::YamlError;

fn error(message: impl std::fmt::Display) -> YamlError {
    YamlError(serde_yaml_ng::Error::custom(message))
}

pub(super) fn parse(input: &str) -> Result<EntityDefinition, YamlError> {
    // JSON's value representation preserves the same exact numbers as kernel literals.
    let mut document: Value = serde_yaml_ng::from_str(input).map_err(YamlError)?;
    let definitions = document
        .as_object_mut()
        .and_then(|object| object.remove("$defs"))
        .unwrap_or_else(|| Value::Object(Default::default()));
    if !definitions.is_object() {
        return Err(error("$defs must be a mapping of fragment names to values"));
    }
    let root = serde_json::json!({"$defs": definitions});
    let expanded = expand(document, &root, &mut Vec::new(), 0)?;
    serde_json::from_value(expanded).map_err(error)
}

fn expand(
    value: Value,
    root: &Value,
    active: &mut Vec<String>,
    depth: usize,
) -> Result<Value, YamlError> {
    // Bound both reference chains and structural nesting before recursing.
    if depth > 128 {
        return Err(error("fragment expansion exceeds nesting limit of 128"));
    }
    match value {
        Value::Object(mut object) => {
            let base = if let Some(reference) = object.remove("$ref") {
                let reference = reference
                    .as_str()
                    .ok_or_else(|| error("$ref must be a string"))?;
                let pointer = reference
                    .strip_prefix('#')
                    .filter(|pointer| pointer.starts_with("/$defs/"))
                    .ok_or_else(|| error("$ref must be a document-local #/$defs/ JSON pointer"))?;
                if active.iter().any(|entry| entry == reference) {
                    return Err(error(format!("cyclic fragment reference {reference}")));
                }
                let target = root
                    .pointer(pointer)
                    .ok_or_else(|| error(format!("unknown fragment reference {reference}")))?;
                active.push(reference.to_owned());
                let resolved = expand(target.clone(), root, active, depth + 1)?;
                active.pop();
                Some(resolved)
            } else {
                None
            };
            let mut overrides = serde_json::Map::new();
            for (key, value) in object {
                overrides.insert(key, expand(value, root, active, depth + 1)?);
            }
            match base {
                Some(base) if overrides.is_empty() => Ok(base),
                Some(mut base) => {
                    if !base.is_object() {
                        return Err(error("$ref with sibling keys must resolve to a mapping"));
                    }
                    overlay(&mut base, Value::Object(overrides));
                    Ok(base)
                }
                None => Ok(Value::Object(overrides)),
            }
        }
        Value::Array(values) => values
            .into_iter()
            .map(|value| expand(value, root, active, depth + 1))
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array),
        scalar => Ok(scalar),
    }
}

// Inheritance combines mappings recursively; a local scalar or sequence replaces the base value.
fn overlay(base: &mut Value, local: Value) {
    if let (Value::Object(base), Value::Object(local)) = (&mut *base, &local) {
        for (key, value) in local {
            match base.get_mut(key) {
                Some(previous) => overlay(previous, value.clone()),
                None => {
                    base.insert(key.clone(), value.clone());
                }
            }
        }
    } else {
        *base = local;
    }
}
