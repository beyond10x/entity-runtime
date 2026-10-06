//! The `entity` command line: every command, option and value the binary parses.
//!
//! The binary (`src/main.rs`) parses [`Cli`] and does the work; this module only declares what it
//! accepts, so that tooling can read the same definition — the documentation site's CLI reference
//! is generated from [`Cli`] by `entity-runtime-docs`. Nothing here performs IO.

use clap::{ArgGroup, Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

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

/// The `entity` command line.
#[derive(Parser)]
#[command(name = "entity", version, about = ABOUT, long_about = LONG_ABOUT)]
pub struct Cli {
    /// The command to run.
    #[command(subcommand)]
    pub command: Command,
}

/// Every top-level command of `entity`.
#[derive(Subcommand)]
pub enum Command {
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
        /// How to print what the definition declares.
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
        /// How to draw it.
        #[arg(long, value_enum, default_value_t = GraphFormat::Text)]
        format: GraphFormat,
    },
    /// Create an instance: definition + id + fields -> Decision.
    Create {
        /// The definition files to register.
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
        /// Eventlog File selection JSON; selects the recorded facade for `--store`.
        #[cfg(feature = "eventlog-providers")]
        #[arg(long, requires = "store")]
        eventlog_config: Option<PathBuf>,
        /// Provenance required when the decision is stored.
        #[command(flatten)]
        recording: RecordingArgs,
        /// How to print the decision.
        #[arg(long, value_enum, default_value_t = Format::Json)]
        format: Format,
    },
    /// Execute an operation: definition + instance + operation + arguments -> Decision.
    Execute {
        /// The definition files to register.
        #[command(flatten)]
        definition: DefinitionArg,
        /// The current instance (or a Decision holding one), as inline JSON, `@<path>` or `-`.
        ///
        /// Not needed when `--store` and `--id` say where to find it, and refused beside them: two
        /// sources for one instance would leave the caller guessing which one decided.
        #[arg(long, required_unless_present = "store", conflicts_with = "store")]
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
        /// Eventlog File selection JSON; selects the recorded facade for `--store`.
        #[cfg(feature = "eventlog-providers")]
        #[arg(long, requires = "store")]
        eventlog_config: Option<PathBuf>,
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
        /// The revision of the stored instance this request was decided on.
        ///
        /// Defaults to whatever the store holds when the command runs, which is what a sequential
        /// local command wants. Pass it when retrying: an already-accepted `--record-id` is
        /// returned as the original record only when the retry names the same revision the first
        /// request was decided on, so a lost response can be recovered after the subject moved.
        #[arg(long, requires = "store")]
        expected_revision: Option<u64>,
        /// Provenance required when the decision is stored.
        #[command(flatten)]
        recording: RecordingArgs,
        /// How to print the decision.
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
        /// Eventlog File selection JSON; selects the recorded facade for `--store`.
        #[cfg(feature = "eventlog-providers")]
        #[arg(long)]
        eventlog_config: Option<PathBuf>,
        /// Which entity type to list.
        #[arg(long)]
        entity: String,
        /// How to print the identities.
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
        /// The definition files to register.
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

/// The surfaces `entity generate` writes.
#[derive(Subcommand)]
pub enum GenerateCommand {
    /// Write standalone HTML/Markdown docs plus OpenAPI and AsyncAPI contracts.
    Docs {
        /// The definition files to register.
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
        /// The definition files to register.
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

/// The operations `entity store` performs on a persistent store.
#[derive(Subcommand)]
pub enum StoreCommand {
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
    /// Prepare a new recorded Eventlog File authority and print its exact reopen authority.
    #[cfg(feature = "eventlog-providers")]
    ProvisionEventlogFile {
        /// Destination Eventlog File root.
        #[arg(long)]
        root: PathBuf,
        /// Logical Entity Runtime scope bound to the physical provider.
        #[arg(long)]
        scope: String,
        /// Eventlog tenant identity.
        #[arg(long)]
        tenant: String,
        /// Optional exact pre-existing provider generation.
        #[arg(long)]
        expected_stream_identity: Option<String>,
        /// Maximum events in one authoritative capture.
        #[arg(long)]
        max_events: u64,
        /// Maximum blobs in one authoritative capture.
        #[arg(long)]
        max_blobs: u64,
        /// Maximum projection rows in one authoritative capture.
        #[arg(long)]
        max_projection_rows: u64,
        /// Maximum total payload bytes in one authoritative capture.
        #[arg(long)]
        max_payload_bytes: u64,
        /// Bounded synchronous bridge queue capacity.
        #[arg(long)]
        queue_capacity: u16,
        /// Caller-owned operational facts for the binding write.
        #[command(flatten)]
        context: Box<EventlogContextArgs>,
    },
}

/// The caller-owned operational facts an Eventlog binding write records.
#[cfg(feature = "eventlog-providers")]
#[derive(Args)]
pub struct EventlogContextArgs {
    /// Opaque principal for whom the operation runs.
    #[arg(long)]
    pub subject: String,
    /// Opaque agent or service issuing the operation.
    #[arg(long)]
    pub actor: String,
    /// Caller-stable request identity.
    #[arg(long)]
    pub request_id: String,
    /// Caller-stable trace identity.
    #[arg(long)]
    pub trace_id: String,
    /// Optional causing event identity.
    #[arg(long)]
    pub causation_id: Option<String>,
    /// Bounded automation depth.
    #[arg(long, default_value_t = 0)]
    pub causation_depth: u32,
    /// Caller-understood RFC 3339 occurrence time.
    #[arg(long)]
    pub occurred_at: String,
}

/// The definition files a command registers.
#[derive(Args)]
pub struct DefinitionArg {
    /// The definition file, YAML. Repeat to register several types or versions at once.
    #[arg(long = "definition", required = true)]
    pub definitions: Vec<PathBuf>,
}

/// The provenance a stored or recorded decision carries. The binary checks that it is complete.
#[derive(Args)]
#[command(group(
    ArgGroup::new("actor_choice")
        .args(["actor", "no_actor"])
        .multiple(false)
))]
pub struct RecordingArgs {
    /// Caller-supplied idempotency identity for this complete decision.
    #[arg(long, requires = "recorded_at", requires = "actor_choice")]
    pub record_id: Option<String>,
    /// When this was recorded, ISO-8601. Your clock: the kernel has none.
    #[arg(long, requires = "record_id", requires = "actor_choice")]
    pub recorded_at: Option<String>,
    /// The wider flow, when there is one.
    #[arg(long, requires = "record_id")]
    pub correlation: Option<String>,
    /// What immediately led to this record, when there is one.
    #[arg(long, requires = "record_id")]
    pub causation: Option<String>,
    /// Who asked.
    #[arg(long, requires = "record_id")]
    pub actor: Option<String>,
    /// Record explicitly that no actor caused this decision.
    #[arg(long, requires = "record_id")]
    pub no_actor: bool,
}

/// How `inspect`, `create`, `execute` and `list` print their result.
#[derive(Clone, Copy, ValueEnum)]
pub enum Format {
    /// A short human-readable rendering.
    Text,
    /// JSON, one document.
    Json,
    /// YAML, one document.
    Yaml,
}

/// How `graph` draws a lifecycle or a reference graph.
#[derive(Clone, Copy, ValueEnum)]
pub enum GraphFormat {
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
