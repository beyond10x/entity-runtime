---
format: aep.planning-md/1
id: story:the-store-keeps-the-envelope
kind: story
status: draft
title: The store keeps the envelope, not only the event
summary: Recording seals an event with recorded_at/correlation/causation/actor and every provider then stores the bare DomainEvent; an adopter that needs who/when durable has to smuggle the seal into payload.
relations:
- decomposes: epic:the-store-an-adopter-runs-on
revision: 3
---
# Story: The store keeps the envelope, not only the event

## Outcome

A second process reading a store can say who recorded an event, when, in which flow and what caused
it — from the store, through the SPI, without the shell having invented a place to hide those four
values.

## Context

**A finding from the first adopter's wave F, story 3 (`engineering-protocols`
`story:events-reach-the-store`, 2026-08-28), recorded here so it is not re-discovered.**

`Recording::seal` (R-86) wraps each `DomainEvent` in an `Envelope` carrying `event_id`,
`recorded_at`, `correlation`, `causation` and `actor`. Then:

- `Store::commit` takes a `Decision`, whose `events` are bare `DomainEvent`s
  (`crates/entity-store/src/lib.rs`, `Decision` in `entity-core`).
- `SqliteStore::commit` writes `decision.events` (`crates/entity-sqlite/src/lib.rs`, the events
  loop); `FileStore` and `MemoryStore` the same.
- `entity-cli execute --store … --correlation …` prints the sealed envelopes and commits the
  decision (`crates/entity-cli/src/main.rs`, the `Execute` arm): the seal goes to stdout and the
  store never sees it.

So none of the four values R-86 says a log needs reaches a provider. The adopter, needing *who moved
a story and when* to be readable by a second process, wrote the sealed envelope's fields into the
event's `payload` — which works, and is a shell inventing the place the SPI should have.

## Acceptance

- A provider stores what `Recording::seal` produces, or the SPI gives an envelope a first-class place
  in `commit`, and `EventProvider` can read it back. Which of the two is the runtime owner's
  decision; the story is that today there is neither.
- `entity-cli execute --store` with the envelope flags commits the sealed events, not the bare ones.
- The conformance suite has a case: an event committed sealed reads back with its seal.
- `docs/requirements.md` gains the row, `store-v0.1.md` § 6 the paragraph.

## Out of Scope

What an event was *decided on* — that is `story:events-carry-what-they-were-decided-on`, a field on
the `DomainEvent` itself. This story is about the four values that already exist and do not land.

## Open Questions

Envelope on the wire (`Ask::Commit`) and in the SQLite `events` table: a column per field, or the
sealed JSON as the stored document? Decides: runtime owner. Default if nobody answers: the sealed
`Envelope<DomainEvent>` is the stored document, since it is already the shape `seal` produces and
`deny_unknown_fields` round-trips it.
