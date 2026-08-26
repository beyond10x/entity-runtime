---
format: aep.planning-md/1
id: story:hybrid-provider
kind: story
status: implemented
title: A composite over a local provider and a remote one
summary: One Store implementation over two, whose behaviour is entirely the declared policy.
relations:
- decomposes: epic:centralized-and-hybrid-storage
revision: 4
---
# Story: A composite over a local provider and a remote one

## Outcome

Somebody who wants a local copy for speed and a server for truth gets both behind one `Store`, and
the caller above it does not change.

## Context

`entity-store` gives the traits and `entity-remote` gives a second implementation of them. The
composite is what makes the pair useful — and it is the place where the four policy words from
`story:authority-and-conflict` are actually spent.

## Acceptance

`entity-hybrid` (in `entity-remote`) implements `Store` over a local `Store` and a remote one; it
passes the conformance suite with the remote as authority and with the local store as authority; a
stale answer is only ever served when the policy says so, and the answer says it was stale at the
point of use. R-106, R-107.

## Out of Scope

Background replication, a sync daemon, and any scheduling. The composite acts when it is called.

## Open Questions

None outstanding.
