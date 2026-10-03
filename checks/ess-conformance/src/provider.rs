//! Real SQLite provider and synchronous facade calls; SQL corruption is an explicit fixture.
//! These scenarios prove returned semantics and integrity. Bounded calls and retained-path cost
//! are verified separately by runtime counter tests and the measured workload.
use std::num::NonZeroU16;

use entity_core::{EntityDefinition, Registry};
use entity_eventlog::{
    sync::{
        BridgeConfig, BridgeStartError, CallWait, EventlogRecordedStoreOwner,
        EventlogRecordedStoreProvisioner, ProvisionAuthority, ShutdownMode, SyncExecutionError,
        SyncReadError,
    },
    Authority, CapturePolicy, EventlogOperationContext, RecordedProviderFacade,
};
use entity_store::asynchronous::{BatchKey, SubjectHistory};
use ess_conformance::target::TargetError;
use eventlog_core::CaptureLimits;
use serde_json::{json, Value};
use time::OffsetDateTime;

use crate::{
    common::{self, Fields, TempRoot},
    executor,
    store::{self, read, response, serialize, subject, Failure, Result},
};

const PREFIX: &str = "ess_provider";
const LIMITS: CaptureLimits = CaptureLimits {
    max_events: 1024,
    max_blobs: 8192,
    max_projection_rows: 8192,
    max_payload_bytes: 64 * 1024 * 1024,
};

/// The one hand-written transient enum must match the compiler's complete declaration.
pub fn check_model(model: &str) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let model: Value = serde_json::from_str(model)?;
    if model["domains"].get("entity-provider.tracking").is_some() {
        let declaration = model["types"]
            .get("entity-provider.tracking.CapturePolicy")
            .ok_or("provider specification must declare CapturePolicy")?;
        let variants = CapturePolicy::ALL.map(|policy| match policy {
            CapturePolicy::FullVerification => "FullVerification",
            CapturePolicy::ProviderTracked => "ProviderTracked",
        });
        if declaration["body"] != json!({"kind":"enum","variants":variants}) {
            return Err("CapturePolicy differs from the complete compiled ESS enum".into());
        }
    }
    Ok(())
}

pub struct Target {
    facade: Option<RecordedProviderFacade>,
    authority: Option<Authority>,
    registry: Registry,
    root: TempRoot,
}

impl Target {
    pub fn new() -> std::result::Result<Self, TargetError> {
        Ok(Self {
            facade: None,
            authority: None,
            registry: Registry::new(),
            root: TempRoot::new()?,
        })
    }
    fn path(&self) -> String {
        self.root
            .0
            .join("provider.sqlite3")
            .to_string_lossy()
            .into_owned()
    }
    fn facade(&self) -> Result<&RecordedProviderFacade> {
        self.facade
            .as_ref()
            .ok_or_else(|| Failure::named("NotOpen", "provider is not open"))
    }
    fn close(&mut self) {
        if let Some(mut facade) = self.facade.take() {
            let _ = facade.shutdown(ShutdownMode::Drain, CallWait::Forever);
        }
    }
    fn reopen(&mut self, policy: CapturePolicy) -> Result<Value> {
        let authority = self
            .authority
            .clone()
            .ok_or_else(|| Failure::named("NotProvisioned", "no authority"))?;
        self.close();
        self.facade = Some(
            RecordedProviderFacade::start_with_read_policy(
                self.registry.clone(),
                EventlogRecordedStoreOwner::Sqlite {
                    path: self.path(),
                    prefix: PREFIX.into(),
                    authority,
                    limits: LIMITS,
                },
                bridge(),
                policy,
            )
            .map_err(start_error)?,
        );
        Ok(Value::Null)
    }
    pub fn execute(
        &mut self,
        operation: &str,
        input: &Fields,
    ) -> std::result::Result<Fields, TargetError> {
        let request = common::text(input, "request")?;
        let result = serde_json::from_str::<Value>(request)
            .map_err(Failure::from)
            .and_then(|value| self.call(operation, &value));
        response(result)
    }
    fn call(&mut self, operation: &str, value: &Value) -> Result<Value> {
        match operation {
            "Register" => {
                if self.facade.is_some() {
                    return Err(Failure::named(
                        "AlreadyOpen",
                        "register definitions before opening",
                    ));
                }
                let definition: EntityDefinition = read(value, "definition")?;
                self.registry.register(definition).map_err(Failure::debug)?;
                Ok(Value::Null)
            }
            "Provision" => {
                let policy = policy(value)?;
                if self.authority.is_some() {
                    return Err(Failure::named(
                        "AlreadyProvisioned",
                        "fixture already has an authority",
                    ));
                }
                let facade = RecordedProviderFacade::provision(
                    self.registry.clone(),
                    EventlogRecordedStoreProvisioner::Sqlite {
                        path: self.path(),
                        prefix: PREFIX.into(),
                        authority: ProvisionAuthority {
                            logical_scope: "ess-scope".into(),
                            tenant: "ess-tenant".into(),
                            expected_stream_identity: None,
                        },
                        limits: LIMITS,
                    },
                    context("provision"),
                    bridge(),
                )
                .map_err(start_error)?;
                self.authority = Some(facade.authority().clone());
                self.facade = Some(facade);
                self.reopen(policy)
            }
            "Reopen" => self.reopen(policy(value)?),
            "Create" => {
                let request = executor::create(value)?;
                let outcome = self
                    .facade()?
                    .create(
                        context(&request.recording.record_id),
                        request,
                        CallWait::Forever,
                    )
                    .map_err(execution_error)?;
                Ok(store::outcome(outcome))
            }
            "Batch" => {
                let key: BatchKey = read(value, "key")?;
                let actions = executor::actions(value)?;
                let outcome = self
                    .facade()?
                    .execute_batch(context("batch"), key, actions, CallWait::Forever)
                    .map_err(execution_error)?;
                Ok(store::outcome(outcome))
            }
            "Load" => serialize(
                self.facade()?
                    .scoped()
                    .load_recorded(&subject(value)?, CallWait::Forever)
                    .map_err(read_error)?,
            ),
            "Histories" => {
                let subjects = read::<Vec<Value>>(value, "subjects")?
                    .iter()
                    .map(subject)
                    .collect::<Result<Vec<_>>>()?;
                let histories = self
                    .facade()?
                    .scoped()
                    .read_histories(&subjects, CallWait::Forever)
                    .map_err(read_error)?;
                Ok(Value::Array(
                    histories.iter().map(history_summary).collect(),
                ))
            }
            "LookupBatch" => {
                let batch = self
                    .facade()?
                    .scoped()
                    .lookup_batch(&read(value, "key")?, CallWait::Forever)
                    .map_err(read_error)?;
                // Presence comes from the actual lookup; receipt coordinates remain covered by executor cases.
                Ok(json!({"present":batch.is_some()}))
            }
            "Snapshot" => {
                let snapshot = self
                    .facade()?
                    .complete_snapshot(CallWait::Forever)
                    .map_err(read_error)?;
                Ok(Value::Array(
                    snapshot
                        .histories
                        .iter()
                        .map(|snapshot| history_summary(&snapshot.history))
                        .collect(),
                ))
            }
            "SqlMutate" => {
                let kind: String = read(value, "kind")?;
                let sql = match kind.as_str() {
                    "blob" => "UPDATE ess_provider_blobs SET bytes=zeroblob(length(bytes)) WHERE rowid=(SELECT MIN(rowid) FROM ess_provider_blobs)",
                    "event" => "UPDATE ess_provider_events SET data='{}' WHERE global_seq=(SELECT MAX(global_seq) FROM ess_provider_events)",
                    "projection" => "UPDATE ess_provider_p_er_subjects_v1 SET body='{}'",
                    "identity" => "UPDATE ess_provider_identity SET stream_identity='substituted-generation'",
                    "delete-blob" => "DELETE FROM ess_provider_blobs WHERE rowid=(SELECT MIN(rowid) FROM ess_provider_blobs)",
                    _ => return Err(Failure::named("Deserialize", "unknown SQL mutation fixture")),
                };
                let connection = rusqlite::Connection::open(self.path())
                    .map_err(|e| Failure::named("Fixture", e))?;
                let before: i64 = connection
                    .query_row(
                        "SELECT MAX(global_seq) FROM ess_provider_events",
                        [],
                        |row| row.get(0),
                    )
                    .map_err(|e| Failure::named("Fixture", e))?;
                let changed = connection
                    .execute(sql, [])
                    .map_err(|e| Failure::named("Fixture", e))?;
                let after: i64 = connection
                    .query_row(
                        "SELECT MAX(global_seq) FROM ess_provider_events",
                        [],
                        |row| row.get(0),
                    )
                    .map_err(|e| Failure::named("Fixture", e))?;
                Ok(json!({"changed":changed,"same_event_head":before==after}))
            }
            _ => Err(Failure::named("Unsupported", operation)),
        }
    }
}
impl Drop for Target {
    fn drop(&mut self) {
        self.close();
    }
}
fn history_summary(history: &SubjectHistory) -> Value {
    let records: Vec<_> = history
        .records
        .iter()
        .map(|record| {
            json!({"record_id":record.entry.record_id(),"revision":record.entry.revision()})
        })
        .collect();
    json!({"entity":history.subject.entity,"id":history.subject.id,"records":records})
}
fn policy(value: &Value) -> Result<CapturePolicy> {
    match read::<String>(value, "policy")?.as_str() {
        "FullVerification" => Ok(CapturePolicy::FullVerification),
        "ProviderTracked" => Ok(CapturePolicy::ProviderTracked),
        _ => Err(Failure::named("Deserialize", "unknown capture policy")),
    }
}
fn bridge() -> BridgeConfig {
    BridgeConfig {
        queue_capacity: NonZeroU16::new(8).expect("nonzero constant"),
    }
}
fn context(id: &str) -> EventlogOperationContext {
    EventlogOperationContext {
        subject: "ess-provider".into(),
        actor: "ess-runner".into(),
        request_id: id.into(),
        trace_id: id.into(),
        causation_id: None,
        causation_depth: 0,
        occurred_at: OffsetDateTime::UNIX_EPOCH,
    }
}
fn read_error(error: SyncReadError) -> Failure {
    match error {
        SyncReadError::Store(error) => error.into(),
        other => Failure::named("BridgeRead", format!("{other:?}")),
    }
}
fn start_error(error: BridgeStartError) -> Failure {
    match error {
        BridgeStartError::Open(error) => error.into(),
        other => Failure::named("BridgeStart", format!("{other:?}")),
    }
}
fn execution_error(error: SyncExecutionError) -> Failure {
    match error {
        SyncExecutionError::Execution(error) => executor::execution_error(error),
        other => Failure::named("BridgeExecution", format!("{other:?}")),
    }
}
