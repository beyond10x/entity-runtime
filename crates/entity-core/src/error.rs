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
    /// A rule is inconsistent: an empty name or message, an empty `all` or `any`, or a reference
    /// its scope cannot see — an invariant reading `$args.*`, or any rule reading an undeclared
    /// field or argument.
    InvalidRule {
        /// Where, such as `invariants[0].assert.any[1]`.
        path: String,
        /// What is wrong.
        message: String,
    },
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
            Self::InvalidRule { path, message } => {
                write!(f, "invalid rule at '{path}': {message}")
            }
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

/// The kernel refused to produce a [`Decision`](crate::Decision).
///
/// Every variant is a refusal with an address: which operation, which state, which rule. A
/// refusal changes nothing — the caller's instance is exactly as it was.
#[derive(Debug, Clone, PartialEq)]
pub enum CoreError {
    /// The definition itself is malformed.
    Definition(DefinitionError),
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
    /// An entity invariant would not hold for the resulting state. The state was discarded.
    InvariantViolation {
        /// The rule's name, if it has one.
        rule: Option<String>,
        /// The rule's message, or a default.
        message: String,
    },
    /// A `set` or event template referenced something that does not exist, or used an unknown
    /// expression.
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
            Self::OperationNotFound { .. } => "operation_not_found",
            Self::InvalidTransition { .. } => "invalid_transition",
            Self::PreconditionFailed { .. } => "precondition_failed",
            Self::InvariantViolation { .. } => "invariant_violation",
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
            Self::InvariantViolation { rule, message } => match rule {
                Some(rule) => write!(f, "invariant '{rule}' violated: {message}"),
                None => write!(f, "entity invariant violated: {message}"),
            },
            Self::Template { expression, message } => {
                write!(f, "cannot resolve template '{expression}': {message}")
            }
        }
    }
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
        Self::Definition(value)
    }
}
