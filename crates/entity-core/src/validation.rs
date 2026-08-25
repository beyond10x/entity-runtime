//! Definition validation (at registration) and value validation (at create/execute).

use crate::{
    Condition, DefinitionError, EntityDefinition, FieldDefinition, FieldKind, ObjectSchema,
    RuleDefinition, ValidationError,
};
use serde_json::{Map, Value};
use std::collections::BTreeSet;

pub(crate) fn validate_definition(definition: &EntityDefinition) -> Result<(), DefinitionError> {
    if definition.entity.trim().is_empty() {
        return Err(DefinitionError::EmptyEntityName);
    }
    if definition.version == 0 {
        return Err(DefinitionError::ZeroVersion);
    }
    if definition.lifecycle.states.is_empty() {
        return Err(DefinitionError::EmptyLifecycle);
    }

    let mut states = BTreeSet::new();
    for state in &definition.lifecycle.states {
        if state.trim().is_empty() {
            return Err(DefinitionError::EmptyLifecycleState);
        }
        if !states.insert(state.clone()) {
            return Err(DefinitionError::DuplicateLifecycleState {
                state: state.clone(),
            });
        }
    }

    if !states.contains(&definition.lifecycle.initial) {
        return Err(DefinitionError::UnknownInitialState {
            state: definition.lifecycle.initial.clone(),
        });
    }

    validate_schema_definition(&definition.schema, "schema")?;

    for (index, invariant) in definition.invariants.iter().enumerate() {
        validate_rule_definition(
            invariant,
            &format!("invariants[{index}]"),
            RuleScope::Invariant {
                fields: &definition.schema,
            },
        )?;
    }

    if let Some(event) = &definition.create.emit {
        if event.event_type.trim().is_empty() {
            return Err(DefinitionError::EmptyEventType { operation: None });
        }
    }

    for (operation_name, operation) in &definition.operations {
        if operation_name.trim().is_empty() {
            return Err(DefinitionError::EmptyOperationName);
        }
        if operation.transitions.is_empty() {
            return Err(DefinitionError::NoTransitions {
                operation: operation_name.clone(),
            });
        }

        validate_schema_definition(
            &operation.arguments,
            &format!("operations.{operation_name}.arguments"),
        )?;

        let mut operation_source_states = BTreeSet::new();
        for transition in &operation.transitions {
            if transition.from.is_empty() {
                return Err(DefinitionError::EmptyFromStates {
                    operation: operation_name.clone(),
                });
            }
            for from in transition.from.iter() {
                if !states.contains(from) {
                    return Err(DefinitionError::UnknownFromState {
                        operation: operation_name.clone(),
                        state: from.clone(),
                    });
                }
                if !operation_source_states.insert(from.clone()) {
                    return Err(DefinitionError::AmbiguousTransition {
                        operation: operation_name.clone(),
                        state: from.clone(),
                    });
                }
            }
            if !states.contains(&transition.to) {
                return Err(DefinitionError::UnknownToState {
                    operation: operation_name.clone(),
                    state: transition.to.clone(),
                });
            }
        }

        for (index, precondition) in operation.preconditions.iter().enumerate() {
            validate_rule_definition(
                precondition,
                &format!("operations.{operation_name}.preconditions[{index}]"),
                RuleScope::Operation {
                    fields: &definition.schema,
                    args: &operation.arguments,
                },
            )?;
        }

        if !definition.schema.additional_fields {
            for field in operation.set.keys() {
                if !definition.schema.fields.contains_key(field) {
                    return Err(DefinitionError::UnknownSetField {
                        operation: operation_name.clone(),
                        field: field.clone(),
                    });
                }
            }
        }

        for event in &operation.emits {
            if event.event_type.trim().is_empty() {
                return Err(DefinitionError::EmptyEventType {
                    operation: Some(operation_name.clone()),
                });
            }
        }
    }

    Ok(())
}

#[derive(Clone, Copy)]
enum RuleScope<'a> {
    Invariant {
        fields: &'a ObjectSchema,
    },
    Operation {
        fields: &'a ObjectSchema,
        args: &'a ObjectSchema,
    },
}

fn validate_rule_definition(
    rule: &RuleDefinition,
    path: &str,
    scope: RuleScope<'_>,
) -> Result<(), DefinitionError> {
    if rule
        .name
        .as_ref()
        .is_some_and(|name| name.trim().is_empty())
    {
        return Err(DefinitionError::InvalidRule {
            path: path.to_owned(),
            message: "rule name cannot be empty".into(),
        });
    }

    if rule
        .message
        .as_ref()
        .is_some_and(|message| message.trim().is_empty())
    {
        return Err(DefinitionError::InvalidRule {
            path: path.to_owned(),
            message: "rule message cannot be empty".into(),
        });
    }

    validate_condition_definition(&rule.condition, &format!("{path}.assert"), scope)
}

fn validate_condition_definition(
    condition: &Condition,
    path: &str,
    scope: RuleScope<'_>,
) -> Result<(), DefinitionError> {
    match condition {
        Condition::Literal(_) => Ok(()),
        Condition::All { all } => {
            if all.is_empty() {
                return invalid_rule(path, "'all' must contain at least one condition");
            }
            for (index, child) in all.iter().enumerate() {
                validate_condition_definition(child, &format!("{path}.all[{index}]"), scope)?;
            }
            Ok(())
        }
        Condition::Any { any } => {
            if any.is_empty() {
                return invalid_rule(path, "'any' must contain at least one condition");
            }
            for (index, child) in any.iter().enumerate() {
                validate_condition_definition(child, &format!("{path}.any[{index}]"), scope)?;
            }
            Ok(())
        }
        Condition::Not { not } => validate_condition_definition(not, &format!("{path}.not"), scope),
        Condition::Exists { exists } => validate_operand(exists, &format!("{path}.exists"), scope),
        Condition::Eq { eq } => validate_pair(eq, &format!("{path}.eq"), scope),
        Condition::Ne { ne } => validate_pair(ne, &format!("{path}.ne"), scope),
        Condition::Gt { gt } => validate_pair(gt, &format!("{path}.gt"), scope),
        Condition::Gte { gte } => validate_pair(gte, &format!("{path}.gte"), scope),
        Condition::Lt { lt } => validate_pair(lt, &format!("{path}.lt"), scope),
        Condition::Lte { lte } => validate_pair(lte, &format!("{path}.lte"), scope),
        Condition::In { values } => validate_pair(values, &format!("{path}.in"), scope),
        Condition::Contains { contains } => {
            validate_pair(contains, &format!("{path}.contains"), scope)
        }
    }
}

fn validate_pair(
    values: &[Value; 2],
    path: &str,
    scope: RuleScope<'_>,
) -> Result<(), DefinitionError> {
    validate_operand(&values[0], &format!("{path}[0]"), scope)?;
    validate_operand(&values[1], &format!("{path}[1]"), scope)
}

fn validate_operand(
    value: &Value,
    path: &str,
    scope: RuleScope<'_>,
) -> Result<(), DefinitionError> {
    match value {
        Value::String(text) if text.starts_with("$$") => Ok(()),
        Value::String(expression) if expression.starts_with('$') => {
            validate_rule_reference(expression, path, scope)
        }
        Value::Array(values) => {
            for (index, value) in values.iter().enumerate() {
                validate_operand(value, &format!("{path}[{index}]"), scope)?;
            }
            Ok(())
        }
        Value::Object(values) => {
            for (key, value) in values {
                validate_operand(value, &format!("{path}.{key}"), scope)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn validate_rule_reference(
    expression: &str,
    path: &str,
    scope: RuleScope<'_>,
) -> Result<(), DefinitionError> {
    let common = matches!(
        expression,
        "$id" | "$entity" | "$version" | "$state" | "$to_state" | "$fields"
    );
    if common {
        return Ok(());
    }

    if let Some(field_path) = expression.strip_prefix("$fields.") {
        return validate_known_root_field(field_path, path, scope_fields(scope), "field");
    }

    match scope {
        RuleScope::Invariant { .. } => invalid_rule(
            path,
            &format!(
                "reference '{expression}' is not available in entity invariants; use current-state references such as $fields.* or $state"
            ),
        ),
        RuleScope::Operation { fields, args } => {
            if matches!(expression, "$from_state" | "$args" | "$old_fields") {
                return Ok(());
            }
            if let Some(arg_path) = expression.strip_prefix("$args.") {
                return validate_known_root_field(arg_path, path, args, "argument");
            }
            if let Some(field_path) = expression.strip_prefix("$old_fields.") {
                return validate_known_root_field(field_path, path, fields, "field");
            }
            invalid_rule(path, &format!("unknown rule reference '{expression}'"))
        }
    }
}

fn scope_fields<'a>(scope: RuleScope<'a>) -> &'a ObjectSchema {
    match scope {
        RuleScope::Invariant { fields } | RuleScope::Operation { fields, .. } => fields,
    }
}

fn validate_known_root_field(
    reference_path: &str,
    path: &str,
    schema: &ObjectSchema,
    kind: &str,
) -> Result<(), DefinitionError> {
    let root = reference_path.split('.').next().unwrap_or_default();
    if root.is_empty() {
        return invalid_rule(path, "reference path cannot be empty");
    }
    if !schema.additional_fields && !schema.fields.contains_key(root) {
        return invalid_rule(path, &format!("unknown {kind} '{root}' in reference"));
    }
    Ok(())
}

fn invalid_rule(path: &str, message: &str) -> Result<(), DefinitionError> {
    Err(DefinitionError::InvalidRule {
        path: path.to_owned(),
        message: message.to_owned(),
    })
}

fn validate_schema_definition(schema: &ObjectSchema, path: &str) -> Result<(), DefinitionError> {
    for (name, field) in &schema.fields {
        validate_field_definition(field, &format!("{path}.{name}"))?;
    }
    Ok(())
}

fn validate_field_definition(field: &FieldDefinition, path: &str) -> Result<(), DefinitionError> {
    if let (Some(min), Some(max)) = (field.min_length, field.max_length) {
        if min > max {
            return Err(DefinitionError::InvalidField {
                path: path.to_owned(),
                message: "min_length cannot exceed max_length".into(),
            });
        }
    }
    if let (Some(min), Some(max)) = (field.min, field.max) {
        if min > max {
            return Err(DefinitionError::InvalidField {
                path: path.to_owned(),
                message: "min cannot exceed max".into(),
            });
        }
    }

    match field.kind {
        FieldKind::Enum if field.values.is_empty() => {
            return Err(DefinitionError::InvalidField {
                path: path.to_owned(),
                message: "enum must declare at least one value".into(),
            });
        }
        FieldKind::Array if field.items.is_none() => {
            return Err(DefinitionError::InvalidField {
                path: path.to_owned(),
                message: "array must declare 'items'".into(),
            });
        }
        _ => {}
    }

    if let Some(items) = &field.items {
        validate_field_definition(items, &format!("{path}[]"))?;
    }
    for (name, property) in &field.properties {
        validate_field_definition(property, &format!("{path}.{name}"))?;
    }

    if let Some(default) = &field.default {
        let mut errors = Vec::new();
        validate_value(field, default, path, &mut errors);
        if let Some(error) = errors.into_iter().next() {
            return Err(DefinitionError::InvalidField {
                path: error.path,
                message: format!("invalid default: {}", error.message),
            });
        }
    }

    Ok(())
}

pub(crate) fn apply_defaults(schema: &ObjectSchema, object: &mut Map<String, Value>) {
    for (name, definition) in &schema.fields {
        if !object.contains_key(name) {
            if let Some(default) = &definition.default {
                object.insert(name.clone(), default.clone());
            }
        }
    }
}

pub(crate) fn validate_object(
    schema: &ObjectSchema,
    object: &Map<String, Value>,
    root_path: &str,
) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    for (name, definition) in &schema.fields {
        let path = format!("{root_path}.{name}");
        match object.get(name) {
            Some(value) => validate_value(definition, value, &path, &mut errors),
            None if definition.required => {
                errors.push(ValidationError::new(path, "required field is missing"));
            }
            None => {}
        }
    }

    if !schema.additional_fields {
        for name in object.keys() {
            if !schema.fields.contains_key(name) {
                errors.push(ValidationError::new(
                    format!("{root_path}.{name}"),
                    "field is not declared in the schema",
                ));
            }
        }
    }

    errors
}

fn validate_value(
    definition: &FieldDefinition,
    value: &Value,
    path: &str,
    errors: &mut Vec<ValidationError>,
) {
    match definition.kind {
        FieldKind::String => match value.as_str() {
            Some(string) => validate_string(definition, string, path, errors),
            None => wrong_type(path, "string", errors),
        },
        FieldKind::Integer => match value.as_i64().or_else(|| value.as_u64().map(|v| v as i64)) {
            Some(integer) => validate_number(definition, integer as f64, path, errors),
            None => wrong_type(path, "integer", errors),
        },
        FieldKind::Number => match value.as_f64() {
            Some(number) => validate_number(definition, number, path, errors),
            None => wrong_type(path, "number", errors),
        },
        FieldKind::Boolean => {
            if !value.is_boolean() {
                wrong_type(path, "boolean", errors);
            }
        }
        FieldKind::Enum => match value.as_str() {
            Some(string)
                if definition
                    .values
                    .iter()
                    .any(|candidate| candidate == string) => {}
            Some(string) => errors.push(ValidationError::new(
                path,
                format!(
                    "'{string}' is not one of [{}]",
                    definition.values.join(", ")
                ),
            )),
            None => wrong_type(path, "enum string", errors),
        },
        FieldKind::Array => match value.as_array() {
            Some(values) => {
                if let Some(items) = &definition.items {
                    for (index, value) in values.iter().enumerate() {
                        validate_value(items, value, &format!("{path}[{index}]"), errors);
                    }
                }
            }
            None => wrong_type(path, "array", errors),
        },
        FieldKind::Object => match value.as_object() {
            Some(object) => validate_inline_object(definition, object, path, errors),
            None => wrong_type(path, "object", errors),
        },
        FieldKind::Json => {}
    }
}

fn validate_string(
    definition: &FieldDefinition,
    value: &str,
    path: &str,
    errors: &mut Vec<ValidationError>,
) {
    let length = value.chars().count();
    if let Some(min) = definition.min_length {
        if length < min {
            errors.push(ValidationError::new(
                path,
                format!("length {length} is below minimum {min}"),
            ));
        }
    }
    if let Some(max) = definition.max_length {
        if length > max {
            errors.push(ValidationError::new(
                path,
                format!("length {length} exceeds maximum {max}"),
            ));
        }
    }
}

fn validate_number(
    definition: &FieldDefinition,
    value: f64,
    path: &str,
    errors: &mut Vec<ValidationError>,
) {
    if let Some(min) = definition.min {
        if value < min {
            errors.push(ValidationError::new(
                path,
                format!("value {value} is below minimum {min}"),
            ));
        }
    }
    if let Some(max) = definition.max {
        if value > max {
            errors.push(ValidationError::new(
                path,
                format!("value {value} exceeds maximum {max}"),
            ));
        }
    }
}

fn validate_inline_object(
    definition: &FieldDefinition,
    object: &Map<String, Value>,
    path: &str,
    errors: &mut Vec<ValidationError>,
) {
    for (name, property) in &definition.properties {
        let child_path = format!("{path}.{name}");
        match object.get(name) {
            Some(value) => validate_value(property, value, &child_path, errors),
            None if property.required => {
                errors.push(ValidationError::new(
                    child_path,
                    "required field is missing",
                ));
            }
            None => {}
        }
    }

    if !definition.additional_properties {
        for name in object.keys() {
            if !definition.properties.contains_key(name) {
                errors.push(ValidationError::new(
                    format!("{path}.{name}"),
                    "property is not declared in the schema",
                ));
            }
        }
    }
}

fn wrong_type(path: &str, expected: &str, errors: &mut Vec<ValidationError>) {
    errors.push(ValidationError::new(path, format!("expected {expected}")));
}
