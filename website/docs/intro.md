---
slug: /
sidebar_position: 1
title: What Entity Runtime does
description: A deterministic authority for lifecycle-governed state changes proposed by people, services, and AI agents.
---

# Let agents propose. Let deterministic rules decide.

An AI agent is good at interpreting a request and choosing what to try. It should not be the final
authority on whether durable state may change.

Entity Runtime puts a small, deterministic decision boundary between a proposal and the systems
that keep or act on it. You declare an entity type as data:

- which fields it carries;
- which lifecycle states exist;
- which named operations can move it;
- which rules must hold;
- which fields an operation changes; and
- which events an accepted operation emits.

The runtime evaluates one explicit equation:

<img
  src="/entity-runtime/img/decision-equation.svg"
  alt="A definition, current instance, named operation, and arguments enter Entity Runtime. The deterministic result is either a complete decision or a typed refusal."
  loading="eager"
/>

The model can propose `approve` with a reason. The runtime decides whether `approve` exists, whether
it is legal from the current state, whether the arguments are valid, and whether every policy rule
holds. A refusal returns without changing the caller's instance or emitting an event.

## A boundary, not an agent framework

Entity Runtime does not call a model, plan tasks, choose tools, authenticate users, read a clock,
mint identifiers, publish messages, or perform the side effect an event describes. Your trusted
shell does those things. The runtime answers the narrower question that must stay predictable:

> Given exactly these facts, is this state change valid, and what is the complete result?

That separation lets the probabilistic part of a system remain flexible without making its durable
state probabilistic too.

## What you get

### Policy outside the prompt

Lifecycle and policy are versioned data, not prose that every prompt and tool handler must interpret
again. Misspelled keys, impossible transitions, invalid references, and inconsistent defaults are
refused when a definition is registered.

### Operations instead of patches

An agent asks for a named operation such as `submit`, `approve`, or `reject`. It does not write
`state: approved` or patch arbitrary fields. The definition owns the legal path.

### Structured refusals

Failures are typed: `invalid_transition`, `precondition_failed`, `validation`,
`precondition_unobservable`, and more. Programs match the kind and fields; people get a readable
message. A refusal changes nothing.

### Replayable evidence

An accepted decision records the normalized command, validated definition snapshot, result,
changed fields, and events. A trusted shell can add actor, time, correlation, and causation before
storing the record. Replay reruns the decision instead of trusting the stored answer.

### Storage without silent overwrites

Providers store state and events together and compare the revision the caller expected. File,
memory, SQLite, PostgreSQL, remote, and hybrid options serve different deployment boundaries without
pulling IO into the kernel.

## Model coverage and release status

This guide targets [0.17.7](https://github.com/beyond10x/entity-runtime/releases/tag/0.17.7).
The API remains in development. Entity definitions drive execution and generated domain interfaces;
the runtime itself has authored Rust subsystem contracts, not a whole-system ESS specification.
The [system model and derivation map](/docs/system-model) shows command, event, provider, and query
coverage and distinguishes generated surfaces from handwritten command handling.

## Where to start

- [Why this matters for agentic systems](/docs/agentic-systems)
- [Run the refund quickstart](/docs/guide/getting-started)
- [Model your own policy](/docs/guide/modeling)
- [Connect an agent safely](/docs/guide/agent-integration)
