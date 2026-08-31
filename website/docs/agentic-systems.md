---
sidebar_position: 2
title: Why agents need deterministic authority
description: Keep agent intent flexible while making state transitions, refusals, and evidence reproducible.
---

# Why agents need deterministic authority

An agent can understand a customer message, gather evidence, and propose a refund. Those are useful
probabilistic tasks. Whether a refund is allowed is a different kind of problem.

If policy lives only in a prompt or tool description, every call asks a language model to
reinterpret it. If the tool accepts arbitrary updates, one mistaken call can skip a lifecycle,
overwrite newer work, or leave state changed without evidence explaining why.

Entity Runtime separates the two jobs:

<img
  src="/entity-runtime/img/agent-boundary.svg"
  alt="An agent proposes an operation and arguments. A trusted shell adds canonical state, the validated definition, identity, authority, timestamps, and the expected revision. Entity Runtime returns either a decision to record atomically or a typed refusal that changes nothing."
  loading="eager"
/>

## What the agent may choose

Give the agent the operations and domain arguments it is allowed to propose. For a refund that may
be `submit`, `approve`, or `reject`, plus a reason based on the evidence it gathered.

Do not let the model choose the authority around the proposal. A trusted shell should load:

- the validated definition version;
- the canonical current instance;
- the subject identity;
- actor identity and authority role;
- timestamps, record IDs, correlation, and causation; and
- the storage expectation used to detect a concurrent writer.

In the refund example, the agent may propose approval. The shell supplies `actor_role: agent` from
its authenticated execution context. A refund above the declared threshold is then refused. The
model cannot become a human approver by saying that it is one.

## Why this is stronger than prompt instructions

### The available actions are discoverable

`entity inspect` and `entity graph` expose the states, operations, arguments, and relationships a
definition actually declares. The generated [CLI skill](./guide/agent-integration#install-the-cli-skill)
teaches an agent how to invoke the installed version without copying a stale command summary into
every repository.

### Policy is evaluated, not interpreted

Conditions are a closed data AST. There are no callbacks, lookups, clocks, loops, or hidden helper
functions inside a rule. The same inputs produce the same decision and the same serialized bytes.

### Missing evidence is not silently false

Rules distinguish `false` from `unknown`. If a policy asks whether a review score is at least four
and no score was observed, the result is `precondition_unobservable`, naming every missing path.
The agent can gather evidence or escalate instead of treating absence as rejection.

### Refusals are part of the tool contract

Exit `1` means the request was understood and refused. The JSON result names the operation, state,
rule, reason, or unresolved facts. An agent can revise its proposal based on structured data rather
than scraping error prose.

### Accepted changes carry their proof

A recorded decision preserves what was asked, which definition decided it, what changed, and which
events resulted. Actor and time enter at the trusted edge. Replay detects altered input, output, or
event evidence.

## A safe agent loop

1. Load the canonical instance and the allowed definition in trusted code.
2. Expose a tool whose input is a constrained operation name and its domain arguments.
3. Inject identity, authority, and provenance outside the model-controlled payload.
4. Execute the operation.
5. On a typed refusal, return its structured fields to the agent and change nothing.
6. On a decision, atomically record it at the expected revision before performing downstream work.
7. Treat emitted events as facts to act on, not proof that an external side effect already happened.

## What this does not solve

Entity Runtime does not make model output true, secure a transport, authorize a user, or execute an
external action. It makes the transition boundary explicit and testable. Authentication determines
who the caller is; the shell converts that identity into trusted arguments and recording metadata;
the runtime evaluates policy; another component performs authorized side effects.

Continue with [Connect an agent safely](./guide/agent-integration) or run the
[quickstart](./guide/getting-started).
