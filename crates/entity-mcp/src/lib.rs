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

/// The default for the largest request frame [`Server::serve`] reads, in bytes, newline excluded.
///
/// A peer on stdio is not trusted to bound its own lines: without a limit one unterminated line
/// is buffered whole before parsing, and the buffer keeps that allocation afterwards.
pub const DEFAULT_FRAME_LIMIT: usize = 4 * 1024 * 1024;

/// The frame buffer capacity kept between requests; a larger one is released after its frame.
const RETAINED_FRAME_CAPACITY: usize = 64 * 1024;

/// A configured MCP tools server.
pub struct Server<'a, S> {
    registry: &'a Registry,
    store: S,
    tools: BTreeMap<String, ToolTarget>,
    /// The `tools/list` result, built once: the registry and the tool set are fixed for the
    /// server's life, and rebuilding every description and JSON schema per request was a quarter
    /// of a short session's instructions.
    catalogue: Value,
    frame_limit: usize,
    /// How many tool descriptors this server has built, so a test can hold it to one per tool.
    #[cfg(test)]
    descriptor_builds: std::cell::Cell<usize>,
}

/// What one read of a newline-delimited frame found.
#[derive(Debug, PartialEq, Eq)]
enum Frame {
    /// The input ended before any byte of a new frame.
    End,
    /// A complete frame, now in the buffer without its newline.
    Line,
    /// A frame longer than the limit. Its bytes were read and dropped, never buffered.
    Oversized,
}

/// Reads one frame into `frame`, holding at most `limit` bytes of it at any time.
///
/// An oversized frame is consumed up to its newline (or the end of input) so the next frame starts
/// cleanly, and its partial contents are released rather than kept as the buffer's capacity.
fn read_frame(
    input: &mut impl BufRead,
    frame: &mut Vec<u8>,
    limit: usize,
) -> Result<Frame, std::io::Error> {
    frame.clear();
    let mut started = false;
    let mut oversized = false;
    loop {
        let available = match input.fill_buf() {
            Ok(available) => available,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        };
        if available.is_empty() {
            return Ok(match (started, oversized) {
                (false, _) => Frame::End,
                (true, false) => Frame::Line,
                (true, true) => Frame::Oversized,
            });
        }
        started = true;
        let newline = available.iter().position(|byte| *byte == b'\n');
        let chunk = &available[..newline.unwrap_or(available.len())];
        if !oversized {
            if frame.len() + chunk.len() > limit {
                oversized = true;
                *frame = Vec::new();
            } else {
                frame.extend_from_slice(chunk);
            }
        }
        let used = newline.map_or(available.len(), |at| at + 1);
        input.consume(used);
        if newline.is_some() {
            return Ok(if oversized {
                Frame::Oversized
            } else {
                Frame::Line
            });
        }
    }
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
                let name = format!("{}.{}", definition.entity, reserved);
                validate_tool_name(&name)?;
                tools.entry(name).or_insert_with(|| match reserved {
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
                validate_tool_name(&name)?;
                tools.entry(name).or_insert_with(|| ToolTarget::Execute {
                    entity: definition.entity.clone(),
                    operation: operation.clone(),
                });
            }
        }
        let mut server = Self {
            registry,
            store,
            tools,
            catalogue: Value::Null,
            frame_limit: DEFAULT_FRAME_LIMIT,
            #[cfg(test)]
            descriptor_builds: std::cell::Cell::new(0),
        };
        let listed: Vec<Value> = server
            .tools
            .iter()
            .map(|(name, target)| server.tool(name, target))
            .collect();
        server.catalogue = json!({ "tools": listed, "ttlMs": 300_000 });
        Ok(server)
    }

    /// Sets the largest request frame [`Self::serve`] reads, in bytes, newline excluded.
    ///
    /// The default is [`DEFAULT_FRAME_LIMIT`]. A longer frame is discarded unparsed and answered
    /// with a JSON-RPC `-32600` error; the server goes on with the next frame.
    #[must_use]
    pub const fn with_frame_limit(mut self, bytes: usize) -> Self {
        self.frame_limit = bytes;
        self
    }

    /// Processes newline-delimited JSON-RPC until EOF.
    ///
    /// A frame longer than the frame limit is read and dropped without being buffered, and
    /// answered with a JSON-RPC `-32600` error whose `id` is `null`, since the request was never
    /// parsed.
    ///
    /// # Errors
    ///
    /// Reading or writing the supplied transport failed.
    pub fn serve(
        &mut self,
        input: &mut impl BufRead,
        output: &mut impl Write,
    ) -> Result<(), std::io::Error> {
        let mut frame = Vec::new();
        loop {
            if frame.capacity() > RETAINED_FRAME_CAPACITY {
                // One large frame is not kept as the buffer's capacity for the rest of the session.
                frame = Vec::new();
            }
            let response = match read_frame(input, &mut frame, self.frame_limit)? {
                Frame::End => return Ok(()),
                Frame::Oversized => Some(error_response(
                    Value::Null,
                    -32600,
                    format!(
                        "Invalid Request: frame exceeds {} bytes and was discarded",
                        self.frame_limit
                    ),
                )),
                Frame::Line => {
                    let mut end = frame.len();
                    while end > 0 && frame[end - 1] == b'\r' {
                        end -= 1;
                    }
                    let trimmed = &frame[..end];
                    if trimmed.is_empty() {
                        continue;
                    }
                    match serde_json::from_slice(trimmed) {
                        Ok(request) => self.handle(request),
                        Err(error) => Some(error_response(
                            Value::Null,
                            -32700,
                            format!("Parse error: {error}"),
                        )),
                    }
                }
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
        self.catalogue.clone()
    }

    /// Builds one tool descriptor. Called once per tool, from [`Self::new`].
    fn tool(&self, name: &str, target: &ToolTarget) -> Value {
        #[cfg(test)]
        self.descriptor_builds.set(self.descriptor_builds.get() + 1);
        let destructive = match target {
            ToolTarget::Execute { entity, operation } => {
                writes_fields(&self.definitions(entity), operation)
            }
            _ => false,
        };
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
                "destructiveHint": destructive,
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

/// Whether any registered version of `operation` can overwrite or remove a live field value: any
/// assignment in its `set` or in any outcome's `set` or `set_if_present` (a template, an
/// increment or a clear), or any fulfillment target, whose actions admit `Set` and, when
/// optional, `Remove`. Each of these can replace a value the field already holds; the definition
/// cannot say the field is absent beforehand, so none of them is additive. An operation that
/// writes no field — a transition, events, a response — stays non-destructive. Every outcome
/// counts, refusing ones included, because the annotation describes what a call may do before the
/// branch is known. MCP defines `destructiveHint: false` as "only additive updates", and a client
/// may use it to skip a confirmation, so an overwrite must not be advertised as additive.
fn writes_fields(definitions: &[&EntityDefinition], operation: &str) -> bool {
    definitions
        .iter()
        .filter_map(|definition| definition.operations.get(operation))
        .any(|declared| {
            !declared.set.is_empty()
                || declared.outcomes.iter().any(|outcome| {
                    !outcome.set.is_empty()
                        || !outcome.set_if_present.is_empty()
                        || !outcome.fulfills.is_empty()
                })
        })
}

/// The longest tool name MCP hosts are known to accept.
const MAX_TOOL_NAME: usize = 128;

/// Refuses an assembled tool name a host could not register. Applied to every name the server
/// lists, the built-in `create`/`get`/`list`/`events` tools included.
fn validate_tool_name(name: &str) -> Result<(), String> {
    if name.len() > MAX_TOOL_NAME {
        return Err(format!(
            "MCP tool name {name:?} exceeds {MAX_TOOL_NAME} characters"
        ));
    }
    Ok(())
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
    fn a_built_in_tool_name_over_the_portability_limit_is_refused() {
        let server_for = |entity: String| {
            let definition: EntityDefinition = serde_json::from_value(json!({
                "entity": entity,
                "schema": {},
                "lifecycle": { "initial": "new", "states": ["new"] },
                "operations": {}
            }))
            .expect("definition");
            let mut registry = Registry::new();
            registry.register(definition).expect("valid");
            Server::new(&registry, MemoryStore::new()).map(|server| server.tools.len())
        };
        // `<entity>.create` and `<entity>.events` are the longest built-in names: seven characters
        // past the entity. The entity has no operations, so only built-in names can be refused.
        assert_eq!(server_for("e".repeat(121)), Ok(4), "128 characters fit");
        let refused = server_for("e".repeat(122)).expect_err("129 characters do not");
        assert!(
            refused.contains(&format!("\"{}.create\"", "e".repeat(122)))
                && refused.contains("exceeds 128 characters"),
            "the refusal names the built-in tool and the limit: {refused}"
        );
    }

    #[test]
    fn repeated_tool_listings_build_each_descriptor_once_per_session() {
        let registry = registry();
        let mut server = Server::new(&registry, MemoryStore::new()).expect("server");
        let tools = server.tools.len();
        let first = server
            .handle(json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" }))
            .expect("response");
        for id in 2..=4 {
            let again = server
                .handle(json!({ "jsonrpc": "2.0", "id": id, "method": "tools/list" }))
                .expect("response");
            assert_eq!(
                again["result"], first["result"],
                "the catalogue is immutable"
            );
        }
        assert_eq!(
            first["result"]["tools"].as_array().map(Vec::len),
            Some(tools)
        );
        assert_eq!(
            server.descriptor_builds.get(),
            tools,
            "four listings must build {tools} descriptors, one per tool, not one per request"
        );
    }

    #[test]
    fn an_operation_that_can_remove_a_field_is_advertised_as_destructive() {
        let snoozer: EntityDefinition = serde_json::from_value(json!({
            "entity": "snoozer",
            "schema": { "fields": { "snoozed_until": { "type": "string" } } },
            "lifecycle": { "initial": "active", "states": ["active"] },
            "operations": {
                "wake": {
                    "transitions": [{ "from": "active", "to": "active" }],
                    "set": { "snoozed_until": { "cleared": true } }
                },
                "touch": { "transitions": [{ "from": "active", "to": "active" }] }
            }
        }))
        .expect("definition");
        let invoice: EntityDefinition = serde_json::from_value(json!({
            "entity": "invoice", "version": 1, "semantics": "service/3",
            "schema": { "fields": {
                "issued_at": { "type": "string", "required": true },
                "note": { "type": "string" },
                "draft": { "type": "string" }
            }},
            "lifecycle": { "initial": "Draft", "states": ["Draft"] },
            "operations": {
                "Discard": { "outcomes": [{
                    "name": "discarded", "effect": "updates",
                    "set": { "draft": { "cleared": true } }
                }]},
                "Annotate": { "outcomes": [{
                    "name": "annotated", "effect": "updates",
                    "fulfills": { "note": { "actions": "optional" } }
                }]},
                "Issue": { "outcomes": [{
                    "name": "issued", "effect": "updates",
                    "fulfills": { "issued_at": { "actions": "required" } }
                }]}
            }
        }))
        .expect("definition");
        let mut registry = registry();
        registry.register(snoozer).expect("valid");
        registry.register(invoice).expect("valid");
        let mut server = Server::new(&registry, MemoryStore::new()).expect("server");
        let listed = server
            .handle(json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" }))
            .expect("response");
        let hints: BTreeMap<&str, &Value> = listed["result"]["tools"]
            .as_array()
            .expect("tools")
            .iter()
            .map(|tool| {
                (
                    tool["name"].as_str().expect("name"),
                    &tool["annotations"]["destructiveHint"],
                )
            })
            .collect();
        for (tool, destructive) in [
            ("snoozer.wake", true),
            ("invoice.Discard", true),
            ("invoice.Annotate", true),
            ("snoozer.touch", false),
            // A required fulfillment cannot remove its field, but its `Set` overwrites it:
            // `an_operation_that_overwrites_a_field_is_advertised_as_destructive`.
            ("invoice.Issue", true),
            ("refund.approve", false),
            ("invoice.create", false),
        ] {
            assert_eq!(
                hints[tool],
                &json!(destructive),
                "{tool} destructiveHint must be {destructive}"
            );
        }
    }

    #[test]
    fn an_operation_that_overwrites_a_field_is_advertised_as_destructive() {
        let counter: EntityDefinition = serde_json::from_value(json!({
            "entity": "counter",
            "schema": { "fields": {
                "name": { "type": "string" },
                "count": { "type": "integer", "required": true, "default": 0 }
            }},
            "lifecycle": { "initial": "open", "states": ["open", "closed"] },
            "operations": {
                "rename": {
                    "arguments": { "fields": { "name": { "type": "string", "required": true } } },
                    "transitions": [{ "from": "open", "to": "open" }],
                    "set": { "name": "$args.name" }
                },
                "close": {
                    "transitions": [{ "from": "open", "to": "closed" }],
                    "emits": [{ "type": "Closed", "payload": { "count": "$fields.count" } }]
                }
            }
        }))
        .expect("definition");
        let ticket: EntityDefinition = serde_json::from_value(json!({
            "entity": "ticket", "version": 1, "semantics": "service/3",
            "schema": { "fields": {
                "count": { "type": "integer", "required": true, "default": 0 },
                "label": { "type": "string" },
                "issued_at": { "type": "string", "required": true }
            }},
            "lifecycle": { "initial": "open", "states": ["open"] },
            "operations": {
                "Bump": {
                    "arguments": { "fields": { "accept": { "type": "boolean", "required": true } } },
                    "outcomes": [
                        { "name": "refused", "when": { "eq": ["$args.accept", false] },
                          "refuses": { "error": "NotAccepted" } },
                        { "name": "bumped", "effect": "updates",
                          "set": { "count": { "increment": 1 } } }
                    ]
                },
                "Label": {
                    "arguments": { "fields": { "label": { "type": "string" } } },
                    "outcomes": [{ "name": "labelled", "effect": "updates",
                        "set_if_present": { "label": { "argument": "label" } } }]
                },
                "Issue": { "outcomes": [{
                    "name": "issued", "effect": "updates",
                    "fulfills": { "issued_at": { "actions": "required" } }
                }]},
                "Ping": { "outcomes": [{ "name": "pinged", "effect": "none",
                    "emits": [{ "type": "Pinged", "payload": { "count": "$fields.count" } }] }] },
                "Later": { "outcomes": [{ "name": "noted", "effect": "updates" }] }
            }
        }))
        .expect("definition");
        // A later version that starts writing a field makes the one tool for both versions
        // destructive.
        let mut later = ticket.clone();
        later.version = 2;
        later.operations.get_mut("Later").expect("Later").outcomes[0]
            .set
            .insert("label".to_owned(), json!("noted"));
        let mut registry = registry();
        registry.register(counter).expect("valid");
        registry.register(ticket).expect("valid");
        registry.register(later).expect("valid");
        let mut server = Server::new(&registry, MemoryStore::new()).expect("server");
        let listed = server
            .handle(json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" }))
            .expect("response");
        let hints: BTreeMap<&str, &Value> = listed["result"]["tools"]
            .as_array()
            .expect("tools")
            .iter()
            .map(|tool| {
                (
                    tool["name"].as_str().expect("name"),
                    &tool["annotations"]["destructiveHint"],
                )
            })
            .collect();
        for (tool, destructive) in [
            ("counter.rename", true),
            ("ticket.Bump", true),
            ("ticket.Label", true),
            ("ticket.Issue", true),
            ("ticket.Later", true),
            ("counter.close", false),
            ("ticket.Ping", false),
            ("refund.approve", false),
        ] {
            assert_eq!(
                hints[tool],
                &json!(destructive),
                "{tool} destructiveHint must be {destructive}"
            );
        }
    }

    #[test]
    fn an_oversized_frame_is_answered_with_an_error_and_the_next_frame_is_served() {
        let registry = registry();
        let mut server = Server::new(&registry, MemoryStore::new())
            .expect("server")
            .with_frame_limit(64);
        let oversized = format!(
            "{{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\",\"pad\":\"{}\"}}",
            "x".repeat(4096)
        );
        let input =
            format!("{oversized}\r\n{{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"ping\"}}\n");
        // A small read buffer makes the frame arrive in many chunks, as a pipe delivers it.
        let mut reader = std::io::BufReader::with_capacity(7, input.as_bytes());
        let mut output = Vec::new();
        server.serve(&mut reader, &mut output).expect("serves");
        let responses: Vec<Value> = String::from_utf8(output)
            .expect("utf-8")
            .lines()
            .map(|line| serde_json::from_str(line).expect("json"))
            .collect();
        assert_eq!(responses.len(), 2, "one answer per frame: {responses:?}");
        assert_eq!(responses[0]["id"], Value::Null);
        assert_eq!(responses[0]["error"]["code"], -32600);
        assert!(
            responses[0]["error"]["message"]
                .as_str()
                .is_some_and(|message| message.contains("frame exceeds 64 bytes")),
            "the error names the limit: {}",
            responses[0]
        );
        assert_eq!(responses[1]["id"], 2);
        assert_eq!(responses[1]["result"], json!({}));
    }

    #[test]
    fn a_frame_that_is_not_utf_8_is_a_parse_error_and_the_session_goes_on() {
        let registry = registry();
        let mut server = Server::new(&registry, MemoryStore::new()).expect("server");
        let mut input = b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"p\xffng\"}\n".to_vec();
        input.extend_from_slice(b"{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"ping\"}\n");
        let mut output = Vec::new();
        server
            .serve(&mut input.as_slice(), &mut output)
            .expect("serves");
        let responses: Vec<Value> = String::from_utf8(output)
            .expect("utf-8")
            .lines()
            .map(|line| serde_json::from_str(line).expect("json"))
            .collect();
        assert_eq!(responses.len(), 2, "one answer per frame: {responses:?}");
        assert_eq!(responses[0]["error"]["code"], -32700);
        assert_eq!(responses[1]["id"], 2);
    }

    #[test]
    fn an_oversized_frame_is_never_buffered_beyond_the_limit() {
        let input = format!("{}\nnext\n", "y".repeat(1 << 20));
        let mut reader = std::io::BufReader::with_capacity(4096, input.as_bytes());
        let mut frame = Vec::new();
        let read = read_frame(&mut reader, &mut frame, 1024).expect("reads");
        assert_eq!(read, Frame::Oversized);
        assert!(
            frame.capacity() <= 1024,
            "a megabyte line held {} bytes of buffer under a 1024-byte limit",
            frame.capacity()
        );
        assert_eq!(
            read_frame(&mut reader, &mut frame, 1024).expect("reads"),
            Frame::Line
        );
        assert_eq!(frame, b"next");
        assert_eq!(
            read_frame(&mut reader, &mut frame, 1024).expect("reads"),
            Frame::End
        );
    }

    #[test]
    fn an_unterminated_oversized_frame_at_the_end_of_input_is_still_answered() {
        let input = "z".repeat(200);
        let mut reader = std::io::BufReader::with_capacity(16, input.as_bytes());
        let mut frame = Vec::new();
        assert_eq!(
            read_frame(&mut reader, &mut frame, 100).expect("reads"),
            Frame::Oversized
        );
        assert_eq!(
            read_frame(&mut reader, &mut frame, 100).expect("reads"),
            Frame::End
        );
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
