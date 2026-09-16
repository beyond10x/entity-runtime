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
    /// target is the shell's to resolve, exactly as `aep artifact relate` resolves one today.
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
        defects.extend(self.declared_relation_defects());
        if defects.is_empty() {
            Ok(())
        } else {
            Err(DefinitionErrors::new(defects))
        }
    }

    /// The half of a declared relation only the whole registry can answer.
    ///
    /// For an `owns` relation the carrying document is **not** the one the author is reading — the
    /// carrier field lives on the target, "because that is where an owner's identity lives on the
    /// thing it owns" — so the check belongs here rather than on the declaring definition. The
    /// second claimant of a field and the second owner of an entity are reported in name order, so
    /// one registry assembled from ten files reports the same pair every time.
    fn declared_relation_defects(&self) -> Vec<DefinitionError> {
        let mut defects = Vec::new();
        // Which definition claims to own each entity, and which relation of it first did.
        let mut owners: BTreeMap<&str, (&str, &str)> = BTreeMap::new();
        // Which relation claims each (entity, field) carrier.
        let mut carriers: BTreeMap<(&str, &str), (&str, &str)> = BTreeMap::new();

        for definition in self.iter() {
            for (name, relation) in &definition.relations {
                let Some(target) = self.latest(&relation.target) else {
                    defects.push(DefinitionError::RelationTargetMissing {
                        entity: definition.entity.clone(),
                        relation: name.clone(),
                        target: relation.target.clone(),
                    });
                    continue;
                };
                if relation.kind == crate::RelationKind::References {
                    // The declaring definition already checked the carrier's shape; here only its
                    // exclusive claim on the field is in question.
                    claim(
                        &mut carriers,
                        &mut defects,
                        &definition.entity,
                        &relation.via,
                        name,
                    );
                    continue;
                }

                if let Some((owner, first)) = owners.get(target.entity.as_str()) {
                    defects.push(DefinitionError::RelationSecondOwner {
                        target: target.entity.clone(),
                        owner: (*owner).to_owned(),
                        other: definition.entity.clone(),
                    });
                    let _ = first;
                } else {
                    owners.insert(&target.entity, (&definition.entity, name));
                }
                claim(
                    &mut carriers,
                    &mut defects,
                    &target.entity,
                    &relation.via,
                    name,
                );

                // The carrier lives on the target, is typed as the **source's** identity kind, and
                // is required whatever the cardinality says: `cardinality` says how many of the
                // target there are and says nothing about that field.
                let Some(field) = target.schema.fields.get(&relation.via) else {
                    defects.push(DefinitionError::RelationCarrierWrong {
                        entity: definition.entity.clone(),
                        relation: name.clone(),
                        via: relation.via.clone(),
                        detail: format!("'{}' declares no such field", target.entity),
                    });
                    continue;
                };
                if !field.required {
                    defects.push(DefinitionError::RelationCarrierOptionality {
                        relation: name.clone(),
                        via: relation.via.clone(),
                    });
                }
                if field.kind == crate::FieldKind::Array {
                    defects.push(DefinitionError::RelationCarrierWrong {
                        entity: definition.entity.clone(),
                        relation: name.clone(),
                        via: relation.via.clone(),
                        detail: "it is an array field, and an owner's identity is one value \
                                 whether the owner has one of these or a thousand"
                            .to_owned(),
                    });
                }
                if let Some(identity) = &definition.identity {
                    if let Some(declared) = definition.schema.fields.get(identity.field.as_str()) {
                        if declared.kind != field.kind {
                            defects.push(DefinitionError::RelationCarrierWrong {
                                entity: definition.entity.clone(),
                                relation: name.clone(),
                                via: relation.via.clone(),
                                detail: format!(
                                    "it is a {} field, and the owner's identity is a {} one",
                                    field.kind, declared.kind
                                ),
                            });
                        }
                    }
                }
            }
        }
        defects
    }

    /// The highest registered version of `entity`, which is the document a relation target names.
    fn latest(&self, entity: &str) -> Option<&ValidatedDefinition> {
        self.definitions.get(entity)?.values().next_back()
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

/// Records one relation's claim on one field, reporting the second claimant by name.
fn claim<'a>(
    carriers: &mut BTreeMap<(&'a str, &'a str), (&'a str, &'a str)>,
    defects: &mut Vec<DefinitionError>,
    entity: &'a str,
    field: &'a str,
    relation: &'a str,
) {
    match carriers.get(&(entity, field)) {
        Some((_, first)) => defects.push(DefinitionError::RelationFieldClaimedTwice {
            entity: entity.to_owned(),
            field: field.to_owned(),
            relation: (*first).to_owned(),
            other: relation.to_owned(),
        }),
        None => {
            carriers.insert((entity, field), (entity, relation));
        }
    }
}
