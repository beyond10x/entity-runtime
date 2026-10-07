//! Real SQLite provider and synchronous facade calls; SQL corruption, a copy of the closed store
//! and a truncated tail are explicit fixtures. These scenarios prove returned semantics and
//! integrity, and how each open verified as the facade reports it. Bounded calls and retained-path
//! cost are verified separately by runtime counter tests and the measured workload.
use std::num::NonZeroU16;

use entity_core::{EntityDefinition, Registry};
use entity_eventlog::{
    sync::{
        BridgeConfig, BridgeStartError, CallWait, EventlogRecordedStoreOwner,
        EventlogRecordedStoreProvisioner, ProvisionAuthority, ShutdownMode, SyncExecutionError,
        SyncReadError,
    },
    Authority, CapturePolicy, EventlogOperationContext, OpenVerification, RecordedProviderFacade,
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
    fn kept(&self) -> std::path::PathBuf {
        self.root.0.join("kept.sqlite3")
    }
    fn owner(&self) -> Result<EventlogRecordedStoreOwner> {
        let authority = self
            .authority
            .clone()
            .ok_or_else(|| Failure::named("NotProvisioned", "no authority"))?;
        Ok(EventlogRecordedStoreOwner::Sqlite {
            path: self.path(),
            prefix: PREFIX.into(),
            authority,
            limits: LIMITS,
        })
    }
    /// Reports how the open verified the authority, as the facade itself reports it.
    fn reopen(&mut self, policy: CapturePolicy) -> Result<Value> {
        let owner = self.owner()?;
        self.close();
        let facade = RecordedProviderFacade::start_with_read_policy(
            self.registry.clone(),
            owner,
            bridge(),
            policy,
        )
        .map_err(start_error)?;
        let opened = match facade.open_verification() {
            OpenVerification::Complete => json!({"open":"complete"}),
            OpenVerification::Checkpoint => json!({"open":"checkpoint"}),
            OpenVerification::Suffix { events } => json!({"open":"suffix","events":events}),
        };
        self.facade = Some(facade);
        Ok(opened)
    }
    /// The SQLite file and its write-ahead companions, which together are one store state.
    fn files(path: &std::path::Path) -> [std::path::PathBuf; 3] {
        let name = |suffix: &str| {
            let mut name = path.as_os_str().to_owned();
            name.push(suffix);
            std::path::PathBuf::from(name)
        };
        [path.to_owned(), name("-wal"), name("-shm")]
    }
    /// Copies one closed store state over another, companions included or removed.
    fn copy_state(from: &std::path::Path, to: &std::path::Path) -> Result<()> {
        for (source, target) in Self::files(from).iter().zip(Self::files(to).iter()) {
            if source.exists() {
                std::fs::copy(source, target).map_err(|e| Failure::named("Fixture", e))?;
            } else if target.exists() {
                std::fs::remove_file(target).map_err(|e| Failure::named("Fixture", e))?;
            }
        }
        Ok(())
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
            "EnableDurableCheckpoints" => {
                self.facade()?
                    .enable_durable_open_checkpoints(CallWait::Forever)
                    .map_err(read_error)?;
                Ok(Value::Null)
            }
            "DiscardCheckpoint" => {
                let owner = self.owner()?;
                self.close();
                let discarded = owner.discard_open_checkpoint().map_err(Failure::from)?;
                Ok(json!({"discarded":discarded}))
            }
            "KeepCopy" => {
                self.owner()?;
                self.close();
                Self::copy_state(std::path::Path::new(&self.path()), &self.kept())?;
                Ok(Value::Null)
            }
            "TruncateTail" => {
                self.owner()?;
                self.close();
                let path = self.path();
                let fixture = |e: rusqlite::Error| Failure::named("Fixture", e);
                // The current checkpoint rows, read before the kept copy replaces the file.
                let current = rusqlite::Connection::open(&path).map_err(fixture)?;
                let snapshots: Vec<(String, String, i64, i64, String, String)> = current
                    .prepare(&format!("SELECT tenant_id, stream_id, version, state_schema_version, state, recorded_at FROM {PREFIX}_snapshots WHERE stream_type='er.open-checkpoint'"))
                    .and_then(|mut statement| {
                        statement
                            .query_map([], |row| {
                                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?))
                            })?
                            .collect()
                    })
                    .map_err(fixture)?;
                let generations: Vec<(String, String, String, Option<String>)> = current
                    .prepare(&format!("SELECT tenant_id, stream_id, generation, cached_generation FROM {PREFIX}_snapshot_generations WHERE stream_type='er.open-checkpoint'"))
                    .and_then(|mut statement| {
                        statement
                            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)))?
                            .collect()
                    })
                    .map_err(fixture)?;
                drop(current);
                if snapshots.is_empty() {
                    return Err(Failure::named("Fixture", "no open checkpoint to carry"));
                }
                Self::copy_state(&self.kept(), std::path::Path::new(&path))?;
                let restored = rusqlite::Connection::open(&path).map_err(fixture)?;
                for (tenant, stream, generation, cached) in &generations {
                    restored
                        .execute(
                            &format!("INSERT OR REPLACE INTO {PREFIX}_snapshot_generations (tenant_id, stream_type, stream_id, generation, cached_generation) VALUES (?1, 'er.open-checkpoint', ?2, ?3, ?4)"),
                            rusqlite::params![tenant, stream, generation, cached],
                        )
                        .map_err(fixture)?;
                }
                for (tenant, stream, version, schema, state, recorded) in &snapshots {
                    restored
                        .execute(
                            &format!("INSERT OR REPLACE INTO {PREFIX}_snapshots (tenant_id, stream_type, stream_id, version, state_schema_version, state, recorded_at) VALUES (?1, 'er.open-checkpoint', ?2, ?3, ?4, ?5, ?6)"),
                            rusqlite::params![tenant, stream, version, schema, state, recorded],
                        )
                        .map_err(fixture)?;
                }
                Ok(Value::Null)
            }
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
                    // The open checkpoint changed without recomputing its digest.
                    "checkpoint" => "UPDATE ess_provider_snapshots SET state=json_set(state,'$.position',json_extract(state,'$.position')+1) WHERE stream_type='er.open-checkpoint'",
                    // A write that changes nothing but is still a write the provider did not make.
                    "rewrite-identity" => "UPDATE ess_provider_identity SET stream_identity=stream_identity",
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
