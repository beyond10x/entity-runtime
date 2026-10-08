---
format: aep.planning-md/3
id: story:definitions-served-over-http-and-nats
kind: story
status: draft
title: Registered definitions served over HTTP and NATS
owner: entity-runtime
scope:
- confidence: inferred
  path: Cargo.toml
- confidence: cited
  path: crates/entity-runtime-docs/src/status.rs
- confidence: inferred
  path: crates/entity-shell
- confidence: inferred
  path: crates/entity-surface
- confidence: cited
  path: docs/plan/next-wave-the-shell.md
- confidence: inferred
  path: ess/components
- confidence: inferred
  path: ess/domains
- confidence: inferred
  path: ess/ess-inputs.yaml
- confidence: inferred
  path: website/data/status.json
- confidence: cited
  path: website/docs/concepts/guarantees.md
- confidence: inferred
  path: website/docs/status.md
revision: 3
---
# Registered definitions served over HTTP and NATS

## Outcome

Recorded from `docs/plan/next-wave-the-shell.md:120-127`, the only unshipped item the 2026-10-06
documentation overhaul found in `docs/plan/`: "register schemas … get an API" over HTTP and NATS.
No crate or story held it. This draft keeps the idea in the store; it is not accepted work.

## Acceptance

Not written. Before this story can leave draft, it needs an owner, a consumer that asks for it, and
an ESS model of the served surface (AGENTS.md: a new noun gets its ESS domain first).

## Out of scope

Everything until then.
