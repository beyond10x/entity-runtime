//! The kernel: the only code that produces a [`Decision`].

use crate::{
    validation::{apply_defaults, validate_object},
    Condition, CoreError, EntityDefinition, EventDefinition, Registry, RuleDefinition,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

/// One instance of an entity type: which definition it was created under, its identity, where it
/// is in its lifecycle, how many times it has changed, and its fields.
///
/// The kernel never mutates one of these. An operation takes an instance by reference and returns
/// a new one inside a [`Decision`]; the caller decides whether to keep it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EntityInstance {
    /// The definition's entity name.
    pub entity: String,
    /// The definition's version. Executed only against that definition.
    pub version: u32,
    /// The instance's identity, supplied by the caller at creation. Opaque to the kernel.
    pub id: String,
    /// The current lifecycle state. Written only by [`create`] and [`execute`].
    pub lifecycle_state: String,
    /// `1` after creation, `+1` per successful operation. What a store compares for optimistic
    /// concurrency.
    pub revision: u64,
    /// The fields, in name order.
    pub fields: BTreeMap<String, Value>,
}

/// A fact about what happened to an instance, materialised from an operation's event template.
///
/// This is the domain fact only. The envelope a log needs around it — an event id, the time it
/// was recorded, correlation and causation, the actor — is the shell's to add, because the kernel
/// has no clock and no id generator and must not pretend otherwise.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DomainEvent {
    /// The definition's entity name.
    pub entity: String,
    /// The definition's version.
    pub version: u32,
    /// The instance the event is about.
    pub id: String,
    /// The instance's revision *after* the operation that emitted this event.
    pub revision: u64,

    /// The event type, as declared in the definition.
    #[serde(rename = "type")]
    pub event_type: String,

    /// The payload, with every template reference resolved.
    pub payload: Value,
}

/// What the kernel decided: the instance as it is afterwards, and the events that describe how
/// it got there.
///
/// A `Decision` is the only thing the kernel produces. Persisting the instance, appending the
/// events and publishing them are the shell's, and are expected to happen together.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Decision {
    /// The instance after the operation.
    pub instance: EntityInstance,
    /// Zero or more events, in declaration order.
    pub events: Vec<DomainEvent>,
}

/// The kernel over a [`Registry`]: looks a definition up by the instance's `(entity, version)`
/// and hands it to [`create`] or [`execute`].
#[derive(Debug, Clone, Copy)]
pub struct Runtime<'a> {
    registry: &'a Registry,
}

impl<'a> Runtime<'a> {
    /// A runtime over `registry`.
    pub fn new(registry: &'a Registry) -> Self {
        Self { registry }
    }

    /// Creates an instance of `entity`/`version` with identity `id` and the given `fields`.
    ///
    /// # Errors
    ///
    /// [`CoreError::EntityNotRegistered`] when no such definition is registered; otherwise
    /// whatever [`create`] returns.
    pub fn create(
        &self,
        entity: &str,
        version: u32,
        id: impl Into<String>,
        fields: Value,
    ) -> Result<Decision, CoreError> {
        let definition = self.definition(entity, version)?;
        create(definition, id.into(), fields)
    }

    /// Executes `operation` on `instance` with the given `arguments`.
    ///
    /// # Errors
    ///
    /// [`CoreError::EntityNotRegistered`] when the instance's `(entity, version)` is not
    /// registered; otherwise whatever [`execute`] returns.
    pub fn execute(
        &self,
        instance: &EntityInstance,
        operation: &str,
        arguments: Value,
    ) -> Result<Decision, CoreError> {
        let definition = self.definition(&instance.entity, instance.version)?;
        execute(definition, instance, operation, arguments)
    }

    fn definition(&self, entity: &str, version: u32) -> Result<&EntityDefinition, CoreError> {
        self.registry
            .get(entity, version)
            .ok_or_else(|| CoreError::EntityNotRegistered {
                entity: entity.to_owned(),
                version,
            })
    }
}

/// Creates an instance under `definition`.
///
/// Defaults are applied to `fields`, the result is validated against the schema, the instance
/// enters the lifecycle's initial state at revision `1`, the invariants are checked, and the
/// creation event — if the definition declares one — is materialised.
///
/// # Errors
///
/// * [`CoreError::Validation`] — `fields` is not an object, or a value does not satisfy the schema;
///   every failure is listed.
/// * [`CoreError::InvariantViolation`] — an invariant does not hold for the new instance.
/// * [`CoreError::Template`] — the creation event references something that does not exist.
pub fn create(
    definition: &EntityDefinition,
    id: String,
    fields: Value,
) -> Result<Decision, CoreError> {
    let mut object = into_object(fields, "fields")?;
    apply_defaults(&definition.schema, &mut object);

    let validation = validate_object(&definition.schema, &object, "fields");
    if !validation.is_empty() {
        return Err(CoreError::Validation(validation));
    }

    let fields = map_to_btree(object);
    let instance = EntityInstance {
        entity: definition.entity.clone(),
        version: definition.version,
        id,
        lifecycle_state: definition.lifecycle.initial.clone(),
        revision: 1,
        fields,
    };

    let empty_args = Map::new();
    let empty_fields = BTreeMap::new();
    let context = TemplateContext {
        definition,
        id: &instance.id,
        args: &empty_args,
        old_fields: &empty_fields,
        new_fields: &instance.fields,
        from_state: None,
        to_state: &instance.lifecycle_state,
    };

    check_invariants(definition, &context)?;

    let mut events = Vec::new();
    if let Some(event) = &definition.create.emit {
        events.push(materialize_event(event, &context, instance.revision)?);
    }

    Ok(Decision { instance, events })
}

/// Executes `operation_name` on `instance` under `definition`.
///
/// The steps, in order: verify the instance matches the definition; find the operation; default
/// and validate the arguments; select the transition from the current state; evaluate the
/// preconditions; resolve every `set` assignment against the pre-operation fields; validate the
/// resulting fields; construct the next instance; evaluate the invariants against it; materialise
/// the events. A refusal at any step returns before the next, and `instance` is untouched.
///
/// # Errors
///
/// * [`CoreError::EntityMismatch`] — the instance was created under another definition.
/// * [`CoreError::OperationNotFound`] — no such operation.
/// * [`CoreError::Validation`] — an argument, or a field after `set`, does not satisfy its schema.
/// * [`CoreError::InvalidTransition`] — no transition starts from the current state.
/// * [`CoreError::PreconditionFailed`] — a precondition evaluated to `false`.
/// * [`CoreError::InvariantViolation`] — an invariant would not hold afterwards.
/// * [`CoreError::Template`] — a `set` value or event payload references something missing.
pub fn execute(
    definition: &EntityDefinition,
    instance: &EntityInstance,
    operation_name: &str,
    arguments: Value,
) -> Result<Decision, CoreError> {
    ensure_instance_matches(definition, instance)?;

    let operation =
        definition
            .operations
            .get(operation_name)
            .ok_or_else(|| CoreError::OperationNotFound {
                operation: operation_name.to_owned(),
            })?;

    let mut args = into_object(arguments, "arguments")?;
    apply_defaults(&operation.arguments, &mut args);
    let argument_errors = validate_object(&operation.arguments, &args, "arguments");
    if !argument_errors.is_empty() {
        return Err(CoreError::Validation(argument_errors));
    }

    let transition = operation
        .transitions
        .iter()
        .find(|transition| {
            transition
                .from
                .iter()
                .any(|state| state == &instance.lifecycle_state)
        })
        .ok_or_else(|| CoreError::InvalidTransition {
            operation: operation_name.to_owned(),
            state: instance.lifecycle_state.clone(),
        })?;

    let old_fields = instance.fields.clone();

    let precondition_context = TemplateContext {
        definition,
        id: &instance.id,
        args: &args,
        old_fields: &old_fields,
        new_fields: &old_fields,
        from_state: Some(&instance.lifecycle_state),
        to_state: &transition.to,
    };
    check_preconditions(
        operation_name,
        &operation.preconditions,
        &precondition_context,
    )?;

    let mut new_fields = old_fields.clone();

    // Field assignments see the pre-operation fields. This makes a set block
    // order-independent and therefore deterministic.
    for (field, template) in &operation.set {
        let value = resolve_template(template, &precondition_context)?;
        new_fields.insert(field.clone(), value);
    }

    let object = btree_to_map(&new_fields);
    let state_errors = validate_object(&definition.schema, &object, "fields");
    if !state_errors.is_empty() {
        return Err(CoreError::Validation(state_errors));
    }

    let next_revision = instance.revision.saturating_add(1);
    let next_instance = EntityInstance {
        entity: instance.entity.clone(),
        version: instance.version,
        id: instance.id.clone(),
        lifecycle_state: transition.to.clone(),
        revision: next_revision,
        fields: new_fields,
    };

    let context = TemplateContext {
        definition,
        id: &instance.id,
        args: &args,
        old_fields: &old_fields,
        new_fields: &next_instance.fields,
        from_state: Some(&instance.lifecycle_state),
        to_state: &next_instance.lifecycle_state,
    };

    check_invariants(definition, &context)?;

    let mut events = Vec::with_capacity(operation.emits.len());
    for event in &operation.emits {
        events.push(materialize_event(event, &context, next_revision)?);
    }

    Ok(Decision {
        instance: next_instance,
        events,
    })
}

fn check_preconditions(
    operation: &str,
    rules: &[RuleDefinition],
    context: &TemplateContext<'_>,
) -> Result<(), CoreError> {
    for rule in rules {
        if !evaluate_condition(&rule.condition, context)? {
            return Err(CoreError::PreconditionFailed {
                operation: operation.to_owned(),
                rule: rule.name.clone(),
                message: rule
                    .message
                    .clone()
                    .unwrap_or_else(|| "condition evaluated to false".into()),
            });
        }
    }
    Ok(())
}

fn check_invariants(
    definition: &EntityDefinition,
    context: &TemplateContext<'_>,
) -> Result<(), CoreError> {
    for rule in &definition.invariants {
        if !evaluate_condition(&rule.condition, context)? {
            return Err(CoreError::InvariantViolation {
                rule: rule.name.clone(),
                message: rule
                    .message
                    .clone()
                    .unwrap_or_else(|| "condition evaluated to false".into()),
            });
        }
    }
    Ok(())
}

fn evaluate_condition(
    condition: &Condition,
    context: &TemplateContext<'_>,
) -> Result<bool, CoreError> {
    match condition {
        Condition::Literal(value) => Ok(*value),
        Condition::All { all } => {
            for condition in all {
                if !evaluate_condition(condition, context)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        Condition::Any { any } => {
            for condition in any {
                if evaluate_condition(condition, context)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        Condition::Not { not } => Ok(!evaluate_condition(not, context)?),
        Condition::Exists { exists } => Ok(matches!(
            resolve_operand(exists, context)?,
            Operand::Present(_)
        )),
        Condition::Eq { eq } => compare_values(eq, context, |left, right| left == right),
        Condition::Ne { ne } => compare_values(ne, context, |left, right| left != right),
        Condition::Gt { gt } => compare_numbers(gt, context, |left, right| left > right),
        Condition::Gte { gte } => compare_numbers(gte, context, |left, right| left >= right),
        Condition::Lt { lt } => compare_numbers(lt, context, |left, right| left < right),
        Condition::Lte { lte } => compare_numbers(lte, context, |left, right| left <= right),
        Condition::In { values } => {
            let (needle, haystack) = resolve_pair(values, context)?;
            match (needle, haystack) {
                (Operand::Present(needle), Operand::Present(Value::Array(values))) => {
                    Ok(values.iter().any(|value| value == &needle))
                }
                _ => Ok(false),
            }
        }
        Condition::Contains { contains } => {
            let (container, needle) = resolve_pair(contains, context)?;
            match (container, needle) {
                (Operand::Present(Value::Array(values)), Operand::Present(needle)) => {
                    Ok(values.iter().any(|value| value == &needle))
                }
                (
                    Operand::Present(Value::String(value)),
                    Operand::Present(Value::String(needle)),
                ) => Ok(value.contains(&needle)),
                (Operand::Present(Value::Object(value)), Operand::Present(Value::String(key))) => {
                    Ok(value.contains_key(&key))
                }
                _ => Ok(false),
            }
        }
    }
}

fn compare_values(
    pair: &[Value; 2],
    context: &TemplateContext<'_>,
    predicate: impl FnOnce(&Value, &Value) -> bool,
) -> Result<bool, CoreError> {
    let (left, right) = resolve_pair(pair, context)?;
    match (left, right) {
        (Operand::Present(left), Operand::Present(right)) => Ok(predicate(&left, &right)),
        _ => Ok(false),
    }
}

fn compare_numbers(
    pair: &[Value; 2],
    context: &TemplateContext<'_>,
    predicate: impl FnOnce(f64, f64) -> bool,
) -> Result<bool, CoreError> {
    let (left, right) = resolve_pair(pair, context)?;
    match (left, right) {
        (Operand::Present(left), Operand::Present(right)) => {
            match (left.as_f64(), right.as_f64()) {
                (Some(left), Some(right)) => Ok(predicate(left, right)),
                _ => Ok(false),
            }
        }
        _ => Ok(false),
    }
}

fn resolve_pair(
    pair: &[Value; 2],
    context: &TemplateContext<'_>,
) -> Result<(Operand, Operand), CoreError> {
    Ok((
        resolve_operand(&pair[0], context)?,
        resolve_operand(&pair[1], context)?,
    ))
}

#[derive(Debug)]
enum Operand {
    Missing,
    Present(Value),
}

fn resolve_operand(value: &Value, context: &TemplateContext<'_>) -> Result<Operand, CoreError> {
    match value {
        Value::String(literal) if literal.starts_with("$$") => {
            Ok(Operand::Present(Value::String(literal[1..].to_owned())))
        }
        Value::String(expression) if expression.starts_with('$') => {
            resolve_expression_optional(expression, context)
        }
        Value::Array(values) => {
            let mut resolved = Vec::with_capacity(values.len());
            for value in values {
                match resolve_operand(value, context)? {
                    Operand::Present(value) => resolved.push(value),
                    Operand::Missing => return Ok(Operand::Missing),
                }
            }
            Ok(Operand::Present(Value::Array(resolved)))
        }
        Value::Object(values) => {
            let mut resolved = Map::new();
            for (key, value) in values {
                match resolve_operand(value, context)? {
                    Operand::Present(value) => {
                        resolved.insert(key.clone(), value);
                    }
                    Operand::Missing => return Ok(Operand::Missing),
                }
            }
            Ok(Operand::Present(Value::Object(resolved)))
        }
        other => Ok(Operand::Present(other.clone())),
    }
}

fn ensure_instance_matches(
    definition: &EntityDefinition,
    instance: &EntityInstance,
) -> Result<(), CoreError> {
    if definition.entity == instance.entity && definition.version == instance.version {
        return Ok(());
    }

    Err(CoreError::EntityMismatch {
        expected_entity: definition.entity.clone(),
        expected_version: definition.version,
        actual_entity: instance.entity.clone(),
        actual_version: instance.version,
    })
}

fn materialize_event(
    definition: &EventDefinition,
    context: &TemplateContext<'_>,
    revision: u64,
) -> Result<DomainEvent, CoreError> {
    Ok(DomainEvent {
        entity: context.definition.entity.clone(),
        version: context.definition.version,
        id: context.id.to_owned(),
        revision,
        event_type: definition.event_type.clone(),
        payload: resolve_template(&definition.payload, context)?,
    })
}

struct TemplateContext<'a> {
    definition: &'a EntityDefinition,
    id: &'a str,
    args: &'a Map<String, Value>,
    old_fields: &'a BTreeMap<String, Value>,
    new_fields: &'a BTreeMap<String, Value>,
    from_state: Option<&'a str>,
    to_state: &'a str,
}

fn resolve_template(value: &Value, context: &TemplateContext<'_>) -> Result<Value, CoreError> {
    match value {
        Value::String(literal) if literal.starts_with("$$") => {
            Ok(Value::String(literal[1..].to_owned()))
        }
        Value::String(expression) if expression.starts_with('$') => {
            resolve_expression(expression, context)
        }
        Value::Array(values) => values
            .iter()
            .map(|value| resolve_template(value, context))
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array),
        Value::Object(values) => {
            let mut resolved = Map::new();
            for (key, value) in values {
                resolved.insert(key.clone(), resolve_template(value, context)?);
            }
            Ok(Value::Object(resolved))
        }
        other => Ok(other.clone()),
    }
}

fn resolve_expression(expression: &str, context: &TemplateContext<'_>) -> Result<Value, CoreError> {
    match resolve_expression_optional(expression, context)? {
        Operand::Present(value) => Ok(value),
        Operand::Missing => Err(template_error(
            expression,
            "referenced value does not exist",
        )),
    }
}

fn resolve_expression_optional(
    expression: &str,
    context: &TemplateContext<'_>,
) -> Result<Operand, CoreError> {
    match expression {
        "$id" => Ok(Operand::Present(Value::String(context.id.to_owned()))),
        "$entity" => Ok(Operand::Present(Value::String(
            context.definition.entity.clone(),
        ))),
        "$version" => Ok(Operand::Present(Value::from(context.definition.version))),
        "$from_state" => Ok(match context.from_state {
            Some(state) => Operand::Present(Value::String(state.to_owned())),
            None => Operand::Missing,
        }),
        "$to_state" | "$state" => Ok(Operand::Present(Value::String(context.to_state.to_owned()))),
        "$args" => Ok(Operand::Present(Value::Object(context.args.clone()))),
        "$fields" => Ok(Operand::Present(Value::Object(btree_to_map(
            context.new_fields,
        )))),
        "$old_fields" => Ok(Operand::Present(Value::Object(btree_to_map(
            context.old_fields,
        )))),
        _ => {
            if let Some(path) = expression.strip_prefix("$args.") {
                return Ok(resolve_path_optional(
                    Value::Object(context.args.clone()),
                    path,
                ));
            }
            if let Some(path) = expression.strip_prefix("$fields.") {
                return Ok(resolve_path_optional(
                    Value::Object(btree_to_map(context.new_fields)),
                    path,
                ));
            }
            if let Some(path) = expression.strip_prefix("$old_fields.") {
                return Ok(resolve_path_optional(
                    Value::Object(btree_to_map(context.old_fields)),
                    path,
                ));
            }

            Err(template_error(expression, "unknown template expression"))
        }
    }
}

fn resolve_path_optional(mut value: Value, path: &str) -> Operand {
    if path.is_empty() {
        return Operand::Missing;
    }

    for segment in path.split('.') {
        value = match value {
            Value::Object(object) => match object.get(segment).cloned() {
                Some(value) => value,
                None => return Operand::Missing,
            },
            _ => return Operand::Missing,
        };
    }

    Operand::Present(value)
}

fn template_error(expression: &str, message: &str) -> CoreError {
    CoreError::Template {
        expression: expression.to_owned(),
        message: message.to_owned(),
    }
}

fn into_object(value: Value, path: &str) -> Result<Map<String, Value>, CoreError> {
    match value {
        Value::Object(object) => Ok(object),
        _ => Err(CoreError::Validation(vec![crate::ValidationError::new(
            path,
            "expected object",
        )])),
    }
}

fn map_to_btree(map: Map<String, Value>) -> BTreeMap<String, Value> {
    map.into_iter().collect()
}

fn btree_to_map(map: &BTreeMap<String, Value>) -> Map<String, Value> {
    map.iter()
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}
