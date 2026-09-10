//! Opt-in named command decisions; legacy definitions and replay keep their existing contract.
//!
//! Selection, mutation and verification remain in the pure kernel. External facts are explicit
//! inputs, never callbacks. See `docs/design/kernel-outcomes-v1.md` for the capability boundary.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::runtime::{
    canonical_object, evaluate_condition, materialize_event, resolve_template, TemplateContext,
};
use crate::validation::{self, Scope, ScopeKind};
use crate::{
    Condition, CoreError, DefinitionErrors, DomainEvent, EntityDefinition, EntityInstance,
    EventDefinition, FieldKind, ObjectSchema, OperationDefinition, TransitionDefinition, Truth,
    ValidatedDefinition,
};

/// The format discriminator is independent of the entity's domain definition version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DefinitionFormat {
    /// The first explicitly selected outcome profile.
    #[serde(rename = "entity-outcome-definition/1")]
    V1,
}

/// One entity's named command semantics, parsed but not yet validated.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Definition {
    /// Explicit profile identity; never inferred from other fields.
    pub format: DefinitionFormat,
    /// Schema, lifecycle and invariants. Legacy operations and creation events must be empty.
    pub entity: EntityDefinition,
    /// Public commands; internal branch programs are not caller-addressable.
    pub commands: BTreeMap<String, Command>,
}

/// Arguments, external observations and every possible named outcome of one command.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Command {
    /// Caller input, defaulted and validated before selection.
    pub arguments: ObjectSchema,
    /// Explicit Boolean facts supplied by trusted bindings; defaults are forbidden.
    pub observations: ObjectSchema,
    /// Named alternatives, with exactly one otherwise branch.
    pub outcomes: BTreeMap<String, Outcome>,
}

/// A condition and its selected effect, inseparable in the persisted definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Outcome {
    /// What the kernel must observe before choosing this outcome.
    pub condition: Selection,
    /// What this outcome does, after selection has succeeded.
    pub effect: Effect,
}

/// Selection has no implicit priority. Multiple true conditions are ambiguous.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "condition", rename_all = "snake_case", deny_unknown_fields)]
pub enum Selection {
    /// A predicate over normalized arguments and supplied identity/type/version.
    When {
        /// Uses the current ER condition vocabulary; unsupported ESS operators are not erased.
        predicate: Condition,
    },
    /// Used only when every nondefault condition is observed false.
    Otherwise {},
    /// A separately declared Boolean observation; omission is Unknown.
    External {
        /// Exact declared field, independent of caller arguments.
        field: String,
    },
    /// A lowerer-derived set of declared states that this command treats as wrong.
    WrongState {
        /// No current instance means false; an unknown lifecycle state is refused before selection.
        states: Vec<String>,
    },
}

/// An event template and the payload contract that materialization must satisfy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TypedEvent {
    /// Event name and recursively resolved payload template.
    pub template: EventDefinition,
    /// Payload schema, validated both at registration and after materialization.
    pub schema: ObjectSchema,
}

/// Effects have disjoint shapes, preventing a refusal from also mutating state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "effect", rename_all = "snake_case", deny_unknown_fields)]
pub enum Effect {
    /// Create one instance, with every event at revision one.
    Create {
        /// Field templates reading arguments, never previous state.
        set: BTreeMap<String, Value>,
        /// Ordered events materialized after all invariants hold.
        emits: Vec<TypedEvent>,
    },
    /// Apply one ordinary kernel operation after this branch is selected.
    Change {
        /// The branch's own allowed transitions.
        transitions: Vec<TransitionDefinition>,
        /// Assignments evaluated against prior fields.
        set: BTreeMap<String, Value>,
        /// Ordered events at the resulting entity revision.
        emits: Vec<TypedEvent>,
    },
    /// Accepted observation, with unchanged optional state and no events.
    Observe {},
    /// Declared business refusal, also with unchanged optional state and no events.
    Refuse {
        /// Declared error identity, not a kernel diagnostic name.
        error: String,
        /// Error payload template over the normalized input.
        payload: Value,
        /// The payload must satisfy this schema before the record is returned.
        schema: ObjectSchema,
    },
}

/// An invocation contains no selected branch; the kernel owns that choice.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Invocation {
    /// Declared command name.
    pub command: String,
    /// Existing or prospective entity identity, supplied at the boundary.
    pub id: String,
    /// Original arguments on input; normalized arguments in a returned record.
    pub arguments: Map<String, Value>,
    /// Observed external facts, distinct from ordinary arguments.
    pub observations: Map<String, Value>,
}

/// A declared business error that does not change entity state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BusinessError {
    /// The declared identity.
    pub name: String,
    /// The schema-validated payload.
    pub payload: Value,
}

/// Explicit record dispatch; old decision readers cannot silently accept this envelope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecordFormat {
    /// Complete named command decision or observation.
    #[serde(rename = "entity-outcome-record/1")]
    V1,
}

/// Complete comparison evidence for replay, including observations before an entity exists.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    /// Required format identity.
    pub format: RecordFormat,
    /// Exact semantics used for this invocation.
    pub definition: Definition,
    /// Normalized command and separately observed inputs.
    pub invocation: Invocation,
    /// Selected outcome to verify again during replay, never a dispatch instruction.
    pub outcome: String,
    /// Absent before creation, unchanged after an observation.
    #[serde(deserialize_with = "read_instance")]
    pub result: Option<EntityInstance>,
    /// Present only for a declared refusing outcome.
    pub error: Option<BusinessError>,
    /// Every domain event, in original order, including an empty vector.
    #[serde(deserialize_with = "read_events")]
    pub events: Vec<DomainEvent>,
}

/// A profile diagnostic; business refusals are successful records instead.
#[derive(Debug, Clone, PartialEq)]
pub enum Failure {
    /// A malformed definition, including a dormant branch.
    Definition {
        /// Location in the new definition.
        path: String,
        /// All available details from the underlying validator.
        detail: String,
    },
    /// A schema, invariant, transition or template failure from the shared kernel.
    Core(CoreError),
    /// No declared command has this name.
    UnknownCommand(String),
    /// Selection cannot be completed with the supplied observations.
    Unobservable {
        /// Undecidable outcomes, in stable name order.
        outcomes: Vec<String>,
        /// Missing input addresses, sorted and deduplicated.
        paths: Vec<String>,
    },
    /// Several nondefault outcomes hold; none is implicitly preferred.
    Ambiguous {
        /// Matching outcome names in stable order.
        outcomes: Vec<String>,
    },
    /// A changing branch needs an existing instance.
    InstanceRequired,
    /// Creation cannot overwrite an existing instance.
    InstanceAlreadyExists,
    /// The invocation does not address the supplied instance.
    IdentityMismatch,
    /// A record disagrees with the complete recomputed history.
    Replay {
        /// First disagreeing record.
        index: usize,
        /// The violated history contract.
        detail: String,
    },
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Failure {}
impl From<CoreError> for Failure {
    fn from(value: CoreError) -> Self {
        Self::Core(value)
    }
}
fn defect(path: &str, detail: impl ToString) -> Failure {
    Failure::Definition {
        path: path.into(),
        detail: detail.to_string(),
    }
}
fn schema(schema: &ObjectSchema, path: &str) -> Result<(), Failure> {
    let errors = validation::validate_schema_definition(schema, path);
    if errors.is_empty() {
        Ok(())
    } else {
        Err(defect(path, DefinitionErrors::new(errors)))
    }
}
fn check_object(
    schema: &ObjectSchema,
    object: &Map<String, Value>,
    path: &str,
) -> Result<(), Failure> {
    let errors = validation::validate_object(schema, object, path);
    if errors.is_empty() {
        Ok(())
    } else {
        Err(CoreError::Validation(errors).into())
    }
}
fn check_payload(schema: &ObjectSchema, value: &Value, path: &str) -> Result<(), Failure> {
    let object = value.as_object().ok_or_else(|| {
        CoreError::Validation(vec![crate::ValidationError::new(
            path,
            "expected object payload",
        )])
    })?;
    check_object(schema, object, path)
}

/// An immutable executable handle; all branches pass registration before one can be selected.
#[derive(Debug, Clone)]
pub struct Validated {
    definition: Definition,
    base: ValidatedDefinition,
    changes: BTreeMap<(String, String), ValidatedDefinition>,
}

impl Validated {
    /// Validate the complete definition, retaining prepared branch operations privately.
    ///
    /// # Errors
    /// A located definition failure; no partial handle is returned.
    pub fn new(definition: Definition) -> Result<Self, Failure> {
        let base =
            ValidatedDefinition::new(definition.entity.clone()).map_err(|e| defect("entity", e))?;
        if base.create.emit.is_some() || !base.operations.is_empty() {
            return Err(defect(
                "entity",
                "legacy creation events and operations must be empty",
            ));
        }
        if definition.commands.is_empty() {
            return Err(defect("commands", "at least one command is required"));
        }
        let mut changes = BTreeMap::new();
        for (command_name, command) in &definition.commands {
            let path = format!("commands.{command_name}");
            if command_name.trim().is_empty() {
                return Err(defect(&path, "command name is empty"));
            }
            schema(&command.arguments, &format!("{path}.arguments"))?;
            schema(&command.observations, &format!("{path}.observations"))?;
            if command.observations.additional_fields
                || command
                    .observations
                    .fields
                    .values()
                    .any(|f| f.kind != FieldKind::Boolean || f.default.as_value().is_some())
            {
                return Err(defect(
                    &path,
                    "observations must be closed Boolean fields without defaults",
                ));
            }
            if command
                .outcomes
                .values()
                .filter(|o| matches!(o.condition, Selection::Otherwise {}))
                .count()
                != 1
            {
                return Err(defect(&path, "exactly one otherwise outcome is required"));
            }
            let input_scope = Scope {
                kind: ScopeKind::Input,
                fields: &base.schema,
                args: Some(&command.arguments),
            };
            for (name, outcome) in &command.outcomes {
                let at = format!("{path}.outcomes.{name}");
                if name.trim().is_empty() {
                    return Err(defect(&at, "outcome name is empty"));
                }
                match &outcome.condition {
                    Selection::When { predicate } => {
                        validation::validate_condition_definition(predicate, &at, input_scope)
                            .map_err(|e| defect(&at, e))?
                    }
                    Selection::External { field }
                        if !command.observations.fields.contains_key(field) =>
                    {
                        return Err(defect(
                            &at,
                            "external condition names no declared observation",
                        ))
                    }
                    Selection::WrongState { states }
                        if states.is_empty()
                            || states.iter().collect::<BTreeSet<_>>().len() != states.len()
                            || states.iter().any(|s| !base.lifecycle.states.contains(s)) =>
                    {
                        return Err(defect(&at, "wrong states must be distinct declared states"))
                    }
                    _ => {}
                }
                match &outcome.effect {
                    Effect::Create { set, emits } => {
                        for (field, value) in set {
                            if !base.schema.additional_fields
                                && !base.schema.fields.contains_key(field)
                            {
                                return Err(defect(
                                    &at,
                                    format!("undeclared creation field {field}"),
                                ));
                            }
                            validation::validate_template(value, &at, input_scope)
                                .map_err(|e| defect(&at, e))?;
                        }
                        for event in emits {
                            schema(&event.schema, &at)?;
                            validation::validate_event_definition(
                                &event.template,
                                &at,
                                None,
                                Scope {
                                    kind: ScopeKind::CommandCreate,
                                    ..input_scope
                                },
                            )
                            .map_err(|e| defect(&at, e))?;
                        }
                    }
                    Effect::Change {
                        transitions,
                        set,
                        emits,
                    } => {
                        for event in emits {
                            schema(&event.schema, &at)?;
                        }
                        let mut prepared = base.as_definition().clone();
                        prepared.operations.insert(
                            command_name.clone(),
                            OperationDefinition {
                                arguments: command.arguments.clone(),
                                transitions: transitions.clone(),
                                preconditions: Vec::new(),
                                set: set.clone(),
                                emits: emits.iter().map(|e| e.template.clone()).collect(),
                            },
                        );
                        changes.insert(
                            (command_name.clone(), name.clone()),
                            ValidatedDefinition::new(prepared).map_err(|e| defect(&at, e))?,
                        );
                    }
                    Effect::Refuse {
                        error,
                        payload,
                        schema: shape,
                    } => {
                        if error.trim().is_empty() {
                            return Err(defect(&at, "error name is empty"));
                        }
                        schema(shape, &at)?;
                        validation::validate_template(payload, &at, input_scope)
                            .map_err(|e| defect(&at, e))?;
                    }
                    Effect::Observe {} => {}
                }
            }
        }
        Ok(Self {
            definition,
            base,
            changes,
        })
    }

    /// The exact source definition; private prepared programs are never published as history.
    #[must_use]
    pub fn definition(&self) -> &Definition {
        &self.definition
    }

    /// Select and execute one command without modifying the supplied instance.
    ///
    /// # Errors
    /// Unobservable/ambiguous selection, invalid input or a shared kernel refusal. A declared
    /// business refusal instead returns a record with unchanged optional state.
    pub fn decide(
        &self,
        before: Option<&EntityInstance>,
        mut invocation: Invocation,
    ) -> Result<Record, Failure> {
        if invocation.id.trim().is_empty() {
            return Err(Failure::IdentityMismatch);
        }
        if let Some(instance) = before {
            if instance.id != invocation.id {
                return Err(Failure::IdentityMismatch);
            }
            crate::runtime::ensure_instance_matches(&self.base, instance)?;
            if instance.revision == 0 || instance.revision > i64::MAX as u64 {
                return Err(Failure::IdentityMismatch);
            }
            check_object(&self.base.schema, &instance.fields, "before.fields")?;
        }
        let command = self
            .definition
            .commands
            .get(&invocation.command)
            .ok_or_else(|| Failure::UnknownCommand(invocation.command.clone()))?;
        validation::apply_defaults(&command.arguments, &mut invocation.arguments);
        invocation.arguments = canonical_object(invocation.arguments);
        invocation.observations = canonical_object(invocation.observations);
        check_object(&command.arguments, &invocation.arguments, "arguments")?;
        check_object(
            &command.observations,
            &invocation.observations,
            "observations",
        )?;
        let empty = Map::new();
        let fields = before.map_or(&empty, |i| &i.fields);
        let context = TemplateContext {
            definition: &self.base,
            id: &invocation.id,
            args: &invocation.arguments,
            old_fields: fields,
            new_fields: fields,
            from_state: before.map(|i| i.lifecycle_state.as_str()),
            to_state: before.map_or(self.base.lifecycle.initial.as_str(), |i| {
                i.lifecycle_state.as_str()
            }),
        };
        let mut matches = Vec::new();
        let mut unknown = Vec::new();
        let mut paths = BTreeSet::new();
        let mut otherwise = None;
        for (name, outcome) in &command.outcomes {
            let truth = match &outcome.condition {
                Selection::Otherwise {} => {
                    otherwise = Some(name);
                    continue;
                }
                Selection::When { predicate } => {
                    evaluate_condition(predicate, &context, &mut paths)?
                }
                Selection::External { field } => invocation
                    .observations
                    .get(field)
                    .and_then(Value::as_bool)
                    .map_or_else(
                        || {
                            paths.insert(format!("observations.{field}"));
                            Truth::Unknown
                        },
                        Truth::from_bool,
                    ),
                Selection::WrongState { states } => {
                    Truth::from_bool(before.is_some_and(|i| states.contains(&i.lifecycle_state)))
                }
            };
            match truth {
                Truth::True => matches.push(name.clone()),
                Truth::Unknown => unknown.push(name.clone()),
                Truth::False => {}
            }
        }
        if !unknown.is_empty() {
            return Err(Failure::Unobservable {
                outcomes: unknown,
                paths: paths.into_iter().collect(),
            });
        }
        if matches.len() > 1 {
            return Err(Failure::Ambiguous { outcomes: matches });
        }
        let name = matches
            .first()
            .unwrap_or_else(|| otherwise.expect("registration requires one default"));
        let effect = &command.outcomes[name].effect;
        let mut result = before.cloned();
        let mut events = Vec::new();
        let mut error = None;
        match effect {
            Effect::Create { set, emits } => {
                if before.is_some() {
                    return Err(Failure::InstanceAlreadyExists);
                }
                let fields = set
                    .iter()
                    .map(|(key, value)| Ok((key.clone(), resolve_template(value, &context)?)))
                    .collect::<Result<Map<_, _>, CoreError>>()?;
                let decision =
                    crate::create(&self.base, invocation.id.clone(), Value::Object(fields))?;
                let context = TemplateContext {
                    new_fields: &decision.instance.fields,
                    to_state: &decision.instance.lifecycle_state,
                    ..context
                };
                for event in emits {
                    events.push(materialize_event(
                        &event.template,
                        &context,
                        1,
                        &invocation.arguments,
                    )?);
                }
                result = Some(decision.instance);
            }
            Effect::Change { .. } => {
                let before = before.ok_or(Failure::InstanceRequired)?;
                let prepared = &self.changes[&(invocation.command.clone(), name.clone())];
                let decision = crate::execute(
                    prepared,
                    before,
                    &invocation.command,
                    Value::Object(invocation.arguments.clone()),
                )?;
                events = decision.events;
                result = Some(decision.instance);
            }
            Effect::Observe {} => {}
            Effect::Refuse {
                error: identity,
                payload,
                schema,
            } => {
                let payload = resolve_template(payload, &context)?;
                check_payload(schema, &payload, "error.payload")?;
                error = Some(BusinessError {
                    name: identity.clone(),
                    payload,
                });
            }
        }
        if let Effect::Create { emits, .. } | Effect::Change { emits, .. } = effect {
            for (index, (event, typed)) in events.iter().zip(emits).enumerate() {
                check_payload(
                    &typed.schema,
                    &event.payload,
                    &format!("events[{index}].payload"),
                )?;
            }
        }
        Ok(Record {
            format: RecordFormat::V1,
            definition: self.definition.clone(),
            invocation,
            outcome: name.clone(),
            result,
            error,
            events,
        })
    }
}

/// Recompute a complete, single-identity history, including pre-creation observations.
///
/// # Errors
/// A changed definition/identity, an invalid invocation or a record differing from recomputation.
pub fn replay(records: &[Record]) -> Result<Option<EntityInstance>, Failure> {
    let Some(first) = records.first() else {
        return Ok(None);
    };
    let definition = Validated::new(first.definition.clone())?;
    let mut instance = None;
    for (index, record) in records.iter().enumerate() {
        let refuse = |detail: &str| Failure::Replay {
            index,
            detail: detail.into(),
        };
        if record.definition != first.definition {
            return Err(refuse("definition changed without a migration boundary"));
        }
        if record.invocation.id != first.invocation.id {
            return Err(refuse("history contains another identity"));
        }
        let computed = definition.decide(instance.as_ref(), record.invocation.clone())?;
        if computed != *record {
            return Err(refuse("record differs from recomputed outcome decision"));
        }
        instance = computed.result;
    }
    Ok(instance)
}

// Legacy state/event types deliberately retain their old readers. New records close those
// nested envelopes locally instead of making an unrelated compatibility change to those types.
fn closed(value: &Value, allowed: &[&str]) -> Result<(), String> {
    let object = value
        .as_object()
        .ok_or_else(|| "expected object".to_owned())?;
    for key in object.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(format!("unknown field `{key}`"));
        }
    }
    Ok(())
}
fn read_instance<'de, D: serde::Deserializer<'de>>(
    reader: D,
) -> Result<Option<EntityInstance>, D::Error> {
    let value = Option::<Value>::deserialize(reader)?;
    value
        .map(|value| {
            closed(
                &value,
                &[
                    "entity",
                    "version",
                    "id",
                    "lifecycle_state",
                    "revision",
                    "fields",
                ],
            )
            .map_err(serde::de::Error::custom)?;
            serde_json::from_value(value).map_err(serde::de::Error::custom)
        })
        .transpose()
}
fn read_events<'de, D: serde::Deserializer<'de>>(reader: D) -> Result<Vec<DomainEvent>, D::Error> {
    Vec::<Value>::deserialize(reader)?
        .into_iter()
        .map(|value| {
            closed(
                &value,
                &[
                    "entity",
                    "version",
                    "id",
                    "revision",
                    "type",
                    "from_state",
                    "to_state",
                    "changed",
                    "args",
                    "payload",
                ],
            )
            .map_err(serde::de::Error::custom)?;
            serde_json::from_value(value).map_err(serde::de::Error::custom)
        })
        .collect()
}
