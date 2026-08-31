# Generated surfaces v0.1

This design pins the projections built from a validated definition set. It covers R-115 through
R-118. These surfaces do not add execution semantics: the registry and kernel remain the authority,
and every stored write still crosses the provider contract as one recorded commit.

## 1. Graph projection (R-115)

`entity-graph` has one ordered `Graph` model for lifecycle and typed-reference views. Text,
Mermaid, DOT, SVG and HTML render that model; a renderer does not parse definitions independently.
Lifecycle Mermaid is `stateDiagram-v2`, reference Mermaid is `flowchart LR`, and untrusted names
are labels on opaque node identifiers. Renderers escape for their own grammar and are deterministic.

## 2. Contracts and documentation (R-116)

`entity-surface` is IO-free. Given ordered validated definitions it returns values or a stable map
of safe relative paths. The same field-schema projection feeds:

- OpenAPI 3.2 paths for create, get, list, events and each declared operation;
- AsyncAPI 3.1 channels for declared domain event types, with payload schemas inferred from literal
  templates and referenced field/argument schemas;
- an index and one HTML/Markdown page per logical entity, including every version, property,
  lifecycle, operation, argument, rule message, emitted event and projection.

The contracts describe adopter-owned facades and publishers. They do not claim that this repository
opens an HTTP listener, owns a broker or authenticates an actor.

The CLI stages a complete documentation bundle beside its destination and publishes by rename. An
existing directory is refused. `--force` may replace only a directory carrying the generator's
format marker; it never authorises deletion of an arbitrary directory.

## 3. MCP tools (R-117)

`entity mcp` is a synchronous stdio JSON-RPC shell over a validated registry and File Store v2.
For each entity it exposes `<entity>.create`, `.get`, `.list`, `.events`, and one tool for each
declared operation. Input schemas come from the same projection as the generated contracts. Names
outside the portable ASCII component set, overlong names, and operations colliding with built-ins
are refused before the server starts.

The server implements stateless discovery for protocol `2026-07-28` and initialization-era
`2025-11-25`. Stdout carries protocol messages only. It exposes no network listener, prompts,
resources, sampling or model-controlled definition/store selection.

Write tools require caller-supplied recording provenance. Operation tools also require the revision
the caller observed. The shell loads that exact stored subject, compares the revision before kernel
evaluation, and commits with the same revision expectation. A stale call is a typed tool error and
does not write state or events. Actor metadata is evidence, not authentication; a trusted MCP host
must derive or validate it.

## 4. Generated Rust command (R-118)

`entity generate rust-cli` validates the complete definition set, embeds its exact YAML bytes, and
generates a Clap-derived Rust command. Entity names are subcommands; `create`, `get`, `list`,
`events`, and every non-colliding declared operation are direct child commands. Stored writes use
the shared provider-backed shell and therefore require provenance and expected revision where
applicable.

Generation accepts only a matching entity-runtime source checkout, produces and retains an ordinary
Cargo crate, resolves its lockfile and builds with `--locked --offline`, then installs the binary for
the host platform. It fetches no code and does not pretend to cross-compile. The generated source
directory must be absent unless `--force` names one carrying the generator marker; the installed
binary likewise requires explicit replacement authority.

## 5. Shared boundary

`entity-shell` centralizes provider-backed create/get/list/events/execute behavior used by MCP and
generated commands. It owns no IO or transport. The caller chooses a provider and supplies every
ambient fact; the shell preserves the sequence “load, compare observed revision, decide, construct
record, commit with expectation.” The kernel remains IO-free and a refusal changes nothing.
