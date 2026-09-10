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

    fn extend(&mut self, defects: impl IntoIterator<Item = DefinitionError>) {
        self.0.extend(defects);
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
    validate_definition_for(definition, ValueProfile::Legacy)
}

/// Schema vocabulary is admitted only through the selected outcome envelope.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ValueProfile {
    Legacy,
    Collections,
    TaggedUnions,
}

impl ValueProfile {
    pub(crate) fn collections(self) -> bool {
        self != Self::Legacy
    }
}

pub(crate) fn validate_definition_for(
    definition: &EntityDefinition,
    profile: ValueProfile,
) -> Result<(), DefinitionErrors> {
    let collections = profile.collections();
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

    defects.extend(validate_schema_definition(
        &definition.schema,
        "schema",
        profile,
    ));

    let invariant_scope = Scope {
        collections,
        bindings: &[],
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

    // A projection naming a field the schema does not have, or a state the lifecycle does not
    // declare, is refused where it is written rather than producing an empty read model at run time
    // — an index that is silently always empty is the hardest kind of wrong to notice.
    for (name, projection) in &definition.projections {
        let path = format!("projections.{name}");
        defects.check(
            validate_reference(
                &projection.key,
                Scope {
                    collections,
                    bindings: &[],
                    kind: ScopeKind::Invariant,
                    fields: &definition.schema,
                    args: None,
                },
            )
            .map_err(|detail| DefinitionError::InvalidTemplate {
                path: path.clone(),
                message: format!("`key`: {detail}"),
            }),
        );

        if let Some(state) = &projection.in_state {
            if !definition.lifecycle.states.contains(state) {
                defects.push(DefinitionError::InvalidRule {
                    path: path.clone(),
                    message: format!(
                        "`in_state` names `{state}`, which the lifecycle does not declare"
                    ),
                });
            }
        }
    }

    if let Some(event) = &definition.create.emit {
        defects.check(validate_event_definition(
            event,
            "create.emit",
            None,
            Scope {
                collections,
                bindings: &[],
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

        defects.extend(validate_schema_definition(
            &operation.arguments,
            &format!("operations.{operation_name}.arguments"),
            profile,
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
            collections,
            bindings: &[],
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
            collections,
            bindings: &[],
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

pub(crate) fn validate_event_definition(
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
pub(crate) enum ScopeKind {
    Invariant,
    Precondition,
    CreateTemplate,
    OperationTemplate,
    Input,
    CommandCreate,
}

impl ScopeKind {
    fn allowed(self) -> &'static str {
        match self {
            Self::Input => "$id, $entity, $version, $args, $args.<path>",
            Self::CommandCreate => "$id, $entity, $version, $args, $args.<path>, $state, $to_state, $fields, $fields.<path>",
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
            Self::Input => "a command input expression",
            Self::CommandCreate => "a command creation event",
            Self::Invariant => "an entity invariant",
            Self::Precondition => "an operation precondition",
            Self::CreateTemplate => "a creation event payload",
            Self::OperationTemplate => "an operation template",
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Scope<'a> {
    pub(crate) collections: bool,
    pub(crate) bindings: &'a [(&'a str, &'a FieldDefinition)],
    pub(crate) kind: ScopeKind,
    pub(crate) fields: &'a ObjectSchema,
    pub(crate) args: Option<&'a ObjectSchema>,
}

impl Scope<'_> {
    /// Whether a bare reference (no path) is available here.
    fn allows(&self, expression: &str) -> bool {
        match expression {
            "$id" | "$entity" | "$version" => true,
            "$fields" => self.kind != ScopeKind::Input,
            "$state" => !matches!(self.kind, ScopeKind::Precondition | ScopeKind::Input),
            "$to_state" => !matches!(self.kind, ScopeKind::Invariant | ScopeKind::Input),
            "$args" => matches!(
                self.kind,
                ScopeKind::Precondition
                    | ScopeKind::OperationTemplate
                    | ScopeKind::Input
                    | ScopeKind::CommandCreate
            ),
            "$from_state" | "$old_fields" => matches!(
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

    if let Some(path) = expression.strip_prefix("$bound.") {
        return bound_field(path, scope).map(|_| ());
    }
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
    schema_field(schema, path, noun).map(|_| ())
}

fn schema_field<'a>(
    schema: &'a ObjectSchema,
    path: &str,
    noun: &str,
) -> Result<Option<&'a FieldDefinition>, String> {
    let mut segments = path.split('.');
    let root = segments.next().unwrap_or_default();
    if root.is_empty() {
        return Err("the path is empty".into());
    }
    let field = match schema.fields.get(root) {
        Some(field) => field,
        None if schema.additional_fields => return Ok(None),
        None => return Err(format!("unknown {noun} '{root}'")),
    };
    field_path(field, segments, root)
}

fn non_nullable(mut field: &FieldDefinition) -> Result<&FieldDefinition, String> {
    while field.kind == FieldKind::Nullable {
        field = field
            .items
            .as_deref()
            .ok_or("nullable must declare items")?;
    }
    Ok(field)
}

fn field_path<'a, 'p>(
    mut field: &'a FieldDefinition,
    segments: impl Iterator<Item = &'p str>,
    root: &str,
) -> Result<Option<&'a FieldDefinition>, String> {
    let mut walked = root.to_owned();
    let mut segments = segments.peekable();
    while let Some(segment) = segments.next() {
        field = non_nullable(field)?;
        match field.kind {
            FieldKind::Json => return Ok(None),
            FieldKind::Object => match field.properties.get(segment) {
                Some(next) => field = next,
                None if field.additional_properties => return Ok(None),
                None => return Err(format!("'{walked}' declares no property '{segment}'")),
            },
            FieldKind::Union => {
                let union = field
                    .union
                    .as_ref()
                    .ok_or("union must declare its envelope")?;
                if segment == union.tag {
                    return if segments.peek().is_none() {
                        // A known scalar address, never an untyped collection binding.
                        Ok(None)
                    } else {
                        Err(format!(
                            "'{walked}.{segment}' is a union tag, not an object"
                        ))
                    };
                }
                if segment != union.content {
                    return Err(format!("'{walked}' declares no union property '{segment}'"));
                }
                walked.push('.');
                walked.push_str(segment);
                let rest: Vec<_> = segments.collect();
                let mut resolved = Vec::new();
                for (name, variant) in &union.variants {
                    resolved.push(
                        field_path(variant, rest.iter().copied(), &walked)
                            .map_err(|error| format!("union variant '{name}': {error}"))?,
                    );
                }
                // A typed binder requires the same schema in every alternative. Ordinary
                // templates may use an address checked against all alternatives independently.
                return Ok(resolved
                    .first()
                    .copied()
                    .flatten()
                    .filter(|first| resolved.iter().all(|candidate| *candidate == Some(*first))));
            }
            kind => {
                return Err(format!(
                    "'{walked}' is a {kind} field, so '{segment}' resolves to nothing"
                ))
            }
        }
        walked.push('.');
        walked.push_str(segment);
    }
    Ok(Some(field))
}

fn bound_field<'a>(path: &str, scope: Scope<'a>) -> Result<Option<&'a FieldDefinition>, String> {
    let mut segments = path.split('.');
    let name = segments.next().unwrap_or_default();
    let field = scope
        .bindings
        .iter()
        .rev()
        .find(|(bind, _)| *bind == name)
        .map(|(_, field)| *field)
        .ok_or_else(|| format!("unknown lexical binder '{name}'"))?;
    field_path(field, segments, name)
}

fn reference_field<'a>(
    expression: &str,
    scope: Scope<'a>,
) -> Result<Option<&'a FieldDefinition>, String> {
    validate_reference(expression, scope)?;
    if let Some(path) = expression.strip_prefix("$bound.") {
        return bound_field(path, scope);
    }
    for (prefix, schema, noun) in [
        ("$fields.", Some(scope.fields), "field"),
        ("$old_fields.", Some(scope.fields), "field"),
        ("$args.", scope.args, "argument"),
    ] {
        if let (Some(path), Some(schema)) = (expression.strip_prefix(prefix), schema) {
            return schema_field(schema, path, noun);
        }
    }
    Err("quantifier target must reference a declared collection field".into())
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

pub(crate) fn validate_condition_definition(
    condition: &Condition,
    path: &str,
    scope: Scope<'_>,
) -> Result<(), DefinitionError> {
    match condition {
        Condition::Literal(_) => Ok(()),
        Condition::Forall { forall } => validate_quantified(forall, path, scope),
        Condition::AnyElement { any_element } => validate_quantified(any_element, path, scope),
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
        Condition::Before { before } => {
            validate_instant_pair(before, &format!("{path}.before"), scope)
        }
        Condition::After { after } => validate_instant_pair(after, &format!("{path}.after"), scope),
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

fn validate_quantified(
    quantified: &crate::definition::Quantified,
    path: &str,
    scope: Scope<'_>,
) -> Result<(), DefinitionError> {
    if !scope.collections {
        return invalid_rule(path, "element quantifiers require outcome profile 2");
    }
    let mut letters = quantified.bind.bytes();
    if !letters
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == b'_')
        || !letters.all(|c| c.is_ascii_alphanumeric() || c == b'_')
    {
        return invalid_rule(path, "quantifier binder must be an identifier");
    }
    let field = reference_field(&quantified.over, scope)
        .and_then(|field| field.ok_or("quantifier target is not a typed collection".into()))
        .and_then(non_nullable)
        .map_err(|message| DefinitionError::InvalidRule {
            path: path.into(),
            message,
        })?;
    if !matches!(field.kind, FieldKind::Array | FieldKind::Map) {
        return invalid_rule(path, "quantifier target must be an array or map");
    }
    let Some(element) = field.items.as_deref() else {
        return invalid_rule(path, "quantifier collection must declare items");
    };
    let mut bindings = scope.bindings.to_vec();
    bindings.push((&quantified.bind, element));
    validate_condition_definition(
        &quantified.body,
        &format!("{path}.body"),
        Scope {
            bindings: &bindings,
            ..scope
        },
    )
}

fn validate_pair(values: &[Value; 2], path: &str, scope: Scope<'_>) -> Result<(), DefinitionError> {
    validate_operand(&values[0], &format!("{path}[0]"), scope)?;
    validate_operand(&values[1], &format!("{path}[1]"), scope)
}

/// The forms `crate::timestamp` reads, named in every refusal so the author can fix the value
/// without going to read the parser.
const INSTANT_FORMS: &str = "`YYYY-MM-DD` or `YYYY-MM-DDTHH:MM:SS[.fff][Z]`, where a space may \
                             stand in for the `T` and an explicit offset is not read";

/// The two operands of `before`/`after`, each of which has to be readable as an instant.
fn validate_instant_pair(
    values: &[Value; 2],
    path: &str,
    scope: Scope<'_>,
) -> Result<(), DefinitionError> {
    validate_instant_operand(&values[0], &format!("{path}[0]"), scope)?;
    validate_instant_operand(&values[1], &format!("{path}[1]"), scope)
}

/// One operand of `before`/`after`: a `$` reference, or a literal this kernel can actually read.
///
/// Registration is where a defect that could never work is caught, and an unreadable literal is
/// exactly that. `compare_instants` answers [`Unknown`](crate::Truth::Unknown) for a value it
/// cannot read — the deliberate half of the three-valued design, because *this is not a timestamp I
/// can read* is a statement about the reader rather than an observation about the instance. But a
/// literal is not an instance: nothing a caller ever supplies can change it, so the rule is
/// unobservable at **every** evaluation, and for a literal that does not begin with `$` the refusal
/// it produces names nothing at all, because `compare_instants` records an unread operand only when
/// what was *written* starts with one. A gate that can only ever refuse, without saying what to
/// fix, is worse than no gate; so it is refused where it is written.
///
/// The reference walk runs **first**, at every depth, so a `$` reference nested inside a list or an
/// object is still reported with its own path and the reason it cannot resolve; the instant rule
/// then refuses whatever survives the walk. Reversed, the typo in
/// `before: [["$fields.nope"], "2026-08-25"]` would be reported only as *not an instant* one level
/// up — a refusal naming nothing the author could act on, which is the failure this whole rule
/// exists to prevent.
fn validate_instant_operand(
    value: &Value,
    path: &str,
    scope: Scope<'_>,
) -> Result<(), DefinitionError> {
    validate_operand(value, path, scope)?;

    match value {
        // `$$2026-08-25` resolves to the literal `$2026-08-25`, which no reading of it parses.
        // Refused rather than quietly read as the text after the escape: `resolve_operand` strips
        // one `$` at run time, and honouring the escape differently here would need two rules to
        // agree forever about a spelling nobody has a reason to write.
        Value::String(escaped) if escaped.starts_with("$$") => unreadable_instant(
            path,
            format!(
                "{value} escapes to the literal '{}', which is not an instant this kernel reads",
                &escaped[1..]
            ),
        ),
        // The walk above already said whether this reference resolves; what it resolves *to* is
        // the instance's business, so there is nothing further to check here.
        Value::String(expression) if expression.starts_with('$') => Ok(()),
        Value::String(literal) if crate::timestamp::parse(literal).is_some() => Ok(()),
        _ => unreadable_instant(path, format!("{value} is not an instant this kernel reads")),
    }
}

fn unreadable_instant(path: &str, detail: String) -> Result<(), DefinitionError> {
    invalid_rule(
        path,
        format!(
            "{detail}, so the comparison would be unobservable at every evaluation; an operand of \
             'before'/'after' is either a '$' reference or a literal instant written \
             {INSTANT_FORMS}"
        ),
    )
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
pub(crate) fn validate_template(
    value: &Value,
    path: &str,
    scope: Scope<'_>,
) -> Result<(), DefinitionError> {
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

pub(crate) fn validate_schema_definition(
    schema: &ObjectSchema,
    path: &str,
    profile: ValueProfile,
) -> Vec<DefinitionError> {
    let mut defects = Vec::new();
    for (name, field) in &schema.fields {
        validate_field_definition(field, &format!("{path}.{name}"), profile, &mut defects);
    }
    defects
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
    if let Some(union) = &field.union {
        for (name, variant) in &union.variants {
            collect_field_targets(variant, &format!("{path}.union.variants[{name:?}]"), found);
        }
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
    if field.union.is_some() && field.kind != FieldKind::Union {
        return refuse("union", "a union field");
    }
    if field.items.is_some()
        && !matches!(
            field.kind,
            FieldKind::Array | FieldKind::Nullable | FieldKind::Map
        )
    {
        return refuse("items", "an array, nullable or map field");
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

fn validate_field_definition(
    field: &FieldDefinition,
    path: &str,
    profile: ValueProfile,
    defects: &mut Vec<DefinitionError>,
) {
    if !profile.collections() && matches!(field.kind, FieldKind::Nullable | FieldKind::Map) {
        defects.push(DefinitionError::InvalidField {
            path: path.to_owned(),
            message: "nullable and map require outcome profile 2".into(),
        });
    }
    if field.kind == FieldKind::Union && profile != ValueProfile::TaggedUnions {
        defects.push(DefinitionError::InvalidField {
            path: path.to_owned(),
            message: "union requires outcome profile 3".into(),
        });
    }
    if let Err(defect) = validate_constraint_applicability(field, path) {
        defects.push(defect);
    }

    if let (Some(min), Some(max)) = (field.min_length, field.max_length) {
        if min > max {
            defects.push(DefinitionError::InvalidField {
                path: path.to_owned(),
                message: "min_length cannot exceed max_length".into(),
            });
        }
    }
    if let (Some(min), Some(max)) = (&field.min, &field.max) {
        if crate::number::compare(min, max).is_gt() {
            defects.push(DefinitionError::InvalidField {
                path: path.to_owned(),
                message: "min cannot exceed max".into(),
            });
        }
    }

    match field.kind {
        FieldKind::Union if field.union.is_none() => {
            defects.push(DefinitionError::InvalidField {
                path: path.to_owned(),
                message: "union must declare 'union'".into(),
            });
        }
        FieldKind::Enum if field.values.is_empty() => {
            defects.push(DefinitionError::InvalidField {
                path: path.to_owned(),
                message: "enum must declare at least one value".into(),
            });
        }
        FieldKind::Array | FieldKind::Nullable | FieldKind::Map if field.items.is_none() => {
            defects.push(DefinitionError::InvalidField {
                path: path.to_owned(),
                message: format!("{} must declare 'items'", field.kind),
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
            defects.push(DefinitionError::InvalidField {
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
            defects.push(DefinitionError::InvalidField {
                path: path.to_owned(),
                message: "'inverse' is a label and cannot be blank; leave it out instead".into(),
            });
        }
        _ => {}
    }

    if let Some(items) = &field.items {
        validate_field_definition(items, &format!("{path}[]"), profile, defects);
    }
    for (name, property) in &field.properties {
        validate_field_definition(property, &format!("{path}.{name}"), profile, defects);
    }
    if let Some(union) = &field.union {
        if union.tag.trim().is_empty()
            || union.content.trim().is_empty()
            || union.tag == union.content
        {
            defects.push(DefinitionError::InvalidField {
                path: format!("{path}.union"),
                message: "union tag and content must be nonempty distinct property names".into(),
            });
        }
        if union.variants.is_empty() {
            defects.push(DefinitionError::InvalidField {
                path: format!("{path}.union.variants"),
                message: "union must declare at least one variant".into(),
            });
        }
        for (name, variant) in &union.variants {
            let at = format!("{path}.union.variants[{name:?}]");
            if name.trim().is_empty() {
                defects.push(DefinitionError::InvalidField {
                    path: at.clone(),
                    message: "union variant name cannot be blank".into(),
                });
            }
            validate_field_definition(variant, &at, profile, defects);
        }
    }

    if let Some(default) = field.default.as_value() {
        let mut default = default.clone();
        apply_nested_defaults(field, &mut default);
        let mut errors = Vec::new();
        validate_value(field, &default, path, &mut errors);
        for error in errors {
            defects.push(DefinitionError::InvalidField {
                path: error.path,
                message: format!("invalid default: {}", error.message),
            });
        }
    }
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
            if let Some(default) = definition.default.as_value() {
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
        FieldKind::Union => {
            if let (Some(union), Value::Object(envelope)) = (&definition.union, value) {
                let variant = envelope
                    .get(&union.tag)
                    .and_then(Value::as_str)
                    .and_then(|tag| union.variants.get(tag));
                if let (Some(variant), Some(payload)) = (variant, envelope.get_mut(&union.content))
                {
                    apply_nested_defaults(variant, payload);
                }
            }
        }
        FieldKind::Nullable if !value.is_null() => {
            if let Some(inner) = &definition.items {
                apply_nested_defaults(inner, value);
            }
        }
        FieldKind::Map => {
            if let (Some(items), Value::Object(values)) = (&definition.items, value) {
                for element in values.values_mut() {
                    apply_nested_defaults(items, element);
                }
            }
        }
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
        FieldKind::Union => match value.as_object() {
            Some(envelope) => {
                if let Some(union) = &definition.union {
                    for key in envelope.keys() {
                        if key != &union.tag && key != &union.content {
                            errors.push(ValidationError::new(
                                member_path(path, key),
                                "unknown union property",
                            ));
                        }
                    }
                    let tag_path = member_path(path, &union.tag);
                    let variant = match envelope.get(&union.tag) {
                        Some(Value::String(tag)) => match union.variants.get(tag) {
                            Some(variant) => Some(variant),
                            None => {
                                errors.push(ValidationError::new(
                                    tag_path,
                                    format!("unknown union tag '{tag}'"),
                                ));
                                None
                            }
                        },
                        Some(_) => {
                            wrong_type(&tag_path, "union tag string", errors);
                            None
                        }
                        None => {
                            errors.push(ValidationError::new(
                                tag_path,
                                "required union tag is missing",
                            ));
                            None
                        }
                    };
                    let content_path = member_path(path, &union.content);
                    match (variant, envelope.get(&union.content)) {
                        (_, None) => errors.push(ValidationError::new(
                            content_path,
                            "required union payload is missing",
                        )),
                        (Some(variant), Some(payload)) => {
                            validate_value(variant, payload, &content_path, errors)
                        }
                        (None, Some(_)) => {}
                    }
                }
            }
            None => wrong_type(path, "union object", errors),
        },
        FieldKind::Nullable => {
            if !value.is_null() {
                if let Some(inner) = &definition.items {
                    validate_value(inner, value, path, errors);
                }
            }
        }
        FieldKind::Map => match value.as_object() {
            Some(values) => {
                if let Some(items) = &definition.items {
                    for (key, value) in values {
                        validate_value(items, value, &format!("{path}[{key:?}]"), errors);
                    }
                }
            }
            None => wrong_type(path, "map", errors),
        },
        FieldKind::String => match value.as_str() {
            Some(string) => validate_string(definition, string, path, errors),
            None => wrong_type(path, "string", errors),
        },
        // Integers are compared as f64 rather than coerced to i64: `as u64 as i64` wrapped
        // 18446744073709551615 to -1, which passed a `max` bound and made a `min` message name a
        // number nobody sent.
        FieldKind::Integer => {
            if value.is_i64() || value.is_u64() {
                validate_number(definition, value.as_number().expect("number"), path, errors);
            } else {
                wrong_type(path, "integer", errors);
            }
        }
        FieldKind::Number => match value.as_number() {
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
    value: &serde_json::Number,
    path: &str,
    errors: &mut Vec<ValidationError>,
) {
    if let Some(min) = &definition.min {
        if crate::number::compare(value, min).is_lt() {
            errors.push(ValidationError::new(
                path,
                format!("value {value} is below minimum {min}"),
            ));
        }
    }
    if let Some(max) = &definition.max {
        if crate::number::compare(value, max).is_gt() {
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
