# The entity command

`entity` is the reference **shell** around the kernel: it reads files and standard input, calls
`entity-core`, prints the result and chooses an exit code. All IO lives here; identifiers come
from you; nothing reads a clock.

```console
$ entity --help
```

## Verbs

| verb | does |
|---|---|
| `validate <file>...` | parse and register each definition; print `valid`/`invalid` per file; exit 1 if any is invalid |
| `inspect <file> [--format text\|json\|yaml]` | fields, states, rules and operations of one definition |
| `graph <file> [--format text\|dot]` | the lifecycle as `from --operation--> to` lines, or Graphviz DOT |
| `create --definition <file> --id <id> [--fields <value>] [--format …]` | a Decision for a new instance |
| `execute --definition <file> --instance <value> --operation <op> [--arguments <value>] [--format …]` | a Decision, or a typed refusal |

`--definition` may be repeated for `execute` (several types or versions at once). `create` takes
exactly one, so the type to create is unambiguous.

## Values

`--fields`, `--instance` and `--arguments` accept three forms:

| form | example |
|---|---|
| inline JSON | `--fields '{"title": "Login fails"}'` |
| `@<path>` | `--instance @open.json` (JSON or YAML) |
| `-` | `--instance -` reads standard input (JSON or YAML) |

An `--instance` may be an `EntityInstance` **or a whole Decision** as printed by `create`/`execute`;
the command takes the instance out of a Decision. That is what makes this a pipeline:

```console
$ entity create --definition examples/order.yaml --id ord-1 \
    --fields '{"customer_id":"c-1","total_cents":2599}' \
  | entity execute --definition examples/order.yaml --instance - \
    --operation submit --arguments '{"actor":"alice"}' \
  | entity execute --definition examples/order.yaml --instance - \
    --operation approve --arguments '{"actor":"bob"}' --format text
order ord-1 is approved (revision 3); events: OrderApproved
```

## Output

`--format json` (default for `create`/`execute`) prints the Decision:

```json
{
  "instance": {
    "entity": "order",
    "version": 1,
    "id": "ord-1",
    "lifecycle_state": "submitted",
    "revision": 2,
    "fields": { "customer_id": "c-1", "priority": "normal", "tags": [], "total_cents": 2599 }
  },
  "events": [
    { "entity": "order", "version": 1, "id": "ord-1", "revision": 2,
      "type": "OrderSubmitted",
      "payload": { "actor": "alice", "from": "draft", "order_id": "ord-1", "to": "submitted" } }
  ]
}
```

`--format yaml` prints the same as YAML; `--format text` prints one line:
`order ord-1 is submitted (revision 2); events: OrderSubmitted`.

## Exit codes

| code | meaning | where the reason goes |
|---|---|---|
| `0` | the kernel decided | the Decision on stdout |
| `1` | the kernel **refused** | the typed refusal as JSON on stdout; one sentence on stderr |
| `2` | the invocation was wrong — unreadable file, unparsable YAML/JSON, ambiguous `create` | stderr |

A refusal always carries `kind` and `message`, plus the fields of its kind:

| kind | fields |
|---|---|
| `validation` | `errors: [{path, message}]` — every failure, not the first |
| `invalid_transition` | `operation`, `state` |
| `precondition_failed` | `operation`, `rule`, `reason` |
| `invariant_violation` | `rule`, `reason` |
| `operation_not_found` | `operation` |
| `entity_not_registered` | `entity`, `version` |
| `entity_mismatch` | `expected: {entity, version}`, `actual: {entity, version}` |
| `template` | `expression`, `reason` |
| `definition` | the definition is malformed; `validate` prints the specific defect |

```console
$ entity create --definition examples/order.yaml --id ord-1 \
    --fields '{"total_cents": -5, "priority": "urgent"}'
{
  "errors": [
    { "message": "required field is missing", "path": "fields.customer_id" },
    { "message": "'urgent' is not one of [low, normal, high]", "path": "fields.priority" },
    { "message": "value -5 is below minimum 0", "path": "fields.total_cents" }
  ],
  "kind": "validation",
  "message": "validation failed; fields.customer_id: required field is missing; fields.priority: 'urgent' is not one of [low, normal, high]; fields.total_cents: value -5 is below minimum 0"
}
$ echo $?
1
```

## What the command does not do

It holds no state between invocations, so `execute` needs the instance every time. It does not
persist the Decision, append the events or publish them — a shell that does is the next thing to
build on the [library](library.md). It has no clock: an operation that needs one declares an
argument and you pass the value.
