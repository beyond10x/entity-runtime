# Getting started

Ten minutes from nothing to a refused operation with a reason. Everything below was run against
the binary this repository builds; the transcripts are the command's real output.

## Install

Every release carries the `entity` command prebuilt for Linux (x86_64, aarch64), macOS (x86_64,
arm64) and Windows (x86_64), with a `SHA256SUMS` file:
<https://github.com/beyond10x/entity-runtime/releases>. Unpack the archive for your platform and
put `entity` on your `PATH`.

Or build it. Rust 1.85 or newer; either in a checkout:

```console
$ git clone https://github.com/beyond10x/entity-runtime
$ cd entity-runtime
$ cargo build --release -p entity-cli
$ ./target/release/entity --version
```

or install the command straight from the repository:

```console
$ cargo install --git https://github.com/beyond10x/entity-runtime entity-cli
```

To embed the kernel in your own program, add the library crate — see
[The library](library.md).

## Write a definition

A definition is a YAML document: the fields an instance has, the states it may be in, and the
operations that move it between them. Save this as `ticket.yaml`:

```yaml
entity: ticket
version: 1

schema:
  fields:
    title:      { type: string, required: true, min_length: 1 }
    points:     { type: integer, min: 0, max: 100 }
    resolution: { type: string }

lifecycle:
  initial: open
  states: [open, in_progress, closed]

invariants:
  - name: closed_requires_resolution
    assert: { any: [ { ne: [$state, closed] }, { exists: $fields.resolution } ] }
    message: a closed ticket states how it was resolved

operations:
  start:
    arguments: { fields: { assignee: { type: string, required: true } } }
    transitions: [ { from: open, to: in_progress } ]
    emits: [ { type: TicketStarted, payload: { id: $id, assignee: $args.assignee } } ]

  close:
    arguments: { fields: { resolution: { type: string, required: true } } }
    transitions: [ { from: [open, in_progress], to: closed } ]
    preconditions:
      - name: estimated
        assert: { gt: [$fields.points, 0] }
        message: unestimated tickets cannot be closed
    set: { resolution: $args.resolution }
    emits: [ { type: TicketClosed, payload: { id: $id, resolution: $fields.resolution } } ]
```

Check it:

```console
$ entity validate ticket.yaml
ticket.yaml: valid (ticket v1)
1 file(s), 0 invalid
```

A definition with a mistake — a transition to a state the lifecycle does not declare, a rule that
reads an argument from inside an invariant — is refused here, by name, before it can ever run.
[The definition language](definitions.md) is the full reference.

## Create an instance

The kernel generates no identifiers; you supply one.

```console
$ entity create --definition ticket.yaml --id t-1 --fields '{"title":"Login fails"}' > open.json
$ entity graph ticket.yaml
ticket v1: initial open
in_progress --close--> closed
open --close--> closed
open --start--> in_progress
```

`open.json` is a **Decision**: the instance as created (state `open`, revision `1`) and the events
creation emitted (none here — this definition declares no creation event).

## Execute an operation

A Decision goes straight back in as the next `--instance`:

```console
$ entity execute --definition ticket.yaml --instance @open.json \
    --operation start --arguments '{"assignee":"alice"}' --format text
ticket t-1 is in_progress (revision 2); events: TicketStarted
```

## Be refused, with a reason

The ticket has no `points`, so `close` fails its precondition. The kernel returns the typed
refusal and changes nothing:

```console
$ entity execute --definition ticket.yaml --instance @open.json \
    --operation start --arguments '{"assignee":"alice"}' \
  | entity execute --definition ticket.yaml --instance - \
    --operation close --arguments '{"resolution":"fixed"}'
{
  "kind": "precondition_failed",
  "message": "precondition 'estimated' failed for operation 'close': unestimated tickets cannot be closed",
  "operation": "close",
  "reason": "unestimated tickets cannot be closed",
  "rule": "estimated"
}
$ echo $?
1
```

Exit `1` means *the kernel refused*; `2` would mean *you invoked it wrongly*; `0` is a decision.
The refusal is JSON on stdout so a pipeline can read it, and a sentence on stderr so a person can.

## What just held

* The lifecycle answered before the rules did: `close` from `open` is declared, so the refusal
  came from the precondition, not from `invalid_transition`.
* The instance in `open.json` is exactly as it was. A refusal never produces a partial state.
* Nothing was persisted or published. That is your shell's job — the `entity` command is one such
  shell, and [The library](library.md) is how to write another.

## Next

* [The definition language](definitions.md) — every key, every operator, every reference.
* [The entity command](cli.md) — verbs, input forms, exit codes.
* [The library](library.md) — `Registry`, `Runtime`, `Decision`, and the errors.
* [The kernel design](../design/kernel-v0.1.md) — why it is shaped this way.
