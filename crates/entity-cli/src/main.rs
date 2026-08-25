//! `entity` — the reference shell around `entity-core`.
//!
//! Everything the kernel refuses to do happens here and only here: files are read, standard input
//! is consumed, output is printed and an exit code is chosen. Identifiers come from the caller.
//! Nothing reads a clock.
//!
//! Exit codes: `0` the kernel produced a result · `1` the kernel refused, or a definition was
//! invalid (the typed refusal is printed) · `2` the invocation itself was wrong — a missing file,
//! unparsable input, two flags reading standard input.

use clap::{Args, Parser, Subcommand, ValueEnum};
use entity_core::{
    CoreError, Decision, DefinitionError, EntityDefinition, EntityInstance, Registry, Runtime,
    ValidationError,
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
Only one flag per invocation may read standard input. A Decision printed by `create` or `execute`
can be fed straight back as an --instance.

Exit codes: 0 decided · 1 refused (or a definition is invalid) · 2 invalid invocation.";

#[derive(Parser)]
#[command(name = "entity", version, about = ABOUT, long_about = LONG_ABOUT)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Check definitions; report every file, and exit 1 if any is invalid.
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
    /// The definition file, YAML. Repeat to register several types or versions at once.
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
    /// The kernel refused. Exit 1. The refusal is printed to stdout in JSON.
    Refused(CoreError),
    /// Already reported in full on stdout — `validate` prints a line per file. Exit 1.
    Reported,
    /// The invocation was wrong. Exit 2. Printed to stderr.
    Usage(String),
}

impl From<CoreError> for Failure {
    fn from(error: CoreError) -> Self {
        Self::Refused(error)
    }
}

impl From<DefinitionError> for Failure {
    fn from(error: DefinitionError) -> Self {
        Self::Refused(error.into())
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
        Err(Failure::Reported) => ExitCode::from(1),
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
            let definition = load_validated(&definition)?;
            match format {
                Format::Text => write_all(out, &inspect_text(&definition)),
                Format::Json => write_all(out, &to_json(&definition)?),
                Format::Yaml => write_all(out, &to_yaml(&definition)?),
            }
        }
        Command::Graph { definition, format } => {
            let definition = load_validated(&definition)?;
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
            let fields = read_value(&fields, "--fields", &mut StdinOnce::default())?;
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
            // One reader, so a second `-` is refused rather than silently handed an empty
            // document: the caller's arguments would otherwise be consumed as the instance.
            let mut stdin = StdinOnce::default();
            let instance = read_instance(&instance, &mut stdin)?;
            let arguments = read_value(&arguments, "--arguments", &mut stdin)?;
            let decision = Runtime::new(&registry).execute(&instance, &operation, arguments)?;
            write_decision(out, &decision, format)
        }
    }
}

// --- Loading ---------------------------------------------------------------------------------------

/// Standard input can be read once. The second `-` is an invocation error, not a mystery.
#[derive(Default)]
struct StdinOnce {
    taken: Option<&'static str>,
}

impl StdinOnce {
    fn read(&mut self, flag: &'static str) -> Result<String, Failure> {
        if let Some(first) = self.taken {
            return Err(Failure::Usage(format!(
                "{first} already reads standard input; {flag} cannot read it as well — pass one of \
                 them inline or as @<path>"
            )));
        }
        self.taken = Some(flag);
        let mut text = String::new();
        io::stdin()
            .read_to_string(&mut text)
            .map_err(|error| Failure::Usage(format!("cannot read stdin for {flag}: {error}")))?;
        Ok(text)
    }
}

fn load_definition(path: &Path) -> Result<EntityDefinition, String> {
    let text = fs::read_to_string(path).map_err(|error| format!("cannot read: {error}"))?;
    entity_yaml::from_str(&text).map_err(|error| error.to_string())
}

/// One definition, parsed and validated — what `inspect`, `graph` and `validate` all need.
fn load_validated(path: &Path) -> Result<EntityDefinition, Failure> {
    let definition = load_definition(path)
        .map_err(|error| Failure::Usage(format!("{}: {error}", path.display())))?;
    definition.validate()?;
    Ok(definition)
}

fn load_registry(paths: &[PathBuf]) -> Result<Registry, Failure> {
    let mut registry = Registry::new();
    for path in paths {
        let definition = load_validated(path)?;
        registry.register(definition)?;
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

/// Inline JSON, `@path`, or `-` for stdin. Files and stdin may be JSON or YAML.
///
/// JSON is tried first and YAML second, so a document that is valid JSON is read as JSON —
/// notably one carrying surrogate-pair escapes (`😀`, what `json.dumps` and `jq -a`
/// emit by default), which a YAML 1.1 parser rejects.
fn read_value(source: &str, flag: &'static str, stdin: &mut StdinOnce) -> Result<Value, Failure> {
    let text = if source == "-" {
        stdin.read(flag)?
    } else if let Some(path) = source.strip_prefix('@') {
        fs::read_to_string(path)
            .map_err(|error| Failure::Usage(format!("cannot read {path} for {flag}: {error}")))?
    } else {
        source.to_owned()
    };
    parse_value(&text, flag)
}

fn parse_value(text: &str, flag: &str) -> Result<Value, Failure> {
    match serde_json::from_str(text) {
        Ok(value) => Ok(value),
        Err(json) => serde_yaml_ng::from_str(text).map_err(|yaml| {
            Failure::Usage(format!(
                "{flag} is not valid JSON or YAML: {yaml} (as JSON: {json})"
            ))
        }),
    }
}

/// An `EntityInstance`, or a `Decision` from an earlier `create`/`execute`, whose instance is taken.
fn read_instance(source: &str, stdin: &mut StdinOnce) -> Result<EntityInstance, Failure> {
    let mut value = read_value(source, "--instance", stdin)?;
    if let Some(inner) = value.get("instance").cloned() {
        if value.get("events").is_some() {
            value = inner;
        }
    }
    serde_json::from_value(value)
        .map_err(|error| Failure::Usage(format!("--instance is not an entity instance: {error}")))
}

// --- Rendering -------------------------------------------------------------------------------------

/// Every file is reported, whatever went wrong with the one before it: a syntax slip in the first
/// example must not hide a broken lifecycle in the third.
fn validate(paths: &[PathBuf], out: &mut impl Write) -> Result<(), Failure> {
    let mut invalid = 0usize;
    for path in paths {
        let outcome = load_definition(path).and_then(|definition| {
            definition
                .validate()
                .map(|()| (definition.entity.clone(), definition.version))
                .map_err(|error| error.to_string())
        });
        match outcome {
            Ok((entity, version)) => {
                writeln!(out, "{}: valid ({entity} v{version})", path.display())
                    .map_err(io_failure)?;
            }
            Err(reason) => {
                invalid += 1;
                writeln!(out, "{}: invalid: {reason}", path.display()).map_err(io_failure)?;
            }
        }
    }
    writeln!(out, "{} file(s), {invalid} invalid", paths.len()).map_err(io_failure)?;
    if invalid > 0 {
        return Err(Failure::Reported);
    }
    Ok(())
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
        let mut notes = vec![field.kind.to_string()];
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

/// A DOT string literal. A state called `a"b` would otherwise close the quote and produce a graph
/// no renderer accepts — or, worse, one carrying attributes nobody wrote.
fn dot_quote(value: &str) -> String {
    let mut quoted = String::with_capacity(value.len() + 2);
    quoted.push('"');
    for character in value.chars() {
        match character {
            '"' | '\\' => {
                quoted.push('\\');
                quoted.push(character);
            }
            '\n' => quoted.push_str("\\n"),
            other => quoted.push(other),
        }
    }
    quoted.push('"');
    quoted
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
            let _ = writeln!(text, "digraph {} {{", dot_quote(&definition.entity));
            let _ = writeln!(text, "  rankdir=LR;");
            let _ = writeln!(
                text,
                "  {} [peripheries=2];",
                dot_quote(&definition.lifecycle.initial)
            );
            for state in &definition.lifecycle.states {
                let _ = writeln!(text, "  {};", dot_quote(state));
            }
            for (from, operation, to) in &edges {
                let _ = writeln!(
                    text,
                    "  {} -> {} [label={}];",
                    dot_quote(from),
                    dot_quote(to),
                    dot_quote(operation)
                );
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
        CoreError::UnknownState { entity, state } => json!({ "entity": entity, "state": state }),
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
        CoreError::Definition(error) => json!({ "defect": error.kind() }),
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
    serde_yaml_ng::to_string(value)
        .map_err(|error| Failure::Usage(format!("cannot render YAML: {error}")))
}

fn write_all(out: &mut impl Write, text: &str) -> Result<(), Failure> {
    out.write_all(text.as_bytes()).map_err(io_failure)
}

fn io_failure(error: io::Error) -> Failure {
    Failure::Usage(format!("cannot write output: {error}"))
}
