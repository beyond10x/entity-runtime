---
slug: /
sidebar_position: 1
title: Entity Runtime
description: A deterministic Rust kernel that decides lifecycle-governed state changes from entity definitions declared as data.
---

# Entity Runtime

Entity Runtime decides whether a proposed state change is legal. You declare an entity type as
data — its fields, lifecycle states, named operations, rules and events — and a deterministic Rust
kernel answers each request with the complete next decision or a typed refusal:

<img
  src="/entity-runtime/img/decision-equation.svg"
  alt="A definition, current instance, named operation, and arguments enter Entity Runtime. The deterministic result is either a complete decision or a typed refusal."
  loading="eager"
/>

An application, a person or an AI agent asks for a named operation such as `approve`. The kernel
checks that the operation exists, that it is legal from the current state, that the arguments fit
their schema and that every rule holds. A refusal changes nothing and emits no event.

It ships as a workspace of Rust libraries and one command, `entity`. The current release is
[0.27.0](https://github.com/beyond10x/entity-runtime/releases/tag/0.27.0); the API is still in
development and a minor release may change it.

## What it does

- **Decides from data.** Definitions are YAML or JSON documents with closed keys. Rules are a
  closed condition language with `true`, `false` and `unknown` results; there is no embedded code.
- **Refuses with a reason.** Every refusal has a `kind` a program can match: `invalid_transition`,
  `precondition_failed`, `precondition_unobservable`, `validation` and more
  ([typed refusals](./reference/refusals.md)).
- **Records what it decided.** A decision carries the normalized command, the definition snapshot,
  the changes and the events, so a stored decision can be replayed and checked
  ([storage and replay](./concepts/storage.md)).
- **Keeps IO at the edges.** The kernel, `entity-core`, reads no clock, file, network or random
  source. Storage lives in provider crates: File Store, SQLite, PostgreSQL, a remote protocol and
  Eventlog-backed recorded stores.
- **Projects one model into many surfaces.** The same definitions drive lifecycle graphs, entity
  documentation with OpenAPI and AsyncAPI contracts, MCP tools and a generated Rust command.

## What it does not do

Entity Runtime does not call a model, plan tasks, authenticate anyone, read a clock, mint
identifiers, publish messages or perform the side effect an event describes. The application
around it — the *trusted shell* — does those things and hands the kernel the facts as data. It is
not a hosted service, a database server, a message bus, a workflow engine or a scripting runtime.
[The decision boundary](./concepts/decision-boundary.md) explains why that split matters when the
caller is an agent, and [guarantees and limits](./concepts/guarantees.md) lists what is promised.

## Where it sits

- [ESS](https://beyond10x.github.io/ess/) ([GitHub](https://github.com/beyond10x/ess)) specifies
  Entity Runtime: `entity-core`, `entity-store`, `entity-executor`, `entity-shell` and
  `entity-query` are held to executable ESS contracts that `task check` runs against the real
  libraries. ESS also lowers one component of a specification to Entity Runtime definitions.
- [AEP](https://beyond10x.github.io/ecosystem/aep/) ([GitHub](https://github.com/beyond10x/aep))
  uses Entity Runtime: the lifecycle of each kind of planning artifact is an entity definition,
  and `entity-core` decides every move between its states.
- [Eventlog](https://beyond10x.github.io/ecosystem/eventlog/)
  ([GitHub](https://github.com/beyond10x/eventlog)) is the append-only storage under
  `entity-eventlog`, which keeps complete recorded state on its file, SQLite, PostgreSQL and tree
  providers.

## Start here

1. [Getting started](./getting-started.md): install `entity` and watch a rule refuse an agent's
   refund approval.
2. [Model policy as data](./guides/model-policy-as-data.md): turn your own domain into a
   definition.
3. [Connect an agent](./guides/connect-an-agent.md): let a model propose operations without
   letting it choose its own authority.
4. [Status](./status.md): what is shipped today and what is planned.
