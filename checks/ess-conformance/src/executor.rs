//! The actual runtime-neutral executor over the real recorded-memory provider.
//!
//! Refusal recording and fault/lineage ports are explicitly supplied fixtures because
//! MemoryRecordedStore does not advertise those optional capabilities. A lineage fixture
//! captures a real append request and refuses the append; it never invents a durable commit.
use std::{collections::BTreeMap, sync::Mutex, task::Poll};

use entity_core::{EntityDefinition, EntityInstance, Registry};
use entity_executor::{
    test_support::{block_on, poll_once},
    BatchAction, CreateRequest, ExecuteRequest, ExecutionError, Executor, MergeRequest,
    VersionedBatchAction,
};
use entity_store::{
    asynchronous::{
        self as a, AsyncRecordedReader, AsyncRecordedWriter, AsyncRefusalRecorder, AsyncStateReader,
    },
    RecordedObservation,
};
use ess_conformance::target::TargetError;
use serde_json::{json, Value};

use crate::{
    common::{self, Fields},
    store::{self, optional, read, recording, response, serialize, subject, Failure, Result},
};

#[derive(Default)]
struct RefusalFixture(Mutex<Vec<a::RecordedRefusal>>);
impl AsyncRefusalRecorder for RefusalFixture {
    fn record_refusal<'a>(
        &'a self,
        refusal: &'a a::RecordedRefusal,
    ) -> a::BoxFuture<'a, std::result::Result<bool, a::AsyncStoreError>> {
        Box::pin(async move {
            let mut held = self
                .0
                .lock()
                .map_err(|e| a::AsyncStoreError::Backend(e.to_string()))?;
            let prior = held.contains(refusal);
            if !prior {
                held.push(refusal.clone());
            }
            Ok(prior)
        })
    }
    fn refusals<'a>(
        &'a self,
    ) -> a::BoxFuture<'a, std::result::Result<Vec<a::RecordedRefusal>, a::AsyncStoreError>> {
        Box::pin(async move {
            self.0
                .lock()
                .map(|held| held.clone())
                .map_err(|e| a::AsyncStoreError::Backend(e.to_string()))
        })
    }
}

struct Ports {
    memory: a::MemoryRecordedStore,
    load_fault: Option<a::AsyncStoreError>,
    supplied_history: Option<a::SubjectHistory>,
    captured: Mutex<Option<a::AppendRequest>>,
}
impl AsyncStateReader for Ports {
    fn load<'a>(
        &'a self,
        subject: &'a a::Subject,
    ) -> a::BoxFuture<'a, std::result::Result<Option<EntityInstance>, a::AsyncStoreError>> {
        Box::pin(async move {
            if let Some(error) = &self.load_fault {
                return Err(error.clone());
            }
            self.memory.load(subject).await
        })
    }
}
impl AsyncRecordedReader for Ports {
    fn lookup_record<'a>(
        &'a self,
        id: &'a str,
    ) -> a::BoxFuture<'a, std::result::Result<Option<a::RecordLookup>, a::AsyncStoreError>> {
        self.memory.lookup_record(id)
    }
    fn lookup_batch<'a>(
        &'a self,
        key: &'a a::BatchKey,
    ) -> a::BoxFuture<'a, std::result::Result<Option<a::StoredBatch>, a::AsyncStoreError>> {
        self.memory.lookup_batch(key)
    }
    fn history<'a>(
        &'a self,
        subject: &'a a::Subject,
    ) -> a::BoxFuture<'a, std::result::Result<a::SubjectHistory, a::AsyncStoreError>> {
        Box::pin(async move {
            match &self.supplied_history {
                Some(h) if &h.subject == subject => Ok(h.clone()),
                _ => self.memory.history(subject).await,
            }
        })
    }
    fn complete_snapshot<'a>(
        &'a self,
        scope: &'a str,
    ) -> a::BoxFuture<'a, std::result::Result<a::CompleteStoreSnapshot, a::AsyncStoreError>> {
        self.memory.complete_snapshot(scope)
    }
}
impl AsyncRecordedWriter for Ports {
    fn append(
        &self,
        request: a::AppendRequest,
    ) -> a::BoxFuture<'_, std::result::Result<a::AppendOutcome, a::WriteFailure>> {
        if self.supplied_history.is_none() {
            return self.memory.append(request);
        }
        Box::pin(async move {
            request.validate().map_err(a::WriteFailure::NotCommitted)?;
            *self.captured.lock().map_err(|e| {
                a::WriteFailure::NotCommitted(a::AsyncStoreError::Backend(e.to_string()))
            })? = Some(request);
            Err(a::WriteFailure::NotCommitted(a::AsyncStoreError::Backend(
                "supplied lineage port captures but refuses append".into(),
            )))
        })
    }
}

/// Isolated registry, real provider and optional explicitly supplied ports for one scenario.
pub struct Target {
    registry: Registry,
    ports: Ports,
    refusals: RefusalFixture,
    record_refusals: bool,
}
impl Target {
    pub fn new() -> std::result::Result<Self, TargetError> {
        Ok(Self {
            registry: Registry::new(),
            ports: Ports {
                memory: a::MemoryRecordedStore::new(),
                load_fault: None,
                supplied_history: None,
                captured: Mutex::new(None),
            },
            refusals: RefusalFixture::default(),
            record_refusals: false,
        })
    }
    fn executor(&self) -> Executor<'_> {
        if self.record_refusals {
            Executor::recording_refusals(&self.registry, &self.ports, &self.refusals)
        } else {
            Executor::new(&self.registry, &self.ports)
        }
    }
    pub fn execute(
        &mut self,
        operation: &str,
        input: &Fields,
    ) -> std::result::Result<Fields, TargetError> {
        let request = match serde_json::from_str::<Value>(common::text(input, "request")?) {
            Ok(v) => v,
            Err(e) => return response(Err(e.into())),
        };
        response(self.invoke(operation, &request))
    }
    fn invoke(&mut self, operation: &str, request: &Value) -> Result<Value> {
        match operation {
            "Register" => {
                let d: EntityDefinition = read(request, "definition")?;
                self.registry.register(d).map_err(Failure::debug)?;
                Ok(Value::Null)
            }
            "ClearRegistry" => {
                self.registry = Registry::new();
                Ok(Value::Null)
            }
            "Create" => Ok(store::outcome(
                block_on(self.executor().create(create(request)?)).map_err(execution_error)?,
            )),
            "Execute" => {
                let result =
                    if request.get("definition_version").is_some() {
                        block_on(self.executor().execute_versioned(
                            execute(request)?,
                            read(request, "definition_version")?,
                        ))
                    } else {
                        block_on(self.executor().execute(execute(request)?))
                    };
                Ok(store::outcome(result.map_err(execution_error)?))
            }
            "Observe" => Ok(store::outcome(
                block_on(
                    self.executor()
                        .observe(read::<RecordedObservation>(request, "observation")?),
                )
                .map_err(execution_error)?,
            )),
            "Batch" => Ok(store::outcome(block_on(self.run_batch(request))?)),
            "CancelBatch" => {
                let key = read(request, "key")?;
                let versioned = versioned_actions(request)?;
                let legacy = if versioned.is_none() {
                    actions(request)?
                } else {
                    Vec::new()
                };
                let executor = self.executor();
                let mut future = Box::pin(async move {
                    match versioned {
                        Some(actions) => executor.batch_versioned(key, actions).await,
                        None => executor.batch(key, legacy).await,
                    }
                    .map_err(execution_error)
                });
                if !read::<bool>(request, "poll")? {
                    drop(future);
                    return Ok(json!({"poll":"Unpolled"}));
                }
                match poll_once(future.as_mut()) {
                    Poll::Pending => {
                        drop(future);
                        Ok(json!({"poll":"Pending"}))
                    }
                    Poll::Ready(result) => {
                        Ok(json!({"poll":"Ready","outcome":store::outcome(result?)}))
                    }
                }
            }
            "Load" => serialize(block_on(self.ports.load(&subject(request)?))?),
            "History" => Ok(store::history(&block_on(
                self.ports.history(&subject(request)?),
            )?)),
            "HistoryCoordinates" => {
                let history = block_on(self.ports.history(&subject(request)?))?;
                Ok(json!(history.records.iter().map(|record|json!({"record_id":record.entry.record_id(),"kind":record.entry.kind(),"revision":record.entry.revision(),"events":record.entry.events(),"receipt":record.receipt})).collect::<Vec<_>>()))
            }
            "LookupRecord" => Ok(store::lookup(block_on(self.ports.lookup_record(&read::<
                String,
            >(
                request,
                "record_id",
            )?))?)),
            "LookupReceipt" => {
                match block_on(
                    self.ports
                        .lookup_record(&read::<String>(request, "record_id")?),
                )? {
                    Some(a::RecordLookup::Committed(record)) => serialize(record.receipt),
                    Some(a::RecordLookup::Imported(evidence)) => {
                        Ok(json!({"kind":"Imported","evidence":evidence}))
                    }
                    None => Ok(Value::Null),
                }
            }
            "Trace" => serialize(self.ports.memory.trace()),
            "ClearTrace" => {
                self.ports.memory.clear_trace();
                Ok(Value::Null)
            }
            "ScriptAppend" => {
                let script = match read::<String>(request, "script")?.as_str() {
                    "CommitThenUncertain" => a::AppendScript::CommitThenUncertain,
                    "CommitThenPending" => a::AppendScript::CommitThenPending,
                    _ => return Err(Failure::named("Deserialize", "unknown append script")),
                };
                self.ports.memory.script_next_append(script);
                Ok(Value::Null)
            }
            "RecordingRefusals" => {
                self.record_refusals = read(request, "enabled")?;
                Ok(Value::Null)
            }
            "Refusals" => {
                let refusals = block_on(self.refusals.refusals())?;
                Ok(json!(refusals.iter().map(|r|json!({"key":r.key,"subjects":r.subjects,"request":r.request,"recording":{"record_id":r.recording.record_id,"recorded_at":r.recording.recorded_at,"correlation":r.recording.correlation,"causation":r.recording.causation,"actor":r.recording.actor},"reason":reason(&r.reason)})).collect::<Vec<_>>()))
            }
            "RefusalReasons" => serialize(
                block_on(self.refusals.refusals())?
                    .into_iter()
                    .map(|r| reason(&r.reason))
                    .collect::<Vec<_>>(),
            ),
            "SupplyLoadFailure" => {
                self.ports.load_fault = match read::<String>(request, "kind")?.as_str() {
                    "None" => None,
                    "Unreachable" => Some(a::AsyncStoreError::Unreachable {
                        provider: read(request, "provider")?,
                        detail: read(request, "detail")?,
                    }),
                    "Forked" => Some(a::AsyncStoreError::Forked {
                        subject: subject(request)?,
                        heads: read(request, "heads")?,
                    }),
                    _ => return Err(Failure::named("Deserialize", "unknown supplied load fault")),
                };
                Ok(Value::Null)
            }
            "SupplyHistory" => {
                self.ports.supplied_history = Some(store::history_from(&request["history"])?);
                Ok(Value::Null)
            }
            "CapturedMerge" => {
                let held = self
                    .ports
                    .captured
                    .lock()
                    .map_err(|e| Failure::named("AdapterLock", e))?;
                Ok(held.as_ref().map_or(Value::Null,|r|json!(r.members.iter().map(|m|json!({"merge":m.merge.as_ref().map(|b|json!({"heads":b.heads,"base":b.base})),"result":match &m.entry{a::RecordedEntry::Decision(c)=>Some(&c.instance),_=>None}})).collect::<Vec<_>>())))
            }
            "CapturedAppend" => {
                let held = self
                    .ports
                    .captured
                    .lock()
                    .map_err(|e| Failure::named("AdapterLock", e))?;
                Ok(held.as_ref().map_or(Value::Null,|r|json!({"key":r.key,"members":r.members.iter().map(|m|json!({"expect":match m.expect{entity_store::Expect::Absent=>Value::Null,entity_store::Expect::Revision(n)=>json!(n)},"entry":m.entry,"merge":m.merge.as_ref().map(|b|json!({"heads":b.heads,"base":b.base}))})).collect::<Vec<_>>()})))
            }
            "SeedImported" => {
                self.ports
                    .memory
                    .seed_imported(store::history_from(&request["history"])?)?;
                Ok(Value::Null)
            }
            "VerifyComplete" => Ok(store::assurance(block_on(a::verify_complete_store(
                &self.ports,
                &read::<String>(request, "scope")?,
            ))?)),
            "Tamper" => {
                let id: String = read(request, "record_id")?;
                match read::<String>(request, "field")?.as_str() {
                    "result" => self.ports.memory.tamper_decision_result_for_test(&id),
                    "event" => self.ports.memory.tamper_record_event_for_test(&id),
                    "receipt_subject" => self
                        .ports
                        .memory
                        .tamper_receipt_subject_for_test(&id, &read::<String>(request, "id")?),
                    "definition" => self.ports.memory.remove_imported_definition_for_test(&id),
                    _ => return Err(Failure::named("Deserialize", "unknown tamper control")),
                }
                Ok(Value::Null)
            }
            _ => Err(Failure::named(
                "Operation",
                format!("unknown executor operation {operation}"),
            )),
        }
    }
    async fn run_batch(&self, request: &Value) -> Result<a::AppendOutcome> {
        let key = read(request, "key")?;
        let result = match versioned_actions(request)? {
            Some(actions) => self.executor().batch_versioned(key, actions).await,
            None => self.executor().batch(key, actions(request)?).await,
        };
        result.map_err(execution_error)
    }
}

fn execution_error(error: ExecutionError) -> Failure {
    match error {
        ExecutionError::Core(e) => Failure::debug(e),
        ExecutionError::Store(e) => e.into(),
        ExecutionError::Write(e) => e.into(),
    }
}
fn create(v: &Value) -> Result<CreateRequest> {
    Ok(CreateRequest {
        subject: subject(v)?,
        definition_version: read(v, "version")?,
        fields: read(v, "fields")?,
        recording: recording(&v["recording"])?,
    })
}
fn execute(v: &Value) -> Result<ExecuteRequest> {
    Ok(ExecuteRequest {
        subject: subject(v)?,
        expected_revision: read(v, "expected_revision")?,
        operation: read(v, "operation")?,
        arguments: read(v, "arguments")?,
        fulfillments: optional(v, "fulfillments", BTreeMap::new())?,
        recording: recording(&v["recording"])?,
    })
}
fn actions(v: &Value) -> Result<Vec<BatchAction>> {
    read::<Vec<Value>>(v, "actions")?
        .iter()
        .map(|a| match read::<String>(a, "kind")?.as_str() {
            "create" => Ok(BatchAction::Create(create(a)?)),
            "execute" => Ok(BatchAction::Execute(execute(a)?)),
            "observe" => Ok(BatchAction::Observe(read(a, "observation")?)),
            "merge" => Ok(BatchAction::Merge(MergeRequest {
                execute: execute(a)?,
                first: read(a, "first")?,
            })),
            _ => Err(Failure::named("Deserialize", "unknown batch action")),
        })
        .collect()
}

fn versioned_actions(v: &Value) -> Result<Option<Vec<VersionedBatchAction>>> {
    let raw = read::<Vec<Value>>(v, "actions")?;
    if !raw
        .iter()
        .any(|action| action.get("definition_version").is_some())
    {
        return Ok(None);
    }
    raw.iter()
        .map(|action| {
            let kind = read::<String>(action, "kind")?;
            match kind.as_str() {
                "execute" => Ok(VersionedBatchAction::Execute {
                    definition_version: read(action, "definition_version")?,
                    request: execute(action)?,
                }),
                "merge" => Ok(VersionedBatchAction::Merge {
                    definition_version: read(action, "definition_version")?,
                    request: MergeRequest {
                        execute: execute(action)?,
                        first: read(action, "first")?,
                    },
                }),
                "create" | "observe" if action.get("definition_version").is_some() => {
                    Err(Failure::named(
                        "Deserialize",
                        "definition_version is only valid for execute or merge",
                    ))
                }
                "create" => Ok(VersionedBatchAction::Create(create(action)?)),
                "observe" => Ok(VersionedBatchAction::Observe(read(action, "observation")?)),
                _ => Err(Failure::named("Deserialize", "unknown batch action")),
            }
        })
        .collect::<Result<Vec<_>>>()
        .map(Some)
}

fn reason(reason: &a::RefusalReason) -> Value {
    match reason {
        a::RefusalReason::Kernel { message } => json!({"kind":"Kernel","message":message}),
        a::RefusalReason::Conflict {
            subject,
            expected,
            found,
        } => json!({"kind":"Conflict","subject":subject,"expected":expected,"found":found}),
        a::RefusalReason::Forked { subject, heads } => {
            json!({"kind":"Forked","subject":subject,"heads":heads})
        }
    }
}
