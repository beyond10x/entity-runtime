---
sidebar_position: 12
title: Guarantees and limits
description: The properties Entity Runtime enforces, the responsibilities it leaves to a trusted shell, and the capabilities it does not claim.
---

# Guarantees and limits

Entity Runtime is useful because its boundary is narrow enough to state precisely.

## Kernel guarantees

### No ambient IO

The kernel reads no filesystem, network, environment, clock, random source, thread, or async runtime.
Its runtime dependencies are `serde` and `serde_json`. Every fact from the outside world enters as
an explicit input.

### Deterministic decisions

The same validated definition, instance, operation, and arguments produce the same `Decision` and
the same serialized bytes. Maps are ordered and numeric comparisons preserve JSON precision.

### Refusal changes nothing

Execution borrows the caller's instance and returns a new one only on success. A failed transition,
rule, validation, invariant, or template produces no partial instance and no events.

### Lifecycle through operations only

Creation enters the declared initial state. Every later lifecycle change comes from a named
operation and declared transition. There is no status setter.

### Closed definitions

Unknown keys, unknown condition operators, invalid constraint/type combinations, inaccessible rule
references, and impossible template paths are refused. Independent definition and value defects are
accumulated with paths.

### Explicit unknowns

Missing evidence is distinct from evidence that contradicts a rule. An unreachable provider is
distinct from an absent entity.

## Storage guarantees

- State and events are one provider commit, not a caller-managed pair of writes.
- Expected revisions prevent silent lost updates.
- Complete recorded decisions carry caller-supplied provenance.
- Record IDs are idempotent only for identical bytes.
- Replay reruns complete decisions and compares their evidence.
- Memory, SQLite, and PostgreSQL support all-or-nothing ordered batches.
- Hybrid authority and failure policy have no default somebody forgot to choose.

## Trusted-shell responsibilities

The surrounding application must:

- load canonical definitions and instances;
- authenticate callers and derive authority;
- supply IDs, times, actor, correlation, and causation;
- enforce existence and graph-wide constraints for typed references;
- commit accepted decisions before publishing or acting on events;
- secure transport, credentials, database connections, and filesystems; and
- decide retry, escalation, and offline behavior.

## Deliberate non-capabilities

Entity Runtime is not:

- an LLM client, planner, memory system, or tool-calling framework;
- an authorization or identity provider;
- a scripting or expression runtime;
- a database server or message bus;
- a search index or blob store;
- a scheduler, clock, or ID generator;
- a side-effect executor; or
- a guarantee that model-supplied facts are true.

The `entity-remote` crate supplies a transport-neutral protocol, not an HTTP stack. The CLI uses the
File Store; SQLite and PostgreSQL are library integrations. Definition migration between arbitrary
versions, search/blob providers, and an `explain` command are not shipped capabilities.

## Security model in one sentence

Treat agent output as a proposal, inject authority and provenance in trusted code, evaluate the
proposal against canonical state, and act only on a decision that was committed successfully.
