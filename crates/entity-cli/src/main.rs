//! `entity` — the reference shell around `entity-core`.
//!
//! Everything the kernel refuses to do happens here and only here: files are read, standard input
//! is consumed, output is printed and an exit code is chosen. Identifiers come from the caller.
//! Nothing reads a clock.
//!
//! Exit codes: `0` the kernel produced a result · `1` the kernel refused, or a definition was
//! invalid (the typed refusal is printed) · `2` the invocation itself was wrong — a missing file,
//! unparsable input, two flags reading standard input.

use clap::{ArgGroup, Args, Parser, Subcommand, ValueEnum};
use entity_core::{
    CoreError, Decision, DefinitionErrors, EntityDefinition, EntityInstance, Registry, Runtime,
    ValidationError,
};
use entity_store::{
    migrate_file_store_v1, Expect, FileStore, RecordedCommit, Recording, StateProvider, Store,
};
use serde_json::{json, Value};
use std::{
    fmt::Write as _,
    fs,
    io::{self, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Command as ProcessCommand, ExitCode},
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
        #[arg(
            long,
            requires_all = ["record_id", "recorded_at", "actor_choice"]
        )]
        store: Option<PathBuf>,
        /// Provenance required when the decision is stored.
        #[command(flatten)]
        recording: RecordingArgs,
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
        #[arg(
            long,
            requires = "id",
            requires_all = ["record_id", "recorded_at", "actor_choice"]
        )]
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
        /// Provenance required when the decision is stored.
        #[command(flatten)]
        recording: RecordingArgs,
        #[arg(long, value_enum, default_value_t = Format::Json)]
        format: Format,
    },
    /// List what a store holds for one entity type: every identity, sorted, one per line.
    ///
    /// The question `create --store` and `execute --store` could not answer: they can act on an
    /// instance whose id you already know, and nothing could say which ids there are. A shell that
    /// did not write a store has to be able to ask it what it holds before it can do anything else.
    List {
        /// The directory an earlier `create --store` wrote into.
        #[arg(long)]
        store: PathBuf,
        /// Which entity type to list.
        #[arg(long)]
        entity: String,
        #[arg(long, value_enum, default_value_t = Format::Text)]
        format: Format,
    },
    /// Generate public surfaces from a validated definition set.
    Generate {
        /// The generated artifact.
        #[command(subcommand)]
        command: GenerateCommand,
    },
    /// Mount stored entities as model-controlled MCP tools over standard input/output.
    Mcp {
        #[command(flatten)]
        definition: DefinitionArg,
        /// File Store v2 root used by every tool call.
        #[arg(long)]
        store: PathBuf,
    },
    /// Work with persistent stores.
    Store {
        /// The store operation.
        #[command(subcommand)]
        command: StoreCommand,
    },
    /// Render a compact Agent Skills document that teaches this installed CLI.
    Skill {
        /// Write the skill to this path instead of standard output.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Replace the explicitly named output file when it already exists.
        #[arg(long, requires = "out")]
        force: bool,
    },
}

#[derive(Subcommand)]
enum GenerateCommand {
    /// Write standalone HTML/Markdown docs plus OpenAPI and AsyncAPI contracts.
    Docs {
        #[command(flatten)]
        definition: DefinitionArg,
        /// Destination directory.
        #[arg(long)]
        out: PathBuf,
        /// Replace this exact directory only when it carries the generator marker.
        #[arg(long)]
        force: bool,
    },
    /// Generate, compile and install a definition-specific Rust command.
    RustCli {
        #[command(flatten)]
        definition: DefinitionArg,
        /// Binary and Cargo package name.
        #[arg(long)]
        name: String,
        /// Installed host-platform binary path.
        #[arg(long)]
        out: PathBuf,
        /// Matching entity-runtime source checkout. Defaults to the current directory.
        #[arg(long, default_value = ".")]
        runtime_source: PathBuf,
        /// Retained generated crate. Defaults to build/entity-runtime/NAME.
        #[arg(long)]
        build_dir: Option<PathBuf>,
        /// Replace only exact generator-owned build and output targets.
        #[arg(long)]
        force: bool,
    },
}

#[derive(Subcommand)]
enum StoreCommand {
    /// Migrate a pre-0.15 File Store into the confined v2 format, out of place.
    MigrateFile {
        /// The legacy File Store directory. It is never modified.
        #[arg(long, value_name = "V1_ROOT")]
        from: PathBuf,
        /// A destination path that does not exist.
        #[arg(long, value_name = "V2_ROOT")]
        to: PathBuf,
        /// Validate the complete migration without writing destination bytes.
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Args)]
struct DefinitionArg {
    /// The definition file, YAML. Repeat to register several types or versions at once.
    #[arg(long = "definition", required = true)]
    definitions: Vec<PathBuf>,
}

#[derive(Args)]
#[command(group(
    ArgGroup::new("actor_choice")
        .args(["actor", "no_actor"])
        .multiple(false)
))]
struct RecordingArgs {
    /// Caller-supplied idempotency identity for this complete decision.
    #[arg(long, requires = "recorded_at", requires = "actor_choice")]
    record_id: Option<String>,
    /// When this was recorded, ISO-8601. Your clock: the kernel has none.
    #[arg(long, requires = "record_id", requires = "actor_choice")]
    recorded_at: Option<String>,
    /// The wider flow, when there is one.
    #[arg(long, requires = "record_id")]
    correlation: Option<String>,
    /// What immediately led to this record, when there is one.
    #[arg(long, requires = "record_id")]
    causation: Option<String>,
    /// Who asked.
    #[arg(long, requires = "record_id")]
    actor: Option<String>,
    /// Record explicitly that no actor caused this decision.
    #[arg(long, requires = "record_id")]
    no_actor: bool,
}

impl RecordingArgs {
    fn into_recording(self, stored: bool) -> Result<Option<Recording>, Failure> {
        if stored && self.record_id.is_none() {
            return Err(Failure::Usage(
                "--store requires --record-id, --recorded-at and exactly one of --actor/--no-actor"
                    .to_owned(),
            ));
        }
        let Some(record_id) = self.record_id else {
            return Ok(None);
        };
        let recorded_at = self
            .recorded_at
            .ok_or_else(|| Failure::Usage("--record-id requires --recorded-at".to_owned()))?;
        if self.actor.is_none() && !self.no_actor {
            return Err(Failure::Usage(
                "--record-id requires exactly one of --actor/--no-actor".to_owned(),
            ));
        }
        Ok(Some(Recording {
            record_id,
            recorded_at,
            correlation: self.correlation,
            causation: self.causation,
            actor: self.actor,
        }))
    }
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
    /// Mermaid state-diagram or flowchart source.
    Mermaid,
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
            recording,
            format,
        } => {
            let registry = load_registry(&definition.definitions)?;
            let (entity, version) = chosen_type(&registry, wanted.as_deref())?;
            let fields = read_value(&fields, "--fields", &mut StdinOnce::default())?;
            let decision = Runtime::new(&registry).create(&entity, version, id, fields)?;
            let recording = recording.into_recording(store.is_some())?;
            if let Some(recording) = recording {
                let recorded = RecordedCommit::new(decision, &recording)
                    .map_err(|error| Failure::Usage(error.to_string()))?;
                if let Some(root) = &store {
                    commit(root, &recorded, Expect::Absent)?;
                }
                write_recorded(out, &recorded, format)
            } else {
                write_decision(out, &decision, format)
            }
        }
        Command::Execute {
            definition,
            instance,
            store,
            id,
            wanted_entity,
            operation,
            arguments,
            recording,
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
            let recording = recording.into_recording(from_store)?;
            if let Some(recording) = recording {
                let recorded = RecordedCommit::new(decision, &recording)
                    .map_err(|error| Failure::Usage(error.to_string()))?;
                if from_store {
                    commit(store.as_ref().expect("checked above"), &recorded, expected)?;
                }
                write_recorded(out, &recorded, format)
            } else {
                write_decision(out, &decision, format)
            }
        }
        Command::List {
            store,
            entity,
            format,
        } => {
            // A store that cannot be read is a wrong invocation — the path, most likely — and is
            // reported as one; the store answering "nothing" for a type nobody stored under is an
            // answer, printed as an empty list with exit 0.
            let ids = FileStore::open(&store)
                .ids(&entity)
                .map_err(|error| Failure::Usage(error.to_string()))?;
            match format {
                Format::Text => {
                    let text: String = ids.iter().map(|id| format!("{id}\n")).collect();
                    write_all(out, &text)
                }
                Format::Json => write_all(out, &to_json(&ids)?),
                Format::Yaml => write_all(out, &to_yaml(&ids)?),
            }
        }
        Command::Generate { command } => match command {
            GenerateCommand::Docs {
                definition,
                out: destination,
                force,
            } => {
                let registry = load_registry(&definition.definitions)?;
                let definitions: Vec<EntityDefinition> = registry
                    .iter()
                    .map(|definition| definition.as_definition().clone())
                    .collect();
                let bundle = entity_surface::documentation(&definitions).map_err(Failure::Usage)?;
                install_documentation(&destination, &bundle, force)?;
                writeln!(
                    out,
                    "generated {} file(s) for {} definition(s) at {}",
                    bundle.len(),
                    definitions.len(),
                    destination.display()
                )
                .map_err(io_failure)
            }
            GenerateCommand::RustCli {
                definition,
                name,
                out: destination,
                runtime_source,
                build_dir,
                force,
            } => generate_rust_cli(
                &definition.definitions,
                &name,
                &destination,
                &runtime_source,
                build_dir.as_deref(),
                force,
                out,
            ),
        },
        Command::Mcp { definition, store } => {
            let registry = load_registry(&definition.definitions)?;
            let file_store = FileStore::open(&store);
            let mut server =
                entity_mcp::Server::new(&registry, file_store).map_err(Failure::Usage)?;
            let stdin = io::stdin();
            let mut input = BufReader::new(stdin.lock());
            server
                .serve(&mut input, out)
                .map_err(|error| Failure::Usage(format!("MCP transport failed: {error}")))
        }
        Command::Store { command } => match command {
            StoreCommand::MigrateFile { from, to, dry_run } => {
                let report = migrate_file_store_v1(&from, &to, dry_run)
                    .map_err(|error| Failure::Usage(error.to_string()))?;
                if report.dry_run {
                    writeln!(
                        out,
                        "valid: {} subject(s), {} event(s); no files written",
                        report.subjects, report.events
                    )
                    .map_err(io_failure)
                } else {
                    writeln!(
                        out,
                        "migrated {} subject(s), {} event(s) from {} to {}",
                        report.subjects,
                        report.events,
                        from.display(),
                        to.display()
                    )
                    .map_err(io_failure)
                }
            }
        },
        Command::Skill { out: path, force } => render_skill(out, path.as_deref(), force),
    }
}

const ENTITY_SKILL: &str = include_str!("../assets/entity-skill.md");

fn install_documentation(
    destination: &Path,
    bundle: &entity_surface::DocumentationBundle,
    force: bool,
) -> Result<(), Failure> {
    let parent = destination.parent().unwrap_or_else(|| Path::new("."));
    let name = destination.file_name().ok_or_else(|| {
        Failure::Usage("--out must name a directory, not a filesystem root".into())
    })?;
    fs::create_dir_all(parent)
        .map_err(|error| Failure::Usage(format!("cannot create {}: {error}", parent.display())))?;
    if destination.exists() {
        if !force {
            return Err(Failure::Usage(format!(
                "{} already exists; pass --force to replace a generated directory",
                destination.display()
            )));
        }
        if !destination.join(entity_surface::DOCS_MARKER).is_file() {
            return Err(Failure::Usage(format!(
                "{} is not marked as entity-runtime generated documentation and will not be replaced",
                destination.display()
            )));
        }
    }
    let stage = parent.join(format!(
        ".{}.generating.{}",
        name.to_string_lossy(),
        std::process::id()
    ));
    if stage.exists() {
        return Err(Failure::Usage(format!(
            "staging directory {} already exists",
            stage.display()
        )));
    }
    for (relative, contents) in bundle {
        let relative = Path::new(relative);
        if relative.is_absolute()
            || relative
                .components()
                .any(|part| !matches!(part, std::path::Component::Normal(_)))
        {
            return Err(Failure::Usage(format!(
                "generator produced unsafe relative path {}",
                relative.display()
            )));
        }
        let path = stage.join(relative);
        if let Some(directory) = path.parent() {
            fs::create_dir_all(directory).map_err(|error| {
                Failure::Usage(format!("cannot create {}: {error}", directory.display()))
            })?;
        }
        fs::write(&path, contents)
            .map_err(|error| Failure::Usage(format!("cannot write {}: {error}", path.display())))?;
    }
    if destination.exists() {
        let backup = parent.join(format!(
            ".{}.replaced.{}",
            name.to_string_lossy(),
            std::process::id()
        ));
        fs::rename(destination, &backup).map_err(|error| {
            Failure::Usage(format!(
                "cannot stage replacement of {}: {error}",
                destination.display()
            ))
        })?;
        if let Err(error) = fs::rename(&stage, destination) {
            let _ = fs::rename(&backup, destination);
            return Err(Failure::Usage(format!(
                "cannot publish {}: {error}",
                destination.display()
            )));
        }
        fs::remove_dir_all(&backup).map_err(|error| {
            Failure::Usage(format!(
                "published {}, but cannot remove generated backup {}: {error}",
                destination.display(),
                backup.display()
            ))
        })
    } else {
        fs::rename(&stage, destination).map_err(|error| {
            Failure::Usage(format!("cannot publish {}: {error}", destination.display()))
        })
    }
}

const GENERATED_CLI_MARKER: &str = ".entity-runtime-cli.json";

#[allow(clippy::too_many_arguments)]
fn generate_rust_cli(
    definition_paths: &[PathBuf],
    name: &str,
    destination: &Path,
    runtime_source: &Path,
    requested_build_dir: Option<&Path>,
    force: bool,
    out: &mut impl Write,
) -> Result<(), Failure> {
    if !valid_binary_name(name) {
        return Err(Failure::Usage(
            "--name must start with an ASCII letter and contain only letters, digits, '-' or '_'"
                .into(),
        ));
    }
    let runtime_source = runtime_source.canonicalize().map_err(|error| {
        Failure::Usage(format!(
            "cannot resolve runtime source {}: {error}",
            runtime_source.display()
        ))
    })?;
    let workspace_manifest =
        fs::read_to_string(runtime_source.join("Cargo.toml")).map_err(|error| {
            Failure::Usage(format!(
                "{} is not an entity-runtime source checkout: {error}",
                runtime_source.display()
            ))
        })?;
    let expected = format!("version = \"{}\"", env!("CARGO_PKG_VERSION"));
    if !workspace_manifest.contains(&expected) {
        return Err(Failure::Usage(format!(
            "{} does not declare entity-runtime version {}; use the matching source checkout",
            runtime_source.display(),
            env!("CARGO_PKG_VERSION")
        )));
    }

    let registry = load_registry(definition_paths)?;
    let definitions: Vec<EntityDefinition> = registry
        .iter()
        .map(|definition| definition.as_definition().clone())
        .collect();
    for definition in &definitions {
        for operation in definition.operations.keys() {
            if ["create", "get", "list", "events"].contains(&operation.as_str()) {
                return Err(Failure::Usage(format!(
                    "operation {operation:?} on {} collides with a generated CLI command",
                    definition.entity
                )));
            }
        }
    }
    let build_dir = requested_build_dir
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("build/entity-runtime").join(name));
    if build_dir.parent().is_none() {
        return Err(Failure::Usage(
            "--build-dir must not be a filesystem root".into(),
        ));
    }
    if build_dir.exists() {
        if !force || !build_dir.join(GENERATED_CLI_MARKER).is_file() {
            return Err(Failure::Usage(format!(
                "{} already exists and is not authorised for replacement",
                build_dir.display()
            )));
        }
        fs::remove_dir_all(&build_dir).map_err(|error| {
            Failure::Usage(format!("cannot replace {}: {error}", build_dir.display()))
        })?;
    }
    if destination.exists() && !force {
        return Err(Failure::Usage(format!(
            "{} already exists; pass --force to replace that exact binary",
            destination.display()
        )));
    }
    fs::create_dir_all(build_dir.join("src")).map_err(|error| {
        Failure::Usage(format!("cannot create {}: {error}", build_dir.display()))
    })?;
    fs::create_dir_all(build_dir.join("definitions"))
        .map_err(|error| Failure::Usage(format!("cannot create definitions directory: {error}")))?;
    for (at, path) in definition_paths.iter().enumerate() {
        let contents = fs::read_to_string(path)
            .map_err(|error| Failure::Usage(format!("cannot read {}: {error}", path.display())))?;
        fs::write(build_dir.join(format!("definitions/{at}.yaml")), contents).map_err(|error| {
            Failure::Usage(format!("cannot write embedded definition: {error}"))
        })?;
    }
    fs::write(
        build_dir.join(GENERATED_CLI_MARKER),
        format!(
            "{{\"format\":\"entity-runtime-cli/1\",\"runtime\":\"{}\"}}\n",
            env!("CARGO_PKG_VERSION")
        ),
    )
    .map_err(|error| Failure::Usage(format!("cannot write generator marker: {error}")))?;

    let manifest = generated_manifest(name, &runtime_source);
    fs::write(build_dir.join("Cargo.toml"), manifest)
        .map_err(|error| Failure::Usage(format!("cannot write generated Cargo.toml: {error}")))?;
    fs::write(
        build_dir.join("src/main.rs"),
        generated_main(name, &definitions),
    )
    .map_err(|error| Failure::Usage(format!("cannot write generated Rust source: {error}")))?;

    run_cargo(&build_dir, &["generate-lockfile", "--offline"])?;
    run_cargo(&build_dir, &["build", "--release", "--locked", "--offline"])?;
    let built = build_dir
        .join("target/release")
        .join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
    let parent = destination.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)
        .map_err(|error| Failure::Usage(format!("cannot create {}: {error}", parent.display())))?;
    let temporary = parent.join(format!(
        ".{}.installing.{}",
        destination
            .file_name()
            .unwrap_or_else(|| std::ffi::OsStr::new(name))
            .to_string_lossy(),
        std::process::id()
    ));
    let mut source = fs::File::open(&built).map_err(|error| {
        Failure::Usage(format!(
            "cannot open built binary {}: {error}",
            built.display()
        ))
    })?;
    let mut staged = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|error| {
            Failure::Usage(format!(
                "cannot reserve installation path {}: {error}",
                temporary.display()
            ))
        })?;
    io::copy(&mut source, &mut staged)
        .map_err(|error| Failure::Usage(format!("cannot stage {}: {error}", built.display())))?;
    drop(source);
    drop(staged);
    fs::set_permissions(
        &temporary,
        fs::metadata(&built)
            .map_err(|error| {
                Failure::Usage(format!("cannot inspect {}: {error}", built.display()))
            })?
            .permissions(),
    )
    .map_err(|error| Failure::Usage(format!("cannot preserve binary permissions: {error}")))?;
    if destination.exists() {
        let backup = parent.join(format!(
            ".{}.replaced.{}",
            destination
                .file_name()
                .unwrap_or_else(|| std::ffi::OsStr::new(name))
                .to_string_lossy(),
            std::process::id()
        ));
        if backup.exists() {
            return Err(Failure::Usage(format!(
                "replacement backup {} already exists",
                backup.display()
            )));
        }
        fs::rename(destination, &backup).map_err(|error| {
            Failure::Usage(format!(
                "cannot stage replacement of {}: {error}",
                destination.display()
            ))
        })?;
        if let Err(error) = fs::rename(&temporary, destination) {
            let _ = fs::rename(&backup, destination);
            return Err(Failure::Usage(format!(
                "cannot install {}: {error}",
                destination.display()
            )));
        }
        fs::remove_file(&backup).map_err(|error| {
            Failure::Usage(format!(
                "installed {}, but cannot remove backup {}: {error}",
                destination.display(),
                backup.display()
            ))
        })?;
    } else {
        fs::rename(&temporary, destination).map_err(|error| {
            Failure::Usage(format!("cannot install {}: {error}", destination.display()))
        })?;
    }
    writeln!(
        out,
        "generated {name} at {}; source retained at {}",
        destination.display(),
        build_dir.display()
    )
    .map_err(io_failure)
}

fn valid_binary_name(name: &str) -> bool {
    name.as_bytes().first().is_some_and(u8::is_ascii_alphabetic)
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn run_cargo(build_dir: &Path, arguments: &[&str]) -> Result<(), Failure> {
    let status = ProcessCommand::new("cargo")
        .args(arguments)
        .current_dir(build_dir)
        .status()
        .map_err(|error| Failure::Usage(format!("cannot run Cargo: {error}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(Failure::Usage(format!(
            "Cargo {} failed with {status}",
            arguments.join(" ")
        )))
    }
}

fn generated_manifest(name: &str, runtime_source: &Path) -> String {
    let path = |member: &str| {
        runtime_source
            .join("crates")
            .join(member)
            .display()
            .to_string()
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
    };
    format!(
        "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\nrust-version = \"1.85\"\n\n[workspace]\n\n[dependencies]\nentity-core = {{ path = \"{}\" }}\nentity-store = {{ path = \"{}\" }}\nentity-shell = {{ path = \"{}\" }}\nentity-yaml = {{ path = \"{}\" }}\nserde = {{ version = \"1\", features = [\"derive\"] }}\nserde_json = {{ version = \"1\", features = [\"arbitrary_precision\"] }}\nserde_yaml_ng = \"0.10\"\nclap = {{ version = \"4.6\", features = [\"derive\"] }}\n",
        path("entity-core"),
        path("entity-store"),
        path("entity-shell"),
        path("entity-yaml")
    )
}

fn generated_main(name: &str, definitions: &[EntityDefinition]) -> String {
    let mut grouped: std::collections::BTreeMap<String, Vec<&EntityDefinition>> =
        std::collections::BTreeMap::new();
    for definition in definitions {
        grouped
            .entry(definition.entity.clone())
            .or_default()
            .push(definition);
    }
    let mut source = String::new();
    source.push_str(
        r#"use std::{fs, path::PathBuf, process::ExitCode};
use clap::{Args, Parser, Subcommand, ValueEnum};
use entity_core::Registry;
use entity_shell::StoredRuntime;
use entity_store::{FileStore, Recording};
use serde::Serialize;
use serde_json::Value;

#[derive(Parser)]
#[command(version, about = "Definition-specific Entity Runtime command")]
struct Cli {
    #[arg(long)]
    store: PathBuf,
    #[command(subcommand)]
    entity: EntityCommand,
}

#[derive(Subcommand)]
enum EntityCommand {
"#,
    );
    for (at, entity) in grouped.keys().enumerate() {
        let _ = writeln!(
            source,
            "    #[command(name = {})]\n    E{at} {{ #[command(subcommand)] command: E{at}Command }},",
            rust_string(entity)
        );
    }
    source.push_str("}\n\n");
    for (at, versions) in grouped.values().enumerate() {
        let mut operations = std::collections::BTreeSet::new();
        for definition in versions {
            operations.extend(definition.operations.keys().cloned());
        }
        let _ = writeln!(source, "#[derive(Subcommand)]\nenum E{at}Command {{");
        source.push_str(
            "    Create(CreateArgs),\n    Get(IdArgs),\n    List(ListArgs),\n    Events(IdArgs),\n",
        );
        for (operation_at, operation) in operations.iter().enumerate() {
            let _ = writeln!(
                source,
                "    #[command(name = {})]\n    O{operation_at}(OperationArgs),",
                rust_string(operation)
            );
        }
        source.push_str("}\n\n");
    }
    source.push_str(
        r#"#[derive(Args)]
struct CreateArgs {
    #[arg(long)] id: String,
    #[arg(long)] version: Option<u32>,
    #[arg(long, default_value = "{}")] fields: String,
    #[command(flatten)] recording: RecordingArgs,
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)] format: OutputFormat,
}

#[derive(Args)]
struct IdArgs {
    #[arg(long)] id: String,
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)] format: OutputFormat,
}

#[derive(Args)]
struct ListArgs {
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)] format: OutputFormat,
}

#[derive(Args)]
struct OperationArgs {
    #[arg(long)] id: String,
    #[arg(long)] expected_revision: u64,
    #[arg(long, default_value = "{}")] arguments: String,
    #[command(flatten)] recording: RecordingArgs,
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)] format: OutputFormat,
}

#[derive(Args)]
struct RecordingArgs {
    #[arg(long)] record_id: String,
    #[arg(long)] recorded_at: String,
    #[arg(long)] actor: Option<String>,
    #[arg(long)] no_actor: bool,
    #[arg(long)] correlation: Option<String>,
    #[arg(long)] causation: Option<String>,
}

#[derive(Clone, Copy, ValueEnum)]
enum OutputFormat { Text, Json, Yaml }

enum Action {
    Create(CreateArgs), Get(IdArgs), List(ListArgs), Events(IdArgs),
    Execute(&'static str, OperationArgs),
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let (entity, action) = match cli.entity {
"#,
    );
    for (at, (entity, versions)) in grouped.iter().enumerate() {
        let mut operations = std::collections::BTreeSet::new();
        for definition in versions {
            operations.extend(definition.operations.keys().cloned());
        }
        let _ = writeln!(
            source,
            "        EntityCommand::E{at} {{ command }} => ({}, match command {{",
            rust_string(entity)
        );
        source.push_str(&format!(
            "            E{at}Command::Create(args) => Action::Create(args),\n            E{at}Command::Get(args) => Action::Get(args),\n            E{at}Command::List(args) => Action::List(args),\n            E{at}Command::Events(args) => Action::Events(args),\n"
        ));
        for (operation_at, operation) in operations.iter().enumerate() {
            let _ = writeln!(
                source,
                "            E{at}Command::O{operation_at}(args) => Action::Execute({}, args),",
                rust_string(operation)
            );
        }
        source.push_str("        }),\n");
    }
    source.push_str(
        r#"    };
    match run(&cli.store, entity, action) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => { eprintln!("refused: {error}"); ExitCode::from(1) }
    }
}

fn run(store_path: &std::path::Path, entity: &str, action: Action) -> Result<(), String> {
    let registry = registry()?;
    let mut store = FileStore::open(store_path);
    let mut runtime = StoredRuntime::new(&registry, &mut store);
    match action {
        Action::Create(args) => {
            let versions: Vec<u32> = registry.versions(entity).map(|item| item.version).collect();
            let version = match (args.version, versions.as_slice()) {
                (Some(version), _) if versions.contains(&version) => version,
                (None, [only]) => *only,
                (Some(version), _) => return Err(format!("version {version} is not one of {versions:?}")),
                (None, _) => return Err(format!("--version is required; choose one of {versions:?}")),
            };
            let fields = value(&args.fields, "--fields")?;
            let commit = runtime.create(entity, version, args.id, fields, &recording(args.recording)?)
                .map_err(|error| error.to_string())?;
            let text = format!("{} {} is {} (revision {})", commit.instance.entity, commit.instance.id, commit.instance.lifecycle_state, commit.instance.revision);
            emit(&commit, args.format, &text)
        }
        Action::Get(args) => {
            let instance = runtime.get(entity, &args.id).map_err(|error| error.to_string())?;
            let text = format!("{} {} is {} (revision {})", instance.entity, instance.id, instance.lifecycle_state, instance.revision);
            emit(&instance, args.format, &text)
        }
        Action::List(args) => {
            let ids = runtime.list(entity).map_err(|error| error.to_string())?;
            emit(&ids, args.format, &ids.join("\n"))
        }
        Action::Events(args) => {
            let events = runtime.events(entity, &args.id).map_err(|error| error.to_string())?;
            let text = events.iter().map(|event| event.event_type.as_str()).collect::<Vec<_>>().join("\n");
            emit(&events, args.format, &text)
        }
        Action::Execute(operation, args) => {
            let arguments = value(&args.arguments, "--arguments")?;
            let commit = runtime.execute(entity, &args.id, args.expected_revision, operation, arguments, &recording(args.recording)?)
                .map_err(|error| error.to_string())?;
            let text = format!("{} {} is {} (revision {})", commit.instance.entity, commit.instance.id, commit.instance.lifecycle_state, commit.instance.revision);
            emit(&commit, args.format, &text)
        }
    }
}

fn registry() -> Result<Registry, String> {
    let mut registry = Registry::new();
    for text in DEFINITIONS {
        let definition = entity_yaml::from_str(text).map_err(|error| error.to_string())?;
        registry.register(definition).map_err(|error| error.to_string())?;
    }
    registry.validate_all().map_err(|error| error.to_string())?;
    Ok(registry)
}

const DEFINITIONS: &[&str] = &[
"#,
    );
    for at in 0..definitions.len() {
        let _ = writeln!(source, "    include_str!(\"../definitions/{at}.yaml\"),");
    }
    source.push_str(
        r#"];

fn recording(args: RecordingArgs) -> Result<Recording, String> {
    if args.actor.is_some() == args.no_actor {
        return Err("pass exactly one of --actor and --no-actor".into());
    }
    Ok(Recording {
        record_id: args.record_id,
        recorded_at: args.recorded_at,
        correlation: args.correlation,
        causation: args.causation,
        actor: args.actor,
    })
}

fn value(source: &str, flag: &str) -> Result<Value, String> {
    let text = if let Some(path) = source.strip_prefix('@') {
        fs::read_to_string(path).map_err(|error| format!("cannot read {path} for {flag}: {error}"))?
    } else {
        source.to_owned()
    };
    serde_json::from_str(&text).or_else(|json| {
        serde_yaml_ng::from_str(&text).map_err(|yaml| format!("{flag} is not JSON or YAML: {yaml} (as JSON: {json})"))
    })
}

fn emit<T: Serialize>(value: &T, format: OutputFormat, text: &str) -> Result<(), String> {
    match format {
        OutputFormat::Text => println!("{text}"),
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(value).map_err(|error| error.to_string())?),
        OutputFormat::Yaml => print!("{}", serde_yaml_ng::to_string(value).map_err(|error| error.to_string())?),
    }
    Ok(())
}
"#,
    );
    let _ = writeln!(source, "// Generated by entity {name}.");
    source
}

fn rust_string(value: &str) -> String {
    serde_json::to_string(value).expect("a string serializes")
}

fn render_skill(stdout: &mut impl Write, path: Option<&Path>, force: bool) -> Result<(), Failure> {
    let document = ENTITY_SKILL.replace("{{VERSION}}", env!("CARGO_PKG_VERSION"));
    let Some(path) = path else {
        return write_all(stdout, &document);
    };
    if path.exists() && !force {
        return Err(Failure::Usage(format!(
            "{} already exists; pass --force to replace that exact file",
            path.display()
        )));
    }
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)
        .map_err(|error| Failure::Usage(format!("cannot create {}: {error}", parent.display())))?;
    let name = path.file_name().ok_or_else(|| {
        Failure::Usage("--out must name a file, not a filesystem root".to_owned())
    })?;
    let temporary = parent.join(format!(
        ".{}.writing.{}",
        name.to_string_lossy(),
        std::process::id()
    ));
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| {
                Failure::Usage(format!("cannot create {}: {error}", temporary.display()))
            })?;
        file.write_all(document.as_bytes()).map_err(io_failure)?;
        file.sync_all().map_err(io_failure)?;
        drop(file);
        match fs::rename(&temporary, path) {
            Ok(()) => Ok(()),
            Err(first) if force && path.exists() => {
                fs::remove_file(path).map_err(|error| {
                    Failure::Usage(format!("cannot replace {}: {error}", path.display()))
                })?;
                fs::rename(&temporary, path).map_err(|error| {
                    Failure::Usage(format!(
                        "cannot install {} after replacement was authorised: {error} (initial rename: {first})",
                        path.display()
                    ))
                })
            }
            Err(error) => Err(Failure::Usage(format!(
                "cannot install {}: {error}",
                path.display()
            ))),
        }
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

/// Commits a decision to the store at `root`, turning a refusal into a usage failure.
///
/// Exit 1 rather than 2: a revision conflict is not a wrong invocation, it is the store answering
/// that somebody else moved first — the same class of "no" as the kernel refusing an operation.
fn commit(root: &Path, decision: &RecordedCommit, expect: Expect) -> Result<(), Failure> {
    FileStore::open(root)
        .commit_recorded(decision, expect)
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
        value = inner;
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
    let mut set_defects = 0usize;
    if invalid == 0 {
        let mut registry = Registry::new();
        for path in paths {
            if let Ok(definition) = load_definition(path) {
                if let Err(errors) = registry.register(definition) {
                    set_defects += errors.len();
                    for error in &errors {
                        writeln!(out, "across the set: {error}").map_err(io_failure)?;
                    }
                }
            }
        }
        if set_defects == 0 {
            if let Err(errors) = registry.validate_all() {
                set_defects += errors.len();
                for error in &errors {
                    writeln!(out, "across the set: {error}").map_err(io_failure)?;
                }
            }
        }
    }

    writeln!(out, "{} file(s), {invalid} invalid", paths.len()).map_err(io_failure)?;
    if invalid > 0 || set_defects > 0 {
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
        if let Some(default) = field.default.as_value() {
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
        GraphFormat::Mermaid => entity_graph::render::mermaid(drawing),
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
                .record
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

fn write_recorded(
    out: &mut impl Write,
    commit: &RecordedCommit,
    format: Format,
) -> Result<(), Failure> {
    match format {
        Format::Json => write_all(out, &to_json(commit)?),
        Format::Yaml => write_all(out, &to_yaml(commit)?),
        Format::Text => {
            let events: Vec<_> = commit
                .envelope
                .record
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
                    "{} {} is {} (revision {}); record {}; events: {events}\n",
                    commit.instance.entity,
                    commit.instance.id,
                    commit.instance.lifecycle_state,
                    commit.instance.revision,
                    commit.envelope.record_id
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
        CoreError::RevisionExhausted {
            entity,
            id,
            revision,
        } => json!({ "entity": entity, "id": id, "revision": revision }),
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
