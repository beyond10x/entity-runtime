---
sidebar_position: 1
title: Model policy as data
description: Turn domain state, actions and rules into a definition people and agents can inspect before anything executes.
lede: Write a definition, validate it, inspect it, and watch the kernel enforce its lifecycle and invariant.
source: "crates/entity-core/src/definition.rs, crates/entity-cli (validate, inspect, graph, create, execute), run with entity 0.27.0"
---

# Model policy as data

A useful definition says what a change *means*. It is not a JSON schema with a writable `status`
field; it is the authority that owns every legal transition. This guide models a support ticket.

## Start with the questions people ask

1. What facts must always travel with it? → `schema`
2. Which states are meaningful to an operator? → `lifecycle`
3. Which named actions may change it? → `operations`
4. What must be true before each action? → `preconditions`
5. What must be true after every accepted action? → `invariants`
6. Which facts should downstream systems observe? → `emits`

Keep facts in fields and authority outside them. Actor identity, the current time and correlation
belong to the trusted shell, or to operation arguments when a rule must evaluate them.

## Write the definition

Save this as `ticket.yaml`:

```yaml
entity: ticket
version: 1

schema:
  additional_fields: false
  fields:
    title: { type: string, required: true, min_length: 1 }
    resolution: { type: enum, values: [fixed, wontfix] }

lifecycle:
  initial: open
  states: [open, active, closed]

invariants:
  - name: closed_has_a_resolution
    assert:
      any:
        - ne: [$state, closed]
        - exists: $fields.resolution
    message: a closed ticket records how it was resolved

operations:
  start:
    transitions: [{ from: open, to: active }]
  close:
    arguments:
      fields:
        resolution: { type: enum, values: [fixed, wontfix], required: true }
    transitions: [{ from: active, to: closed }]
    set:
      resolution: $args.resolution
    emits:
      - type: TicketClosed
        payload: { ticket: $id, resolution: $fields.resolution }
```

Every state change is a named operation. Prefer `close` over a generic `set_status`: a named
operation has its own arguments, rules, assignments and events, and gives an agent a small,
inspectable vocabulary.

A **precondition** asks whether this operation may run now; it sees the current fields, the
validated arguments and the transition. An **invariant** asks whether the resulting entity is valid
in every state; it sees the next fields and state, but not the arguments. "A closed ticket records
how it was resolved" is an invariant because it must hold however the ticket reaches `closed`.

## Validate and inspect it

```shell-session
$ entity validate ticket.yaml
ticket.yaml: valid (ticket v1)
1 file(s), 0 invalid
$ entity inspect ticket.yaml
entity: ticket  version: 1
states: open (initial), active, closed
fields:
  resolution: enum, one of [fixed, wontfix]
  title: string, required
invariants:
  closed_has_a_resolution — a closed ticket records how it was resolved
operations:
  close: active -> closed
    arguments: resolution*  (* required)
    sets: resolution
    emits: TicketClosed
  start: open -> active
$ entity graph ticket.yaml
ticket v1: initial open
active --close--> closed
open --start--> active
```

Keys are closed. A misspelled key is refused rather than silently weakening the policy — here
`requried` in place of `required` on `title`:

```shell-session
$ entity validate ticket-typo.yaml
ticket-typo.yaml: invalid: invalid entity YAML: schema.fields.title: unknown field `requried`, expected one of `type`, `required`, `default`, `min_length`, `max_length`, `alphabet`, `min`, `max`, `values`, `items`, `properties`, `additional_properties`, `entity`, `inverse`, `acyclic`, `key`, `tag`, `variants` at line 7 column 28
1 file(s), 1 invalid
```

## Watch the kernel hold the lifecycle

```shell-session
$ entity create --definition ticket.yaml --id T-1 --fields '{"title":"Login fails"}' > t1.json
$ entity execute --definition ticket.yaml --instance @t1.json \
    --operation close --arguments '{"resolution":"fixed"}'
{
  "kind": "invalid_transition",
  "message": "operation 'close' is not valid from lifecycle state 'open'",
  "operation": "close",
  "state": "open"
}
refused: operation 'close' is not valid from lifecycle state 'open'
$ entity execute --definition ticket.yaml --instance @t1.json --operation start > t2.json
$ entity execute --definition ticket.yaml --instance @t2.json \
    --operation close --arguments '{"resolution":"fixed"}' --format text
ticket T-1 is closed (revision 3); events: TicketClosed
```

## Treat missing evidence deliberately

A comparison over a value nobody recorded answers `unknown`, not `false`, and the refusal names
every missing path. Use that when an absent observation should stop the workflow and say what to
gather. Put an `exists` test beside the comparison when absence should be an ordinary failure:

```yaml
assert:
  all:
    - exists: $fields.review_score
    - gte: [$fields.review_score, 4]
```

## Events are facts, not side effects

`TicketClosed` means the kernel accepted the operation. A shell may publish it after the decision
is durably recorded. Keep imperative work out of templates; a template only fills data from the
decision.

When definitions reference each other with `type: ref`, validate the whole set together so a
reference to an unregistered type is refused, and draw it with `entity graph --references`
([render graphs](./render-graphs.md)). The [definition language](../reference/definitions.md) lists
every key, condition and template reference.
