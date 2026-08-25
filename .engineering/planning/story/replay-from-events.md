---
format: aep.planning-md/1
id: story:replay-from-events
kind: story
status: draft
title: Rehydrate an instance from its events
summary: A fold from an event history to an instance, without opening a second write path to lifecycle_state; eventlog is the natural store.
relations:
- derived_from: epic:kernel
revision: 2
---
# Story: Rehydrate an instance from its events

## Outcome

An instance can be rebuilt from its event history, so a shell may keep the events as the record and
the instance as a cache (R-81, event sourcing).

## Context

R-34 must survive this: replaying `OrderFulfilled` may set the state to `fulfilled` because the
event was produced by an operation that was permitted to, and nothing else may. The fold therefore
needs the events to carry enough to be applied without re-running rules, or the definition to
declare per event what it does to the state. Decide first. `eventlog` is the natural store.

## Acceptance

`rehydrate(definition, events) -> EntityInstance` with a test that a create + n operations, folded
from their events, equals the instance the operations returned; a test that a hand-written event
whose operation the lifecycle does not permit is refused by the fold.
