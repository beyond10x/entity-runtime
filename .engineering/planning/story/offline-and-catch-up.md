---
format: aep.planning-md/1
id: story:offline-and-catch-up
kind: story
status: implemented
title: The local side works with the remote unreachable, and reconciles when it returns
summary: catch_up replays what the authority holds now, keeps what it could not replay, and merges nothing.
relations:
- decomposes: epic:centralized-and-hybrid-storage
revision: 4
---
# Story: The local side works with the remote unreachable

## Outcome

Somebody on a laptop with no network keeps working, and when the network returns they are told what
reconciled and what did not — rather than finding out later that it silently did not.

## Context

This is the story that makes the hybrid worth having, and the one where an over-helpful
implementation does the most damage. A catch-up that merges is a catch-up that can quietly pick the
wrong value; a catch-up that reports success while leaving work behind is worse.

## Acceptance

A laptop that wrote while the replica was down catches up when it returns; `catch_up` replays what
the authority holds **now**, not what it held when the divergence was recorded; it **keeps** what it
could not replay rather than reporting success; and it merges nothing — a divergence that comes back
as a conflict stays outstanding for a person. R-108.

## Out of Scope

Conflict resolution of any kind, automatic retry schedules, and any notion of a background sync.

## Open Questions

How an operator is shown outstanding divergences is a CLI question, not a store one, and is not
answered here.
