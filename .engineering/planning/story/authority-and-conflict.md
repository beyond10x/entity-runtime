---
format: aep.planning-md/1
id: story:authority-and-conflict
kind: story
status: implemented
title: Which side wins, declared per entity type, and what a losing write becomes
summary: Policy is four required words with no default; a losing write is recorded as a divergence rather than swallowed.
relations:
- decomposes: epic:centralized-and-hybrid-storage
revision: 4
---
# Story: Which side wins, declared per entity type

## Outcome

Nobody discovers which copy of their data won by comparing the two afterwards. The answer is a word
somebody typed before the first write, and a write that lost is a record they can act on.

## Context

A composite over two stores has to answer four questions — who is authoritative, which side is read,
what happens when the remote is unreachable, and what happens when the two diverge. A default for
any of them is a policy nobody chose, applied to somebody's data. This story is the decision that
`Policy` has no `Default` implementation.

## Acceptance

`Policy::new(authority, read_path, when_unreachable, on_divergence)` takes all four as required
arguments and there is no `Default`; with the remote as authority, a refused remote write never
reaches the local copy; refusing on divergence lets no write stand unreplicated; with the local
store as authority, a losing replica write is **recorded as a `Divergence`**, not swallowed. R-106,
R-107.

## Out of Scope

Merging. A divergence is recorded and surfaced; nothing here decides which of two conflicting
values is right, because that is a question about the domain and this crate does not have one.

## Open Questions

None outstanding.
