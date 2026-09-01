---
sidebar_position: 4
title: Model policy as data
description: Turn domain state, actions, and rules into a definition that people and agents can inspect before execution.
---

# Model policy as data

A useful definition says what a change *means*. It is not a JSON schema with a writable `status`
field; it is the authority that owns every legal transition.

## Start with the questions people ask

For a refund request:

1. What facts must always travel with it?
2. Which states are meaningful to an operator?
3. Which named actions may change it?
4. What must be true before each action?
5. What must be true after every accepted action?
6. Which facts should downstream systems observe?

Those answers map directly to `schema`, `lifecycle`, `operations`, `preconditions`, `invariants`,
and `emits`.

## Model facts, not capabilities

Fields describe the entity: amount, order identity, collected evidence, and recorded reasons. Keep
ambient authority outside the instance. Actor identity, current time, and correlation belong to the
trusted shell or to explicit operation arguments when policy must evaluate them.

Use the narrowest field type and bounds that express the domain. Definition keys are closed, so a
misspelled constraint is refused instead of silently weakening policy.

## Make every state change a named operation

Prefer:

```yaml
operations:
  approve:
    transitions: [{ from: submitted, to: approved }]
```

over a generic `set_status` operation. A named operation can have its own argument schema, rules,
field assignments, and events. It also gives an agent a small, inspectable action vocabulary.

## Put rules at the correct boundary

A **precondition** asks whether this operation may run now. It can see the current fields, validated
arguments, and selected transition.

An **invariant** asks whether the resulting entity is valid in every state. It sees the next fields
and next state, but not operation arguments or the previous state.

For example, “large refunds require a human actor” is a precondition because it depends on the
operation's trusted `actor_role` argument. “Every approved refund has a reason” is an invariant
because it must hold regardless of which future operation reaches `approved`.

## Treat missing evidence deliberately

A comparison against a missing value is `unknown`, not `false`. Use that when an absent observation
should stop the workflow and tell the caller what to gather. Pair a comparison with `exists` when
absence should be an ordinary failed rule.

## Events are facts, not side effects

`RefundApproved` means the runtime accepted the operation. It does not mean money moved. A shell may
publish that event to a payment worker after the decision is durably recorded. Keep imperative work
outside templates; templates only materialize data from the decision context.

## Validate before agents see the definition

Run the whole registry together when definitions contain typed references:

```bash
entity validate customer.yaml refund.yaml
entity inspect refund.yaml
entity graph refund.yaml
entity graph customer.yaml refund.yaml --references
```

Registration accumulates independent defects so authors can fix a document in one pass. Once a
definition is validated, kernel entry points accept the validated handle rather than raw parsed
data.

The [definition reference](./definitions) lists every field kind, condition, template reference,
and evaluation step.
