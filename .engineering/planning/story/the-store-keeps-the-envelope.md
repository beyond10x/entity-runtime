---
format: aep.planning-md/1
id: story:the-store-keeps-the-envelope
kind: story
status: draft
title: The store keeps the envelope, not only the event
summary: Recording seals an event with recorded_at/correlation/causation/actor and every provider then stores the bare DomainEvent; an adopter that needs who/when durable has to smuggle the seal into payload.
relations:
- decomposes: epic:the-store-an-adopter-runs-on
revision: 4
---
# Story: The store keeps the decision envelope

## Outcome

A second process reading a store can identify the exact decision that changed state, who recorded it, when, in which flow and what caused it, without the shell hiding provenance in event payloads.

## Context

The first adopter showed that `Recording::seal` was printed by the CLI while every provider committed the bare `Decision`. That loses the evidence at the persistence boundary. Event-only storage also cannot distinguish a decision that emitted zero events from no decision, or group several events at one revision without provider-specific behavior.

The 0.15.0 correction makes the persisted unit a `RecordedCommit`: resulting instance plus an `Envelope<DecisionRecord>`. The record contains the normalized command, definition snapshot, transition, changed fields and ordered nested events. `RecordedObservation` is the separate append-only shape for evidence that does not change revision.

## Acceptance

Every provider commits and reads the complete recorded decision envelope, stored create and execute print the exact object committed, same-revision observations remain distinct and ordered, and the shared conformance suite proves retry identity is based only on the caller-supplied record id and complete byte equality.

## Compatibility

This changes provider storage and Remote Store bytes, so it ships only in coordinated release 0.15.0 under the Atlas migration ADR. Existing File Stores cross the boundary only through the explicit out-of-place v1-to-v2 command; database migrations retain old material behind an unverified snapshot marker rather than inventing missing provenance.
