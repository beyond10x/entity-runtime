---
format: aep.planning-md/1
id: story:remote-provider
kind: story
status: implemented
title: A provider that talks to a server, with the network in the shell and nowhere else
summary: A versioned, transport-agnostic protocol; the caller supplies the transport, so this repository opens no socket.
relations:
- decomposes: epic:centralized-and-hybrid-storage
revision: 4
---
# Story: A provider that talks to a server

## Outcome

A team whose record of truth is a server can use this runtime against it, and find out when the
server did not answer — rather than being told the instance does not exist.

## Context

Every provider so far is local. The moment one is not, a failure mode appears that has no local
equivalent: silence. A store that cannot be reached must not answer *absent*, because *absent* is a
fact about the data and silence is a fact about the network, and confusing them is how a
synchronisation deletes something.

## Acceptance

`entity-remote` implements `Store` over a `Transport` the caller supplies; a store that could not be
reached answers `Unreachable`, never absent; the protocol is versioned and a request at a wire
version this build does not know is refused **by name**; a conflict crosses the wire as a conflict
and not as a generic failure; and a remote store passes the conformance suite like a local one.
R-104, R-105.

## Out of Scope

**An HTTP client.** `Transport` is the caller's to implement, so nothing here opens a socket, the
gate never touches a network, and `LoopbackTransport` is labelled in its own documentation as
standing in for exactly that. A server implementation, authentication and authorisation are all
somebody else's — this is the client half of a protocol, not a product.

## Open Questions

None outstanding.
