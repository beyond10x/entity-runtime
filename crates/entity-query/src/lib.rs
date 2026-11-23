//! Optional indexed document queries outside the deterministic kernel.
//!
//! A store remains useful with only point reads and writes. Providers serving a central authority
//! may additionally implement [`DocumentQueryProvider`] so callers can page over selected instance
//! documents without enumerating and hydrating an entire entity type.

use std::borrow::Borrow;
use std::cmp::Ordering;
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
    ///
    /// A cursor is `v1.<digest>.<last id>`: the 128-bit FNV-1a digest of the query's canonical
    /// identity and the last instance id, both in lowercase hexadecimal, so its length does not
    /// grow with the predicate. A cursor of the earlier form `<identity>.<last id>`, which carries
    /// the whole canonical identity, still continues the query it belongs to.
    pub fn after_id(&self) -> Result<String, QueryError> {
        let Some(cursor) = &self.after else {
            return Ok(String::new());
        };
        let malformed = || QueryError::Invalid("query cursor is malformed".to_owned());
        let last_id = if let Some(rest) = cursor.0.strip_prefix(CURSOR_V1) {
            let (digest, last_id) = rest.split_once('.').ok_or_else(malformed)?;
            check_hex(digest)?;
            if digest.as_bytes() != encode_hex(&self.digest()?.to_be_bytes()).as_bytes() {
                return Err(another_query());
            }
            last_id
        } else {
            let (identity, last_id) = cursor.0.split_once('.').ok_or_else(malformed)?;
            check_hex(identity)?;
            if !self.identity_is(identity)? {
                return Err(another_query());
            }
            last_id
        };
        String::from_utf8(decode_hex(last_id)?)
            .map_err(|_| QueryError::Invalid("query cursor identity is not UTF-8".to_owned()))
    }

    /// The 128-bit FNV-1a digest of this query's canonical identity, serialised straight into the
    /// hash without a buffer.
    fn digest(&self) -> Result<u128, QueryError> {
        let mut hasher = Fnv1a128::new();
        serde_json::to_writer(&mut hasher, &(&self.entity, &self.matching))
            .map_err(|error| QueryError::Invalid(error.to_string()))?;
        Ok(hasher.0)
    }

    /// Whether `hex`, already checked as canonical hexadecimal, encodes this query's identity.
    ///
    /// The identity is serialised straight into a comparison with the cursor's text, so a
    /// continuation allocates neither a decoded copy of the cursor nor a serialised copy of the
    /// query — both the size of the predicate.
    fn identity_is(&self, hex: &str) -> Result<bool, QueryError> {
        let mut matcher = HexMatcher {
            expected: hex.as_bytes(),
            matched: true,
        };
        serde_json::to_writer(&mut matcher, &(&self.entity, &self.matching))
            .map_err(|error| QueryError::Invalid(error.to_string()))?;
        Ok(matcher.matched && matcher.expected.is_empty())
    }

    #[cfg(test)]
    fn identity_bytes(&self) -> Result<Vec<u8>, QueryError> {
        serde_json::to_vec(&(&self.entity, &self.matching))
            .map_err(|error| QueryError::Invalid(error.to_string()))
    }

    fn cursor_after(&self, id: &str) -> Result<QueryCursor, QueryError> {
        Ok(QueryCursor(format!(
            "{CURSOR_V1}{}.{}",
            encode_hex(&self.digest()?.to_be_bytes()),
            encode_hex(id.as_bytes())
        )))
    }
}

/// The prefix of a cursor that carries a digest of its query rather than the whole query. An
/// earlier cursor starts with hexadecimal digits, never with `v`.
const CURSOR_V1: &str = "v1.";

fn another_query() -> QueryError {
    QueryError::Invalid("query cursor belongs to another query".to_owned())
}

/// The 128-bit FNV-1a hash of the bytes written to it.
struct Fnv1a128(u128);

impl Fnv1a128 {
    const OFFSET_BASIS: u128 = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d;
    const PRIME: u128 = 0x0000_0000_0100_0000_0000_0000_0000_013b;

    const fn new() -> Self {
        Self(Self::OFFSET_BASIS)
    }
}

impl std::io::Write for Fnv1a128 {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        for &byte in bytes {
            self.0 = (self.0 ^ u128::from(byte)).wrapping_mul(Self::PRIME);
        }
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
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
    comparisons::note();
    match (actual, expected) {
        (Value::Number(actual), Value::Number(expected)) => {
            entity_core::compare_numbers(actual, expected).is_eq()
        }
        (Value::Object(actual), Value::Object(expected)) => expected.iter().all(|(key, value)| {
            actual
                .get(key)
                .is_some_and(|candidate| contains(candidate, value))
        }),
        (Value::Array(actual), Value::Array(expected)) => array_contains(actual, expected),
        _ => actual == expected,
    }
}

/// Every expected member is contained by some actual member.
///
/// A scalar member can only be contained by an equal scalar, numbers by exact value. Searching
/// the stored array from its start for each member costs its length per member, quadratic when a
/// query names many of a row's members; a sorted index of the stored scalars costs about
/// `n·log n` once and `log n` per member, so it is built when the expected scalars outnumber
/// `log n`. Objects and nested arrays are still searched recursively.
fn array_contains(actual: &[Value], expected: &[Value]) -> bool {
    let scalars = expected.iter().filter(|value| is_scalar(value)).count();
    let log = (usize::BITS - actual.len().leading_zeros()) as usize;
    let indexed = scalars > 1 && scalars > log;
    let mut index: Option<Vec<&Value>> = None;
    expected.iter().all(|value| {
        if indexed && is_scalar(value) {
            let index = index.get_or_insert_with(|| {
                let mut held: Vec<&Value> = actual.iter().filter(|held| is_scalar(held)).collect();
                held.sort_by(|left, right| scalar_order(left, right));
                held
            });
            index
                .binary_search_by(|held| scalar_order(held, value))
                .is_ok()
        } else {
            actual.iter().any(|candidate| contains(candidate, value))
        }
    })
}

const fn is_scalar(value: &Value) -> bool {
    !matches!(value, Value::Array(_) | Value::Object(_))
}

/// A total order on scalars whose equality is containment's: numbers by exact value, everything
/// else by kind and then value.
fn scalar_order(left: &Value, right: &Value) -> Ordering {
    comparisons::note();
    const fn rank(value: &Value) -> u8 {
        match value {
            Value::Null => 0,
            Value::Bool(_) => 1,
            Value::Number(_) => 2,
            Value::String(_) => 3,
            Value::Array(_) => 4,
            Value::Object(_) => 5,
        }
    }
    match (left, right) {
        (Value::Bool(left), Value::Bool(right)) => left.cmp(right),
        (Value::Number(left), Value::Number(right)) => entity_core::compare_numbers(left, right),
        (Value::String(left), Value::String(right)) => left.cmp(right),
        _ => rank(left).cmp(&rank(right)),
    }
}

/// Counts value comparisons in this crate's tests, so a test can hold containment's cost; a no-op
/// otherwise.
mod comparisons {
    #[cfg(test)]
    thread_local! {
        static COUNT: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    }

    #[cfg(test)]
    pub(crate) fn note() {
        COUNT.with(|count| count.set(count.get() + 1));
    }

    #[cfg(not(test))]
    #[inline(always)]
    pub(crate) const fn note() {}

    /// Comparisons made on this thread so far.
    #[cfg(test)]
    pub(crate) fn taken() -> u64 {
        COUNT.with(std::cell::Cell::get)
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

/// Compares bytes written to it with the bytes a canonical hexadecimal text encodes.
struct HexMatcher<'a> {
    /// The hexadecimal digits not yet compared.
    expected: &'a [u8],
    /// `false` once a written byte differed or outran `expected`.
    matched: bool,
}

impl std::io::Write for HexMatcher<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        for &byte in bytes {
            if !self.matched {
                break;
            }
            match self.expected {
                [high, low, rest @ ..]
                    if hex_digit(*high) == Ok(byte >> 4) && hex_digit(*low) == Ok(byte & 0x0f) =>
                {
                    self.expected = rest;
                }
                _ => self.matched = false,
            }
        }
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Refuses text that is not canonical lowercase hexadecimal, as [`decode_hex`] would.
fn check_hex(value: &str) -> Result<(), QueryError> {
    if value.len() % 2 != 0 || !value.bytes().all(|byte| hex_digit(byte).is_ok()) {
        return Err(QueryError::Invalid(
            "query cursor has invalid hexadecimal data".to_owned(),
        ));
    }
    Ok(())
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
    fn a_cursor_whose_identity_is_a_prefix_or_an_extension_of_the_query_belongs_to_another_query() {
        let query = DocumentQuery::for_entity("work").matching("status", Value::from("active"));
        let identity = encode_hex(&query.identity_bytes().expect("identity"));
        let last = encode_hex(b"id-1");
        for foreign in [
            identity[..identity.len() - 2].to_owned(),
            format!("{identity}00"),
            String::new(),
        ] {
            let cursor = QueryCursor(format!("{foreign}.{last}"));
            assert_eq!(
                query.clone().after(cursor).after_id(),
                Err(QueryError::Invalid(
                    "query cursor belongs to another query".to_owned()
                )),
                "identity {foreign:?}"
            );
        }
        assert_eq!(
            query
                .clone()
                .after(QueryCursor(format!("{identity}.{last}")))
                .after_id(),
            Ok("id-1".to_owned())
        );
    }

    #[test]
    fn a_cursor_has_the_same_length_however_large_the_query_grows() {
        // The cursor carried the hex-encoded query: 113, 2,097 and 131,121 bytes for these.
        let lengths: Vec<usize> = [32, 1_024, 65_536]
            .into_iter()
            .map(|size| {
                DocumentQuery::for_entity("work")
                    .matching("title", Value::from("t".repeat(size)))
                    .cursor_after("id-1")
                    .expect("cursor")
                    .as_str()
                    .len()
            })
            .collect();
        let expected = CURSOR_V1.len() + 32 + 1 + "id-1".len() * 2;
        assert_eq!(lengths, [expected; 3]);
    }

    #[test]
    fn a_cursor_names_its_version_the_fnv_1a_digest_of_the_query_and_the_last_id() {
        let query = DocumentQuery::for_entity("docs");
        assert_eq!(
            query.cursor_after("a").expect("cursor").as_str(),
            "v1.2b0ed12cf104ee82390a85b3b219daac.61"
        );
        // Published 128-bit FNV-1a vectors.
        for (input, digest) in [
            (&b""[..], 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d_u128),
            (b"a", 0xd228_cb69_6f1a_8caf_7891_2b70_4e4a_8964),
            (b"foobar", 0x343e_1662_793c_64bf_6f0d_3597_ba44_6f18),
        ] {
            let mut hasher = Fnv1a128::new();
            std::io::Write::write_all(&mut hasher, input).expect("hashed");
            assert_eq!(hasher.0, digest);
        }
    }

    #[test]
    fn a_cursor_carrying_the_whole_query_still_continues_that_query() {
        let query = DocumentQuery::for_entity("work").matching("status", Value::from("active"));
        let former = QueryCursor(format!(
            "{}.{}",
            encode_hex(&query.identity_bytes().expect("identity")),
            encode_hex(b"id-1")
        ));
        assert_eq!(
            query.clone().after(former.clone()).after_id(),
            Ok("id-1".to_owned())
        );
        let other = DocumentQuery::for_entity("work").matching("status", Value::from("draft"));
        assert_eq!(other.after(former).after_id(), Err(another_query()));
    }

    #[test]
    fn a_digest_cursor_of_another_query_is_refused_as_another_query() {
        let query = DocumentQuery::for_entity("work").matching("status", Value::from("active"));
        let digest = encode_hex(&query.digest().expect("digest").to_be_bytes());
        let last = encode_hex(b"id-1");
        let others = [
            DocumentQuery::for_entity("work").matching("status", Value::from("draft")),
            DocumentQuery::for_entity("works").matching("status", Value::from("active")),
            DocumentQuery::for_entity("work"),
        ];
        for other in &others {
            let cursor = other.cursor_after("id-1").expect("cursor");
            assert_eq!(
                query.clone().after(cursor).after_id(),
                Err(another_query()),
                "cursor of {other:?}"
            );
        }
        for foreign in [
            digest[..digest.len() - 2].to_owned(),
            format!("{digest}00"),
            String::new(),
        ] {
            let cursor = QueryCursor(format!("{CURSOR_V1}{foreign}.{last}"));
            assert_eq!(
                query.clone().after(cursor).after_id(),
                Err(another_query()),
                "digest {foreign:?}"
            );
        }
        assert_eq!(
            query
                .clone()
                .after(QueryCursor(format!(
                    "{CURSOR_V1}{}.{last}",
                    digest.to_uppercase()
                )))
                .after_id(),
            Err(QueryError::Invalid(
                "query cursor has invalid hexadecimal data".to_owned()
            ))
        );
        assert_eq!(
            query
                .clone()
                .after(QueryCursor(format!("{CURSOR_V1}{digest}")))
                .after_id(),
            Err(QueryError::Invalid("query cursor is malformed".to_owned()))
        );
        assert_eq!(
            query
                .after(QueryCursor(format!("{CURSOR_V1}{digest}.{last}")))
                .after_id(),
            Ok("id-1".to_owned())
        );
    }

    #[test]
    fn array_containment_keeps_numeric_equivalence_and_nested_matching_on_the_indexed_path() {
        let actual = serde_json::json!([
            1, 2.0, "a", null, true, 0, { "k": 1, "j": [5, "x"] }, [3, 4]
        ]);
        // Five scalars against eight stored members: enough to take the index.
        assert!(contains(
            &actual,
            &serde_json::json!([1.0, 2, "a", null, true])
        ));
        assert!(contains(
            &actual,
            &serde_json::json!([-0.0, 1e0, 20e-1, "a", true, { "k": 1.0 }, [4.0]])
        ));
        assert!(contains(
            &actual,
            &serde_json::json!([{ "j": ["x", 5.0] }, 1, 2, "a", null])
        ));
        assert!(!contains(
            &actual,
            &serde_json::json!([1, 2, "a", null, false])
        ));
        assert!(!contains(&actual, &serde_json::json!([1, 2, "a", null, 3])));
        assert!(!contains(
            &actual,
            &serde_json::json!([1, 2, "a", null, "1"])
        ));
        assert!(!contains(
            &actual,
            &serde_json::json!([1, 2, "a", null, { "k": 2 }])
        ));
        // A scalar never matches a stored array or object, indexed or not.
        assert!(!contains(
            &serde_json::json!([[3], { "k": 3 }]),
            &serde_json::json!([3, 3.0, 3e0, 30e-1, 300e-2])
        ));
        assert!(!contains(
            &serde_json::json!([[3]]),
            &serde_json::json!([3])
        ));
    }

    #[test]
    fn containing_every_member_of_a_long_array_makes_a_near_linear_number_of_comparisons() {
        // Searching the stored array from its start for each requested member made
        // 512 · 513 / 2 = 131,328 comparisons here.
        let tags: Vec<Value> = (0..512)
            .map(|tag| Value::from(format!("tag-{tag:04}")))
            .collect();
        let actual = Value::Array(tags.clone());
        let expected = Value::Array(tags);
        let before = comparisons::taken();
        assert!(contains(&actual, &expected));
        let taken = comparisons::taken() - before;
        assert!(taken < 20_000, "{taken} comparisons");
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
