//! The kernel: the only code that produces a [`Decision`].

use crate::{
    observed::Observed,
    validation::{apply_defaults, validate_object_under},
    CompareOp, Comparison, Condition, CoreError, EntityDefinition, EventDefinition, FieldKind,
    ObjectSchema, OperationDefinition, OutcomeDefinition, OutcomeEffect, Quantifier,
    RefusalDefinition, Registry, RuleDefinition, Truth, ValidatedDefinition,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

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

    /// The state the instance was in before. `None` on a creation event: there was no before.
    pub from_state: Option<String>,

    /// The state it is in after. Written by the kernel when the operation was permitted, which is
    /// what lets a fold set a lifecycle state without becoming a second way to set one (R-34).
    pub to_state: String,

    /// The fields this operation wrote, and only those. Every field on a creation event.
    ///
    /// Recorded because an event that says only *what happened* cannot be folded back into an
    /// instance: `set:` assignments would be lost, and a rehydrated instance would silently differ
    /// from the one the operations returned. An event that cannot rebuild what it describes is a
    /// notification, not a record.
    pub changed: Map<String, Value>,

    /// The arguments the operation was decided on — what the rules read when they permitted it —
    /// verbatim, after defaults and schema validation. On a creation event, the creation's fields.
    ///
    /// The kernel has no clock and no lookup (R-62): what the world knew entered as `$args`, and
    /// a precondition that read `$args.evidence.test_result >= 1` left an event that could not say
    /// what the count was. Now it can, and a fold checks it (R-97): a replayed history whose
    /// arguments would not have satisfied the preconditions is refused. Required when parsed —
    /// an event with no `args` key is not an event this kernel wrote.
    pub args: Map<String, Value>,

    /// The payload, with every template reference resolved.
    pub payload: Value,
}

/// The normalized command whose evaluation produced a decision record.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "command", rename_all = "snake_case", deny_unknown_fields)]
pub enum DecisionCommand {
    /// Creation from caller-supplied fields, after defaults and canonicalization.
    Create {
        /// The normalized creation fields — what the instance became.
        fields: Map<String, Value>,
        /// The creation command's input, and what re-selects the branch on replay. Empty for a
        /// `kernel/1` creation, whose input *is* its fields.
        ///
        /// The two are redundant by construction and cannot drift: [`replay`](crate::replay)
        /// recomputes the whole decision and byte-compares the record, so a record whose `fields`
        /// are not what its `arguments` produce is refused.
        #[serde(default, skip_serializing_if = "Map::is_empty")]
        arguments: Map<String, Value>,
    },
    /// One named operation with its validated/defaulted arguments.
    Execute {
        /// The operation name.
        operation: String,
        /// The normalized arguments.
        arguments: Map<String, Value>,
    },
    /// Material imported without enough original command data for genesis replay.
    LegacyImport,
}

/// Durable evidence of one complete kernel evaluation.
///
/// Events are nested under the decision that assigned their revision, so zero, one and many-event
/// decisions all replay with the same revision semantics.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DecisionRecord {
    /// The exact validated definition snapshot used by the decision.
    pub definition: Option<EntityDefinition>,
    /// The normalized input command.
    pub command: DecisionCommand,
    /// The subject entity type.
    pub entity: String,
    /// The subject identity.
    pub id: String,
    /// The revision produced.
    pub revision: u64,
    /// The state before execution, or `None` for creation.
    pub from_state: Option<String>,
    /// The state produced.
    pub to_state: String,
    /// The complete resulting state, so a store never has to trust a separate mutable snapshot.
    pub result: EntityInstance,
    /// Every field whose value changed.
    pub changed: Map<String, Value>,
    /// Ordered domain facts emitted by this one decision.
    pub events: Vec<DomainEvent>,
    /// The named branch the kernel selected. `None` where the branch was the implicit one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<String>,
    /// What the selected branch did. `None` where the branch was the implicit one.
    ///
    /// Written into the record rather than inferred from `from_state == to_state`, because a
    /// definition may legitimately declare a self-transition and an update is not one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effect: Option<DecisionEffect>,
    /// The declared response the selected branch determined. `None` where the branch was the
    /// implicit one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response: Option<Map<String, Value>>,
}

/// What a decision did to the instance.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DecisionEffect {
    /// The instance came into being.
    Created,
    /// It moved between declared states.
    Moved,
    /// It was written without leaving its state, and no self-transition was synthesized.
    Updated,
    /// It was accepted and nothing about it changed. Still one revision on.
    Unchanged,
}

/// What the kernel answered: an accepted decision, or a branch that refuses by name.
///
/// The branch-aware result. [`create`] and [`execute`] keep their `Result<Decision, CoreError>`
/// return and reach a refusing branch as [`CoreError::Refused`], so every existing caller compiles
/// and behaves as it does now; both pairs share one implementation.
///
/// The two variants are very different sizes, and the larger one is not boxed: a decision is what
/// the accepting path produces on every successful call, and putting it behind an allocation to
/// even the variants out would cost that path an allocation to spare the refusing path a move.
#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum Evaluation {
    /// The command was decided.
    Accepted(Decision),
    /// The selected branch refuses. Nothing durable was produced.
    Refused(Refusal),
}

impl Evaluation {
    /// The decision, or the branch's refusal as a typed kernel error.
    ///
    /// # Errors
    ///
    /// [`CoreError::Refused`], carrying the branch's name and its declared error.
    pub fn into_decision(self) -> Result<Decision, CoreError> {
        match self {
            Self::Accepted(decision) => Ok(decision),
            Self::Refused(refusal) => Err(CoreError::Refused {
                outcome: refusal.outcome,
                error: refusal.error,
                message: refusal.message,
            }),
        }
    }

    /// The decision, if the command was accepted.
    #[must_use]
    pub fn accepted(&self) -> Option<&Decision> {
        match self {
            Self::Accepted(decision) => Some(decision),
            Self::Refused(_) => None,
        }
    }
}

/// A branch that refuses, and the error it refuses with.
///
/// Carries the error's **name**. The source determines no error field values, so the payload is a
/// binding obligation rather than a kernel invention.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    /// The branch that refused.
    pub outcome: String,
    /// The declared error's name.
    pub error: String,
    /// What to say to a person, if the branch declares it.
    pub message: Option<String>,
}

/// What the kernel decided: the instance as it is afterwards, and its durable record.
///
/// A `Decision` is the only thing the kernel produces. Persisting the instance, appending the
/// events and publishing them are the shell's, and are expected to happen together.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Decision {
    /// The instance after the operation.
    pub instance: EntityInstance,
    /// The complete normalized record used for durable storage and verified replay.
    pub record: DecisionRecord,
    /// Zero or more events, in declaration order.
    ///
    /// Kept as a source- and wire-compatibility view for 0.14 adopters. Durable stores use the
    /// events nested in [`DecisionRecord`]; serializers retain this view so a decision printed by
    /// an older shell consumer still has the shape that consumer understands.
    pub events: Vec<DomainEvent>,
}

impl Decision {
    /// Builds an explicitly unverified legacy-import decision.
    ///
    /// New state-changing code must use [`create`] or [`execute`]. This constructor exists for
    /// migrations and provider repair tools that must preserve old bytes without inventing the
    /// definition or command that produced them.
    #[must_use]
    pub fn legacy_import(instance: EntityInstance, events: Vec<DomainEvent>) -> Self {
        let record = DecisionRecord {
            definition: None,
            command: DecisionCommand::LegacyImport,
            entity: instance.entity.clone(),
            id: instance.id.clone(),
            revision: instance.revision,
            from_state: None,
            to_state: instance.lifecycle_state.clone(),
            result: instance.clone(),
            changed: instance.fields.clone(),
            events: events.clone(),
            outcome: None,
            effect: None,
            response: None,
        };
        Self {
            instance,
            record,
            events,
        }
    }
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

    /// Creates an instance, answering a refusing branch as a value rather than as an error.
    ///
    /// # Errors
    ///
    /// [`CoreError::EntityNotRegistered`] when no such definition is registered; otherwise
    /// whatever [`decide_create`] returns.
    pub fn decide_create(
        &self,
        entity: &str,
        version: u32,
        id: impl Into<String>,
        input: Value,
    ) -> Result<Evaluation, CoreError> {
        let definition = self.definition(entity, version)?;
        decide_create(definition, id.into(), input)
    }

    /// Executes an operation, answering a refusing branch as a value rather than as an error.
    ///
    /// # Errors
    ///
    /// [`CoreError::EntityNotRegistered`] when the instance's `(entity, version)` is not
    /// registered; otherwise whatever [`decide`] returns.
    pub fn decide(
        &self,
        instance: &EntityInstance,
        operation: &str,
        arguments: Value,
    ) -> Result<Evaluation, CoreError> {
        let definition = self.definition(&instance.entity, instance.version)?;
        decide(definition, instance, operation, arguments)
    }

    fn definition(&self, entity: &str, version: u32) -> Result<&ValidatedDefinition, CoreError> {
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
    definition: &ValidatedDefinition,
    id: String,
    fields: Value,
) -> Result<Decision, CoreError> {
    decide_create(definition, id, fields)?.into_decision()
}

/// Creates an instance under `definition`, answering a refusing branch as a value.
///
/// Under `kernel/1` the third argument is the creation **fields**, exactly as [`create`] has always
/// read it. Under `service/1` it is the creation **arguments**: the caller's input, from which the
/// selected branch's `set` produces the fields. That is the whole reason a `service/1` creation
/// reconstructs its original request as `arguments` and not as `fields` — reconstructing it as the
/// fields the branch produced would hand a retry a request the caller never sent.
///
/// # Errors
///
/// * [`CoreError::Validation`] — `id` is empty, the input is not an object, or a value does not
///   satisfy its schema; every failure is listed.
/// * [`CoreError::NoOutcomeSelected`] / [`CoreError::OutcomeUnobservable`] — no branch applies, or
///   a branch's guard could not be answered.
/// * [`CoreError::IdentityMismatch`] — the identity field does not mirror the storage address.
/// * [`CoreError::InvariantViolation`] — an invariant does not hold for the new instance.
/// * [`CoreError::Template`] — a `set`, event payload or response references something missing.
pub fn decide_create(
    definition: &ValidatedDefinition,
    id: String,
    input: Value,
) -> Result<Evaluation, CoreError> {
    if id.trim().is_empty() {
        return Err(CoreError::Validation(vec![crate::ValidationError::new(
            "id",
            "identity cannot be empty; the kernel generates none, so the caller supplies one",
        )]));
    }
    if definition.semantics.is_service_1() && !definition.create.outcomes.is_empty() {
        return service_create(definition, id, input);
    }

    // Step 3. A `kernel/1` creation's input *is* its fields; there are no arguments to normalize.
    let mut object = into_object(input, "fields")?;
    apply_defaults(&definition.schema, &mut object);

    let validation =
        validate_object_under(&definition.schema, &object, "fields", definition.semantics);
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
        arguments_schema: None,
        old_fields: &empty,
        new_fields: &instance.fields,
        from_state: None,
        to_state: &instance.lifecycle_state,
    };

    // Step 11. A `kernel/1` definition cannot declare `identity` at all, so this is a no-op there
    // and no committed identifier moves.
    check_identity_mirror(definition, &instance)?;
    check_invariants(definition, &context)?;

    let mut events = Vec::new();
    if let Some(event) = &definition.create.emit {
        // A creation's arguments are its fields: that is what the caller presented and what the
        // schema checked, so that is what the event records it was decided on.
        events.push(materialize_event(
            event,
            &context,
            instance.revision,
            &instance.fields,
        )?);
    }

    let record = DecisionRecord {
        definition: Some(definition_snapshot(definition)),
        command: DecisionCommand::Create {
            fields: instance.fields.clone(),
            arguments: Map::new(),
        },
        entity: instance.entity.clone(),
        id: instance.id.clone(),
        revision: instance.revision,
        from_state: None,
        to_state: instance.lifecycle_state.clone(),
        result: instance.clone(),
        changed: instance.fields.clone(),
        events: events.clone(),
        outcome: None,
        effect: None,
        response: None,
    };
    Ok(Evaluation::Accepted(Decision {
        instance,
        record,
        events,
    }))
}

/// The `service/1` creation: steps 3, 4, 6, 8, 9, 10, 11, 12, 13, 14 and 15 of the numbered order.
///
/// Steps 0, 1, 2 and 5 are omitted — there is no instance to match, no operation to find and no
/// state to move from — and step 7 with them: a creation has no preconditions of its own.
fn service_create(
    definition: &ValidatedDefinition,
    id: String,
    input: Value,
) -> Result<Evaluation, CoreError> {
    // Step 3. Defaults, then validation, against the creation command's declared arguments.
    let mut args = into_object(input, "arguments")?;
    apply_defaults(&definition.create.arguments, &mut args);
    let errors = validate_object_under(
        &definition.create.arguments,
        &args,
        "arguments",
        definition.semantics,
    );
    if !errors.is_empty() {
        return Err(CoreError::Validation(errors));
    }

    let initial = definition.lifecycle.initial.clone();
    let empty = Map::new();
    // Step 4, in `CreateSelector`: at creation there is no instance, so no fields and no
    // from-state, and the initial state is a constant of the lifecycle rather than a fact about a
    // subject. Registration refuses a creation selector that reads either.
    let selector_context = TemplateContext {
        definition,
        id: &id,
        args: &args,
        arguments_schema: Some(&definition.create.arguments),
        old_fields: &empty,
        new_fields: &empty,
        from_state: None,
        to_state: &initial,
    };
    let outcome = select_outcome(
        CREATE_COMMAND,
        &definition.create.outcomes,
        None,
        &selector_context,
    )?;

    // Step 6. A refusing branch returns here: no record, no revision, no state, no events and no
    // response.
    if let Some(refusal) = &outcome.refuses {
        return Ok(Evaluation::Refused(Refusal {
            outcome: outcome.name.clone(),
            error: refusal.error.clone(),
            message: refusal.message.clone(),
        }));
    }

    // Step 8, in `CreateSet`: at creation the fields are what `set` is producing, so the scope
    // admits no `$fields` and this context carries none.
    let set_context = TemplateContext {
        definition,
        id: &id,
        args: &args,
        arguments_schema: Some(&definition.create.arguments),
        old_fields: &empty,
        new_fields: &empty,
        from_state: None,
        to_state: &initial,
    };
    let mut fields = Map::new();
    for (field, template) in &outcome.set {
        fields.insert(field.clone(), resolve_template(template, &set_context)?);
    }
    // The declared defaults fill what the branch did not write. A `kernel/1` creation defaults at
    // step 3 because its input is its fields; a `service/1` creation's fields do not exist until
    // the branch has run, so the same rule applies here and the result is validated below either
    // way.
    apply_defaults(&definition.schema, &mut fields);
    let fields = canonical_object(fields);

    // Step 9.
    let errors = validate_object_under(&definition.schema, &fields, "fields", definition.semantics);
    if !errors.is_empty() {
        return Err(CoreError::Validation(errors));
    }

    // Step 10. The state is the lifecycle's initial and the revision is 1 rather than a successor.
    let instance = EntityInstance {
        entity: definition.entity.clone(),
        version: definition.version,
        id,
        lifecycle_state: initial,
        revision: 1,
        fields,
    };

    // Step 11, before the invariants, so a rule judging the instance judges one whose address and
    // identity field already agree.
    check_identity_mirror(definition, &instance)?;

    // Steps 13 and 14 read `CreateOutcomeTemplate`: the post-`set` fields and the arguments alike,
    // because both are published to a caller that sent the arguments and a creation may publish an
    // argument no field stores.
    let context = TemplateContext {
        definition,
        id: &instance.id,
        args: &args,
        arguments_schema: Some(&definition.create.arguments),
        old_fields: &empty,
        new_fields: &instance.fields,
        from_state: None,
        to_state: &instance.lifecycle_state,
    };

    // Step 12.
    check_invariants(definition, &context)?;

    // Step 13. A `service/1` creation *does* have arguments, and they are what `args` records.
    let mut events = Vec::with_capacity(outcome.emits.len());
    for event in &outcome.emits {
        events.push(materialize_event(
            event,
            &context,
            instance.revision,
            &args,
        )?);
    }

    // Step 14, after the events, so both read one set of post-`set` fields.
    let response = materialize_response(&outcome.responds, &context)?;

    let record = DecisionRecord {
        definition: Some(definition_snapshot(definition)),
        command: DecisionCommand::Create {
            fields: instance.fields.clone(),
            arguments: args,
        },
        entity: instance.entity.clone(),
        id: instance.id.clone(),
        revision: instance.revision,
        from_state: None,
        to_state: instance.lifecycle_state.clone(),
        result: instance.clone(),
        changed: instance.fields.clone(),
        events: events.clone(),
        outcome: Some(outcome.name.clone()),
        effect: Some(match outcome.effect {
            OutcomeEffect::Creates => DecisionEffect::Created,
            _ => DecisionEffect::Unchanged,
        }),
        response: Some(response),
    };
    Ok(Evaluation::Accepted(Decision {
        instance,
        record,
        events,
    }))
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
    definition: &ValidatedDefinition,
    instance: &EntityInstance,
    operation_name: &str,
    arguments: Value,
) -> Result<Decision, CoreError> {
    decide(definition, instance, operation_name, arguments)?.into_decision()
}

/// Executes `operation_name` on `instance`, answering a refusing branch as a value.
///
/// The sixteen steps, numbered 0 to 15, and a refusal at any of them returns before the next:
///
/// ```text
///   0  instance (entity, version) matches the definition        EntityMismatch
///   1  instance carries a state the definition declares         UnknownState
///   2  operation exists                                         OperationNotFound
///   3  arguments: defaults, then validation                     Validation
///   4  input selection                                          NoOutcomeSelected
///                                                               OutcomeUnobservable
///   5  state admissibility of the selected branch               InvalidTransition
///                                                               UnspecifiedMoveSource
///   6  a refusing branch returns here                           Evaluation::Refused
///   7  preconditions, against current state + arguments         PreconditionFailed
///   8  the selected branch's set, against pre-operation fields  Template
///   9  resulting fields validated against the schema            Validation
///  10  next instance: state from the branch's effect, +1 rev    —
///  11  identity mirror, when declared                           IdentityMismatch
///  12  invariants, against the next state                       InvariantViolation
///  13  the selected branch's events, in declaration order       Template
///  14  the selected branch's response, in schema order          Template
///  15  Evaluation::Accepted(Decision)                           —
/// ```
///
/// A `kernel/1` operation keeps this order and its refusal names exactly. Its single implicit
/// branch makes step 4 a no-op, makes step 5 answer `InvalidTransition` and only
/// `InvalidTransition`, and makes steps 6, 11 and 14 absent; renumbering 5 to 15 against the legacy
/// column recovers the twelve-step list unchanged.
///
/// # Errors
///
/// Every variant the step table names.
pub fn decide(
    definition: &ValidatedDefinition,
    instance: &EntityInstance,
    operation_name: &str,
    arguments: Value,
) -> Result<Evaluation, CoreError> {
    // Steps 0 and 1, in the order the code has always checked them.
    ensure_instance_matches(definition, instance)?;

    // Step 2.
    let operation =
        definition
            .operations
            .get(operation_name)
            .ok_or_else(|| CoreError::OperationNotFound {
                operation: operation_name.to_owned(),
            })?;

    // Step 3.
    let args = normalize_arguments(definition, operation_name, arguments)?;

    // The pre-operation fields are borrowed, never copied: they are only ever read, and `set`
    // resolves every assignment against them.
    let old_fields = &instance.fields;
    let selected = if definition.semantics.is_service_1() && !operation.outcomes.is_empty() {
        // Step 4, in `OutcomeSelector`. The destination state is what the selected branch's effect
        // produces, so a selector may read neither `$state` nor `$to_state`; registration refuses
        // one that does, which is why passing the held state here cannot be read.
        let selector_context = TemplateContext {
            definition,
            id: &instance.id,
            args: &args,
            arguments_schema: Some(&operation.arguments),
            old_fields,
            new_fields: old_fields,
            from_state: Some(&instance.lifecycle_state),
            to_state: &instance.lifecycle_state,
        };
        let outcome = select_outcome(
            operation_name,
            &operation.outcomes,
            Some(&instance.lifecycle_state),
            &selector_context,
        )?;
        // Step 5.
        let outcome = admit_state(
            definition,
            operation,
            operation_name,
            outcome,
            &instance.lifecycle_state,
        )?;
        Branch::outcome(outcome, &instance.lifecycle_state)
    } else {
        // Step 4 is a no-op for the implicit branch; step 5 is the transition selection, which
        // answers `InvalidTransition` and only `InvalidTransition`.
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
        Branch::implicit(operation, &transition.to)
    };

    // Step 6.
    if let Some((name, refusal)) = selected.refusal() {
        return Ok(Evaluation::Refused(Refusal {
            outcome: name.to_owned(),
            error: refusal.error.clone(),
            message: refusal.message.clone(),
        }));
    }

    let context = TemplateContext {
        definition,
        id: &instance.id,
        args: &args,
        arguments_schema: Some(&operation.arguments),
        old_fields,
        new_fields: old_fields,
        from_state: Some(&instance.lifecycle_state),
        to_state: &selected.to_state,
    };
    // Step 7.
    check_preconditions(operation_name, &operation.preconditions, &context)?;

    // Step 8.
    let mut new_fields = canonical_object(old_fields.clone());
    for (field, template) in selected.set {
        let value = resolve_template(template, &context)?;
        new_fields.insert(field.clone(), value);
    }

    // Step 9.
    let state_errors = validate_object_under(
        &definition.schema,
        &new_fields,
        "fields",
        definition.semantics,
    );
    if !state_errors.is_empty() {
        return Err(CoreError::Validation(state_errors));
    }

    // Step 10.
    let next_revision = instance
        .revision
        .checked_add(1)
        .filter(|revision| *revision <= i64::MAX as u64)
        .ok_or_else(|| CoreError::RevisionExhausted {
            entity: instance.entity.clone(),
            id: instance.id.clone(),
            revision: instance.revision,
        })?;
    let next_instance = EntityInstance {
        entity: instance.entity.clone(),
        version: instance.version,
        id: instance.id.clone(),
        lifecycle_state: selected.to_state.clone(),
        revision: next_revision,
        fields: new_fields,
    };

    // Step 11, before the invariants.
    check_identity_mirror(definition, &next_instance)?;

    let context = TemplateContext {
        definition,
        id: &instance.id,
        args: &args,
        arguments_schema: Some(&operation.arguments),
        old_fields,
        new_fields: &next_instance.fields,
        from_state: Some(&instance.lifecycle_state),
        to_state: &next_instance.lifecycle_state,
    };

    // Step 12.
    check_invariants(definition, &context)?;

    // Step 13, in declaration order, duplicates preserved.
    let mut events = Vec::with_capacity(selected.emits.len());
    for event in selected.emits {
        events.push(materialize_event(event, &context, next_revision, &args)?);
    }

    // Step 14, after the events, so both read one set of post-`set` fields.
    let response = match selected.responds {
        Some(responds) => Some(materialize_response(responds, &context)?),
        None => None,
    };

    let changed = changed_fields(&context);
    let record = DecisionRecord {
        definition: Some(definition_snapshot(definition)),
        command: DecisionCommand::Execute {
            operation: operation_name.to_owned(),
            arguments: args,
        },
        entity: next_instance.entity.clone(),
        id: next_instance.id.clone(),
        revision: next_instance.revision,
        from_state: Some(instance.lifecycle_state.clone()),
        to_state: next_instance.lifecycle_state.clone(),
        result: next_instance.clone(),
        changed,
        events: events.clone(),
        outcome: selected.name.map(ToOwned::to_owned),
        effect: selected.effect,
        response,
    };
    // Step 15.
    Ok(Evaluation::Accepted(Decision {
        instance: next_instance,
        record,
        events,
    }))
}

/// The name a creation's selection refusals carry, since a creation has no operation.
const CREATE_COMMAND: &str = "create";

/// The branch a command's evaluation selected, resolved to the five things every step after it
/// needs.
///
/// `kernel/1` is `service/1` with one implicit branch, so there is one evaluation path and not two.
struct Branch<'a> {
    name: Option<&'a str>,
    to_state: String,
    set: &'a BTreeMap<String, Value>,
    emits: &'a [EventDefinition],
    responds: Option<&'a BTreeMap<String, Value>>,
    effect: Option<DecisionEffect>,
    refuses: Option<&'a RefusalDefinition>,
}

impl<'a> Branch<'a> {
    /// The implicit branch: the transition selection a `kernel/1` operation performs, with the
    /// operation's own `set` and `emits`.
    fn implicit(operation: &'a OperationDefinition, to_state: &str) -> Self {
        Self {
            name: None,
            to_state: to_state.to_owned(),
            set: &operation.set,
            emits: &operation.emits,
            responds: None,
            effect: None,
            refuses: None,
        }
    }

    /// A named branch. `Moves` goes to its declared state; `Updates` and `None` keep the state the
    /// instance rests in, and no self-transition is synthesized for either.
    fn outcome(outcome: &'a OutcomeDefinition, from_state: &str) -> Self {
        let (to_state, effect) = match &outcome.effect {
            OutcomeEffect::Moves { to, .. } => (to.clone(), DecisionEffect::Moved),
            OutcomeEffect::Updates => (from_state.to_owned(), DecisionEffect::Updated),
            OutcomeEffect::None => (from_state.to_owned(), DecisionEffect::Unchanged),
            // Registration refuses `creates` on an operation branch.
            OutcomeEffect::Creates => (from_state.to_owned(), DecisionEffect::Created),
        };
        Self {
            name: Some(&outcome.name),
            to_state,
            set: &outcome.set,
            emits: &outcome.emits,
            responds: Some(&outcome.responds),
            effect: Some(effect),
            refuses: outcome.refuses.as_ref(),
        }
    }

    fn refusal(&self) -> Option<(&'a str, &'a RefusalDefinition)> {
        Some((self.name?, self.refuses?))
    }
}

/// Step 4: which branch this input takes, in declared order.
///
/// Two lines, and three shapes fall out of them. For each non-`wrong_state` branch:
///
/// 1. **The state test.** A declared `in_state` that is not the instance's current state **skips
///    the branch**, and its `when` is not evaluated: a guard whose branch the held state has
///    already excluded cannot make the command unobservable.
/// 2. **The selector test.** The branch's selector is its `when` if it declares one and `True` if
///    it does not — so a matching bare state guard is selected in exactly the states it names, for
///    every admitted input, which is the source's own reading rather than a convenience.
///
/// The input guard is answered **before** the held state wherever a branch declares no `in_state`,
/// which is what the source's own reference target does: a non-positive amount is refused whatever
/// state the invoice is in.
fn select_outcome<'a>(
    command: &str,
    outcomes: &'a [OutcomeDefinition],
    state: Option<&str>,
    context: &TemplateContext<'_>,
) -> Result<&'a OutcomeDefinition, CoreError> {
    for outcome in outcomes.iter().filter(|outcome| !outcome.wrong_state) {
        if let (Some(guarded), Some(state)) = (outcome.in_state.as_deref(), state) {
            if guarded != state {
                continue;
            }
        }
        let Some(when) = &outcome.when else {
            return Ok(outcome);
        };
        let mut unobserved = Unobserved::new();
        match evaluate_condition(when, context, None, &mut unobserved)? {
            Truth::True => return Ok(outcome),
            Truth::False => continue,
            // Selection stops here and no later branch is tried. Reading an unanswerable guard as
            // *not this branch* would hand the command to a branch its author wrote for a
            // different fact.
            Truth::Unknown => {
                return Err(CoreError::OutcomeUnobservable {
                    operation: command.to_owned(),
                    outcome: outcome.name.clone(),
                    unresolved: unobserved.into_iter().collect(),
                })
            }
        }
    }
    Err(CoreError::NoOutcomeSelected {
        operation: command.to_owned(),
    })
}

/// Every state some move of this operation starts from.
pub(crate) fn move_sources(operation: &OperationDefinition) -> BTreeSet<String> {
    operation
        .outcomes
        .iter()
        .filter_map(|outcome| outcome.effect.move_sources())
        .flat_map(|from| from.iter().cloned())
        .collect()
}

/// Every state **no** move of this operation starts from — the complement, taken over the union,
/// which is what the source's own condition means and computes.
pub(crate) fn wrong_states(
    definition: &EntityDefinition,
    operation: &OperationDefinition,
) -> BTreeSet<String> {
    let sources = move_sources(operation);
    definition
        .lifecycle
        .states
        .iter()
        .filter(|state| !sources.contains(*state))
        .cloned()
        .collect()
}

/// Step 5: whether the selected branch's move can start where the instance rests.
fn admit_state<'a>(
    definition: &EntityDefinition,
    operation: &'a OperationDefinition,
    operation_name: &str,
    outcome: &'a OutcomeDefinition,
    state: &str,
) -> Result<&'a OutcomeDefinition, CoreError> {
    let Some(from) = outcome.effect.move_sources() else {
        return Ok(outcome);
    };
    if from.iter().any(|source| source == state) {
        return Ok(outcome);
    }
    if wrong_states(definition, operation).contains(state) {
        // No move of this operation starts here, so the wrong-state branch is by the source's own
        // definition the one that answers — and where none is declared, this is exactly what
        // `kernel/1` returns today.
        return operation
            .outcomes
            .iter()
            .find(|outcome| outcome.wrong_state)
            .ok_or_else(|| CoreError::InvalidTransition {
                operation: operation_name.to_owned(),
                state: state.to_owned(),
            });
    }
    // Some *other* branch of this operation moves from here, but not the one the input selected.
    // The source neither validates this pair, nor synthesizes a scenario for it, nor runs it, nor
    // answers it in a reference target, so it is named rather than answered.
    Err(CoreError::UnspecifiedMoveSource {
        operation: operation_name.to_owned(),
        outcome: outcome.name.clone(),
        state: state.to_owned(),
        from: from.iter().cloned().collect(),
    })
}

/// Step 11: the identity field and the storage address still agree.
///
/// A no-op for a `kernel/1` definition, which cannot declare `identity` at all.
fn check_identity_mirror(
    definition: &EntityDefinition,
    instance: &EntityInstance,
) -> Result<(), CoreError> {
    let Some(identity) = &definition.identity else {
        return Ok(());
    };
    let kind = definition
        .schema
        .fields
        .get(&identity.field)
        .map_or(FieldKind::String, |field| field.kind);
    let value = instance.fields.get(&identity.field);
    let derived = match value {
        Some(value) => crate::identity::address(kind, value).map_err(|error| error.to_string()),
        None => Err("the identity field carries no value".to_owned()),
    };
    match derived {
        Ok(address) if address == instance.id => Ok(()),
        Ok(address) => Err(CoreError::IdentityMismatch {
            field: identity.field.clone(),
            id: instance.id.clone(),
            value: format!("'{address}'"),
        }),
        Err(detail) => Err(CoreError::IdentityMismatch {
            field: identity.field.clone(),
            id: instance.id.clone(),
            value: detail,
        }),
    }
}

/// Step 14: the branch's declared response, resolved in the scope its command sits in.
fn materialize_response(
    responds: &BTreeMap<String, Value>,
    context: &TemplateContext<'_>,
) -> Result<Map<String, Value>, CoreError> {
    let mut response = Map::new();
    for (field, template) in responds {
        response.insert(
            field.clone(),
            canonicalize(resolve_template(template, context)?),
        );
    }
    Ok(response)
}

fn definition_snapshot(definition: &ValidatedDefinition) -> EntityDefinition {
    let value = serde_json::to_value(definition.as_definition())
        .expect("an EntityDefinition always serializes to JSON");
    serde_json::from_value(canonicalize(value))
        .expect("canonicalizing JSON cannot change an EntityDefinition's shape")
}

/// Defaults and validates operation arguments without evaluating state-dependent rules.
/// Used to compare a retried command with the exact normalized intent recorded previously.
pub fn normalize_arguments(
    definition: &ValidatedDefinition,
    operation_name: &str,
    arguments: Value,
) -> Result<Map<String, Value>, CoreError> {
    let operation =
        definition
            .operations
            .get(operation_name)
            .ok_or_else(|| CoreError::OperationNotFound {
                operation: operation_name.to_owned(),
            })?;
    let mut args = into_object(arguments, "arguments")?;
    apply_defaults(&operation.arguments, &mut args);
    let errors = validate_object_under(
        &operation.arguments,
        &args,
        "arguments",
        definition.semantics,
    );
    if !errors.is_empty() {
        return Err(CoreError::Validation(errors));
    }
    Ok(args)
}

/// Every address a condition read and found nothing at, gathered as it is evaluated.
///
/// A `BTreeSet` rather than a `Vec`: sorted, so the same refusal prints the same way every time,
/// and without repeats, so a reference read by three operands is named once.
type Unobserved = BTreeSet<String>;

pub(crate) fn check_preconditions(
    operation: &str,
    rules: &[RuleDefinition],
    context: &TemplateContext<'_>,
) -> Result<(), CoreError> {
    for rule in rules {
        let mut unobserved = Unobserved::new();
        match evaluate_condition(&rule.condition, context, None, &mut unobserved)? {
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

pub(crate) fn check_invariants(
    definition: &EntityDefinition,
    context: &TemplateContext<'_>,
) -> Result<(), CoreError> {
    for rule in &definition.invariants {
        let mut unobserved = Unobserved::new();
        match evaluate_condition(&rule.condition, context, None, &mut unobserved)? {
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
    bindings: Option<&Bindings<'_>>,
    unobserved: &mut Unobserved,
) -> Result<Truth, CoreError> {
    match condition {
        Condition::Literal(value) => Ok(Truth::from_bool(*value)),
        Condition::All { all } => {
            let mut result = Truth::True;
            for condition in all {
                result = result.and(evaluate_condition(
                    condition, context, bindings, unobserved,
                )?);
            }
            Ok(result)
        }
        Condition::Any { any } => {
            let mut result = Truth::False;
            for condition in any {
                result = result.or(evaluate_condition(
                    condition, context, bindings, unobserved,
                )?);
            }
            Ok(result)
        }
        Condition::Not { not } => Ok(evaluate_condition(not, context, bindings, unobserved)?.not()),
        Condition::Compare { compare } => evaluate_compare(compare, context, bindings, unobserved),
        Condition::Truthy { truthy } => {
            match resolve_operand(truthy, context, bindings, unobserved)? {
                // ESS's `is_truthy`, arm for arm.
                Some(Value::Bool(value)) => Ok(Truth::from_bool(value)),
                Some(Value::Number(number)) => match Observed::of_number(&number) {
                    Some(observed) => Ok(Truth::from_bool(!observed.is_zero())),
                    // A token the source cannot observe is not a value it can answer about.
                    None => Ok(Truth::Unknown),
                },
                // "the fact is present, therefore true", with `false` as the one written exception.
                Some(Value::String(text)) => {
                    Ok(Truth::from_bool(!text.is_empty() && text != "false"))
                }
                // A list, a map, a union and a struct have no scalar spelling.
                Some(_) => Ok(Truth::Unknown),
                None => Ok(Truth::Unknown),
            }
        }
        Condition::ForAll { for_all } => {
            quantify(for_all, context, bindings, unobserved, Quantification::All)
        }
        Condition::ForAny { for_any } => {
            quantify(for_any, context, bindings, unobserved, Quantification::Any)
        }
        Condition::Exists { exists } => {
            // A question about the store, not about a value: the kernel holds the instance, so it
            // can always answer it. Nothing goes into `unobserved` — this operator *is* the
            // observation, and in `any: [{exists: $fields.a}, {eq: [$fields.b, 1]}]` naming
            // `$fields.a` beside the genuinely unreadable `$fields.b` would send whoever reads the
            // refusal after the wrong one.
            let mut answered = Unobserved::new();
            let resolved = resolve_operand(exists, context, bindings, &mut answered)?;
            Ok(Truth::from_bool(resolved.is_some()))
        }
        Condition::Before { before } => {
            compare_instants(before, context, bindings, unobserved, |left, right| {
                left < right
            })
        }
        Condition::After { after } => {
            compare_instants(after, context, bindings, unobserved, |left, right| {
                left > right
            })
        }
        Condition::Eq { eq } => {
            let equal = equality(context);
            compare_values(eq, context, bindings, unobserved, |left, right| {
                equal(left, right)
            })
        }
        Condition::Ne { ne } => {
            let equal = equality(context);
            compare_values(ne, context, bindings, unobserved, |left, right| {
                !equal(left, right)
            })
        }
        Condition::Gt { gt } => compare_numbers(gt, context, bindings, unobserved, Ordering::is_gt),
        Condition::Gte { gte } => compare_numbers(gte, context, bindings, unobserved, |order| {
            order.is_gt() || order.is_eq()
        }),
        Condition::Lt { lt } => compare_numbers(lt, context, bindings, unobserved, Ordering::is_lt),
        Condition::Lte { lte } => compare_numbers(lte, context, bindings, unobserved, |order| {
            order.is_lt() || order.is_eq()
        }),
        Condition::In { values } => {
            let equal = equality(context);
            let (needle, haystack) = resolve_pair(values, context, bindings, unobserved)?;
            match (needle, haystack) {
                (Some(needle), Some(Value::Array(values))) => Ok(Truth::from_bool(
                    values.iter().any(|value| equal(value, &needle)),
                )),
                // Both resolved, and the haystack is not a list: observed, and it does not hold.
                (Some(_), Some(_)) => Ok(Truth::False),
                _ => Ok(Truth::Unknown),
            }
        }
        Condition::Contains { contains } => {
            let equal = equality(context);
            let (container, needle) = resolve_pair(contains, context, bindings, unobserved)?;
            match (container, needle) {
                (Some(Value::Array(values)), Some(needle)) => Ok(Truth::from_bool(
                    values.iter().any(|value| equal(value, &needle)),
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

/// Which equality `eq`, `ne`, `in` and `contains` use.
///
/// Under `service/1` two numbers compare through the source observation rule, because these four
/// are what a lowering emits for the source's own equality and membership tests and a membership
/// test that disagreed with `compare` would be a second numeric semantics inside one document.
/// Under `kernel/1` every answer is the one it is today.
fn equality(context: &TemplateContext<'_>) -> fn(&Value, &Value) -> bool {
    if context.definition.semantics.is_service_1() {
        observed_values_equal
    } else {
        values_equal
    }
}

/// Which branch of a quantifier's fold is being taken.
#[derive(Clone, Copy)]
enum Quantification {
    All,
    Any,
}

/// `for_all` / `for_any`, element for element.
///
/// An unobserved collection is `Unknown`, never the vacuous truth an empty one gives: *nobody
/// looked* and *there was nothing to look at* are different. Evaluation stops as soon as the fold
/// is settled, which is not an optimisation — walking a collection whose verdict is already settled
/// would report causes from elements nobody is waiting on.
fn quantify(
    quantifier: &Quantifier,
    context: &TemplateContext<'_>,
    bindings: Option<&Bindings<'_>>,
    unobserved: &mut Unobserved,
    mode: Quantification,
) -> Result<Truth, CoreError> {
    let Some(collection) = resolve_operand(&quantifier.over, context, bindings, unobserved)? else {
        return Ok(Truth::Unknown);
    };
    // An array's elements in index order; a map's **values** in canonical key order, which is the
    // order this runtime canonicalizes an object into everywhere else. Map keys are not
    // addressable, so a body cannot reach for one.
    let elements: Vec<&Value> = match &collection {
        Value::Array(values) => values.iter().collect(),
        Value::Object(members) => members.values().collect(),
        _ => return Ok(Truth::Unknown),
    };
    let mut result = match mode {
        Quantification::All => Truth::True,
        Quantification::Any => Truth::False,
    };
    for element in elements {
        let inner = Bindings {
            name: &quantifier.bind,
            value: element,
            outer: bindings,
        };
        let answer = evaluate_condition(&quantifier.body, context, Some(&inner), unobserved)?;
        result = match mode {
            Quantification::All => result.and(answer),
            Quantification::Any => result.or(answer),
        };
        let settled = match mode {
            Quantification::All => result == Truth::False,
            Quantification::Any => result == Truth::True,
        };
        if settled {
            break;
        }
    }
    Ok(result)
}

/// One quantifier binder, and every binder enclosing it.
///
/// A binder **extends** whichever scope encloses it, and only for the duration of the body: every
/// other address passes through untouched, which is what lets a body mix element facts with free
/// ones. A nested `as` equal to an enclosing one is admitted and the inner one wins, because that
/// is what the source's own rewrite does and refusing it would be narrower than the source.
pub(crate) struct Bindings<'a> {
    name: &'a str,
    value: &'a Value,
    outer: Option<&'a Bindings<'a>>,
}

impl<'a> Bindings<'a> {
    /// The element bound to `name`, innermost first.
    fn get(&self, name: &str) -> Option<&'a Value> {
        if self.name == name {
            return Some(self.value);
        }
        self.outer?.get(name)
    }
}

/// `compare`: the exact three-valued scalar comparison, row for row.
fn evaluate_compare(
    comparison: &Comparison,
    context: &TemplateContext<'_>,
    bindings: Option<&Bindings<'_>>,
    unobserved: &mut Unobserved,
) -> Result<Truth, CoreError> {
    let left = resolve_operand(&comparison.left, context, bindings, unobserved)?;
    let right = resolve_operand(&comparison.right, context, bindings, unobserved)?;
    // Either operand resolving to nothing — absent, or present and null — is unevaluable.
    let (Some(left), Some(right)) = (left, right) else {
        return Ok(Truth::Unknown);
    };
    let op = comparison.op;
    Ok(match (&left, &right) {
        (Value::Number(left), Value::Number(right)) => {
            // Each operand through its own door: a number reached through a reference is a wire
            // number, an authored numeric literal is a literal. Reading both through the wire door
            // would erase the distinction the source draws.
            let left = observe(left, &comparison.left);
            let right = observe(right, &comparison.right);
            match (left, right) {
                (Some(left), Some(right)) => Truth::from_bool(op.accepts(left.cmp(right))),
                // A token the source cannot observe answers `Unknown`, and never panics.
                _ => Truth::Unknown,
            }
        }
        (Value::String(left), Value::String(right)) if op.is_ordering() => {
            match scale_compare(&context.definition.scales, left, right) {
                Some(order) => Truth::from_bool(op.accepts(order)),
                // No declared scale contains both values, or two disagree.
                None => Truth::Unknown,
            }
        }
        (Value::String(left), Value::String(right)) => {
            Truth::from_bool(op.accepts(left.cmp(right)))
        }
        (Value::Bool(left), Value::Bool(right)) if !op.is_ordering() => {
            Truth::from_bool(op.accepts(left.cmp(right)))
        }
        // A list, a map, a union and a struct have no scalar spelling, so this is unevaluable by
        // construction — and cross-type ordering, and same-type ordering where none exists, with it.
        (left, right) if !is_scalar(left) || !is_scalar(right) => Truth::Unknown,
        _ if op.is_ordering() => Truth::Unknown,
        // Two different scalar kinds are never equal.
        _ => Truth::from_bool(op == CompareOp::Ne),
    })
}

/// Which door a numeric operand is read through: an authored literal is a literal, and anything
/// reached through a reference is a wire number.
fn observe(value: &serde_json::Number, written: &Value) -> Option<Observed> {
    match written {
        Value::Number(literal) => Observed::of_literal(&literal.to_string()),
        _ => Observed::of_number(value),
    }
}

fn is_scalar(value: &Value) -> bool {
    matches!(value, Value::Bool(_) | Value::Number(_) | Value::String(_))
}

/// The ordering one declared scale gives two text values, or nothing.
///
/// Scans every declared scale for one containing both values and answers `None` when none does
/// **or when two disagree**. A specification that declares no scale is in the `None` case for every
/// pair, so every `<`, `<=`, `>` and `>=` between two text values is `Unknown` — the absence of a
/// scale is a value of the context, not a missing feature, and never `false`.
#[must_use]
pub fn scale_compare(
    scales: &BTreeMap<String, Vec<String>>,
    left: &str,
    right: &str,
) -> Option<Ordering> {
    let mut answer: Option<Ordering> = None;
    for values in scales.values() {
        let (Some(left), Some(right)) = (
            values.iter().position(|value| value == left),
            values.iter().position(|value| value == right),
        ) else {
            continue;
        };
        let order = left.cmp(&right);
        match answer {
            None => answer = Some(order),
            Some(previous) if previous == order => {}
            // Two scales disagree about this pair, so the ordering is not this context's to give.
            Some(_) => return None,
        }
    }
    answer
}

/// Equality that agrees with the ordering operators.
///
/// `serde_json`'s own `==` compares a number's *representation*, so `100` and `100.0` are unequal
/// — while `gte`/`lte` compare through `f64` and call them equal. A definition tested with integer
/// fixtures would then refuse the same document written with a decimal point. Numbers here compare
/// numerically, at every depth; everything else compares structurally.
fn values_equal(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Number(left), Value::Number(right)) => crate::number::compare(left, right).is_eq(),
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

/// The same equality with its numeric leaves read under the source observation rule.
///
/// Two tokens the source cannot observe fall back to comparing the tokens exactly, because
/// answering *equal* about a pair nothing can read would be an observation nobody made.
fn observed_values_equal(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Number(left), Value::Number(right)) => {
            match (Observed::of_number(left), Observed::of_number(right)) {
                (Some(left), Some(right)) => left.cmp(right).is_eq(),
                _ => left == right,
            }
        }
        (Value::Array(left), Value::Array(right)) => {
            left.len() == right.len()
                && left
                    .iter()
                    .zip(right.iter())
                    .all(|(left, right)| observed_values_equal(left, right))
        }
        (Value::Object(left), Value::Object(right)) => {
            left.len() == right.len()
                && left.iter().all(|(key, left)| {
                    right
                        .get(key)
                        .is_some_and(|right| observed_values_equal(left, right))
                })
        }
        _ => left == right,
    }
}

fn compare_values(
    pair: &[Value; 2],
    context: &TemplateContext<'_>,
    bindings: Option<&Bindings<'_>>,
    unobserved: &mut Unobserved,
    predicate: impl FnOnce(&Value, &Value) -> bool,
) -> Result<Truth, CoreError> {
    let (left, right) = resolve_pair(pair, context, bindings, unobserved)?;
    match (left, right) {
        (Some(left), Some(right)) => Ok(Truth::from_bool(predicate(&left, &right))),
        _ => Ok(Truth::Unknown),
    }
}

/// Orders two instants, where anything this kernel cannot read is `Unknown` rather than `false`.
///
/// Unlike [`compare_numbers`], a value that is present and unreadable does **not** answer `false`.
/// *These are not numbers* is an observation; *this is not a timestamp I can read* is a statement
/// about the reader, and answering `false` would let a gate quietly permit a move against a value
/// nobody understood. See `crate::timestamp` for what is read and what is refused.
fn compare_instants(
    pair: &[Value; 2],
    context: &TemplateContext<'_>,
    bindings: Option<&Bindings<'_>>,
    unobserved: &mut Unobserved,
    predicate: impl FnOnce(crate::timestamp::Timestamp, crate::timestamp::Timestamp) -> bool,
) -> Result<Truth, CoreError> {
    let (left, right) = resolve_pair(pair, context, bindings, unobserved)?;
    let (Some(left), Some(right)) = (left, right) else {
        return Ok(Truth::Unknown);
    };
    let mut read = |value: &Value, written: &Value| {
        let instant = value.as_str().and_then(crate::timestamp::parse);
        if instant.is_none() {
            if let Value::String(expression) = written {
                if expression.starts_with('$') {
                    unobserved.insert(expression.clone());
                }
            }
        }
        instant
    };
    let left = read(&left, &pair[0]);
    let right = read(&right, &pair[1]);
    match (left, right) {
        (Some(left), Some(right)) => Ok(Truth::from_bool(predicate(left, right))),
        _ => Ok(Truth::Unknown),
    }
}

fn compare_numbers(
    pair: &[Value; 2],
    context: &TemplateContext<'_>,
    bindings: Option<&Bindings<'_>>,
    unobserved: &mut Unobserved,
    predicate: impl FnOnce(Ordering) -> bool,
) -> Result<Truth, CoreError> {
    let (left, right) = resolve_pair(pair, context, bindings, unobserved)?;
    match (left, right) {
        // Both resolved. Not being numbers is an observation about them, not an absence.
        (Some(left), Some(right)) => match (left.as_number(), right.as_number()) {
            (Some(left), Some(right)) => Ok(Truth::from_bool(predicate(crate::number::compare(
                left, right,
            )))),
            _ => Ok(Truth::False),
        },
        _ => Ok(Truth::Unknown),
    }
}

fn resolve_pair(
    pair: &[Value; 2],
    context: &TemplateContext<'_>,
    bindings: Option<&Bindings<'_>>,
    unobserved: &mut Unobserved,
) -> Result<(Option<Value>, Option<Value>), CoreError> {
    // Both sides, always: the left one failing to resolve must not hide the right one's address.
    let left = resolve_operand(&pair[0], context, bindings, unobserved)?;
    let right = resolve_operand(&pair[1], context, bindings, unobserved)?;
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
    bindings: Option<&Bindings<'_>>,
    unobserved: &mut Unobserved,
) -> Result<Option<Value>, CoreError> {
    match value {
        Value::String(literal) if literal.starts_with("$$") => {
            Ok(Some(Value::String(literal[1..].to_owned())))
        }
        Value::String(expression) if expression.starts_with('$') => {
            match resolve_expression_optional(expression, context, bindings)? {
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
                match resolve_operand(value, context, bindings, unobserved)? {
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
                match resolve_operand(value, context, bindings, unobserved)? {
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
    args: &Map<String, Value>,
) -> Result<DomainEvent, CoreError> {
    Ok(DomainEvent {
        entity: context.definition.entity.clone(),
        version: context.definition.version,
        id: context.id.to_owned(),
        revision,
        event_type: definition.event_type.clone(),
        from_state: context.from_state.map(ToOwned::to_owned),
        to_state: context.to_state.to_owned(),
        changed: changed_fields(context),
        args: args.clone(),
        payload: canonicalize(resolve_template(&definition.payload, context)?),
    })
}

/// The fields this operation wrote: every one whose value differs from before it ran.
///
/// A creation has no "before", so every field it set is written. Derived from the two field maps
/// rather than from the `set:` block, so a field an invariant or a default settled is recorded too
/// — the record is what the instance *became*, not what the author remembered to list.
pub(crate) fn changed_fields(context: &TemplateContext<'_>) -> Map<String, Value> {
    context
        .new_fields
        .iter()
        .filter(|(name, value)| context.old_fields.get(*name) != Some(*value))
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect()
}

pub(crate) struct TemplateContext<'a> {
    pub(crate) definition: &'a EntityDefinition,
    pub(crate) id: &'a str,
    pub(crate) args: &'a Map<String, Value>,
    /// The schema the arguments were validated against, so a `service/1` address into an argument
    /// collection is resolved by the kind the schema declares. `None` where there are none.
    pub(crate) arguments_schema: Option<&'a ObjectSchema>,
    pub(crate) old_fields: &'a Map<String, Value>,
    pub(crate) new_fields: &'a Map<String, Value>,
    pub(crate) from_state: Option<&'a str>,
    pub(crate) to_state: &'a str,
}

pub(crate) fn resolve_template(
    value: &Value,
    context: &TemplateContext<'_>,
) -> Result<Value, CoreError> {
    match value {
        Value::String(literal) if literal.starts_with("$$") => {
            Ok(Value::String(literal[1..].to_owned()))
        }
        Value::String(expression) if expression.starts_with('$') => {
            match resolve_expression_optional(expression, context, None)? {
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
    bindings: Option<&Bindings<'_>>,
) -> Result<Option<Value>, CoreError> {
    // A binder extends whichever scope encloses it, for the duration of its body only: `$<bind>` is
    // the element and `$<bind>.<path>` is a path into it, while every address whose first segment is
    // not a binder passes through untouched. An inner binder wins over an outer one of the same
    // name, because that is what the source's own rewrite does.
    if let Some(bindings) = bindings {
        let (name, path) = match expression[1..].split_once('.') {
            Some((name, path)) => (name, Some(path)),
            None => (&expression[1..], None),
        };
        if let Some(element) = bindings.get(name) {
            return Ok(match path {
                None => Some(element.clone()),
                Some(path) => walk(element, path, None, context.service()),
            });
        }
    }

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
            let service = context.service();
            for (prefix, map, schema) in [
                ("$args.", context.args, context.arguments_schema),
                (
                    "$fields.",
                    context.new_fields,
                    Some(&context.definition.schema),
                ),
                (
                    "$old_fields.",
                    context.old_fields,
                    Some(&context.definition.schema),
                ),
            ] {
                if let Some(path) = expression.strip_prefix(prefix) {
                    // Walk by reference and clone the leaf: a `$fields.x` reference used to copy
                    // every field of the instance to read one of them.
                    return Ok(lookup(map, path, schema, service));
                }
            }

            Err(template_error(expression, "unknown template expression"))
        }
    }
}

impl TemplateContext<'_> {
    fn service(&self) -> bool {
        self.definition.semantics.is_service_1()
    }
}

/// Resolves one path into a root map.
///
/// Under `kernel/1` this walks objects only, so `<collection>.count` and `<array>.<n>` resolve to
/// nothing and every existing definition keeps the answer it has. Under `service/1` those two
/// address forms resolve, and a declared `map`'s **keys** stay unaddressable: the only `count` a
/// map has is its size, so `{metadata: {count: "7"}}` cannot be read as its own `count` key.
fn lookup(
    root: &Map<String, Value>,
    path: &str,
    schema: Option<&ObjectSchema>,
    service: bool,
) -> Option<Value> {
    let mut segments = path.splitn(2, '.');
    let first = segments.next()?;
    if first.is_empty() {
        return None;
    }
    let value = root.get(first)?;
    let field = schema.and_then(|schema| schema.fields.get(first));
    match segments.next() {
        None => Some(value.clone()),
        Some(rest) => walk(value, rest, field, service),
    }
}

/// Walks the rest of a path from one value, with the declared field it came from where there is one.
///
/// Recursive rather than iterative because a `service/1` collection address produces a **new**
/// value — a count is a number nothing held — and only the leaf is cloned either way.
fn walk(
    value: &Value,
    path: &str,
    field: Option<&crate::FieldDefinition>,
    service: bool,
) -> Option<Value> {
    let mut segments = path.splitn(2, '.');
    let segment = segments.next()?;
    let rest = segments.next();

    if service {
        if let Some((next, next_field)) = collection_address(value, field, segment) {
            return match rest {
                None => Some(next),
                Some(rest) => walk(&next, rest, next_field, service),
            };
        }
        // A declared map's keys are not addressable, so nothing but its size resolves.
        if field.is_some_and(|field| field.kind == FieldKind::Map) {
            return None;
        }
    }

    let next = value.as_object()?.get(segment)?;
    let next_field = field.and_then(|field| match field.kind {
        FieldKind::Object => field.properties.get(segment),
        // A union's payload sits under the derived content key and its type depends on the tag, so
        // the walk continues without a declared field rather than claiming one variant's.
        FieldKind::Union => field.variants.get(segment),
        _ => None,
    });
    match rest {
        None => Some(next.clone()),
        Some(rest) => walk(next, rest, next_field, service),
    }
}

/// The two `service/1` collection address forms, with the field the walk continues under, or
/// `None` where neither applies.
fn collection_address<'a>(
    value: &Value,
    field: Option<&'a crate::FieldDefinition>,
    segment: &str,
) -> Option<(Value, Option<&'a crate::FieldDefinition>)> {
    let declared = field.map(|field| field.kind);
    match value {
        // A declared map's size, and nothing else it holds.
        Value::Object(members) if declared == Some(FieldKind::Map) && segment == "count" => {
            Some((Value::from(members.len()), None))
        }
        Value::Array(values) if segment == "count" => Some((Value::from(values.len()), None)),
        // `0`, or a digit string with no leading zero.
        Value::Array(values) => {
            if segment != "0" && segment.starts_with('0') {
                return None;
            }
            let index: usize = segment.parse().ok()?;
            let element = values.get(index)?.clone();
            Some((element, field.and_then(|field| field.items.as_deref())))
        }
        _ => None,
    }
}

fn template_error(expression: &str, message: &str) -> CoreError {
    CoreError::Template {
        expression: expression.to_owned(),
        message: message.to_owned(),
    }
}

fn into_object(value: Value, path: &str) -> Result<Map<String, Value>, CoreError> {
    match value {
        Value::Object(object) => Ok(canonical_object(object)),
        _ => Err(CoreError::Validation(vec![crate::ValidationError::new(
            path,
            "expected object",
        )])),
    }
}

pub(crate) fn canonical_object(object: Map<String, Value>) -> Map<String, Value> {
    let ordered: std::collections::BTreeMap<_, _> = object
        .into_iter()
        .map(|(key, value)| (key, canonicalize(value)))
        .collect();
    ordered.into_iter().collect()
}

pub(crate) fn canonicalize(value: Value) -> Value {
    match value {
        Value::Object(object) => Value::Object(canonical_object(object)),
        Value::Array(values) => Value::Array(values.into_iter().map(canonicalize).collect()),
        other => other,
    }
}
