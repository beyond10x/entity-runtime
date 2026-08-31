---
sidebar_position: 7
title: Definition language
description: Complete reference for schemas, lifecycles, operations, rules, conditions, templates, references, and evaluation order.
---

# Definition language

A definition is a YAML or JSON document describing one entity type. Rules are a closed data AST,
and templates are ordinary values containing checked references. There is no embedded code.

Every definition key is closed. A misspelled `requried`, an unknown condition operator, or a
condition carrying two operators is refused rather than ignored.

```yaml
entity: refund
version: 1
schema: {}
lifecycle: {}
invariants: []
create: {}
operations: {}
```

`entity` must be non-empty. `version` defaults to `1` and must be greater than zero. The pair
`(entity, version)` identifies a definition in a registry.

## Schema

```yaml
schema:
  additional_fields: false
  fields:
    title:       { type: string, required: true, min_length: 1 }
    amount:      { type: integer, min: 1, max: 100000 }
    confidence:  { type: number, min: 0, max: 1 }
    urgent:      { type: boolean, default: false }
    priority:    { type: enum, values: [low, normal, high], default: normal }
    tags:        { type: array, items: { type: string }, default: [] }
    address:     { type: object, properties: { city: { type: string } } }
    customer_id: { type: ref, entity: customer, inverse: refunds, acyclic: false }
    metadata:    { type: json }
```

| Type | Accepted value | Applicable keys |
|---|---|---|
| `string` | UTF-8 text | `min_length`, `max_length` |
| `integer` | whole JSON number | `min`, `max` |
| `number` | any JSON number | `min`, `max` |
| `boolean` | `true` or `false` | none |
| `enum` | one string in `values` | non-empty `values` |
| `array` | list | required `items` field definition |
| `object` | mapping | `properties`, `additional_properties` |
| `ref` | non-empty identity string | required `entity`; optional `inverse`, `acyclic` |
| `json` | any JSON value | none |

Every field also accepts `required` and `default`. Defaults are applied before validation and are
validated when the definition is registered. A nested default is applied when its containing object
exists; it does not invent the containing object.

Numeric comparisons do not pass through `f64`, so large integers and bounds retain their JSON
precision. A constraint on the wrong type is a definition defect, not an ignored decoration.

Validation accumulates independent value failures and returns a path for each one, such as
`fields.amount_cents` or `arguments.items[2].sku`. Undeclared fields are refused unless the relevant
object explicitly enables additional fields.

### Typed references

A `ref` declares that a string identity points at another entity type. Register all related
definitions together and call `Registry::validate_all`; an unknown target type is refused. The
kernel is intentionally given one instance at a time and cannot prove that the referenced instance
exists. The shell enforces existence and `acyclic` graph constraints.

`inverse` names how readers describe the opposite direction; it does not create a second stored
edge. `acyclic` defaults to `false`.

## Lifecycle

```yaml
lifecycle:
  initial: draft
  states: [draft, submitted, approved, rejected]
```

States are an open vocabulary inside each definition. `states` must be non-empty, each state is
non-empty and unique, and `initial` must be declared. Creation enters `initial`.

Transitions live on operations. There is no generic status write.

## Operations

```yaml
operations:
  approve:
    arguments:
      fields:
        reason: { type: string, required: true, min_length: 1 }
    transitions:
      - from: submitted
        to: approved
    preconditions:
      - name: evidence_is_present
        assert: { gt: [$fields.evidence_count, 0] }
        message: a refund cannot be approved without evidence
    set:
      decision_reason: $args.reason
    emits:
      - type: RefundApproved
        payload: { refund_id: $id, reason: $fields.decision_reason }
```

- `arguments` is an object schema. Defaults are applied and values validated before rules run.
- `transitions` must contain at least one edge. Within an operation, at most one edge may start from
  any state.
- `preconditions` run against the current fields, validated arguments, and selected transition.
- `set` assigns fields from templates. Every assignment reads the pre-operation fields, so entry
  order has no meaning. The resulting fields are validated again.
- `emits` contains zero or more event templates. `emit` is accepted as an alias. Events see the
  post-operation fields and are materialized last.

Revision is `1` after creation and increases by one per accepted operation. Execution refuses before
exceeding the supported signed 64-bit revision range.

## Creation

```yaml
create:
  emit:
    type: RefundDrafted
    payload: { refund_id: $id, state: $state, fields: $fields }
```

Creation validates and defaults fields, enters the initial state, checks invariants, and emits at
most one event. Creation templates have no arguments or previous state.

## Rules and scope

| | Precondition | Invariant |
|---|---|---|
| Attached to | one operation | the entity definition |
| Evaluated | before `set` | after creation or `set`, against the next state |
| May read | `$args`, `$fields`, `$old_fields`, `$from_state`, `$to_state`, identity | `$fields`, `$state`, identity |
| Refusal when false | `precondition_failed` | `invariant_violation` |
| Refusal when unknown | `precondition_unobservable` | `invariant_unobservable` |

Rule names and messages are optional but must not be blank when written. Both appear in refusals.
References outside the rule's scope or outside a declared schema are refused at registration.

## Condition operators

Every condition carries exactly one operator.

| Operator | Meaning |
|---|---|
| `true`, `false` | literal condition |
| `all: [c, ...]` | every child; list must not be empty |
| `any: [c, ...]` | at least one child; list must not be empty |
| `not: c` | logical negation |
| `exists: value` | whether the reference resolves to a non-null value |
| `eq`, `ne: [a, b]` | structural equality or inequality |
| `gt`, `gte`, `lt`, `lte: [a, b]` | numeric comparison |
| `in: [needle, list]` | list contains value |
| `contains: [container, needle]` | array element, string substring, or object key membership |
| `before`, `after: [a, b]` | ordering of two caller-supplied ISO-8601 instants |

There are no calls, loops, arithmetic expressions, clocks, random sources, or lookups. Time enters
as a field or operation argument. `before` and `after` parse strict calendar dates or timestamps;
equal instants satisfy neither operator.

### Three-valued results

A condition answers `true`, `false`, or `unknown`, and a rule holds only on `true`.

`exists` asks whether a value is present and is always answerable. Comparisons become `unknown` when
an operand cannot be observed. A YAML key with no value deserializes as null and is treated as
unobserved. A literal null deliberately written inside the definition remains a literal value.

`false` dominates `all`, `true` dominates `any`, and `not unknown` remains unknown. All operands are
evaluated so an unobservable refusal can name every missing path.

Use `exists` alongside a comparison when absence should be an ordinary failure:

```yaml
assert:
  all:
    - exists: $fields.review_score
    - gte: [$fields.review_score, 4]
```

Leave the comparison unguarded when missing evidence should produce an unobservable refusal.

## Templates and references

A template is any JSON/YAML value. A string beginning with `$` is a reference. Arrays and objects
resolve recursively. `$$` escapes a literal leading dollar.

| Reference | Value |
|---|---|
| `$id` | instance identity |
| `$entity`, `$version` | definition identity |
| `$state`, `$to_state` | next state; initial state during creation |
| `$from_state` | previous state; absent during creation |
| `$args`, `$args.path` | validated, defaulted arguments |
| `$fields`, `$fields.path` | pre-operation fields in `set`; post-operation fields in events |
| `$old_fields`, `$old_fields.path` | pre-operation fields |

There is no `$now` or `uuid()`. References are checked against their scope and schema at
registration. A path inside a `json` field may remain a runtime check because its shape is
deliberately undeclared; failure is a typed `template` refusal, never a silent null.

## Evaluation order

1. Definition and instance identity match; the instance claims a declared state.
2. The operation exists.
3. Arguments are defaulted and validated.
4. A transition is selected from the current state.
5. Preconditions run.
6. `set` templates resolve from the old fields.
7. Resulting fields are validated.
8. The next instance is constructed with revision plus one.
9. Invariants run against the next instance.
10. Events are materialized.
11. The complete `Decision` is returned.

A refusal at any step returns before the next and leaves the caller-owned instance untouched.
