//! The definition model: what an entity type *is*, as data.
//!
//! Everything in this module deserialises from YAML or JSON. None of it is executable: the
//! condition language is an AST, the templates are values with `$` references, and there is no
//! place to put code. That is what keeps a definition portable, inspectable and safe to load
//! from a file somebody else wrote.
//!
//! Every struct here refuses unknown keys, and a condition refuses anything but exactly one
//! known operator. A definition that is *nearly* right — `requried: true`, two operators in one
//! `assert`, a `precondition:` that should have been `preconditions:` — is a definition that
//! would silently enforce less than it says, so it is refused where it is read.

use serde::{Deserialize, Serialize};
use serde_json::{Number, Value};
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
#[serde(deny_unknown_fields)]
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
    /// arguments, the previous state or the transition, so it cannot depend on how the state was
    /// reached.
    #[serde(default)]
    pub invariants: Vec<RuleDefinition>,

    /// What happens on creation.
    #[serde(default)]
    pub create: CreateDefinition,

    /// The operations an instance accepts, by name.
    #[serde(default)]
    pub operations: BTreeMap<String, OperationDefinition>,

    /// Read models this type declares, by name.
    ///
    /// Declared here and executed by the shell, which is the same split as everything else: the
    /// kernel holds the statement as data and performs none of it. A projection touches no
    /// instance the kernel was handed, so it could not be evaluated here even in principle.
    #[serde(default)]
    pub projections: BTreeMap<String, ProjectionDefinition>,
}

/// A read model: instances grouped by something they hold.
///
/// Deliberately one shape — group by a key, optionally over a subset. `by_status` is
/// `key: $state`; `open_per_customer` is `key: $fields.customer` with `in_state: open`. That is
/// what a read model is for, and it is the shape a store can build an index for.
///
/// The condition language grows operator by operator and never into a language, so this does not
/// gain filters, joins or aggregates because they would be convenient. A projection that needs
/// arithmetic is a consumer's job, over what this hands it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectionDefinition {
    /// What to group by: `$state`, `$id`, `$entity`, or `$fields.<name>`.
    pub key: String,

    /// Only instances in this lifecycle state. Every instance when absent.
    #[serde(default)]
    pub in_state: Option<String>,
}

impl EntityDefinition {
    /// Checks that this definition is internally consistent and could be executed.
    ///
    /// [`Registry::register`](crate::Registry::register) calls this; it is public so a tool can
    /// check a definition without building a registry.
    ///
    /// # Errors
    ///
    /// The first [`DefinitionError`](crate::DefinitionError) found.
    pub fn validate(&self) -> Result<(), crate::DefinitionErrors> {
        crate::validation::validate_definition(self)
    }
}

/// A set of named, typed fields — the shape of an instance or of an operation's arguments.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(deny_unknown_fields)]
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
///
/// A constraint that does not apply to the field's kind — `values` on a `string`, `items` on an
/// `object` — is refused when the definition is registered rather than silently ignored.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(deny_unknown_fields)]
pub struct FieldDefinition {
    /// The kind of value.
    #[serde(rename = "type")]
    pub kind: FieldKind,

    /// Whether a value must be present after defaults are applied.
    #[serde(default)]
    pub required: bool,

    /// The value used when none is supplied. Validated against this field at registration, and
    /// applied at every depth — a default on a nested `properties` entry is filled in too.
    #[serde(
        default,
        deserialize_with = "deserialize_declared_default",
        serialize_with = "serialize_declared_default",
        skip_serializing_if = "DeclaredDefault::is_absent"
    )]
    pub default: DeclaredDefault,

    /// Minimum length in characters. `string` only.
    #[serde(default)]
    pub min_length: Option<usize>,

    /// Maximum length in characters. `string` only.
    #[serde(default)]
    pub max_length: Option<usize>,

    /// Minimum value. `integer` and `number` only.
    #[serde(default)]
    pub min: Option<Number>,

    /// Maximum value. `integer` and `number` only.
    #[serde(default)]
    pub max: Option<Number>,

    /// The permitted values. `enum` only, and required there.
    #[serde(default)]
    pub values: Vec<String>,

    /// The element definition. `array` only, and required there.
    #[serde(default)]
    pub items: Option<Box<FieldDefinition>>,

    /// The nested properties. `object` only.
    #[serde(default)]
    pub properties: BTreeMap<String, FieldDefinition>,

    /// Whether an object may carry properties not declared in `properties`. `object` only.
    #[serde(default)]
    pub additional_properties: bool,

    /// The entity type this field points at. `ref` only, and required there.
    ///
    /// Naming the target is what makes a pointer a *typed* pointer: `customer: {type: ref, entity:
    /// customer}` says an order's customer is a customer, and [`Registry::validate_all`] refuses a
    /// registry whose definitions point at a type nobody registered.
    ///
    /// [`Registry::validate_all`]: crate::Registry::validate_all
    #[serde(default)]
    pub entity: Option<String>,

    /// What the other side reads this edge by — `blocks` for a `blocked_by`. `ref` only, optional.
    ///
    /// A **label**, not a second edge. Nothing stores the reverse and the kernel never traverses
    /// it; it exists so tooling and prose can name the direction they are reading, the way
    /// `aep` `RelationKind::inverse_label` already does.
    #[serde(default)]
    pub inverse: Option<String>,

    /// Whether this edge may form a cycle. `ref` only; absent means `false`.
    ///
    /// An `Option<bool>` rather than a `bool` so that **written** and **absent** are different
    /// things. With a plain `bool`, `acyclic: false` on a `string` is indistinguishable from not
    /// writing it at all, so it would be accepted in silence — which is precisely the defect R-26
    /// exists to prevent, arriving through the machinery built to prevent it. Read it through
    /// [`FieldDefinition::is_acyclic`].
    ///
    /// A **declaration**, not an enforcement. The kernel is handed one instance and cannot see a
    /// graph (R-01), so it records what the definition claims and the shell enforces it — which is
    /// exactly the split `aep artifact relate` already runs, rebuilding the graph before it
    /// writes. Declaring it here is what turns a rule written in prose into one a shell can read.
    #[serde(default)]
    pub acyclic: Option<bool>,
}

/// Whether a field omitted `default`, or declared a value including an explicit JSON `null`.
///
/// `Option<Value>` cannot represent that distinction under Serde: both a missing key and
/// `default: null` deserialize as `None`. Definitions are data, so erasing a key the author wrote
/// is not an acceptable interpretation.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum DeclaredDefault {
    /// The definition did not declare a default.
    #[default]
    Absent,
    /// The exact value declared, including [`Value::Null`].
    Value(Value),
}

impl DeclaredDefault {
    /// The declared value, if the key was present.
    #[must_use]
    pub const fn as_value(&self) -> Option<&Value> {
        match self {
            Self::Absent => None,
            Self::Value(value) => Some(value),
        }
    }

    fn is_absent(&self) -> bool {
        matches!(self, Self::Absent)
    }
}

fn deserialize_declared_default<'de, D>(deserializer: D) -> Result<DeclaredDefault, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Value::deserialize(deserializer).map(DeclaredDefault::Value)
}

fn serialize_declared_default<S>(
    default: &DeclaredDefault,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    match default {
        DeclaredDefault::Absent => serializer.serialize_unit(),
        DeclaredDefault::Value(value) => value.serialize(serializer),
    }
}

impl FieldDefinition {
    /// Whether this reference declares that it may not form a cycle. Absent reads as `false`.
    #[must_use]
    pub fn is_acyclic(&self) -> bool {
        self.acyclic.unwrap_or(false)
    }
}

/// The kinds a field may have.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum FieldKind {
    /// A UTF-8 string; `min_length` and `max_length` apply.
    #[default]
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
    /// An identifier naming an instance of another entity type; `entity` applies and is required.
    ///
    /// The value is a non-empty string and the kernel checks nothing else about it. Whether an
    /// instance of that type actually carries that identity is a question about *another
    /// instance*, which the kernel is never handed — see [`Registry::validate_all`] for the half it
    /// can answer and `docs/design/kernel-v0.1.md` for why the other half is the shell's.
    ///
    /// [`Registry::validate_all`]: crate::Registry::validate_all
    Ref,
}

impl FieldKind {
    /// The spelling used in a document, for messages.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::String => "string",
            Self::Integer => "integer",
            Self::Number => "number",
            Self::Boolean => "boolean",
            Self::Enum => "enum",
            Self::Array => "array",
            Self::Object => "object",
            Self::Json => "json",
            Self::Ref => "ref",
        }
    }
}

impl std::fmt::Display for FieldKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The states an instance may occupy.
///
/// Transitions are not declared here but on the operations that perform them: a state machine
/// whose edges are named operations, each with its own arguments and rules.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LifecycleDefinition {
    /// The state a newly created instance is in. Must be one of `states`.
    pub initial: String,

    /// Every state, each declared once, none empty.
    pub states: Vec<String>,
}

/// What creation does beyond validating the fields and entering the initial state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(deny_unknown_fields)]
pub struct CreateDefinition {
    /// The event emitted on creation, if any. Its templates see `$id`, `$entity`, `$version`,
    /// `$state` and `$fields`; there is no previous state and there are no arguments.
    #[serde(default)]
    pub emit: Option<EventDefinition>,
}

/// One operation: how an instance moves from one state to another, and what that produces.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct OperationDefinition {
    /// The arguments the operation takes. Defaulted, then validated, before anything else.
    #[serde(default)]
    pub arguments: ObjectSchema,

    /// The transitions this operation performs. At most one may start from any given state.
    pub transitions: Vec<TransitionDefinition>,

    /// Rules evaluated against the current state, the selected transition and the validated
    /// arguments, before any mutation. A precondition may read `$args.*`, `$fields.*`,
    /// `$old_fields.*`, `$from_state`, `$to_state`, `$id`, `$entity` and `$version` — but not
    /// `$state`, which in a rule would silently mean the state the operation is heading for.
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
#[serde(deny_unknown_fields)]
pub struct TransitionDefinition {
    /// The state or states the operation may start from.
    pub from: OneOrMany<String>,

    /// The state the instance is in afterwards.
    pub to: String,
}

/// An event an operation emits: a type name and a templated payload.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EventDefinition {
    /// The event type, such as `OrderSubmitted`. Must not be empty.
    #[serde(rename = "type")]
    pub event_type: String,

    /// The payload template. Any JSON value; strings beginning with `$` are references, checked
    /// against the emitting scope when the definition is registered.
    #[serde(default = "empty_object")]
    pub payload: Value,
}

/// A named rule: a condition that must evaluate to `true`, and what to say when it does not.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
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

/// Every operator a condition may use, in the order the documentation lists them.
pub const CONDITION_OPERATORS: &[&str] = &[
    "all", "any", "not", "exists", "eq", "ne", "gt", "gte", "lt", "lte", "in", "contains",
    "before", "after",
];

/// A deliberately small, deterministic predicate language, written as data.
///
/// Operands are ordinary YAML/JSON values and may contain the same `$...` references as event and
/// `set` templates.
///
/// The operators fall into two groups, and which group an operator is in decides whether it can
/// answer [`Unknown`](crate::Truth::Unknown):
///
/// * **Questions about the store** — [`Condition::Exists`]. *Is there a value at this address?*
///   Always answerable, because the kernel holds the instance and can see the key. Two-valued.
/// * **Questions about a value** — every comparison and membership test. *What does it say?*
///   Unanswerable when there is no value to read, so a reference that resolves to nothing makes
///   them [`Unknown`](crate::Truth::Unknown) rather than false: *nobody recorded this* is not
///   *this is wrong*.
///
/// No operator is three-valued by itself; `Unknown` is a property of the question, not of the
/// operator asking it. That is what keeps `not` ordinary — `not: { exists: … }` means exactly
/// what it reads as.
///
/// There is no function call, no loop, no arithmetic, no clock and no lookup. A definition can be
/// validated at registration and evaluated the same way every time because of what this type
/// cannot express.
///
/// A condition is `true`, `false`, or a mapping carrying **exactly one** known operator. Two
/// operators in one mapping, or a misspelled one, is a refusal naming what was found and what is
/// accepted — a silently dropped half-rule is the failure this refusal exists to prevent.
#[derive(Debug, Clone, Serialize, PartialEq)]
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
    /// There is a value at this address. A question about the store, so always answerable:
    /// two-valued, and `not: { exists: … }` is its negation in the ordinary way.
    ///
    /// A key present with nothing after it does **not** exist — `null` is not a value.
    Exists {
        /// The operand, usually a reference such as `$fields.reason`.
        exists: Value,
    },
    /// The first instant is earlier than the second.
    ///
    /// Both operands are read as ISO-8601 — `2026-08-25`, or `2026-08-25T12:00:00[.fff][Z]`. An
    /// operand this kernel cannot read makes the comparison [`Unknown`](crate::Truth::Unknown), not
    /// `false`, because *this is not a timestamp I can read* is a statement about the reader rather
    /// than about the world.
    ///
    /// There is no `$now`, and there will not be (R-62): the clock is read at the edge and handed
    /// in as an argument, which is what keeps a decision replayable a year later.
    Before {
        /// Earlier, then later.
        before: [Value; 2],
    },
    /// The first instant is later than the second. The mirror of [`Condition::Before`], with the
    /// same reading and the same refusals.
    After {
        /// Later, then earlier.
        after: [Value; 2],
    },
    /// The two operands are equal. Numbers compare numerically, so `100` equals `100.0`.
    Eq {
        /// Left and right.
        eq: [Value; 2],
    },
    /// The two operands differ. Numbers compare numerically.
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

impl<'de> Deserialize<'de> for Condition {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        Self::from_value(value).map_err(serde::de::Error::custom)
    }
}

impl Condition {
    /// Reads a condition out of an already-parsed value, naming what is wrong when it is not one.
    ///
    /// # Errors
    ///
    /// A sentence for a person: an unknown operator, more than one operator, a `not` that is not a
    /// condition, a comparison that is not a pair.
    pub fn from_value(value: Value) -> Result<Self, String> {
        let operators = || CONDITION_OPERATORS.join(", ");
        let map = match value {
            Value::Bool(literal) => return Ok(Self::Literal(literal)),
            Value::Object(map) => map,
            other => {
                return Err(format!(
                    "a condition is `true`, `false`, or a mapping with one operator ({}); found {}",
                    operators(),
                    describe(&other)
                ))
            }
        };

        let unknown: Vec<&str> = map
            .keys()
            .map(String::as_str)
            .filter(|key| !CONDITION_OPERATORS.contains(key))
            .collect();
        if !unknown.is_empty() {
            return Err(format!(
                "unknown condition operator '{}'; expected one of {}",
                unknown.join("', '"),
                operators()
            ));
        }
        if map.len() != 1 {
            let mut found: Vec<&str> = map.keys().map(String::as_str).collect();
            found.sort_unstable();
            return Err(format!(
                "a condition carries exactly one operator; found {} ('{}'). Combine them with \
                 `all` or `any` instead — a second key here would otherwise be dropped",
                map.len(),
                found.join("', '")
            ));
        }

        let (operator, operand) = map.into_iter().next().expect("exactly one entry");
        match operator.as_str() {
            "all" => Ok(Self::All {
                all: children(operand, "all")?,
            }),
            "any" => Ok(Self::Any {
                any: children(operand, "any")?,
            }),
            "not" => Ok(Self::Not {
                not: Box::new(Self::from_value(operand)?),
            }),
            "exists" => Ok(Self::Exists { exists: operand }),
            "before" => Ok(Self::Before {
                before: pair(operand, "before")?,
            }),
            "after" => Ok(Self::After {
                after: pair(operand, "after")?,
            }),
            "eq" => Ok(Self::Eq {
                eq: pair(operand, "eq")?,
            }),
            "ne" => Ok(Self::Ne {
                ne: pair(operand, "ne")?,
            }),
            "gt" => Ok(Self::Gt {
                gt: pair(operand, "gt")?,
            }),
            "gte" => Ok(Self::Gte {
                gte: pair(operand, "gte")?,
            }),
            "lt" => Ok(Self::Lt {
                lt: pair(operand, "lt")?,
            }),
            "lte" => Ok(Self::Lte {
                lte: pair(operand, "lte")?,
            }),
            "in" => Ok(Self::In {
                values: pair(operand, "in")?,
            }),
            "contains" => Ok(Self::Contains {
                contains: pair(operand, "contains")?,
            }),
            other => Err(format!(
                "unknown condition operator '{other}'; expected one of {}",
                operators()
            )),
        }
    }
}

fn children(operand: Value, operator: &str) -> Result<Vec<Condition>, String> {
    match operand {
        Value::Array(values) => values.into_iter().map(Condition::from_value).collect(),
        other => Err(format!(
            "'{operator}' takes a list of conditions; found {}",
            describe(&other)
        )),
    }
}

fn pair(operand: Value, operator: &str) -> Result<[Value; 2], String> {
    match operand {
        Value::Array(values) if values.len() == 2 => {
            let mut values = values.into_iter();
            let left = values.next().expect("two values");
            let right = values.next().expect("two values");
            Ok([left, right])
        }
        Value::Array(values) => Err(format!(
            "'{operator}' takes exactly two operands; found {}",
            values.len()
        )),
        other => Err(format!(
            "'{operator}' takes a list of two operands; found {}",
            describe(&other)
        )),
    }
}

fn describe(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "a list",
        Value::Object(_) => "a mapping",
    }
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
    /// The values, as a slice — no allocation, one or many alike.
    pub fn as_slice(&self) -> &[T] {
        match self {
            Self::One(value) => std::slice::from_ref(value),
            Self::Many(values) => values,
        }
    }

    /// The values, in order.
    pub fn iter(&self) -> std::slice::Iter<'_, T> {
        self.as_slice().iter()
    }

    /// Whether there are no values. Only `Many([])` is empty.
    pub fn is_empty(&self) -> bool {
        self.as_slice().is_empty()
    }
}

impl<'a, T> IntoIterator for &'a OneOrMany<T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
