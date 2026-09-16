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
    /// A `ref` field points at an entity type the registry does not hold.
    ///
    /// A **set**-level defect, not a definition-level one, which is why it comes from
    /// [`Registry::validate_all`](crate::Registry::validate_all) and never from `register`. Two
    /// types that point at each other — a story naming its epic, an epic naming its stories — are
    /// ordinary, and a check that ran at registration would make them impossible to register in
    /// either order.
    UnknownRelationTarget {
        /// The entity whose definition declares the reference.
        entity: String,
        /// Where the reference is declared, such as `schema.customer`.
        path: String,
        /// The entity type it points at, which nothing registered.
        target: String,
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

    /// A `kernel/1` definition carries a key, field kind or condition operator only `service/1`
    /// admits.
    ///
    /// The opt-in is `semantics: service/1`. Refused rather than ignored, for the reason every
    /// closed key is: an author who writes one believes it is evaluated, and a `kernel/1` document
    /// carrying a branch would be a document whose branches nothing selects.
    SemanticsKeyNotAvailable {
        /// Where, such as `create.outcomes` or `invariants[0].assert.compare`.
        path: String,
        /// The key, kind or operator that is not available.
        key: String,
    },
    /// A conditional source path does not name one optional, no-default argument leaf.
    ConditionalArgumentInvalid {
        /// The conditional map member carrying the path.
        path: String,
        /// The authored argument path.
        argument: String,
        /// What makes it invalid.
        message: String,
    },
    /// A conditional destination is undeclared, required, defaulted or differently typed.
    ConditionalTargetInvalid {
        /// The conditional map carrying the destination.
        path: String,
        /// The destination field.
        field: String,
        /// What makes it invalid.
        message: String,
    },
    /// One destination appears in both an ordinary and a conditional output map.
    ConditionalTargetConflict {
        /// The conditional map carrying the duplicate destination.
        path: String,
        /// The duplicated field.
        field: String,
    },
    /// Conditional state insertion was declared on an operation rather than creation.
    ConditionalSetOnOperation {
        /// The operation.
        operation: String,
        /// The outcome.
        outcome: String,
        /// The field it tried to insert.
        field: String,
    },
    /// A branch's name is empty or whitespace.
    EmptyOutcomeName {
        /// The creation or operation it belongs to.
        command: String,
    },
    /// Two branches of one creation or operation carry the same name.
    DuplicateOutcome {
        /// The creation or operation.
        command: String,
        /// The repeated name.
        outcome: String,
    },
    /// More than one selector-free branch, or one that is not last among the non-`wrong_state`
    /// branches.
    ///
    /// The source's default branch is *taken when no conditional outcome matched*, a
    /// position-independent meaning; this runtime selects by declared order, so the only
    /// declaration order that spells that meaning is last.
    AmbiguousDefaultOutcome {
        /// The creation or operation.
        command: String,
    },
    /// More than one `wrong_state` branch in one operation.
    DuplicateWrongStateOutcome {
        /// The operation.
        operation: String,
    },
    /// A creation branch declares `wrong_state`, which has no meaning where there is no subject.
    WrongStateOnCreate {
        /// The branch.
        outcome: String,
    },
    /// A `wrong_state` branch also declares its own `when` or `in_state`.
    WrongStateWithSelector {
        /// The operation.
        operation: String,
        /// The branch.
        outcome: String,
    },
    /// An operation declares both a `wrong_state` branch and an `in_state` branch.
    ///
    /// The source's own rule: a command using explicit state guards cannot also declare
    /// `WrongState`, because overlapping precedence is not inferred. Keeping the two constructs
    /// apart is what stops the state-admissibility step skipping a state-guarded branch.
    WrongStateWithStateGuard {
        /// The operation.
        operation: String,
    },
    /// An operation declares a `wrong_state` branch while its moves already start from every state.
    WrongStateUnreachable {
        /// The operation.
        operation: String,
    },
    /// An `in_state` branch whose effect is a move names a state that move does not start from.
    GuardStateOutsideMove {
        /// The operation.
        operation: String,
        /// The branch.
        outcome: String,
        /// The guarded state, which the branch's own `from` does not contain.
        state: String,
    },
    /// An operation branch declares the creation effect.
    CreatesEffectOnOperation {
        /// The operation.
        operation: String,
        /// The branch.
        outcome: String,
    },
    /// A creation branch's effect is neither `creates` nor `none`.
    MissingCreatesEffect {
        /// The branch.
        outcome: String,
        /// The effect it declared.
        effect: &'static str,
    },
    /// A refusing branch also writes, emits, responds or changes state.
    ///
    /// A refusal produces nothing durable, so a refusing branch that claimed to is a branch whose
    /// declaration nothing could honour.
    RefusalMutatesState {
        /// The creation or operation.
        command: String,
        /// The branch.
        outcome: String,
    },
    /// A branch declares no event, no write, no response field, no refusal and no effect, so
    /// nothing about taking it could be observed.
    UnobservableOutcome {
        /// The creation or operation.
        command: String,
        /// The branch.
        outcome: String,
    },
    /// A branch's `in_state`, `effect.to` or `effect.from` names a state the lifecycle does not
    /// declare.
    UnknownOutcomeState {
        /// The creation or operation.
        command: String,
        /// The branch.
        outcome: String,
        /// The undeclared state.
        state: String,
    },
    /// A branch responds with a field the command's `response` schema does not declare.
    ResponseFieldUnknown {
        /// The creation or operation.
        command: String,
        /// The branch.
        outcome: String,
        /// The undeclared response field.
        field: String,
    },
    /// An accepting branch leaves a required response field undetermined.
    OutcomeResponseIncomplete {
        /// The creation or operation.
        command: String,
        /// The branch.
        outcome: String,
        /// The required response field nothing determines.
        field: String,
    },
    /// `identity.field` is not a declared **required** field of the schema.
    IdentityFieldUnknown {
        /// The field the identity names.
        field: String,
    },
    /// `identity.field`'s kind has no address function. Only `json` is in this position.
    IdentityFieldNotAddressable {
        /// The field the identity names.
        field: String,
        /// Its declared kind.
        kind: &'static str,
    },
    /// A `references` relation's `via` is not a declared field of this definition.
    RelationViaUnknown {
        /// The relation.
        relation: String,
        /// The field it names.
        via: String,
    },
    /// A relation carrier's kind is not the one its row admits.
    RelationViaWrongShape {
        /// The relation.
        relation: String,
        /// The carrier field.
        via: String,
        /// What the row admits.
        expected: String,
        /// What was declared.
        found: String,
    },
    /// A relation carrier's optionality is not the one its row admits.
    ///
    /// The source offers an optional carrier for the `references`/`one` row and for no other, so
    /// `owns` and `references`/`many` keep `required: true` rather than all three being widened
    /// together.
    RelationCarrierOptionality {
        /// The relation.
        relation: String,
        /// The carrier field.
        via: String,
    },
    /// A relation names a target the registry does not hold.
    ///
    /// A **set**-level defect, like [`DefinitionError::UnknownRelationTarget`], and reported from
    /// [`Registry::validate_all`](crate::Registry::validate_all) for the same reason.
    RelationTargetMissing {
        /// The declaring entity.
        entity: String,
        /// The relation.
        relation: String,
        /// The target nobody registered.
        target: String,
    },
    /// Two definitions declare themselves the owner of one entity type.
    RelationSecondOwner {
        /// The owned entity.
        target: String,
        /// The first owner, in name order.
        owner: String,
        /// The second.
        other: String,
    },
    /// An `owns` relation's `via` is not a correctly shaped required field of the **target**.
    RelationCarrierWrong {
        /// The declaring entity.
        entity: String,
        /// The relation.
        relation: String,
        /// The carrier field, which lives on the target.
        via: String,
        /// What is wrong with it.
        detail: String,
    },
    /// One field carries two relations.
    RelationFieldClaimedTwice {
        /// The entity whose field it is.
        entity: String,
        /// The field.
        field: String,
        /// The first relation to claim it, in name order.
        relation: String,
        /// The second.
        other: String,
    },
    /// A `map` field declares no key spelling, or one that is not a text primitive.
    MapKeyNotText {
        /// Where, such as `schema.metadata`.
        path: String,
    },
    /// A `map` field declares no value definition.
    MapValueMissing {
        /// Where.
        path: String,
    },
    /// A `union`'s tag key equals the content key a nested variant object would use.
    UnionTagCollides {
        /// Where.
        path: String,
        /// The tag that collides.
        tag: String,
    },
    /// A `union` declares no variants, or no tag at all.
    UnionVariantMissing {
        /// Where.
        path: String,
    },
    /// A quantifier's `as` is not one non-empty path segment.
    QuantifierBindInvalid {
        /// Where.
        path: String,
        /// The binder that was written.
        bind: String,
    },
    /// A quantifier's `in` does not name an `array` or `map` field.
    QuantifierOverNotCollection {
        /// Where.
        path: String,
        /// What `in` named.
        over: String,
        /// Why it is not a collection.
        detail: String,
    },
    /// A quantifier body reads an address neither the enclosing scope nor the binder admits.
    QuantifierBodyScope {
        /// Where.
        path: String,
        /// The address it reached for.
        expression: String,
        /// What is wrong with it.
        detail: String,
    },
    /// A condition nests deeper than the limit, refused with its limit rather than by the reader.
    ConditionTooDeep {
        /// Where.
        path: String,
        /// How deep it nests.
        depth: usize,
        /// How deep it may nest.
        limit: usize,
    },
    /// A `compare` operand is a literal list or mapping, which the source compares nothing about.
    CompareOperandNotAddressable {
        /// Where.
        path: String,
        /// Which side.
        side: &'static str,
    },
    /// A declared scale's name is empty or whitespace.
    ScaleUnnamed,
    /// A declared scale lists no values.
    ScaleEmpty {
        /// The scale.
        scale: String,
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
            Self::UnknownRelationTarget { .. } => "unknown_relation_target",
            Self::InvalidTemplate { .. } => "invalid_template",
            Self::DuplicateDefinition { .. } => "duplicate_definition",
            Self::SemanticsKeyNotAvailable { .. } => "semantics_key_not_available",
            Self::ConditionalArgumentInvalid { .. } => "conditional_argument_invalid",
            Self::ConditionalTargetInvalid { .. } => "conditional_target_invalid",
            Self::ConditionalTargetConflict { .. } => "conditional_target_conflict",
            Self::ConditionalSetOnOperation { .. } => "conditional_set_on_operation",
            Self::EmptyOutcomeName { .. } => "empty_outcome_name",
            Self::DuplicateOutcome { .. } => "duplicate_outcome",
            Self::AmbiguousDefaultOutcome { .. } => "ambiguous_default_outcome",
            Self::DuplicateWrongStateOutcome { .. } => "duplicate_wrong_state_outcome",
            Self::WrongStateOnCreate { .. } => "wrong_state_on_create",
            Self::WrongStateWithSelector { .. } => "wrong_state_with_selector",
            Self::WrongStateWithStateGuard { .. } => "wrong_state_with_state_guard",
            Self::WrongStateUnreachable { .. } => "wrong_state_unreachable",
            Self::GuardStateOutsideMove { .. } => "guard_state_outside_move",
            Self::CreatesEffectOnOperation { .. } => "creates_effect_on_operation",
            Self::MissingCreatesEffect { .. } => "missing_creates_effect",
            Self::RefusalMutatesState { .. } => "refusal_mutates_state",
            Self::UnobservableOutcome { .. } => "unobservable_outcome",
            Self::UnknownOutcomeState { .. } => "unknown_outcome_state",
            Self::ResponseFieldUnknown { .. } => "response_field_unknown",
            Self::OutcomeResponseIncomplete { .. } => "outcome_response_incomplete",
            Self::IdentityFieldUnknown { .. } => "identity_field_unknown",
            Self::IdentityFieldNotAddressable { .. } => "identity_field_not_addressable",
            Self::RelationViaUnknown { .. } => "relation_via_unknown",
            Self::RelationViaWrongShape { .. } => "relation_via_wrong_shape",
            Self::RelationCarrierOptionality { .. } => "relation_carrier_optionality",
            Self::RelationTargetMissing { .. } => "relation_target_missing",
            Self::RelationSecondOwner { .. } => "relation_second_owner",
            Self::RelationCarrierWrong { .. } => "relation_carrier_wrong",
            Self::RelationFieldClaimedTwice { .. } => "relation_field_claimed_twice",
            Self::MapKeyNotText { .. } => "map_key_not_text",
            Self::MapValueMissing { .. } => "map_value_missing",
            Self::UnionTagCollides { .. } => "union_tag_collides",
            Self::UnionVariantMissing { .. } => "union_variant_missing",
            Self::QuantifierBindInvalid { .. } => "quantifier_bind_invalid",
            Self::QuantifierOverNotCollection { .. } => "quantifier_over_not_collection",
            Self::QuantifierBodyScope { .. } => "quantifier_body_scope",
            Self::ConditionTooDeep { .. } => "condition_too_deep",
            Self::CompareOperandNotAddressable { .. } => "compare_operand_not_addressable",
            Self::ScaleUnnamed => "scale_unnamed",
            Self::ScaleEmpty { .. } => "scale_empty",
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
            Self::UnknownRelationTarget {
                entity,
                path,
                target,
            } => write!(
                f,
                "{entity}'s '{path}' points at entity '{target}', which is not registered"
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
            Self::SemanticsKeyNotAvailable { path, key } => {
                let required = match key.as_str() {
                    "set_if_present" | "payload_if_present" | "responds_if_present" => {
                        "service/2"
                    }
                    _ => "service/1",
                };
                write!(
                    f,
                    "'{key}' at '{path}' is available only under `semantics: {required}`; a \
                     definition with older semantics would declare a rule nothing evaluates"
                )
            }
            Self::ConditionalArgumentInvalid {
                path,
                argument,
                message,
            } => write!(
                f,
                "conditional argument '{argument}' at '{path}' is invalid: {message}"
            ),
            Self::ConditionalTargetInvalid {
                path,
                field,
                message,
            } => write!(
                f,
                "conditional target '{field}' at '{path}' is invalid: {message}"
            ),
            Self::ConditionalTargetConflict { path, field } => write!(
                f,
                "conditional target '{field}' at '{path}' is also produced by the ordinary map"
            ),
            Self::ConditionalSetOnOperation {
                operation,
                outcome,
                field,
            } => write!(
                f,
                "operation '{operation}' outcome '{outcome}' conditionally writes field '{field}'; conditional state insertion is creation-only"
            ),
            Self::EmptyOutcomeName { command } => {
                write!(f, "'{command}' declares an outcome with an empty name")
            }
            Self::DuplicateOutcome { command, outcome } => write!(
                f,
                "'{command}' declares outcome '{outcome}' more than once"
            ),
            Self::AmbiguousDefaultOutcome { command } => write!(
                f,
                "'{command}' must declare at most one outcome with no `when`, `in_state` or \
                 `wrong_state`, and it is declared last among the branches that are not \
                 `wrong_state`"
            ),
            Self::DuplicateWrongStateOutcome { operation } => write!(
                f,
                "operation '{operation}' declares more than one `wrong_state` outcome"
            ),
            Self::WrongStateOnCreate { outcome } => write!(
                f,
                "creation outcome '{outcome}' declares `wrong_state`, but a creation has no \
                 subject resting in a state"
            ),
            Self::WrongStateWithSelector { operation, outcome } => write!(
                f,
                "operation '{operation}' outcome '{outcome}' declares `wrong_state` beside its own \
                 `when` or `in_state`"
            ),
            Self::WrongStateWithStateGuard { operation } => write!(
                f,
                "operation '{operation}' declares both a `wrong_state` outcome and an `in_state` \
                 outcome; a command using explicit state guards cannot also declare a wrong-state \
                 branch, because overlapping precedence is not inferred"
            ),
            Self::WrongStateUnreachable { operation } => write!(
                f,
                "operation '{operation}' declares a `wrong_state` outcome, but its moves already \
                 start from every declared state, so no state is a wrong one"
            ),
            Self::GuardStateOutsideMove {
                operation,
                outcome,
                state,
            } => write!(
                f,
                "operation '{operation}' outcome '{outcome}' is guarded on state '{state}', which \
                 its own move does not start from"
            ),
            Self::CreatesEffectOnOperation { operation, outcome } => write!(
                f,
                "operation '{operation}' outcome '{outcome}' declares the `creates` effect, which \
                 only a creation branch may"
            ),
            Self::MissingCreatesEffect { outcome, effect } => write!(
                f,
                "creation outcome '{outcome}' declares effect '{effect}'; a creation branch's \
                 effect is `creates` or `none`"
            ),
            Self::RefusalMutatesState { command, outcome } => write!(
                f,
                "'{command}' outcome '{outcome}' refuses and also writes, emits, responds or \
                 changes state; a refusal produces nothing durable"
            ),
            Self::UnobservableOutcome { command, outcome } => write!(
                f,
                "'{command}' outcome '{outcome}' declares no `emits`, no `set`, no `responds`, no \
                 `refuses` and no effect, so nothing about taking it could be observed"
            ),
            Self::UnknownOutcomeState {
                command,
                outcome,
                state,
            } => write!(
                f,
                "'{command}' outcome '{outcome}' names state '{state}', which the lifecycle does \
                 not declare"
            ),
            Self::ResponseFieldUnknown {
                command,
                outcome,
                field,
            } => write!(
                f,
                "'{command}' outcome '{outcome}' responds with '{field}', which the declared \
                 response does not carry"
            ),
            Self::OutcomeResponseIncomplete {
                command,
                outcome,
                field,
            } => write!(
                f,
                "'{command}' outcome '{outcome}' leaves required response field '{field}' \
                 undetermined"
            ),
            Self::IdentityFieldUnknown { field } => write!(
                f,
                "identity names '{field}', which is not a declared required field of the schema"
            ),
            Self::IdentityFieldNotAddressable { field, kind } => write!(
                f,
                "identity field '{field}' is a {kind} field, which has no address function"
            ),
            Self::RelationViaUnknown { relation, via } => write!(
                f,
                "relation '{relation}' is carried by '{via}', which this definition does not declare"
            ),
            Self::RelationViaWrongShape {
                relation,
                via,
                expected,
                found,
            } => write!(
                f,
                "relation '{relation}' is carried by '{via}', which is {found}; that row carries \
                 {expected}"
            ),
            Self::RelationCarrierOptionality { relation, via } => write!(
                f,
                "relation '{relation}' is carried by optional field '{via}'; only a \
                 references/one carrier may be optional"
            ),
            Self::RelationTargetMissing {
                entity,
                relation,
                target,
            } => write!(
                f,
                "{entity}'s relation '{relation}' points at entity '{target}', which is not \
                 registered"
            ),
            Self::RelationSecondOwner {
                target,
                owner,
                other,
            } => write!(
                f,
                "entity '{target}' is owned by both '{owner}' and '{other}'; one entity has at \
                 most one owner"
            ),
            Self::RelationCarrierWrong {
                entity,
                relation,
                via,
                detail,
            } => write!(
                f,
                "{entity}'s relation '{relation}' is carried by '{via}' on its target: {detail}"
            ),
            Self::RelationFieldClaimedTwice {
                entity,
                field,
                relation,
                other,
            } => write!(
                f,
                "{entity}'s field '{field}' carries both relation '{relation}' and '{other}'"
            ),
            Self::MapKeyNotText { path } => write!(
                f,
                "invalid field definition at '{path}': a map declares `key`, the spelling its keys \
                 are checked as"
            ),
            Self::MapValueMissing { path } => write!(
                f,
                "invalid field definition at '{path}': a map declares `items`, the definition its \
                 values satisfy"
            ),
            Self::UnionTagCollides { path, tag } => write!(
                f,
                "invalid field definition at '{path}': tag '{tag}' collides with the content key a \
                 variant would be carried under"
            ),
            Self::UnionVariantMissing { path } => write!(
                f,
                "invalid field definition at '{path}': a union declares `tag` and at least one \
                 entry in `variants`"
            ),
            Self::QuantifierBindInvalid { path, bind } => write!(
                f,
                "invalid rule at '{path}': `as` is one non-empty path segment; found '{bind}'"
            ),
            Self::QuantifierOverNotCollection { path, over, detail } => write!(
                f,
                "invalid rule at '{path}': `in` names '{over}', which is not a collection — {detail}"
            ),
            Self::QuantifierBodyScope {
                path,
                expression,
                detail,
            } => write!(
                f,
                "invalid rule at '{path}': the body reads '{expression}', which neither the \
                 enclosing scope nor the binder admits — {detail}"
            ),
            Self::ConditionTooDeep { path, depth, limit } => write!(
                f,
                "invalid rule at '{path}': the condition nests {depth} deep, and the limit is {limit}"
            ),
            Self::CompareOperandNotAddressable { path, side } => write!(
                f,
                "invalid rule at '{path}': the {side} operand of `compare` is a literal list or \
                 mapping, which has no scalar spelling to compare"
            ),
            Self::ScaleUnnamed => write!(f, "a declared scale's name cannot be empty"),
            Self::ScaleEmpty { scale } => {
                write!(f, "scale '{scale}' declares no values")
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

/// Every defect in one definition, in the order they were found.
///
/// Non-empty by construction: it is only ever produced instead of an `Ok`.
///
/// Validation used to stop at the first defect, so a document with four of them took four
/// attempts to fix and each attempt told you nothing about the next. Value validation already
/// reported every failing field at once (R-23); this is the same courtesy for the definition
/// itself, and `aep` invariant 3 asks for it by name.
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
    /// A prepared operation was continued with a different subject identity.
    SubjectMismatch {
        /// The prepared definition's entity.
        entity: String,
        /// The identity supplied during preparation.
        expected_id: String,
        /// The loaded instance's identity.
        actual_id: String,
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
    /// The instance is already at the largest revision every bundled provider can represent.
    RevisionExhausted {
        /// The entity type.
        entity: String,
        /// The instance identity.
        id: String,
        /// The terminal revision.
        revision: u64,
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

    /// The selected branch refuses, by the name of the error it declares.
    ///
    /// Not a defect: the model said this is what happens. A refusal produces no
    /// [`DecisionRecord`](crate::DecisionRecord), no revision, no state, no events and no response,
    /// so nothing about it reaches a store. This is how a refusing branch reaches a caller of
    /// [`create`](crate::create) or [`execute`](crate::execute), which keep their
    /// `Result<Decision, CoreError>` return; a caller that wants the branch as a value uses
    /// [`decide`](crate::decide) or [`decide_create`](crate::decide_create) instead.
    ///
    /// The error's **name** only: the source determines no error field values, so a payload is a
    /// binding obligation rather than a kernel invention.
    Refused {
        /// The branch that refused.
        outcome: String,
        /// The declared error's name.
        error: String,
        /// What to say to a person, if the branch declares it.
        message: Option<String>,
    },
    /// No branch of the command applies to this input.
    NoOutcomeSelected {
        /// The operation, or `create`.
        operation: String,
    },
    /// A branch's input guard could not be answered, so the command is refused rather than handed
    /// to a branch its author wrote for a different fact.
    ///
    /// Selection stops here and no later branch is tried. Reading an unanswerable guard as *not
    /// this branch* is the collapse the three-valued rules exist to prevent.
    OutcomeUnobservable {
        /// The operation, or `create`.
        operation: String,
        /// The branch whose guard could not be answered.
        outcome: String,
        /// Every reference the guard reads that resolved to nothing, sorted and without repeats.
        unresolved: Vec<String>,
    },
    /// The selected branch's move does not start where the instance rests, and no `wrong_state`
    /// branch answers for that state either.
    ///
    /// A statement about the **specification**, which is why it is not a branch result and not a
    /// [`Refused`](Self::Refused): no branch of the model claimed this case. The source admits the
    /// command and answers nothing for this one (state, input) pair, so this names the four facts a
    /// reader needs to repair it, and produces nothing durable.
    UnspecifiedMoveSource {
        /// The operation.
        operation: String,
        /// The branch the input selected.
        outcome: String,
        /// Where the instance rests.
        state: String,
        /// The states the selected branch's move does start from.
        from: Vec<String>,
    },
    /// The identity field no longer mirrors the storage address.
    ///
    /// Checked after every branch's `set`, so a rule judging the instance judges one whose address
    /// and identity field already agree.
    IdentityMismatch {
        /// The declared identity field.
        field: String,
        /// The storage address the instance carries.
        id: String,
        /// The address the identity field's value derives to, or why it has none.
        value: String,
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
            Self::SubjectMismatch { .. } => "subject_mismatch",
            Self::UnknownState { .. } => "unknown_state",
            Self::RevisionExhausted { .. } => "revision_exhausted",
            Self::OperationNotFound { .. } => "operation_not_found",
            Self::InvalidTransition { .. } => "invalid_transition",
            Self::PreconditionFailed { .. } => "precondition_failed",
            Self::PreconditionUnobservable { .. } => "precondition_unobservable",
            Self::InvariantViolation { .. } => "invariant_violation",
            Self::InvariantUnobservable { .. } => "invariant_unobservable",
            Self::Template { .. } => "template",
            Self::Refused { .. } => "refused",
            Self::NoOutcomeSelected { .. } => "no_outcome_selected",
            Self::OutcomeUnobservable { .. } => "outcome_unobservable",
            Self::UnspecifiedMoveSource { .. } => "unspecified_move_source",
            Self::IdentityMismatch { .. } => "identity_mismatch",
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
            Self::SubjectMismatch {
                entity,
                expected_id,
                actual_id,
            } => write!(
                f,
                "prepared {entity} subject mismatch: expected identity '{expected_id}', got '{actual_id}'"
            ),
            Self::UnknownState { entity, state } => write!(
                f,
                "instance claims lifecycle state '{state}', which '{entity}' does not declare"
            ),
            Self::RevisionExhausted {
                entity,
                id,
                revision,
            } => write!(
                f,
                "{entity} {id} is at terminal revision {revision}; no provider can store another revision"
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
            Self::Refused {
                outcome,
                error,
                message,
            } => match message {
                Some(message) => {
                    write!(f, "outcome '{outcome}' refuses with '{error}': {message}")
                }
                None => write!(f, "outcome '{outcome}' refuses with '{error}'"),
            },
            Self::NoOutcomeSelected { operation } => write!(
                f,
                "no outcome of '{operation}' applies to this input; the command declares no branch \
                 that answers for it"
            ),
            Self::OutcomeUnobservable {
                operation,
                outcome,
                unresolved,
            } => write!(
                f,
                "the guard of outcome '{outcome}' of '{operation}' cannot be evaluated; {}",
                nothing_observed_at(unresolved)
            ),
            Self::UnspecifiedMoveSource {
                operation,
                outcome,
                state,
                from,
            } => write!(
                f,
                "the input selected outcome '{outcome}' of '{operation}', whose move starts from \
                 [{}] and not from '{state}' where the instance rests; '{state}' is a source of \
                 some other move of this operation, so it is not a wrong state either, and the \
                 specification says what happens for no such pair",
                from.join(", ")
            ),
            Self::IdentityMismatch { field, id, value } => write!(
                f,
                "identity field '{field}' addresses to {value}, but the instance is stored at \
                 '{id}'"
            ),
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
