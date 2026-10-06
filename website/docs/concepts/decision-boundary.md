---
sidebar_position: 1
title: The decision boundary
description: Keep agent intent flexible while state transitions, refusals and evidence stay reproducible.
lede: An agent proposes an operation; a trusted shell supplies authority and canonical state; the kernel decides.
source: "crates/entity-core (runtime, validation), crates/entity-shell (StoredRuntime), examples/refund.yaml"
---

# The decision boundary

An agent can read a customer message, gather evidence and propose a refund. Those are useful
probabilistic tasks. Whether the refund is *allowed* is a different kind of question, and Entity
Runtime answers only that one.

If policy lives in a prompt or a tool description, every call asks a model to interpret it again.
If a tool accepts arbitrary updates, one mistaken call can skip a lifecycle, overwrite newer work,
or change state without a record of why. Entity Runtime separates the two jobs:

<img
  src="/entity-runtime/img/agent-boundary.svg"
  alt="An agent proposes an operation and arguments. A trusted shell adds canonical state, the validated definition, identity, authority, timestamps, and the expected revision. Entity Runtime returns either a decision to record atomically or a typed refusal that changes nothing."
  loading="eager"
/>

## Three parties

| Party | Owns |
|---|---|
| The agent (or any caller) | which named operation to try, and the domain arguments it was delegated |
| The trusted shell — your application | the definition set, the canonical instance, authentication, authority, record ids, timestamps, the expected revision, storage, and what happens after a commit |
| The kernel, `entity-core` | whether the operation exists, is legal from the current state, has valid arguments and satisfies every rule; and the complete result |

In the [getting-started](../getting-started.md) refund, the agent may propose `approve`. The shell
supplies `actor_role: agent` from its own knowledge of the caller, and the rule
`large_refunds_need_a_human` refuses a refund above 5000 cents. The model cannot become a human
approver by saying it is one, because it never sets `actor_role`.

## Why this holds better than prompt instructions

- **The actions are discoverable.** `entity inspect` and `entity graph` show the states,
  operations, arguments and references a definition actually declares, and `entity skill` renders
  usage guidance for the installed version.
- **Policy is evaluated, not interpreted.** Conditions are a closed data language. There are no
  callbacks, lookups, clocks or loops in a rule. The same inputs give the same decision and the
  same serialized bytes.
- **Missing evidence is not silently false.** A comparison over a value nobody observed answers
  `unknown`; the refusal is `precondition_unobservable` and names every missing path, so the caller
  can gather evidence instead of treating absence as a no.
- **Refusals are part of the tool contract.** A refusal is data with a `kind` and the rule, state
  or path that caused it, so an agent can revise its proposal without scraping prose.
- **Accepted changes carry their record.** A decision keeps what was asked, which definition
  decided it, what changed and which events resulted. Actor and time enter at the shell.

## A safe agent loop

1. Load the canonical instance and the allowed definitions in trusted code.
2. Expose a tool whose input is an allowed operation name and its domain arguments.
3. Inject identity, authority and provenance outside the model-controlled payload.
4. Execute the operation.
5. On a refusal, return its structured fields to the agent; nothing changed.
6. On a decision, commit it at the expected revision before doing anything downstream.
7. Treat emitted events as facts to act on, not as proof that a side effect already happened.

[Connect an agent](../guides/connect-an-agent.md) turns this loop into a tool handler.

## What this does not solve

Entity Runtime does not make model output true, secure a transport, authorize a user or perform an
external action. Authentication decides who the caller is; the shell turns that into trusted
arguments and recording metadata; the kernel evaluates the policy; another component performs
authorized side effects after the commit.
