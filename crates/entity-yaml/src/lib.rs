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
    serde_yaml_ng::from_str(input).map_err(YamlError)
}
