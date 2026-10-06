---
sidebar_position: 5
title: Guarantees and limits
description: The properties Entity Runtime enforces, the responsibilities it leaves to a trusted shell, and the capabilities it does not claim.
lede: The kernel is deterministic, does no IO and changes nothing on refusal; everything that touches the world belongs to the shell.
source: "AGENTS.md invariants and their tests: crates/entity-core/tests/purity.rs, crates/entity-core/tests/requirements.rs; crates/entity-store/tests/conformance.rs"
---

# Guarantees and limits

These are the guarantees of the 0.27.0 runtime. Its API is still in development. Definition
versions, runtime releases and storage formats are separate compatibility boundaries: a store
written by a newer record format must not be opened by an older build (each format names itself,
and an older reader refuses it by name). The [status page](../status.md) names the test that holds
each shipped capability.

## Kernel guarantees

### No ambient IO

`entity-core` reads no filesystem, network, environment, clock, random source, thread or async
runtime. Its only dependencies are `serde` and `serde_json`. A test scans its sources for those
calls and pins its dependency list, so every fact from the outside world enters as an explicit
input.

### Deterministic decisions

The same validated definition, instance, operation and arguments produce the same `Decision` and
the same serialized bytes. Maps are ordered, and numeric comparisons keep JSON precision instead
of passing through `f64`.

### A refusal changes nothing

Execution borrows the caller's instance and returns a new one only on success. A failed
transition, rule, validation, invariant or template produces no partial instance and no events.

A host may ask for refusals to be recorded: an executor built with `Executor::recording_refusals`
records each kernel refusal, expectation conflict and write to a forked subject before returning
it. On an Eventlog store that is one `er.refused_request` event per distinct refusal on its own
`er.refusal` stream; no subject's revision moves.

### Lifecycle through operations only

Creation enters the declared initial state. Every later state comes from a named operation and a
declared transition; there is no status setter. `EntityInstance` has public, serializable fields
because stores round-trip it, so the host still chooses which instance is canonical. The kernel
refuses an instance that claims a state its definition does not declare.

### Closed definitions

Unknown keys, unknown condition operators, constraints on the wrong field type, rule references
outside their scope and template paths the schema does not declare are refused at registration.
Independent defects are reported together, each with its path.

### Explicit unknowns

Missing evidence is distinct from evidence that contradicts a rule. An unreachable provider is
distinct from an absent entity.

## Storage guarantees

- State and events are one provider commit, not a pair of writes the caller coordinates.
- Expected revisions prevent silent lost updates.
- Recorded decisions carry caller-supplied provenance.
- A record id is idempotent only for identical bytes.
- `replay` re-executes complete decisions and compares the recorded result and events.
- Memory, SQLite and PostgreSQL stores commit an ordered multi-subject batch completely or not at
  all; an Eventlog store publishes each append, one decision or a batch, as one append group. The
  File Store commits one subject document at a time.
- A hybrid store's authority and failure policy have no default anybody forgot to choose.

## What the trusted shell must do

- load canonical definitions and instances;
- authenticate callers and derive their authority;
- supply ids, times, actor, correlation and causation;
- enforce that referenced instances exist, and graph-wide constraints;
- commit an accepted decision before publishing or acting on its events;
- secure transports, credentials, database connections and filesystems; and
- decide retry, escalation and offline behaviour.

## What Entity Runtime is not

- an LLM client, planner, memory system or tool-calling framework;
- an authorization or identity provider;
- a scripting or expression runtime;
- a database server, message bus, search index or blob store;
- a scheduler, a clock or an id generator;
- a side-effect executor; or
- a guarantee that model-supplied facts are true.

`entity-remote` defines a transport-neutral protocol, not an HTTP stack. The `entity` command
stores into a File Store; a build with the `eventlog-providers` feature can also use an Eventlog
File store. SQLite, PostgreSQL, remote, hybrid and the other Eventlog providers are library
integrations. `entity-query` offers containment queries over the Memory and PostgreSQL stores and
the Eventlog facades, not a search service.

:::caution[Planned]

Migrating stored instances between definition versions, a JSON Schema for the definition format,
an `entity explain` command, named reusable predicates and schema fragments, checking a service
response against its declared schema, and serving definitions over HTTP and NATS are planned, not
shipped.

:::

## The security model in one sentence

Treat agent output as a proposal, inject authority and provenance in trusted code, evaluate the
proposal against canonical state, and act only on a decision that was committed.
