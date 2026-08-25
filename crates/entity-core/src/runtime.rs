//! The kernel: the only code that produces a [`Decision`].

use crate::{
    validation::{apply_defaults, validate_object},
    Condition, CoreError, EntityDefinition, EventDefinition, Registry, RuleDefinition, Truth,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeSet;

/// One instance of an entity type: which definition it was created under, its identity, where it
/// is in its lifecycle, how many times it has changed, and its fields.
///
/// The kernel never mutates one of these. An operation takes an instance by reference and returns
/// a new one inside a [`Decision`]; the caller decides whether to keep it.
///
/// # What the kernel can and cannot check about an instance it is handed
///
/// The fields are public and the type is `Deserialize`, because an instance is *data* that a
/// store round-trips. The kernel therefore cannot know whether the instance in front of it is one
/// it produced — that is the shell's to know, and it is why storing an instance and appending its
/// events is a single job. What the kernel does check is that the instance could exist at all:
/// its `(entity, version)` must match the definition, and its `lifecycle_state` must be one the
/// definition declares (else [`CoreError::UnknownState`]). Whatever state it legitimately claims,
/// the next state is the kernel's alone — only [`create`] and [`execute`] write one.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EntityInstance {
    /// The definition's entity name.
    pub entity: String,
    /// The definition's version. Executed only against that definition.
    pub version: u32,
    /// The instance's identity, supplied by the caller at creation. Opaque to the kernel, and
    /// never empty.
    pub id: String,
    /// The current lifecycle state. Written only by [`create`] and [`execute`].
    pub lifecycle_state: String,
    /// `1` after creation, `+1` per successful operation. What a store compares for optimistic
    /// concurrency.
    pub revision: u64,
    /// The fields, in name order. A `serde_json::Map` without `preserve_order`, so iteration and
    /// serialisation are sorted and two identical decisions produce identical bytes.
    pub fields: Map<String, Value>,
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
/// * [`CoreError::Validation`] — `id` is empty, `fields` is not an object, or a value does not
///   satisfy the schema; every field failure is listed.
/// * [`CoreError::InvariantViolation`] — an invariant does not hold for the new instance.
/// * [`CoreError::Template`] — the creation event references something that does not exist.
pub fn create(
    definition: &EntityDefinition,
    id: String,
    fields: Value,
) -> Result<Decision, CoreError> {
    if id.trim().is_empty() {
        return Err(CoreError::Validation(vec![crate::ValidationError::new(
            "id",
            "identity cannot be empty; the kernel generates none, so the caller supplies one",
        )]));
    }

    let mut object = into_object(fields, "fields")?;
    apply_defaults(&definition.schema, &mut object);

    let validation = validate_object(&definition.schema, &object, "fields");
    if !validation.is_empty() {
        return Err(CoreError::Validation(validation));
    }

    let instance = EntityInstance {
        entity: definition.entity.clone(),
        version: definition.version,
        id,
        lifecycle_state: definition.lifecycle.initial.clone(),
        revision: 1,
        fields: object,
    };

    let empty = Map::new();
    let context = TemplateContext {
        definition,
        id: &instance.id,
        args: &empty,
        old_fields: &empty,
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
/// The steps, in order: verify the instance matches the definition and carries a declared state;
/// find the operation; default and validate the arguments; select the transition from the current
/// state; evaluate the preconditions; resolve every `set` assignment against the pre-operation
/// fields; validate the resulting fields; construct the next instance; evaluate the invariants
/// against it; materialise the events. A refusal at any step returns before the next, and
/// `instance` is untouched.
///
/// # Errors
///
/// * [`CoreError::EntityMismatch`] — the instance was created under another definition.
/// * [`CoreError::UnknownState`] — the instance claims a state the definition does not declare.
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

    // The pre-operation fields are borrowed, never copied: they are only ever read, and `set`
    // resolves every assignment against them.
    let old_fields = &instance.fields;

    let context = TemplateContext {
        definition,
        id: &instance.id,
        args: &args,
        old_fields,
        new_fields: old_fields,
        from_state: Some(&instance.lifecycle_state),
        to_state: &transition.to,
    };
    check_preconditions(operation_name, &operation.preconditions, &context)?;

    let mut new_fields = old_fields.clone();
    for (field, template) in &operation.set {
        let value = resolve_template(template, &context)?;
        new_fields.insert(field.clone(), value);
    }

    let state_errors = validate_object(&definition.schema, &new_fields, "fields");
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
        old_fields,
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

/// Every address a condition read and found nothing at, gathered as it is evaluated.
///
/// A `BTreeSet` rather than a `Vec`: sorted, so the same refusal prints the same way every time,
/// and without repeats, so a reference read by three operands is named once.
type Unobserved = BTreeSet<String>;

fn check_preconditions(
    operation: &str,
    rules: &[RuleDefinition],
    context: &TemplateContext<'_>,
) -> Result<(), CoreError> {
    for rule in rules {
        let mut unobserved = Unobserved::new();
        match evaluate_condition(&rule.condition, context, &mut unobserved)? {
            Truth::True => {}
            Truth::False => {
                return Err(CoreError::PreconditionFailed {
                    operation: operation.to_owned(),
                    rule: rule.name.clone(),
                    message: rule
                        .message
                        .clone()
                        .unwrap_or_else(|| "condition evaluated to false".into()),
                })
            }
            Truth::Unknown => {
                return Err(CoreError::PreconditionUnobservable {
                    operation: operation.to_owned(),
                    rule: rule.name.clone(),
                    message: rule
                        .message
                        .clone()
                        .unwrap_or_else(|| "condition could not be evaluated".into()),
                    unresolved: unobserved.into_iter().collect(),
                })
            }
        }
    }
    Ok(())
}

fn check_invariants(
    definition: &EntityDefinition,
    context: &TemplateContext<'_>,
) -> Result<(), CoreError> {
    for rule in &definition.invariants {
        let mut unobserved = Unobserved::new();
        match evaluate_condition(&rule.condition, context, &mut unobserved)? {
            Truth::True => {}
            Truth::False => {
                return Err(CoreError::InvariantViolation {
                    rule: rule.name.clone(),
                    message: rule
                        .message
                        .clone()
                        .unwrap_or_else(|| "condition evaluated to false".into()),
                })
            }
            Truth::Unknown => {
                return Err(CoreError::InvariantUnobservable {
                    rule: rule.name.clone(),
                    message: rule
                        .message
                        .clone()
                        .unwrap_or_else(|| "condition could not be evaluated".into()),
                    unresolved: unobserved.into_iter().collect(),
                })
            }
        }
    }
    Ok(())
}

/// Evaluates a condition to [`Truth`], recording every address a *value* question read and found
/// nothing at.
///
/// `Unknown` belongs to the question, not to the operator. Asking whether a value is there is
/// always answerable; asking what it says is not, when it is not there.
///
/// `all` and `any` evaluate **every** operand, deliberately. Kleene's connectives are
/// order-independent, so the truth value is the same either way — but *which* unobserved
/// addresses have been recorded when the answer comes back is not, and a refusal that names one
/// missing fact out of three costs the operator three round trips. Evaluation here is pure and
/// cannot fail partway, so there is nothing to be bought by stopping early.
fn evaluate_condition(
    condition: &Condition,
    context: &TemplateContext<'_>,
    unobserved: &mut Unobserved,
) -> Result<Truth, CoreError> {
    match condition {
        Condition::Literal(value) => Ok(Truth::from_bool(*value)),
        Condition::All { all } => {
            let mut result = Truth::True;
            for condition in all {
                result = result.and(evaluate_condition(condition, context, unobserved)?);
            }
            Ok(result)
        }
        Condition::Any { any } => {
            let mut result = Truth::False;
            for condition in any {
                result = result.or(evaluate_condition(condition, context, unobserved)?);
            }
            Ok(result)
        }
        Condition::Not { not } => Ok(evaluate_condition(not, context, unobserved)?.not()),
        Condition::Exists { exists } => {
            // A question about the store, not about a value: the kernel holds the instance, so it
            // can always answer it. Nothing goes into `unobserved` — this operator *is* the
            // observation, and in `any: [{exists: $fields.a}, {eq: [$fields.b, 1]}]` naming
            // `$fields.a` beside the genuinely unreadable `$fields.b` would send whoever reads the
            // refusal after the wrong one.
            let mut answered = Unobserved::new();
            let resolved = resolve_operand(exists, context, &mut answered)?;
            Ok(Truth::from_bool(resolved.is_some()))
        }
        Condition::Eq { eq } => compare_values(eq, context, unobserved, values_equal),
        Condition::Ne { ne } => compare_values(ne, context, unobserved, |left, right| {
            !values_equal(left, right)
        }),
        Condition::Gt { gt } => {
            compare_numbers(gt, context, unobserved, |left, right| left > right)
        }
        Condition::Gte { gte } => {
            compare_numbers(gte, context, unobserved, |left, right| left >= right)
        }
        Condition::Lt { lt } => {
            compare_numbers(lt, context, unobserved, |left, right| left < right)
        }
        Condition::Lte { lte } => {
            compare_numbers(lte, context, unobserved, |left, right| left <= right)
        }
        Condition::In { values } => {
            let (needle, haystack) = resolve_pair(values, context, unobserved)?;
            match (needle, haystack) {
                (Some(needle), Some(Value::Array(values))) => Ok(Truth::from_bool(
                    values.iter().any(|value| values_equal(value, &needle)),
                )),
                // Both resolved, and the haystack is not a list: observed, and it does not hold.
                (Some(_), Some(_)) => Ok(Truth::False),
                _ => Ok(Truth::Unknown),
            }
        }
        Condition::Contains { contains } => {
            let (container, needle) = resolve_pair(contains, context, unobserved)?;
            match (container, needle) {
                (Some(Value::Array(values)), Some(needle)) => Ok(Truth::from_bool(
                    values.iter().any(|value| values_equal(value, &needle)),
                )),
                (Some(Value::String(value)), Some(Value::String(needle))) => {
                    Ok(Truth::from_bool(value.contains(&needle)))
                }
                (Some(Value::Object(value)), Some(Value::String(key))) => {
                    Ok(Truth::from_bool(value.contains_key(&key)))
                }
                (Some(_), Some(_)) => Ok(Truth::False),
                _ => Ok(Truth::Unknown),
            }
        }
    }
}

/// Equality that agrees with the ordering operators.
///
/// `serde_json`'s own `==` compares a number's *representation*, so `100` and `100.0` are unequal
/// — while `gte`/`lte` compare through `f64` and call them equal. A definition tested with integer
/// fixtures would then refuse the same document written with a decimal point. Numbers here compare
/// numerically, at every depth; everything else compares structurally.
fn values_equal(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Number(left), Value::Number(right)) => match (left.as_f64(), right.as_f64()) {
            (Some(left), Some(right)) => left == right,
            _ => left == right,
        },
        (Value::Array(left), Value::Array(right)) => {
            left.len() == right.len()
                && left
                    .iter()
                    .zip(right.iter())
                    .all(|(left, right)| values_equal(left, right))
        }
        (Value::Object(left), Value::Object(right)) => {
            left.len() == right.len()
                && left.iter().all(|(key, left)| {
                    right
                        .get(key)
                        .is_some_and(|right| values_equal(left, right))
                })
        }
        _ => left == right,
    }
}

fn compare_values(
    pair: &[Value; 2],
    context: &TemplateContext<'_>,
    unobserved: &mut Unobserved,
    predicate: impl FnOnce(&Value, &Value) -> bool,
) -> Result<Truth, CoreError> {
    let (left, right) = resolve_pair(pair, context, unobserved)?;
    match (left, right) {
        (Some(left), Some(right)) => Ok(Truth::from_bool(predicate(&left, &right))),
        _ => Ok(Truth::Unknown),
    }
}

fn compare_numbers(
    pair: &[Value; 2],
    context: &TemplateContext<'_>,
    unobserved: &mut Unobserved,
    predicate: impl FnOnce(f64, f64) -> bool,
) -> Result<Truth, CoreError> {
    let (left, right) = resolve_pair(pair, context, unobserved)?;
    match (left, right) {
        // Both resolved. Not being numbers is an observation about them, not an absence.
        (Some(left), Some(right)) => match (left.as_f64(), right.as_f64()) {
            (Some(left), Some(right)) => Ok(Truth::from_bool(predicate(left, right))),
            _ => Ok(Truth::False),
        },
        _ => Ok(Truth::Unknown),
    }
}

fn resolve_pair(
    pair: &[Value; 2],
    context: &TemplateContext<'_>,
    unobserved: &mut Unobserved,
) -> Result<(Option<Value>, Option<Value>), CoreError> {
    // Both sides, always: the left one failing to resolve must not hide the right one's address.
    let left = resolve_operand(&pair[0], context, unobserved)?;
    let right = resolve_operand(&pair[1], context, unobserved)?;
    Ok((left, right))
}

/// Resolves an operand, where nothing to observe is `None` rather than an error — which is what
/// makes a comparison against it [`Truth::Unknown`].
///
/// Two things count as nothing to observe, and the second is the one that matters in practice: a
/// reference that names no key, and a reference to a key that is **present and null**. `key:` with
/// nothing after it is how YAML front matter spells *nobody filled this in*, and a gate that
/// exists to catch exactly that must not read it as a value. A `null` written as a literal in the
/// definition is left alone — the author wrote it, so it is an observation.
///
/// Every address that resolves to nothing is recorded in `unobserved`, including inside a list or
/// mapping operand, so the refusal can name all of them at once.
fn resolve_operand(
    value: &Value,
    context: &TemplateContext<'_>,
    unobserved: &mut Unobserved,
) -> Result<Option<Value>, CoreError> {
    match value {
        Value::String(literal) if literal.starts_with("$$") => {
            Ok(Some(Value::String(literal[1..].to_owned())))
        }
        Value::String(expression) if expression.starts_with('$') => {
            match resolve_expression_optional(expression, context)? {
                Some(Value::Null) | None => {
                    unobserved.insert(expression.clone());
                    Ok(None)
                }
                resolved => Ok(resolved),
            }
        }
        Value::Array(values) => {
            let mut resolved = Vec::with_capacity(values.len());
            let mut complete = true;
            for value in values {
                // Keep going after the first gap: the addresses are the point.
                match resolve_operand(value, context, unobserved)? {
                    Some(value) => resolved.push(value),
                    None => complete = false,
                }
            }
            Ok(complete.then_some(Value::Array(resolved)))
        }
        Value::Object(values) => {
            let mut resolved = Map::new();
            let mut complete = true;
            for (key, value) in values {
                match resolve_operand(value, context, unobserved)? {
                    Some(value) => {
                        resolved.insert(key.clone(), value);
                    }
                    None => complete = false,
                }
            }
            Ok(complete.then_some(Value::Object(resolved)))
        }
        other => Ok(Some(other.clone())),
    }
}

fn ensure_instance_matches(
    definition: &EntityDefinition,
    instance: &EntityInstance,
) -> Result<(), CoreError> {
    if definition.entity != instance.entity || definition.version != instance.version {
        return Err(CoreError::EntityMismatch {
            expected_entity: definition.entity.clone(),
            expected_version: definition.version,
            actual_entity: instance.entity.clone(),
            actual_version: instance.version,
        });
    }

    if !definition
        .lifecycle
        .states
        .iter()
        .any(|state| state == &instance.lifecycle_state)
    {
        return Err(CoreError::UnknownState {
            entity: definition.entity.clone(),
            state: instance.lifecycle_state.clone(),
        });
    }

    Ok(())
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
    old_fields: &'a Map<String, Value>,
    new_fields: &'a Map<String, Value>,
    from_state: Option<&'a str>,
    to_state: &'a str,
}

fn resolve_template(value: &Value, context: &TemplateContext<'_>) -> Result<Value, CoreError> {
    match value {
        Value::String(literal) if literal.starts_with("$$") => {
            Ok(Value::String(literal[1..].to_owned()))
        }
        Value::String(expression) if expression.starts_with('$') => {
            match resolve_expression_optional(expression, context)? {
                Some(value) => Ok(value),
                None => Err(template_error(
                    expression,
                    "referenced value does not exist",
                )),
            }
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

fn resolve_expression_optional(
    expression: &str,
    context: &TemplateContext<'_>,
) -> Result<Option<Value>, CoreError> {
    match expression {
        "$id" => Ok(Some(Value::String(context.id.to_owned()))),
        "$entity" => Ok(Some(Value::String(context.definition.entity.clone()))),
        "$version" => Ok(Some(Value::from(context.definition.version))),
        "$from_state" => Ok(context
            .from_state
            .map(|state| Value::String(state.to_owned()))),
        "$to_state" | "$state" => Ok(Some(Value::String(context.to_state.to_owned()))),
        "$args" => Ok(Some(Value::Object(context.args.clone()))),
        "$fields" => Ok(Some(Value::Object(context.new_fields.clone()))),
        "$old_fields" => Ok(Some(Value::Object(context.old_fields.clone()))),
        _ => {
            for (prefix, map) in [
                ("$args.", context.args),
                ("$fields.", context.new_fields),
                ("$old_fields.", context.old_fields),
            ] {
                if let Some(path) = expression.strip_prefix(prefix) {
                    // Walk by reference and clone the leaf: a `$fields.x` reference used to copy
                    // every field of the instance to read one of them.
                    return Ok(lookup(map, path).cloned());
                }
            }

            Err(template_error(expression, "unknown template expression"))
        }
    }
}

fn lookup<'a>(root: &'a Map<String, Value>, path: &str) -> Option<&'a Value> {
    let mut segments = path.split('.');
    let first = segments.next()?;
    if first.is_empty() {
        return None;
    }
    let mut value = root.get(first)?;
    for segment in segments {
        value = value.as_object()?.get(segment)?;
    }
    Some(value)
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
