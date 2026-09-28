//! Shared synchronous shell exercised against real providers, including controlled edge failures.
use crate::common::{self, fields, integer, json, string, Fields, TempRoot};
use entity_core::{
    Decision, DecisionRecord, DomainEvent, EntityDefinition, EntityInstance, Registry, Runtime,
};
use entity_shell::{ShellError, StoredRuntime};
use entity_store::{
    Envelope, EventProvider, Expect, FileStore, HistoryProvider, MemoryStore, RecordedCommit,
    RecordedObservation, RecordedStore, Recording, StateProvider, Store, StoreError,
};
use ess_conformance::target::TargetError;
use ess_primitives::node::Node;
use serde::Deserialize;
use serde_json::{json as value, Value};
use std::cell::RefCell;

pub struct Target {
    registry: Registry,
    provider: Guarded,
    root: TempRoot,
}

impl Target {
    pub fn new() -> Result<Self, TargetError> {
        Ok(Self {
            registry: Registry::new(),
            provider: Guarded::new(Backend::Memory(MemoryStore::new())),
            root: TempRoot::new()?,
        })
    }

    pub fn execute(&mut self, operation: &str, input: &Fields) -> Result<Fields, TargetError> {
        match operation {
            "Open" => {
                let provider = common::text(input, "provider")?;
                self.provider = Guarded::new(match provider {
                    "Memory" => Backend::Memory(MemoryStore::new()),
                    "File" => Backend::File(FileStore::open(&self.root.0)),
                    _ => {
                        return failure(
                            "invalid_provider",
                            "input",
                            "Input",
                            "only Memory and File supply this synchronous contract",
                        )
                    }
                });
                Ok(success())
            }
            "Register" | "Replace" => {
                let definition: EntityDefinition = match decode(input, "definition") {
                    Ok(value) => value,
                    Err(result) => return Ok(result),
                };
                let result = if operation == "Register" {
                    self.registry.register(definition)
                } else {
                    self.registry.replace(definition)
                };
                match result {
                    Ok(()) => {
                        let mut output = success();
                        output.insert("count".into(), integer(self.registry.len() as i64));
                        Ok(output)
                    }
                    Err(error) => failure("definition", "kernel", "Definition", error.to_string()),
                }
            }
            "ClearRegistry" => {
                self.registry = Registry::new();
                Ok(success())
            }
            "Create" => {
                let request: Create = match decode(input, "request") {
                    Ok(value) => value,
                    Err(result) => return Ok(result),
                };
                let recording = request.recording.into();
                let result = StoredRuntime::new(&self.registry, &mut self.provider).create(
                    &request.entity,
                    request.version,
                    request.id,
                    request.fields,
                    &recording,
                );
                commit_result(result)
            }
            "Execute" => {
                let request: Execute = match decode(input, "request") {
                    Ok(value) => value,
                    Err(result) => return Ok(result),
                };
                let recording = request.recording.into();
                let result = StoredRuntime::new(&self.registry, &mut self.provider).execute(
                    &request.entity,
                    &request.id,
                    request.expected_revision,
                    &request.operation,
                    request.arguments,
                    &recording,
                );
                commit_result(result)
            }
            "Get" => {
                let request: Subject = match decode(input, "request") {
                    Ok(value) => value,
                    Err(result) => return Ok(result),
                };
                match StoredRuntime::new(&self.registry, &mut self.provider)
                    .get(&request.entity, &request.id)
                {
                    Ok(instance) => instance_result(instance),
                    Err(error) => shell_failure(error),
                }
            }
            "List" => {
                let entity = common::text(input, "entity")?;
                match StoredRuntime::new(&self.registry, &mut self.provider).list(entity) {
                    Ok(ids) => {
                        let mut result = success();
                        result.insert("value".into(), json(&ids)?);
                        result.insert("ids".into(), json(&ids)?);
                        result.insert("count".into(), integer(ids.len() as i64));
                        Ok(result)
                    }
                    Err(error) => shell_failure(error),
                }
            }
            "Events" => {
                let request: Subject = match decode(input, "request") {
                    Ok(value) => value,
                    Err(result) => return Ok(result),
                };
                match StoredRuntime::new(&self.registry, &mut self.provider)
                    .events(&request.entity, &request.id)
                {
                    Ok(events) => {
                        let mut result = success();
                        result.insert("value".into(), json(&events)?);
                        result.insert(
                            "event_types".into(),
                            json(
                                &events
                                    .iter()
                                    .map(|event| &event.event_type)
                                    .collect::<Vec<_>>(),
                            )?,
                        );
                        result.insert("count".into(), integer(events.len() as i64));
                        Ok(result)
                    }
                    Err(error) => shell_failure(error),
                }
            }
            "Records" => {
                let request: Subject = match decode(input, "request") {
                    Ok(value) => value,
                    Err(result) => return Ok(result),
                };
                match self.provider.records(&request.entity, &request.id) {
                    Ok(records) => {
                        let mut result = success();
                        result.insert("value".into(), json(&records)?);
                        result.insert(
                            "ids".into(),
                            json(
                                &records
                                    .iter()
                                    .map(|record| &record.record_id)
                                    .collect::<Vec<_>>(),
                            )?,
                        );
                        result.insert("count".into(), integer(records.len() as i64));
                        Ok(result)
                    }
                    Err(error) => shell_failure(error.into()),
                }
            }
            "Reopen" => match self.provider.backend {
                Backend::File(_) => {
                    self.provider = Guarded::new(Backend::File(FileStore::open(&self.root.0)));
                    Ok(success())
                }
                Backend::Memory(_) => failure(
                    "invalid_provider",
                    "input",
                    "Input",
                    "MemoryStore has no reopen capability",
                ),
            },
            "Fault" => {
                let fault: Fault = match decode(input, "request") {
                    Ok(value) => value,
                    Err(result) => return Ok(result),
                };
                *self.provider.fault.borrow_mut() = Some(fault);
                Ok(success())
            }
            "Race" => {
                let request: Execute = match decode(input, "request") {
                    Ok(value) => value,
                    Err(result) => return Ok(result),
                };
                let instance = match self.provider.load(&request.entity, &request.id) {
                    Ok(Some(instance)) => instance,
                    Ok(None) => {
                        return shell_failure(ShellError::NotFound {
                            entity: request.entity,
                            id: request.id,
                        })
                    }
                    Err(error) => return shell_failure(error.into()),
                };
                let decision = match Runtime::new(&self.registry).execute(
                    &instance,
                    &request.operation,
                    request.arguments,
                ) {
                    Ok(decision) => decision,
                    Err(error) => return shell_failure(error.into()),
                };
                let recording: Recording = request.recording.into();
                match RecordedCommit::new(decision, &recording) {
                    Ok(commit) => {
                        let output = commit_result(Ok(commit.clone()))?;
                        self.provider.race =
                            Some((commit, Expect::Revision(request.expected_revision)));
                        Ok(output)
                    }
                    Err(error) => shell_failure(ShellError::Recording(error.to_string())),
                }
            }
            "Calls" => {
                let mut result = success();
                result.insert("value".into(), json(&*self.provider.calls.borrow())?);
                Ok(result)
            }
            "ClearCalls" => {
                self.provider.calls.borrow_mut().clear();
                Ok(success())
            }
            other => Err(common::unavailable(format!(
                "unknown entity.shell operation {other}"
            ))),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Subject {
    entity: String,
    id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Metadata {
    record_id: String,
    recorded_at: String,
    correlation: Option<String>,
    causation: Option<String>,
    actor: Option<String>,
}
impl From<Metadata> for Recording {
    fn from(value: Metadata) -> Self {
        Self {
            record_id: value.record_id,
            recorded_at: value.recorded_at,
            correlation: value.correlation,
            causation: value.causation,
            actor: value.actor,
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Create {
    entity: String,
    version: u32,
    id: String,
    fields: Value,
    recording: Metadata,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Execute {
    entity: String,
    id: String,
    expected_revision: u64,
    operation: String,
    arguments: Value,
    recording: Metadata,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Fault {
    method: Method,
    error: Failure,
}
#[derive(Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
enum Method {
    Load,
    Ids,
    Events,
    Records,
    Commit,
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Failure {
    Unreachable,
    Backend,
    RecordConflict,
}

enum Backend {
    Memory(MemoryStore),
    File(FileStore),
}
impl Backend {
    fn store(&self) -> &dyn RecordedStore {
        match self {
            Self::Memory(store) => store,
            Self::File(store) => store,
        }
    }
    fn store_mut(&mut self) -> &mut dyn RecordedStore {
        match self {
            Self::Memory(store) => store,
            Self::File(store) => store,
        }
    }
}
struct Guarded {
    backend: Backend,
    fault: RefCell<Option<Fault>>,
    race: Option<(RecordedCommit, Expect)>,
    calls: RefCell<Vec<&'static str>>,
}
impl Guarded {
    fn new(backend: Backend) -> Self {
        Self {
            backend,
            fault: RefCell::new(None),
            race: None,
            calls: RefCell::new(Vec::new()),
        }
    }
    fn enter(&self, method: Method, name: &'static str) -> Result<(), StoreError> {
        self.calls.borrow_mut().push(name);
        if self
            .fault
            .borrow()
            .as_ref()
            .is_some_and(|fault| fault.method == method)
        {
            let fault = self
                .fault
                .borrow_mut()
                .take()
                .expect("checked configured failure");
            return Err(match fault.error {
                Failure::Unreachable => StoreError::Unreachable {
                    provider: "scenario".into(),
                    detail: "injected unavailable provider".into(),
                },
                Failure::Backend => StoreError::Backend("injected provider failure".into()),
                Failure::RecordConflict => StoreError::RecordConflict {
                    record_id: "injected".into(),
                },
            });
        }
        Ok(())
    }
}
impl StateProvider for Guarded {
    fn load(&self, entity: &str, id: &str) -> Result<Option<EntityInstance>, StoreError> {
        self.enter(Method::Load, "load")?;
        self.backend.store().load(entity, id)
    }
    fn ids(&self, entity: &str) -> Result<Vec<String>, StoreError> {
        self.enter(Method::Ids, "ids")?;
        self.backend.store().ids(entity)
    }
}
impl EventProvider for Guarded {
    fn events(&self, entity: &str, id: &str) -> Result<Vec<DomainEvent>, StoreError> {
        self.enter(Method::Events, "events")?;
        self.backend.store().events(entity, id)
    }
}
impl HistoryProvider for Guarded {
    fn records(&self, entity: &str, id: &str) -> Result<Vec<Envelope<DecisionRecord>>, StoreError> {
        self.enter(Method::Records, "records")?;
        self.backend.store().records(entity, id)
    }
    fn observations(&self, entity: &str, id: &str) -> Result<Vec<RecordedObservation>, StoreError> {
        self.backend.store().observations(entity, id)
    }
}
impl Store for Guarded {
    fn history(&self) -> Option<&dyn HistoryProvider> {
        Some(self)
    }
    fn commit(&mut self, decision: &Decision, expect: Expect) -> Result<(), StoreError> {
        self.enter(Method::Commit, "commit")?;
        self.backend.store_mut().commit(decision, expect)
    }
    fn commit_recorded(
        &mut self,
        commit: &RecordedCommit,
        expect: Expect,
    ) -> Result<(), StoreError> {
        self.enter(Method::Commit, "commit_recorded")?;
        if let Some((competitor, expected)) = self.race.take() {
            self.backend
                .store_mut()
                .commit_recorded(&competitor, expected)?;
        }
        self.backend.store_mut().commit_recorded(commit, expect)
    }
}

fn success() -> Fields {
    fields([
        ("ok", Node::Bool(true)),
        ("kind", string("")),
        ("boundary", string("")),
        ("variant", string("")),
        ("detail", string("")),
        ("value", string("null")),
        ("ids", string("[]")),
        ("event_types", string("[]")),
        ("count", integer(0)),
        ("revision", integer(0)),
        ("state", string("")),
        ("fields", string("{}")),
        ("record_id", string("")),
    ])
}
fn failure(
    kind: &str,
    boundary: &str,
    variant: &str,
    detail: impl Into<String>,
) -> Result<Fields, TargetError> {
    let mut result = success();
    result.insert("ok".into(), Node::Bool(false));
    result.insert("kind".into(), string(kind));
    result.insert("boundary".into(), string(boundary));
    result.insert("variant".into(), string(variant));
    result.insert("detail".into(), string(detail));
    Ok(result)
}
fn shell_failure(error: ShellError) -> Result<Fields, TargetError> {
    let (variant, coordinates) = match &error {
        ShellError::Core(_) => ("Core", Value::Null),
        ShellError::Recording(_) => ("Recording", Value::Null),
        ShellError::NotFound { entity, id } => ("NotFound", value!({"entity":entity,"id":id})),
        ShellError::StaleRevision {
            entity,
            id,
            expected,
            found,
        } => (
            "StaleRevision",
            value!({"entity":entity,"id":id,"expected":expected,"found":found}),
        ),
        ShellError::Store(StoreError::RevisionConflict {
            entity,
            id,
            expected,
            found,
        }) => (
            "RevisionConflict",
            value!({"entity":entity,"id":id,"expected":expected.to_string(),"found":found}),
        ),
        ShellError::Store(StoreError::RecordConflict { record_id }) => {
            ("RecordConflict", value!({"record_id":record_id}))
        }
        ShellError::Store(StoreError::Unreachable { provider, detail }) => {
            ("Unreachable", value!({"provider":provider,"detail":detail}))
        }
        ShellError::Store(StoreError::Backend(_)) => ("Backend", Value::Null),
    };
    let mut result = failure(error.kind(), error.boundary(), variant, error.to_string())?;
    result.insert("value".into(), json(&coordinates)?);
    Ok(result)
}
fn instance_result(instance: EntityInstance) -> Result<Fields, TargetError> {
    let mut result = success();
    result.insert(
        "revision".into(),
        Node::Number(
            ess_primitives::facts::Number::decimal_literal(&instance.revision.to_string())
                .expect("every u64 is an exact ESS decimal integer"),
        ),
    );
    result.insert("state".into(), string(&instance.lifecycle_state));
    result.insert("fields".into(), json(&instance.fields)?);
    result.insert("value".into(), json(&instance)?);
    Ok(result)
}
fn commit_result(result: Result<RecordedCommit, ShellError>) -> Result<Fields, TargetError> {
    match result {
        Ok(commit) => {
            let mut result = instance_result(commit.instance.clone())?;
            result.insert("value".into(), json(&commit)?);
            result.insert("record_id".into(), string(&commit.envelope.record_id));
            result.insert(
                "event_types".into(),
                json(
                    &commit
                        .envelope
                        .record
                        .events
                        .iter()
                        .map(|event| &event.event_type)
                        .collect::<Vec<_>>(),
                )?,
            );
            result.insert(
                "count".into(),
                integer(commit.envelope.record.events.len() as i64),
            );
            Ok(result)
        }
        Err(error) => shell_failure(error),
    }
}
fn decode<T: serde::de::DeserializeOwned>(input: &Fields, field: &str) -> Result<T, Fields> {
    let parsed = common::text(input, field)
        .map_err(|error| format!("{error:?}"))
        .and_then(|text| serde_json::from_str(text).map_err(|error| error.to_string()));
    parsed.map_err(|detail| {
        failure("decode", "input", "Decode", detail)
            .expect("constructing a refusal performs no fallible operation")
    })
}
