//! Direct observations of the synchronous and asynchronous storage libraries.
//!
//! Complete record inputs are authored fixtures. The adapters call the public storage
//! methods without deriving expected values or claiming additional provider capabilities.
use entity_core::{Decision, EntityDefinition, EntityInstance};
use entity_executor::test_support::block_on;
use entity_store::{
    asynchronous::{self as a, AsyncRecordedReader, AsyncRecordedWriter, AsyncStateReader},
    AtomicBatchStore, AtomicCommit, Expect, FileStore, LegacyStoreSource, MemoryStore,
    RecordedCommit, RecordedObservation, Recording, StoreError,
};
use ess_conformance::target::TargetError;
use serde::{de::DeserializeOwned, Deserialize};
use serde_json::{json, Value};

use crate::common::{self, Fields, TempRoot};

/// One scenario owns every provider and all scratch files it can observe.
pub struct Target {
    memory: MemoryStore,
    file: FileStore,
    recorded: a::MemoryRecordedStore,
    root: TempRoot,
}

#[derive(Debug)]
pub(super) struct Failure {
    code: String,
    value: Value,
}

pub(super) type Result<T> = std::result::Result<T, Failure>;

impl Failure {
    pub(super) fn named(code: &str, detail: impl ToString) -> Self {
        Self {
            code: code.into(),
            value: json!({"detail":detail.to_string()}),
        }
    }
    pub(super) fn debug(error: impl std::fmt::Debug + std::fmt::Display) -> Self {
        let debug = format!("{error:?}");
        let code = debug.split([' ', '{', '(']).next().unwrap_or("Error");
        Self::named(code, error)
    }
}

impl From<serde_json::Error> for Failure {
    fn from(error: serde_json::Error) -> Self {
        Self::named("Deserialize", error)
    }
}
impl From<StoreError> for Failure {
    fn from(error: StoreError) -> Self {
        let mut failure = Self::debug(&error);
        match error {
            StoreError::RevisionConflict {
                entity,
                id,
                expected,
                found,
            } => {
                failure.value["entity"] = json!(entity);
                failure.value["id"] = json!(id);
                failure.value["expected"] = expect_value(expected);
                failure.value["found"] = json!(found);
            }
            StoreError::RecordConflict { record_id } => {
                failure.value["record_id"] = json!(record_id)
            }
            StoreError::Unreachable { provider, .. } => failure.value["provider"] = json!(provider),
            StoreError::Backend(_) => {}
        }
        failure
    }
}
impl From<a::AsyncStoreError> for Failure {
    fn from(error: a::AsyncStoreError) -> Self {
        let mut failure = Self::debug(&error);
        match error {
            a::AsyncStoreError::RevisionConflict {
                subject,
                expected,
                found,
            } => {
                failure.value["subject"] = json!(subject);
                failure.value["expected"] = expect_value(expected);
                failure.value["found"] = json!(found);
            }
            a::AsyncStoreError::RecordConflict { record_id }
            | a::AsyncStoreError::DuplicateRecordId { record_id }
            | a::AsyncStoreError::HistoricalRetryUnverifiable { record_id, .. } => {
                failure.value["record_id"] = json!(record_id)
            }
            a::AsyncStoreError::BatchConflict { key } => failure.value["key"] = json!(key),
            a::AsyncStoreError::PreviouslyRecordedBatchEntries { indices } => {
                failure.value["indices"] = json!(indices)
            }
            a::AsyncStoreError::CorruptHistory { subject, .. } => {
                failure.value["subject"] = json!(subject)
            }
            a::AsyncStoreError::Forked { subject, heads } => {
                failure.value["subject"] = json!(subject);
                failure.value["heads"] = json!(heads);
            }
            a::AsyncStoreError::Unreachable { provider, .. }
            | a::AsyncStoreError::ProviderIntegrity { provider, .. } => {
                failure.value["provider"] = json!(provider)
            }
            a::AsyncStoreError::PositionExhausted { domain } => {
                failure.value["domain"] = json!(domain)
            }
            a::AsyncStoreError::BatchExceedsReadBounds {
                bound,
                limit,
                would_hold,
            } => {
                failure.value["bound"] = json!(bound);
                failure.value["limit"] = json!(limit);
                failure.value["would_hold"] = json!(would_hold);
            }
            a::AsyncStoreError::InvalidInput(_)
            | a::AsyncStoreError::Encoding(_)
            | a::AsyncStoreError::Backend(_) => {}
        }
        failure
    }
}
impl From<a::WriteFailure> for Failure {
    fn from(error: a::WriteFailure) -> Self {
        match error {
            a::WriteFailure::NotCommitted(error) => {
                let mut failure: Self = error.into();
                failure.value["write_outcome"] = json!("NotCommitted");
                failure
            }
            a::WriteFailure::Uncertain { key, cause } => {
                let mut failure = Self::named("Uncertain", cause);
                failure.value["key"] = json!(key);
                failure.value["write_outcome"] = json!("Uncertain");
                failure
            }
        }
    }
}
impl From<entity_core::CoreError> for Failure {
    fn from(error: entity_core::CoreError) -> Self {
        Self::debug(error)
    }
}
impl From<std::io::Error> for Failure {
    fn from(error: std::io::Error) -> Self {
        Self::named("Io", error)
    }
}

pub(super) fn read<T: DeserializeOwned>(value: &Value, field: &str) -> Result<T> {
    Ok(serde_json::from_value(
        value
            .get(field)
            .cloned()
            .ok_or_else(|| Failure::named("Deserialize", format!("missing {field}")))?,
    )?)
}
pub(super) fn optional<T: DeserializeOwned>(value: &Value, field: &str, fallback: T) -> Result<T> {
    value.get(field).map_or(Ok(fallback), |v| {
        serde_json::from_value(v.clone()).map_err(Into::into)
    })
}
pub(super) fn recording(value: &Value) -> Result<Recording> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Wire {
        record_id: String,
        recorded_at: String,
        correlation: Option<String>,
        causation: Option<String>,
        actor: Option<String>,
    }
    let w: Wire = serde_json::from_value(value.clone())?;
    Ok(Recording {
        record_id: w.record_id,
        recorded_at: w.recorded_at,
        correlation: w.correlation,
        causation: w.causation,
        actor: w.actor,
    })
}
pub(super) fn expect(value: &Value) -> Result<Expect> {
    if value.is_null() {
        Ok(Expect::Absent)
    } else {
        Ok(Expect::Revision(serde_json::from_value(value.clone())?))
    }
}
fn expect_value(value: Expect) -> Value {
    match value {
        Expect::Absent => Value::Null,
        Expect::Revision(n) => json!(n),
    }
}
pub(super) fn response(value: Result<Value>) -> std::result::Result<Fields, TargetError> {
    match value {
        Ok(value) => Ok(common::fields([
            ("status", common::string("ok")),
            ("error", common::string("")),
            ("value", common::json(&value)?),
        ])),
        Err(error) => Ok(common::fields([
            ("status", common::string("error")),
            ("error", common::string(error.code)),
            ("value", common::json(&error.value)?),
        ])),
    }
}
pub(super) fn serialize(value: impl serde::Serialize) -> Result<Value> {
    Ok(serde_json::to_value(value)?)
}

pub(super) fn outcome(value: a::AppendOutcome) -> Value {
    match value {
        a::AppendOutcome::Empty => json!({"kind":"Empty"}),
        a::AppendOutcome::Committed { receipt, replayed } => {
            json!({"kind":"Committed","receipt":receipt,"replayed":replayed})
        }
        a::AppendOutcome::Historical {
            evidence,
            assurance,
        } => {
            json!({"kind":"Historical","evidence":evidence,"assurance":subject_assurance(assurance)})
        }
    }
}
pub(super) fn subject_assurance(value: a::SubjectAssurance) -> Value {
    match value {
        a::SubjectAssurance::VerifiedFromGenesis { subject } => {
            json!({"kind":"VerifiedFromGenesis","subject":subject})
        }
        a::SubjectAssurance::VerifiedAfterBoundary {
            subject,
            anchor_revision,
        } => {
            json!({"kind":"VerifiedAfterBoundary","subject":subject,"anchor_revision":anchor_revision})
        }
    }
}
pub(super) fn assurance(value: a::StoreAssurance) -> Value {
    json!({"scope":value.scope,"coverage":format!("{:?}",value.coverage),"subjects":value.subjects.into_iter().map(subject_assurance).collect::<Vec<_>>()})
}
pub(super) fn history(value: &a::SubjectHistory) -> Value {
    json!({"subject":value.subject,"origin":value.origin,"records":value.records.iter().map(stored_record).collect::<Vec<_>>()})
}
pub(super) fn stored_record(value: &a::StoredRecord) -> Value {
    json!({"entry":value.entry,"position":value.position,"receipt":value.receipt,"expect":match value.expect { Expect::Absent=>Value::Null,Expect::Revision(n)=>json!(n)},"request_bytes":String::from_utf8_lossy(&value.request_bytes),"record_bytes":String::from_utf8_lossy(&value.record_bytes),"lineage":value.lineage.as_ref().map(|l|json!({"digest":l.digest,"parents":l.parents}))})
}
pub(super) fn lookup(value: Option<a::RecordLookup>) -> Value {
    match value {
        None => Value::Null,
        Some(a::RecordLookup::Committed(record)) => {
            json!({"kind":"Committed","record":stored_record(&record)})
        }
        Some(a::RecordLookup::Imported(evidence)) => json!({"kind":"Imported","evidence":evidence}),
    }
}
pub(super) fn subject(value: &Value) -> Result<a::Subject> {
    // Public fields deliberately preserve malformed values until the selected real API checks them.
    Ok(a::Subject {
        entity: read(value, "entity")?,
        id: read(value, "id")?,
    })
}
pub(super) fn history_from(value: &Value) -> Result<a::SubjectHistory> {
    let mut records = Vec::new();
    for record in read::<Vec<Value>>(value, "records")? {
        let entry: a::RecordedEntry = read(&record, "entry")?;
        let request_bytes = match record.get("request_bytes") {
            Some(v) => serde_json::from_value::<String>(v.clone())?.into_bytes(),
            None => a::original_request_comparison_bytes(&entry)?,
        };
        let record_bytes = match record.get("record_bytes") {
            Some(v) => serde_json::from_value::<String>(v.clone())?.into_bytes(),
            None => a::record_comparison_bytes(&entry)?,
        };
        records.push(a::StoredRecord {
            entry,
            position: read(&record, "position")?,
            receipt: read(&record, "receipt")?,
            expect: expect(&record["expect"])?,
            request_bytes,
            record_bytes,
            lineage: match record.get("lineage").filter(|v| !v.is_null()) {
                None => None,
                Some(v) => Some(Box::new(a::Lineage {
                    digest: read(v, "digest")?,
                    parents: read(v, "parents")?,
                })),
            },
        });
    }
    Ok(a::SubjectHistory {
        subject: read(value, "subject")?,
        origin: read(value, "origin")?,
        records,
    })
}

impl Target {
    pub fn new() -> std::result::Result<Self, TargetError> {
        let root = TempRoot::new()?;
        Ok(Self {
            memory: MemoryStore::new(),
            file: FileStore::open(root.0.join("file")),
            recorded: a::MemoryRecordedStore::new(),
            root,
        })
    }
    fn sync(&self, provider: &str) -> Result<&dyn entity_store::RecordedStore> {
        match provider {
            "Memory" => Ok(&self.memory),
            "File" => Ok(&self.file),
            _ => Err(Failure::named(
                "Provider",
                "this synchronous operation requires Memory or File",
            )),
        }
    }
    fn sync_mut(&mut self, provider: &str) -> Result<&mut dyn entity_store::RecordedStore> {
        match provider {
            "Memory" => Ok(&mut self.memory),
            "File" => Ok(&mut self.file),
            _ => Err(Failure::named(
                "Provider",
                "this synchronous operation requires Memory or File",
            )),
        }
    }
    pub fn execute(
        &mut self,
        operation: &str,
        input: &Fields,
    ) -> std::result::Result<Fields, TargetError> {
        let request = match serde_json::from_str::<Value>(common::text(input, "request")?) {
            Ok(value) => value,
            Err(error) => return response(Err(error.into())),
        };
        response(self.invoke(operation, &request))
    }
    fn invoke(&mut self, operation: &str, request: &Value) -> Result<Value> {
        let provider = optional::<String>(request, "provider", "Memory".into())?;
        match operation {
            "Commit" => {
                let decision: Decision = read(request, "decision")?;
                self.sync_mut(&provider)?
                    .commit(&decision, expect(&request["expect"])?)?;
                Ok(Value::Null)
            }
            "CommitRecorded" => {
                let commit: RecordedCommit = read(request, "commit")?;
                self.sync_mut(&provider)?
                    .commit_recorded(&commit, expect(&request["expect"])?)?;
                Ok(Value::Null)
            }
            "Observe" => {
                let observation: RecordedObservation = read(request, "observation")?;
                self.sync_mut(&provider)?.observe(&observation)?;
                Ok(Value::Null)
            }
            "Load" => {
                let subject = subject(request)?;
                if provider == "RecordedMemory" {
                    serialize(block_on(AsyncStateReader::load(&self.recorded, &subject))?)
                } else {
                    serialize(self.sync(&provider)?.load(&subject.entity, &subject.id)?)
                }
            }
            "Ids" => serialize(
                self.sync(&provider)?
                    .ids(&read::<String>(request, "entity")?)?,
            ),
            "Events" | "Records" | "Observations" => {
                let subject = subject(request)?;
                let store = self.sync(&provider)?;
                match operation {
                    "Events" => serialize(store.events(&subject.entity, &subject.id)?),
                    "Records" => serialize(store.records(&subject.entity, &subject.id)?),
                    _ => serialize(store.observations(&subject.entity, &subject.id)?),
                }
            }
            "AtomicBatch" => {
                let mut commits = Vec::new();
                for value in read::<Vec<Value>>(request, "commits")? {
                    commits.push(AtomicCommit::new(
                        read(&value, "decision")?,
                        expect(&value["expect"])?,
                    ));
                }
                if provider != "Memory" {
                    return Err(Failure::named(
                        "Capability",
                        "FileStore does not implement AtomicBatchStore",
                    ));
                }
                self.memory.commit_batch(&commits)?;
                Ok(Value::Null)
            }
            "Reopen" => {
                self.file = FileStore::open(self.root.0.join("file"));
                Ok(Value::Null)
            }
            "WriteFileFixture" => {
                let path = confined(&self.root.0, &read::<String>(request, "path")?)?;
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(path, read::<String>(request, "bytes")?)?;
                Ok(Value::Null)
            }
            "ReadFileFixture" => {
                let path = confined(&self.root.0, &read::<String>(request, "path")?)?;
                serialize(std::fs::read_to_string(path)?)
            }
            "FileExists" => {
                let path = confined(&self.root.0, &read::<String>(request, "path")?)?;
                serialize(path.try_exists()?)
            }
            "Project" => {
                let definition: EntityDefinition = read(request, "definition")?;
                let instances: Vec<EntityInstance> = read(request, "instances")?;
                serialize(entity_store::project(&definition, &instances))
            }
            "MigrateFile" => {
                let source = confined(&self.root.0, &read::<String>(request, "source")?)?;
                let destination = confined(&self.root.0, &read::<String>(request, "destination")?)?;
                serialize(entity_store::migrate_file_store_v1(
                    source,
                    destination,
                    read(request, "dry_run")?,
                )?)
            }
            "AcquireLegacy" => {
                let source = self
                    .file
                    .acquire_legacy(&read::<String>(request, "source_id")?)?;
                Ok(
                    json!({"source_id":source.source_id,"histories":source.histories.iter().map(history).collect::<Vec<_>>()}),
                )
            }
            "Seal" => {
                let metadata = recording(&request["recording"])?;
                serialize(
                    metadata
                        .seal(read::<Value>(request, "value")?)
                        .map_err(Failure::debug)?,
                )
            }
            "ValidateEnvelope" => {
                let envelope: entity_store::Envelope<Value> = read(request, "envelope")?;
                envelope.validate().map_err(Failure::debug)?;
                serialize(envelope)
            }
            "ValidateRecordedCommit" => {
                let commit: RecordedCommit = read(request, "commit")?;
                commit.validate()?;
                serialize(commit)
            }
            "ValidateObservation" => {
                let observation: RecordedObservation = read(request, "observation")?;
                observation.validate()?;
                serialize(observation)
            }
            "RecordFraming" => serialize(a::record_framing(
                read::<String>(request, "bytes")?.as_bytes(),
            )?),
            "ReadRecordDomain" => serialize(a::read_record_in_domain(
                &read::<String>(request, "domain")?,
                read::<String>(request, "bytes")?.as_bytes(),
            )?),
            "RecordDomains" => {
                let entry: a::RecordedEntry = read(request, "entry")?;
                Ok(json!({"record":a::record_domain(&entry),"request":a::request_domain(&entry)}))
            }
            "RequestBytes" => {
                let entry: a::RecordedEntry = read(request, "entry")?;
                serialize(
                    String::from_utf8(a::original_request_comparison_bytes(&entry)?)
                        .map_err(|e| Failure::named("Encoding", e))?,
                )
            }
            "SubjectCoordinate" => {
                let s = a::Subject::new(
                    read::<String>(request, "entity")?,
                    read::<String>(request, "id")?,
                )?;
                serialize(s.coordinate_id())
            }
            "BatchCoordinate" => serialize(read::<a::BatchKey>(request, "key")?.coordinate_id()?),
            "MemberCoordinate" => serialize(a::member_id(
                &read(request, "key")?,
                read(request, "index")?,
            )?),
            "CheckExpectation" => {
                let entity: String = read(request, "entity")?;
                let id: String = read(request, "id")?;
                entity_store::check(
                    &entity,
                    &id,
                    expect(&request["expect"])?,
                    read(request, "found")?,
                )?;
                Ok(Value::Null)
            }
            "Append" => {
                let key: a::BatchKey = read(request, "key")?;
                let mut members = Vec::new();
                for m in read::<Vec<Value>>(request, "members")? {
                    let entry: a::RecordedEntry = read(&m, "entry")?;
                    let request_bytes = optional::<String>(
                        &m,
                        "request_bytes",
                        String::from_utf8(a::original_request_comparison_bytes(&entry)?)
                            .map_err(|e| Failure::named("Encoding", e))?,
                    )?;
                    members.push(a::AppendMember::new(
                        expect(&m["expect"])?,
                        entry,
                        request_bytes.into_bytes(),
                    ));
                }
                let mut append = a::AppendRequest::new(key, members)?;
                // Public fields are intentionally mutable. This explicitly supplied transport
                // mutation tests writer-entry validation, not a constructor-only path.
                if let Some(index) = request.get("duplicate_member").and_then(Value::as_u64) {
                    let member = append
                        .members
                        .get(index as usize)
                        .ok_or_else(|| {
                            Failure::named("Deserialize", "member index outside request")
                        })?
                        .clone();
                    append.members.push(member);
                }
                Ok(outcome(block_on(self.recorded.append(append))?))
            }
            "SeedImported" => {
                self.recorded
                    .seed_imported(history_from(&request["history"])?)?;
                Ok(Value::Null)
            }
            "LookupRecord" => Ok(lookup(block_on(self.recorded.lookup_record(&read::<
                String,
            >(
                request,
                "record_id",
            )?))?)),
            "LookupBatch" => {
                let batch = block_on(self.recorded.lookup_batch(&read(request, "key")?))?;
                Ok(batch.map_or(Value::Null,|b|json!({"key":b.key,"records":b.records.iter().map(stored_record).collect::<Vec<_>>(),"receipt":b.receipt,"comparison_bytes":String::from_utf8_lossy(&b.comparison_bytes)})))
            }
            "History" => Ok(history(&block_on(
                self.recorded.history(&subject(request)?),
            )?)),
            "VerifyComplete" => Ok(assurance(block_on(a::verify_complete_store(
                &self.recorded,
                &read::<String>(request, "scope")?,
            ))?)),
            "VerifySubject" => {
                let s = subject(request)?;
                let h = block_on(self.recorded.history(&s))?;
                let terminal = block_on(AsyncStateReader::load(&self.recorded, &s))?
                    .ok_or_else(|| Failure::named("Absent", "subject is absent"))?;
                Ok(subject_assurance(a::verify_subject_history(&h, &terminal)?))
            }
            "VerifyExplicit" => {
                let snapshot = block_on(
                    self.recorded
                        .complete_snapshot(&read::<String>(request, "scope")?),
                )?;
                let coverage = if read::<bool>(request, "complete_marker")? {
                    a::StoreCoverage::CompleteSnapshot
                } else {
                    a::StoreCoverage::ExplicitSet
                };
                Ok(assurance(a::verify_store_histories(
                    &a::CompleteStoreSnapshot {
                        coverage,
                        ..snapshot
                    },
                )?))
            }
            "BranchHeads" => {
                let h = history_from(&request["history"])?;
                serialize(a::branch_heads(&h)?)
            }
            "BranchTips" => serialize(a::branch_tips(&history_from(&request["history"])?)),
            "Trace" => serialize(self.recorded.trace()),
            "ClearTrace" => {
                self.recorded.clear_trace();
                Ok(Value::Null)
            }
            "SetPosition" => {
                self.recorded
                    .set_next_store_position_for_test(read(request, "position")?);
                Ok(Value::Null)
            }
            "Tamper" => {
                let record_id: String = read(request, "record_id")?;
                match read::<String>(request, "field")?.as_str() {
                    "result" => self.recorded.tamper_decision_result_for_test(&record_id),
                    "event" => self.recorded.tamper_record_event_for_test(&record_id),
                    "receipt_subject" => self.recorded.tamper_receipt_subject_for_test(
                        &record_id,
                        &read::<String>(request, "id")?,
                    ),
                    "definition" => self
                        .recorded
                        .remove_imported_definition_for_test(&record_id),
                    _ => {
                        return Err(Failure::named(
                            "Deserialize",
                            "unknown documented tamper control",
                        ))
                    }
                }
                Ok(Value::Null)
            }
            "CanonicalBytes" => {
                let bytes = a::canonical_domain_bytes(
                    &read::<String>(request, "domain")?,
                    read(request, "value")?,
                )?;
                serialize(String::from_utf8(bytes).map_err(|e| Failure::named("Encoding", e))?)
            }
            _ => Err(Failure::named(
                "Operation",
                format!("unknown storage operation {operation}"),
            )),
        }
    }
}

fn confined(root: &std::path::Path, path: &str) -> Result<std::path::PathBuf> {
    let relative = std::path::Path::new(path);
    if relative
        .components()
        .any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        return Err(Failure::named(
            "FixturePath",
            "fixture path must be relative normal components",
        ));
    }
    Ok(root.join(relative))
}
