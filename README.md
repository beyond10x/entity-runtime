# entity-runtime

> A schema-driven entity runtime. Entity types are declared as data — schema, lifecycle,
> operations, rules, events — and an IO-free, deterministic kernel decides
> `definition + instance + operation + arguments → Decision { instance, record, events }`.

A deterministic Rust kernel (`entity-core`), narrow storage and projection libraries, a YAML
adapter, and the `entity` command.
[`docs/VISION.md`](docs/VISION.md) is the internal argument; [`docs/requirements.md`](docs/requirements.md)
is the register every row of which names the test that pins it. Human-facing product documentation
is authored separately under [`website/docs/`](website/docs/) and published at
<https://beyond10x.github.io/entity-runtime/>.

## A definition

```yaml
entity: order
version: 1

schema:
  fields:
    customer_id:      { type: string, required: true }
    total_cents:      { type: integer, required: true, min: 0 }
    rejection_reason: { type: string }

lifecycle:
  initial: draft
  states: [draft, submitted, approved, rejected]

invariants:
  - name: rejected_requires_reason
    assert: { any: [ { ne: [$state, rejected] }, { exists: $fields.rejection_reason } ] }
    message: rejected orders must have a rejection reason

operations:
  submit:
    arguments: { fields: { actor: { type: string, required: true } } }
    transitions: [ { from: draft, to: submitted } ]
    emits: [ { type: OrderSubmitted, payload: { order_id: $id, actor: $args.actor } } ]

  approve:
    transitions: [ { from: submitted, to: approved } ]
    preconditions:
      - name: positive_total
        assert: { gt: [$fields.total_cents, 0] }
        message: zero-value orders cannot be approved

  reject:
    arguments: { fields: { reason: { type: string, required: true } } }
    transitions: [ { from: submitted, to: rejected } ]
    set: { rejection_reason: $args.reason }
    emits: [ { type: OrderRejected, payload: { reason: $fields.rejection_reason } } ]
```

The full example, with every field kind and both rule kinds, is
[`examples/order.yaml`](examples/order.yaml).

## From Rust

```rust
use entity_core::{Registry, Runtime};
use serde_json::json;

let definition = entity_yaml::from_str(&yaml)?;      // &str in, definition out — no file IO here
let mut registry = Registry::new();
registry.register(definition)?;                       // validated now, or refused with a typed reason
let runtime = Runtime::new(&registry);

let created   = runtime.create("order", 1, "ord-1", json!({ "customer_id": "c-1", "total_cents": 2599 }))?;
let submitted = runtime.execute(&created.instance, "submit", json!({ "actor": "alice" }))?;
let approved  = runtime.execute(&submitted.instance, "approve", json!({}))?;

assert_eq!(approved.instance.lifecycle_state, "approved");
assert_eq!(approved.instance.revision, 3);
// approved.record is the normalized, replay-verifiable evidence a shell envelopes and stores.
```

The kernel never mutates `created.instance`; each call returns a new one. A refusal is a
`CoreError` variant — `InvalidTransition`, `PreconditionFailed`, `InvariantViolation`,
`Validation(Vec<_>)`, … — and nothing happened.

## From the command line

```console
$ entity validate examples/order.yaml
examples/order.yaml: valid (order v1)
1 file(s), 0 invalid

$ entity graph examples/order.yaml
order v1: initial draft
approved --fulfill--> fulfilled
draft --cancel--> cancelled
draft --submit--> submitted
submitted --approve--> approved
submitted --cancel--> cancelled
submitted --reject--> rejected

$ entity create --definition examples/order.yaml --id ord-1 \
    --fields '{"customer_id":"c-1","total_cents":0}' > draft.json

$ entity execute --definition examples/order.yaml --instance @draft.json \
    --operation submit --arguments '{"actor":"alice"}' --format text
order ord-1 is submitted (revision 2); events: OrderSubmitted

$ entity execute --definition examples/order.yaml --instance @draft.json \
    --operation submit --arguments '{"actor":"alice"}' \
  | entity execute --definition examples/order.yaml --instance - \
    --operation approve --arguments '{"actor":"bob"}'
{
  "kind": "precondition_failed",
  "message": "precondition 'positive_total' failed for operation 'approve': zero-value orders cannot be approved",
  "operation": "approve",
  "reason": "zero-value orders cannot be approved",
  "rule": "positive_total"
}
$ echo $?
1
```

| verb | does |
|---|---|
| `validate <file>...` | parse and register; exit 1 naming every invalid file |
| `inspect <file>` | fields, states, rules, operations — `--format text\|json\|yaml` |
| `graph <file>` | lifecycle or typed references as text, Mermaid, DOT, SVG, or HTML |
| `create --definition <file> --id <id> [--fields <json\|@path\|->]` | a `Decision` |
| `execute --definition <file> --instance <json\|@path\|-> --operation <op> [--arguments …]` | a `Decision`, or a typed refusal |
| `generate docs --definition <file> --out <dir>` | entity pages, graphs, OpenAPI, and AsyncAPI |
| `generate rust-cli --definition <file> --name <name> --out <path>` | a retained, definition-specific Rust command |
| `mcp --definition <file> --store <dir>` | schema-derived stored entity tools over stdio |
| `store migrate-file --from OLD --to NEW [--dry-run]` | out-of-place File Store v2 migration |
| `skill [--out PATH] [--force]` | the version-stamped Agent Skill for this CLI |

Exit codes: `0` decided · `1` refused by the kernel, refusal as JSON on stdout · `2` bad invocation.
The command is the reference *shell*: all IO is here, identifiers are yours, and a `Decision` it
prints is accepted back as the next `--instance`.

## What holds

| property | how it is kept |
|---|---|
| the kernel does no IO — no clock, ids, files, network, randomness, async | `crates/entity-core/tests/purity.rs` strips comments and strings, expands every `use` path and matches whole words, so a grouped import or an alias is caught too; it is checked against fourteen plantings and eight lookalikes, and pins the dependency list to `serde` + `serde_json` |
| same inputs, same `Decision`, same bytes | ordered maps only (`HashMap` is a banned token); a determinism test |
| a refusal changes nothing | instances are taken by `&`; a test executes three refusals and compares before/after |
| the kernel never writes a state except through an operation | only `create` and `execute` assign one, and an instance claiming a state the definition does not declare is refused |
| a rule or template cannot read what its scope forbids | refused at registration, whole path checked — `$fields.address.countri` never reaches run time |
| a key nobody reads cannot silence a rule | every definition struct denies unknown fields; a condition carries exactly one operator |
| every public item is documented, no `unsafe` | `missing_docs` + `unsafe_code = "forbid"` in `[workspace.lints]`, fatal under the gate's `-D warnings` |
| a recorded store write is whole and attributable | `RecordedCommit` binds the complete result and replay evidence to caller-supplied provenance; providers preserve it atomically and idempotently |
| a store that could not be reached is never *absent* | `StoreError::Unreachable` is its own variant and survives the wire. *Absent* is a fact about the data; silence is a fact about the network, and a provider that confuses them is how a synchronisation deletes something |
| a hybrid's behaviour is four words somebody typed | `Policy::new` takes authority, read path, unreachable behaviour and divergence behaviour, and has **no `Default`** — a default policy is one nobody chose applied to somebody's data |
| replay reaches no state `execute` would refuse | complete decision replay reruns the normalized command and compares the definition, result, changes and events; legacy event folding remains an explicit migration boundary |
| every requirement is pinned | the requirements gate fails when a row cites a test that does not exist |

Numbers compare numerically everywhere, so `eq: [$fields.total, 100]` holds for `100.0` too. A key
the model does not declare, a condition with two operators, a constraint on the wrong kind, a
template or a reference path that could never resolve — each is refused when the document is read,
because a definition that says less than its author meant is the failure this format exists to
prevent.

Rules are **three-valued**: a comparison against a missing value is `unknown`, distinct from a value
that contradicts the rule. Preconditions and invariants report unobservable outcomes with every
missing path, while `exists` remains the explicit two-valued presence check. The public
[definition reference](https://beyond10x.github.io/entity-runtime/docs/guide/definitions) explains
how to choose between an ordinary failure and a request for more evidence.

## Where it sits

| repo | relationship |
|---|---|
| [engineering-protocols](https://github.com/beyond10x/engineering-protocols) | the first adopter, and no longer only intended: it takes `entity-core` as a dependency, its eight lifecycles are expressed as definitions this kernel executes, and its `aep-backend-sqlite` is an adapter over `entity-sqlite`. The arrow points one way — nothing of theirs appears here. [`docs/design/engineering-protocols-adoption-v0.1.md`](docs/design/engineering-protocols-adoption-v0.1.md) |
| [eventlog](https://github.com/beyond10x/eventlog) | the append-only side: this decides, that stores |
| [atlas](https://github.com/beyond10x/atlas) | the map of the `beyond10x` estate this sits in |

## Install

Prebuilt `entity` binaries for Linux, macOS and Windows are attached to every release, with a
`SHA256SUMS` file: <https://github.com/beyond10x/entity-runtime/releases>. Or
`cargo install --git https://github.com/beyond10x/entity-runtime entity-cli`.

## Build

Requires Rust 1.85+ and [go-task](https://taskfile.dev); `task plan-check` additionally
needs the `protocol` CLI from `engineering-protocols`.

```console
task check     # fmt · clippy -D warnings · test · doc -D warnings · examples · requirements · plan
```

Run that; it is the measurement. Counts of suites and tests are read from its output, not from
this file.

## Where everything is

| | |
|---|---|
| [`crates/entity-core/`](crates/entity-core/) | the kernel — decides, stores nothing |
| [`crates/entity-yaml/`](crates/entity-yaml/) | `&str → EntityDefinition` |
| [`crates/entity-store/`](crates/entity-store/) | the provider traits, memory and file stores, the event envelope, projections, and one conformance suite that travels to each |
| [`crates/entity-sqlite/`](crates/entity-sqlite/) | one `BEGIN`, both writes, one `COMMIT` — the promise a file store cannot make |
| [`crates/entity-remote/`](crates/entity-remote/) | a store somewhere else, and a hybrid over a local one whose policy is four required words with no default |
| [`crates/entity-graph/`](crates/entity-graph/) | a definition, drawn |
| [`crates/entity-surface/`](crates/entity-surface/) | one IO-free projection into schemas, contracts, and entity documentation |
| [`crates/entity-shell/`](crates/entity-shell/) | provider-backed operations shared by generated commands and MCP |
| [`crates/entity-mcp/`](crates/entity-mcp/) | synchronous schema-derived MCP tools over caller-provided IO |
| [`crates/entity-cli/`](crates/entity-cli/) | the `entity` command |
| [`examples/`](examples/) | definitions the gate validates |
| [`website/docs/`](website/docs/) | human-facing product documentation for agent builders and Rust adopters |
| [`docs/guide/`](docs/guide/) | repository-internal historical guide material; not published by the website |
| [`docs/VISION.md`](docs/VISION.md) | why |
| [`docs/requirements.md`](docs/requirements.md) | the register, every row pinned |
| [`docs/design/`](docs/design/) | the kernel design (normative) and the adoption design (proposed) |
| [`AGENTS.md`](AGENTS.md) | the working agreement: invariants, each with the check that enforces it |
| `.engineering/` | this repository's planning store, driven through `protocol artifact` |
| [`website/`](website/) | the standalone Docusaurus product site — <https://beyond10x.github.io/entity-runtime/> |
| [`CHANGELOG.md`](CHANGELOG.md) | what a user of the runtime sees change |

## Licence

Apache-2.0. See [LICENSE](LICENSE).
