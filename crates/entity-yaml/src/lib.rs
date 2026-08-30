//! YAML adapter for `entity-core` definitions.
//!
//! One function: text in, [`EntityDefinition`] out. Parsing is from `&str` only — this crate does
//! not read files, so the filesystem stays where it belongs, in the shell that calls it. The
//! returned definition is *parsed*, not yet *validated*; validation happens when it is handed to
//! [`entity_core::Registry::register`].
//!
//! ```
//! let definition = entity_yaml::from_str(r#"
//! entity: light
//! lifecycle:
//!   initial: off
//!   states: [off, on]
//! schema: {}
//! operations:
//!   switch_on:
//!     transitions: [{ from: off, to: on }]
//!   switch_off:
//!     transitions: [{ from: on, to: off }]
//! "#)?;
//! assert_eq!(definition.entity, "light");
//! assert_eq!(definition.version, 1); // the default
//! assert_eq!(definition.operations.len(), 2);
//! # Ok::<(), entity_yaml::YamlError>(())
//! ```

use entity_core::EntityDefinition;
use serde::de::{DeserializeSeed, Error as _, MapAccess, SeqAccess, Visitor};
use std::fmt;

/// The YAML could not be parsed into an [`EntityDefinition`].
///
/// Carries the parser's own message, which names the line and column.
#[derive(Debug)]
pub struct YamlError(serde_yaml_ng::Error);

impl fmt::Display for YamlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid entity YAML: {}", self.0)
    }
}

impl std::error::Error for YamlError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0)
    }
}

/// Parses a YAML document into an [`EntityDefinition`].
///
/// # Errors
///
/// [`YamlError`] when the text is not valid YAML or does not have the shape of a definition —
/// a missing `lifecycle`, a field without a `type`, a condition with an unknown operator.
pub fn from_str(input: &str) -> Result<EntityDefinition, YamlError> {
    let mut documents = serde_yaml_ng::Deserializer::from_str(input);
    if let Some(document) = documents.next() {
        NoDuplicates.deserialize(document).map_err(YamlError)?;
    }
    serde_yaml_ng::from_str(input).map_err(YamlError)
}

/// A first pass that consumes no values and exists only to reject mapping ambiguity. Serde's map
/// collectors otherwise keep the last occurrence of a dynamic key, unlike duplicate struct
/// fields, so a repeated schema field silently changes the definition that is validated.
struct NoDuplicates;

impl<'de> DeserializeSeed<'de> for NoDuplicates {
    type Value = ();

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(NoDuplicatesVisitor)
    }
}

struct NoDuplicatesVisitor;

impl<'de> Visitor<'de> for NoDuplicatesVisitor {
    type Value = ();

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("unambiguous YAML")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut keys = Vec::new();
        while let Some(key) = map.next_key::<serde_yaml_ng::Value>()? {
            if key.as_str() == Some("<<") {
                return Err(A::Error::custom(
                    "YAML merge keys are not accepted; write every effective key once",
                ));
            }
            if keys.contains(&key) {
                return Err(A::Error::custom(format!("duplicate mapping key {key:?}")));
            }
            keys.push(key);
            map.next_value_seed(NoDuplicates)?;
        }
        Ok(())
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        while sequence.next_element_seed(NoDuplicates)?.is_some() {}
        Ok(())
    }

    fn visit_bool<E>(self, _: bool) -> Result<Self::Value, E> {
        Ok(())
    }

    fn visit_i64<E>(self, _: i64) -> Result<Self::Value, E> {
        Ok(())
    }

    fn visit_u64<E>(self, _: u64) -> Result<Self::Value, E> {
        Ok(())
    }

    fn visit_f64<E>(self, _: f64) -> Result<Self::Value, E> {
        Ok(())
    }

    fn visit_str<E>(self, _: &str) -> Result<Self::Value, E> {
        Ok(())
    }

    fn visit_string<E>(self, _: String) -> Result<Self::Value, E> {
        Ok(())
    }

    fn visit_none<E>(self) -> Result<Self::Value, E> {
        Ok(())
    }

    fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        NoDuplicates.deserialize(deserializer)
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E> {
        Ok(())
    }
}
