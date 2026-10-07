---
sidebar_position: 3
title: Definition language
description: Reference for the kernel/1 definition format — schemas, lifecycles, operations, rules, conditions, templates, references, registration refusals and evaluation order.
source: "Written by hand against crates/entity-core/src/definition.rs, validation.rs and runtime.rs, docs/design/kernel-v0.1.md § 6, and crates/entity-core/tests/requirements.rs; no schema exists to generate it from yet"
---

# Definition language

A definition is a YAML or JSON document describing one entity type. Rules are a closed data AST,
and templates are ordinary values containing checked references. There is no embedded code.

This page describes the default rules, `kernel/1`, which a definition without a `semantics` key
follows. [Service semantics](../concepts/service-semantics.md) describes what `semantics:
service/1` to `service/3` add: named outcomes, identity, relations and more field kinds and
operators.

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
| `string` | UTF-8 text | `min_length`, `max_length`; `alphabet` under the [service rules](../concepts/service-semantics.md#text-length-and-alphabets) |
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
- A `set` value `{increment: n}`, a mapping whose only key is `increment`, adds `n` to the field's
  value before the operation instead of replacing it. `n` is a number or a template that resolves
  to one, such as `$args.amount`; a negative `n` decrements. The field must be a required
  `integer` or `number`, and the amount must always be present and of the field's kind. The sum is
  exact: `0.1` plus `0.2` is `0.3`. A sum the field's kind cannot hold is refused as
  `increment_overflow` rather than wrapped, and a sum past `min` or `max` is a `validation`
  refusal. A mapping with any second key that is not `cleared` is an ordinary object template.
- A `set` value `{cleared: true}`, a mapping whose only key is `cleared`, leaves the field absent
  after the operation. The field must be declared and not `required`; one with a `default` may be
  cleared, and the default does not fill it again. Clearing a field that is already absent is
  accepted. The decision and every event it emits name the field in `removed`, never in
  `changed`, and the schema check, invariants, events and response see it absent: `$fields`
  omits it and a template reading it is a `template` refusal. A creation cannot clear, and a
  field takes one assignment, so `{cleared: true, increment: 1}`, or a cleared field that the
  same outcome also names in `set_if_present` or `fulfills`, is refused.
- `emits` contains zero or more event templates. `emit` is accepted as an alias for the same
  list (under `create`, `emit` is a single template). Events see the post-operation fields and
  are materialized last. An operation that emits nothing leaves no event, so an event-only
  history cannot see that it ran: a fold stops short if it was the last decision and refuses
  the next revision as a gap otherwise. Keep decision records if you need to rebuild such a
  subject.

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
| `starts_with`, `ends_with: [text, affix]` | byte-wise, case-sensitive string prefix or suffix; `false` when a reference is not a string, and a non-string literal is refused at registration |
| `before`, `after: [a, b]` | ordering of two caller-supplied ISO-8601 instants |

There are no calls, loops, arithmetic expressions, clocks, random sources, or lookups. Time enters
as a field or operation argument. `before` and `after` parse strict calendar dates or timestamps;
equal instants satisfy neither operator. A literal operand of `before` or `after` must itself be a
readable instant — an impossible date, an offset-bearing timestamp, a number or any other value the
kernel cannot read is refused at registration, because it would leave the rule unobservable at every
evaluation.

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

## Refusals at registration

Registering a definition refuses with **every** defect it has, so fixing a document takes one
pass; `entity validate` prints them all. A check whose prerequisite already failed is skipped: a
lifecycle with a duplicate state is one finding, not one per transition it invalidates.

| Defect | `kind` |
|---|---|
| an empty entity name, or version `0` | `empty_entity_name`, `zero_version` |
| an empty lifecycle, an empty state name, a duplicate state, an `initial` not among the states | `empty_lifecycle`, `empty_lifecycle_state`, `duplicate_lifecycle_state`, `unknown_initial_state` |
| an empty operation name, an operation without transitions, an empty `from` list | `empty_operation_name`, `no_transitions`, `empty_from_states` |
| a transition through an undeclared state, two transitions of one operation from one state | `unknown_from_state`, `unknown_to_state`, `ambiguous_transition` |
| `set` writing an undeclared field, an empty event type | `unknown_set_field`, `empty_event_type` |
| a `set` increment on a creation, on a field that is not a required `integer` or `number`, or with an amount that is not an always-present number of the field's kind | `increment_on_create`, `increment_target_invalid`, `increment_amount_invalid` |
| a `set` clear on a creation, on a field that is required or not declared, or with a flag other than `true`; a `set` value naming both `cleared` and `increment` | `clear_on_create`, `clear_target_invalid`, `clear_flag_invalid`, `set_assignment_conflict` |
| an inconsistent field: `min` above `max`, an enum without `values`, an array without `items`, a default that fails its own field | `invalid_field` |
| a constraint on a kind it does not govern, such as `min_length` on an `integer` | `constraint_not_applicable` |
| an inconsistent rule: an empty name or message, an empty `all` or `any`, a reference its scope cannot see or the schema does not declare | `invalid_rule` |
| a template whose scope could never resolve it | `invalid_template` |
| a second definition of an `(entity, version)` already registered | `duplicate_definition` |
| a key only the service rules admit, in a `kernel/1` document | `semantics_key_not_available` |
| a `ref` to a type nobody registered (from `Registry::validate_all`, not `register`) | `unknown_relation_target` |

A key the format does not declare, or a condition with two operators or an unknown one, is refused
earlier still, when the document is parsed. [Typed refusals](./refusals.md) lists the defects the
service rules add.

## Evaluation order

An operation runs these twelve steps in this order, and a refusal at any step returns before the
next and leaves the caller's instance untouched:

| Step | Check or action | Refusal `kind` |
|---|---|---|
| 0 | the instance's `(entity, version)` matches the definition | `entity_mismatch` |
| 1 | the instance claims a state the definition declares | `unknown_state` |
| 2 | the operation exists | `operation_not_found` |
| 3 | arguments are defaulted, then validated | `validation` |
| 4 | a transition is selected from the current state | `invalid_transition` |
| 5 | preconditions, against the current state and the arguments | `precondition_failed`, `precondition_unobservable` |
| 6 | `set`, every assignment read from the fields before the operation | `template`, `increment_overflow` |
| 7 | the resulting fields are validated against the schema | `validation` |
| 8 | the next instance is built: the new state, revision plus one | |
| 9 | invariants, against the next state | `invariant_violation`, `invariant_unobservable` |
| 10 | events are materialized from their templates | `template` |
| 11 | the `Decision` is returned | |

The order is part of the contract: "you cannot do that from here" (`invalid_transition`) is never
masked by a failed precondition, and an invariant judges the state that would be stored. A
`service/1` definition inserts four steps — branch selection, a refusing branch's return, the
identity check and the declared response — and moves nothing else.
