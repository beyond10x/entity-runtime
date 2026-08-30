---
format: aep.planning-md/1
id: story:verifiable-decision-replay
kind: story
status: draft
title: Replay verifies durable decision records
summary: History records enough normalized input and definition data to rerun decisions without trusting forged events.
relations:
- decomposes: epic:the-store-an-adopter-runs-on
revision: 1
---
## Context

Event-only replay accepts unknown event types, cannot represent zero or multiple events per decision, and bypasses the rules that produced state.

## Acceptance

A stored decision record can be rerun deterministically against its recorded definition snapshot, and any altered command, transition, field change or event is refused without producing state.
