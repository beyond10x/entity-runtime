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

    /// Which document rules this definition is read under. Absent means [`Semantics::Kernel1`].
    ///
    /// Not the definition's *identity*: that is `(entity, version)`, and two definitions differing
    /// only in evaluation rules are still one entity type to every stored instance. Bumping
    /// `version` to signal a document shape would claim a new type nobody created an instance
    /// under, so the opt-in is its own closed key.
    #[serde(default, skip_serializing_if = "Semantics::is_kernel_1")]
    pub semantics: Semantics,

    /// Which schema field carries the instance's logical identity. `service/1` only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity: Option<IdentityDefinition>,

    /// The relations this definition declares, by name. `service/1` only.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub relations: BTreeMap<String, RelationDefinition>,

    /// Ordered value scales, lowest rank first, that text comparison is answered inside.
    ///
    /// A **declaration** carried in the definition, not a lookup: it is snapshotted into every
    /// [`DecisionRecord`](crate::DecisionRecord) with the rest of the definition and replay reads
    /// the snapshot. `service/1` only.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub scales: BTreeMap<String, Vec<String>>,

    /// Which versioned source-number observation rule answers this definition's numeric
    /// predicates. `service/1` only.
    #[serde(default, skip_serializing_if = "NumberObservation::is_source_number_1")]
    pub number_observation: NumberObservation,
}

/// Which document rules a definition is read under.
///
/// A `kernel/1` definition gets the evaluation it gets today, refusal variant for refusal variant.
/// Every semantic addition of `service/1` — branch selection, the identity address, the source
/// number domain, the new operators, collection addressing — is reached only from
/// [`Semantics::Service1`].
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum Semantics {
    /// The original kernel document rules. Every committed definition is one of these.
    #[default]
    #[serde(rename = "kernel/1")]
    Kernel1,
    /// The service document rules: named outcomes, refusals, responses, identity, relations and
    /// the source number domain.
    #[serde(rename = "service/1")]
    Service1,
    /// Service rules with typed conditional presence in produced values.
    #[serde(rename = "service/2")]
    Service2,
}

impl Semantics {
    /// Whether this is the default, so a `kernel/1` definition serializes without the key.
    #[must_use]
    pub fn is_kernel_1(&self) -> bool {
        matches!(self, Self::Kernel1)
    }

    /// Whether the `service/1` rules apply.
    #[must_use]
    pub fn is_service_1(&self) -> bool {
        matches!(self, Self::Service1)
    }

    /// Whether either version of the service document rules applies.
    #[must_use]
    pub fn has_service_semantics(self) -> bool {
        matches!(self, Self::Service1 | Self::Service2)
    }

    /// Whether typed conditional presence is available.
    #[must_use]
    pub fn has_conditional_presence(self) -> bool {
        matches!(self, Self::Service2)
    }

    /// The spelling used in a document, for messages.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Kernel1 => "kernel/1",
            Self::Service1 => "service/1",
            Self::Service2 => "service/2",
        }
    }
}

impl std::fmt::Display for Semantics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Which versioned source-number observation rule a `service/1` definition is answered under.
///
/// Frozen into the definition, and so into the [`DecisionRecord`](crate::DecisionRecord)'s
/// definition snapshot, because a later source read-door stage must not change what an already
/// recorded decision meant.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum NumberObservation {
    /// The reading of the current service-producing source tool: integer first, then a correctly
    /// rounded finite binary64, followed by the source's canonical representation.
    #[default]
    #[serde(rename = "source-number/1")]
    SourceNumber1,
}

impl NumberObservation {
    /// Whether this is the default, so a `kernel/1` definition serializes without the key.
    #[must_use]
    pub fn is_source_number_1(&self) -> bool {
        matches!(self, Self::SourceNumber1)
    }
}

/// The schema field that carries the instance's logical identity.
///
/// The logical identity is an ordinary declared field in its own kind; the **storage address** is
/// [`EntityInstance::id`](crate::EntityInstance), derived from that field's value by
/// [`identity::address`](crate::identity::address). The two are kept apart deliberately: a guard,
/// a view and a relation carrier all read the logical value, and `$id` stays the storage
/// coordinate and nothing else.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct IdentityDefinition {
    /// The declared, required schema field the address is derived from.
    pub field: String,
}

/// One declared relation: who owns whom, through which field.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RelationDefinition {
    /// Whether this definition owns the target or merely points at it.
    pub kind: RelationKind,
    /// The entity type on the other end.
    pub target: String,
    /// How many of the target there are.
    pub cardinality: Cardinality,
    /// The field carrying the relation — on the **target** for `owns`, on this definition for
    /// `references`.
    pub via: String,
}

/// Which side of a relation this definition is.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RelationKind {
    /// This definition owns the target; the carrier field lives on the target.
    Owns,
    /// This definition points at the target; the carrier field lives here.
    References,
}

impl RelationKind {
    /// The spelling used in a document, for messages.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Owns => "owns",
            Self::References => "references",
        }
    }
}

impl std::fmt::Display for RelationKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// How many instances of the target a relation reaches.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Cardinality {
    /// Exactly one.
    One,
    /// Any number.
    Many,
}

impl Cardinality {
    /// The spelling used in a document, for messages.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::One => "one",
            Self::Many => "many",
        }
    }
}

impl std::fmt::Display for Cardinality {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
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

impl ObjectSchema {
    /// Whether this schema declares nothing and admits nothing — the value a `kernel/1` definition
    /// has for every schema key this contract adds, so those keys serialize away.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.fields.is_empty() && !self.additional_fields
    }
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

    /// How a `map`'s keys are spelled. `map` only, and required there.
    ///
    /// A JSON object's keys are always strings, so a key kind is a statement about their
    /// **spelling**: `boolean` admits exactly `false` and `true`, `integer` admits decimal integer
    /// text, `string` checks nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<MapKey>,

    /// The key carrying a `union`'s variant label. `union` only, and required there.
    ///
    /// The **content** key is derived rather than declared — `value`, or `content` when `tag` is
    /// itself `value` — which is what keeps two readers from disagreeing about it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,

    /// A `union`'s variants by label. `union` only, and required non-empty there.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub variants: BTreeMap<String, FieldDefinition>,
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
    /// An object whose keys are spelled by `key` and whose values each satisfy `items`.
    ///
    /// The keys are **not addressable**: the only `count` a map has is its size, and a body reaching
    /// for a key is refused at registration.
    Map,
    /// An adjacently tagged value: the variant label under `tag`, the payload under the derived
    /// content key.
    Union,
    /// A finite IEEE-754 double, held as the token it was written as so the sign of a zero survives
    /// in the bytes.
    Binary64,
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
            Self::Map => "map",
            Self::Union => "union",
            Self::Binary64 => "binary64",
        }
    }

    /// Whether this kind is one only a `service/1` definition may declare.
    #[must_use]
    pub fn is_service_only(self) -> bool {
        matches!(self, Self::Map | Self::Union | Self::Binary64)
    }

    /// Whether a value of this kind is read as a number under the source observation rule.
    #[must_use]
    pub fn is_numeric(self) -> bool {
        matches!(self, Self::Integer | Self::Number | Self::Binary64)
    }
}

/// How a [`FieldKind::Map`]'s keys are spelled.
///
/// Reproduces the source's own key projection variant for variant. A JSON object's keys are
/// strings, so every row is a check on the **text**.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MapKey {
    /// Any text.
    String,
    /// Exactly `false` or `true`.
    Boolean,
    /// Decimal integer text.
    Integer,
    /// Exact decimal text.
    Decimal,
    /// An instant this runtime reads.
    Timestamp,
    /// An ISO-8601 duration.
    Duration,
    /// A canonical hyphenated UUID.
    Uuid,
    /// Padded base64.
    Bytes,
}

impl MapKey {
    /// The spelling used in a document, for messages.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::String => "string",
            Self::Boolean => "boolean",
            Self::Integer => "integer",
            Self::Decimal => "decimal",
            Self::Timestamp => "timestamp",
            Self::Duration => "duration",
            Self::Uuid => "uuid",
            Self::Bytes => "bytes",
        }
    }
}

impl std::fmt::Display for MapKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
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
    ///
    /// This is the `kernel/1` spelling and is resolved in the creation template scope, unchanged.
    #[serde(default)]
    pub emit: Option<EventDefinition>,

    /// The creation command's input. `service/1` only.
    #[serde(default, skip_serializing_if = "ObjectSchema::is_empty")]
    pub arguments: ObjectSchema,

    /// The creation command's declared response. `service/1` only.
    #[serde(default, skip_serializing_if = "ObjectSchema::is_empty")]
    pub response: ObjectSchema,

    /// The ordered named branches of the creation. `service/1` only.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outcomes: Vec<OutcomeDefinition>,
}

/// One operation: how an instance moves from one state to another, and what that produces.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct OperationDefinition {
    /// The arguments the operation takes. Defaulted, then validated, before anything else.
    #[serde(default)]
    pub arguments: ObjectSchema,

    /// The transitions this operation performs. At most one may start from any given state.
    ///
    /// Defaulted so a `service/1` operation may carry branches instead. The serialization is
    /// unchanged — a `kernel/1` operation always declares transitions — and the existing
    /// `NoTransitions` refusal still fires for every `kernel/1` operation and for a `service/1`
    /// operation that declares neither transitions nor outcomes.
    #[serde(default)]
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

    /// The command's declared response. `service/1` only.
    #[serde(default, skip_serializing_if = "ObjectSchema::is_empty")]
    pub response: ObjectSchema,

    /// The ordered named branches of the operation. `service/1` only.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outcomes: Vec<OutcomeDefinition>,
}

/// One named branch of a creation or an operation.
///
/// `kernel/1` is `service/1` with one implicit branch: a `kernel/1` operation reads as a single
/// unnamed outcome whose applicability is the existing transition selection and whose effect is
/// [`OutcomeEffect::Moves`]. There is one evaluation path, not two; what `kernel/1` keeps is the
/// exact refusal variants it returns today.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(deny_unknown_fields)]
pub struct OutcomeDefinition {
    /// The branch's name, reported in the record and in a refusal. Must not be empty, and must be
    /// unique within one creation or operation.
    pub name: String,

    /// The branch's input guard.
    ///
    /// Answered **before** the held state is tested when the branch declares no `in_state`, and
    /// after it when it does; a guard answering [`Truth::Unknown`](crate::Truth) refuses the
    /// command rather than falling through to a branch its author wrote for a different fact.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<Condition>,

    /// The state this branch is selected in. A matching `in_state` with no `when` is a `True`
    /// selector; a non-matching one skips the branch without evaluating its `when` at all.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub in_state: Option<String>,

    /// Whether this is the branch taken when the subject rests in a state **no** move of this
    /// operation starts from.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub wrong_state: bool,

    /// What the branch does to the instance.
    #[serde(default, skip_serializing_if = "OutcomeEffect::is_none")]
    pub effect: OutcomeEffect,

    /// Field assignments, resolved against the pre-operation fields.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub set: BTreeMap<String, Value>,

    /// Creation fields copied from optional argument leaves when those leaves are present.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub set_if_present: BTreeMap<String, PresentArgument>,

    /// Domain events, in declaration order. Duplicates are preserved.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub emits: Vec<EventDefinition>,

    /// The declared response this branch determines, as templates.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub responds: BTreeMap<String, Value>,

    /// Optional response members copied from optional argument leaves when present.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub responds_if_present: BTreeMap<String, PresentArgument>,

    /// The named error this branch refuses with. A refusing branch produces no record, no
    /// revision, no state, no events and no response.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refuses: Option<RefusalDefinition>,
}

impl OutcomeDefinition {
    /// Whether this branch declares neither `when` nor `in_state` nor `wrong_state` — the
    /// selector-free default, which is declared last among the non-`wrong_state` branches.
    #[must_use]
    pub fn is_default_branch(&self) -> bool {
        self.when.is_none() && self.in_state.is_none() && !self.wrong_state
    }

    /// Whether anything about this branch is observable: an event, a write, a response field, a
    /// refusal or an effect.
    #[must_use]
    pub fn is_observable(&self) -> bool {
        !self.emits.is_empty()
            || !self.set.is_empty()
            || !self.set_if_present.is_empty()
            || !self.responds.is_empty()
            || !self.responds_if_present.is_empty()
            || self.refuses.is_some()
            || !self.effect.is_none()
    }
}

/// What a selected branch does to the instance.
///
/// [`OutcomeEffect::Updates`] produces a decision whose `to_state` equals its `from_state` and
/// which declares no transition: no self-transition is synthesized, at registration or at run
/// time. The distinction is written into the record rather than inferred from
/// `from_state == to_state`, because a definition may legitimately declare a self-transition.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum OutcomeEffect {
    /// Accepts and changes no state.
    #[default]
    None,
    /// Brings the instance into being. Creation branches only.
    Creates,
    /// Moves the instance between declared states.
    Moves {
        /// The state afterwards.
        to: String,
        /// The state or states this move may start from.
        from: OneOrMany<String>,
    },
    /// Writes fields without leaving the state.
    Updates,
}

impl OutcomeEffect {
    /// Whether this branch changes no state, which is the value that serializes away.
    #[must_use]
    pub fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }

    /// The states this effect may start from, or `None` when it is not a move.
    #[must_use]
    pub fn move_sources(&self) -> Option<&OneOrMany<String>> {
        match self {
            Self::Moves { from, .. } => Some(from),
            _ => None,
        }
    }

    /// The spelling used in a document, for messages.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Creates => "creates",
            Self::Moves { .. } => "moves",
            Self::Updates => "updates",
        }
    }
}

/// The named error a refusing branch produces.
///
/// The error's **name** only: the source determines no error field values, so the payload is a
/// binding obligation rather than a kernel invention.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RefusalDefinition {
    /// The declared error's name.
    pub error: String,
    /// What to say to a person. Optional.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
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

    /// Optional object members copied from optional argument leaves when present.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub payload_if_present: BTreeMap<String, PresentArgument>,
}

/// A closed path below `$args` whose optional leaf controls one produced member's presence.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PresentArgument {
    /// Dot-separated path below `$args`; the serialized form never includes `$args.`.
    pub argument: String,
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
///
/// The last four are `service/1` only: a `kernel/1` definition carrying one is refused at
/// registration by `SemanticsKeyNotAvailable`, and a build predating them refuses the document by
/// naming the operator it does not know.
pub const CONDITION_OPERATORS: &[&str] = &[
    "all", "any", "not", "exists", "eq", "ne", "gt", "gte", "lt", "lte", "in", "contains",
    "before", "after", "compare", "truthy", "for_all", "for_any",
];

/// Every operator only a `service/1` definition may use.
pub const SERVICE_CONDITION_OPERATORS: &[&str] = &["compare", "truthy", "for_all", "for_any"];

/// How deep a condition may nest before it is refused with its limit rather than by the reader.
pub const MAX_CONDITION_DEPTH: usize = 32;

/// The closed comparison a [`Condition::Compare`] performs.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CompareOp {
    /// Equal.
    Eq,
    /// Not equal.
    Ne,
    /// Strictly less.
    Lt,
    /// Less or equal.
    Lte,
    /// Strictly greater.
    Gt,
    /// Greater or equal.
    Gte,
}

impl CompareOp {
    /// Whether this operator accepts the ordering two operands produced.
    #[must_use]
    pub fn accepts(self, order: std::cmp::Ordering) -> bool {
        match self {
            Self::Eq => order.is_eq(),
            Self::Ne => !order.is_eq(),
            Self::Lt => order.is_lt(),
            Self::Lte => order.is_lt() || order.is_eq(),
            Self::Gt => order.is_gt(),
            Self::Gte => order.is_gt() || order.is_eq(),
        }
    }

    /// Whether this operator orders its operands, rather than only testing equality.
    #[must_use]
    pub fn is_ordering(self) -> bool {
        matches!(self, Self::Lt | Self::Lte | Self::Gt | Self::Gte)
    }

    /// The spelling used in a document, for messages.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Eq => "eq",
            Self::Ne => "ne",
            Self::Lt => "lt",
            Self::Lte => "lte",
            Self::Gt => "gt",
            Self::Gte => "gte",
        }
    }
}

impl std::fmt::Display for CompareOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The two operands and the operator of a [`Condition::Compare`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Comparison {
    /// The left operand.
    pub left: Value,
    /// Which comparison to make.
    pub op: CompareOp,
    /// The right operand.
    pub right: Value,
}

/// A closed quantifier over an `array`'s elements or a `map`'s values.
///
/// The outer key is renamed — the source spells the existential quantifier `exists:`, and this
/// runtime's `exists` is already the *store* question — and the inner three are not.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Quantifier {
    /// The collection: a reference to an `array` or `map` field.
    #[serde(rename = "in")]
    pub over: Value,
    /// The name the body binds each element to. One non-empty path segment.
    #[serde(rename = "as")]
    pub bind: String,
    /// The body, evaluated once per element with `$<bind>` bound to it.
    #[serde(rename = "that")]
    pub body: Box<Condition>,
}

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
    /// The exact three-valued scalar comparison. `service/1` only.
    ///
    /// Its own operator because two of its rows are **not** what the existing operators answer, and
    /// the difference is `Unknown` against `false`: `gt`/`gte`/`lt`/`lte` answer two-valued `false`
    /// for non-numeric operands, and `eq`/`ne` compare arrays and objects structurally where this
    /// answers `Unknown`.
    Compare {
        /// Left, operator, right.
        compare: Box<Comparison>,
    },
    /// The bare-path reading: a boolean is itself, a number is non-zero, a string is non-empty and
    /// not the four characters `false`, nothing resolved is `Unknown`, anything else is `Unknown`.
    /// `service/1` only.
    Truthy {
        /// The operand, usually a reference.
        truthy: Value,
    },
    /// Every element of the collection satisfies the body. Vacuously `True` over an empty
    /// collection, `Unknown` over an unobserved one. `service/1` only.
    ForAll {
        /// The collection, the binder and the body.
        for_all: Box<Quantifier>,
    },
    /// At least one element of the collection satisfies the body. `False` over an empty
    /// collection, `Unknown` over an unobserved one. `service/1` only.
    ForAny {
        /// The collection, the binder and the body.
        for_any: Box<Quantifier>,
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
            "compare" => Ok(Self::Compare {
                compare: Box::new(structured::<Comparison>(
                    operand,
                    "compare",
                    "a mapping with `left`, `op` and `right`",
                )?),
            }),
            "truthy" => Ok(Self::Truthy { truthy: operand }),
            "for_all" => Ok(Self::ForAll {
                for_all: Box::new(structured::<Quantifier>(
                    operand,
                    "for_all",
                    "a mapping with `in`, `as` and `that`",
                )?),
            }),
            "for_any" => Ok(Self::ForAny {
                for_any: Box::new(structured::<Quantifier>(
                    operand,
                    "for_any",
                    "a mapping with `in`, `as` and `that`",
                )?),
            }),
            other => Err(format!(
                "unknown condition operator '{other}'; expected one of {}",
                operators()
            )),
        }
    }

    /// How deeply this condition nests, counting itself as one.
    #[must_use]
    pub fn depth(&self) -> usize {
        1 + match self {
            Self::All { all } => all.iter().map(Self::depth).max().unwrap_or(0),
            Self::Any { any } => any.iter().map(Self::depth).max().unwrap_or(0),
            Self::Not { not } => not.depth(),
            Self::ForAll { for_all } => for_all.body.depth(),
            Self::ForAny { for_any } => for_any.body.depth(),
            _ => 0,
        }
    }

    /// The operator's spelling, for a refusal that has to name what it found.
    #[must_use]
    pub fn operator(&self) -> &'static str {
        match self {
            Self::Literal(_) => "literal",
            Self::All { .. } => "all",
            Self::Any { .. } => "any",
            Self::Not { .. } => "not",
            Self::Exists { .. } => "exists",
            Self::Before { .. } => "before",
            Self::After { .. } => "after",
            Self::Eq { .. } => "eq",
            Self::Ne { .. } => "ne",
            Self::Gt { .. } => "gt",
            Self::Gte { .. } => "gte",
            Self::Lt { .. } => "lt",
            Self::Lte { .. } => "lte",
            Self::In { .. } => "in",
            Self::Contains { .. } => "contains",
            Self::Compare { .. } => "compare",
            Self::Truthy { .. } => "truthy",
            Self::ForAll { .. } => "for_all",
            Self::ForAny { .. } => "for_any",
        }
    }
}

/// Reads one operator's structured operand, naming the shape it expected rather than handing a
/// reader whatever serde's own message happened to be about a renamed key.
fn structured<T: serde::de::DeserializeOwned>(
    operand: Value,
    operator: &str,
    expected: &str,
) -> Result<T, String> {
    if !operand.is_object() {
        return Err(format!(
            "'{operator}' takes {expected}; found {}",
            describe(&operand)
        ));
    }
    serde_json::from_value(operand)
        .map_err(|error| format!("'{operator}' takes {expected}: {error}"))
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
