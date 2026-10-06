---
sidebar_position: 3
title: Service semantics
description: Named outcomes, declared refusals, responses, logical identity and relations — the opt-in service/1, service/2 and service/3 definition rules.
lede: A definition may opt into service rules, under which the kernel selects one named outcome per request and the caller never chooses it.
source: "docs/design/service-semantics-v0.1.md, crates/entity-core/src/definition.rs (Semantics, OutcomeEffect), crates/entity-core/tests/service_semantics.rs, crates/entity-core/tests/text_alphabet.rs, CHANGELOG 0.19.0, 0.26.0 and 0.27.0"
---

# Service semantics

A definition with no `semantics` key follows the original rules, `kernel/1`: an operation moves the
instance along its transitions, and every refusal is one of the kernel's typed refusals. A
definition may opt into a second set of rules with `semantics: service/1`, `service/2` or
`service/3`. The newer rules add; a `kernel/1` definition keeps the bytes and the behaviour it had,
and it is refused at registration if it uses a key only the service rules admit.

## Named outcomes

Under the service rules a creation and an operation carry **outcomes**: ordered branches, each
with its own guard (`when`), `effect`, `set`, `emits`, declared response (`responds`) and an
optional declared refusal (`refuses`). The kernel selects the branch from the validated inputs; a
caller asks for the operation and never names a branch.

| `effect` | Means |
|---|---|
| `creates` | brings the instance into being (creation branches only) |
| `updates` | writes fields without leaving the state, and declares no transition |
| `moves` | moves the instance from one declared state to another |
| none | accepts and changes no state |

This definition validates with `entity 0.27.0` and runs through the `entity` command:

```yaml
entity: invoice
version: 1
semantics: service/1
identity: { field: invoice_id }

schema:
  fields:
    invoice_id: { type: string, required: true }
    total: { type: integer, required: true }

lifecycle:
  initial: open
  states: [open]

create:
  arguments:
    fields:
      invoice_id: { type: string, required: true }
      amount: { type: integer, required: true }
  outcomes:
    - name: accepted
      effect: creates
      set:
        invoice_id: $args.invoice_id
        total: $args.amount
      emits:
        - type: InvoiceCreated
          payload: { invoice_id: $fields.invoice_id }

operations:
  adjust:
    arguments:
      fields:
        amount: { type: integer, required: true }
    response:
      fields:
        new_total: { type: integer, required: true }
    outcomes:
      - name: adjusted
        when: { compare: { left: $args.amount, op: gt, right: 0 } }
        effect: updates
        set: { total: $args.amount }
        emits:
          - type: InvoiceAdjusted
            payload: { total: $fields.total }
        responds: { new_total: $fields.total }
      - name: rejected
        refuses:
          error: AmountNotPositive
          message: an invoice total is positive
```

Under the service rules the `--fields` of `entity create` are the creation's arguments. The first
branch whose guard holds is selected; the declared refusal is a value with the outcome's name and
error:

```shell-session
$ entity create --definition invoice.yaml --id s:INV-1 \
    --fields '{"invoice_id":"INV-1","amount":120}' > created.json
$ entity execute --definition invoice.yaml --instance @created.json \
    --operation adjust --arguments '{"amount":150}' --format text
invoice s:INV-1 is open (revision 2); events: InvoiceAdjusted
$ entity execute --definition invoice.yaml --instance @created.json \
    --operation adjust --arguments '{"amount":0}'
{
  "error": "AmountNotPositive",
  "kind": "refused",
  "message": "outcome 'rejected' refuses with 'AmountNotPositive': an invoice total is positive",
  "outcome": "rejected",
  "reason": "an invoice total is positive"
}
refused: outcome 'rejected' refuses with 'AmountNotPositive': an invoice total is positive
```

The accepted decision's record names the outcome (`adjusted`), the effect (`updated`) and the
response (`{"new_total": 150}`). In Rust, `create` and `execute` return a declared refusal as
`CoreError::Refused`; `decide` and `decide_create` return it as the value `Evaluation::Refused`.

:::caution[Known limitation]

A `moves` effect carries its states as a map, `effect: { moves: { from: open, to: paid } }`.
`entity-yaml`, and so the `entity` command, cannot read that form today: it refuses the document
with `expected a YAML tag starting with '!'`, and the tagged form with `expected unambiguous YAML`.
A definition with a `moves` outcome loads only through `serde_json` in Rust, as the kernel's own
tests do. A fix is planned.

:::

## Identity and relations

A service definition may declare a **logical identity**, `identity: { field: … }`, kept apart from
the storage address an instance is stored at. The address is derived from it by one function,
`entity_core::identity::address`: text identities address as `s:` followed by their contents,
numbers share one canonical spelling, and composites address as canonical JSON. The kernel checks
after every branch that the two still agree:

```shell-session
$ entity create --definition invoice.yaml --id INV-1 \
    --fields '{"invoice_id":"INV-1","amount":120}'
{
  "address": "'s:INV-1'",
  "field": "invoice_id",
  "id": "INV-1",
  "kind": "identity_mismatch",
  "message": "identity field 'invoice_id' addresses to 's:INV-1', but the instance is stored at 'INV-1'"
}
refused: identity field 'invoice_id' addresses to 's:INV-1', but the instance is stored at 'INV-1'
```

`create_derived` and `decide_create_derived` compute the address from the selected creation
outcome's identity instead of taking it from the caller; both paths produce the same decision and
record when the address agrees.

**Relations** (`owns` or `references`, `one` or `many`, and the field that carries them) are
checked at registration: the carrier's shape and optionality by `EntityDefinition::validate`, the
target's identity kind, a second owner and a field claimed twice by `Registry::validate_all`.
Whether a referenced instance exists is still the shell's to check.

## Values and conditions

The service rules add three field kinds — `map`, `union` (adjacently tagged) and `binary64` (a
finite double kept as its token, so the sign of a zero survives) — and four condition operators:
`compare` (an exact three-valued comparison), `truthy`, and the quantifiers `for_all` and
`for_any` over an array's elements or a map's values. `scales:` declares the ordered value scales
text comparison is answered in; without one, ordering two texts is `unknown`. The
[definition language](../reference/definitions.md) lists the `kernel/1` keys these build on.

## Text length and alphabets

Since 0.27.0 the service rules read and constrain text:

- **Length.** A rule may read the length of a declared `string` field, argument or nested property
  as `<path>.count`, wherever an array's or a map's `count` may be read. Length is counted in
  Unicode scalar values with no normalization: a composed `é` is 1, an `e` followed by a combining
  accent is 2.
- **Alphabet.** A `string` field, argument or nested property may declare
  `alphabet: "<characters>"`. A value is valid only when every Unicode scalar value in it is one
  of those characters; the empty text always is. There is no normalization and no case folding. A
  value outside its alphabet is a `validation` error at its path naming the first offending
  character, its code point and its position (counted from 1), beside the value's other errors.

This definition, `extension.yaml`, uses both:

```yaml
entity: extension
version: 1
semantics: service/1
identity: { field: number }

schema:
  fields:
    number: { type: string, required: true, alphabet: "0123456789" }

lifecycle:
  initial: active
  states: [active]

invariants:
  - name: number_is_short
    assert: { compare: { left: $fields.number.count, op: lte, right: 6 } }
    message: an extension has at most six digits

create:
  arguments:
    fields:
      number: { type: string, required: true, alphabet: "0123456789" }
  outcomes:
    - name: created
      effect: creates
      set: { number: $args.number }
```

```shell-session
$ entity create --definition extension.yaml --id s:1234 --fields '{"number":"1234"}' --format text
extension s:1234 is active (revision 1); events: none
$ entity create --definition extension.yaml --id s:12a4 --fields '{"number":"12a4"}'
{
  "errors": [
    {
      "message": "character 'a' (U+0061) at position 3 is not in the alphabet",
      "path": "arguments.number"
    }
  ],
  "kind": "validation",
  "message": "validation failed; arguments.number: character 'a' (U+0061) at position 3 is not in the alphabet"
}
refused: validation failed; arguments.number: character 'a' (U+0061) at position 3 is not in the alphabet
$ entity create --definition extension.yaml --id s:1234567 --fields '{"number":"1234567"}'
{
  "kind": "invariant_violation",
  "message": "invariant 'number_is_short' violated: an extension has at most six digits",
  "reason": "an extension has at most six digits",
  "rule": "number_is_short"
}
refused: invariant 'number_is_short' violated: an extension has at most six digits
```

Registration refuses each misuse by path, with every other defect of the document:

| Definition says | `entity validate` reports |
|---|---|
| `count` on an `integer` field | ``invalid rule at 'invariants[0].assert.compare.left': '$fields.n.count' cannot resolve: 'n' is a integer field, so 'count' resolves to nothing; `count` reads a text, an array or a map`` |
| a segment past a length, `$fields.number.count.digits` | `invalid rule at 'invariants[0].assert.compare.left': '$fields.number.count.digits' cannot resolve: 'number.count' is a count, so 'digits' resolves to nothing` |
| `alphabet` on an `integer` field | `invalid field definition at 'schema.n': 'alphabet' does not apply to a integer field; it applies to a string field` |
| `alphabet: ""` | `invalid field definition at 'schema.number': alphabet must declare at least one character` |
| `alphabet: "01234567890"` | `invalid field definition at 'schema.number': alphabet writes '0' twice, at positions 1 and 11` |
| `default: "12x"` beside that alphabet | `invalid field definition at 'create.arguments.number': invalid default: character 'x' (U+0078) at position 3 is not in the alphabet` |
| `alphabet` in a `kernel/1` definition | ``'alphabet' at 'schema.code.alphabet' is available only under `semantics: service/1`; a definition with older semantics would declare a rule nothing evaluates`` |

Registration also refuses a length read through a quantifier element or used as a projection key.
A path into a `json` field, a union payload, an undeclared member or a quantifier element still
resolves to nothing, so recorded decisions replay unchanged, and a stored value under a declared
`string` that is not a text has no length. A definition without an alphabet keeps its bytes.

`entity generate docs` carries an alphabet into the OpenAPI and AsyncAPI schemas as `x-alphabet`;
in the `openapi.yaml` generated for `extension.yaml`:

```yaml
    ExtensionV1Fields:
      additionalProperties: false
      properties:
        number:
          type: string
          x-alphabet: '0123456789'
```

:::caution[Known limitation]

An alphabet on an operation's declared `response` field is admitted but not enforced: responses are
not checked against their declared schema yet, for `max_length` either. An operation whose
response field declares `alphabet: "0123456789"` and responds `"abc"` is accepted. Checking
responses is planned.

:::

## What each version adds

| Semantics | Adds | Record and request framing |
|---|---|---|
| `service/1` | named outcomes, effects, declared refusals and responses; identity, relations, the new field kinds and operators; text length and alphabets (0.27.0) | `er.record/2`, `er.request/2` |
| `service/2` | an optional argument copied into creation state, events and responses keeps its absence through replay and retry | `er.record/3`, `er.request/3` |
| `service/3` | after the loaded outcome is selected, an operation may ask the host for typed `Set`, `Preserve` and optional `Remove` actions on its fields; events, responses and replay use the resulting values | `er.record/4`, `er.request/4` |

`kernel/1` records stay `er.record/1`. A reader that knows only an older framing refuses a newer
record by name before reading its payload, so **a store holding records of a newer format must not
be opened by a build that predates it**.

## Limits

- `rehydrate` refuses a `service/1` definition: an event-only fold cannot see which branch ran.
  Replay a service history from its decision records.
- Under `service/1`, numeric predicates answer on the value the specification source would have
  observed, while the authored token is stored unchanged; `kernel/1` numbers are untouched.
- Since 0.26.0 a declared refusal is returned even when a command supplies the fulfillment keys of
  a success outcome, and a declared creation refusal comes before an existing-subject conflict.
