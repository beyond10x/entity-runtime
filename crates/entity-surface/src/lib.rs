//! Deterministic public surfaces projected from validated entity definitions.
//!
//! This crate performs no IO. Callers receive a stable map of relative paths to bytes and decide
//! where, or whether, to write them. The same schema projection feeds human documentation,
//! OpenAPI, AsyncAPI and MCP so an argument cannot be documented one way and offered to an agent
//! another way.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use entity_core::{DeclaredDefault, EntityDefinition, FieldDefinition, FieldKind, ObjectSchema};
use entity_graph::{render, Graph, Layout};
use serde_json::{json, Map, Value};

/// Marker written into every generated documentation directory.
pub const DOCS_MARKER: &str = ".entity-runtime-docs.json";

/// A complete generated documentation directory, keyed by safe relative path.
pub type DocumentationBundle = BTreeMap<String, String>;

/// Projects an entity object schema into JSON Schema 2020-12 vocabulary.
#[must_use]
pub fn object_schema(schema: &ObjectSchema) -> Value {
    let mut properties = Map::new();
    let mut required = Vec::new();
    for (name, field) in &schema.fields {
        properties.insert(name.clone(), field_schema(field));
        if field.required {
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

/// Generates the canonical contract-only OpenAPI 3.2 document for a definition set.
#[must_use]
pub fn openapi(definitions: &[EntityDefinition]) -> Value {
    let mut schemas = Map::new();
    let mut paths = Map::new();
    for definition in definitions {
        let key = schema_key(definition);
        schemas.insert(format!("{key}Fields"), object_schema(&definition.schema));
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
                            "fields": { "$ref": format!("#/components/schemas/{key}Fields") },
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
                            "arguments": object_schema(&operation.arguments),
                            "recording": { "$ref": "#/components/schemas/Recording" }
                        }
                    }}}},
                    "responses": standard_write_responses(&format!("#/components/schemas/{key}Instance"))
                }
            }));
        }
    }
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

/// Generates the contract-only AsyncAPI 3.1 document for every emitted event type.
#[must_use]
pub fn asyncapi(definitions: &[EntityDefinition]) -> Value {
    let mut channels = Map::new();
    let mut messages = Map::new();
    let mut operations = Map::new();
    let mut declared: BTreeMap<String, (String, String, Vec<Value>)> = BTreeMap::new();
    for definition in definitions {
        if let Some(event) = &definition.create.emit {
            declared
                .entry(format!("{}.{}", definition.entity, event.event_type))
                .or_insert_with(|| {
                    (
                        definition.entity.clone(),
                        event.event_type.clone(),
                        Vec::new(),
                    )
                })
                .2
                .push(event_envelope_schema(definition, event, None));
        }
        for operation in definition.operations.values() {
            for event in &operation.emits {
                declared
                    .entry(format!("{}.{}", definition.entity, event.event_type))
                    .or_insert_with(|| {
                        (
                            definition.entity.clone(),
                            event.event_type.clone(),
                            Vec::new(),
                        )
                    })
                    .2
                    .push(event_envelope_schema(
                        definition,
                        event,
                        Some(&operation.arguments),
                    ));
            }
        }
    }
    for (channel_name, (entity, event_type, schemas)) in declared {
        let message_key = safe_identifier(&channel_name);
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
            json!({ "oneOf": schemas })
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
    files.insert(
        "openapi.yaml".into(),
        serde_yaml_ng::to_string(&openapi).map_err(|error| error.to_string())?,
    );
    files.insert(
        "asyncapi.json".into(),
        pretty_json(&asyncapi).map_err(|error| error.to_string())?,
    );
    files.insert(
        "asyncapi.yaml".into(),
        serde_yaml_ng::to_string(&asyncapi).map_err(|error| error.to_string())?,
    );
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
                out.insert("items".into(), field_schema(items));
            }
        }
        FieldKind::Object => {
            let schema = ObjectSchema {
                fields: field.properties.clone(),
                additional_fields: field.additional_properties,
            };
            return with_default(object_schema(&schema), &field.default);
        }
        FieldKind::Json => {}
        FieldKind::Ref => {
            set_type(&mut out, "string");
            out.insert("minLength".into(), json!(1));
            if let Some(entity) = &field.entity {
                out.insert("x-entity-ref".into(), Value::String(entity.clone()));
            }
        }
    }
    if let Some(min) = field.min_length {
        out.insert("minLength".into(), json!(min));
    }
    if let Some(max) = field.max_length {
        out.insert("maxLength".into(), json!(max));
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

fn domain_event_schema() -> Value {
    json!({
        "type": "object", "additionalProperties": false,
        "required": ["entity", "version", "id", "revision", "type", "from_state", "to_state", "changed", "args", "payload"],
        "properties": {
            "entity": { "type": "string" }, "version": { "type": "integer", "minimum": 1 },
            "id": { "type": "string" }, "revision": { "type": "integer", "minimum": 1 },
            "type": { "type": "string" }, "from_state": { "type": ["string", "null"] },
            "to_state": { "type": "string" }, "changed": { "type": "object" },
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
    format!("{}V{}", pascal(&definition.entity), definition.version)
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
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '_' {
                character
            } else {
                '_'
            }
        })
        .collect()
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

    #[test]
    fn unsafe_names_never_become_paths_or_markup() {
        assert_eq!(slug("../a"), "~2E~2E~2Fa");
        assert_eq!(html("<script>"), "&lt;script&gt;");
    }
}
