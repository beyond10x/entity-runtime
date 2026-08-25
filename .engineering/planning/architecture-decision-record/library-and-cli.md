---
format: aep.planning-md/1
id: architecture-decision-record:library-and-cli
kind: architecture-decision-record
status: proposed
title: 'Two surfaces: a library crate and a CLI that is the reference shell'
summary: entity-core is the product; the entity command is the shell around it, with clap derive and three exit codes.
relations:
- decides: epic:kernel
revision: 2
---
# ADR: Two surfaces — a library crate and a CLI that is the reference shell

## Status

Proposed 2026-08-25, on the operator's instruction that the system offer a Rust library crate and a
CLI layer. Implemented in 0.1.

## Context

The proof of concept had a hard-coded demo binary. A runtime nobody can call from a shell is not
adoptable by an agent harness or a script; a runtime that is only a binary is not embeddable.

## Decision

`entity-core` is the product, a library with a documented public API. `entity-cli` builds the
`entity` command with clap's derive API — `validate`, `inspect`, `graph`, `create`, `execute` — and
is the reference *shell*: all IO is there, identifiers come from the caller, and a `Decision` it
prints is accepted back as the next `--instance`. Exit codes: `0` decided, `1` refused (the typed
refusal as JSON on stdout), `2` invalid invocation.

## Alternatives

* **One crate with a `cli` feature** — fewer crates, but the kernel's dependency list would carry
  `clap` and `serde_yaml` behind a flag, and the purity test's manifest check would need exceptions.
  Rejected.
* **A daemon/HTTP surface first** — needs a store to be useful and drags IO into the first
  release. Deferred behind `story:provider-spi`.

## Consequences

Three crates, one workspace lint set. The YAML adapter is its own crate so the kernel never sees
`serde_yaml`. Every CLI verb that later needs a clock reads it in the CLI and passes it in.
