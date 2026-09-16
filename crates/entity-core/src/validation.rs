//! Definition validation (at registration) and value validation (at create/execute).
//!
//! Registration is where a defect that could never work is caught: an undeclared state, a
//! constraint on a kind it does not apply to, a rule or template reading something its scope
//! cannot see — at any depth of the schema, not only at the root. What is left for run time is
//! only what run time knows: whether a value satisfies its field, and whether a path into a
//! `json` field or an open schema happens to resolve.

use crate::{
    observed::Observed, Cardinality, Condition, DefinitionError, DefinitionErrors,
    EntityDefinition, EventDefinition, FieldDefinition, FieldKind, MapKey, ObjectSchema,
    OutcomeDefinition, OutcomeEffect, RelationKind, RuleDefinition, Semantics, ValidationError,
    MAX_CONDITION_DEPTH, SERVICE_CONDITION_OPERATORS,
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
    let mut defects = Defects::default();
    let semantics = definition.semantics;
    let service = semantics.is_service_1();

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

    // What only a `service/1` definition may carry. Refused rather than ignored: an author who
    // writes a branch believes something selects it.
    if !service {
        for (path, key, present) in [
            ("identity", "identity", definition.identity.is_some()),
            ("relations", "relations", !definition.relations.is_empty()),
            ("scales", "scales", !definition.scales.is_empty()),
            (
                "create.arguments",
                "arguments",
                !definition.create.arguments.is_empty(),
            ),
            (
                "create.response",
                "response",
                !definition.create.response.is_empty(),
            ),
            (
                "create.outcomes",
                "outcomes",
                !definition.create.outcomes.is_empty(),
            ),
        ] {
            if present {
                defects.push(DefinitionError::SemanticsKeyNotAvailable {
                    path: path.to_owned(),
                    key: key.to_owned(),
                });
            }
        }
        for (name, operation) in &definition.operations {
            if !operation.response.is_empty() {
                defects.push(DefinitionError::SemanticsKeyNotAvailable {
                    path: format!("operations.{name}.response"),
                    key: "response".to_owned(),
                });
            }
            if !operation.outcomes.is_empty() {
                defects.push(DefinitionError::SemanticsKeyNotAvailable {
                    path: format!("operations.{name}.outcomes"),
                    key: "outcomes".to_owned(),
                });
            }
        }
    }

    defects.extend(validate_schema_definition(
        &definition.schema,
        "schema",
        semantics,
    ));
    defects.extend(validate_scales(definition));
    defects.extend(validate_identity(definition));
    defects.extend(validate_relations(definition));

    let invariant_scope = Scope {
        kind: ScopeKind::Invariant,
        fields: &definition.schema,
        args: None,
        binders: None,
        semantics,
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
                    kind: ScopeKind::Invariant,
                    fields: &definition.schema,
                    args: None,
                    binders: None,
                    semantics,
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

    // The `kernel/1` spelling, unchanged: its scope admits no `$args` at all, and `create` builds
    // its context with none.
    if let Some(event) = &definition.create.emit {
        defects.check(validate_event_definition(
            event,
            "create.emit",
            None,
            Scope {
                kind: ScopeKind::CreateTemplate,
                fields: &definition.schema,
                args: None,
                binders: None,
                semantics,
            },
        ));
    }

    if service {
        defects.extend(validate_schema_definition(
            &definition.create.arguments,
            "create.arguments",
            semantics,
        ));
        defects.extend(validate_schema_definition(
            &definition.create.response,
            "create.response",
            semantics,
        ));
        defects.extend(validate_outcomes(definition, None, "create"));

        let create_selector = Scope {
            kind: ScopeKind::CreateSelector,
            fields: &definition.schema,
            args: Some(&definition.create.arguments),
            binders: None,
            semantics,
        };
        // `set` reads `CreateSet` — no `$fields`, because at creation the fields are what it is
        // producing — while `emits` and `responds` read `CreateOutcomeTemplate`, which adds them
        // back because both are materialised after the branch's `set`.
        let create_set = Scope {
            kind: ScopeKind::CreateSet,
            ..create_selector
        };
        let create_template = Scope {
            kind: ScopeKind::CreateOutcomeTemplate,
            ..create_selector
        };
        for outcome in &definition.create.outcomes {
            let path = format!("create.outcomes.{}", outcome.name);
            if let Some(when) = &outcome.when {
                defects.check(validate_condition_definition(
                    when,
                    &format!("{path}.when"),
                    create_selector,
                ));
            }
            for (field, value) in &outcome.set {
                if !definition.schema.additional_fields
                    && !definition.schema.fields.contains_key(field)
                {
                    defects.push(DefinitionError::UnknownSetField {
                        operation: "create".to_owned(),
                        field: field.clone(),
                    });
                }
                defects.check(validate_template(
                    value,
                    &format!("{path}.set.{field}"),
                    create_set,
                ));
            }
            for (index, event) in outcome.emits.iter().enumerate() {
                defects.check(validate_event_definition(
                    event,
                    &format!("{path}.emits[{index}]"),
                    None,
                    create_template,
                ));
            }
            for (field, value) in &outcome.responds {
                if !definition.create.response.fields.contains_key(field)
                    && !definition.create.response.additional_fields
                {
                    defects.push(DefinitionError::ResponseFieldUnknown {
                        command: "create".to_owned(),
                        outcome: outcome.name.clone(),
                        field: field.clone(),
                    });
                }
                defects.check(validate_template(
                    value,
                    &format!("{path}.responds.{field}"),
                    create_template,
                ));
            }
            if outcome.refuses.is_none() {
                for (field, declared) in &definition.create.response.fields {
                    if declared.required && !outcome.responds.contains_key(field) {
                        defects.push(DefinitionError::OutcomeResponseIncomplete {
                            command: "create".to_owned(),
                            outcome: outcome.name.clone(),
                            field: field.clone(),
                        });
                    }
                }
            }
        }
    }

    for (operation_name, operation) in &definition.operations {
        if operation_name.trim().is_empty() {
            defects.push(DefinitionError::EmptyOperationName);
        }
        // Still fires for every `kernel/1` operation, and for a `service/1` operation that declares
        // neither transitions nor outcomes.
        if operation.transitions.is_empty() && !(service && !operation.outcomes.is_empty()) {
            defects.push(DefinitionError::NoTransitions {
                operation: operation_name.clone(),
            });
        }

        defects.extend(validate_schema_definition(
            &operation.arguments,
            &format!("operations.{operation_name}.arguments"),
            semantics,
        ));
        if service {
            defects.extend(validate_schema_definition(
                &operation.response,
                &format!("operations.{operation_name}.response"),
                semantics,
            ));
            defects.extend(validate_outcomes(
                definition,
                Some(operation),
                operation_name,
            ));
        }

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
            binders: None,
            semantics,
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
            binders: None,
            semantics,
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

        // A `service/1` operation branch: its selector in `OutcomeSelector`, its `set`, `emits` and
        // `responds` in the operation template scope.
        let selector_scope = Scope {
            kind: ScopeKind::OutcomeSelector,
            fields: &definition.schema,
            args: Some(&operation.arguments),
            binders: None,
            semantics,
        };
        for outcome in &operation.outcomes {
            let path = format!("operations.{operation_name}.outcomes.{}", outcome.name);
            defects.extend(validate_outcome_expressions(
                definition,
                outcome,
                &path,
                operation_name,
                selector_scope,
                template_scope,
                &operation.response,
            ));
        }
    }

    defects.into_result()
}

/// Every reference a branch carries, each in the scope its own step resolves it in.
fn validate_outcome_expressions(
    definition: &EntityDefinition,
    outcome: &OutcomeDefinition,
    path: &str,
    command: &str,
    selector: Scope<'_>,
    template: Scope<'_>,
    response: &ObjectSchema,
) -> Vec<DefinitionError> {
    let mut defects = Vec::new();
    if let Some(when) = &outcome.when {
        if let Err(defect) = validate_condition_definition(when, &format!("{path}.when"), selector)
        {
            defects.push(defect);
        }
    }
    for (field, value) in &outcome.set {
        if !definition.schema.additional_fields && !definition.schema.fields.contains_key(field) {
            defects.push(DefinitionError::UnknownSetField {
                operation: command.to_owned(),
                field: field.clone(),
            });
        }
        if let Err(defect) = validate_template(value, &format!("{path}.set.{field}"), template) {
            defects.push(defect);
        }
    }
    for (index, event) in outcome.emits.iter().enumerate() {
        if let Err(defect) = validate_event_definition(
            event,
            &format!("{path}.emits[{index}]"),
            Some(&command.to_owned()),
            template,
        ) {
            defects.push(defect);
        }
    }
    for (field, value) in &outcome.responds {
        if !response.fields.contains_key(field) && !response.additional_fields {
            defects.push(DefinitionError::ResponseFieldUnknown {
                command: command.to_owned(),
                outcome: outcome.name.clone(),
                field: field.clone(),
            });
        }
        if let Err(defect) = validate_template(value, &format!("{path}.responds.{field}"), template)
        {
            defects.push(defect);
        }
    }
    // Every accepting branch determines every required response field.
    if outcome.refuses.is_none() {
        for (field, declared) in &response.fields {
            if declared.required && !outcome.responds.contains_key(field) {
                defects.push(DefinitionError::OutcomeResponseIncomplete {
                    command: command.to_owned(),
                    outcome: outcome.name.clone(),
                    field: field.clone(),
                });
            }
        }
    }
    defects
}

/// The shape of one command's branches: names, defaults, wrong-state constructs, effects and the
/// states they name.
///
/// `operation` is `None` for the creation, which has no held state, no move and no wrong state.
fn validate_outcomes(
    definition: &EntityDefinition,
    operation: Option<&crate::OperationDefinition>,
    command: &str,
) -> Vec<DefinitionError> {
    let outcomes: &[OutcomeDefinition] = match operation {
        Some(operation) => &operation.outcomes,
        None => &definition.create.outcomes,
    };
    let mut defects = Vec::new();
    let states: BTreeSet<&str> = definition
        .lifecycle
        .states
        .iter()
        .map(String::as_str)
        .collect();
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    let mut defaults = 0_usize;
    let mut wrong_states = 0_usize;
    let mut state_guards = 0_usize;
    let last_selecting = outcomes
        .iter()
        .rposition(|outcome| !outcome.wrong_state)
        .unwrap_or(0);

    for (index, outcome) in outcomes.iter().enumerate() {
        if outcome.name.trim().is_empty() {
            defects.push(DefinitionError::EmptyOutcomeName {
                command: command.to_owned(),
            });
        } else if !seen.insert(outcome.name.as_str()) {
            defects.push(DefinitionError::DuplicateOutcome {
                command: command.to_owned(),
                outcome: outcome.name.clone(),
            });
        }

        if outcome.in_state.is_some() {
            state_guards += 1;
        }
        if outcome.is_default_branch() {
            defaults += 1;
            // The source's default is *taken when no conditional outcome matched*, a
            // position-independent meaning; declared order is what this runtime selects by, so last
            // is the only declaration that spells it. A bare state guard is not a second default
            // and does not have to be last.
            if defaults > 1 || index != last_selecting {
                defects.push(DefinitionError::AmbiguousDefaultOutcome {
                    command: command.to_owned(),
                });
            }
        }

        if outcome.wrong_state {
            wrong_states += 1;
            if operation.is_none() {
                defects.push(DefinitionError::WrongStateOnCreate {
                    outcome: outcome.name.clone(),
                });
            } else {
                if wrong_states > 1 {
                    defects.push(DefinitionError::DuplicateWrongStateOutcome {
                        operation: command.to_owned(),
                    });
                }
                if outcome.when.is_some() || outcome.in_state.is_some() {
                    defects.push(DefinitionError::WrongStateWithSelector {
                        operation: command.to_owned(),
                        outcome: outcome.name.clone(),
                    });
                }
            }
        }

        // Effects, and which command may carry which.
        match (&outcome.effect, operation) {
            (OutcomeEffect::Creates, Some(_)) => {
                defects.push(DefinitionError::CreatesEffectOnOperation {
                    operation: command.to_owned(),
                    outcome: outcome.name.clone(),
                });
            }
            (OutcomeEffect::Creates | OutcomeEffect::None, None) => {}
            (effect, None) => defects.push(DefinitionError::MissingCreatesEffect {
                outcome: outcome.name.clone(),
                effect: effect.as_str(),
            }),
            _ => {}
        }

        let mut named: Vec<&str> = outcome.in_state.iter().map(String::as_str).collect();
        if let OutcomeEffect::Moves { to, from } = &outcome.effect {
            named.push(to.as_str());
            named.extend(from.iter().map(String::as_str));
        }
        for state in named {
            if !states.contains(state) {
                defects.push(DefinitionError::UnknownOutcomeState {
                    command: command.to_owned(),
                    outcome: outcome.name.clone(),
                    state: state.to_owned(),
                });
            }
        }

        // A state-guarded move starts where it is guarded, or the guard names a state the branch
        // could never move from.
        if let (Some(guarded), OutcomeEffect::Moves { from, .. }) =
            (outcome.in_state.as_deref(), &outcome.effect)
        {
            if !from.iter().any(|state| state == guarded) {
                defects.push(DefinitionError::GuardStateOutsideMove {
                    operation: command.to_owned(),
                    outcome: outcome.name.clone(),
                    state: guarded.to_owned(),
                });
            }
        }

        if outcome.refuses.is_some()
            && (!outcome.effect.is_none()
                || !outcome.set.is_empty()
                || !outcome.emits.is_empty()
                || !outcome.responds.is_empty())
        {
            defects.push(DefinitionError::RefusalMutatesState {
                command: command.to_owned(),
                outcome: outcome.name.clone(),
            });
        }

        // Weaker than the source's own rule, and the weakening is required rather than convenient:
        // this runtime admits a creation that emits nothing at all, and `kernel/1` behaviour does
        // not move. An accepting wrong-state branch is exempt, which is the source's own exemption.
        if !outcome.is_observable() && !outcome.wrong_state {
            defects.push(DefinitionError::UnobservableOutcome {
                command: command.to_owned(),
                outcome: outcome.name.clone(),
            });
        }
    }

    if let Some(operation) = operation {
        if wrong_states > 0 {
            if state_guards > 0 {
                defects.push(DefinitionError::WrongStateWithStateGuard {
                    operation: command.to_owned(),
                });
            }
            if crate::runtime::wrong_states(definition, operation).is_empty() {
                defects.push(DefinitionError::WrongStateUnreachable {
                    operation: command.to_owned(),
                });
            }
        }
    }
    defects
}

/// The declared value scales text comparison is answered inside.
fn validate_scales(definition: &EntityDefinition) -> Vec<DefinitionError> {
    let mut defects = Vec::new();
    for (name, values) in &definition.scales {
        if name.trim().is_empty() {
            defects.push(DefinitionError::ScaleUnnamed);
        }
        if values.is_empty() {
            defects.push(DefinitionError::ScaleEmpty {
                scale: name.clone(),
            });
        }
    }
    defects
}

/// The declared logical identity: a required field of a kind the address function admits.
///
/// The source admits an `Optional` identity type and its own conformance refuses to instantiate an
/// entity whose identity is null, so a lowering emits a **required** field of the unwrapped kind
/// and an optional one is refused here.
fn validate_identity(definition: &EntityDefinition) -> Vec<DefinitionError> {
    let Some(identity) = &definition.identity else {
        return Vec::new();
    };
    match definition.schema.fields.get(&identity.field) {
        None => vec![DefinitionError::IdentityFieldUnknown {
            field: identity.field.clone(),
        }],
        Some(field) if !field.required => vec![DefinitionError::IdentityFieldUnknown {
            field: identity.field.clone(),
        }],
        Some(field) if field.kind == FieldKind::Json => {
            vec![DefinitionError::IdentityFieldNotAddressable {
                field: identity.field.clone(),
                kind: field.kind.as_str(),
            }]
        }
        Some(_) => Vec::new(),
    }
}

/// The half of a relation one definition can answer: a `references` carrier is declared here, its
/// `many` row is a list, and each row carries the optionality its row admits.
///
/// **The carrier's kind is not one of these**, on either row. § 8.1 types every carrier by the
/// *related* entity's identity field, and one definition does not hold the other, so every kind
/// comparison is [`Registry::validate_all`](crate::Registry::validate_all)'s — as is the whole
/// `owns` half, where the carrying document is not even the one the author is reading.
fn validate_relations(definition: &EntityDefinition) -> Vec<DefinitionError> {
    let mut defects = Vec::new();
    for (name, relation) in &definition.relations {
        if relation.kind != RelationKind::References {
            continue;
        }
        // Which relation claims a field is a **set**-level question, because an `owns` relation
        // declared elsewhere claims a field of this definition; `Registry::validate_all` asks it.
        let Some(field) = definition.schema.fields.get(&relation.via) else {
            defects.push(DefinitionError::RelationViaUnknown {
                relation: name.clone(),
                via: relation.via.clone(),
            });
            continue;
        };
        // The `one` row admits `target.identity.type_ref` or `Optional<it>`
        // (`ESS/crates/specify/ess-domain/src/entity.rs:1399-1402`), so both optionalities are
        // admitted — this is the only row the source offers an optional carrier for — and the kind
        // is whatever shape the target's identity has, including a list. **Nothing here is
        // answerable about that kind and nothing is guessed:** an earlier draft refused every array
        // carrier on this row, which refused exactly the source-admitted case of a list identity,
        // and it could not have known better, because one definition does not hold the target. The
        // whole question is `Registry::validate_all`'s.
        if relation.cardinality == Cardinality::One {
            continue;
        }

        // The `many` row is `List<target identity type>` with no optional offered. The outer list
        // is the cardinality and is answerable here; its element is the target's identity shape and
        // is not.
        if field.kind == FieldKind::Array {
            if !field.required {
                defects.push(DefinitionError::RelationCarrierOptionality {
                    relation: name.clone(),
                    via: relation.via.clone(),
                });
            }
        } else {
            defects.push(DefinitionError::RelationViaWrongShape {
                relation: name.clone(),
                via: relation.via.clone(),
                expected: "an array of the target's identity kind".to_owned(),
                found: format!("a {} field", field.kind),
            });
        }
    }
    defects
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
    /// An operation branch's input guard, at step 4.
    OutcomeSelector,
    /// A creation branch's input guard, at step 4.
    CreateSelector,
    /// A creation branch's `set`, at step 8.
    CreateSet,
    /// A creation branch's `emits` payloads and its `responds` map, at steps 13 and 14.
    CreateOutcomeTemplate,
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
            Self::OutcomeSelector => {
                "$id, $entity, $version, $from_state, $args, $args.<path>, $fields, $fields.<path>"
            }
            Self::CreateSelector => "$id, $entity, $version, $args, $args.<path>",
            Self::CreateSet => "$id, $entity, $version, $state, $to_state, $args, $args.<path>",
            Self::CreateOutcomeTemplate => {
                "$id, $entity, $version, $state, $to_state, $args, $args.<path>, $fields, \
                 $fields.<path>"
            }
        }
    }

    fn is_rule(self) -> bool {
        matches!(
            self,
            Self::Invariant | Self::Precondition | Self::OutcomeSelector | Self::CreateSelector
        )
    }

    fn what(self) -> &'static str {
        match self {
            Self::Invariant => "an entity invariant",
            Self::Precondition => "an operation precondition",
            Self::CreateTemplate => "a creation event payload",
            Self::OperationTemplate => "an operation template",
            Self::OutcomeSelector => "an outcome selector",
            Self::CreateSelector => "a creation outcome selector",
            Self::CreateSet => "a creation outcome assignment",
            Self::CreateOutcomeTemplate => "a creation outcome template",
        }
    }
}

/// One quantifier binder, and every binder enclosing it, as registration sees them.
///
/// The element definition is `None` where the collection's element kind is not declared — inside a
/// `json` field, or under a schema that admits additional members — which is the same position a
/// path into one of those is already in.
#[derive(Clone, Copy)]
struct BinderScope<'a> {
    name: &'a str,
    element: Option<&'a FieldDefinition>,
    outer: Option<&'a BinderScope<'a>>,
}

impl<'a> BinderScope<'a> {
    fn get(&self, name: &str) -> Option<(bool, Option<&'a FieldDefinition>)> {
        if self.name == name {
            return Some((true, self.element));
        }
        self.outer?.get(name)
    }
}

#[derive(Clone, Copy)]
struct Scope<'a> {
    kind: ScopeKind,
    fields: &'a ObjectSchema,
    args: Option<&'a ObjectSchema>,
    binders: Option<&'a BinderScope<'a>>,
    semantics: Semantics,
}

impl Scope<'_> {
    /// Whether a bare reference (no path) is available here.
    fn allows(&self, expression: &str) -> bool {
        match expression {
            "$id" | "$entity" | "$version" => true,
            // At creation the fields are what `set` is producing, and at selection there is no
            // instance at all.
            "$fields" => !matches!(self.kind, ScopeKind::CreateSelector | ScopeKind::CreateSet),
            // At selection the destination state is what the selected branch's effect produces, so
            // a selector reading it would read a value the selection is deciding.
            "$state" => !matches!(
                self.kind,
                ScopeKind::Precondition | ScopeKind::OutcomeSelector | ScopeKind::CreateSelector
            ),
            "$to_state" => !matches!(
                self.kind,
                ScopeKind::Invariant | ScopeKind::OutcomeSelector | ScopeKind::CreateSelector
            ),
            "$from_state" => matches!(
                self.kind,
                ScopeKind::Precondition | ScopeKind::OperationTemplate | ScopeKind::OutcomeSelector
            ),
            "$args" => matches!(
                self.kind,
                ScopeKind::Precondition
                    | ScopeKind::OperationTemplate
                    | ScopeKind::OutcomeSelector
                    | ScopeKind::CreateSelector
                    | ScopeKind::CreateSet
                    | ScopeKind::CreateOutcomeTemplate
            ),
            // Nothing has been written yet at selection, so `$old_fields` would be `$fields` under
            // a second name; at creation there is no previous instance at all.
            "$old_fields" => matches!(
                self.kind,
                ScopeKind::Precondition | ScopeKind::OperationTemplate
            ),
            _ => false,
        }
    }

    fn service(&self) -> bool {
        self.semantics.is_service_1()
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

    // A binder extends the enclosing scope for the duration of the body, and its element is checked
    // against the collection's own element schema.
    if let Some(binders) = scope.binders {
        let (name, path) = match expression[1..].split_once('.') {
            Some((name, path)) => (name, Some(path)),
            None => (&expression[1..], None),
        };
        if let Some((_, element)) = binders.get(name) {
            let (Some(element), Some(path)) = (element, path) else {
                return Ok(());
            };
            return walk_field_path(element, &format!("${name}"), path, scope.service())
                .map_err(|detail| format!("'{expression}' cannot resolve: {detail}"));
        }
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
        return validate_reference_path(schema, path, noun, scope.service())
            .map_err(|detail| format!("'{expression}' cannot resolve: {detail}"));
    }

    if expression.starts_with('$') {
        return refused(format!("'{expression}' is not a reference available here"));
    }
    Ok(())
}

/// Walks `path` through the schema, so `$fields.address.countri` is refused where
/// `$fields.address.country` is accepted.
fn validate_reference_path(
    schema: &ObjectSchema,
    path: &str,
    noun: &str,
    service: bool,
) -> Result<(), String> {
    let mut segments = path.splitn(2, '.');
    let root = segments.next().unwrap_or_default();
    if root.is_empty() {
        return Err("the path is empty".into());
    }

    let field = match schema.fields.get(root) {
        Some(field) => field,
        None if schema.additional_fields => return Ok(()),
        None => return Err(format!("unknown {noun} '{root}'")),
    };

    match segments.next() {
        None => Ok(()),
        Some(rest) => walk_field_path(field, root, rest, service),
    }
}

/// The key a `union`'s payload is carried under, derived rather than declared so two readers cannot
/// disagree about it.
fn content_key(field: &FieldDefinition) -> &'static str {
    if field.tag.as_deref() == Some("value") {
        "content"
    } else {
        "value"
    }
}

/// Whether a path segment is an array ordinal: `0`, or a digit string with no leading zero.
fn is_ordinal(segment: &str) -> bool {
    segment == "0"
        || (!segment.is_empty()
            && !segment.starts_with('0')
            && segment.bytes().all(|byte| byte.is_ascii_digit()))
}

/// Walks the rest of an address from one declared field.
fn walk_field_path(
    field: &FieldDefinition,
    walked: &str,
    path: &str,
    service: bool,
) -> Result<(), String> {
    let mut segments = path.splitn(2, '.');
    let segment = segments.next().unwrap_or_default();
    let rest = segments.next();
    if segment.is_empty() {
        return Err("the path is empty".into());
    }
    let here = format!("{walked}.{segment}");

    if service {
        // The two collection address forms, checked at registration like every other address.
        match field.kind {
            FieldKind::Array if segment == "count" || is_ordinal(segment) => {
                if segment == "count" {
                    return match rest {
                        None => Ok(()),
                        Some(rest) => Err(format!(
                            "'{here}' is a count, so '{rest}' resolves to nothing"
                        )),
                    };
                }
                let Some(items) = field.items.as_deref() else {
                    return Ok(());
                };
                return match rest {
                    None => Ok(()),
                    Some(rest) => walk_field_path(items, &here, rest, service),
                };
            }
            FieldKind::Array => {
                return Err(format!(
                    "'{walked}' is an array field, so '{segment}' resolves to nothing; an array \
                     addresses `count` or an element index"
                ));
            }
            // A map's keys are not addressable, so the only `count` a map has is its size and a
            // member named `count` cannot be read as its own key.
            FieldKind::Map => {
                if segment != "count" {
                    return Err(format!(
                        "'{walked}' is a map field, so '{segment}' resolves to nothing; a map's \
                         keys are not addressable and only `count` reads it"
                    ));
                }
                return match rest {
                    None => Ok(()),
                    Some(rest) => Err(format!(
                        "'{here}' is a count, so '{rest}' resolves to nothing"
                    )),
                };
            }
            _ => {}
        }
    }

    match field.kind {
        FieldKind::Json => Ok(()),
        FieldKind::Object => match field.properties.get(segment) {
            Some(next) => match rest {
                None => Ok(()),
                Some(rest) => walk_field_path(next, &here, rest, service),
            },
            None if field.additional_properties => Ok(()),
            None => Err(format!("'{walked}' declares no property '{segment}'")),
        },
        // A union carries its label under `tag` and its payload under the derived content key. The
        // payload's type depends on the label, so a path through it is admitted and answered at run
        // time — the same position a path into a `json` field is already in.
        FieldKind::Union if service => {
            let tag = field.tag.as_deref().unwrap_or_default();
            if segment == tag {
                return match rest {
                    None => Ok(()),
                    Some(rest) => Err(format!(
                        "'{here}' is the union's tag, so '{rest}' resolves to nothing"
                    )),
                };
            }
            if segment == content_key(field) {
                return Ok(());
            }
            Err(format!(
                "'{walked}' is a union field, so '{segment}' resolves to nothing; it carries \
                 '{tag}' and '{}'",
                content_key(field)
            ))
        }
        kind => Err(format!(
            "'{walked}' is a {kind} field, so '{segment}' resolves to nothing"
        )),
    }
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
    // A document nested past the limit is refused with a code and a limit rather than by the
    // deserializer, which is the same number the source uses for the same reason.
    let depth = condition.depth();
    if depth > MAX_CONDITION_DEPTH {
        return Err(DefinitionError::ConditionTooDeep {
            path: path.to_owned(),
            depth,
            limit: MAX_CONDITION_DEPTH,
        });
    }
    // A separate pass over the whole tree, and the separateness is the load-bearing part: it runs
    // outside `validate_quantifier`, whose `map_err` relabels a defect from a quantifier **body** as
    // `QuantifierBodyScope`. An unreadable literal is not a scope defect, and keeping the walk out
    // of that call is what leaves it named for the observation domain, with its own path.
    //
    // The *order* of the two passes is not load-bearing, and was measured rather than assumed:
    // running this one second leaves every case in
    // `an_unobservable_numeric_literal_is_refused_in_every_service_1_operator_and_at_every_depth`
    // green, because the scope walk answers nothing about an authored number. Order decides only
    // which of two independent defects a document carrying both is refused with first.
    if scope.service() {
        unobservable_condition_literals(condition, path)?;
    }
    validate_condition_body(condition, path, scope)
}

fn validate_condition_body(
    condition: &Condition,
    path: &str,
    scope: Scope<'_>,
) -> Result<(), DefinitionError> {
    // A `kernel/1` definition carrying one of these would declare a rule nothing evaluates.
    if !scope.service() && SERVICE_CONDITION_OPERATORS.contains(&condition.operator()) {
        return Err(DefinitionError::SemanticsKeyNotAvailable {
            path: path.to_owned(),
            key: condition.operator().to_owned(),
        });
    }
    match condition {
        Condition::Literal(_) => Ok(()),
        Condition::Compare { compare } => {
            // The source compares scalars only, so a literal list or mapping operand is refused
            // here and the unevaluable row is reachable only through a reference.
            for (side, operand) in [("left", &compare.left), ("right", &compare.right)] {
                if matches!(operand, Value::Array(_) | Value::Object(_)) {
                    return Err(DefinitionError::CompareOperandNotAddressable {
                        path: path.to_owned(),
                        side,
                    });
                }
                validate_operand(operand, &format!("{path}.compare.{side}"), scope)?;
            }
            Ok(())
        }
        Condition::Truthy { truthy } => validate_operand(truthy, &format!("{path}.truthy"), scope),
        Condition::ForAll { for_all } => validate_quantifier(for_all, path, "for_all", scope),
        Condition::ForAny { for_any } => validate_quantifier(for_any, path, "for_any", scope),
        Condition::All { all } => {
            if all.is_empty() {
                return invalid_rule(path, "'all' must contain at least one condition");
            }
            for (index, child) in all.iter().enumerate() {
                validate_condition_body(child, &format!("{path}.all[{index}]"), scope)?;
            }
            Ok(())
        }
        Condition::Any { any } => {
            if any.is_empty() {
                return invalid_rule(path, "'any' must contain at least one condition");
            }
            for (index, child) in any.iter().enumerate() {
                validate_condition_body(child, &format!("{path}.any[{index}]"), scope)?;
            }
            Ok(())
        }
        Condition::Not { not } => validate_condition_body(not, &format!("{path}.not"), scope),
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

fn validate_pair(values: &[Value; 2], path: &str, scope: Scope<'_>) -> Result<(), DefinitionError> {
    validate_operand(&values[0], &format!("{path}[0]"), scope)?;
    validate_operand(&values[1], &format!("{path}[1]"), scope)
}

/// Every authored numeric literal a `service/1` condition writes, wherever it is written.
///
/// The observability rule belongs to the **operand**, not to one operator: `compare`, `eq`, `ne`,
/// `in`, `contains`, the ordering operators and `truthy` all read what is written there, so
/// refusing an unreadable literal in only one of them leaves the same defect admitted in the others
/// under a different key — and a membership list is where it hides best, because the literal is
/// nested one level below the operand. The match below is exhaustive on purpose: a new operator is
/// a compile error here rather than a gap nobody notices.
///
/// Under `kernel/1` this is not reached at all. That door is `number::compare`, which reads every
/// token this runtime can hold, so nothing a `kernel/1` definition admits today stops being
/// admitted.
fn unobservable_condition_literals(
    condition: &Condition,
    path: &str,
) -> Result<(), DefinitionError> {
    let pair = |values: &[Value; 2], operator: &str| {
        let path = format!("{path}.{operator}");
        unobservable_literals(&values[0], &format!("{path}[0]"))?;
        unobservable_literals(&values[1], &format!("{path}[1]"))
    };
    match condition {
        Condition::Literal(_) => Ok(()),
        Condition::Compare { compare } => {
            unobservable_literals(&compare.left, &format!("{path}.compare.left"))?;
            unobservable_literals(&compare.right, &format!("{path}.compare.right"))
        }
        Condition::Truthy { truthy } => unobservable_literals(truthy, &format!("{path}.truthy")),
        Condition::ForAll { for_all } => quantifier_literals(for_all, path, "for_all"),
        Condition::ForAny { for_any } => quantifier_literals(for_any, path, "for_any"),
        Condition::All { all } => {
            for (index, child) in all.iter().enumerate() {
                unobservable_condition_literals(child, &format!("{path}.all[{index}]"))?;
            }
            Ok(())
        }
        Condition::Any { any } => {
            for (index, child) in any.iter().enumerate() {
                unobservable_condition_literals(child, &format!("{path}.any[{index}]"))?;
            }
            Ok(())
        }
        Condition::Not { not } => unobservable_condition_literals(not, &format!("{path}.not")),
        // Three operators read no number, and each already refuses what it cannot read where it is
        // written: `exists` asks whether an address resolves and is two-valued, and `before`/`after`
        // are `kernel/1` instant operators whose operands `validate_instant_operand` refuses unless
        // they are a reference or a readable instant. Answering them here would move their refusal
        // to another message without admitting or refusing anything new.
        Condition::Exists { .. } | Condition::Before { .. } | Condition::After { .. } => Ok(()),
        Condition::Eq { eq } => pair(eq, "eq"),
        Condition::Ne { ne } => pair(ne, "ne"),
        Condition::Gt { gt } => pair(gt, "gt"),
        Condition::Gte { gte } => pair(gte, "gte"),
        Condition::Lt { lt } => pair(lt, "lt"),
        Condition::Lte { lte } => pair(lte, "lte"),
        Condition::In { values } => pair(values, "in"),
        Condition::Contains { contains } => pair(contains, "contains"),
    }
}

/// A quantifier's body, which is where a literal written inside a fold would otherwise be reported
/// as a scope defect. `in` is a reference and `QuantifierOverNotCollection` already answers
/// whatever it is not.
fn quantifier_literals(
    quantifier: &crate::Quantifier,
    path: &str,
    operator: &str,
) -> Result<(), DefinitionError> {
    unobservable_condition_literals(&quantifier.body, &format!("{path}.{operator}.that"))
}

/// Every authored number inside one operand, at any depth a literal may be written at.
fn unobservable_literals(value: &Value, path: &str) -> Result<(), DefinitionError> {
    match value {
        Value::Number(number) => unobservable_literal(number, path),
        Value::Array(values) => {
            for (index, value) in values.iter().enumerate() {
                unobservable_literals(value, &format!("{path}[{index}]"))?;
            }
            Ok(())
        }
        Value::Object(values) => {
            for (key, value) in values {
                unobservable_literals(value, &format!("{path}.{key}"))?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// A numeric literal the source cannot observe would make its comparison unevaluable at every
/// evaluation, so it is refused where it is written rather than answered `Unknown` forever.
fn unobservable_literal(number: &serde_json::Number, path: &str) -> Result<(), DefinitionError> {
    if Observed::of_literal(&number.to_string()).is_some() {
        return Ok(());
    }
    invalid_rule(
        path,
        format!(
            "{number} is outside the source observation domain, so the comparison would be \
             unevaluable at every evaluation"
        ),
    )
}

/// The closed quantifier: one non-empty binder, a collection to walk, and a body checked in the
/// enclosing scope **extended** by the binder.
fn validate_quantifier(
    quantifier: &crate::Quantifier,
    path: &str,
    operator: &str,
    scope: Scope<'_>,
) -> Result<(), DefinitionError> {
    let path = format!("{path}.{operator}");
    if quantifier.bind.trim().is_empty() || quantifier.bind.contains('.') {
        return Err(DefinitionError::QuantifierBindInvalid {
            path,
            bind: quantifier.bind.clone(),
        });
    }
    // `in` is a reference, and it has to name a collection this schema declares.
    validate_operand(&quantifier.over, &format!("{path}.in"), scope)?;
    let element = match collection_element(&quantifier.over, scope) {
        Ok(element) => element,
        Err(detail) => {
            return Err(DefinitionError::QuantifierOverNotCollection {
                path,
                over: describe_operand(&quantifier.over),
                detail,
            })
        }
    };

    let binder = BinderScope {
        name: &quantifier.bind,
        element,
        outer: scope.binders,
    };
    let inner = Scope {
        binders: Some(&binder),
        ..scope
    };
    // The body reads the enclosing scope and the binder, and nothing else. A body address neither
    // admits is this refusal rather than the enclosing scope's, because the body is where it is
    // written.
    validate_condition_body(&quantifier.body, &format!("{path}.that"), inner).map_err(|defect| {
        match defect {
            DefinitionError::InvalidRule { path, message } => {
                DefinitionError::QuantifierBodyScope {
                    path,
                    expression: describe_operand(&quantifier.over),
                    detail: message,
                }
            }
            other => other,
        }
    })
}

/// The element definition of the collection a quantifier walks, or why it is not one.
///
/// `Ok(None)` where the collection is admitted but its element kind is not declared — inside a
/// `json` field, or under a schema that admits additional members.
fn collection_element<'a>(
    over: &Value,
    scope: Scope<'a>,
) -> Result<Option<&'a FieldDefinition>, String> {
    let Value::String(expression) = over else {
        return Err("`in` is a reference to an array or map field".to_owned());
    };
    if let Some(binders) = scope.binders {
        let (name, path) = match expression[1..].split_once('.') {
            Some((name, path)) => (name, Some(path)),
            None => (&expression[1..], None),
        };
        if let Some((_, element)) = binders.get(name) {
            let (Some(element), Some(path)) = (element, path) else {
                return Ok(None);
            };
            return field_at(element, path).and_then(element_of);
        }
    }
    for (prefix, schema) in [
        ("$fields.", Some(scope.fields)),
        ("$old_fields.", Some(scope.fields)),
        ("$args.", scope.args),
    ] {
        let Some(path) = expression.strip_prefix(prefix) else {
            continue;
        };
        let Some(schema) = schema else {
            return Err(format!("'{expression}' is not available here"));
        };
        let mut segments = path.splitn(2, '.');
        let root = segments.next().unwrap_or_default();
        let field = match schema.fields.get(root) {
            Some(field) => field,
            // An undeclared member under an open schema is admitted and answered at run time.
            None if schema.additional_fields => return Ok(None),
            None => return Err(format!("'{expression}' names nothing the schema declares")),
        };
        return match segments.next() {
            None => element_of(Some(field)),
            Some(rest) => field_at(field, rest).and_then(element_of),
        };
    }
    Err(format!("'{expression}' is not a reference to a collection"))
}

/// The declared field one path reaches from another, or `None` where the kind is not declared.
fn field_at<'a>(
    field: &'a FieldDefinition,
    path: &str,
) -> Result<Option<&'a FieldDefinition>, String> {
    let mut segments = path.splitn(2, '.');
    let segment = segments.next().unwrap_or_default();
    let rest = segments.next();
    match field.kind {
        FieldKind::Json | FieldKind::Union => Ok(None),
        FieldKind::Object => match field.properties.get(segment) {
            Some(next) => match rest {
                None => Ok(Some(next)),
                Some(rest) => field_at(next, rest),
            },
            None if field.additional_properties => Ok(None),
            None => Err(format!("nothing declares a property '{segment}'")),
        },
        FieldKind::Array if is_ordinal(segment) => match field.items.as_deref() {
            None => Ok(None),
            Some(items) => match rest {
                None => Ok(Some(items)),
                Some(rest) => field_at(items, rest),
            },
        },
        kind => Err(format!("a {kind} field has no member '{segment}'")),
    }
}

fn element_of(field: Option<&FieldDefinition>) -> Result<Option<&FieldDefinition>, String> {
    match field {
        None => Ok(None),
        Some(field) if matches!(field.kind, FieldKind::Array | FieldKind::Map) => {
            Ok(field.items.as_deref())
        }
        Some(field) => Err(format!(
            "it is a {} field, and a quantifier walks an array or a map",
            field.kind
        )),
    }
}

fn describe_operand(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
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

fn validate_schema_definition(
    schema: &ObjectSchema,
    path: &str,
    semantics: Semantics,
) -> Vec<DefinitionError> {
    let mut defects = Vec::new();
    for (name, field) in &schema.fields {
        validate_field_definition(field, &format!("{path}.{name}"), semantics, &mut defects);
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
        && !matches!(
            field.kind,
            FieldKind::Integer | FieldKind::Number | FieldKind::Binary64
        )
    {
        return refuse("min/max", "an integer, number or binary64 field");
    }
    if !field.values.is_empty() && field.kind != FieldKind::Enum {
        return refuse("values", "an enum field");
    }
    if field.items.is_some() && !matches!(field.kind, FieldKind::Array | FieldKind::Map) {
        return refuse("items", "an array or map field");
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
    if field.key.is_some() && field.kind != FieldKind::Map {
        return refuse("key", "a map field");
    }
    if (field.tag.is_some() || !field.variants.is_empty()) && field.kind != FieldKind::Union {
        return refuse("tag/variants", "a union field");
    }
    Ok(())
}

fn validate_field_definition(
    field: &FieldDefinition,
    path: &str,
    semantics: Semantics,
    defects: &mut Vec<DefinitionError>,
) {
    if let Err(defect) = validate_constraint_applicability(field, path) {
        defects.push(defect);
    }

    // The three new kinds are `service/1`'s, and a `kernel/1` definition carrying one would type a
    // value nothing validates the way its author expects.
    if field.kind.is_service_only() && !semantics.is_service_1() {
        defects.push(DefinitionError::SemanticsKeyNotAvailable {
            path: path.to_owned(),
            key: field.kind.as_str().to_owned(),
        });
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
        FieldKind::Enum if field.values.is_empty() => {
            defects.push(DefinitionError::InvalidField {
                path: path.to_owned(),
                message: "enum must declare at least one value".into(),
            });
        }
        FieldKind::Array if field.items.is_none() => {
            defects.push(DefinitionError::InvalidField {
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
        FieldKind::Map => {
            if field.key.is_none() {
                defects.push(DefinitionError::MapKeyNotText {
                    path: path.to_owned(),
                });
            }
            if field.items.is_none() {
                defects.push(DefinitionError::MapValueMissing {
                    path: path.to_owned(),
                });
            }
        }
        FieldKind::Union => {
            if field.tag.as_deref().is_none_or(|tag| tag.trim().is_empty())
                || field.variants.is_empty()
            {
                defects.push(DefinitionError::UnionVariantMissing {
                    path: path.to_owned(),
                });
            } else {
                let content = content_key(field);
                // A tag equal to the key a nested variant object would carry its payload under
                // would make one key answer two questions.
                let collides = field.variants.values().any(|variant| {
                    variant.kind == FieldKind::Object && variant.properties.contains_key(content)
                });
                if collides {
                    defects.push(DefinitionError::UnionTagCollides {
                        path: path.to_owned(),
                        tag: field.tag.clone().unwrap_or_default(),
                    });
                }
            }
        }
        _ => {}
    }

    if let Some(items) = &field.items {
        validate_field_definition(items, &format!("{path}[]"), semantics, defects);
    }
    for (name, property) in &field.properties {
        validate_field_definition(property, &format!("{path}.{name}"), semantics, defects);
    }
    for (name, variant) in &field.variants {
        validate_field_definition(variant, &format!("{path}|{name}"), semantics, defects);
    }

    if let Some(default) = field.default.as_value() {
        let mut default = default.clone();
        apply_nested_defaults(field, &mut default);
        let mut errors = Vec::new();
        validate_value(field, &default, path, semantics, &mut errors);
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
    validate_object_under(schema, object, root_path, Semantics::Kernel1)
}

/// The same check under a definition's own document rules.
///
/// Under `service/1` the numeric kinds are narrowed to the source's own observation domain — an
/// integer outside the `i64` span, and any token the source's wire reader refuses, are refused with
/// a path rather than admitted and then answered about by nothing. Under `kernel/1` every answer is
/// the one it is today.
pub(crate) fn validate_object_under(
    schema: &ObjectSchema,
    object: &Map<String, Value>,
    root_path: &str,
    semantics: Semantics,
) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    validate_members(
        &schema.fields,
        schema.additional_fields,
        object,
        root_path,
        "field",
        semantics,
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
    semantics: Semantics,
    errors: &mut Vec<ValidationError>,
) {
    for (name, definition) in fields {
        match object.get(name) {
            Some(value) => validate_value(
                definition,
                value,
                &member_path(root_path, name),
                semantics,
                errors,
            ),
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
    semantics: Semantics,
    errors: &mut Vec<ValidationError>,
) {
    let service = semantics.is_service_1();
    match definition.kind {
        FieldKind::String => match value.as_str() {
            Some(string) => validate_string(definition, string, path, errors),
            None => wrong_type(path, "string", errors),
        },
        // Integers are compared as f64 rather than coerced to i64: `as u64 as i64` wrapped
        // 18446744073709551615 to -1, which passed a `max` bound and made a `min` message name a
        // number nobody sent.
        FieldKind::Integer => {
            // The source admits exactly the `i64` span, so `service/1` narrows this runtime's own
            // wider `is_i64() || is_u64()` by the `u64` tail. `kernel/1` is untouched.
            if service {
                if value.is_i64() {
                    validate_number(
                        definition,
                        value.as_number().expect("number"),
                        path,
                        semantics,
                        errors,
                    );
                } else if value.is_u64() {
                    errors.push(ValidationError::new(
                        path,
                        format!(
                            "value {value} is outside the source integer range \
                             [{}, {}]",
                            i64::MIN,
                            i64::MAX
                        ),
                    ));
                } else {
                    wrong_type(path, "integer", errors);
                }
            } else if value.is_i64() || value.is_u64() {
                validate_number(
                    definition,
                    value.as_number().expect("number"),
                    path,
                    semantics,
                    errors,
                );
            } else {
                wrong_type(path, "integer", errors);
            }
        }
        FieldKind::Number => match value.as_number() {
            Some(number) => validate_number(definition, number, path, semantics, errors),
            None => wrong_type(path, "number", errors),
        },
        // A JSON number held as its token, so the sign of a zero survives in the bytes; the
        // admitted domain is a **finite** binary64 and nothing else.
        FieldKind::Binary64 => match value.as_number() {
            Some(number) => validate_number(definition, number, path, semantics, errors),
            None => wrong_type(path, "binary64 number", errors),
        },
        FieldKind::Map => match value.as_object() {
            Some(members) => {
                let key = definition.key.unwrap_or(MapKey::String);
                for (name, member) in members {
                    if let Err(detail) = validate_map_key(key, name) {
                        errors.push(ValidationError::new(
                            format!("{path}.{name}"),
                            format!("key is not {key} text: {detail}"),
                        ));
                    }
                    if let Some(items) = &definition.items {
                        validate_value(items, member, &format!("{path}.{name}"), semantics, errors);
                    }
                }
            }
            None => wrong_type(path, "map object", errors),
        },
        FieldKind::Union => validate_union(definition, value, path, semantics, errors),
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
                        validate_value(
                            items,
                            value,
                            &format!("{path}[{index}]"),
                            semantics,
                            errors,
                        );
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
                semantics,
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
    semantics: Semantics,
    errors: &mut Vec<ValidationError>,
) {
    if semantics.is_service_1() {
        // A stored value a `service/1` predicate could not read is refused where it arrives, with
        // its path, rather than answered `Unknown` by everything that later reads it. Nothing
        // panics, saturates or invents a value.
        let Some(observed) = Observed::of_number(value) else {
            errors.push(ValidationError::new(
                path,
                format!("value {value} is outside the source observation domain"),
            ));
            return;
        };
        // An authored bound is a literal, and the source reads a literal through its own door.
        for (bound, exceeded, detail) in [
            (&definition.min, true, "is below minimum"),
            (&definition.max, false, "exceeds maximum"),
        ] {
            let Some(bound) = bound else { continue };
            let Some(limit) = Observed::of_literal(&bound.to_string()) else {
                errors.push(ValidationError::new(
                    path,
                    format!("bound {bound} is outside the source observation domain"),
                ));
                continue;
            };
            let order = observed.cmp(limit);
            if (exceeded && order.is_lt()) || (!exceeded && order.is_gt()) {
                errors.push(ValidationError::new(
                    path,
                    format!("value {value} {detail} {bound}"),
                ));
            }
        }
        return;
    }
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

/// A `union` value: the adjacent-tagged form, exactly.
///
/// An object with exactly the tag key and — unless the variant's payload is optional — the derived
/// content key; the tag's value names a declared variant, and the content satisfies that variant's
/// definition. Anything else names the tag it found.
fn validate_union(
    definition: &FieldDefinition,
    value: &Value,
    path: &str,
    semantics: Semantics,
    errors: &mut Vec<ValidationError>,
) {
    let Some(members) = value.as_object() else {
        wrong_type(path, "union object", errors);
        return;
    };
    let tag_key = definition.tag.as_deref().unwrap_or("kind");
    let content = content_key(definition);
    let Some(Value::String(label)) = members.get(tag_key) else {
        errors.push(ValidationError::new(
            path,
            format!("union carries its variant label as text under '{tag_key}'"),
        ));
        return;
    };
    let Some(variant) = definition.variants.get(label) else {
        errors.push(ValidationError::new(
            path,
            format!(
                "'{label}' is not one of [{}]",
                definition
                    .variants
                    .keys()
                    .map(String::as_str)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ));
        return;
    };
    for name in members.keys() {
        if name != tag_key && name != content {
            errors.push(ValidationError::new(
                format!("{path}.{name}"),
                format!("a union carries only '{tag_key}' and '{content}'"),
            ));
        }
    }
    match members.get(content) {
        Some(payload) => validate_value(
            variant,
            payload,
            &format!("{path}.{content}"),
            semantics,
            errors,
        ),
        None if !variant.required => {}
        None => errors.push(ValidationError::new(
            format!("{path}.{content}"),
            format!("variant '{label}' carries a payload"),
        )),
    }
}

/// A map key's spelling, because a JSON object's keys are always strings.
fn validate_map_key(key: MapKey, text: &str) -> Result<(), String> {
    match key {
        MapKey::String => Ok(()),
        MapKey::Boolean => (text == "false" || text == "true")
            .then_some(())
            .ok_or_else(|| "expected `false` or `true`".to_owned()),
        MapKey::Integer => text
            .parse::<i64>()
            .map(|_| ())
            .map_err(|_| "expected decimal integer text".to_owned()),
        MapKey::Decimal => Observed::of_literal(text)
            .map(|_| ())
            .ok_or_else(|| "expected exact decimal text".to_owned()),
        MapKey::Timestamp => crate::timestamp::parse(text)
            .map(|_| ())
            .ok_or_else(|| format!("expected an instant written {INSTANT_FORMS}")),
        MapKey::Duration => is_duration(text)
            .then_some(())
            .ok_or_else(|| "expected an ISO-8601 duration".to_owned()),
        MapKey::Uuid => is_uuid(text)
            .then_some(())
            .ok_or_else(|| "expected a canonical hyphenated UUID".to_owned()),
        MapKey::Bytes => is_padded_base64(text)
            .then_some(())
            .ok_or_else(|| "expected padded base64".to_owned()),
    }
}

/// `P` then at least one component, with `T` before the time components — the shape the source
/// admits, checked as text because that is what a key is.
fn is_duration(text: &str) -> bool {
    let Some(rest) = text.strip_prefix('P').or_else(|| text.strip_prefix("-P")) else {
        return false;
    };
    if rest.is_empty() || rest == "T" {
        return false;
    }
    let mut digits = false;
    let mut component = false;
    for byte in rest.bytes() {
        match byte {
            b'0'..=b'9' | b'.' => digits = true,
            b'T' => digits = false,
            b'Y' | b'M' | b'W' | b'D' | b'H' | b'S' => {
                if !digits {
                    return false;
                }
                digits = false;
                component = true;
            }
            _ => return false,
        }
    }
    component && !digits
}

/// The canonical hyphenated form, and only it: `8-4-4-4-12` lowercase or uppercase hex.
fn is_uuid(text: &str) -> bool {
    let groups: Vec<&str> = text.split('-').collect();
    groups.len() == 5
        && [8, 4, 4, 4, 12].iter().zip(&groups).all(|(width, group)| {
            group.len() == *width && group.bytes().all(|byte| byte.is_ascii_hexdigit())
        })
}

/// Padded base64, which is the one spelling the source admits for bytes.
fn is_padded_base64(text: &str) -> bool {
    if text.is_empty() || text.len() % 4 != 0 {
        return false;
    }
    let body = text.trim_end_matches('=');
    if text.len() - body.len() > 2 {
        return false;
    }
    body.bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'+' || byte == b'/')
}

fn wrong_type(path: &str, expected: &str, errors: &mut Vec<ValidationError>) {
    errors.push(ValidationError::new(path, format!("expected {expected}")));
}
