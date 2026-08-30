//! The registry: validated definitions, by name and version.

use crate::{DefinitionError, DefinitionErrors, EntityDefinition};
use std::collections::BTreeMap;
use std::ops::Deref;

/// A definition that passed complete registration validation.
///
/// The inner value is intentionally private: execution accepts this handle rather than a raw
/// [`EntityDefinition`], making the validation boundary part of the type system instead of a
/// convention every caller has to remember.
#[derive(Debug, Clone, PartialEq)]
pub struct ValidatedDefinition(EntityDefinition);

impl ValidatedDefinition {
    /// Validates `definition` and returns the executable handle.
    ///
    /// # Errors
    ///
    /// Every independent definition defect found.
    pub fn new(definition: EntityDefinition) -> Result<Self, DefinitionErrors> {
        definition.validate()?;
        Ok(Self(definition))
    }

    /// The validated definition data, for inspection and deterministic storage.
    #[must_use]
    pub const fn as_definition(&self) -> &EntityDefinition {
        &self.0
    }

    /// Returns the validated definition data.
    #[must_use]
    pub fn into_definition(self) -> EntityDefinition {
        self.0
    }
}

impl Deref for ValidatedDefinition {
    type Target = EntityDefinition;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

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
    definitions: BTreeMap<String, BTreeMap<u32, ValidatedDefinition>>,
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
        let definition = ValidatedDefinition::new(definition)?;
        self.definitions
            .entry(definition.entity.clone())
            .or_default()
            .insert(definition.version, definition);
        Ok(())
    }

    /// Checks the registry as a **set**: every `ref` points at an entity type it holds.
    ///
    /// [`register`](Self::register) validates a definition on its own, and deliberately says
    /// nothing about references, because two types that point at each other are ordinary — a story
    /// naming its epic and an epic naming its stories cannot both be registered if each demands
    /// the other first. So the question is asked of the finished set, once, by whoever assembled
    /// it.
    ///
    /// Only the **type** is checked. Whether an instance carrying that identity exists is a
    /// question about another instance, and the kernel is handed exactly one (R-01); a reference's
    /// target is the shell's to resolve, exactly as `protocol artifact relate` resolves one today.
    ///
    /// # Errors
    ///
    /// [`DefinitionError::UnknownRelationTarget`] for every reference whose target is missing —
    /// all of them, not the first, because a registry assembled from ten files has ten chances to
    /// name a type that is not there.
    pub fn validate_all(&self) -> Result<(), DefinitionErrors> {
        let mut defects = Vec::new();
        for definition in self.iter() {
            for (path, target) in crate::validation::relation_targets(definition) {
                if !self.definitions.contains_key(&target) {
                    defects.push(DefinitionError::UnknownRelationTarget {
                        entity: definition.entity.clone(),
                        path,
                        target,
                    });
                }
            }
        }
        if defects.is_empty() {
            Ok(())
        } else {
            Err(DefinitionErrors::new(defects))
        }
    }

    /// The definition registered under `(entity, version)`, if any.
    pub fn get(&self, entity: &str, version: u32) -> Option<&ValidatedDefinition> {
        self.definitions.get(entity)?.get(&version)
    }

    /// Every version of `entity`, oldest first.
    pub fn versions(&self, entity: &str) -> impl Iterator<Item = &ValidatedDefinition> {
        self.definitions
            .get(entity)
            .into_iter()
            .flat_map(BTreeMap::values)
    }

    /// Every registered definition, in `(entity, version)` order.
    pub fn iter(&self) -> impl Iterator<Item = &ValidatedDefinition> {
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
