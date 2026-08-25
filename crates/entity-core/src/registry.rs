use crate::{validation::validate_definition, DefinitionError, EntityDefinition};
use std::collections::BTreeMap;

/// Validated entity definitions, keyed by `(entity, version)`.
///
/// A definition enters the registry only through [`Registry::register`], which validates it first;
/// a definition that is in the registry is therefore one the kernel can execute. Several versions
/// of one entity may coexist, which is how instances written under an older definition keep
/// executing while a newer one is rolled out.
#[derive(Debug, Clone, Default)]
pub struct Registry {
    definitions: BTreeMap<(String, u32), EntityDefinition>,
}

impl Registry {
    /// An empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Validates a definition and stores it, replacing an earlier definition of the same
    /// `(entity, version)`.
    ///
    /// # Errors
    ///
    /// The first [`DefinitionError`] found: an undeclared lifecycle state, an ambiguous transition,
    /// a `set` writing an unknown field, a rule referencing something its scope cannot see, an
    /// invalid default, and so on. Nothing is stored when validation fails.
    pub fn register(&mut self, definition: EntityDefinition) -> Result<(), DefinitionError> {
        validate_definition(&definition)?;
        let key = (definition.entity.clone(), definition.version);
        self.definitions.insert(key, definition);
        Ok(())
    }

    /// The definition registered under `(entity, version)`, if any.
    pub fn get(&self, entity: &str, version: u32) -> Option<&EntityDefinition> {
        self.definitions.get(&(entity.to_owned(), version))
    }

    /// Every registered definition, in `(entity, version)` order.
    pub fn iter(&self) -> impl Iterator<Item = &EntityDefinition> {
        self.definitions.values()
    }

    /// How many definitions are registered.
    pub fn len(&self) -> usize {
        self.definitions.len()
    }

    /// Whether no definition is registered.
    pub fn is_empty(&self) -> bool {
        self.definitions.is_empty()
    }
}
