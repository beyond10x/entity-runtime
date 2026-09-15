---
format: aep.planning-md/1
id: story:file-store-atomic-groups-from-eventlog
kind: story
status: draft
title: All-or-nothing batches across subject documents in File Store
summary: Close the File Store single-document atomicity limit, reusing eventlog's crates/eventlog-file atomic groups; gated on the Atlas ADR that decides whether entity-store providers sit on eventlog.
refs:
- provider: eventlog
  reference: crates/eventlog-file
relations:
- informed_by: story:replay-from-events
revision: 2
---
## Problem

`README.md` states the limit plainly: `MemoryStore`, `SqliteStore` and `PostgresStore` support
all-or-nothing ordered batches, and "File Store atomicity is limited to one subject document". A
shell that keeps more than one entity consistent on the filesystem therefore has no all-or-nothing
write to reach for, and either invents one or accepts a torn state it cannot detect.

The artifact that closes this already exists in another repository: `eventlog` 0.2.0 ships
`crates/eventlog-file`, a local JSONL provider with versioned transaction frames, process-safe
writer locking, sequence and previous-digest verification and durable referenced blobs before
acknowledgement, and its `CHANGELOG.md` 0.2.0 § Added (lines 26-30) records the process-safe atomic
groups. Its recovery, privacy and operating boundaries are documented in
`eventlog/docs/design/file-provider.md`. `story:replay-from-events` in this store already names
`eventlog` as "the natural store" for event history.

## Gate — do not start this without the decision

This story is **blocked on a decision that is not ours to take**: whether Entity Runtime's store
providers may sit on `eventlog` at all. `atlas/ROADMAP.md` records that arrow as intended with the
decision open and an Atlas ADR required first, and no manifest in this repository names an
`eventlog` crate today (`grep -rn eventlog --include=Cargo.toml .` returns nothing). `eventlog`'s
own `architecture-decision-record:ess-evolution-05-file-and-atomic-groups` asserts the arrow from
its side ("Provider facade migrations happen in ER") while sitting at status `proposed`, which is
one repository deciding another's dependency direction. The Atlas ADR decides it; until it exists,
this story stays `draft` and nothing here is implemented.

The dependency arrow rule in `AGENTS.md` § Boundaries is what makes this a coordinated migration
rather than a local edit.

## Outcome

File Store offers the same all-or-nothing ordered batch the other three providers offer, across
more than one subject document, with the same refusal semantics — and `README.md` stops naming a
limit that no longer exists.

## Acceptance

A batch spanning two subject documents either lands entirely or leaves both documents byte-identical
to their prior contents, proved against a second process interleaving its own writes and against a
crash between the two documents' writes; the existing `entity-store` conformance suite runs against
File Store with the multi-document batch cases the other providers already pass, and the `README.md`
sentence is rewritten in the same change.

## Options this has to choose between

1. Depend on `eventlog-file` and adopt its transaction frames — the reuse this story is named for,
   and the option the Atlas ADR must admit.
2. Implement the same guarantee inside `entity-store` without the dependency — no cross-repository
   decision, and a second implementation of a problem `eventlog` has already solved and proved.

Do not pick one here. The ADR picks one.

## Origin

Org-state review 2026-09-15, lane `02-er-eventlog`, finding F14 (missed-opportunity, note), at
`eaf4309` (tag `0.18.1`).
