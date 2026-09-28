//! Direct observations of pure kernel APIs. Definitions and values always come from the suite.
use crate::common::{fields, json, string, text, Fields};
use entity_core::{
    CoreError, DefinitionErrors, EntityDefinition, EntityInstance, Evaluation, LoadedDecision,
    PreloadDecision, Registry, ValidatedDefinition,
};
use ess_conformance::target::TargetError;
use ess_primitives::node::Node;
use serde::de::DeserializeOwned;
use serde_json::Value;

pub struct Target {
    registry: Registry,
    returned_records: Vec<entity_core::DecisionRecord>,
}

#[derive(Debug)]
enum Failure {
    Decode(serde_json::Error),
    Core(CoreError),
    Definition(DefinitionErrors),
    Adapter(TargetError),
}
impl From<CoreError> for Failure {
    fn from(e: CoreError) -> Self {
        Self::Core(e)
    }
}
impl From<DefinitionErrors> for Failure {
    fn from(e: DefinitionErrors) -> Self {
        Self::Definition(e)
    }
}
impl From<TargetError> for Failure {
    fn from(e: TargetError) -> Self {
        Self::Adapter(e)
    }
}

fn parse<T: DeserializeOwned>(input: &Fields, key: &str) -> Result<T, Failure> {
    serde_json::from_str(text(input, key)?).map_err(Failure::Decode)
}
fn definition(input: &Fields) -> Result<ValidatedDefinition, Failure> {
    Ok(ValidatedDefinition::new(parse(input, "definition")?)?)
}
fn ok(value: Node) -> Fields {
    fields([("status", string("ok")), ("value", value)])
}
fn refusal(value: entity_core::Refusal) -> Fields {
    fields([
        ("status", string("refused")),
        ("outcome", string(value.outcome)),
        ("error_name", string(value.error)),
        (
            "value",
            string(serde_json::to_string(&value.message).expect("string option")),
        ),
    ])
}
fn evaluation(value: Evaluation) -> Result<Fields, TargetError> {
    match value {
        Evaluation::Refused(r) => Ok(refusal(r)),
        Evaluation::Accepted(d) => {
            let mut result = ok(json(&d)?);
            result.insert("instance".into(), json(&d.instance)?);
            result.insert("events".into(), json(&d.events)?);
            result.insert("record".into(), json(&d.record)?);
            result.insert("response".into(), json(&d.record.response)?);
            if let Some(outcome) = d.record.outcome {
                result.insert("outcome".into(), string(outcome));
            }
            Ok(result)
        }
    }
}

impl Target {
    pub fn new() -> Result<Self, TargetError> {
        Ok(Self {
            registry: Registry::new(),
            returned_records: Vec::new(),
        })
    }
    pub fn execute(&mut self, operation: &str, input: &Fields) -> Result<Fields, TargetError> {
        match self.call(operation, input) {
            Ok(result) => {
                // Scenario-local copies of actual return values, never kernel persistence.
                if let Some(Node::Text(record)) = result.get("record") {
                    self.returned_records
                        .push(serde_json::from_str(record).map_err(crate::common::unavailable)?);
                }
                Ok(result)
            }
            Err(Failure::Adapter(error)) => Err(error),
            Err(Failure::Decode(error)) => Ok(fields([
                ("status", string("error")),
                ("error_kind", string("decode")),
                ("details", string(error.to_string())),
            ])),
            Err(Failure::Definition(errors)) => Ok(fields([
                ("status", string("error")),
                ("error_kind", string("definition")),
                ("details", string(format!("{errors:?}"))),
                (
                    "defect_kinds",
                    Node::Seq(errors.iter().map(|e| string(e.kind())).collect()),
                ),
            ])),
            Err(Failure::Core(error)) => {
                let mut result = fields([
                    ("status", string("error")),
                    ("error_kind", string(error.kind())),
                    ("details", string(format!("{error:?}"))),
                ]);
                if let CoreError::Validation(errors) = &error {
                    result.insert(
                        "paths".into(),
                        Node::Seq(errors.iter().map(|e| string(&e.path)).collect()),
                    );
                }
                if let CoreError::Refused { outcome, error, .. } = &error {
                    result.insert("outcome".into(), string(outcome));
                    result.insert("error_name".into(), string(error));
                }
                if let CoreError::PreconditionUnobservable { unresolved, .. }
                | CoreError::InvariantUnobservable { unresolved, .. }
                | CoreError::OutcomeUnobservable { unresolved, .. } = &error
                {
                    result.insert(
                        "unresolved".into(),
                        Node::Seq(unresolved.iter().map(string).collect()),
                    );
                }
                Ok(result)
            }
        }
    }
    fn call(&mut self, operation: &str, input: &Fields) -> Result<Fields, Failure> {
        match operation {
            "RuntimeCreate" => Ok(evaluation(Evaluation::Accepted(
                entity_core::Runtime::new(&self.registry).create(
                    text(input, "entity")?,
                    parse(input, "version")?,
                    text(input, "id")?,
                    parse(input, "fields")?,
                )?,
            ))?),
            "RuntimeExecute" => Ok(evaluation(Evaluation::Accepted(
                entity_core::Runtime::new(&self.registry).execute(
                    &parse(input, "instance")?,
                    text(input, "operation")?,
                    parse(input, "arguments")?,
                )?,
            ))?),
            "Register" | "Replace" => {
                let d: EntityDefinition = parse(input, "definition")?;
                if operation == "Register" {
                    self.registry.register(d)?;
                } else {
                    self.registry.replace(d)?;
                }
                Ok(ok(json(
                    &self.registry.iter().map(|d| &**d).collect::<Vec<_>>(),
                )?))
            }
            "Registry" => Ok(ok(json(
                &self.registry.iter().map(|d| &**d).collect::<Vec<_>>(),
            )?)),
            "ValidateRegistry" => {
                self.registry.validate_all()?;
                Ok(ok(string("null")))
            }
            "ValidateDefinition" => {
                let d = definition(input)?;
                Ok(ok(json(&*d)?))
            }
            "Create" => Ok(evaluation(entity_core::decide_create(
                &definition(input)?,
                text(input, "id")?.into(),
                parse(input, "fields")?,
            )?)?),
            "CreateDerived" => Ok(evaluation(entity_core::decide_create_derived(
                &definition(input)?,
                parse(input, "fields")?,
            )?)?),
            "Execute" => {
                let instance: EntityInstance = parse(input, "instance")?;
                Ok(evaluation(entity_core::decide(
                    &definition(input)?,
                    &instance,
                    text(input, "operation")?,
                    parse(input, "arguments")?,
                )?)?)
            }
            "NormalizeArguments" => Ok(ok(json(&entity_core::normalize_arguments(
                &definition(input)?,
                text(input, "operation")?,
                parse(input, "arguments")?,
            )?)?)),
            "Prepare" | "Continue" | "Fulfill" => {
                let d = definition(input)?;
                let decision = entity_core::decide_before_load(
                    &d,
                    text(input, "id")?,
                    text(input, "operation")?,
                    parse(input, "arguments")?,
                )?;
                match decision {
                    PreloadDecision::Refused(r) => Ok(refusal(r)),
                    PreloadDecision::Load(p) => {
                        if operation == "Prepare" {
                            let subject = p.subject();
                            Ok(fields([
                                ("status", string("load")),
                                ("arguments", json(p.normalized_arguments())?),
                                (
                                    "subject",
                                    json(
                                        &serde_json::json!({"entity":subject.entity(),"version":subject.version(),"id":subject.id()}),
                                    )?,
                                ),
                            ]))
                        } else {
                            let instance: EntityInstance = parse(input, "instance")?;
                            if operation == "Continue" {
                                return Ok(evaluation(p.continue_with(&instance)?)?);
                            }
                            match p.select_with(&instance)? {
                                LoadedDecision::Complete(e) => Ok(evaluation(e)?),
                                LoadedDecision::NeedsFulfillment(p) => {
                                    let requirements = json(p.requirements())?;
                                    let mut result =
                                        evaluation(p.complete(parse(input, "actions")?)?)?;
                                    result.insert("requirements".into(), requirements);
                                    Ok(result)
                                }
                            }
                        }
                    }
                }
            }
            "Replay" => Ok(ok(json(&entity_core::replay(&parse::<
                Vec<entity_core::DecisionRecord>,
            >(
                input, "records"
            )?)?)?)),
            "ReplayReturned" => Ok(ok(json(&entity_core::replay(&self.returned_records)?)?)),
            "ReplayTampered" => {
                let mut records = self.returned_records.clone();
                let patch: serde_json::Map<String, Value> = parse(input, "patch")?;
                let record = records.last_mut().ok_or_else(|| {
                    Failure::Adapter(crate::common::unavailable("no returned record to tamper"))
                })?;
                let mut raw = serde_json::to_value(&*record).map_err(Failure::Decode)?;
                for (key, value) in patch {
                    raw.as_object_mut()
                        .expect("record object")
                        .insert(key, value);
                }
                *record = serde_json::from_value(raw).map_err(Failure::Decode)?;
                Ok(ok(json(&entity_core::replay(&records)?)?))
            }
            "Rehydrate" => {
                let d = definition(input)?;
                Ok(ok(json(&entity_core::rehydrate(
                    &d,
                    &parse::<Vec<entity_core::DomainEvent>>(input, "events")?,
                )?)?))
            }
            "CompareNumbers" => {
                let left: serde_json::Number = parse(input, "left")?;
                let right: serde_json::Number = parse(input, "right")?;
                let ordering = entity_core::compare_numbers(&left, &right);
                Ok(ok(string(format!("{ordering:?}"))))
            }
            "Address" => {
                let kind: entity_core::FieldKind = parse(input, "kind")?;
                let value: Value = parse(input, "value")?;
                match entity_core::identity::address(kind, &value) {
                    Ok(value) => Ok(ok(string(value))),
                    Err(error) => Ok(fields([
                        ("status", string("error")),
                        ("error_kind", string("address")),
                        ("details", string(format!("{error:?}"))),
                    ])),
                }
            }
            "Timestamp" => Ok(ok(json(&entity_core::is_valid_timestamp(text(
                input, "value",
            )?))?)),
            _ => Err(Failure::Adapter(TargetError::unsupported(
                operation,
                "unknown core API",
            ))),
        }
    }
}
