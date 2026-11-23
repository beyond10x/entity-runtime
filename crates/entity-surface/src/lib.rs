//! Deterministic public surfaces projected from validated entity definitions.
//!
//! This crate performs no IO. Callers receive a stable map of relative paths to bytes and decide
//! where, or whether, to write them. The same schema projection feeds human documentation,
//! OpenAPI, AsyncAPI and MCP so an argument cannot be documented one way and offered to an agent
//! another way.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use entity_core::{DeclaredDefault, EntityDefinition, FieldDefinition, FieldKind, ObjectSchema};
use entity_graph::{render, Graph, Layout};
use serde_json::{json, Map, Value};

/// Marker written into every generated documentation directory.
pub const DOCS_MARKER: &str = ".entity-runtime-docs.json";

/// A complete generated documentation directory, keyed by safe relative path.
pub type DocumentationBundle = BTreeMap<String, String>;

/// The start of the placeholder word a YAML contract carries for a number until its digits are
/// put in (`yaml`).
const NUMBER_MARKER: &str = "entity-runtime-number-";

/// Projects an entity object schema into JSON Schema 2020-12 vocabulary.
///
/// This is the stored shape: every `required` field is required, including one the kernel fills
/// from its declared `default`. A schema for what a caller sends is [`input_object_schema`].
#[must_use]
pub fn object_schema(schema: &ObjectSchema) -> Value {
    object_schema_as(schema, Requiredness::Stored)
}

/// Projects an object schema a caller sends to the kernel — creation fields, creation arguments
/// or operation arguments.
///
/// The kernel fills a declared `default` before it validates, at every depth an object or array
/// element the caller sent reaches, so a required field with a default is not required here. The
/// stored shape keeps it required ([`object_schema`]). A map value or a union payload gets no
/// defaults filled and keeps its stored requiredness.
#[must_use]
pub fn input_object_schema(schema: &ObjectSchema) -> Value {
    object_schema_as(schema, Requiredness::Input)
}

/// Which `required` list an object schema publishes.
#[derive(Clone, Copy, PartialEq)]
enum Requiredness {
    /// The materialized instance: every required field.
    Stored,
    /// Input before the kernel applies declared defaults: a field with a default may be omitted.
    Input,
}

fn object_schema_as(schema: &ObjectSchema, requiredness: Requiredness) -> Value {
    let mut properties = Map::new();
    let mut required = Vec::new();
    for (name, field) in &schema.fields {
        properties.insert(name.clone(), field_schema_as(field, requiredness));
        let filled = requiredness == Requiredness::Input && field.default.as_value().is_some();
        if field.required && !filled {
            required.push(Value::String(name.clone()));
        }
    }
    let mut out = Map::new();
    out.insert("type".into(), Value::String("object".into()));
    out.insert("properties".into(), Value::Object(properties));
    out.insert(
        "additionalProperties".into(),
        Value::Bool(schema.additional_fields),
    );
    if !required.is_empty() {
        out.insert("required".into(), Value::Array(required));
    }
    Value::Object(out)
}

/// The schema of a creation command's input: what the kernel validates the input against.
///
/// A `service/1`, `service/2` or `service/3` definition that declares creation outcomes takes its
/// declared creation arguments, and its selected branch writes the fields; every other definition
/// takes the instance fields directly. This is the selection `entity_core::decide_create` makes.
/// Either way a required member with a declared default may be omitted, because the kernel fills
/// it before validating ([`input_object_schema`]).
#[must_use]
pub fn create_input_schema(definition: &EntityDefinition) -> Value {
    if has_branched_creation(definition) {
        input_object_schema(&definition.create.arguments)
    } else {
        input_object_schema(&definition.schema)
    }
}

fn has_branched_creation(definition: &EntityDefinition) -> bool {
    definition.semantics.has_service_semantics() && !definition.create.outcomes.is_empty()
}

/// Generates the canonical contract-only OpenAPI 3.2 document for a definition set.
#[must_use]
pub fn openapi(definitions: &[EntityDefinition]) -> Value {
    let mut schemas = Map::new();
    let mut paths = Map::new();
    for definition in definitions {
        let key = schema_key(definition);
        let stored = object_schema(&definition.schema);
        let input = create_input_schema(definition);
        // The create request names its own component only where it differs from the stored
        // fields: branched creation takes arguments, and a default makes a field optional to send.
        let create_input = if has_branched_creation(definition) {
            schemas.insert(format!("{key}CreateArguments"), input);
            format!("#/components/schemas/{key}CreateArguments")
        } else if input != stored {
            schemas.insert(format!("{key}CreateFields"), input);
            format!("#/components/schemas/{key}CreateFields")
        } else {
            format!("#/components/schemas/{key}Fields")
        };
        schemas.insert(format!("{key}Fields"), stored);
        schemas.insert(format!("{key}Instance"), instance_schema(definition, &key));

        let collection = format!(
            "/entities/{}/versions/{}",
            path_segment(&definition.entity),
            definition.version
        );
        paths.insert(
            collection.clone(),
            json!({
                "post": {
                    "operationId": format!("create_{}_v{}", safe_identifier(&definition.entity), definition.version),
                    "summary": format!("Create {} v{}", definition.entity, definition.version),
                    "requestBody": { "required": true, "content": { "application/json": { "schema": {
                        "type": "object", "additionalProperties": false,
                        "required": ["id", "fields", "recording"],
                        "properties": {
                            "id": { "type": "string", "minLength": 1 },
                            "fields": { "$ref": create_input },
                            "recording": { "$ref": "#/components/schemas/Recording" }
                        }
                    }}}},
                    "responses": standard_write_responses(&format!("#/components/schemas/{key}Instance"))
                },
                "get": {
                    "operationId": format!("list_{}_v{}", safe_identifier(&definition.entity), definition.version),
                    "summary": format!("List {} v{} identities", definition.entity, definition.version),
                    "responses": { "200": { "description": "Sorted identities", "content": { "application/json": { "schema": {
                        "type": "array", "items": { "type": "string" }
                    }}}}}
                }
            }),
        );

        let subject = format!("{collection}/{{id}}");
        paths.insert(subject.clone(), json!({
            "get": {
                "operationId": format!("get_{}_v{}", safe_identifier(&definition.entity), definition.version),
                "summary": format!("Get {} v{}", definition.entity, definition.version),
                "parameters": [id_parameter()],
                "responses": {
                    "200": { "description": "Current instance", "content": { "application/json": { "schema": { "$ref": format!("#/components/schemas/{key}Instance") }}}},
                    "404": { "$ref": "#/components/responses/NotFound" }
                }
            }
        }));
        paths.insert(format!("{subject}/events"), json!({
            "get": {
                "operationId": format!("events_{}_v{}", safe_identifier(&definition.entity), definition.version),
                "summary": format!("Get {} v{} events", definition.entity, definition.version),
                "parameters": [id_parameter()],
                "responses": { "200": { "description": "Events in recorded order", "content": { "application/json": { "schema": {
                    "type": "array", "items": { "$ref": "#/components/schemas/DomainEvent" }
                }}}}}
            }
        }));
        for (operation_name, operation) in &definition.operations {
            let operation_path = format!("{subject}/operations/{}", path_segment(operation_name));
            paths.insert(operation_path, json!({
                "post": {
                    "operationId": format!("{}_{}_v{}", safe_identifier(operation_name), safe_identifier(&definition.entity), definition.version),
                    "summary": format!("{} {} v{}", operation_name, definition.entity, definition.version),
                    "parameters": [id_parameter()],
                    "requestBody": { "required": true, "content": { "application/json": { "schema": {
                        "type": "object", "additionalProperties": false,
                        "required": ["expected_revision", "arguments", "recording"],
                        "properties": {
                            "expected_revision": { "type": "integer", "minimum": 1 },
                            "arguments": input_object_schema(&operation.arguments),
                            "recording": { "$ref": "#/components/schemas/Recording" }
                        }
                    }}}},
                    "responses": standard_write_responses(&format!("#/components/schemas/{key}Instance"))
                }
            }));
        }
    }
    reserve_unique_operation_ids(&mut paths);
    schemas.insert("Recording".into(), recording_schema());
    schemas.insert("DomainEvent".into(), domain_event_schema());
    schemas.insert(
        "Refusal".into(),
        json!({
            "type": "object", "required": ["refused", "detail"],
            "properties": { "refused": { "const": true }, "detail": { "type": "string" } }
        }),
    );

    json!({
        "openapi": "3.2.0",
        "info": {
            "title": "Entity Runtime contract",
            "version": "1.0.0",
            "description": "A generated contract for an adopter-provided HTTP facade. Entity Runtime does not open a network listener."
        },
        "paths": paths,
        "components": {
            "schemas": schemas,
            "responses": {
                "NotFound": { "description": "No stored subject has that identity" },
                "Refused": { "description": "The kernel or store refused without changing state", "content": { "application/json": { "schema": { "$ref": "#/components/schemas/Refusal" }}}}
            }
        },
        "x-entity-runtime-contract-only": true
    })
}

/// Makes every `operationId` of a generated OpenAPI document unique.
///
/// An id is built from names (`list_counter_v1`), so a domain operation named `list`, `get`,
/// `events` or `create`, or two names that `safe_identifier` spells alike, collide with another
/// method. An id already unique keeps its name. Of the methods sharing one id, the first in (path,
/// method) order keeps it, and each later one takes the lowest `<id>_<n>`, `n >= 2`, that no method
/// of the document holds yet. The order is sorted here, so the answer depends on nothing but the
/// document.
fn reserve_unique_operation_ids(paths: &mut Map<String, Value>) {
    let mut methods: Vec<(String, String, String)> = Vec::new();
    for (path, item) in paths.iter() {
        let Value::Object(item) = item else { continue };
        for (method, operation) in item {
            if let Some(id) = operation.get("operationId").and_then(Value::as_str) {
                methods.push((path.clone(), method.clone(), id.to_owned()));
            }
        }
    }
    methods.sort();
    let mut holders: BTreeMap<&str, usize> = BTreeMap::new();
    for (_, _, id) in &methods {
        *holders.entry(id.as_str()).or_default() += 1;
    }
    let mut taken: BTreeSet<String> = holders.keys().map(|id| (*id).to_owned()).collect();
    let mut renamed = Vec::new();
    let mut kept = BTreeSet::new();
    for (path, method, id) in &methods {
        if holders[id.as_str()] < 2 || kept.insert(id.as_str()) {
            continue;
        }
        let unique = (2..)
            .map(|n| format!("{id}_{n}"))
            .find(|candidate| !taken.contains(candidate))
            .unwrap_or_else(|| id.clone());
        taken.insert(unique.clone());
        renamed.push((path.clone(), method.clone(), unique));
    }
    for (path, method, unique) in renamed {
        if let Some(operation) = paths
            .get_mut(&path)
            .and_then(|item| item.get_mut(&method))
            .and_then(Value::as_object_mut)
        {
            operation.insert("operationId".into(), Value::String(unique));
        }
    }
}

/// Generates the contract-only AsyncAPI 3.1 document for every emitted event type.
#[must_use]
pub fn asyncapi(definitions: &[EntityDefinition]) -> Value {
    let mut channels = Map::new();
    let mut messages = Map::new();
    let mut operations = Map::new();
    let mut declared: BTreeMap<String, (String, String, Vec<Value>)> = BTreeMap::new();
    let mut declare = |definition: &EntityDefinition,
                       event: &entity_core::EventDefinition,
                       arguments: Option<&ObjectSchema>| {
        declared
            .entry(format!(
                "{}.{}",
                safe_identifier(&definition.entity),
                safe_identifier(&event.event_type)
            ))
            .or_insert_with(|| {
                (
                    definition.entity.clone(),
                    event.event_type.clone(),
                    Vec::new(),
                )
            })
            .2
            .push(event_envelope_schema(definition, event, arguments));
    };
    for definition in definitions {
        // A `kernel/1` creation event sees no arguments; the events of a `service/1` creation
        // branch see the creation command's arguments, as the kernel materializes them.
        if let Some(event) = &definition.create.emit {
            declare(definition, event, None);
        }
        for outcome in &definition.create.outcomes {
            for event in &outcome.emits {
                declare(definition, event, Some(&definition.create.arguments));
            }
        }
        for operation in definition.operations.values() {
            for event in &operation.emits {
                declare(definition, event, Some(&operation.arguments));
            }
            for outcome in &operation.outcomes {
                for event in &outcome.emits {
                    declare(definition, event, Some(&operation.arguments));
                }
            }
        }
    }
    for (channel_name, (entity, event_type, schemas)) in declared {
        let message_key = if entity.bytes().all(|b| b.is_ascii_alphanumeric())
            && event_type.bytes().all(|b| b.is_ascii_alphanumeric())
        {
            format!("{entity}_{event_type}")
        } else {
            format!(
                "Message_{}_{}",
                hex_identifier(&entity),
                hex_identifier(&event_type)
            )
        };
        channels.insert(channel_name.clone(), json!({
            "address": channel_name,
            "messages": { message_key.clone(): { "$ref": format!("#/components/messages/{message_key}") }}
        }));
        let payload = if schemas.len() == 1 {
            schemas
                .into_iter()
                .next()
                .unwrap_or_else(domain_event_schema)
        } else {
            json!({ "anyOf": schemas })
        };
        messages.insert(
            message_key.clone(),
            json!({
                "name": event_type,
                "title": format!("{entity} event"),
                "payload": payload
            }),
        );
        operations.insert(
            format!("receive_{message_key}"),
            json!({
                "action": "receive",
                "channel": { "$ref": format!("#/channels/{}", pointer_segment(&channel_name)) }
            }),
        );
    }
    json!({
        "asyncapi": "3.1.0",
        "info": {
            "title": "Entity Runtime events",
            "version": "1.0.0",
            "description": "Generated event contracts. Entity Runtime materializes events but does not publish them."
        },
        "channels": channels,
        "operations": operations,
        "components": { "messages": messages },
        "x-entity-runtime-contract-only": true
    })
}

/// Builds a standalone human documentation bundle and both machine contracts.
///
/// # Errors
///
/// Serialization of a value constructed by this crate failed.
pub fn documentation(definitions: &[EntityDefinition]) -> Result<DocumentationBundle, String> {
    let mut files = BTreeMap::new();
    let openapi = openapi(definitions);
    let asyncapi = asyncapi(definitions);
    files.insert(
        DOCS_MARKER.into(),
        "{\"format\":\"entity-runtime-docs/1\"}\n".into(),
    );
    files.insert(
        "openapi.json".into(),
        pretty_json(&openapi).map_err(|error| error.to_string())?,
    );
    files.insert("openapi.yaml".into(), yaml(&openapi)?);
    files.insert(
        "asyncapi.json".into(),
        pretty_json(&asyncapi).map_err(|error| error.to_string())?,
    );
    files.insert("asyncapi.yaml".into(), yaml(&asyncapi)?);
    files.insert("assets/style.css".into(), STYLE.into());

    let grouped = grouped(definitions);
    files.insert("index.md".into(), markdown_index(definitions, &grouped));
    files.insert("index.html".into(), html_index(definitions, &grouped));
    for (entity, versions) in grouped {
        let stem = slug(&entity);
        files.insert(
            format!("entities/{stem}.md"),
            markdown_entity(&entity, &versions),
        );
        files.insert(
            format!("entities/{stem}.html"),
            html_entity(&entity, &versions),
        );
    }
    Ok(files)
}

fn field_schema(field: &FieldDefinition) -> Value {
    field_schema_as(field, Requiredness::Stored)
}

fn field_schema_as(field: &FieldDefinition, requiredness: Requiredness) -> Value {
    let mut out = Map::new();
    match field.kind {
        FieldKind::String => set_type(&mut out, "string"),
        FieldKind::Integer => set_type(&mut out, "integer"),
        FieldKind::Number => set_type(&mut out, "number"),
        FieldKind::Boolean => set_type(&mut out, "boolean"),
        FieldKind::Enum => {
            set_type(&mut out, "string");
            out.insert(
                "enum".into(),
                Value::Array(field.values.iter().cloned().map(Value::String).collect()),
            );
        }
        FieldKind::Array => {
            set_type(&mut out, "array");
            if let Some(items) = &field.items {
                out.insert("items".into(), field_schema_as(items, requiredness));
            }
        }
        FieldKind::Object => {
            let schema = ObjectSchema {
                fields: field.properties.clone(),
                additional_fields: field.additional_properties,
            };
            return with_default(object_schema_as(&schema, requiredness), &field.default);
        }
        FieldKind::Json => {}
        FieldKind::Ref => {
            set_type(&mut out, "string");
            out.insert("minLength".into(), json!(1));
            if let Some(entity) = &field.entity {
                out.insert("x-entity-ref".into(), Value::String(entity.clone()));
            }
        }
        // A JSON object whose keys are checked by their spelling and whose values are one declared
        // shape. The key spelling is published beside it, because a reader that only saw
        // `additionalProperties` would not know a key is checked at all.
        FieldKind::Map => {
            set_type(&mut out, "object");
            if let Some(items) = &field.items {
                out.insert("additionalProperties".into(), field_schema(items));
            }
            if let Some(key) = field.key {
                out.insert("x-key".into(), Value::String(key.as_str().to_owned()));
            }
        }
        // Adjacent tagging, reproduced: the variant label under the tag key, the payload under the
        // derived content key, one branch per declared variant.
        FieldKind::Union => {
            let tag = field.tag.clone().unwrap_or_default();
            let content = if tag == "value" { "content" } else { "value" };
            let branches: Vec<Value> = field
                .variants
                .iter()
                .map(|(label, variant)| {
                    let mut required = vec![Value::String(tag.clone())];
                    if variant.required {
                        required.push(Value::String(content.to_owned()));
                    }
                    json!({
                        "type": "object",
                        "properties": {
                            tag.clone(): { "const": label },
                            content: field_schema(variant),
                        },
                        "required": required,
                        "additionalProperties": false,
                    })
                })
                .collect();
            out.insert("oneOf".into(), Value::Array(branches));
        }
        FieldKind::Binary64 => {
            set_type(&mut out, "number");
            out.insert("format".into(), Value::String("double".into()));
        }
    }
    if let Some(min) = field.min_length {
        out.insert("minLength".into(), json!(min));
    }
    if let Some(max) = field.max_length {
        out.insert("maxLength".into(), json!(max));
    }
    // Published beside the type, as `x-key` is, rather than as a `pattern`: a character class over
    // arbitrary scalar values is not read the same way by every JSON Schema reader, and a client
    // that saw a bare `string` would send values the kernel refuses.
    if let Some(alphabet) = &field.alphabet {
        out.insert("x-alphabet".into(), Value::String(alphabet.clone()));
    }
    if let Some(min) = &field.min {
        out.insert("minimum".into(), Value::Number(min.clone()));
    }
    if let Some(max) = &field.max {
        out.insert("maximum".into(), Value::Number(max.clone()));
    }
    with_default(Value::Object(out), &field.default)
}

fn with_default(mut schema: Value, default: &DeclaredDefault) -> Value {
    if let (Value::Object(out), Some(value)) = (&mut schema, default.as_value()) {
        out.insert("default".into(), value.clone());
    }
    schema
}

fn set_type(out: &mut Map<String, Value>, kind: &str) {
    out.insert("type".into(), Value::String(kind.into()));
}

fn instance_schema(definition: &EntityDefinition, key: &str) -> Value {
    json!({
        "type": "object", "additionalProperties": false,
        "required": ["entity", "version", "id", "lifecycle_state", "revision", "fields"],
        "properties": {
            "entity": { "const": definition.entity },
            "version": { "const": definition.version },
            "id": { "type": "string", "minLength": 1 },
            "lifecycle_state": { "type": "string", "enum": definition.lifecycle.states },
            "revision": { "type": "integer", "minimum": 1 },
            "fields": { "$ref": format!("#/components/schemas/{key}Fields") }
        }
    })
}

fn recording_schema() -> Value {
    json!({
        "type": "object", "additionalProperties": false,
        "required": ["record_id", "recorded_at", "actor"],
        "properties": {
            "record_id": { "type": "string", "minLength": 1 },
            "recorded_at": { "type": "string", "format": "date-time" },
            "actor": { "type": ["string", "null"] },
            "correlation": { "type": "string", "minLength": 1 },
            "causation": { "type": "string", "minLength": 1 }
        }
    })
}

/// The `DomainEvent` both projections publish, closed as the kernel's type is.
///
/// `removed` is optional because the kernel omits it when empty: it names the fields a decision
/// left absent, through a `set` clear under every semantics or a `service/3` fulfillment `Remove`,
/// so an event of any definition may carry it.
fn domain_event_schema() -> Value {
    json!({
        "type": "object", "additionalProperties": false,
        "required": ["entity", "version", "id", "revision", "type", "from_state", "to_state", "changed", "args", "payload"],
        "properties": {
            "entity": { "type": "string" }, "version": { "type": "integer", "minimum": 1 },
            "id": { "type": "string" }, "revision": { "type": "integer", "minimum": 1 },
            "type": { "type": "string" }, "from_state": { "type": ["string", "null"] },
            "to_state": { "type": "string" }, "changed": { "type": "object" },
            "removed": { "type": "array", "items": { "type": "string" }, "uniqueItems": true },
            "args": { "type": "object" }, "payload": {}
        }
    })
}

fn event_envelope_schema(
    definition: &EntityDefinition,
    event: &entity_core::EventDefinition,
    arguments: Option<&ObjectSchema>,
) -> Value {
    let mut schema = domain_event_schema();
    let Value::Object(root) = &mut schema else {
        return schema;
    };
    let Some(Value::Object(properties)) = root.get_mut("properties") else {
        return schema;
    };
    properties.insert("entity".into(), json!({ "const": definition.entity }));
    properties.insert("version".into(), json!({ "const": definition.version }));
    properties.insert("type".into(), json!({ "const": event.event_type }));
    properties.insert(
        "payload".into(),
        template_schema(&event.payload, definition, arguments),
    );
    schema
}

fn template_schema(
    template: &Value,
    definition: &EntityDefinition,
    arguments: Option<&ObjectSchema>,
) -> Value {
    match template {
        Value::String(text) if text.starts_with("$$") => json!({ "const": &text[1..] }),
        Value::String(reference) if reference.starts_with('$') => {
            reference_schema(reference, definition, arguments)
        }
        Value::Array(items) => json!({
            "type": "array",
            "prefixItems": items.iter().map(|item| template_schema(item, definition, arguments)).collect::<Vec<_>>(),
            "minItems": items.len(),
            "maxItems": items.len()
        }),
        Value::Object(values) => {
            let properties: Map<String, Value> = values
                .iter()
                .map(|(name, value)| (name.clone(), template_schema(value, definition, arguments)))
                .collect();
            let required = values.keys().cloned().collect::<Vec<_>>();
            json!({
                "type": "object",
                "properties": properties,
                "required": required,
                "additionalProperties": false
            })
        }
        literal => json!({ "const": literal }),
    }
}

fn reference_schema(
    reference: &str,
    definition: &EntityDefinition,
    arguments: Option<&ObjectSchema>,
) -> Value {
    match reference {
        "$id" | "$entity" | "$from_state" | "$to_state" | "$state" => {
            json!({ "type": "string" })
        }
        "$version" => json!({ "const": definition.version }),
        "$fields" | "$old_fields" => object_schema(&definition.schema),
        "$args" => arguments.map(object_schema).unwrap_or_else(|| json!({})),
        _ => {
            for (prefix, schema) in [
                ("$fields.", Some(&definition.schema)),
                ("$old_fields.", Some(&definition.schema)),
                ("$args.", arguments),
            ] {
                if let (Some(path), Some(schema)) = (reference.strip_prefix(prefix), schema) {
                    if let Some(field) = field_at(schema, path) {
                        return field_schema(field);
                    }
                }
            }
            json!({})
        }
    }
}

fn field_at<'a>(schema: &'a ObjectSchema, path: &str) -> Option<&'a FieldDefinition> {
    let mut segments = path.split('.');
    let mut field = schema.fields.get(segments.next()?)?;
    for segment in segments {
        field = field.properties.get(segment)?;
    }
    Some(field)
}

fn standard_write_responses(instance_ref: &str) -> Value {
    json!({
        "200": { "description": "Recorded decision", "content": { "application/json": { "schema": {
            "type": "object", "required": ["instance", "envelope"],
            "properties": { "instance": { "$ref": instance_ref }, "envelope": { "type": "object" } }
        }}}},
        "409": { "$ref": "#/components/responses/Refused" },
        "422": { "$ref": "#/components/responses/Refused" }
    })
}

fn id_parameter() -> Value {
    json!({ "name": "id", "in": "path", "required": true, "schema": { "type": "string", "minLength": 1 } })
}

fn grouped(definitions: &[EntityDefinition]) -> BTreeMap<String, Vec<&EntityDefinition>> {
    let mut out: BTreeMap<String, Vec<&EntityDefinition>> = BTreeMap::new();
    for definition in definitions {
        out.entry(definition.entity.clone())
            .or_default()
            .push(definition);
    }
    for versions in out.values_mut() {
        versions.sort_by_key(|definition| definition.version);
    }
    out
}

fn markdown_index(
    definitions: &[EntityDefinition],
    grouped: &BTreeMap<String, Vec<&EntityDefinition>>,
) -> String {
    let mut out = String::from(
        "# Entity reference\n\nGenerated from validated definitions.\n\n## Entities\n\n",
    );
    for (entity, versions) in grouped {
        let listed = versions
            .iter()
            .map(|definition| definition.version.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        let _ = writeln!(
            out,
            "- [{entity}](entities/{}.md) — versions {listed}",
            slug(entity)
        );
    }
    out.push_str("\n## References\n\n```mermaid\n");
    out.push_str(&render::mermaid(&Graph::references(definitions)));
    out.push_str("```\n\n## Machine contracts\n\n- [OpenAPI YAML](openapi.yaml) · [JSON](openapi.json)\n- [AsyncAPI YAML](asyncapi.yaml) · [JSON](asyncapi.json)\n");
    out
}

fn markdown_entity(entity: &str, versions: &[&EntityDefinition]) -> String {
    let mut out = format!("# {entity}\n\n");
    for definition in versions {
        let _ = writeln!(out, "## Version {}\n", definition.version);
        out.push_str("```mermaid\n");
        out.push_str(&render::mermaid(&Graph::lifecycle(definition)));
        out.push_str("```\n\n### Properties\n\n| Property | Type | Required | Constraints |\n|---|---|---:|---|\n");
        for (name, field) in &definition.schema.fields {
            let _ = writeln!(
                out,
                "| {} | {} | {} | {} |",
                md(name),
                field.kind,
                if field.required { "yes" } else { "no" },
                md(&constraints(field))
            );
        }
        out.push_str("\n### Operations\n\n");
        if let Some(event) = &definition.create.emit {
            let _ = writeln!(out, "Creation emits `{}`.\n", md(&event.event_type));
        }
        for (name, operation) in &definition.operations {
            let transitions = operation
                .transitions
                .iter()
                .flat_map(|transition| {
                    transition
                        .from
                        .iter()
                        .map(move |from| format!("{from} → {}", transition.to))
                })
                .collect::<Vec<_>>()
                .join(", ");
            let _ = writeln!(out, "#### {name}\n\nTransitions: {transitions}.\n");
            if !operation.arguments.fields.is_empty() {
                out.push_str("Arguments:\n\n");
                for (argument, field) in &operation.arguments.fields {
                    let _ = writeln!(out, "- `{}` — {}", md(argument), field.kind);
                }
                out.push('\n');
            }
            for rule in &operation.preconditions {
                let _ = writeln!(
                    out,
                    "- Preconditions: {}{}",
                    rule.name.as_deref().unwrap_or("unnamed"),
                    rule.message
                        .as_ref()
                        .map(|message| format!(" — {message}"))
                        .unwrap_or_default()
                );
            }
            let emitted = operation
                .emits
                .iter()
                .map(|event| event.event_type.as_str())
                .collect::<Vec<_>>();
            if !emitted.is_empty() {
                let _ = writeln!(out, "\nEmits: {}.\n", emitted.join(", "));
            }
        }
        if !definition.invariants.is_empty() {
            out.push_str("\n### Invariants\n\n");
            for rule in &definition.invariants {
                let _ = writeln!(
                    out,
                    "- {}{}",
                    rule.name.as_deref().unwrap_or("unnamed"),
                    rule.message
                        .as_ref()
                        .map(|message| format!(" — {message}"))
                        .unwrap_or_default()
                );
            }
        }
        if !definition.projections.is_empty() {
            out.push_str(
                "\n### Projections\n\n| Projection | Key | State filter |\n|---|---|---|\n",
            );
            for (name, projection) in &definition.projections {
                let _ = writeln!(
                    out,
                    "| {} | `{}` | {} |",
                    md(name),
                    md(&projection.key),
                    projection
                        .in_state
                        .as_deref()
                        .map(md)
                        .unwrap_or_else(|| "all".into())
                );
            }
        }
    }
    out
}

fn html_index(
    definitions: &[EntityDefinition],
    grouped: &BTreeMap<String, Vec<&EntityDefinition>>,
) -> String {
    let mut body = String::from(
        "<h1>Entity reference</h1><p>Generated from validated definitions.</p><div class=cards>",
    );
    for (entity, versions) in grouped {
        let listed = versions
            .iter()
            .map(|definition| definition.version.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        let _ = write!(
            body,
            "<a class=card href=\"entities/{}.html\"><strong>{}</strong><span>versions {}</span></a>",
            slug(entity),
            html(entity),
            html(&listed)
        );
    }
    body.push_str("</div><h2>References</h2>");
    let graph = Graph::references(definitions);
    body.push_str(&render::svg(&graph, &Layout::of(&graph)));
    body.push_str("<h2>Machine contracts</h2><p><a href=openapi.yaml>OpenAPI YAML</a> · <a href=openapi.json>JSON</a> · <a href=asyncapi.yaml>AsyncAPI YAML</a> · <a href=asyncapi.json>JSON</a></p>");
    page("Entity reference", "", &body)
}

fn html_entity(entity: &str, versions: &[&EntityDefinition]) -> String {
    let markdown = markdown_entity(entity, versions);
    let mut body = format!(
        "<a href=\"../index.html\">← All entities</a><h1>{}</h1>",
        html(entity)
    );
    for definition in versions {
        let _ = write!(body, "<section><h2>Version {}</h2>", definition.version);
        let graph = Graph::lifecycle(definition);
        body.push_str(&render::svg(&graph, &Layout::of(&graph)));
        body.push_str("<h3>Properties</h3><table><tr><th>Property</th><th>Type</th><th>Required</th><th>Constraints</th></tr>");
        for (name, field) in &definition.schema.fields {
            let _ = write!(
                body,
                "<tr><td><code>{}</code></td><td>{}</td><td>{}</td><td>{}</td></tr>",
                html(name),
                field.kind,
                if field.required { "yes" } else { "no" },
                html(&constraints(field))
            );
        }
        body.push_str("</table><h3>Operations</h3>");
        if let Some(event) = &definition.create.emit {
            let _ = write!(
                body,
                "<p>Creation emits <code>{}</code>.</p>",
                html(&event.event_type)
            );
        }
        for (name, operation) in &definition.operations {
            let _ = write!(
                body,
                "<article><h4>{}</h4><p>Transitions:</p><ul>",
                html(name)
            );
            for transition in &operation.transitions {
                for from in transition.from.iter() {
                    let _ = write!(body, "<li>{} → {}</li>", html(from), html(&transition.to));
                }
            }
            body.push_str("</ul>");
            if !operation.arguments.fields.is_empty() {
                body.push_str("<p>Arguments:</p><table><tr><th>Argument</th><th>Type</th><th>Required</th><th>Constraints</th></tr>");
                for (argument, field) in &operation.arguments.fields {
                    let _ = write!(
                        body,
                        "<tr><td><code>{}</code></td><td>{}</td><td>{}</td><td>{}</td></tr>",
                        html(argument),
                        field.kind,
                        if field.required { "yes" } else { "no" },
                        html(&constraints(field))
                    );
                }
                body.push_str("</table>");
            }
            if !operation.preconditions.is_empty() {
                body.push_str("<p>Preconditions:</p><ul>");
                for rule in &operation.preconditions {
                    let _ = write!(
                        body,
                        "<li><strong>{}</strong>{}</li>",
                        html(rule.name.as_deref().unwrap_or("unnamed")),
                        rule.message
                            .as_ref()
                            .map(|message| format!(" — {}", html(message)))
                            .unwrap_or_default()
                    );
                }
                body.push_str("</ul>");
            }
            if !operation.set.is_empty() {
                body.push_str("<p>Sets: ");
                for (at, field) in operation.set.keys().enumerate() {
                    if at > 0 {
                        body.push_str(", ");
                    }
                    let _ = write!(body, "<code>{}</code>", html(field));
                }
                body.push_str(".</p>");
            }
            if !operation.emits.is_empty() {
                body.push_str("<p>Emits: ");
                for (at, event) in operation.emits.iter().enumerate() {
                    if at > 0 {
                        body.push_str(", ");
                    }
                    let _ = write!(body, "<code>{}</code>", html(&event.event_type));
                }
                body.push_str(".</p>");
            }
            body.push_str("</article>");
        }
        if !definition.invariants.is_empty() {
            body.push_str("<h3>Invariants</h3><ul>");
            for rule in &definition.invariants {
                let _ = write!(
                    body,
                    "<li><strong>{}</strong>{}</li>",
                    html(rule.name.as_deref().unwrap_or("unnamed")),
                    rule.message
                        .as_ref()
                        .map(|message| format!(" — {}", html(message)))
                        .unwrap_or_default()
                );
            }
            body.push_str("</ul>");
        }
        if !definition.projections.is_empty() {
            body.push_str("<h3>Projections</h3><table><tr><th>Projection</th><th>Key</th><th>State filter</th></tr>");
            for (name, projection) in &definition.projections {
                let _ = write!(
                    body,
                    "<tr><td>{}</td><td><code>{}</code></td><td>{}</td></tr>",
                    html(name),
                    html(&projection.key),
                    projection
                        .in_state
                        .as_deref()
                        .map(html)
                        .unwrap_or_else(|| "all".into())
                );
            }
            body.push_str("</table>");
        }
        body.push_str("</section>");
    }
    body.push_str("<details><summary>Markdown source view</summary><pre>");
    body.push_str(&html(&markdown));
    body.push_str("</pre></details>");
    page(entity, "../", &body)
}

fn page(title: &str, prefix: &str, body: &str) -> String {
    format!(
        "<!doctype html><html lang=en><meta charset=utf-8><meta name=viewport content=\"width=device-width,initial-scale=1\"><title>{}</title><link rel=stylesheet href=\"{}assets/style.css\"><main>{}</main></html>\n",
        html(title), prefix, body
    )
}

fn constraints(field: &FieldDefinition) -> String {
    let mut parts = Vec::new();
    if let Some(min) = field.min_length {
        parts.push(format!("min length {min}"));
    }
    if let Some(max) = field.max_length {
        parts.push(format!("max length {max}"));
    }
    if let Some(alphabet) = &field.alphabet {
        parts.push(format!("alphabet {alphabet:?}"));
    }
    if let Some(min) = &field.min {
        parts.push(format!("minimum {min}"));
    }
    if let Some(max) = &field.max {
        parts.push(format!("maximum {max}"));
    }
    if !field.values.is_empty() {
        parts.push(format!("one of {}", field.values.join(", ")));
    }
    if let Some(entity) = &field.entity {
        parts.push(format!("reference to {entity}"));
    }
    if let Some(default) = field.default.as_value() {
        parts.push(format!("default {default}"));
    }
    parts.join("; ")
}

fn schema_key(definition: &EntityDefinition) -> String {
    let name = &definition.entity;
    if name.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
        && name.bytes().all(|byte| byte.is_ascii_alphanumeric())
    {
        format!("{}V{}", pascal(name), definition.version)
    } else {
        let encoded: String = name.bytes().map(|byte| format!("{byte:02X}")).collect();
        format!("Entity_{encoded}V{}", definition.version)
    }
}

fn pascal(value: &str) -> String {
    let mut out = String::new();
    let mut upper = true;
    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            if upper {
                out.extend(character.to_uppercase());
            } else {
                out.push(character);
            }
            upper = false;
        } else {
            upper = true;
        }
    }
    if out.is_empty() {
        "Entity".into()
    } else {
        out
    }
}

fn safe_identifier(value: &str) -> String {
    value
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() {
                char::from(byte).to_string()
            } else {
                format!("_{byte:02X}")
            }
        })
        .collect()
}

fn hex_identifier(value: &str) -> String {
    value.bytes().map(|byte| format!("{byte:02X}")).collect()
}

fn path_segment(value: &str) -> String {
    let mut out = String::new();
    for byte in value.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(char::from(*byte));
        } else {
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

fn pointer_segment(value: &str) -> String {
    value.replace('~', "~0").replace('/', "~1")
}

fn slug(value: &str) -> String {
    let mut out = String::new();
    for byte in value.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_') {
            out.push(char::from(*byte));
        } else {
            let _ = write!(out, "~{byte:02X}");
        }
    }
    if out.is_empty() {
        "entity".into()
    } else {
        out
    }
}

fn html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn md(value: &str) -> String {
    value.replace('|', "&#124;").replace('`', "&#96;")
}

fn pretty_json(value: &Value) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(value).map(|mut text| {
        text.push('\n');
        text
    })
}

/// Writes a contract as YAML in which every number is a plain YAML number spelled with the digits
/// `pretty_json` writes for it.
///
/// `serde_json`'s `arbitrary_precision` feature, which the kernel's exact numbers need, serializes
/// a number as a private one-entry map; the JSON writer knows that map and the YAML writer wrote it
/// literally. The YAML writer spells a plain number only from a native integer or an `f64`, and an
/// `f64` changes digits it cannot hold (`1.50`, `1e3`, `0.1000000000000000000001`). So each number
/// is first written as a placeholder word, then the word is replaced by the number's own digits.
/// The word spells no null, boolean or number and carries no indicator character, so the writer
/// leaves it plain and the digits that replace it are a plain scalar too.
/// A placeholder is trusted only when the text holds its marker exactly once per number; any other
/// occurrence comes from a string in the contract, and the next marker is tried instead.
fn yaml(contract: &Value) -> Result<String, String> {
    let mut generation = 0_u64;
    loop {
        let marker = format!("{NUMBER_MARKER}{generation}-");
        let mut digits = Vec::new();
        let placeheld = numbers_as_placeholders(contract, &marker, &mut digits);
        let text = serde_yaml_ng::to_string(&placeheld).map_err(|error| error.to_string())?;
        if text.matches(marker.as_str()).count() == digits.len() {
            return digits_for_placeholders(&text, &marker, &digits);
        }
        generation += 1;
    }
}

/// The contract with each number replaced by `marker` and the number's index into `digits`.
fn numbers_as_placeholders(value: &Value, marker: &str, digits: &mut Vec<String>) -> Value {
    match value {
        Value::Number(number) => {
            let placeholder = format!("{marker}{}", digits.len());
            digits.push(number.to_string());
            Value::String(placeholder)
        }
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| numbers_as_placeholders(item, marker, digits))
                .collect(),
        ),
        Value::Object(members) => Value::Object(
            members
                .iter()
                .map(|(name, member)| {
                    (
                        name.clone(),
                        numbers_as_placeholders(member, marker, digits),
                    )
                })
                .collect(),
        ),
        other => other.clone(),
    }
}

/// Replaces every placeholder `numbers_as_placeholders` wrote with the digits it stands for.
fn digits_for_placeholders(text: &str, marker: &str, digits: &[String]) -> Result<String, String> {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(marker) {
        out.push_str(&rest[..at]);
        rest = &rest[at + marker.len()..];
        let end = rest
            .find(|character: char| !character.is_ascii_digit())
            .unwrap_or(rest.len());
        let number = rest[..end]
            .parse::<usize>()
            .ok()
            .and_then(|index| digits.get(index))
            .ok_or_else(|| {
                format!(
                    "YAML number placeholder {marker}{} is not one written",
                    &rest[..end]
                )
            })?;
        out.push_str(number);
        rest = &rest[end..];
    }
    out.push_str(rest);
    Ok(out)
}

const STYLE: &str = r#":root{color-scheme:light dark;font:16px/1.55 system-ui,sans-serif}body{margin:0}main{max-width:1100px;margin:auto;padding:2rem}a{color:#6854d9}.cards{display:grid;grid-template-columns:repeat(auto-fit,minmax(12rem,1fr));gap:1rem}.card{border:1px solid #7775;border-radius:.7rem;padding:1rem;text-decoration:none;display:flex;flex-direction:column}.card span{opacity:.7}table{border-collapse:collapse;width:100%;margin:1rem 0}th,td{border:1px solid #7775;padding:.45rem;text-align:left}code,pre{font-family:ui-monospace,monospace}svg{max-width:100%;height:auto;background:#fff;border-radius:.5rem}section{margin-block:2rem}details{margin-top:3rem}pre{white-space:pre-wrap;overflow:auto}"#;

#[cfg(test)]
mod tests {
    use super::*;

    fn definition() -> EntityDefinition {
        serde_json::from_value(json!({
            "entity": "refund",
            "schema": { "fields": {
                "amount": { "type": "integer", "required": true, "min": 0 },
                "customer": { "type": "ref", "entity": "customer" }
            }},
            "lifecycle": { "initial": "draft", "states": ["draft", "approved"] },
            "operations": { "approve": {
                "arguments": { "fields": { "reason": { "type": "string", "required": true } } },
                "transitions": [{ "from": "draft", "to": "approved" }],
                "emits": [{ "type": "RefundApproved", "payload": { "reason": "$args.reason" } }]
            }}
        }))
        .expect("definition")
    }

    #[test]
    fn overlapping_emitters_accept_real_events_in_the_projected_schema() {
        let mut definition = definition();
        let approve = definition.operations["approve"].clone();
        definition.operations.insert("also_approve".into(), approve);
        let spec = asyncapi(&[definition.clone()]);
        let mut registry = entity_core::Registry::new();
        registry.register(definition).unwrap();
        let runtime = entity_core::Runtime::new(&registry);
        let created = runtime
            .create("refund", 1, "one", json!({"amount": 100}))
            .unwrap();
        let decision = runtime
            .execute(&created.instance, "approve", json!({"reason": "accepted"}))
            .unwrap();
        let schema = &spec["components"]["messages"]["refund_RefundApproved"]["payload"];
        assert!(jsonschema::is_valid(
            schema,
            &serde_json::to_value(&decision.events[0]).unwrap()
        ));
        let mut invalid = serde_json::to_value(&decision.events[0]).unwrap();
        invalid["payload"]["reason"] = json!(42);
        assert!(!jsonschema::is_valid(schema, &invalid));
    }

    /// The review's `outcome.yaml` (`service/1`, the event declared on the creation branch), with
    /// one operation whose event is declared on an operation branch and reads its arguments.
    fn parcel() -> EntityDefinition {
        serde_json::from_value(json!({
            "entity": "parcel",
            "version": 1,
            "semantics": "service/1",
            "schema": { "fields": {} },
            "lifecycle": { "initial": "open", "states": ["open", "sealed"] },
            "create": { "outcomes": [{
                "name": "opened",
                "effect": "creates",
                "emits": [{ "type": "ParcelOpened", "payload": {} }]
            }]},
            "operations": { "seal": {
                "arguments": { "fields": { "reason": { "type": "string", "required": true } } },
                "outcomes": [{
                    "name": "sealed",
                    "effect": { "moves": { "from": "open", "to": "sealed" } },
                    "emits": [{ "type": "ParcelSealed", "payload": { "reason": "$args.reason" } }]
                }]
            }}
        }))
        .expect("the fixture is a definition document")
    }

    #[test]
    fn events_declared_on_creation_and_operation_outcomes_are_published_with_their_payloads() {
        let definition = parcel();
        let spec = asyncapi(std::slice::from_ref(&definition));
        let mut registry = entity_core::Registry::new();
        registry
            .register(definition)
            .expect("the fixture registers");
        let runtime = entity_core::Runtime::new(&registry);
        let created = runtime
            .create("parcel", 1, "one", json!({}))
            .expect("created");
        let sealed = runtime
            .execute(&created.instance, "seal", json!({ "reason": "full" }))
            .expect("sealed");

        for (channel, message, event) in [
            (
                "parcel.ParcelOpened",
                "parcel_ParcelOpened",
                &created.events,
            ),
            ("parcel.ParcelSealed", "parcel_ParcelSealed", &sealed.events),
        ] {
            assert_eq!(
                spec["channels"][channel]["messages"][message]["$ref"],
                format!("#/components/messages/{message}"),
                "the outcome's event has no channel: {spec}"
            );
            assert!(
                spec["operations"][format!("receive_{message}")].is_object(),
                "the outcome's event has no receive operation: {spec}"
            );
            let schema = &spec["components"]["messages"][message]["payload"];
            let emitted = serde_json::to_value(&event[0]).expect("an event serializes");
            assert!(
                jsonschema::is_valid(schema, &emitted),
                "the published message refuses the event the kernel emits: {emitted} {schema}"
            );
        }
        let schema = &spec["components"]["messages"]["parcel_ParcelSealed"]["payload"];
        assert_eq!(
            schema["properties"]["payload"]["properties"]["reason"]["type"], "string",
            "an operation branch's event payload keeps its declared argument schema"
        );
        let mut wrong = serde_json::to_value(&sealed.events[0]).expect("an event serializes");
        wrong["payload"]["reason"] = json!(42);
        assert!(!jsonschema::is_valid(schema, &wrong));
    }

    /// The schema an OpenAPI create request's `fields` member names, with its `$ref` followed.
    fn create_fields_schema(spec: &Value, collection: &str) -> Value {
        let reference = spec["paths"][collection]["post"]["requestBody"]["content"]
            ["application/json"]["schema"]["properties"]["fields"]["$ref"]
            .as_str()
            .expect("the create request names its input schema");
        let name = reference
            .strip_prefix("#/components/schemas/")
            .expect("a local component");
        spec["components"]["schemas"][name].clone()
    }

    #[test]
    fn a_branched_creation_request_takes_the_creation_arguments_the_kernel_accepts() {
        // The review's `runtime/parcel.yaml`.
        let definition: EntityDefinition = serde_json::from_value(json!({
            "entity": "parcel",
            "version": 1,
            "semantics": "service/1",
            "schema": { "fields": { "label": { "type": "string", "required": true } } },
            "lifecycle": { "initial": "open", "states": ["open"] },
            "create": {
                "arguments": { "fields": { "label_text": { "type": "string", "required": true } } },
                "outcomes": [{
                    "name": "opened", "effect": "creates", "set": { "label": "$args.label_text" }
                }]
            },
            "operations": {}
        }))
        .expect("the fixture is a definition document");
        let spec = openapi(std::slice::from_ref(&definition));
        let input = create_fields_schema(&spec, "/entities/parcel/versions/1");
        let accepted = json!({ "label_text": "Post" });
        assert!(
            jsonschema::is_valid(&input, &accepted),
            "the create request refuses the creation arguments the kernel accepts: {input}"
        );
        assert!(
            !jsonschema::is_valid(&input, &json!({ "label": "Post" })),
            "the create request admits instance fields the kernel refuses as arguments: {input}"
        );

        let mut registry = entity_core::Registry::new();
        registry
            .register(definition)
            .expect("the fixture registers");
        let created = entity_core::Runtime::new(&registry)
            .create("parcel", 1, "parcel-1", accepted)
            .expect("the kernel accepts the arguments");
        assert_eq!(created.instance.fields["label"], "Post");

        // The direct field-based path keeps the instance fields.
        let direct = openapi(&[self::definition()]);
        assert_eq!(
            create_fields_schema(&direct, "/entities/refund/versions/1"),
            object_schema(&self::definition().schema)
        );
    }

    /// Every `operationId` of a document, keyed by `<METHOD> <path>`.
    fn operation_ids(spec: &Value) -> BTreeMap<String, String> {
        let mut out = BTreeMap::new();
        for (path, item) in spec["paths"].as_object().expect("paths") {
            for (method, operation) in item.as_object().expect("a path item") {
                out.insert(
                    format!("{} {path}", method.to_uppercase()),
                    operation["operationId"].as_str().expect("an id").to_owned(),
                );
            }
        }
        out
    }

    /// The review's `round3/runtime/counter-list.yaml`: a domain operation named `list`, and two
    /// required fields the kernel fills from their declared defaults.
    fn counter() -> EntityDefinition {
        serde_json::from_value(json!({
            "entity": "counter",
            "version": 1,
            "schema": { "additional_fields": false, "fields": {
                "name": { "type": "string", "required": true, "min_length": 1 },
                "views": { "type": "integer", "required": true, "min": 0, "default": 0 },
                "spent": { "type": "number", "required": true, "default": 0 }
            }},
            "lifecycle": { "initial": "open", "states": ["open", "closed"] },
            "create": { "emit": {
                "type": "CounterOpened",
                "payload": { "name": "$fields.name", "views": "$fields.views" }
            }},
            "operations": {
                "list": {
                    "transitions": [{ "from": "open", "to": "open" }],
                    "set": { "views": { "increment": 1 } },
                    "emits": [{ "type": "Viewed", "payload": {
                        "before": "$old_fields.views", "after": "$fields.views"
                    }}]
                },
                "unview": {
                    "transitions": [{ "from": "open", "to": "open" }],
                    "set": { "views": { "increment": -1 } }
                },
                "charge": {
                    "arguments": { "fields": {
                        "amount": { "type": "number", "required": true, "min": 0 }
                    }},
                    "transitions": [{ "from": "open", "to": "open" }],
                    "set": { "spent": { "increment": "$args.amount" } },
                    "emits": [{ "type": "Charged", "payload": {
                        "amount": "$args.amount", "spent": "$fields.spent"
                    }}]
                },
                "close": { "transitions": [{ "from": "open", "to": "closed" }] }
            }
        }))
        .expect("the fixture is a definition document")
    }

    #[test]
    fn a_required_member_with_a_declared_default_is_optional_to_send_and_stays_stored() {
        let mut counter = counter();
        counter
            .operations
            .get_mut("charge")
            .expect("charge")
            .arguments
            .fields
            .insert(
                "note".into(),
                serde_json::from_value(
                    json!({ "type": "string", "required": true, "default": "-" }),
                )
                .expect("a field"),
            );
        let spec = openapi(std::slice::from_ref(&counter));
        let input = create_fields_schema(&spec, "/entities/counter/versions/1");
        let sent = json!({ "name": "c" });
        assert!(
            jsonschema::is_valid(&input, &sent),
            "the create request requires a field the kernel fills from its default: {input}"
        );
        assert!(
            !jsonschema::is_valid(&input, &json!({})),
            "name has no default: {input}"
        );
        assert!(
            !jsonschema::is_valid(&input, &json!({ "name": "c", "views": -1 })),
            "a sent defaulted field is still checked: {input}"
        );

        let mut registry = entity_core::Registry::new();
        registry
            .register(counter.clone())
            .expect("the fixture registers");
        let runtime = entity_core::Runtime::new(&registry);
        let created = runtime
            .create("counter", 1, "c-1", sent)
            .expect("the kernel fills the defaults");
        // The stored shape keeps every required field, and the instance it describes has them.
        let stored = &spec["components"]["schemas"]["CounterV1Fields"];
        assert_eq!(stored["required"], json!(["name", "spent", "views"]));
        assert_eq!(
            spec["components"]["schemas"]["CounterV1Instance"]["properties"]["fields"]["$ref"],
            "#/components/schemas/CounterV1Fields"
        );
        assert!(jsonschema::is_valid(
            stored,
            &Value::Object(created.instance.fields.clone())
        ));

        let charge = &spec["paths"]["/entities/counter/versions/1/{id}/operations/charge"]["post"]
            ["requestBody"]["content"]["application/json"]["schema"]["properties"]["arguments"];
        assert_eq!(
            charge["required"],
            json!(["amount"]),
            "an operation argument with a default is optional to send: {charge}"
        );
        runtime
            .execute(&created.instance, "charge", json!({ "amount": 2 }))
            .expect("the kernel fills the argument's default");
    }

    #[test]
    fn every_operation_id_of_a_generated_document_is_unique_and_unique_names_are_kept() {
        let counter = counter();
        // Every generated verb taken by a domain operation.
        let mut taken = self::definition();
        taken.entity = "ticket".into();
        let approve = taken.operations["approve"].clone();
        for name in ["create", "get", "events", "list"] {
            taken.operations.insert(name.into(), approve.clone());
        }

        let spec = openapi(&[counter, taken]);
        let ids = operation_ids(&spec);
        let distinct: BTreeSet<&String> = ids.values().collect();
        assert_eq!(distinct.len(), ids.len(), "duplicate operationId: {ids:#?}");

        let counter_ids: BTreeMap<&str, &str> = ids
            .iter()
            .filter(|(at, _)| at.contains("/entities/counter/"))
            .map(|(at, id)| (at.as_str(), id.as_str()))
            .collect();
        assert_eq!(
            counter_ids,
            BTreeMap::from([
                ("GET /entities/counter/versions/1", "list_counter_v1"),
                ("POST /entities/counter/versions/1", "create_counter_v1"),
                ("GET /entities/counter/versions/1/{id}", "get_counter_v1"),
                (
                    "GET /entities/counter/versions/1/{id}/events",
                    "events_counter_v1"
                ),
                (
                    "POST /entities/counter/versions/1/{id}/operations/charge",
                    "charge_counter_v1"
                ),
                (
                    "POST /entities/counter/versions/1/{id}/operations/close",
                    "close_counter_v1"
                ),
                (
                    "POST /entities/counter/versions/1/{id}/operations/list",
                    "list_counter_v1_2"
                ),
                (
                    "POST /entities/counter/versions/1/{id}/operations/unview",
                    "unview_counter_v1"
                ),
            ]),
            "the generated method keeps its name and the domain operation takes the suffix"
        );
        for verb in ["create", "get", "events", "list"] {
            assert_eq!(
                ids[&format!("POST /entities/ticket/versions/1/{{id}}/operations/{verb}")],
                format!("{verb}_ticket_v1_2"),
                "{ids:#?}"
            );
        }

        // A document with no collision is unchanged.
        let plain = operation_ids(&openapi(&[self::definition()]));
        assert_eq!(
            plain.values().map(String::as_str).collect::<Vec<_>>(),
            [
                "list_refund_v1",
                "get_refund_v1",
                "events_refund_v1",
                "create_refund_v1",
                "approve_refund_v1"
            ]
        );

        // A suffix another method already holds is skipped. No pair of generated names spells
        // one today, so the document is written by hand.
        let mut paths = Map::new();
        for (path, id) in [("/a", "x"), ("/b", "x"), ("/c", "x_2")] {
            paths.insert(path.into(), json!({ "get": { "operationId": id } }));
        }
        reserve_unique_operation_ids(&mut paths);
        assert_eq!(
            [&paths["/a"], &paths["/b"], &paths["/c"]]
                .map(|item| item["get"]["operationId"].clone()),
            [json!("x"), json!("x_3"), json!("x_2")]
        );
    }

    #[test]
    fn punctuation_case_and_composed_names_never_alias_contract_components() {
        let names = ["foo-bar", "foo_bar", "fooBar", "FooBar", "a.b", "a", "é"];
        let definitions: Vec<_> = names
            .iter()
            .map(|name| {
                let mut definition = definition();
                definition.entity = (*name).into();
                if *name == "a" {
                    definition.operations.get_mut("approve").unwrap().emits[0].event_type =
                        "b.RefundApproved".into();
                }
                definition
            })
            .collect();
        let spec = openapi(&definitions);
        for definition in &definitions {
            assert_eq!(
                spec["components"]["schemas"][format!("{}Fields", schema_key(definition))]
                    ["properties"]["amount"]["type"],
                "integer"
            );
        }
        let keys: std::collections::BTreeSet<_> = definitions.iter().map(schema_key).collect();
        assert_eq!(keys.len(), definitions.len());
        let events = asyncapi(&definitions);
        assert_eq!(
            events["channels"].as_object().unwrap().len(),
            definitions.len()
        );
        assert_eq!(
            events["components"]["messages"].as_object().unwrap().len(),
            definitions.len()
        );
    }

    #[test]
    fn one_projection_feeds_docs_openapi_and_asyncapi() {
        let definition = definition();
        let bundle = documentation(&[definition]).expect("bundle");
        for path in [
            DOCS_MARKER,
            "index.html",
            "index.md",
            "entities/refund.html",
            "entities/refund.md",
            "openapi.yaml",
            "openapi.json",
            "asyncapi.yaml",
            "asyncapi.json",
        ] {
            assert!(bundle.contains_key(path), "missing {path}");
        }
        assert!(bundle["index.md"].contains("flowchart LR"));
        assert!(bundle["entities/refund.md"].contains("stateDiagram-v2"));
        assert!(bundle["openapi.json"].contains("expected_revision"));
        assert!(bundle["asyncapi.json"].contains("refund.RefundApproved"));
        let events: Value = serde_json::from_str(&bundle["asyncapi.json"]).expect("AsyncAPI JSON");
        assert_eq!(
            events["components"]["messages"]["refund_RefundApproved"]["payload"]["properties"]
                ["payload"]["properties"]["reason"]["type"],
            "string",
            "event payload references retain their declared argument schema"
        );
    }

    /// A definition read from JSON keeps each number's digits, so its contract can carry numbers no
    /// `u64`, `i64` or `f64` holds and spellings an `f64` would change (`1.50`, `1e3`).
    fn exact_number_definition() -> EntityDefinition {
        serde_json::from_str(
            r#"{
                "entity": "meter",
                "schema": { "fields": {
                    "reading": {
                        "type": "integer",
                        "min": -170141183460469231731687303715884105729,
                        "max": 340282366920938463463374607431768211456,
                        "default": 18446744073709551616
                    },
                    "rate": {
                        "type": "number",
                        "min": 0.1000000000000000000001,
                        "max": 1e3,
                        "default": 1.50
                    },
                    "steps": {
                        "type": "array",
                        "items": { "type": "number" },
                        "default": [-1.50, 2]
                    }
                }},
                "lifecycle": { "initial": "open", "states": ["open"] },
                "operations": {}
            }"#,
        )
        .expect("definition")
    }

    #[test]
    fn a_yaml_contract_spells_every_number_with_the_json_contracts_digits() {
        let bundle = documentation(&[exact_number_definition()]).expect("bundle");
        let yaml = &bundle["openapi.yaml"];
        let json = &bundle["openapi.json"];
        for (key, digits) in [
            ("minimum", "-170141183460469231731687303715884105729"),
            ("maximum", "340282366920938463463374607431768211456"),
            ("default", "18446744073709551616"),
            ("minimum", "0.1000000000000000000001"),
            // `serde_json` keeps the authored `1e3` as `1e+3`; that is the JSON contract's spelling.
            ("maximum", "1e+3"),
            ("default", "1.50"),
        ] {
            assert!(
                json.contains(&format!("\"{key}\": {digits}")),
                "the JSON contract does not spell {key} {digits}:\n{json}"
            );
            assert!(
                yaml.contains(&format!(" {key}: {digits}\n")),
                "openapi.yaml does not write {key} as the plain number {digits}:\n{yaml}"
            );
        }
        assert!(
            yaml.contains(" - -1.50\n"),
            "openapi.yaml does not write the array default's -1.50 as a plain sequence item:\n{yaml}"
        );
        for contract in ["openapi.yaml", "asyncapi.yaml"] {
            assert!(
                !bundle[contract].contains("serde_json")
                    && !bundle[contract].contains(NUMBER_MARKER),
                "{contract} leaks a private number spelling:\n{}",
                bundle[contract]
            );
        }
        // Read back into `serde_json::Value`, the one value model here that holds a 128-bit
        // integer. It would read a private number map as a number too, which the assertion above
        // has already excluded.
        let read: Value = serde_yaml_ng::from_str(yaml).expect("openapi.yaml parses");
        let fields = &read["components"]["schemas"]["MeterV1Fields"]["properties"];
        for field in ["reading", "rate"] {
            for key in ["minimum", "maximum", "default"] {
                assert!(
                    fields[field][key].is_number(),
                    "{field} {key} reads back as {}",
                    fields[field][key]
                );
            }
        }
        assert_eq!(
            fields["steps"]["default"]
                .as_array()
                .map(|items| items.iter().map(Value::as_f64).collect::<Vec<_>>()),
            Some(vec![Some(-1.5), Some(2.0)]),
            "the array default reads back as numbers"
        );
    }

    #[test]
    fn a_contract_string_that_spells_the_number_placeholder_survives_unchanged() {
        let first = format!("{NUMBER_MARKER}0-0");
        let second = format!("{NUMBER_MARKER}1-");
        let mut definition = exact_number_definition();
        definition.schema.fields.insert(
            "label".into(),
            serde_json::from_value(json!({ "type": "enum", "values": [first, second] }))
                .expect("enum field"),
        );
        let bundle = documentation(&[definition]).expect("bundle");
        let yaml = &bundle["openapi.yaml"];
        assert!(!yaml.contains("serde_json"), "{yaml}");
        let read: Value = serde_yaml_ng::from_str(yaml).expect("openapi.yaml parses");
        let fields = &read["components"]["schemas"]["MeterV1Fields"]["properties"];
        assert_eq!(
            fields["label"]["enum"],
            json!([first, second]),
            "the enum values are the declared strings:\n{yaml}"
        );
        assert!(
            yaml.contains(" default: 1.50\n") && fields["rate"]["default"].is_number(),
            "the number beside them is still a number:\n{yaml}"
        );
    }

    #[test]
    fn a_declared_alphabet_reaches_the_api_schema_and_the_reference_page() {
        let definition: EntityDefinition = serde_json::from_value(json!({
            "entity": "keypad",
            "semantics": "service/1",
            "schema": { "fields": {
                "keys": { "type": "string", "alphabet": "0123456789*#", "max_length": 64 },
                "label": { "type": "string" }
            }},
            "lifecycle": { "initial": "idle", "states": ["idle"] }
        }))
        .expect("definition");
        let fields = &openapi(std::slice::from_ref(&definition))["components"]["schemas"]
            [format!("{}Fields", schema_key(&definition))]["properties"];
        assert_eq!(fields["keys"]["x-alphabet"], "0123456789*#");
        assert_eq!(
            fields["label"].get("x-alphabet"),
            None,
            "a field without an alphabet publishes none"
        );
        let bundle = documentation(&[definition]).expect("bundle");
        assert!(
            bundle["entities/keypad.md"].contains(r#"| max length 64; alphabet "0123456789*#" |"#),
            "{}",
            bundle["entities/keypad.md"]
        );
    }

    #[test]
    fn unsafe_names_never_become_paths_or_markup() {
        assert_eq!(slug("../a"), "~2E~2E~2Fa");
        assert_eq!(html("<script>"), "&lt;script&gt;");
    }
}
