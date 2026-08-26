---
format: aep.planning-md/1
id: epic:centralized-and-hybrid-storage
kind: epic
status: implemented
title: Centralized storage, and hybrid storage that declares its own rules
summary: A store whose record of truth is a server, and a composite over a local store and a remote one whose authority, read path, unreachable behaviour and divergence behaviour are declared rather than defaulted. The network stays in the shell.
relations:
- decomposes: initiative:entity-runtime
revision: 4
---
# Epic: Centralized and hybrid storage

## Outcome

A team whose record of truth is a server can use this runtime against it, and a person working
offline against a local copy of that server knows — as a matter of declared policy rather than of
implementation detail — which copy wins, what happens when the server is silent, and what became of
a write that lost.

## Why Now

The shell made storage a trait with three local implementations. The first question any adopter asks
next is whether the store can be somewhere else, and the honest answer needs the two failure modes
that only appear across a network: silence, and two copies that disagree. Both are cheap to get
wrong now and expensive to change once somebody's data is in it.

## Scope

`entity-remote` — a versioned, transport-agnostic protocol and a `Store` over it — and the hybrid
composite, with `Policy` as four required words and no default. Catch-up: replaying what the
authority holds when the remote returns.

## Out of Scope

**A network client.** `Transport` is the caller's, so the gate never touches a network and
`LoopbackTransport` says in its own documentation that it stands in for one. A server
implementation, authentication, authorisation, and any form of merge — a divergence is recorded and
handed to a person, because choosing between two conflicting values is a question about a domain
this crate does not have.

## Risks

The `Default` that never gets written. A policy type with a default is a policy nobody chose applied
to somebody's data, and the pressure to add one for ergonomics will not go away — which is why its
absence is a requirement (R-106) and not a convention. The second risk is a catch-up that reports
success while leaving work behind; it is closed by a test that asserts what it kept.

## Done When

A remote that did not answer is `Unreachable` and never absent; a hybrid passes the conformance
suite under both authorities; a losing write is a recorded divergence; and `catch_up` replays what
the authority holds now, keeps what it could not, and merges nothing.
