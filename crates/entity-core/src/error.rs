//! Every refusal the kernel can produce, typed.
//!
//! Callers match on variants, never on message text. Messages exist for people; variants exist
//! for programs, and the two are kept apart so a reworded message cannot break a caller.

use std::fmt;

/// A definition is malformed and cannot be registered.
///
/// Found at registration, before the definition can be used for anything. Nothing is stored
/// when one of these is returned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DefinitionError {
    /// `entity` is empty or whitespace.
    EmptyEntityName,
    /// `version` is `0`.
    ZeroVersion,
    /// `lifecycle.states` is empty.
    EmptyLifecycle,
    /// A lifecycle state name is empty or whitespace.
    EmptyLifecycleState,
    /// `lifecycle.initial` is not one of `lifecycle.states`.
    UnknownInitialState {
        /// The undeclared state.
        state: String,
    },
    /// A state appears more than once in `lifecycle.states`.
    DuplicateLifecycleState {
        /// The repeated state.
        state: String,
    },
    /// An operation's name is empty or whitespace.
    EmptyOperationName,
    /// An operation declares no transitions.
    NoTransitions {
        /// The operation.
        operation: String,
    },
    /// A transition's `from` list is empty.
    EmptyFromStates {
        /// The operation.
        operation: String,
    },
    /// A transition starts from a state the lifecycle does not declare.
    UnknownFromState {
        /// The operation.
        operation: String,
        /// The undeclared state.
        state: String,
    },
    /// A transition ends in a state the lifecycle does not declare.
    UnknownToState {
        /// The operation.
        operation: String,
        /// The undeclared state.
        state: String,
    },
    /// Two transitions of one operation start from the same state, so the kernel could not
    /// choose between them.
    AmbiguousTransition {
        /// The operation.
        operation: String,
        /// The state both transitions start from.
        state: String,
    },
    /// A `set` entry writes a field the schema does not declare, and the schema does not allow
    /// additional fields.
    UnknownSetField {
        /// The operation.
        operation: String,
        /// The undeclared field.
        field: String,
    },
    /// An event's `type` is empty or whitespace.
    EmptyEventType {
        /// The operation, or `None` for the creation event.
        operation: Option<String>,
    },
    /// A field definition is inconsistent: `min` above `max`, an enum without values, an array
    /// without `items`, or a default that does not satisfy its own field.
    InvalidField {
        /// Where, such as `schema.total_cents` or `operations.reject.arguments.reason`.
        path: String,
        /// What is wrong.
        message: String,
    },
    /// A constraint is declared on a field whose kind it does not apply to — `values` on a
    /// `string`, `items` on an `object`, `min_length` on an `integer`.
    ///
    /// Refused rather than ignored: an author who writes a constraint believes it is enforced.
    ConstraintNotApplicable {
        /// Where, such as `schema.colour`.
        path: String,
        /// The constraint key, such as `values`.
        constraint: &'static str,
        /// The kind it was declared on.
        kind: &'static str,
        /// The kinds it does apply to.
        applies_to: &'static str,
    },
    /// A rule is inconsistent: an empty name or message, an empty `all` or `any`, or a reference
    /// its scope cannot see — an invariant reading `$args.*`, a precondition reading `$state`, or
    /// any rule reading a field or argument the schema does not declare, at any depth.
    InvalidRule {
        /// Where, such as `invariants[0].assert.any[1]`.
        path: String,
        /// What is wrong.
        message: String,
    },
    /// A `set` value or an event payload references something its scope cannot see, or uses an
    /// expression that is not a reference at all.
    ///
    /// Refused at registration rather than at the first execution: a template that can never
    /// resolve is a defect in the definition, not in the call.
    InvalidTemplate {
        /// Where, such as `operations.reject.set.rejection_reason`.
        path: String,
        /// What is wrong.
        message: String,
    },
    /// A definition with this `(entity, version)` is already registered.
    ///
    /// Replacing one in place would let an instance created under the first be executed under the
    /// second — the situation [`CoreError::EntityMismatch`] exists to refuse, made invisible. Use
    /// [`Registry::replace`](crate::Registry::replace) to mean it.
    DuplicateDefinition {
        /// The entity.
        entity: String,
        /// The version.
        version: u32,
    },
}

impl DefinitionError {
    /// The variant's name, for machine-readable output.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::EmptyEntityName => "empty_entity_name",
            Self::ZeroVersion => "zero_version",
            Self::EmptyLifecycle => "empty_lifecycle",
            Self::EmptyLifecycleState => "empty_lifecycle_state",
            Self::UnknownInitialState { .. } => "unknown_initial_state",
            Self::DuplicateLifecycleState { .. } => "duplicate_lifecycle_state",
            Self::EmptyOperationName => "empty_operation_name",
            Self::NoTransitions { .. } => "no_transitions",
            Self::EmptyFromStates { .. } => "empty_from_states",
            Self::UnknownFromState { .. } => "unknown_from_state",
            Self::UnknownToState { .. } => "unknown_to_state",
            Self::AmbiguousTransition { .. } => "ambiguous_transition",
            Self::UnknownSetField { .. } => "unknown_set_field",
            Self::EmptyEventType { .. } => "empty_event_type",
            Self::InvalidField { .. } => "invalid_field",
            Self::ConstraintNotApplicable { .. } => "constraint_not_applicable",
            Self::InvalidRule { .. } => "invalid_rule",
            Self::InvalidTemplate { .. } => "invalid_template",
            Self::DuplicateDefinition { .. } => "duplicate_definition",
        }
    }
}

impl fmt::Display for DefinitionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyEntityName => write!(f, "entity name cannot be empty"),
            Self::ZeroVersion => write!(f, "entity version must be greater than zero"),
            Self::EmptyLifecycle => write!(f, "lifecycle must contain at least one state"),
            Self::EmptyLifecycleState => write!(f, "lifecycle state names cannot be empty"),
            Self::UnknownInitialState { state } => {
                write!(f, "lifecycle initial state '{state}' is not declared")
            }
            Self::DuplicateLifecycleState { state } => {
                write!(f, "lifecycle state '{state}' is declared more than once")
            }
            Self::EmptyOperationName => write!(f, "operation name cannot be empty"),
            Self::NoTransitions { operation } => {
                write!(
                    f,
                    "operation '{operation}' must declare at least one transition"
                )
            }
            Self::EmptyFromStates { operation } => {
                write!(
                    f,
                    "operation '{operation}' contains an empty 'from' transition"
                )
            }
            Self::UnknownFromState { operation, state } => write!(
                f,
                "operation '{operation}' references unknown source state '{state}'"
            ),
            Self::UnknownToState { operation, state } => write!(
                f,
                "operation '{operation}' references unknown target state '{state}'"
            ),
            Self::AmbiguousTransition { operation, state } => write!(
                f,
                "operation '{operation}' declares more than one transition from state '{state}'"
            ),
            Self::UnknownSetField { operation, field } => {
                write!(f, "operation '{operation}' writes unknown field '{field}'")
            }
            Self::EmptyEventType { operation } => match operation {
                Some(operation) => write!(f, "operation '{operation}' emits an empty event type"),
                None => write!(f, "create emits an empty event type"),
            },
            Self::InvalidField { path, message } => {
                write!(f, "invalid field definition at '{path}': {message}")
            }
            Self::ConstraintNotApplicable {
                path,
                constraint,
                kind,
                applies_to,
            } => write!(
                f,
                "invalid field definition at '{path}': '{constraint}' does not apply to a {kind} \
                 field; it applies to {applies_to}"
            ),
            Self::InvalidRule { path, message } => {
                write!(f, "invalid rule at '{path}': {message}")
            }
            Self::InvalidTemplate { path, message } => {
                write!(f, "invalid template at '{path}': {message}")
            }
            Self::DuplicateDefinition { entity, version } => write!(
                f,
                "entity '{entity}' version {version} is already registered; use `replace` to \
                 change a registered definition"
            ),
        }
    }
}

impl std::error::Error for DefinitionError {}

/// One value did not satisfy its schema.
///
/// Always reported in a list: validation accumulates every failure of an object rather than
/// stopping at the first, so a caller sees all four missing fields at once, not one per attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    /// Where, such as `fields.total_cents` or `arguments.items[2].sku`.
    pub path: String,
    /// What is wrong.
    pub message: String,
}

impl ValidationError {
    /// A validation error at `path`.
    pub fn new(path: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            message: message.into(),
        }
    }
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.path, self.message)
    }
}

impl std::error::Error for ValidationError {}

/// Every defect in one definition, in the order they were found.
///
/// Non-empty by construction: it is only ever produced instead of an `Ok`.
///
/// Validation used to stop at the first defect, so a document with four of them took four
/// attempts to fix and each attempt told you nothing about the next. Value validation already
/// reported every failing field at once (R-23); this is the same courtesy for the definition
/// itself, and `engineering-protocols` invariant 3 asks for it by name.
///
/// Comparing one of these to a single [`DefinitionError`] holds when it carries exactly that
/// defect and nothing else, so a test that asserts one defect is also asserting that there were
/// no others.
#[derive(Debug, Clone, PartialEq)]
pub struct DefinitionErrors(Vec<DefinitionError>);

impl DefinitionErrors {
    /// Builds a list. Panics on an empty one, which would mean a refusal with nothing wrong.
    pub(crate) fn new(defects: Vec<DefinitionError>) -> Self {
        assert!(!defects.is_empty(), "a refusal names at least one defect");
        Self(defects)
    }

    /// The first defect found, which is what a caller that only wants one should read.
    #[must_use]
    pub fn first(&self) -> &DefinitionError {
        &self.0[0]
    }

    /// Every defect, in the order they were found.
    #[must_use]
    pub fn as_slice(&self) -> &[DefinitionError] {
        &self.0
    }

    /// Every defect, owned.
    #[must_use]
    pub fn into_vec(self) -> Vec<DefinitionError> {
        self.0
    }

    /// How many defects there are. Never zero.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Always `false`. Present because clippy asks for it beside [`len`](Self::len), and because
    /// a reader who wonders is better answered by a method than by a comment.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        false
    }

    /// Iterates the defects.
    pub fn iter(&self) -> std::slice::Iter<'_, DefinitionError> {
        self.0.iter()
    }
}

impl<'a> IntoIterator for &'a DefinitionErrors {
    type Item = &'a DefinitionError;
    type IntoIter = std::slice::Iter<'a, DefinitionError>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl IntoIterator for DefinitionErrors {
    type Item = DefinitionError;
    type IntoIter = std::vec::IntoIter<DefinitionError>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl From<DefinitionError> for DefinitionErrors {
    fn from(error: DefinitionError) -> Self {
        Self(vec![error])
    }
}

impl PartialEq<DefinitionError> for DefinitionErrors {
    fn eq(&self, other: &DefinitionError) -> bool {
        self.0.len() == 1 && &self.0[0] == other
    }
}

impl PartialEq<DefinitionErrors> for DefinitionError {
    fn eq(&self, other: &DefinitionErrors) -> bool {
        other == self
    }
}

impl fmt::Display for DefinitionErrors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0.as_slice() {
            [only] => write!(f, "{only}"),
            defects => {
                write!(f, "{} defects", defects.len())?;
                for defect in defects {
                    write!(f, "; {defect}")?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for DefinitionErrors {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.first())
    }
}

/// The kernel refused to produce a [`Decision`](crate::Decision).
///
/// Every variant is a refusal with an address: which operation, which state, which rule. A
/// refusal changes nothing — the caller's instance is exactly as it was.
#[derive(Debug, Clone, PartialEq)]
pub enum CoreError {
    /// The definition itself is malformed. Every defect found, not the first.
    Definition(DefinitionErrors),
    /// One or more values did not satisfy their schema. Every failure is listed.
    Validation(Vec<ValidationError>),
    /// No definition is registered under this `(entity, version)`.
    EntityNotRegistered {
        /// The entity.
        entity: String,
        /// The version.
        version: u32,
    },
    /// The instance was created under a different definition than the one it was executed
    /// against.
    EntityMismatch {
        /// The definition's entity.
        expected_entity: String,
        /// The definition's version.
        expected_version: u32,
        /// The instance's entity.
        actual_entity: String,
        /// The instance's version.
        actual_version: u32,
    },
    /// The instance claims a lifecycle state the definition does not declare.
    ///
    /// The kernel cannot tell whether an instance it is handed is one it produced — that is the
    /// shell's to know — but it can refuse one that could never have existed.
    UnknownState {
        /// The entity.
        entity: String,
        /// The state the instance carries.
        state: String,
    },
    /// The definition declares no such operation.
    OperationNotFound {
        /// The operation.
        operation: String,
    },
    /// The operation declares no transition from the instance's current state.
    InvalidTransition {
        /// The operation.
        operation: String,
        /// The instance's current lifecycle state.
        state: String,
    },
    /// A precondition of the operation evaluated to `false`. Nothing was mutated.
    PreconditionFailed {
        /// The operation.
        operation: String,
        /// The rule's name, if it has one.
        rule: Option<String>,
        /// The rule's message, or a default.
        message: String,
    },
    /// A precondition of the operation could not be evaluated: something it reads has no value.
    ///
    /// The counterpart to [`PreconditionFailed`](Self::PreconditionFailed), and the reason the
    /// two are different variants. *No review has been recorded* and *the review says rejected*
    /// are different facts, and an operator told only that a gate failed will go and fix the
    /// wrong one. `unresolved` carries **every** address nothing was observed at, not the first,
    /// so one refusal can be acted on once.
    PreconditionUnobservable {
        /// The operation.
        operation: String,
        /// The rule's name, if it has one.
        rule: Option<String>,
        /// The rule's message, or a default.
        message: String,
        /// Every reference the rule reads that resolved to nothing, sorted and without repeats.
        unresolved: Vec<String>,
    },
    /// An entity invariant would not hold for the resulting state. The state was discarded.
    InvariantViolation {
        /// The rule's name, if it has one.
        rule: Option<String>,
        /// The rule's message, or a default.
        message: String,
    },
    /// An entity invariant could not be evaluated: something it reads has no value.
    ///
    /// The state was discarded, exactly as for a violation — an invariant nobody can check is not
    /// an invariant that held.
    InvariantUnobservable {
        /// The rule's name, if it has one.
        rule: Option<String>,
        /// The rule's message, or a default.
        message: String,
        /// Every reference the rule reads that resolved to nothing, sorted and without repeats.
        unresolved: Vec<String>,
    },
    /// A `set` or event template referenced something that does not exist at run time.
    ///
    /// Registration refuses a reference the scope cannot see, so this is left for what only the
    /// call knows: a path into a `json` field, or into a schema that admits additional fields.
    Template {
        /// The expression, such as `$args.reason`.
        expression: String,
        /// What is wrong.
        message: String,
    },
}

impl CoreError {
    /// The variant's name, for machine-readable output: `invalid_transition`,
    /// `precondition_failed`, and so on.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Definition(_) => "definition",
            Self::Validation(_) => "validation",
            Self::EntityNotRegistered { .. } => "entity_not_registered",
            Self::EntityMismatch { .. } => "entity_mismatch",
            Self::UnknownState { .. } => "unknown_state",
            Self::OperationNotFound { .. } => "operation_not_found",
            Self::InvalidTransition { .. } => "invalid_transition",
            Self::PreconditionFailed { .. } => "precondition_failed",
            Self::PreconditionUnobservable { .. } => "precondition_unobservable",
            Self::InvariantViolation { .. } => "invariant_violation",
            Self::InvariantUnobservable { .. } => "invariant_unobservable",
            Self::Template { .. } => "template",
        }
    }
}

impl fmt::Display for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Definition(error) => write!(f, "definition error: {error}"),
            Self::Validation(errors) => {
                write!(f, "validation failed")?;
                for error in errors {
                    write!(f, "; {error}")?;
                }
                Ok(())
            }
            Self::EntityNotRegistered { entity, version } => {
                write!(f, "entity '{entity}' version {version} is not registered")
            }
            Self::EntityMismatch {
                expected_entity,
                expected_version,
                actual_entity,
                actual_version,
            } => write!(
                f,
                "instance type mismatch: expected '{expected_entity}' v{expected_version}, got '{actual_entity}' v{actual_version}"
            ),
            Self::UnknownState { entity, state } => write!(
                f,
                "instance claims lifecycle state '{state}', which '{entity}' does not declare"
            ),
            Self::OperationNotFound { operation } => {
                write!(f, "operation '{operation}' is not defined")
            }
            Self::InvalidTransition { operation, state } => write!(
                f,
                "operation '{operation}' is not valid from lifecycle state '{state}'"
            ),
            Self::PreconditionFailed {
                operation,
                rule,
                message,
            } => match rule {
                Some(rule) => write!(
                    f,
                    "precondition '{rule}' failed for operation '{operation}': {message}"
                ),
                None => write!(f, "precondition failed for operation '{operation}': {message}"),
            },
            Self::PreconditionUnobservable {
                operation,
                rule,
                message,
                unresolved,
            } => {
                match rule {
                    Some(rule) => write!(
                        f,
                        "precondition '{rule}' for operation '{operation}' cannot be evaluated: {message}"
                    )?,
                    None => write!(
                        f,
                        "precondition for operation '{operation}' cannot be evaluated: {message}"
                    )?,
                }
                write!(f, "; {}", nothing_observed_at(unresolved))
            }
            Self::InvariantViolation { rule, message } => match rule {
                Some(rule) => write!(f, "invariant '{rule}' violated: {message}"),
                None => write!(f, "entity invariant violated: {message}"),
            },
            Self::InvariantUnobservable {
                rule,
                message,
                unresolved,
            } => {
                match rule {
                    Some(rule) => write!(f, "invariant '{rule}' cannot be evaluated: {message}")?,
                    None => write!(f, "entity invariant cannot be evaluated: {message}")?,
                }
                write!(f, "; {}", nothing_observed_at(unresolved))
            }
            Self::Template { expression, message } => {
                write!(f, "cannot resolve template '{expression}': {message}")
            }
        }
    }
}

/// The tail of an unobservable refusal: what to go and observe.
///
/// A refusal that says *go and observe* without naming what to observe reproduces, in a type,
/// exactly the prose-rule failure this kernel exists to end — so the empty case says so plainly
/// rather than printing an empty list.
fn nothing_observed_at(unresolved: &[String]) -> String {
    if unresolved.is_empty() {
        return "nothing was observed, and the rule does not name a reference".to_owned();
    }
    format!("nothing was observed at {}", unresolved.join(", "))
}

impl std::error::Error for CoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Definition(error) => Some(error),
            _ => None,
        }
    }
}

impl From<DefinitionError> for CoreError {
    fn from(value: DefinitionError) -> Self {
        Self::Definition(value.into())
    }
}

impl From<DefinitionErrors> for CoreError {
    fn from(value: DefinitionErrors) -> Self {
        Self::Definition(value)
    }
}
