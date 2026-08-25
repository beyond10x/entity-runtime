//! `entity` — the reference shell around `entity-core`.
//!
//! Everything the kernel refuses to do happens here and only here: files are read, standard input
//! is consumed, output is printed and an exit code is chosen. Identifiers come from the caller.
//! Nothing reads a clock.
//!
//! Exit codes: `0` the kernel produced a result · `1` the kernel refused (the typed refusal is
//! printed) · `2` the invocation itself was wrong — a missing file, unparsable YAML or JSON.

use clap::{Args, Parser, Subcommand, ValueEnum};
use entity_core::{
    CoreError, Decision, EntityDefinition, EntityInstance, Registry, Runtime, ValidationError,
};
use serde_json::{json, Value};
use std::{
    fmt::Write as _,
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::ExitCode,
};

const ABOUT: &str =
    "Schema-driven entity runtime: validate definitions, create instances, execute operations.";
const LONG_ABOUT: &str = "\
Schema-driven entity runtime: validate definitions, create instances, execute operations.

An entity type is a YAML document — schema, lifecycle, operations, preconditions, invariants,
events. The kernel decides `definition + instance + operation + arguments -> Decision`; this
command is the shell that reads the files and prints the decision.

Values passed with --fields, --instance and --arguments are read three ways:
  inline JSON        --fields '{\"title\": \"Login fails\"}'
  @<path>            --instance @ticket.json      (JSON or YAML)
  -                  --instance -                 (standard input; JSON or YAML)
A Decision printed by `create` or `execute` can be fed straight back as an --instance.

Exit codes: 0 decided · 1 refused by the kernel · 2 invalid invocation.";

#[derive(Parser)]
#[command(name = "entity", version, about = ABOUT, long_about = LONG_ABOUT)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Parse and register definitions; exit 1 if any is invalid, naming every defect found.
    Validate {
        /// Definition files, YAML.
        #[arg(required = true)]
        definitions: Vec<PathBuf>,
    },
    /// Show what a definition declares: fields, states, rules, operations.
    Inspect {
        /// The definition file, YAML.
        definition: PathBuf,
        #[arg(long, value_enum, default_value_t = Format::Text)]
        format: Format,
    },
    /// Draw the lifecycle: states as nodes, operations as the edges between them.
    Graph {
        /// The definition file, YAML.
        definition: PathBuf,
        #[arg(long, value_enum, default_value_t = GraphFormat::Text)]
        format: GraphFormat,
    },
    /// Create an instance: definition + id + fields -> Decision.
    Create {
        #[command(flatten)]
        definition: DefinitionArg,
        /// The new instance's identity. The kernel generates none; you supply it.
        #[arg(long)]
        id: String,
        /// The fields, as inline JSON, `@<path>` or `-` for stdin.
        #[arg(long, default_value = "{}")]
        fields: String,
        #[arg(long, value_enum, default_value_t = Format::Json)]
        format: Format,
    },
    /// Execute an operation: definition + instance + operation + arguments -> Decision.
    Execute {
        #[command(flatten)]
        definition: DefinitionArg,
        /// The current instance (or a Decision holding one), as inline JSON, `@<path>` or `-`.
        #[arg(long)]
        instance: String,
        /// The operation name, as declared in the definition.
        #[arg(long)]
        operation: String,
        /// The arguments, as inline JSON, `@<path>` or `-` for stdin.
        #[arg(long, default_value = "{}")]
        arguments: String,
        #[arg(long, value_enum, default_value_t = Format::Json)]
        format: Format,
    },
}

#[derive(Args)]
struct DefinitionArg {
    /// The definition file, YAML. Repeat to register several versions or types at once.
    #[arg(long = "definition", required = true)]
    definitions: Vec<PathBuf>,
}

#[derive(Clone, Copy, ValueEnum)]
enum Format {
    /// A short human-readable rendering.
    Text,
    /// JSON, one document.
    Json,
    /// YAML, one document.
    Yaml,
}

#[derive(Clone, Copy, ValueEnum)]
enum GraphFormat {
    /// One line per transition: `from --operation--> to`.
    Text,
    /// Graphviz DOT.
    Dot,
}

/// Why the command did not produce a result, and which exit code that earns.
enum Failure {
    /// The kernel refused. Exit 1. The refusal is printed to stdout in the requested format.
    Refused(CoreError),
    /// The invocation was wrong. Exit 2. Printed to stderr.
    Usage(String),
}

impl From<CoreError> for Failure {
    fn from(error: CoreError) -> Self {
        Self::Refused(error)
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let mut out = io::stdout().lock();
    match run(cli.command, &mut out) {
        Ok(()) => ExitCode::SUCCESS,
        Err(Failure::Refused(error)) => {
            // Machine-readable on stdout, so a pipeline can read the refusal; the sentence on
            // stderr, so a person sees it whatever the format.
            let _ = writeln!(
                out,
                "{}",
                serde_json::to_string_pretty(&refusal(&error)).expect("json")
            );
            eprintln!("refused: {error}");
            ExitCode::from(1)
        }
        Err(Failure::Usage(message)) => {
            eprintln!("error: {message}");
            ExitCode::from(2)
        }
    }
}

fn run(command: Command, out: &mut impl Write) -> Result<(), Failure> {
    match command {
        Command::Validate { definitions } => validate(&definitions, out),
        Command::Inspect { definition, format } => {
            let definition = load_definition(&definition)?;
            let mut registry = Registry::new();
            registry
                .register(definition.clone())
                .map_err(CoreError::from)?;
            match format {
                Format::Text => write_all(out, &inspect_text(&definition)),
                Format::Json => write_all(out, &to_json(&definition)?),
                Format::Yaml => write_all(out, &to_yaml(&definition)?),
            }
        }
        Command::Graph { definition, format } => {
            let definition = load_definition(&definition)?;
            let mut registry = Registry::new();
            registry
                .register(definition.clone())
                .map_err(CoreError::from)?;
            write_all(out, &graph(&definition, format))
        }
        Command::Create {
            definition,
            id,
            fields,
            format,
        } => {
            let registry = load_registry(&definition.definitions)?;
            let (entity, version) = single_type(&registry)?;
            let fields = read_value(&fields, "--fields")?;
            let decision = Runtime::new(&registry).create(&entity, version, id, fields)?;
            write_decision(out, &decision, format)
        }
        Command::Execute {
            definition,
            instance,
            operation,
            arguments,
            format,
        } => {
            let registry = load_registry(&definition.definitions)?;
            let instance = read_instance(&instance)?;
            let arguments = read_value(&arguments, "--arguments")?;
            let decision = Runtime::new(&registry).execute(&instance, &operation, arguments)?;
            write_decision(out, &decision, format)
        }
    }
}

// --- Loading ---------------------------------------------------------------------------------------

fn load_definition(path: &Path) -> Result<EntityDefinition, Failure> {
    let text = fs::read_to_string(path)
        .map_err(|error| Failure::Usage(format!("cannot read {}: {error}", path.display())))?;
    entity_yaml::from_str(&text)
        .map_err(|error| Failure::Usage(format!("{}: {error}", path.display())))
}

fn load_registry(paths: &[PathBuf]) -> Result<Registry, Failure> {
    let mut registry = Registry::new();
    for path in paths {
        let definition = load_definition(path)?;
        registry.register(definition).map_err(CoreError::from)?;
    }
    Ok(registry)
}

/// `create` needs to know which type to create. With one definition file that is unambiguous;
/// with several it is not, and the command says so rather than guessing.
fn single_type(registry: &Registry) -> Result<(String, u32), Failure> {
    let mut types = registry
        .iter()
        .map(|definition| (definition.entity.clone(), definition.version));
    match (types.next(), types.next()) {
        (Some(only), None) => Ok(only),
        _ => Err(Failure::Usage(
            "create takes exactly one --definition, so the type to create is unambiguous".into(),
        )),
    }
}

/// Inline JSON, `@path`, or `-` for stdin. Files and stdin may be YAML or JSON.
fn read_value(source: &str, flag: &str) -> Result<Value, Failure> {
    let text = if source == "-" {
        let mut text = String::new();
        io::stdin()
            .read_to_string(&mut text)
            .map_err(|error| Failure::Usage(format!("cannot read stdin for {flag}: {error}")))?;
        text
    } else if let Some(path) = source.strip_prefix('@') {
        fs::read_to_string(path)
            .map_err(|error| Failure::Usage(format!("cannot read {path} for {flag}: {error}")))?
    } else {
        source.to_owned()
    };
    serde_yaml::from_str(&text)
        .map_err(|error| Failure::Usage(format!("{flag} is not valid JSON or YAML: {error}")))
}

/// An `EntityInstance`, or a `Decision` from an earlier `create`/`execute`, whose instance is taken.
fn read_instance(source: &str) -> Result<EntityInstance, Failure> {
    let mut value = read_value(source, "--instance")?;
    if let Some(inner) = value.get("instance").cloned() {
        if value.get("events").is_some() {
            value = inner;
        }
    }
    serde_json::from_value(value)
        .map_err(|error| Failure::Usage(format!("--instance is not an entity instance: {error}")))
}

// --- Rendering -------------------------------------------------------------------------------------

fn validate(paths: &[PathBuf], out: &mut impl Write) -> Result<(), Failure> {
    let mut invalid = 0usize;
    let mut first_refusal = None;
    for path in paths {
        let outcome = load_definition(path).and_then(|definition| {
            let mut registry = Registry::new();
            let (entity, version) = (definition.entity.clone(), definition.version);
            registry
                .register(definition)
                .map(|()| (entity, version))
                .map_err(|error| Failure::Refused(error.into()))
        });
        match outcome {
            Ok((entity, version)) => {
                writeln!(out, "{}: valid ({entity} v{version})", path.display())
                    .map_err(io_failure)?;
            }
            Err(Failure::Refused(error)) => {
                invalid += 1;
                // The definition error itself, without the "definition error:" prefix `CoreError`
                // adds — the line already says which file is invalid.
                let reason = match &error {
                    CoreError::Definition(inner) => inner.to_string(),
                    other => other.to_string(),
                };
                writeln!(out, "{}: invalid: {reason}", path.display()).map_err(io_failure)?;
                first_refusal.get_or_insert(error);
            }
            Err(usage) => return Err(usage),
        }
    }
    writeln!(out, "{} file(s), {invalid} invalid", paths.len()).map_err(io_failure)?;
    match first_refusal {
        Some(error) => Err(Failure::Refused(error)),
        None => Ok(()),
    }
}

fn inspect_text(definition: &EntityDefinition) -> String {
    let mut text = String::new();
    let _ = writeln!(
        text,
        "entity: {}  version: {}",
        definition.entity, definition.version
    );
    let states: Vec<String> = definition
        .lifecycle
        .states
        .iter()
        .map(|state| {
            if *state == definition.lifecycle.initial {
                format!("{state} (initial)")
            } else {
                state.clone()
            }
        })
        .collect();
    let _ = writeln!(text, "states: {}", states.join(", "));
    let _ = writeln!(text, "fields:");
    for (name, field) in &definition.schema.fields {
        let mut notes = vec![format!("{:?}", field.kind).to_lowercase()];
        if field.required {
            notes.push("required".into());
        }
        if let Some(default) = &field.default {
            notes.push(format!("default {default}"));
        }
        if !field.values.is_empty() {
            notes.push(format!("one of [{}]", field.values.join(", ")));
        }
        let _ = writeln!(text, "  {name}: {}", notes.join(", "));
    }
    if definition.schema.additional_fields {
        let _ = writeln!(text, "  (additional fields allowed)");
    }
    if !definition.invariants.is_empty() {
        let _ = writeln!(text, "invariants:");
        for rule in &definition.invariants {
            let _ = writeln!(text, "  {}", rule_line(rule));
        }
    }
    if let Some(event) = &definition.create.emit {
        let _ = writeln!(text, "create: emits {}", event.event_type);
    }
    let _ = writeln!(text, "operations:");
    for (name, operation) in &definition.operations {
        let transitions: Vec<String> = operation
            .transitions
            .iter()
            .map(|transition| {
                let from: Vec<&str> = transition.from.iter().map(String::as_str).collect();
                format!("{} -> {}", from.join("|"), transition.to)
            })
            .collect();
        let _ = writeln!(text, "  {name}: {}", transitions.join(", "));
        if !operation.arguments.fields.is_empty() {
            let args: Vec<String> = operation
                .arguments
                .fields
                .iter()
                .map(|(name, field)| {
                    if field.required {
                        format!("{name}*")
                    } else {
                        name.clone()
                    }
                })
                .collect();
            let _ = writeln!(text, "    arguments: {}  (* required)", args.join(", "));
        }
        for rule in &operation.preconditions {
            let _ = writeln!(text, "    precondition: {}", rule_line(rule));
        }
        if !operation.set.is_empty() {
            let set: Vec<&str> = operation.set.keys().map(String::as_str).collect();
            let _ = writeln!(text, "    sets: {}", set.join(", "));
        }
        if !operation.emits.is_empty() {
            let emits: Vec<&str> = operation
                .emits
                .iter()
                .map(|event| event.event_type.as_str())
                .collect();
            let _ = writeln!(text, "    emits: {}", emits.join(", "));
        }
    }
    text
}

fn rule_line(rule: &entity_core::RuleDefinition) -> String {
    match (&rule.name, &rule.message) {
        (Some(name), Some(message)) => format!("{name} — {message}"),
        (Some(name), None) => name.clone(),
        (None, Some(message)) => format!("(unnamed) — {message}"),
        (None, None) => "(unnamed)".into(),
    }
}

fn graph(definition: &EntityDefinition, format: GraphFormat) -> String {
    let mut edges = Vec::new();
    for (name, operation) in &definition.operations {
        for transition in &operation.transitions {
            for from in transition.from.iter() {
                edges.push((from.clone(), name.clone(), transition.to.clone()));
            }
        }
    }
    edges.sort();
    let mut text = String::new();
    match format {
        GraphFormat::Text => {
            let _ = writeln!(
                text,
                "{} v{}: initial {}",
                definition.entity, definition.version, definition.lifecycle.initial
            );
            for (from, operation, to) in &edges {
                let _ = writeln!(text, "{from} --{operation}--> {to}");
            }
        }
        GraphFormat::Dot => {
            let _ = writeln!(text, "digraph \"{}\" {{", definition.entity);
            let _ = writeln!(text, "  rankdir=LR;");
            let _ = writeln!(
                text,
                "  \"{}\" [peripheries=2];",
                definition.lifecycle.initial
            );
            for state in &definition.lifecycle.states {
                let _ = writeln!(text, "  \"{state}\";");
            }
            for (from, operation, to) in &edges {
                let _ = writeln!(text, "  \"{from}\" -> \"{to}\" [label=\"{operation}\"];");
            }
            let _ = writeln!(text, "}}");
        }
    }
    text
}

fn write_decision(
    out: &mut impl Write,
    decision: &Decision,
    format: Format,
) -> Result<(), Failure> {
    match format {
        Format::Json => write_all(out, &to_json(decision)?),
        Format::Yaml => write_all(out, &to_yaml(decision)?),
        Format::Text => {
            let instance = &decision.instance;
            let events: Vec<&str> = decision
                .events
                .iter()
                .map(|event| event.event_type.as_str())
                .collect();
            let events = if events.is_empty() {
                "none".to_owned()
            } else {
                events.join(", ")
            };
            write_all(
                out,
                &format!(
                    "{} {} is {} (revision {}); events: {events}\n",
                    instance.entity, instance.id, instance.lifecycle_state, instance.revision
                ),
            )
        }
    }
}

fn refusal(error: &CoreError) -> Value {
    let mut refusal = json!({ "kind": error.kind(), "message": error.to_string() });
    let details = match error {
        CoreError::Validation(errors) => {
            json!({ "errors": errors.iter().map(validation_error).collect::<Vec<_>>() })
        }
        CoreError::InvalidTransition { operation, state } => {
            json!({ "operation": operation, "state": state })
        }
        CoreError::PreconditionFailed {
            operation,
            rule,
            message,
        } => json!({ "operation": operation, "rule": rule, "reason": message }),
        CoreError::InvariantViolation { rule, message } => {
            json!({ "rule": rule, "reason": message })
        }
        CoreError::OperationNotFound { operation } => json!({ "operation": operation }),
        CoreError::EntityNotRegistered { entity, version } => {
            json!({ "entity": entity, "version": version })
        }
        CoreError::EntityMismatch {
            expected_entity,
            expected_version,
            actual_entity,
            actual_version,
        } => json!({
            "expected": { "entity": expected_entity, "version": expected_version },
            "actual": { "entity": actual_entity, "version": actual_version }
        }),
        CoreError::Template {
            expression,
            message,
        } => json!({ "expression": expression, "reason": message }),
        CoreError::Definition(_) => json!({}),
    };
    if let (Some(target), Some(source)) = (refusal.as_object_mut(), details.as_object()) {
        for (key, value) in source {
            target.insert(key.clone(), value.clone());
        }
    }
    refusal
}

fn validation_error(error: &ValidationError) -> Value {
    json!({ "path": error.path, "message": error.message })
}

fn to_json<T: serde::Serialize>(value: &T) -> Result<String, Failure> {
    serde_json::to_string_pretty(value)
        .map(|text| text + "\n")
        .map_err(|error| Failure::Usage(format!("cannot render JSON: {error}")))
}

fn to_yaml<T: serde::Serialize>(value: &T) -> Result<String, Failure> {
    serde_yaml::to_string(value)
        .map_err(|error| Failure::Usage(format!("cannot render YAML: {error}")))
}

fn write_all(out: &mut impl Write, text: &str) -> Result<(), Failure> {
    out.write_all(text.as_bytes()).map_err(io_failure)
}

fn io_failure(error: io::Error) -> Failure {
    Failure::Usage(format!("cannot write output: {error}"))
}
