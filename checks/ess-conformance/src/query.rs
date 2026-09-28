//! Real query APIs; exact wire documents never pass through a binary floating-point codec.
use crate::common::{self, fields, integer, json, string, Fields};
use entity_core::{EntityDefinition, EntityInstance, Registry, Runtime};
use entity_query::{
    query_ordered_instances, DocumentPage, DocumentQuery, DocumentQueryProvider, QueryCursor,
    QueryError,
};
use entity_store::{Expect, MemoryStore, Store};
use ess_conformance::target::TargetError;
use ess_primitives::node::Node;
use serde::Deserialize;
use serde_json::Value;

pub struct Target {
    store: MemoryStore,
    continuation: Option<QueryCursor>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Subject {
    id: String,
    fields: Value,
}

impl Target {
    pub fn new() -> Result<Self, TargetError> {
        Ok(Self {
            store: MemoryStore::new(),
            continuation: None,
        })
    }

    pub fn execute(&mut self, operation: &str, input: &Fields) -> Result<Fields, TargetError> {
        match operation {
            "Seed" => self.seed(input),
            "Query" | "Continue" => {
                let mut query: DocumentQuery = match decode(input, "query") {
                    Ok(value) => value,
                    Err(result) => return Ok(result),
                };
                if operation == "Continue" {
                    let Some(cursor) = self.continuation.clone() else {
                        return failure(
                            "continuation_unavailable",
                            "no preceding page supplied a continuation",
                        );
                    };
                    query.after = Some(cursor);
                }
                let result = self.store.query_documents(&query);
                self.page(result)
            }
            "Ordered" => {
                let query = match decode(input, "query") {
                    Ok(value) => value,
                    Err(result) => return Ok(result),
                };
                let instances: Vec<EntityInstance> = match decode(input, "instances") {
                    Ok(value) => value,
                    Err(result) => return Ok(result),
                };
                self.page(query_ordered_instances(&query, instances))
            }
            "Page" => {
                let query = match decode(input, "query") {
                    Ok(value) => value,
                    Err(result) => return Ok(result),
                };
                let instances = match decode(input, "instances") {
                    Ok(value) => value,
                    Err(result) => return Ok(result),
                };
                self.page(DocumentPage::from_matches(&query, instances))
            }
            "DecodeCursor" => {
                let query: DocumentQuery = match decode(input, "query") {
                    Ok(value) => value,
                    Err(result) => return Ok(result),
                };
                match query.after_id() {
                    Ok(id) => {
                        let mut result = success();
                        result.insert("after".into(), string(id));
                        Ok(result)
                    }
                    Err(error) => query_failure(error),
                }
            }
            "Limit" => {
                let query: DocumentQuery = match decode(input, "query") {
                    Ok(value) => value,
                    Err(result) => return Ok(result),
                };
                match query.effective_limit() {
                    Ok(limit) => {
                        let mut result = success();
                        result.insert("count".into(), integer(limit as i64));
                        Ok(result)
                    }
                    Err(error) => query_failure(error),
                }
            }
            other => Err(common::unavailable(format!(
                "unknown entity.query operation {other}"
            ))),
        }
    }

    fn seed(&mut self, input: &Fields) -> Result<Fields, TargetError> {
        let definition: EntityDefinition = match decode(input, "definition") {
            Ok(value) => value,
            Err(result) => return Ok(result),
        };
        let subjects: Vec<Subject> = match decode(input, "subjects") {
            Ok(value) => value,
            Err(result) => return Ok(result),
        };
        let entity = definition.entity.clone();
        let version = definition.version;
        let mut registry = Registry::new();
        if let Err(error) = registry.register(definition) {
            return failure("definition", error.to_string());
        }
        let mut instances = Vec::new();
        for subject in subjects {
            let decision = match Runtime::new(&registry).create(
                &entity,
                version,
                subject.id,
                subject.fields,
            ) {
                Ok(value) => value,
                Err(error) => return failure(error.kind(), error.to_string()),
            };
            if let Err(error) = self.store.commit(&decision, Expect::Absent) {
                return failure("store", error.to_string());
            }
            instances.push(decision.instance);
        }
        let mut result = success();
        result.insert("count".into(), integer(instances.len() as i64));
        result.insert("value".into(), json(&instances)?);
        result.insert(
            "ids".into(),
            json(
                &instances
                    .iter()
                    .map(|instance| &instance.id)
                    .collect::<Vec<_>>(),
            )?,
        );
        Ok(result)
    }

    fn page(&mut self, result: Result<DocumentPage, QueryError>) -> Result<Fields, TargetError> {
        match result {
            Ok(page) => {
                self.continuation.clone_from(&page.next);
                let mut result = success();
                result.insert(
                    "ids".into(),
                    json(
                        &page
                            .items
                            .iter()
                            .map(|instance| &instance.id)
                            .collect::<Vec<_>>(),
                    )?,
                );
                result.insert("count".into(), integer(page.items.len() as i64));
                result.insert("has_next".into(), Node::Bool(page.next.is_some()));
                result.insert(
                    "cursor".into(),
                    string(page.next.as_ref().map(QueryCursor::as_str).unwrap_or("")),
                );
                result.insert("value".into(), json(&page)?);
                Ok(result)
            }
            Err(error) => query_failure(error),
        }
    }
}

fn success() -> Fields {
    fields([
        ("ok", Node::Bool(true)),
        ("kind", string("")),
        ("detail", string("")),
        ("value", string("null")),
        ("ids", string("[]")),
        ("count", integer(0)),
        ("has_next", Node::Bool(false)),
        ("cursor", string("")),
        ("after", string("")),
    ])
}

fn failure(kind: &str, detail: impl Into<String>) -> Result<Fields, TargetError> {
    let mut result = success();
    result.insert("ok".into(), Node::Bool(false));
    result.insert("kind".into(), string(kind));
    result.insert("detail".into(), string(detail));
    Ok(result)
}

fn query_failure(error: QueryError) -> Result<Fields, TargetError> {
    failure(
        match &error {
            QueryError::Invalid(_) => "invalid",
            QueryError::Store(_) => "store",
        },
        error.to_string(),
    )
}

// A malformed caller document is an observed decoding refusal, not an unavailable adapter.
fn decode<T: serde::de::DeserializeOwned>(input: &Fields, field: &str) -> Result<T, Fields> {
    let parsed = common::text(input, field)
        .map_err(|error| format!("{error:?}"))
        .and_then(|text| serde_json::from_str(text).map_err(|error| error.to_string()));
    parsed.map_err(|detail| {
        failure("decode", detail).expect("constructing a refusal performs no fallible operation")
    })
}
