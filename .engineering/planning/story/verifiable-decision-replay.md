---
format: aep.planning-md/1
id: story:verifiable-decision-replay
kind: story
status: implemented
title: Replay verifies durable decision records
summary: History records enough normalized input and definition data to rerun decisions without trusting forged events.
relations:
- decomposes: epic:the-store-an-adopter-runs-on
- serves: vision:O2
revision: 5
---
## Context

Event-only replay accepts unknown event types, cannot represent zero or multiple events per decision, and bypasses the rules that produced state.

## Acceptance

A stored decision record can be rerun deterministically against its recorded definition snapshot, and any altered command, transition, field change or event is refused without producing state.

## Implementation evidence

Moved to implemented on 2026-09-09 after an audit of every non-implemented artifact against `CHANGELOG.md`, `docs/roadmap.md` and the crates.

`entity_core::replay` reruns each recorded command against its definition snapshot and refuses an altered command, transition, fields or events (`crates/entity-core/src/replay.rs`); `rehydrate` was hardened further for 0.18.0 (`docs/design/kernel-v0.1.md` § 10.1).
