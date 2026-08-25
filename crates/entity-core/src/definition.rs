//! The definition model: what an entity type *is*, as data.
//!
//! Everything in this module deserialises from YAML or JSON. None of it is executable: the
//! condition language is an AST, the templates are values with `$` references, and there is no
//! place to put code. That is what keeps a definition portable, inspectable and safe to load
//! from a file somebody else wrote.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

fn default_version() -> u32 {
    1
}

/// One entity type: its schema, lifecycle, rules, creation and operations.
///
/// Identified by `(entity, version)`. Two definitions with the same name and different versions
/// are different types as far as the kernel is concerned; an instance records which one it was
/// created under and is executed against that one only.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EntityDefinition {
    /// The type name, such as `order`. Must not be empty.
    pub entity: String,

    /// The definition version. Defaults to `1`; must be greater than zero.
    #[serde(default = "default_version")]
    pub version: u32,

    /// The shape of an instance's fields.
    pub schema: ObjectSchema,

    /// The states an instance may occupy, and which one it starts in.
    pub lifecycle: LifecycleDefinition,

    /// Rules that must hold for every materialised instance state.
    ///
    /// Evaluated after creation and after every successful operation, against the *next* state.
    /// An invariant may read `$fields.*`, `$state`, `$id`, `$entity` and `$version` — never the
    /// arguments or the previous state, so it cannot depend on how the state was reached.
    #[serde(default)]
    pub invariants: Vec<RuleDefinition>,

    /// What happens on creation.
    #[serde(default)]
    pub create: CreateDefinition,

    /// The operations an instance accepts, by name.
    #[serde(default)]
    pub operations: BTreeMap<String, OperationDefinition>,
}

/// A set of named, typed fields — the shape of an instance or of an operation's arguments.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ObjectSchema {
    /// The declared fields, by name.
    #[serde(default)]
    pub fields: BTreeMap<String, FieldDefinition>,

    /// Whether fields not declared here are accepted. Defaults to `false`: an undeclared field is
    /// a validation error.
    #[serde(default)]
    pub additional_fields: bool,
}

/// One field: its kind and the constraints a value must satisfy.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FieldDefinition {
    /// The kind of value.
    #[serde(rename = "type")]
    pub kind: FieldKind,

    /// Whether a value must be present after defaults are applied.
    #[serde(default)]
    pub required: bool,

    /// The value used when none is supplied. Validated against this field at registration.
    #[serde(default)]
    pub default: Option<Value>,

    /// Minimum length in characters, for strings.
    #[serde(default)]
    pub min_length: Option<usize>,

    /// Maximum length in characters, for strings.
    #[serde(default)]
    pub max_length: Option<usize>,

    /// Minimum value, for integers and numbers.
    #[serde(default)]
    pub min: Option<f64>,

    /// Maximum value, for integers and numbers.
    #[serde(default)]
    pub max: Option<f64>,

    /// The permitted values, for enums. Must not be empty for an enum.
    #[serde(default)]
    pub values: Vec<String>,

    /// The element definition, for arrays. Required for an array.
    #[serde(default)]
    pub items: Option<Box<FieldDefinition>>,

    /// The nested properties, for objects.
    #[serde(default)]
    pub properties: BTreeMap<String, FieldDefinition>,

    /// Whether an object may carry properties not declared in `properties`. Defaults to `false`.
    #[serde(default)]
    pub additional_properties: bool,
}

/// The kinds a field may have.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FieldKind {
    /// A UTF-8 string; `min_length` and `max_length` apply.
    String,
    /// A whole number; `min` and `max` apply.
    Integer,
    /// Any JSON number; `min` and `max` apply.
    Number,
    /// `true` or `false`.
    Boolean,
    /// One of the strings listed in `values`.
    Enum,
    /// A list whose elements each satisfy `items`.
    Array,
    /// A nested object whose members each satisfy `properties`.
    Object,
    /// Any JSON value, unchecked.
    Json,
}

/// The states an instance may occupy.
///
/// Transitions are not declared here but on the operations that perform them: a state machine
/// whose edges are named operations, each with its own arguments and rules.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LifecycleDefinition {
    /// The state a newly created instance is in. Must be one of `states`.
    pub initial: String,

    /// Every state, each declared once, none empty.
    pub states: Vec<String>,
}

/// What creation does beyond validating the fields and entering the initial state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct CreateDefinition {
    /// The event emitted on creation, if any. Its templates see `$id`, `$state` and `$fields`;
    /// there is no `$from_state` and there are no arguments.
    #[serde(default)]
    pub emit: Option<EventDefinition>,
}

/// One operation: how an instance moves from one state to another, and what that produces.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OperationDefinition {
    /// The arguments the operation takes. Defaulted, then validated, before anything else.
    #[serde(default)]
    pub arguments: ObjectSchema,

    /// The transitions this operation performs. At most one may start from any given state.
    pub transitions: Vec<TransitionDefinition>,

    /// Rules evaluated against the current state, the selected transition and the validated
    /// arguments, before any mutation. A precondition may read `$args.*`, `$fields.*`,
    /// `$old_fields.*`, `$from_state`, `$to_state`, `$id`, `$entity` and `$version`.
    #[serde(default)]
    pub preconditions: Vec<RuleDefinition>,

    /// Field assignments applied after the transition is selected and the preconditions hold.
    ///
    /// Values are templates. Every assignment is resolved against the *pre-operation* fields, so
    /// the map has no ordering semantics and the result is the same whatever order the entries are
    /// written in.
    #[serde(default)]
    pub set: BTreeMap<String, Value>,

    /// Domain events emitted after the assignments are applied and the invariants hold. Their
    /// templates see the *post-operation* fields.
    #[serde(default, alias = "emit")]
    pub emits: Vec<EventDefinition>,
}

/// One edge of the lifecycle: from one or more states to one state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TransitionDefinition {
    /// The state or states the operation may start from.
    pub from: OneOrMany<String>,

    /// The state the instance is in afterwards.
    pub to: String,
}

/// An event an operation emits: a type name and a templated payload.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EventDefinition {
    /// The event type, such as `OrderSubmitted`. Must not be empty.
    #[serde(rename = "type")]
    pub event_type: String,

    /// The payload template. Any JSON value; strings beginning with `$` are references.
    #[serde(default = "empty_object")]
    pub payload: Value,
}

/// A named rule: a condition that must evaluate to `true`, and what to say when it does not.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuleDefinition {
    /// The rule's name, reported in the refusal. Optional; must not be empty when present.
    #[serde(default)]
    pub name: Option<String>,

    /// The condition.
    #[serde(rename = "assert")]
    pub condition: Condition,

    /// The message reported in the refusal. Optional; must not be empty when present.
    #[serde(default)]
    pub message: Option<String>,
}

/// A deliberately small, deterministic predicate language, written as data.
///
/// Operands are ordinary YAML/JSON values and may contain the same `$...` references as event and
/// `set` templates. A reference that does not resolve makes a comparison or membership test
/// **false**; [`Condition::Exists`] is the explicit way to ask whether something is there.
///
/// There is no function call, no loop, no arithmetic, no clock and no lookup. A definition can be
/// validated at registration and evaluated the same way every time because of what this type
/// cannot express.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum Condition {
    /// `true` or `false`, literally.
    Literal(bool),
    /// Every child holds. Short-circuits on the first that does not. Must not be empty.
    All {
        /// The children.
        all: Vec<Condition>,
    },
    /// At least one child holds. Short-circuits on the first that does. Must not be empty.
    Any {
        /// The children.
        any: Vec<Condition>,
    },
    /// The child does not hold.
    Not {
        /// The child.
        not: Box<Condition>,
    },
    /// The operand resolves to a value.
    Exists {
        /// The operand, usually a reference such as `$fields.reason`.
        exists: Value,
    },
    /// The two operands are equal.
    Eq {
        /// Left and right.
        eq: [Value; 2],
    },
    /// The two operands differ.
    Ne {
        /// Left and right.
        ne: [Value; 2],
    },
    /// Left is numerically greater than right. False unless both are numbers.
    Gt {
        /// Left and right.
        gt: [Value; 2],
    },
    /// Left is numerically greater than or equal to right. False unless both are numbers.
    Gte {
        /// Left and right.
        gte: [Value; 2],
    },
    /// Left is numerically less than right. False unless both are numbers.
    Lt {
        /// Left and right.
        lt: [Value; 2],
    },
    /// Left is numerically less than or equal to right. False unless both are numbers.
    Lte {
        /// Left and right.
        lte: [Value; 2],
    },
    /// The first operand is an element of the second, which must resolve to an array.
    In {
        /// Needle, then haystack.
        #[serde(rename = "in")]
        values: [Value; 2],
    },
    /// The first operand contains the second: an array contains an element, a string contains a
    /// substring, or an object contains a key.
    Contains {
        /// Container, then needle.
        contains: [Value; 2],
    },
}

fn empty_object() -> Value {
    Value::Object(Default::default())
}

/// A single value or a list of them, so `from: draft` and `from: [draft, submitted]` both parse.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum OneOrMany<T> {
    /// One value.
    One(T),
    /// Several values, possibly none.
    Many(Vec<T>),
}

impl<T> OneOrMany<T> {
    /// The values, in order.
    pub fn iter(&self) -> Box<dyn Iterator<Item = &T> + '_> {
        match self {
            Self::One(value) => Box::new(std::iter::once(value)),
            Self::Many(values) => Box::new(values.iter()),
        }
    }

    /// Whether there are no values. Only `Many([])` is empty.
    pub fn is_empty(&self) -> bool {
        matches!(self, Self::Many(values) if values.is_empty())
    }
}
