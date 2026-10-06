---
sidebar_position: 8
title: Use the entity command
description: Value inputs, stateless pipelines, stored commands and their provenance, output formats and exit codes of the entity command.
lede: entity reads definitions and values, asks the kernel, prints the decision or the refusal, and chooses an exit code.
source: "crates/entity-cli/src/main.rs and src/cli.rs, crates/entity-cli/tests/cli.rs, run with entity 0.27.0"
---

# Use the entity command

`entity` is the reference shell around the kernel, written in Rust with clap derive. Its verbs are
the same for every definition set; a [generated CLI](./generate-a-rust-cli.md) is the variant whose
subcommands come from your definitions. The [CLI reference](../reference/cli.md) lists every
option; this page shows how they combine.

## Pass values three ways

`--fields`, `--instance` and `--arguments` each accept inline JSON, `@path` (a JSON or YAML file) or
`-` (standard input). Only one flag per invocation may read standard input. A `Decision` printed by
`create` or `execute` can be passed back as `--instance`; the command takes its instance.

## Pipe decisions without storing them

Without `--store` the command prints a `Decision` and remembers nothing:

```shell-session
$ entity create --definition refund.yaml --id refund-104 \
    --fields '{"order_id":"order-88","amount_cents":2500,"evidence_count":1}' \
  | entity execute --definition refund.yaml --instance - --operation submit --format text
refund refund-104 is submitted (revision 2); events: RefundSubmitted
```

That suits tests and pipelines. The caller is responsible for passing only trusted instances; the
kernel still refuses an instance of another type or one claiming an undeclared state.

## Store with provenance

`create --store DIR` and `execute --store DIR --id ID` commit to a File Store. A stored command
must say who recorded it and when, because the kernel invents neither:

| Flag | Required with `--store` |
|---|---|
| `--record-id ID` | yes: the idempotency identity of this decision |
| `--recorded-at INSTANT` | yes: ISO-8601, your clock |
| `--actor ID` or `--no-actor` | exactly one |
| `--correlation ID`, `--causation ID` | no |
| `--expected-revision N` (`execute` only) | no: defaults to the revision the store holds |

The output is the exact `RecordedCommit` stored. Missing provenance is an invalid invocation:

```shell-session
$ entity create --definition refund.yaml --id refund-200 \
    --fields '{"order_id":"order-90","amount_cents":900,"evidence_count":1}' \
    --store ./refund-store
error: the following required arguments were not provided:
  --recorded-at <RECORDED_AT>
  --record-id <RECORD_ID>
  <--actor <ACTOR>|--no-actor>

Usage: entity create --definition <DEFINITIONS> --id <ID> --recorded-at <RECORDED_AT> --record-id <RECORD_ID> --fields <FIELDS> --store <STORE> <--actor <ACTOR>|--no-actor>

For more information, try '--help'.
```

That error exits 2.

How `--expected-revision` makes a retry recoverable is shown in
[getting started](../getting-started.md#retry-an-accepted-request) and explained in
[storage and replay](../concepts/storage.md#retry-boundaries). `entity list --store DIR --entity
TYPE` prints the stored ids, sorted.

## Choose an output format

`create`, `execute` and `list` take `--format json|yaml|text`; `create` and `execute` default to
JSON and `list` to text. A JSON decision holds the new `instance`, the replay-verifiable `record`
(command, definition, changes, result, events) and `events`, a compatibility view of the record's
events. `inspect` prints text, JSON or YAML; `graph` prints text, Mermaid, DOT, SVG or HTML.

## Read the exit code

| Code | Meaning | Where the result is |
|---|---|---|
| `0` | decided, or the inspection succeeded | standard output |
| `1` | a definition, kernel or store refusal | JSON on standard output, a sentence on standard error |
| `2` | an invalid invocation, or an input that could not be read | standard error |

`validate` reports every file and exits 1 when any is invalid, including a file it could not read
or parse. A kernel refusal carries `kind`; a store refusal is
`{"refused": true, "by": "store", "kind": …, "detail": …}` with the kinds the MCP tools use
(`revision_conflict`, `record_conflict`, `not_found`, …). [Typed refusals](../reference/refusals.md)
lists them all.

## What the command does not do

It contacts no model, publishes no events, performs no side effects, reads no clock, mints no ids,
authenticates no one and chooses no provenance. The default build stores only into a File Store; a
build with the `eventlog-providers` feature adds an Eventlog File store
([CLI reference](../reference/cli.md#what-an-eventlog-providers-build-adds)). SQLite and PostgreSQL
are library integrations.
