//! Derived query candidates; recorded history remains the authority for returned instances.
use entity_query::{AsyncDocumentQueryProvider, DocumentPage, DocumentQuery, QueryError};
use eventlog_core::{BoxFuture, ProjectionQuery, ProjectionSpec, ProjectionStore, Projector};

use super::*;

/// The existing ER instance document indexed through Eventlog's optional JSON query capability.
/// Hosted owners pass this declaration to their explicit Eventlog projection migration.
pub const DOCUMENT_PROJECTION: ProjectionSpec = ProjectionSpec {
    name: "er_documents_v1",
    indexed: &[],
};

/// An inline fold of all version-one ER namespaces in one Eventlog owner.
///
/// Register once on every writer before traffic. Existing data requires a fenced projection
/// rebuild before inline registration, followed by `EventlogStore::enable_document_queries`.
/// Rows are untrusted derived candidates, never a substitute for recorded replay verification.
pub struct EntityDocumentProjector;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    scope: String,
    head: u64,
    instance: EntityInstance,
}

fn key(scope: &str, entity: &str, id: &str) -> Result<String, StoreError> {
    Ok(format!("{scope}/{}/{id}", digest(&entity)?))
}

fn projection_error(error: impl std::fmt::Display) -> EventLogError {
    EventLogError::Invalid(format!("entity document projection: {error}"))
}

impl Projector for EntityDocumentProjector {
    fn name(&self) -> &'static str {
        DOCUMENT_PROJECTION.name
    }
    fn projections(&self) -> &'static [ProjectionSpec] {
        &[DOCUMENT_PROJECTION]
    }
    fn document_projections(&self) -> &'static [&'static str] {
        &[DOCUMENT_PROJECTION.name]
    }

    fn apply<'a>(
        &'a self,
        event: &'a RecordedEvent,
        store: &'a mut dyn ProjectionStore,
    ) -> BoxFuture<'a, Result<(), EventLogError>> {
        Box::pin(async move {
            let Some(namespace) = event.stream_type.strip_prefix("er-history-v1-") else {
                return Ok(());
            };
            if namespace.len() != 64
                || !namespace
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                return Err(projection_error("invalid namespace coordinate"));
            }
            let record = decode(event).map_err(projection_error)?;
            record.payload.validate().map_err(projection_error)?;
            let (entity, id) = record.payload.subject();
            if event.stream_id != digest(&(entity, id)).map_err(projection_error)? {
                return Err(projection_error("record belongs to another subject stream"));
            }
            let row_key = key(&event.stream_type, entity, id).map_err(projection_error)?;
            let previous: Option<Document> = store
                .get(&DOCUMENT_PROJECTION, &event.tenant, &row_key)
                .await?
                .map(serde_json::from_value)
                .transpose()
                .map_err(projection_error)?;
            if previous.as_ref().is_some_and(|old| {
                old.scope != event.stream_type
                    || old.instance.entity != entity
                    || old.instance.id != id
            }) {
                return Err(projection_error("stored row belongs to another subject"));
            }
            // Catch-up delivery is at least once; an already-applied prefix cannot lower the view.
            if previous
                .as_ref()
                .is_some_and(|old| old.head >= event.version)
            {
                return Ok(());
            }
            if previous.as_ref().map_or(0, |old| old.head).checked_add(1) != Some(event.version) {
                return Err(projection_error("history has a gap; rebuild required"));
            }
            let instance = match record.payload {
                Payload::Decision(commit) => {
                    if previous
                        .as_ref()
                        .map_or(0, |old| old.instance.revision)
                        .checked_add(1)
                        != Some(commit.instance.revision)
                    {
                        return Err(projection_error("decision revision does not advance"));
                    }
                    commit.instance
                }
                Payload::Observation(observation) => {
                    let old = previous
                        .ok_or_else(|| projection_error("observation precedes creation"))?;
                    if old.instance.revision != observation.revision {
                        return Err(projection_error("observation revision differs"));
                    }
                    old.instance
                }
            };
            let body = serde_json::to_value(Document {
                scope: event.stream_type.clone(),
                head: event.version,
                instance,
            })
            .map_err(projection_error)?;
            store
                .upsert(&DOCUMENT_PROJECTION, &event.tenant, &row_key, &body)
                .await
        })
    }
}

impl<S: AtomicEventStore + ?Sized> EventlogStore<S> {
    /// Verifies that an inline document projection covers this tenant and namespace completely.
    ///
    /// This explicit startup scan validates committed histories and their derived row positions.
    /// It never waits for the feed or silently rebuilds a missing/lagging projection. The host
    /// fences old writers, performs any needed rebuild, and registers the projector first.
    /// # Errors
    /// Unsupported/unregistered queries, incomplete coverage, changed or invalid recorded history.
    pub async fn enable_document_queries(&mut self) -> Result<(), QueryError> {
        self.documents_ready = false;
        if !self.store.is_inline(DOCUMENT_PROJECTION.name).await {
            return Err(QueryError::Invalid(
                "register the entity document projector inline before enabling queries".into(),
            ));
        }
        self.store
            .projection_query(
                &DOCUMENT_PROJECTION,
                &self.tenant,
                &ProjectionQuery {
                    matching: json!({}),
                    prefix: Some(format!("{}/", self.history_type)),
                    after: None,
                    limit: 1,
                },
            )
            .await
            .map_err(backend)?;
        for (entity, id) in self.subjects().await? {
            let row_key = key(&self.history_type, &entity, &id)?;
            let body = self
                .store
                .projection_get(&DOCUMENT_PROJECTION, &self.tenant, &row_key)
                .await
                .map_err(backend)?
                .ok_or_else(|| invalid("document projection is incomplete; rebuild required"))?;
            self.verify_document(&row_key, body, &entity).await?;
        }
        self.documents_ready = true;
        Ok(())
    }

    async fn verify_document(
        &mut self,
        row_key: &str,
        body: Value,
        entity: &str,
    ) -> Result<EntityInstance, StoreError> {
        let document: Document = serde_json::from_value(body).map_err(invalid)?;
        if document.scope != self.history_type
            || document.instance.entity != entity
            || row_key != key(&self.history_type, entity, &document.instance.id)?
        {
            return Err(invalid(
                "query candidate belongs to another scope or identity",
            ));
        }
        let subject = self.read_subject(entity, &document.instance.id).await?;
        if subject.head != document.head || subject.instance() != Some(&document.instance) {
            return Err(invalid(
                "document projection differs from recorded history or changed during the query",
            ));
        }
        Ok(document.instance)
    }
}

impl<S: AtomicEventStore + ?Sized> AsyncDocumentQueryProvider for EventlogStore<S> {
    fn query_documents<'a>(
        &'a mut self,
        query: &'a DocumentQuery,
    ) -> entity_query::QueryFuture<'a> {
        Box::pin(async move {
            let limit = query.effective_limit()?;
            let after = query.after_id()?;
            if !self.documents_ready {
                return Err(QueryError::Invalid(
                    "enable document queries after verifying projection coverage".into(),
                ));
            }
            let prefix = key(&self.history_type, &query.entity, "")?;
            let page = self.store.projection_query(&DOCUMENT_PROJECTION, &self.tenant, &ProjectionQuery {
                matching: json!({"instance": {"entity": query.entity, "fields": query.matching}}),
                prefix: Some(prefix.clone()), after: Some(format!("{prefix}{after}")), limit,
            }).await.map_err(backend)?;
            let mut items = Vec::with_capacity(page.rows.len());
            let mut last = after;
            for (row_key, body) in page.rows {
                let instance = self.verify_document(&row_key, body, &query.entity).await?;
                if instance.id <= last {
                    return Err(invalid("document query did not advance in identity order").into());
                }
                last.clone_from(&instance.id);
                items.push(instance);
            }
            if page
                .next_cursor
                .as_deref()
                .is_some_and(|cursor| cursor != format!("{prefix}{last}"))
            {
                return Err(invalid("document query returned an invalid continuation").into());
            }
            DocumentPage::from_page(query, items, page.next_cursor.is_some())
        })
    }
}
