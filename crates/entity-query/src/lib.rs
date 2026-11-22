//! Optional indexed document queries outside the deterministic kernel.
//!
//! A store remains useful with only point reads and writes. Providers serving a central authority
//! may additionally implement [`DocumentQueryProvider`] so callers can page over selected instance
//! documents without enumerating and hydrating an entire entity type.

use std::borrow::Borrow;
use std::collections::BTreeMap;
use std::fmt;

use entity_core::EntityInstance;
use entity_store::{MemoryStore, StoreError};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The default number of documents returned by one query.
pub const DEFAULT_LIMIT: usize = 100;
/// The largest page a provider accepts.
pub const MAX_LIMIT: usize = 1_000;

/// An opaque keyset continuation emitted by a document provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct QueryCursor(String);

impl QueryCursor {
    /// The wire-safe opaque value.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A provider-neutral containment query over one entity type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DocumentQuery {
    /// Entity discriminator stored beside the document.
    pub entity: String,
    /// Recursive JSON containment predicates rooted at instance fields.
    #[serde(default)]
    pub matching: BTreeMap<String, Value>,
    /// Requested page size.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
    /// Continuation returned by a previous identical query.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<QueryCursor>,
}

impl DocumentQuery {
    /// Starts a query for one entity discriminator.
    #[must_use]
    pub fn for_entity(entity: impl Into<String>) -> Self {
        Self {
            entity: entity.into(),
            matching: BTreeMap::new(),
            limit: None,
            after: None,
        }
    }

    /// Adds one recursive field-containment predicate.
    #[must_use]
    pub fn matching(mut self, field: impl Into<String>, value: Value) -> Self {
        self.matching.insert(field.into(), value);
        self
    }

    /// Sets the requested page size.
    #[must_use]
    pub const fn with_limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Continues from a cursor a provider emitted.
    #[must_use]
    pub fn after(mut self, cursor: QueryCursor) -> Self {
        self.after = Some(cursor);
        self
    }

    /// The validated effective page size.
    pub fn effective_limit(&self) -> Result<usize, QueryError> {
        let limit = self.limit.unwrap_or(DEFAULT_LIMIT);
        if !(1..=MAX_LIMIT).contains(&limit) {
            return Err(QueryError::Invalid(format!(
                "query limit must be between 1 and {MAX_LIMIT}"
            )));
        }
        Ok(limit)
    }

    /// The last identity encoded by this query's continuation, or an empty lower bound.
    pub fn after_id(&self) -> Result<String, QueryError> {
        let Some(cursor) = &self.after else {
            return Ok(String::new());
        };
        let (identity, last_id) = cursor
            .0
            .split_once('.')
            .ok_or_else(|| QueryError::Invalid("query cursor is malformed".to_owned()))?;
        if decode_hex(identity)? != self.identity_bytes()? {
            return Err(QueryError::Invalid(
                "query cursor belongs to another query".to_owned(),
            ));
        }
        String::from_utf8(decode_hex(last_id)?)
            .map_err(|_| QueryError::Invalid("query cursor identity is not UTF-8".to_owned()))
    }

    fn identity_bytes(&self) -> Result<Vec<u8>, QueryError> {
        serde_json::to_vec(&(&self.entity, &self.matching))
            .map_err(|error| QueryError::Invalid(error.to_string()))
    }

    fn cursor_after(&self, id: &str) -> Result<QueryCursor, QueryError> {
        Ok(QueryCursor(format!(
            "{}.{}",
            encode_hex(&self.identity_bytes()?),
            encode_hex(id.as_bytes())
        )))
    }
}

/// One stable page of matching instances.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DocumentPage {
    /// Matching documents ordered by opaque identity bytes.
    pub items: Vec<EntityInstance>,
    /// Continuation when more matches follow.
    pub next: Option<QueryCursor>,
}

impl DocumentPage {
    /// Builds a page from at most `limit + 1` ordered matches.
    pub fn from_matches(
        query: &DocumentQuery,
        mut items: Vec<EntityInstance>,
    ) -> Result<Self, QueryError> {
        let limit = query.effective_limit()?;
        let has_more = items.len() > limit;
        if has_more {
            items.truncate(limit);
        }
        let next = if has_more {
            items
                .last()
                .map(|item| query.cursor_after(&item.id))
                .transpose()?
        } else {
            None
        };
        Ok(Self { items, next })
    }
}

/// Why a provider-neutral document query was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryError {
    /// The query or continuation cannot be executed as written.
    Invalid(String),
    /// The underlying store failed.
    Store(StoreError),
}

impl fmt::Display for QueryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(reason) => formatter.write_str(reason),
            Self::Store(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for QueryError {}

impl From<StoreError> for QueryError {
    fn from(value: StoreError) -> Self {
        Self::Store(value)
    }
}

/// Optional provider capability for stable paginated document queries.
pub trait DocumentQueryProvider {
    /// Executes one containment query without requiring callers to enumerate the store.
    fn query_documents(&self, query: &DocumentQuery) -> Result<DocumentPage, QueryError>;
}

impl DocumentQueryProvider for MemoryStore {
    fn query_documents(&self, query: &DocumentQuery) -> Result<DocumentPage, QueryError> {
        // Seek past the cursor and borrow: only the rows the page keeps are copied, never every
        // instance of the type.
        let after = query.after_id()?;
        page_ordered(
            query,
            &after,
            self.instances_after(&query.entity, &after),
            EntityInstance::clone,
        )
    }
}

/// Applies the provider-neutral query semantics to instances already ordered by identity.
///
/// This is the shared fallback for providers whose authoritative complete snapshot supplies state
/// but no native query index. Callers must provide only the requested entity, ordered by `id`.
///
/// # Errors
///
/// Invalid page size/cursor, an out-of-order input, or an instance of another entity.
pub fn query_ordered_instances(
    query: &DocumentQuery,
    instances: impl IntoIterator<Item = EntityInstance>,
) -> Result<DocumentPage, QueryError> {
    let after = query.after_id()?;
    page_ordered(query, &after, instances, |instance| instance)
}

/// The query semantics over owned or borrowed ordered instances; `keep` turns a row the page keeps
/// into an owned document, so a borrowing caller copies only those rows.
fn page_ordered<T: Borrow<EntityInstance>>(
    query: &DocumentQuery,
    after: &str,
    instances: impl IntoIterator<Item = T>,
    keep: impl Fn(T) -> EntityInstance,
) -> Result<DocumentPage, QueryError> {
    let wanted = query.effective_limit()? + 1;
    let mut previous: Option<String> = None;
    let mut items = Vec::with_capacity(wanted);
    for row in instances {
        let instance = row.borrow();
        if instance.entity != query.entity {
            return Err(QueryError::Invalid(
                "query input contains an instance of another entity".to_owned(),
            ));
        }
        match &mut previous {
            Some(id) if *id >= instance.id => {
                return Err(QueryError::Invalid(
                    "query input is not strictly ordered by identity".to_owned(),
                ));
            }
            Some(id) => id.clone_from(&instance.id),
            None => previous = Some(instance.id.clone()),
        }
        if instance.id.as_str() <= after {
            continue;
        }
        if query.matching.iter().all(|(field, expected)| {
            instance
                .fields
                .get(field)
                .is_some_and(|actual| contains(actual, expected))
        }) {
            items.push(keep(row));
            if items.len() == wanted {
                break;
            }
        }
    }
    DocumentPage::from_matches(query, items)
}

/// The provider-neutral meaning of PostgreSQL JSONB containment.
fn contains(actual: &Value, expected: &Value) -> bool {
    match (actual, expected) {
        (Value::Number(actual), Value::Number(expected)) => {
            entity_core::compare_numbers(actual, expected).is_eq()
        }
        (Value::Object(actual), Value::Object(expected)) => expected.iter().all(|(key, value)| {
            actual
                .get(key)
                .is_some_and(|candidate| contains(candidate, value))
        }),
        (Value::Array(actual), Value::Array(expected)) => expected
            .iter()
            .all(|value| actual.iter().any(|candidate| contains(candidate, value))),
        _ => actual == expected,
    }
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    encoded
}

fn decode_hex(value: &str) -> Result<Vec<u8>, QueryError> {
    if value.len() % 2 != 0 {
        return Err(QueryError::Invalid(
            "query cursor has invalid hexadecimal data".to_owned(),
        ));
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| Ok((hex_digit(pair[0])? << 4) | hex_digit(pair[1])?))
        .collect()
}

fn hex_digit(byte: u8) -> Result<u8, QueryError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        _ => Err(QueryError::Invalid(
            "query cursor has invalid hexadecimal data".to_owned(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cursor_is_bound_to_the_query_that_emitted_it() {
        let first = DocumentQuery::for_entity("work").matching("status", Value::from("active"));
        let cursor = first.cursor_after("id-0000000001").expect("cursor");
        assert_eq!(
            first.clone().after(cursor.clone()).after_id(),
            Ok("id-0000000001".to_owned())
        );
        let other = DocumentQuery::for_entity("work").matching("status", Value::from("draft"));
        assert_eq!(
            other.after(cursor).after_id(),
            Err(QueryError::Invalid(
                "query cursor belongs to another query".to_owned()
            ))
        );
    }

    #[test]
    fn page_limits_are_bounded() {
        assert!(DocumentQuery::for_entity("work")
            .with_limit(0)
            .effective_limit()
            .is_err());
        assert!(DocumentQuery::for_entity("work")
            .with_limit(MAX_LIMIT + 1)
            .effective_limit()
            .is_err());
    }

    #[test]
    fn containment_compares_numeric_values_across_json_representations() {
        for (left, right) in [("100", "100.0"), ("0", "-0.0"), ("0.1", "1e-1")] {
            let left: Value = serde_json::from_str(left).unwrap();
            let right: Value = serde_json::from_str(right).unwrap();
            assert!(contains(&left, &right));
            assert!(contains(&right, &left));
        }
        assert!(!contains(&serde_json::json!(0), &serde_json::json!(0.1)));
    }

    #[test]
    fn nested_document_matching_has_the_same_containment_meaning_as_jsonb() {
        let actual = serde_json::json!({
            "metadata": { "locator": "story:one", "revision": 3 },
            "tags": ["runtime", "planning"]
        });
        assert!(contains(
            &actual,
            &serde_json::json!({ "metadata": { "locator": "story:one" } })
        ));
        assert!(contains(
            &actual,
            &serde_json::json!({ "tags": ["planning"] })
        ));
        assert!(!contains(
            &actual,
            &serde_json::json!({ "metadata": { "locator": "story:two" } })
        ));
    }

    #[test]
    fn a_memory_store_page_holds_only_matches_after_the_cursor_and_of_the_queried_type() {
        use entity_core::Decision;
        use entity_store::{Expect, Store};

        let mut store = MemoryStore::new();
        let rows = [
            ("other", "a", "active"),
            ("work", "a", "active"),
            ("work", "b", "draft"),
            ("work", "c", "active"),
            ("work", "d", "active"),
            ("workz", "a", "active"),
        ];
        for (entity, id, status) in rows {
            let instance = EntityInstance {
                id: id.into(),
                entity: entity.into(),
                version: 1,
                revision: 1,
                lifecycle_state: "open".into(),
                fields: [("status".to_owned(), Value::from(status))]
                    .into_iter()
                    .collect(),
            };
            store
                .commit(
                    &Decision::legacy_import(instance, Vec::new()),
                    Expect::Absent,
                )
                .expect("accepted");
        }
        let query = DocumentQuery::for_entity("work")
            .matching("status", Value::from("active"))
            .with_limit(1);
        let mut seen = Vec::new();
        let mut page = store.query_documents(&query).expect("first page");
        loop {
            seen.extend(
                page.items
                    .iter()
                    .map(|item| format!("{}/{}", item.entity, item.id)),
            );
            let Some(next) = page.next.clone() else { break };
            page = store
                .query_documents(&query.clone().after(next))
                .expect("next page");
        }
        assert_eq!(seen, ["work/a", "work/c", "work/d"]);
    }
}
