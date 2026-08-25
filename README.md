# entity-runtime

> A schema-driven entity runtime. Entity types are declared as data — schema, lifecycle,
> operations, rules, events — and an IO-free, deterministic kernel decides
> `definition + instance + operation + arguments → Decision { instance, events }`.

A Rust library (`entity-core`), a YAML adapter (`entity-yaml`) and a command (`entity`).
[`docs/VISION.md`](docs/VISION.md) is the argument; [`docs/requirements.md`](docs/requirements.md)
is the register every row of which names the test that pins it. The guide — getting started, the
definition language, the command, the library — is [`docs/guide/`](docs/guide/), published at
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
// approved.events is what the shell appends and publishes; the kernel has done nothing with them.
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
| `graph <file>` | the lifecycle as `from --operation--> to` lines or `--format dot` |
| `create --definition <file> --id <id> [--fields <json\|@path\|->]` | a `Decision` |
| `execute --definition <file> --instance <json\|@path\|-> --operation <op> [--arguments …]` | a `Decision`, or a typed refusal |

Exit codes: `0` decided · `1` refused by the kernel, refusal as JSON on stdout · `2` bad invocation.
The command is the reference *shell*: all IO is here, identifiers are yours, and a `Decision` it
prints is accepted back as the next `--instance`.

## What holds

| property | how it is kept |
|---|---|
| the kernel does no IO — no clock, ids, files, network, randomness, async | `crates/entity-core/tests/purity.rs` scans every source line and pins the dependency list to `serde` + `serde_json` |
| same inputs, same `Decision`, same bytes | ordered maps only (`HashMap` is a banned token); a determinism test |
| a refusal changes nothing | instances are taken by `&`; a test executes three refusals and compares before/after |
| the lifecycle state has no setter | only `create` and `execute` assign it |
| a rule cannot read what its scope forbids | refused at registration, not at run time |
| every public item is documented, no `unsafe` | `missing_docs` + `unsafe_code = "forbid"` in `[workspace.lints]`, fatal under the gate's `-D warnings` |
| every requirement is pinned | `scripts/check-requirements.py` fails the gate when a row cites a test that does not exist |

Rules are **two-valued**: a reference that does not resolve reads `false`. That is enough for a
lifecycle and not enough for an evidence gate that must tell *nobody looked* from *it is wrong*;
the three-valued extension is `story:three-valued-conditions` and is the first thing
`engineering-protocols` needs before it can be driven by this.

## Where it sits

| repo | relationship |
|---|---|
| [engineering-protocols](https://github.com/beyond10x/engineering-protocols) | the intended first adopter: its artifact model — kinds, lifecycles, moves, events — expressed as definitions this kernel executes. [`docs/design/engineering-protocols-adoption-v0.1.md`](docs/design/engineering-protocols-adoption-v0.1.md) |
| [eventlog](https://github.com/beyond10x/eventlog) | the append-only side: this decides, that stores |
| [atlas](https://github.com/beyond10x/atlas) | the map of the `beyond10x` estate this sits in |

## Install

Prebuilt `entity` binaries for Linux, macOS and Windows are attached to every release, with a
`SHA256SUMS` file: <https://github.com/beyond10x/entity-runtime/releases>. Or
`cargo install --git https://github.com/beyond10x/entity-runtime entity-cli`.

## Build

Requires Rust 1.85+, [go-task](https://taskfile.dev) and `python3`; `task plan-check` additionally
needs the `protocol` CLI from `engineering-protocols`.

```console
task check     # fmt · clippy -D warnings · test · doc -D warnings · examples · requirements · plan
```

Run that; it is the measurement. Counts of suites and tests are read from its output, not from
this file.

## Where everything is

| | |
|---|---|
| [`crates/entity-core/`](crates/entity-core/) | the kernel |
| [`crates/entity-yaml/`](crates/entity-yaml/) | `&str → EntityDefinition` |
| [`crates/entity-cli/`](crates/entity-cli/) | the `entity` command |
| [`examples/`](examples/) | definitions the gate validates |
| [`docs/guide/`](docs/guide/) | getting started · the definition language · the command · the library |
| [`docs/VISION.md`](docs/VISION.md) | why |
| [`docs/requirements.md`](docs/requirements.md) | the register, every row pinned |
| [`docs/design/`](docs/design/) | the kernel design (normative) and the adoption design (proposed) |
| [`AGENTS.md`](AGENTS.md) | the working agreement: invariants, each with the check that enforces it |
| `.engineering/` | this repository's planning store, driven through `protocol artifact` |
| [`website/`](website/) | the Docusaurus site that renders `docs/` — <https://beyond10x.github.io/entity-runtime/> |
| [`CHANGELOG.md`](CHANGELOG.md) | what a user of the runtime sees change |

## Licence

Apache-2.0. See [LICENSE](LICENSE).
