---
sidebar_position: 8
title: CLI reference
description: Commands, value inputs, stored-command provenance, output formats, agent skill rendering, and exit codes.
---

# CLI reference

`entity` is the authored, Clap-derived reference shell around the deterministic kernel.
Its top-level verbs are not generated from an ESS specification; [domain CLIs](./generated-cli)
derive their entity and operation subcommands from mounted entity definitions. It reads files and standard input,
uses the local File Store when requested, prints structured results, and selects an exit code.

## Commands

| Command | Use it to |
|---|---|
| `validate` | parse and validate every supplied definition, reporting every file |
| `inspect` | show the fields, states, rules, and operations in one definition |
| `graph` | render a lifecycle or the typed references between definitions |
| `create` | produce a creation `Decision`, optionally recording it in a File Store |
| `execute` | run a named operation against an input instance or stored identity |
| `list` | list the identities stored under one entity type |
| `generate docs` | build standalone HTML/Markdown entity docs plus OpenAPI and AsyncAPI |
| `generate rust-cli` | compile a definition-specific Rust command for the current platform |
| `mcp` | mount stored entity operations as MCP tools over stdio |
| `store migrate-file` | migrate a pre-0.15 File Store out of place |
| `skill` | render the installed version's Agent Skill |

`inspect` supports text, JSON, and YAML. `graph` supports text, Mermaid, Graphviz DOT, standalone
SVG, and a self-contained HTML document; add `--references` to draw typed relationships instead of
one lifecycle. Mermaid lifecycle output is `stateDiagram-v2`; reference output is `flowchart LR`.

Repeat `--definition` to register a complete related set. When several definitions make the target
ambiguous, supply `--entity`.

Run `entity COMMAND --help` for the exact flags supported by the installed version.

## Value inputs

`--fields`, `--instance`, and `--arguments` accept:

| Form | Example |
|---|---|
| inline JSON | `--fields '{"amount_cents":12500}'` |
| file | `--instance @submitted.json` (JSON or YAML) |
| standard input | `--instance -` (JSON or YAML) |

At most one flag may read `-` in an invocation. A complete `Decision` printed by `create` or
`execute` may be supplied as the next `--instance`; the command extracts its instance.

## Stateless execution

Without `--store`, the command prints a `Decision` and remembers nothing:

```bash
entity create --definition refund.yaml --id refund-104 \
  --fields '{"order_id":"order-88","amount_cents":2500,"evidence_count":1}' \
| entity execute --definition refund.yaml --instance - --operation submit
```

This is useful for tests, pipelines, and understanding the kernel boundary. The caller is
responsible for accepting only trusted instances.

## Stored commands

`create --store` and `execute --store --id` use File Store v2. They require complete provenance:

- `--record-id ID`;
- `--recorded-at INSTANT`;
- exactly one of `--actor ID` and `--no-actor`;
- optional `--correlation ID` and `--causation ID`.

```bash
entity execute --definition refund.yaml --store ./refund-store \
  --id refund-104 --operation approve \
  --arguments '{"actor_role":"human","reason":"supervisor approved"}' \
  --record-id request-104-approved \
  --recorded-at 2026-08-31T10:04:00Z \
  --actor supervisor-7
```

The output is the exact `RecordedCommit` persisted. An incomplete recording envelope is an invalid
invocation. At the provider boundary, an identical recorded commit is idempotent and a reused ID
with different bytes is refused.

Generic `entity execute --store` loads current state and evaluates again on every invocation. It
has no `--expected-revision` flag and does not recover a prior accepted operation by record ID.
The [generated CLI](./generated-cli) and [MCP tools](./mcp) require the caller's observed revision
and use the shared stored runtime's exact-retry path. See [storage](./storage#retry-boundaries)
before turning a local command into a retrying service.

## Output formats

`create` and `execute` default to JSON. `--format yaml` contains the same data. `--format text`
prints a one-line summary such as:

```text
refund refund-104 is approved (revision 3); events: RefundApproved
```

JSON decisions include:

- the resulting `instance`;
- the replay-verifiable `record` containing command, definition, changes, result, and events; and
- `events`, retained as a compatibility view of the record's events.

## Exit codes

| Code | Meaning | Machine-readable result |
|---|---|---|
| `0` | accepted decision or successful inspection | stdout |
| `1` | definition, kernel, or store refusal | structured stdout; readable stderr summary |
| `2` | invalid invocation or unreadable singular input | stderr |

`validate` always reports each requested file and exits `1` when any is invalid, including files it
could not read or parse. Kernel refusals carry `kind`. File Store refusals carry
`{ "refused": true, "by": "store", "detail": "..." }`. Programs should match those fields, never
stderr sentences.

## Render the Agent Skill

```bash
entity skill
entity skill --out .agents/skills/entity/SKILL.md
entity skill --out .agents/skills/entity/SKILL.md --force
```

Stdout and file output are byte-identical and stamped with the installed version. Parent
directories are created. Existing output is refused unless `--force` names that replacement.

## Generate public surfaces

```bash
entity generate docs --definition refund.yaml --out ./refund-reference
entity generate rust-cli --definition refund.yaml --name refundctl \
  --out ./bin/refundctl --runtime-source /src/entity-runtime
entity mcp --definition refund.yaml --store ./refund-store
```

The generated documentation directory is self-contained. Rust CLI generation retains its source
under `build/entity-runtime/NAME` by default and invokes Cargo with `--locked --offline`. Follow the matching-source and Cargo-output
prerequisites in the [generation guide](./generated-cli).

## What the command does not do

The CLI does not contact a model, use SQLite or PostgreSQL, publish events, perform domain side
effects, read a clock, mint IDs, authenticate actors, or choose provenance. Those are shell and
deployment responsibilities.
