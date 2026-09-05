//! A narrow synchronous MCP server over validated entities and a provider.
//!
//! The protocol adapter deliberately exposes tools only: no resources, prompts, sampling, network
//! listener or implicit session state. It accepts both the 2025 initialization era and the
//! stateless 2026 discovery era, and speaks newline-delimited JSON-RPC through caller-provided
//! readers and writers.

use std::collections::BTreeMap;
use std::io::{BufRead, Write};

use entity_core::{EntityDefinition, Registry};
use entity_shell::{ShellError, StoredRuntime};
use entity_store::{Recording, Store};
use serde_json::{json, Map, Value};

const MODERN: &str = "2026-07-28";
const LEGACY: &str = "2025-11-25";
const RESERVED: [&str; 4] = ["create", "get", "list", "events"];

/// A configured MCP tools server.
pub struct Server<'a, S> {
    registry: &'a Registry,
    store: S,
    tools: BTreeMap<String, ToolTarget>,
}

#[derive(Clone)]
enum ToolTarget {
    Create(String),
    Get(String),
    List(String),
    Events(String),
    Execute { entity: String, operation: String },
}

impl<'a, S> Server<'a, S>
where
    S: Store,
{
    /// Builds a server and validates that every generated tool name is portable across MCP hosts.
    ///
    /// # Errors
    ///
    /// An entity or operation name is not MCP-safe, exceeds the length limit, or collides with a
    /// built-in tool.
    pub fn new(registry: &'a Registry, store: S) -> Result<Self, String> {
        let mut tools = BTreeMap::new();
        for definition in registry.iter() {
            validate_component("entity", &definition.entity)?;
            for reserved in RESERVED {
                tools
                    .entry(format!("{}.{}", definition.entity, reserved))
                    .or_insert_with(|| match reserved {
                        "create" => ToolTarget::Create(definition.entity.clone()),
                        "get" => ToolTarget::Get(definition.entity.clone()),
                        "list" => ToolTarget::List(definition.entity.clone()),
                        "events" => ToolTarget::Events(definition.entity.clone()),
                        _ => unreachable!(),
                    });
            }
            for operation in definition.operations.keys() {
                validate_component("operation", operation)?;
                if RESERVED.contains(&operation.as_str()) {
                    return Err(format!(
                        "operation {operation:?} on {} collides with the built-in MCP tool of that name",
                        definition.entity
                    ));
                }
                let name = format!("{}.{}", definition.entity, operation);
                if name.len() > 128 {
                    return Err(format!("MCP tool name {name:?} exceeds 128 characters"));
                }
                tools.entry(name).or_insert_with(|| ToolTarget::Execute {
                    entity: definition.entity.clone(),
                    operation: operation.clone(),
                });
            }
        }
        Ok(Self {
            registry,
            store,
            tools,
        })
    }

    /// Processes newline-delimited JSON-RPC until EOF.
    ///
    /// # Errors
    ///
    /// Reading or writing the supplied transport failed.
    pub fn serve(
        &mut self,
        input: &mut impl BufRead,
        output: &mut impl Write,
    ) -> Result<(), std::io::Error> {
        let mut line = String::new();
        loop {
            line.clear();
            if input.read_line(&mut line)? == 0 {
                return Ok(());
            }
            let trimmed = line.trim_end_matches(['\r', '\n']);
            if trimmed.is_empty() {
                continue;
            }
            let response = match serde_json::from_str(trimmed) {
                Ok(request) => self.handle(request),
                Err(error) => Some(error_response(
                    Value::Null,
                    -32700,
                    format!("Parse error: {error}"),
                )),
            };
            if let Some(response) = response {
                serde_json::to_writer(&mut *output, &response)?;
                output.write_all(b"\n")?;
                output.flush()?;
            }
        }
    }

    /// Handles one decoded JSON-RPC message. Notifications return `None`.
    #[must_use]
    pub fn handle(&mut self, request: Value) -> Option<Value> {
        let object = match request.as_object() {
            Some(object) => object,
            None => return Some(error_response(Value::Null, -32600, "Invalid Request")),
        };
        let id = object.get("id").cloned();
        let method = match object.get("method").and_then(Value::as_str) {
            Some(method) => method,
            None => {
                return Some(error_response(
                    id.unwrap_or(Value::Null),
                    -32600,
                    "Invalid Request: method is required",
                ))
            }
        };
        let id = id?;
        let params = object.get("params").cloned().unwrap_or_else(|| json!({}));
        let result = match method {
            "server/discover" => Ok(self.discovery()),
            "initialize" => Ok(self.initialize(&params)),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(self.list_tools()),
            "tools/call" => self.call_tool(&params),
            _ => Err((-32601, format!("Method not found: {method}"))),
        };
        Some(match result {
            Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
            Err((code, message)) => error_response(id, code, message),
        })
    }

    fn discovery(&self) -> Value {
        json!({
            "protocolVersions": [MODERN, LEGACY],
            "capabilities": { "tools": { "listChanged": false } },
            "serverInfo": { "name": "entity-runtime", "version": env!("CARGO_PKG_VERSION") }
        })
    }

    fn initialize(&self, _params: &Value) -> Value {
        // `initialize` belongs to the initialization-era protocol. A caller sending the modern
        // discovery version on this legacy method is downgraded explicitly; an unknown version
        // must never be reflected back as though the server implemented it.
        json!({
            "protocolVersion": LEGACY,
            "capabilities": { "tools": { "listChanged": false } },
            "serverInfo": { "name": "entity-runtime", "version": env!("CARGO_PKG_VERSION") }
        })
    }

    fn list_tools(&self) -> Value {
        let listed: Vec<Value> = self
            .tools
            .iter()
            .map(|(name, target)| self.tool(name, target))
            .collect();
        json!({ "tools": listed, "ttlMs": 300_000 })
    }

    fn tool(&self, name: &str, target: &ToolTarget) -> Value {
        let (description, input, read_only, idempotent) = match target {
            ToolTarget::Create(entity) => (
                format!("Create and durably record one {entity}."),
                create_schema(self.definitions(entity)),
                false,
                false,
            ),
            ToolTarget::Get(entity) => (
                format!("Read one stored {entity} by id."),
                id_schema(false),
                true,
                true,
            ),
            ToolTarget::List(entity) => (
                format!("List every stored {entity} identity in sorted order."),
                json!({ "type": "object", "additionalProperties": false }),
                true,
                true,
            ),
            ToolTarget::Events(entity) => (
                format!("Read every recorded event for one {entity}."),
                id_schema(false),
                true,
                true,
            ),
            ToolTarget::Execute { entity, operation } => (
                format!("Execute {operation} on one stored {entity} at an observed revision."),
                execute_schema(self.definitions(entity), operation),
                false,
                false,
            ),
        };
        json!({
            "name": name,
            "title": name,
            "description": description,
            "inputSchema": input,
            "outputSchema": { "type": "object" },
            "annotations": {
                "readOnlyHint": read_only,
                "destructiveHint": false,
                "idempotentHint": idempotent,
                "openWorldHint": false
            }
        })
    }

    fn call_tool(&mut self, params: &Value) -> Result<Value, (i64, String)> {
        let name = params
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| (-32602, "tools/call requires a string name".into()))?;
        let arguments = params
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| json!({}));
        let target = self
            .tools
            .get(name)
            .cloned()
            .ok_or_else(|| (-32602, format!("Unknown tool: {name}")))?;
        let result = self.invoke(target, arguments);
        Ok(match result {
            Ok(value) => tool_result(value, false),
            Err(error) => tool_result(
                json!({
                    "refused": true,
                    "by": error.boundary(),
                    "kind": error.kind(),
                    "detail": error.to_string()
                }),
                true,
            ),
        })
    }

    fn invoke(&mut self, target: ToolTarget, arguments: Value) -> Result<Value, ShellError> {
        let object = arguments
            .as_object()
            .ok_or_else(|| ShellError::Recording("tool arguments must be an object".into()))?;
        let mut runtime = StoredRuntime::new(self.registry, &mut self.store);
        match target {
            ToolTarget::Create(entity) => {
                let version = choose_version(self.registry, &entity, object)?;
                let id = string(object, "id")?;
                let fields = object.get("fields").cloned().unwrap_or_else(|| json!({}));
                let recording = recording(object)?;
                serde_json::to_value(runtime.create(&entity, version, id, fields, &recording)?)
                    .map_err(|error| ShellError::Recording(error.to_string()))
            }
            ToolTarget::Get(entity) => {
                serde_json::to_value(runtime.get(&entity, string(object, "id")?)?)
                    .map_err(|error| ShellError::Recording(error.to_string()))
            }
            ToolTarget::List(entity) => serde_json::to_value(runtime.list(&entity)?)
                .map(|items| json!({ "items": items }))
                .map_err(|error| ShellError::Recording(error.to_string())),
            ToolTarget::Events(entity) => {
                serde_json::to_value(runtime.events(&entity, string(object, "id")?)?)
                    .map(|items| json!({ "items": items }))
                    .map_err(|error| ShellError::Recording(error.to_string()))
            }
            ToolTarget::Execute { entity, operation } => {
                let id = string(object, "id")?;
                let expected = object
                    .get("expected_revision")
                    .and_then(Value::as_u64)
                    .ok_or_else(|| {
                        ShellError::Recording("expected_revision must be a positive integer".into())
                    })?;
                let arguments = object
                    .get("arguments")
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                let recording = recording(object)?;
                serde_json::to_value(
                    runtime.execute(&entity, id, expected, &operation, arguments, &recording)?,
                )
                .map_err(|error| ShellError::Recording(error.to_string()))
            }
        }
    }

    fn definitions(&self, entity: &str) -> Vec<&EntityDefinition> {
        self.registry
            .versions(entity)
            .map(|definition| definition.as_definition())
            .collect()
    }
}

fn validate_component(kind: &str, value: &str) -> Result<(), String> {
    if value.is_empty()
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err(format!(
            "{kind} name {value:?} cannot become an MCP tool component; use ASCII letters, digits, underscore or hyphen"
        ));
    }
    Ok(())
}

fn choose_version(
    registry: &Registry,
    entity: &str,
    object: &Map<String, Value>,
) -> Result<u32, ShellError> {
    let versions: Vec<u32> = registry
        .versions(entity)
        .map(|definition| definition.version)
        .collect();
    if versions.len() == 1 && !object.contains_key("version") {
        return Ok(versions[0]);
    }
    object
        .get("version")
        .and_then(Value::as_u64)
        .and_then(|version| u32::try_from(version).ok())
        .filter(|version| versions.contains(version))
        .ok_or_else(|| ShellError::Recording(format!("version must name one of {versions:?}")))
}

fn string<'a>(object: &'a Map<String, Value>, key: &str) -> Result<&'a str, ShellError> {
    object
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| ShellError::Recording(format!("{key} must be a non-empty string")))
}

fn recording(object: &Map<String, Value>) -> Result<Recording, ShellError> {
    let value = object
        .get("recording")
        .and_then(Value::as_object)
        .ok_or_else(|| ShellError::Recording("recording must be an object".into()))?;
    let optional = |key: &str| -> Result<Option<String>, ShellError> {
        match value.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(text)) if !text.trim().is_empty() => Ok(Some(text.clone())),
            _ => Err(ShellError::Recording(format!(
                "recording.{key} must be a non-empty string when present"
            ))),
        }
    };
    if !value.contains_key("actor") {
        return Err(ShellError::Recording(
            "recording.actor must be present as a string or null".into(),
        ));
    }
    Ok(Recording {
        record_id: string(value, "record_id")?.to_owned(),
        recorded_at: string(value, "recorded_at")?.to_owned(),
        correlation: optional("correlation")?,
        causation: optional("causation")?,
        actor: optional("actor")?,
    })
}

fn create_schema(definitions: Vec<&EntityDefinition>) -> Value {
    let field_schemas: Vec<Value> = definitions
        .iter()
        .map(|definition| entity_surface::object_schema(&definition.schema))
        .collect();
    let fields = if field_schemas.len() == 1 {
        field_schemas
            .into_iter()
            .next()
            .unwrap_or_else(|| json!({}))
    } else {
        json!({ "anyOf": field_schemas })
    };
    let versions: Vec<u32> = definitions
        .iter()
        .map(|definition| definition.version)
        .collect();
    let mut required = vec![json!("id"), json!("fields"), json!("recording")];
    if versions.len() > 1 {
        required.push(json!("version"));
    }
    json!({
        "type": "object", "additionalProperties": false, "required": required,
        "allOf": definitions.iter().map(|definition| json!({
            "if": { "properties": { "version": { "const": definition.version } }, "required": ["version"] },
            "then": { "properties": { "fields": entity_surface::object_schema(&definition.schema) } }
        })).collect::<Vec<_>>(),
        "properties": {
            "id": { "type": "string", "minLength": 1 },
            "version": { "type": "integer", "enum": versions },
            "fields": fields,
            "recording": recording_schema()
        }
    })
}

fn execute_schema(definitions: Vec<&EntityDefinition>, operation: &str) -> Value {
    let schemas: Vec<Value> = definitions
        .iter()
        .filter_map(|definition| definition.operations.get(operation))
        .map(|operation| entity_surface::object_schema(&operation.arguments))
        .collect();
    let arguments = if schemas.len() == 1 {
        schemas.into_iter().next().unwrap_or_else(|| json!({}))
    } else {
        json!({ "anyOf": schemas })
    };
    json!({
        "type": "object", "additionalProperties": false,
        "required": ["id", "expected_revision", "arguments", "recording"],
        "properties": {
            "id": { "type": "string", "minLength": 1 },
            "expected_revision": { "type": "integer", "minimum": 1 },
            "arguments": arguments,
            "recording": recording_schema()
        }
    })
}

fn id_schema(_include_version: bool) -> Value {
    json!({
        "type": "object", "additionalProperties": false, "required": ["id"],
        "properties": { "id": { "type": "string", "minLength": 1 } }
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

fn tool_result(value: Value, is_error: bool) -> Value {
    let text =
        serde_json::to_string_pretty(&value).unwrap_or_else(|_| "serialization failed".into());
    json!({
        "content": [{ "type": "text", "text": text }],
        "structuredContent": value,
        "isError": is_error
    })
}

fn error_response(id: Value, code: i64, message: impl Into<String>) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message.into() } })
}

#[cfg(test)]
mod tests {
    use super::*;
    use entity_core::EntityDefinition;
    use entity_store::MemoryStore;

    fn registry() -> Registry {
        let definition: EntityDefinition = serde_json::from_value(json!({
            "entity": "refund",
            "schema": { "fields": { "amount": { "type": "integer", "required": true } } },
            "lifecycle": { "initial": "draft", "states": ["draft", "approved"] },
            "operations": { "approve": { "transitions": [{ "from": "draft", "to": "approved" }] } }
        }))
        .expect("definition");
        let mut registry = Registry::new();
        registry.register(definition).expect("valid");
        registry
    }

    #[test]
    fn overlapping_versions_accept_valid_requests_and_creation_checks_the_selected_version() {
        let mut registry = registry();
        let mut second = registry.get("refund", 1).unwrap().as_definition().clone();
        second.version = 2;
        registry.register(second.clone()).unwrap();
        let definitions: Vec<_> = registry.iter().map(|d| d.as_definition()).collect();
        let recording =
            json!({ "record_id": "one", "recorded_at": "2026-09-05T12:00:00Z", "actor": null });
        let input =
            json!({"id": "one", "version": 1, "fields": {"amount": 100}, "recording": recording});
        let schema = create_schema(definitions.clone());
        assert!(
            jsonschema::is_valid(&schema, &input),
            "overlapping versions must admit valid fields"
        );
        assert!(
            jsonschema::is_valid(
                &execute_schema(definitions, "approve"),
                &json!({"id": "one", "expected_revision": 1, "arguments": {}, "recording": recording})
            ),
            "overlapping argument schemas must admit valid arguments"
        );
        second.schema.fields.get_mut("amount").unwrap().kind = entity_core::FieldKind::String;
        let first = registry.get("refund", 1).unwrap().as_definition();
        let schema = create_schema(vec![first, &second]);
        let mut wrong_version = input.clone();
        wrong_version["fields"]["amount"] = json!("wrong version");
        assert!(!jsonschema::is_valid(&schema, &wrong_version));
        let mut server = Server::new(&registry, MemoryStore::new()).unwrap();
        let result = server.handle(json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": "refund.create", "arguments": input}})).unwrap();
        assert_eq!(result["result"]["isError"], false);
    }

    #[test]
    fn a_single_version_still_refuses_an_explicit_unknown_version() {
        let registry = registry();
        let error = choose_version(
            &registry,
            "refund",
            json!({"version": 99}).as_object().unwrap(),
        )
        .unwrap_err();
        assert!(matches!(error, ShellError::Recording(ref detail) if detail.contains("version")));
    }

    #[test]
    fn both_protocol_eras_discover_the_same_sorted_tools() {
        let registry = registry();
        let mut server = Server::new(&registry, MemoryStore::new()).expect("server");
        let modern = server
            .handle(json!({ "jsonrpc": "2.0", "id": 1, "method": "server/discover" }))
            .expect("response");
        assert_eq!(modern["result"]["protocolVersions"][0], MODERN);
        let listed = server
            .handle(json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }))
            .expect("response");
        let names: Vec<&str> = listed["result"]["tools"]
            .as_array()
            .expect("tools")
            .iter()
            .filter_map(|tool| tool["name"].as_str())
            .collect();
        assert_eq!(
            names,
            [
                "refund.approve",
                "refund.create",
                "refund.events",
                "refund.get",
                "refund.list"
            ]
        );
    }

    #[test]
    fn a_stale_tool_call_is_actionable_and_changes_nothing() {
        let registry = registry();
        let mut server = Server::new(&registry, MemoryStore::new()).expect("server");
        let recording = json!({
            "record_id": "one", "recorded_at": "2026-08-31T10:00:00Z", "actor": "test"
        });
        let created = server
            .handle(json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {
                "name": "refund.create", "arguments": { "id": "one", "fields": { "amount": 10 }, "recording": recording }
            }}))
            .expect("response");
        assert_eq!(created["result"]["isError"], false);
        let stale = server
            .handle(json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {
                "name": "refund.approve", "arguments": {
                    "id": "one", "expected_revision": 9, "arguments": {},
                    "recording": { "record_id": "two", "recorded_at": "2026-08-31T10:01:00Z", "actor": "test" }
                }
            }}))
            .expect("response");
        assert_eq!(stale["result"]["isError"], true);
        assert_eq!(
            stale["result"]["structuredContent"]["kind"],
            "revision_conflict"
        );
    }
}
