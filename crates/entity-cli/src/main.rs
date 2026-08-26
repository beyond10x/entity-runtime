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
    CoreError, Decision, DefinitionErrors, EntityDefinition, EntityInstance, Registry, Runtime,
    ValidationError,
};
use entity_store::{Expect, FileStore, Recording, StateProvider, Store};
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
    /// Draw a definition: its lifecycle, or the references between several definitions.
    Graph {
        /// The definition files, YAML. Several are only useful with `--references`.
        #[arg(required = true)]
        definitions: Vec<PathBuf>,
        /// Draw the references between the definitions instead of one definition's lifecycle:
        /// entity types as nodes, `ref` fields as the edges between them.
        #[arg(long)]
        references: bool,
        #[arg(long, value_enum, default_value_t = GraphFormat::Text)]
        format: GraphFormat,
    },
    /// Create an instance: definition + id + fields -> Decision.
    Create {
        #[command(flatten)]
        definition: DefinitionArg,
        /// Which type to create, when several `--definition` files were given.
        ///
        /// Several are needed whenever a definition declares a `ref`: the type it points at has to
        /// be registered too, or the registry is not a consistent set. With one file this is
        /// unnecessary and the type is unambiguous.
        #[arg(long)]
        entity: Option<String>,
        /// The new instance's identity. The kernel generates none; you supply it.
        #[arg(long)]
        id: String,
        /// The fields, as inline JSON, `@<path>` or `-` for stdin.
        #[arg(long, default_value = "{}")]
        fields: String,
        /// A directory to keep the result in, so the next command can find it.
        ///
        /// Without one this prints a `Decision` and forgets it, which is the kernel's own shape:
        /// it decides and holds nothing. With one, the decision is committed — state and events
        /// together — and `execute --store` can pick the instance up by id instead of being handed
        /// it back on the command line.
        #[arg(long)]
        store: Option<PathBuf>,
        #[arg(long, value_enum, default_value_t = Format::Json)]
        format: Format,
    },
    /// Execute an operation: definition + instance + operation + arguments -> Decision.
    Execute {
        #[command(flatten)]
        definition: DefinitionArg,
        /// The current instance (or a Decision holding one), as inline JSON, `@<path>` or `-`.
        ///
        /// Not needed when `--store` and `--id` say where to find it.
        #[arg(long, required_unless_present = "store")]
        instance: Option<String>,
        /// A directory holding the instance, written by an earlier `create --store`.
        ///
        /// The instance is loaded from it, the decision is committed back to it at the revision
        /// that was loaded, and a concurrent writer is refused rather than overwritten.
        #[arg(long, requires = "id")]
        store: Option<PathBuf>,
        /// Which instance in the store to act on.
        #[arg(long, requires = "store")]
        id: Option<String>,
        /// Which type to act on, when several `--definition` files were given.
        #[arg(long = "entity")]
        wanted_entity: Option<String>,
        /// The operation name, as declared in the definition.
        #[arg(long)]
        operation: String,
        /// The arguments, as inline JSON, `@<path>` or `-` for stdin.
        #[arg(long, default_value = "{}")]
        arguments: String,
        /// The flow these events belong to. Enveloping is all-or-nothing: give this and the
        /// decision's events are printed sealed, with a time, a cause and an actor.
        #[arg(long, requires = "recorded_at", requires = "causation")]
        correlation: Option<String>,
        /// When this was recorded, ISO-8601. Your clock: the kernel has none.
        #[arg(long)]
        recorded_at: Option<String>,
        /// What immediately led to this — the step before it, not the whole flow.
        #[arg(long)]
        causation: Option<String>,
        /// Who asked. Leave it out and the envelope records that nothing human did, explicitly.
        #[arg(long)]
        actor: Option<String>,
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
    /// One line per edge: `from --label--> to`.
    Text,
    /// Graphviz DOT, for whoever already has `dot`.
    Dot,
    /// A standalone SVG, laid out here rather than by a tool nobody controls the version of.
    Svg,
    /// One self-contained page: the drawing, and the same edges as a table beneath it.
    Html,
}

/// Why the command did not produce a result, and which exit code that earns.
enum Failure {
    /// The kernel refused. Exit 1. The refusal is printed to stdout in JSON.
    Refused(CoreError),
    /// The store refused. Exit 1, beside the kernel's refusals rather than beside a usage error:
    /// a revision conflict is not a wrong invocation, it is somebody else having moved first.
    StoreRefused(String),
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

impl From<DefinitionErrors> for Failure {
    fn from(error: DefinitionErrors) -> Self {
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
        Err(Failure::StoreRefused(message)) => {
            // Same shape as a kernel refusal — JSON on stdout for a pipeline, a sentence on stderr
            // for a person — because to a caller it is the same class of answer: no, and here is
            // exactly what was found instead.
            let _ = writeln!(
                out,
                "{}",
                serde_json::to_string_pretty(&json!({
                    "refused": true,
                    "by": "store",
                    "detail": message,
                }))
                .expect("json")
            );
            eprintln!("refused: {message}");
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
        Command::Graph {
            definitions,
            references,
            format,
        } => {
            let loaded = definitions
                .iter()
                .map(|path| load_validated(path))
                .collect::<Result<Vec<_>, _>>()?;
            let drawing =
                if references {
                    entity_graph::Graph::references(&loaded)
                } else {
                    match loaded.as_slice() {
                        [only] => entity_graph::Graph::lifecycle(only),
                        _ => return Err(Failure::Usage(
                            "a lifecycle is one definition's; pass one file, or --references to \
                             draw the edges between several"
                                .to_owned(),
                        )),
                    }
                };
            write_all(out, &graph(&drawing, format))
        }
        Command::Create {
            definition,
            entity: wanted,
            id,
            fields,
            store,
            format,
        } => {
            let registry = load_registry(&definition.definitions)?;
            let (entity, version) = chosen_type(&registry, wanted.as_deref())?;
            let fields = read_value(&fields, "--fields", &mut StdinOnce::default())?;
            let decision = Runtime::new(&registry).create(&entity, version, id, fields)?;
            if let Some(root) = store {
                // A creation expects nothing to be there. Committing a second one under the same
                // identity is refused rather than overwriting the first.
                commit(&root, &decision, Expect::Absent)?;
            }
            write_decision(out, &decision, format)
        }
        Command::Execute {
            definition,
            instance,
            store,
            id,
            wanted_entity,
            operation,
            arguments,
            correlation,
            recorded_at,
            causation,
            actor,
            format,
        } => {
            let registry = load_registry(&definition.definitions)?;
            // One reader, so a second `-` is refused rather than silently handed an empty
            // document: the caller's arguments would otherwise be consumed as the instance.
            let mut stdin = StdinOnce::default();
            let (instance, from_store) = match (&store, &id) {
                (Some(root), Some(id)) => {
                    let (entity, _) = chosen_type(&registry, wanted_entity.as_deref())?;
                    let held = FileStore::open(root)
                        .load(&entity, id)
                        .map_err(|error| Failure::Usage(error.to_string()))?
                        .ok_or_else(|| {
                            Failure::Usage(format!(
                                "the store at {} holds no {entity} with id {id}",
                                root.display()
                            ))
                        })?;
                    (held, true)
                }
                _ => {
                    let source = instance.as_deref().expect("clap requires one of the two");
                    (read_instance(source, &mut stdin)?, false)
                }
            };
            let arguments = read_value(&arguments, "--arguments", &mut stdin)?;
            // The revision *as loaded*, so a writer that moved in between is refused rather than
            // overwritten. Reading it before executing is the point: the decision's own revision is
            // already one ahead.
            let expected = Expect::Revision(instance.revision);
            let decision = Runtime::new(&registry).execute(&instance, &operation, arguments)?;
            if from_store {
                commit(store.as_ref().expect("checked above"), &decision, expected)?;
            }
            // Sealed only when the caller supplied what the kernel cannot know. Clap requires the
            // three together, so there is no half-enveloped shape to interpret.
            if let (Some(correlation), Some(recorded_at), Some(causation)) =
                (correlation, recorded_at, causation)
            {
                let recording = Recording {
                    recorded_at,
                    correlation,
                    causation,
                    actor,
                };
                let sealed = recording.seal(&decision.events);
                return write_all(
                    out,
                    &to_json(&json!({
                        "instance": decision.instance,
                        "events": sealed,
                    }))?,
                );
            }
            write_decision(out, &decision, format)
        }
    }
}

/// Commits a decision to the store at `root`, turning a refusal into a usage failure.
///
/// Exit 1 rather than 2: a revision conflict is not a wrong invocation, it is the store answering
/// that somebody else moved first — the same class of "no" as the kernel refusing an operation.
fn commit(root: &Path, decision: &Decision, expect: Expect) -> Result<(), Failure> {
    FileStore::open(root)
        .commit(decision, expect)
        .map_err(|error| Failure::StoreRefused(error.to_string()))
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
    // Asked of the finished set, not of each file: two types that point at each other are ordinary,
    // and a check that ran per file would refuse whichever was loaded first.
    registry.validate_all()?;
    Ok(registry)
}

/// `create` needs to know which type to create. With one definition file that is unambiguous;
/// with several it is not, and the command says so rather than guessing.
fn chosen_type(registry: &Registry, wanted: Option<&str>) -> Result<(String, u32), Failure> {
    let mut named: Vec<(String, u32)> = registry
        .iter()
        .filter(|definition| wanted.is_none_or(|wanted| definition.entity == wanted))
        .map(|definition| (definition.entity.clone(), definition.version))
        .collect();

    match (named.len(), wanted) {
        (1, _) => Ok(named.remove(0)),
        // Several definitions and no `--entity`. This is ordinary now rather than a mistake: a
        // definition that declares a `ref` needs the type it points at registered beside it, so a
        // reference example is *always* several files. Name the type instead of guessing.
        (_, None) => {
            let available: Vec<&str> = registry
                .iter()
                .map(|definition| definition.entity.as_str())
                .collect();
            Err(Failure::Usage(format!(
                "several definitions are registered, so which type to create is ambiguous — pass \
                 --entity, one of: {}",
                available.join(", ")
            )))
        }
        (0, Some(wanted)) => {
            let available: Vec<&str> = registry
                .iter()
                .map(|definition| definition.entity.as_str())
                .collect();
            Err(Failure::Usage(format!(
                "no --definition declares entity '{wanted}'; the registry holds: {}",
                available.join(", ")
            )))
        }
        (_, Some(wanted)) => Err(Failure::Usage(format!(
            "several versions of '{wanted}' are registered; create the version you mean by \
             passing only its definition"
        ))),
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
        let outcome = load_definition(path)
            .map_err(|reason| vec![reason])
            .and_then(|definition| {
                definition
                    .validate()
                    .map(|()| (definition.entity.clone(), definition.version))
                    .map_err(|errors| errors.iter().map(ToString::to_string).collect())
            });
        match outcome {
            Ok((entity, version)) => {
                writeln!(out, "{}: valid ({entity} v{version})", path.display())
                    .map_err(io_failure)?;
            }
            Err(reasons) => {
                invalid += 1;
                // One line names the file and how much is wrong with it; the defects follow,
                // indented, one per line. Reporting only the first meant fixing a definition took
                // as many runs as it had faults.
                match reasons.as_slice() {
                    [only] => writeln!(out, "{}: invalid: {only}", path.display()),
                    many => writeln!(out, "{}: invalid: {} defects", path.display(), many.len()),
                }
                .map_err(io_failure)?;
                if reasons.len() > 1 {
                    for reason in &reasons {
                        writeln!(out, "  {reason}").map_err(io_failure)?;
                    }
                }
            }
        }
    }
    // Validating several definitions is validating a *set*, and a set has a question a file does
    // not: does every `ref` point at a type somebody declared? Without this the gate could run
    // `validate` over a reference example and miss a dangling edge entirely, which is what an
    // independent review of this command found.
    let mut dangling = 0usize;
    if invalid == 0 && paths.len() > 1 {
        let mut registry = Registry::new();
        for path in paths {
            if let Ok(definition) = load_definition(path) {
                let _ = registry.register(definition);
            }
        }
        if let Err(errors) = registry.validate_all() {
            dangling = errors.len();
            for error in &errors {
                writeln!(out, "across the set: {error}").map_err(io_failure)?;
            }
        }
    }

    writeln!(out, "{} file(s), {invalid} invalid", paths.len()).map_err(io_failure)?;
    if invalid > 0 || dangling > 0 {
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
        // A reference's notes come from the field itself, or from an array's `items` — the same
        // declaration either way, and a reader should not have to know which shape it was written
        // in to be told what the edge points at.
        let reference = field
            .entity
            .as_ref()
            .map(|entity| (entity, field, ""))
            .or_else(|| {
                let items = field.items.as_ref()?;
                Some((items.entity.as_ref()?, items.as_ref(), " (each)"))
            });
        if let Some((entity, declared, each)) = reference {
            notes.push(format!("-> {entity}{each}"));
            if let Some(inverse) = &declared.inverse {
                notes.push(format!("read back as {inverse}"));
            }
            if declared.is_acyclic() {
                notes.push("acyclic".into());
            }
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

fn graph(drawing: &entity_graph::Graph, format: GraphFormat) -> String {
    // The drawing, the layout and every emitter live in `entity-graph`: this is a shell, and a
    // renderer in it would be one nothing but this binary could use — the library caller who wants
    // the same picture would have to reimplement it.
    let layout = entity_graph::Layout::of(drawing);
    match format {
        GraphFormat::Text => entity_graph::render::text(drawing),
        GraphFormat::Dot => entity_graph::render::dot(drawing),
        GraphFormat::Svg => entity_graph::render::svg(drawing, &layout),
        GraphFormat::Html => entity_graph::render::html(drawing, &layout),
    }
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
        CoreError::PreconditionUnobservable {
            operation,
            rule,
            message,
            unresolved,
        } => json!({
            "operation": operation, "rule": rule, "reason": message, "unresolved": unresolved
        }),
        CoreError::InvariantViolation { rule, message } => {
            json!({ "rule": rule, "reason": message })
        }
        CoreError::InvariantUnobservable {
            rule,
            message,
            unresolved,
        } => json!({ "rule": rule, "reason": message, "unresolved": unresolved }),
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
        // Every defect, not the first: a caller fixing a definition from this output should not
        // have to run the command once per fault.
        CoreError::Definition(errors) => json!({
            "defect": errors.first().kind(),
            "defects": errors.iter()
                .map(|defect| json!({ "kind": defect.kind(), "message": defect.to_string() }))
                .collect::<Vec<_>>()
        }),
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
