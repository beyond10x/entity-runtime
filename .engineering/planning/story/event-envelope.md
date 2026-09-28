---
format: aep.planning-md/3
id: story:event-envelope
kind: story
status: implemented
title: A reference event envelope type
summary: event_id, recorded_at, correlation, causation and actor around a DomainEvent — defined outside entity-core, so shells agree on the shape without the kernel touching a clock.
relations:
- derived_from: epic:kernel
- decomposes: epic:the-shell
- depends_on: story:provider-spi
revision: 7
transitions:
- {from: "draft", to: "proposed", at: "2026-08-26T02:42:27Z", actor: "timo", revision: 5, imported: true}
- {from: "proposed", to: "active", at: "2026-08-26T02:42:27Z", actor: "timo", revision: 6, imported: true}
- {from: "active", to: "implemented", at: "2026-08-26T02:42:28Z", actor: "timo", revision: 7, decided_on: {"recorded":{"test_result":1}}, imported: true}
---
# Story: A reference event envelope type

## Outcome

Shells agree on the shape around a `DomainEvent` — `event_id`, `recorded_at`, `correlation`,
`causation`, `actor` — without the kernel touching a clock or an id generator.

## Acceptance

An `Envelope<DomainEvent>` type in a crate outside `entity-core` (the SPI crate, or its own), with
the four-field argument `domain_event.rs` in `engineering-protocols` makes for correlation ≠
causation; `entity execute` gains flags to supply the envelope values and prints enveloped events
when they are given; R-72 stays as stated — the kernel's event carries none of this.
