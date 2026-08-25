# The definition language

A definition is a YAML (or JSON) document that describes one entity type. Nothing in it is code:
the rules are an AST, the templates are values with `$` references, and there is no place to put
an expression. That is what lets the kernel validate a definition when it is registered and
evaluate it identically everywhere.

```yaml
entity: order          # the type name; must not be empty
version: 1             # default 1; must be > 0; (entity, version) is the identity
schema: ...            # § Schema
lifecycle: ...         # § Lifecycle
invariants: ...        # § Rules
create: ...            # § Creation
operations: ...        # § Operations
```

The worked example with every field kind and both rule kinds is
[`examples/order.yaml`](https://github.com/beyond10x/entity-runtime/blob/main/examples/order.yaml).

## Schema

```yaml
schema:
  additional_fields: false          # default; true accepts undeclared fields
  fields:
    customer_id: { type: string, required: true, min_length: 1 }
    total_cents: { type: integer, required: true, min: 0 }
    priority:    { type: enum, values: [low, normal, high], default: normal }
    tags:        { type: array, default: [], items: { type: string } }
    address:     { type: object, properties: { city: { type: string, required: true } } }
    extra:       { type: json }
```

| kind | value | constraints that apply |
|---|---|---|
| `string` | UTF-8 text | `min_length`, `max_length` (in characters) |
| `integer` | a whole number | `min`, `max` |
| `number` | any JSON number | `min`, `max` |
| `boolean` | `true`/`false` | — |
| `enum` | one of `values` | `values` (required, non-empty) |
| `array` | a list | `items` (required): the element definition |
| `object` | a nested object | `properties`, `additional_properties` |
| `json` | anything | — |

Every field accepts `required` and `default`. Defaults are applied **before** validation, and a
default is itself validated against its field when the definition is registered.

Validation **accumulates**: an object with four bad values reports four errors, each with a path
(`fields.total_cents`, `arguments.items[2].sku`). An undeclared field is an error unless
`additional_fields: true`; the fields document itself must be an object.

The same schema shape describes an operation's `arguments`.

## Lifecycle

```yaml
lifecycle:
  initial: draft
  states: [draft, submitted, approved, rejected, fulfilled, cancelled]
```

States are an open vocabulary; each is declared once, none is empty, and `initial` must be one of
them. Creation puts the instance in `initial`.

Transitions are **not** declared here. They live on operations, because the edge is the operation:
it has arguments, rules and events of its own. Nothing writes the lifecycle state except an
operation — there is no generic status field to patch.

## Operations

```yaml
operations:
  reject:
    arguments:
      fields:
        actor:  { type: string, required: true }
        reason: { type: string, required: true, min_length: 1 }
    transitions:
      - from: submitted                # one state, or a list: [draft, submitted]
        to: rejected
    preconditions:                     # § Rules
      - name: not_locked
        assert: { ne: [$fields.locked, true] }
        message: locked orders cannot be rejected
    set:                               # deterministic field assignments
      rejection_reason: $args.reason
    emits:                             # 0..N events, each with a templated payload
      - type: OrderRejected
        payload: { order_id: $id, actor: $args.actor, reason: $fields.rejection_reason }
```

* **`transitions`** — at least one. Within one operation at most one transition may start from any
  given state; two would leave the kernel guessing, and the definition is refused.
* **`arguments`** — a schema. Defaulted, then validated, before anything else is evaluated.
* **`set`** — every assignment is resolved against the **pre-operation** fields, so
  `set: {a: $fields.b, b: $fields.a}` is a swap and the map has no ordering semantics. After `set`
  the fields are validated against the schema again.
* **`emits`** — events, materialised last, from templates that see the **post-operation** fields.
  `emit` is accepted as an alias.

`revision` is `1` after creation and `+1` per successful operation. A refusal consumes none.

## Creation

```yaml
create:
  emit:
    type: OrderCreated
    payload: { order_id: $id, state: $state, fields: $fields }
```

Creation validates the fields, enters `initial`, checks the invariants and emits at most one event.
Its templates see `$id`, `$entity`, `$version`, `$state` and `$fields` — there is no previous
state and there are no arguments.

## Rules

Two kinds, told apart by **what they may see**:

| | precondition | invariant |
|---|---|---|
| belongs to | an operation | the entity |
| evaluated | after arguments and transition, before `set` | after creation and after every operation, on the **next** state, before events escape |
| may read | `$args.*` `$fields.*` `$old_fields.*` `$from_state` `$to_state` `$id` `$entity` `$version` | `$fields.*` `$state` `$id` `$entity` `$version` |
| refusal | `precondition_failed` | `invariant_violation` |

An invariant that could read `$args` would be a precondition in disguise. The reference is refused
when the definition is registered, as is any reference to a field or argument the schema does not
declare.

```yaml
invariants:
  - name: rejected_requires_reason
    assert:
      any:
        - ne: [$state, rejected]
        - exists: $fields.rejection_reason
    message: rejected orders must have a rejection reason
```

`name` and `message` are optional; both appear in the refusal, and a rule without a message gets
a default.

## Conditions

An `assert` is an AST:

| operator | holds when |
|---|---|
| `true` / `false` | literally |
| `all: [c, ...]` | every child holds (short-circuits; must not be empty) |
| `any: [c, ...]` | at least one child holds (short-circuits; must not be empty) |
| `not: c` | the child does not hold |
| `exists: x` | `x` resolves to a value |
| `eq: [a, b]` / `ne: [a, b]` | structural equality / inequality of JSON values |
| `gt`, `gte`, `lt`, `lte: [a, b]` | numeric comparison; `false` unless both are numbers |
| `in: [needle, haystack]` | `haystack` resolves to an array containing `needle` |
| `contains: [container, needle]` | array ∋ element, string ⊇ substring, or object ∋ key |

A reference that does not resolve makes a comparison or membership test **false**; `exists` is
the one operator that asks about presence. There is no function call, loop, arithmetic, clock,
random source or lookup.

> Known limit: *missing* and *false* are one verdict. A consumer that must tell *nobody looked*
> from *it is wrong* needs a third value — see the [kernel design § 4](../design/kernel-v0.1.md#4-the-condition-language).

## Templates and references

A template is any JSON/YAML value. A string beginning with `$` is a reference; anything else is a
literal; `$$` escapes a literal leading dollar; arrays and objects are resolved recursively.

| reference | resolves to |
|---|---|
| `$id` | the instance's identity |
| `$entity`, `$version` | the definition's |
| `$state`, `$to_state` | the state after the operation (at creation: `initial`) |
| `$from_state` | the state before the operation (absent at creation) |
| `$args`, `$args.<path>` | the validated, defaulted arguments |
| `$fields`, `$fields.<path>` | in `set`: the pre-operation fields; in events: the post-operation fields |
| `$old_fields`, `$old_fields.<path>` | the pre-operation fields, always |

There is no `$now` and no `uuid()`. An operation that needs a timestamp declares an argument —
`occurred_at: { type: string, required: true }` — and the shell, which has a clock, supplies it.
A reference that does not resolve in a `set` value or an event payload is an error, never a
silent `null`.

## Evaluation order

1. the instance's `(entity, version)` matches the definition — else `entity_mismatch`
2. the operation exists — else `operation_not_found`
3. arguments: defaults, then validation — else `validation`
4. a transition is selected from the current state — else `invalid_transition`
5. preconditions, against the current state and the arguments — else `precondition_failed`
6. `set`, every assignment against the pre-operation fields — else `template`
7. the resulting fields are validated — else `validation`
8. the next instance is constructed: new state, revision + 1
9. invariants, against the next state — else `invariant_violation`
10. events are materialised — else `template`
11. the Decision is returned

A refusal at any step returns before the next, and the caller's instance is untouched.

## Refusals at registration

Registering a definition refuses, with a typed `DefinitionError`: an empty entity name; version
`0`; an empty lifecycle or an empty state name; an `initial` not among the states; a duplicate
state; an empty operation name; an operation without transitions; a transition through an
undeclared state; two transitions from one state; `set` writing an undeclared field; an empty
event type; an inconsistent field (`min` above `max`, an enum without `values`, an array without
`items`, a default that fails its own field); an inconsistent rule (empty name or message, empty
`all`/`any`, a reference the scope cannot see or the schema does not declare).
