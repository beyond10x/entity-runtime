//! Definition validation (at registration) and value validation (at create/execute).
//!
//! Registration is where a defect that could never work is caught: an undeclared state, a
//! constraint on a kind it does not apply to, a rule or template reading something its scope
//! cannot see — at any depth of the schema, not only at the root. What is left for run time is
//! only what run time knows: whether a value satisfies its field, and whether a path into a
//! `json` field or an open schema happens to resolve.

use crate::{
    Condition, DefinitionError, DefinitionErrors, EntityDefinition, EventDefinition,
    FieldDefinition, FieldKind, ObjectSchema, RuleDefinition, ValidationError,
};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};

// --- Definition validation -----------------------------------------------------------------------

/// Collects defects instead of stopping at the first.
///
/// Every check below is run for its own sake; the only reason one is ever skipped is that its
/// prerequisite already failed and running it would report the same fault a second time under a
/// different name. A cascade is worse than a short list: it buries the defect that caused it.
#[derive(Default)]
struct Defects(Vec<DefinitionError>);

impl Defects {
    fn push(&mut self, defect: DefinitionError) {
        self.0.push(defect);
    }

    /// Records the defect a check found, if it found one.
    fn check(&mut self, result: Result<(), DefinitionError>) {
        if let Err(defect) = result {
            self.0.push(defect);
        }
    }

    fn into_result(self) -> Result<(), DefinitionErrors> {
        if self.0.is_empty() {
            Ok(())
        } else {
            Err(DefinitionErrors::new(self.0))
        }
    }
}

pub(crate) fn validate_definition(definition: &EntityDefinition) -> Result<(), DefinitionErrors> {
    let mut defects = Defects::default();

    if definition.entity.trim().is_empty() {
        defects.push(DefinitionError::EmptyEntityName);
    }
    if definition.version == 0 {
        defects.push(DefinitionError::ZeroVersion);
    }

    // The lifecycle is checked first and its soundness gates the transition checks below: when the
    // ladder itself is malformed, every transition would report a second time as a state the
    // lifecycle does not declare, which is one fault wearing as many names as the document has
    // operations.
    let mut states = BTreeSet::new();
    let mut ladder_is_sound = true;
    if definition.lifecycle.states.is_empty() {
        defects.push(DefinitionError::EmptyLifecycle);
        ladder_is_sound = false;
    }
    for state in &definition.lifecycle.states {
        if state.trim().is_empty() {
            defects.push(DefinitionError::EmptyLifecycleState);
            ladder_is_sound = false;
            continue;
        }
        if !states.insert(state.clone()) {
            defects.push(DefinitionError::DuplicateLifecycleState {
                state: state.clone(),
            });
            ladder_is_sound = false;
        }
    }
    if ladder_is_sound && !states.contains(&definition.lifecycle.initial) {
        defects.push(DefinitionError::UnknownInitialState {
            state: definition.lifecycle.initial.clone(),
        });
    }

    defects.check(validate_schema_definition(&definition.schema, "schema"));

    let invariant_scope = Scope {
        kind: ScopeKind::Invariant,
        fields: &definition.schema,
        args: None,
    };
    for (index, invariant) in definition.invariants.iter().enumerate() {
        defects.check(validate_rule_definition(
            invariant,
            &format!("invariants[{index}]"),
            invariant_scope,
        ));
    }

    if let Some(event) = &definition.create.emit {
        defects.check(validate_event_definition(
            event,
            "create.emit",
            None,
            Scope {
                kind: ScopeKind::CreateTemplate,
                fields: &definition.schema,
                args: None,
            },
        ));
    }

    for (operation_name, operation) in &definition.operations {
        if operation_name.trim().is_empty() {
            defects.push(DefinitionError::EmptyOperationName);
        }
        if operation.transitions.is_empty() {
            defects.push(DefinitionError::NoTransitions {
                operation: operation_name.clone(),
            });
        }

        defects.check(validate_schema_definition(
            &operation.arguments,
            &format!("operations.{operation_name}.arguments"),
        ));

        let mut operation_source_states = BTreeSet::new();
        for transition in &operation.transitions {
            if transition.from.is_empty() {
                defects.push(DefinitionError::EmptyFromStates {
                    operation: operation_name.clone(),
                });
            }
            for from in transition.from.iter() {
                if ladder_is_sound && !states.contains(from) {
                    defects.push(DefinitionError::UnknownFromState {
                        operation: operation_name.clone(),
                        state: from.clone(),
                    });
                    // A state the ladder does not declare is not also *ambiguous*; naming it twice
                    // would make one typo look like two problems.
                    continue;
                }
                if !operation_source_states.insert(from.clone()) {
                    defects.push(DefinitionError::AmbiguousTransition {
                        operation: operation_name.clone(),
                        state: from.clone(),
                    });
                }
            }
            if ladder_is_sound && !states.contains(&transition.to) {
                defects.push(DefinitionError::UnknownToState {
                    operation: operation_name.clone(),
                    state: transition.to.clone(),
                });
            }
        }

        let rule_scope = Scope {
            kind: ScopeKind::Precondition,
            fields: &definition.schema,
            args: Some(&operation.arguments),
        };
        for (index, precondition) in operation.preconditions.iter().enumerate() {
            defects.check(validate_rule_definition(
                precondition,
                &format!("operations.{operation_name}.preconditions[{index}]"),
                rule_scope,
            ));
        }

        let template_scope = Scope {
            kind: ScopeKind::OperationTemplate,
            fields: &definition.schema,
            args: Some(&operation.arguments),
        };
        for (field, template) in &operation.set {
            if !definition.schema.additional_fields && !definition.schema.fields.contains_key(field)
            {
                defects.push(DefinitionError::UnknownSetField {
                    operation: operation_name.clone(),
                    field: field.clone(),
                });
                // The template is checked anyway: its own references are a separate fault.
            }
            defects.check(validate_template(
                template,
                &format!("operations.{operation_name}.set.{field}"),
                template_scope,
            ));
        }

        for (index, event) in operation.emits.iter().enumerate() {
            defects.check(validate_event_definition(
                event,
                &format!("operations.{operation_name}.emits[{index}]"),
                Some(operation_name),
                template_scope,
            ));
        }
    }

    defects.into_result()
}

fn validate_event_definition(
    event: &EventDefinition,
    path: &str,
    operation: Option<&String>,
    scope: Scope<'_>,
) -> Result<(), DefinitionError> {
    if event.event_type.trim().is_empty() {
        return Err(DefinitionError::EmptyEventType {
            operation: operation.cloned(),
        });
    }
    validate_template(&event.payload, &format!("{path}.payload"), scope)
}

// --- Reference scopes ----------------------------------------------------------------------------

/// Which references a value may carry, which differs by where the value sits.
///
/// The distinction is the point: an invariant that could read `$args` would be a precondition in
/// disguise, true only for the operation that happened to supply the argument, and a precondition
/// that could read `$state` would be reading the state the operation is heading *for* while
/// looking like it reads the state it starts from.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ScopeKind {
    Invariant,
    Precondition,
    CreateTemplate,
    OperationTemplate,
}

impl ScopeKind {
    fn allowed(self) -> &'static str {
        match self {
            Self::Invariant => "$id, $entity, $version, $state, $fields, $fields.<path>",
            Self::Precondition => {
                "$id, $entity, $version, $from_state, $to_state, $args, $args.<path>, $fields, \
                 $fields.<path>, $old_fields, $old_fields.<path>"
            }
            Self::CreateTemplate => {
                "$id, $entity, $version, $state, $to_state, $fields, $fields.<path>"
            }
            Self::OperationTemplate => {
                "$id, $entity, $version, $state, $from_state, $to_state, $args, $args.<path>, \
                 $fields, $fields.<path>, $old_fields, $old_fields.<path>"
            }
        }
    }

    fn is_rule(self) -> bool {
        matches!(self, Self::Invariant | Self::Precondition)
    }

    fn what(self) -> &'static str {
        match self {
            Self::Invariant => "an entity invariant",
            Self::Precondition => "an operation precondition",
            Self::CreateTemplate => "a creation event payload",
            Self::OperationTemplate => "an operation template",
        }
    }
}

#[derive(Clone, Copy)]
struct Scope<'a> {
    kind: ScopeKind,
    fields: &'a ObjectSchema,
    args: Option<&'a ObjectSchema>,
}

impl Scope<'_> {
    /// Whether a bare reference (no path) is available here.
    fn allows(&self, expression: &str) -> bool {
        match expression {
            "$id" | "$entity" | "$version" | "$fields" => true,
            "$state" => !matches!(self.kind, ScopeKind::Precondition),
            "$to_state" => !matches!(self.kind, ScopeKind::Invariant),
            "$from_state" | "$args" | "$old_fields" => matches!(
                self.kind,
                ScopeKind::Precondition | ScopeKind::OperationTemplate
            ),
            _ => false,
        }
    }
}

/// Checks one `$...` reference against its scope, following the path through the schema.
fn validate_reference(expression: &str, scope: Scope<'_>) -> Result<(), String> {
    let refused = |detail: String| {
        Err(format!(
            "{detail}; {} may read {}",
            scope.kind.what(),
            scope.kind.allowed()
        ))
    };

    if scope.allows(expression) {
        return Ok(());
    }

    for (prefix, schema, noun) in [
        ("$fields.", Some(scope.fields), "field"),
        ("$old_fields.", Some(scope.fields), "field"),
        ("$args.", scope.args, "argument"),
    ] {
        let Some(path) = expression.strip_prefix(prefix) else {
            continue;
        };
        let root = prefix.trim_end_matches('.');
        if !scope.allows(root) {
            return refused(format!("'{expression}' is not available here"));
        }
        let Some(schema) = schema else {
            return refused(format!("'{expression}' is not available here"));
        };
        return validate_reference_path(schema, path, noun)
            .map_err(|detail| format!("'{expression}' cannot resolve: {detail}"));
    }

    if expression.starts_with('$') {
        return refused(format!("'{expression}' is not a reference available here"));
    }
    Ok(())
}

/// Walks `path` through the schema, so `$fields.address.countri` is refused where
/// `$fields.address.country` is accepted.
fn validate_reference_path(schema: &ObjectSchema, path: &str, noun: &str) -> Result<(), String> {
    let mut segments = path.split('.');
    let root = segments.next().unwrap_or_default();
    if root.is_empty() {
        return Err("the path is empty".into());
    }

    let mut field = match schema.fields.get(root) {
        Some(field) => field,
        None if schema.additional_fields => return Ok(()),
        None => return Err(format!("unknown {noun} '{root}'")),
    };

    let mut walked = root.to_owned();
    for segment in segments {
        match field.kind {
            FieldKind::Json => return Ok(()),
            FieldKind::Object => match field.properties.get(segment) {
                Some(next) => field = next,
                None if field.additional_properties => return Ok(()),
                None => {
                    return Err(format!("'{walked}' declares no property '{segment}'"));
                }
            },
            kind => {
                return Err(format!(
                    "'{walked}' is a {kind} field, so '{segment}' resolves to nothing"
                ));
            }
        }
        walked.push('.');
        walked.push_str(segment);
    }
    Ok(())
}

// --- Rules ---------------------------------------------------------------------------------------

fn validate_rule_definition(
    rule: &RuleDefinition,
    path: &str,
    scope: Scope<'_>,
) -> Result<(), DefinitionError> {
    if rule
        .name
        .as_ref()
        .is_some_and(|name| name.trim().is_empty())
    {
        return invalid_rule(path, "rule name cannot be empty");
    }

    if rule
        .message
        .as_ref()
        .is_some_and(|message| message.trim().is_empty())
    {
        return invalid_rule(path, "rule message cannot be empty");
    }

    validate_condition_definition(&rule.condition, &format!("{path}.assert"), scope)
}

fn validate_condition_definition(
    condition: &Condition,
    path: &str,
    scope: Scope<'_>,
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

fn validate_pair(values: &[Value; 2], path: &str, scope: Scope<'_>) -> Result<(), DefinitionError> {
    validate_operand(&values[0], &format!("{path}[0]"), scope)?;
    validate_operand(&values[1], &format!("{path}[1]"), scope)
}

fn validate_operand(value: &Value, path: &str, scope: Scope<'_>) -> Result<(), DefinitionError> {
    walk_references(value, path, scope, &mut |expression, path, scope| {
        validate_reference(expression, scope).map_err(|message| {
            if scope.kind.is_rule() {
                DefinitionError::InvalidRule {
                    path: path.to_owned(),
                    message,
                }
            } else {
                DefinitionError::InvalidTemplate {
                    path: path.to_owned(),
                    message,
                }
            }
        })
    })
}

/// The same walk for a `set` value or an event payload, reported as a template defect.
fn validate_template(value: &Value, path: &str, scope: Scope<'_>) -> Result<(), DefinitionError> {
    validate_operand(value, path, scope)
}

/// Visits every `$` string inside a value, at any depth. `$$literal` is not a reference.
fn walk_references(
    value: &Value,
    path: &str,
    scope: Scope<'_>,
    check: &mut impl FnMut(&str, &str, Scope<'_>) -> Result<(), DefinitionError>,
) -> Result<(), DefinitionError> {
    match value {
        Value::String(text) if text.starts_with("$$") => Ok(()),
        Value::String(expression) if expression.starts_with('$') => check(expression, path, scope),
        Value::Array(values) => {
            for (index, value) in values.iter().enumerate() {
                walk_references(value, &format!("{path}[{index}]"), scope, check)?;
            }
            Ok(())
        }
        Value::Object(values) => {
            for (key, value) in values {
                walk_references(value, &format!("{path}.{key}"), scope, check)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn invalid_rule(path: &str, message: impl Into<String>) -> Result<(), DefinitionError> {
    Err(DefinitionError::InvalidRule {
        path: path.to_owned(),
        message: message.into(),
    })
}

// --- Field definitions ---------------------------------------------------------------------------

fn validate_schema_definition(schema: &ObjectSchema, path: &str) -> Result<(), DefinitionError> {
    for (name, field) in &schema.fields {
        validate_field_definition(field, &format!("{path}.{name}"))?;
    }
    Ok(())
}

/// Which constraints a kind admits. A constraint outside its kind's list is refused rather than
/// ignored, because an author who writes one believes it is enforced.
/// Every `ref` a definition declares, as `(path, target entity)`, at any depth.
///
/// Walks the entity schema and every operation's argument schema, through `items` and `properties`,
/// because a reference nested in a list of objects is still a reference and a check that only
/// looked at top-level fields would be one an author discovers by being wrong.
pub(crate) fn relation_targets(definition: &EntityDefinition) -> Vec<(String, String)> {
    let mut found = Vec::new();
    collect_targets(&definition.schema, "schema", &mut found);
    for (name, operation) in &definition.operations {
        collect_targets(
            &operation.arguments,
            &format!("operations.{name}.arguments"),
            &mut found,
        );
    }
    found
}

fn collect_targets(schema: &ObjectSchema, path: &str, found: &mut Vec<(String, String)>) {
    for (name, field) in &schema.fields {
        collect_field_targets(field, &format!("{path}.{name}"), found);
    }
}

fn collect_field_targets(field: &FieldDefinition, path: &str, found: &mut Vec<(String, String)>) {
    if field.kind == FieldKind::Ref {
        if let Some(target) = &field.entity {
            found.push((path.to_owned(), target.clone()));
        }
    }
    if let Some(items) = &field.items {
        collect_field_targets(items, &format!("{path}[]"), found);
    }
    for (name, property) in &field.properties {
        collect_field_targets(property, &format!("{path}.{name}"), found);
    }
}

/// Which constraints a kind admits. A constraint outside its kind's list is refused rather than
/// ignored, because an author who writes one believes it is enforced.
fn validate_constraint_applicability(
    field: &FieldDefinition,
    path: &str,
) -> Result<(), DefinitionError> {
    let refuse = |constraint: &'static str, applies_to: &'static str| {
        Err(DefinitionError::ConstraintNotApplicable {
            path: path.to_owned(),
            constraint,
            kind: field.kind.as_str(),
            applies_to,
        })
    };

    if (field.min_length.is_some() || field.max_length.is_some()) && field.kind != FieldKind::String
    {
        return refuse("min_length/max_length", "a string field");
    }
    if (field.min.is_some() || field.max.is_some())
        && !matches!(field.kind, FieldKind::Integer | FieldKind::Number)
    {
        return refuse("min/max", "an integer or number field");
    }
    if !field.values.is_empty() && field.kind != FieldKind::Enum {
        return refuse("values", "an enum field");
    }
    if field.items.is_some() && field.kind != FieldKind::Array {
        return refuse("items", "an array field");
    }
    if (!field.properties.is_empty() || field.additional_properties)
        && field.kind != FieldKind::Object
    {
        return refuse("properties/additional_properties", "an object field");
    }
    if (field.entity.is_some() || field.inverse.is_some() || field.acyclic.is_some())
        && field.kind != FieldKind::Ref
    {
        return refuse("entity/inverse/acyclic", "a ref field");
    }
    Ok(())
}

fn validate_field_definition(field: &FieldDefinition, path: &str) -> Result<(), DefinitionError> {
    validate_constraint_applicability(field, path)?;

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
        // A `ref` that does not say what it points at is a string with extra ceremony. Naming the
        // target is the entire content of the kind.
        FieldKind::Ref
            if field
                .entity
                .as_deref()
                .is_none_or(|entity| entity.trim().is_empty()) =>
        {
            return Err(DefinitionError::InvalidField {
                path: path.to_owned(),
                message: "ref must declare 'entity', the type it points at".into(),
            });
        }
        FieldKind::Ref
            if field
                .inverse
                .as_deref()
                .is_some_and(|label| label.trim().is_empty()) =>
        {
            return Err(DefinitionError::InvalidField {
                path: path.to_owned(),
                message: "'inverse' is a label and cannot be blank; leave it out instead".into(),
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

// --- Values --------------------------------------------------------------------------------------

/// Fills in declared defaults, at every depth an object or array element already reaches.
///
/// A default inside an object that was not supplied at all is **not** materialised: filling it
/// would invent an object the caller never sent. A default inside an object that *is* present —
/// `{"address": {}}` — is filled, which is what a `default` on a nested property means.
pub(crate) fn apply_defaults(schema: &ObjectSchema, object: &mut Map<String, Value>) {
    apply_member_defaults(&schema.fields, object);
}

fn apply_member_defaults(
    fields: &BTreeMap<String, FieldDefinition>,
    object: &mut Map<String, Value>,
) {
    for (name, definition) in fields {
        if !object.contains_key(name) {
            if let Some(default) = &definition.default {
                object.insert(name.clone(), default.clone());
            }
        }
        if let Some(value) = object.get_mut(name) {
            apply_nested_defaults(definition, value);
        }
    }
}

fn apply_nested_defaults(definition: &FieldDefinition, value: &mut Value) {
    match definition.kind {
        FieldKind::Object => {
            if let Value::Object(map) = value {
                apply_member_defaults(&definition.properties, map);
            }
        }
        FieldKind::Array => {
            if let (Some(items), Value::Array(values)) = (&definition.items, value) {
                for element in values {
                    apply_nested_defaults(items, element);
                }
            }
        }
        _ => {}
    }
}

pub(crate) fn validate_object(
    schema: &ObjectSchema,
    object: &Map<String, Value>,
    root_path: &str,
) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    validate_members(
        &schema.fields,
        schema.additional_fields,
        object,
        root_path,
        "field",
        &mut errors,
    );
    errors
}

/// The one membership check, used for a top-level schema and for a nested object alike.
fn validate_members(
    fields: &BTreeMap<String, FieldDefinition>,
    additional: bool,
    object: &Map<String, Value>,
    root_path: &str,
    noun: &str,
    errors: &mut Vec<ValidationError>,
) {
    for (name, definition) in fields {
        match object.get(name) {
            Some(value) => validate_value(definition, value, &member_path(root_path, name), errors),
            None if definition.required => {
                errors.push(ValidationError::new(
                    member_path(root_path, name),
                    format!("required {noun} is missing"),
                ));
            }
            None => {}
        }
    }

    if !additional {
        for name in object.keys() {
            if !fields.contains_key(name) {
                errors.push(ValidationError::new(
                    member_path(root_path, name),
                    format!("{noun} is not declared in the schema"),
                ));
            }
        }
    }
}

fn member_path(root: &str, name: &str) -> String {
    format!("{root}.{name}")
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
        // Integers are compared as f64 rather than coerced to i64: `as u64 as i64` wrapped
        // 18446744073709551615 to -1, which passed a `max` bound and made a `min` message name a
        // number nobody sent.
        FieldKind::Integer => {
            if value.is_i64() || value.is_u64() {
                validate_number(definition, value.as_f64().unwrap_or_default(), path, errors);
            } else {
                wrong_type(path, "integer", errors);
            }
        }
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
            Some(object) => validate_members(
                &definition.properties,
                definition.additional_properties,
                object,
                path,
                "property",
                errors,
            ),
            None => wrong_type(path, "object", errors),
        },
        FieldKind::Json => {}
        // The identity is opaque to the kernel, exactly as `EntityInstance::id` is (R-75): a
        // non-empty string and nothing more. Whether an instance of the target type carries it is
        // a question about another instance, which the kernel is never handed.
        FieldKind::Ref => match value.as_str() {
            Some(identity) if identity.trim().is_empty() => errors.push(ValidationError::new(
                path,
                "a reference is not empty or whitespace",
            )),
            Some(_) => {}
            None => wrong_type(path, "ref", errors),
        },
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

fn wrong_type(path: &str, expected: &str, errors: &mut Vec<ValidationError>) {
    errors.push(ValidationError::new(path, format!("expected {expected}")));
}
