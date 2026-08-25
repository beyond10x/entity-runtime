//! The registry: validated definitions, by name and version.

use crate::{DefinitionError, DefinitionErrors, EntityDefinition};
use std::collections::BTreeMap;

/// Validated entity definitions, keyed by `(entity, version)`.
///
/// A definition enters the registry only through [`Registry::register`] or
/// [`Registry::replace`], both of which validate it first; a definition that is in the registry
/// is therefore one the kernel can execute. Several versions of one entity may coexist, which is
/// how instances written under an older definition keep executing while a newer one is rolled out.
///
/// Registering over an existing `(entity, version)` is **refused**, because an instance created
/// under the first definition would then be executed under the second while still matching by
/// name and version — the very confusion [`CoreError::EntityMismatch`](crate::CoreError) exists to
/// catch. [`Registry::replace`] is how a caller says they mean it.
#[derive(Debug, Clone, Default)]
pub struct Registry {
    definitions: BTreeMap<String, BTreeMap<u32, EntityDefinition>>,
}

impl Registry {
    /// An empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Validates a definition and stores it.
    ///
    /// # Errors
    ///
    /// [`DefinitionError::DuplicateDefinition`] when this `(entity, version)` is already
    /// registered; otherwise **every** defect the document has — an undeclared lifecycle state, an
    /// ambiguous transition, a `set` writing an unknown field, a rule or template referencing
    /// something its scope cannot see, an inapplicable constraint, an invalid default. Nothing is
    /// stored when validation fails.
    pub fn register(&mut self, definition: EntityDefinition) -> Result<(), DefinitionErrors> {
        if self.get(&definition.entity, definition.version).is_some() {
            return Err(DefinitionError::DuplicateDefinition {
                entity: definition.entity.clone(),
                version: definition.version,
            }
            .into());
        }
        self.replace(definition)
    }

    /// Validates a definition and stores it, replacing any definition of the same
    /// `(entity, version)`.
    ///
    /// # Errors
    ///
    /// Every defect the document has. Nothing is stored, and nothing is removed, when validation
    /// fails.
    pub fn replace(&mut self, definition: EntityDefinition) -> Result<(), DefinitionErrors> {
        definition.validate()?;
        self.definitions
            .entry(definition.entity.clone())
            .or_default()
            .insert(definition.version, definition);
        Ok(())
    }

    /// The definition registered under `(entity, version)`, if any.
    pub fn get(&self, entity: &str, version: u32) -> Option<&EntityDefinition> {
        self.definitions.get(entity)?.get(&version)
    }

    /// Every version of `entity`, oldest first.
    pub fn versions(&self, entity: &str) -> impl Iterator<Item = &EntityDefinition> {
        self.definitions
            .get(entity)
            .into_iter()
            .flat_map(BTreeMap::values)
    }

    /// Every registered definition, in `(entity, version)` order.
    pub fn iter(&self) -> impl Iterator<Item = &EntityDefinition> {
        self.definitions.values().flat_map(BTreeMap::values)
    }

    /// How many definitions are registered.
    pub fn len(&self) -> usize {
        self.definitions.values().map(BTreeMap::len).sum()
    }

    /// Whether no definition is registered.
    pub fn is_empty(&self) -> bool {
        self.definitions.values().all(BTreeMap::is_empty)
    }
}
