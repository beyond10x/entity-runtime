# The definition language

A definition is a YAML (or JSON) document that describes one entity type. Nothing in it is code:
the rules are an AST, the templates are values with `$` references, and there is no place to put
an expression. That is what lets the kernel validate a definition when it is registered and
evaluate it identically everywhere.

**Every key is closed.** A key the model does not declare is refused, not ignored — `requried: true`
would otherwise leave a field optional while its author believed it was required, and a
`precondition:` that should have been `preconditions:` would leave an operation unguarded. The same
holds inside a condition: exactly one operator, spelled correctly.

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

Every field accepts `required` and `default`. Defaults are applied **before** validation, at every
depth an object or array element already reaches — a `default` on a nested property of an object
that was supplied is filled in — and a default is itself validated against its field when the
definition is registered. A default never invents the object that would hold it: supply
`{"address": {}}` and its properties' defaults land; supply nothing and no `address` appears.

A constraint written on a kind it does not govern is **refused**, not ignored: `values` on a
`string`, `items` on an `object`, `min_length` on an `integer`. An `integer` outside the range of a
64-bit signed value is compared numerically rather than wrapped, so a huge number cannot pass a
`max` bound.

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
| may **not** read | `$state` | `$args.*` `$old_fields.*` `$from_state` `$to_state` |
| refusal | `precondition_failed`, or `precondition_unobservable` | `invariant_violation`, or `invariant_unobservable` |

An invariant that could read `$args` would be a precondition in disguise. A precondition that could
read `$state` would be worse: `$state` is the state the operation is heading *for*, so
`eq: [$state, draft]` on a `draft → submitted` transition reads as "we are in draft" and refuses
every time it should pass. Both references are refused when the definition is registered — as is
any reference to a field, nested property or argument the schema does not declare:
`$fields.address.countri` is refused where `$fields.address.country` is accepted, and
`$fields.title.length` is refused because `title` is a string and nothing lives below it.

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
a default. This one asks `exists`, a question about the store, so an order that reaches `rejected`
with no reason recorded is refused as a plain violation — see [Three values, and where the third
one comes from](#three-values-and-where-the-third-one-comes-from) below.

## Conditions

An `assert` is an AST:

| operator | evaluates to |
|---|---|
| `true` / `false` | literally |
| `all: [c, ...]` | `true` when every child does; must not be empty |
| `any: [c, ...]` | `true` when at least one child does; must not be empty |
| `not: c` | the child's value, negated — and `unknown` negates to `unknown` |
| `exists: x` | `true` when there is a value at `x`, else `false` |
| `eq: [a, b]` / `ne: [a, b]` | structural equality / inequality of JSON values |
| `gt`, `gte`, `lt`, `lte: [a, b]` | numeric comparison; `false` when both resolve and either is not a number |
| `in: [needle, haystack]` | `true` when `haystack` is an array containing `needle` |
| `contains: [container, needle]` | array ∋ element, string ⊇ substring, or object ∋ key |

Numbers compare **numerically** everywhere, so `eq: [$fields.total, 100]` holds for `100` and for
`100.0` and agrees with `gte`/`lte`. There is no function call, loop, arithmetic, clock, random
source or lookup.

A condition carries exactly one operator. Two in one mapping — the indentation slip that puts `any:`
beside `all:` — is refused by name rather than silently enforcing the first.

### Three values, and where the third one comes from

A rule answers `true`, `false` or `unknown`, and **holds only when the answer is `true`**.

`unknown` is a property of the **question**, not of the operator asking it. The operators split
into two groups:

| the question | operators | can answer `unknown`? |
|---|---|---|
| **about the store** — is there a value at this address? | `exists` | no. The kernel holds the instance, so it can always look |
| **about a value** — what does it say? | `eq` `ne` `gt` `gte` `lt` `lte` `in` `contains` | yes, when there is no value to read |

So `exists` is an ordinary two-valued predicate and `not: { exists: $fields.x }` means exactly what
it reads as. What has no answer is `gte: [$fields.score, 4]` on a claim nobody has scored — you
cannot say whether an unwritten score clears four.

A key present with nothing after it is **not** a value. `review:` with a blank line after it is how
YAML spells *nobody filled this in*, so `exists` reports `false` for it and a comparison against it
reports `unknown`. A `null` you write as a literal in the definition is a value; you wrote it.

An `unknown` rule is its own refusal, `precondition_unobservable` or `invariant_unobservable`,
naming **every** address it could not read:

```console
$ entity execute --definition claim.yaml --instance @c-1.json --operation accept
refused: precondition 'evidenced' for operation 'accept' cannot be evaluated: an accepted claim
carries an approved review scoring at least four; nothing was observed at $fields.review,
$fields.score
```

Compare `precondition_failed`, which means somebody looked and what they found contradicts the
rule. Sending an operator to fix a review that was never written is the failure this distinction
exists to prevent.

The connectives are [Kleene's](https://en.wikipedia.org/wiki/Three-valued_logic): `false`
dominates `all`, `true` dominates `any`, and `not unknown` is `unknown`. On any rule that never
reads a missing value they are ordinary boolean logic, so nothing else changes. `all` and `any`
evaluate **every** operand rather than stopping early — the answer is the same either way, and
evaluating all of them is what lets one refusal name all three missing facts instead of three
refusals naming one each.

### Guarding a value question with a store question

Because `false` dominates `all`, putting the presence test in the same rule turns *nobody recorded
it* into a plain failure rather than a stall:

```yaml
assert:
  all:
    - exists: $fields.resolution          # false when nothing is recorded ...
    - eq: [$fields.resolution, fixed]     # ... so this cannot leave the rule unknown
```

Order does not matter — Kleene's connectives are commutative. Write it that way when a missing
value should refuse plainly, and leave the comparison bare when a missing value should stop the
gate and send somebody to go and record one.

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

Templates are checked when the definition is registered, against the scope they sit in: a creation
event has no arguments and no `$from_state`; an operation template may read both; an argument the
operation does not declare is refused there and then. What registration cannot decide — a path into
a `json` field, whose shape no schema describes — stays a run-time error, never a silent `null`.

## Evaluation order

1. the instance's `(entity, version)` matches the definition — else `entity_mismatch` — and its
   `lifecycle_state` is one the definition declares — else `unknown_state`
2. the operation exists — else `operation_not_found`
3. arguments: defaults, then validation — else `validation`
4. a transition is selected from the current state — else `invalid_transition`
5. preconditions, against the current state and the arguments — else `precondition_failed`, or
   `precondition_unobservable` when a rule reads something nobody recorded
6. `set`, every assignment against the pre-operation fields — else `template`
7. the resulting fields are validated — else `validation`
8. the next instance is constructed: new state, revision + 1
9. invariants, against the next state — else `invariant_violation`, or `invariant_unobservable`
10. events are materialised — else `template`
11. the Decision is returned

A refusal at any step returns before the next, and the caller's instance is untouched.

## Refusals at registration

Registering a definition refuses, with a typed `DefinitionError`: an empty entity name; version
`0`; an empty lifecycle or an empty state name; an `initial` not among the states; a duplicate
state; an empty operation name; an operation without transitions; a transition through an
undeclared state; two transitions from one state; `set` writing an undeclared field; an empty
event type; an inconsistent field (`min` above `max`, an enum without `values`, an array without
`items`, a default that fails its own field); a constraint on a kind it does not govern; an
inconsistent rule (empty name or message, empty `all`/`any`, a reference the scope cannot see or
the schema does not declare, at any depth); a template whose scope could never resolve it; and a
second definition of an `(entity, version)` already registered.

A key the model does not declare, or a condition with two operators or a misspelled one, is refused
earlier still — when the document is parsed.
