---
format: aep.planning-md/1
id: story:events-carry-what-they-were-decided-on
kind: story
status: draft
title: An event records the arguments the operation was decided on
summary: DomainEvent gains args — what the rules read when they permitted the operation — written by the kernel, checked by replay, hashed into the derived id.
relations:
- decomposes: epic:the-store-an-adopter-runs-on
revision: 3
---
# Story: An event records the arguments the operation was decided on

## Outcome

Reading an event log, a person can see not only that an instance moved from `active` to
`implemented` but what the rules read when they permitted it — the evidence count, the date, the
approver — so *what made this done* is in the log and not only in the shell that asked.

## Context

`DomainEvent` carries `entity, version, id, revision, type, from_state, to_state, changed`
(`crates/entity-core/src/runtime.rs:51-78`). `changed` is the fields the operation wrote (R-89).
The **arguments** — what the world knew, entering as `$args` because the kernel has no clock and no
lookup (invariant 7, R-62) — are not on it. A precondition that read `$args.evidence.test_result >= 1`
leaves an event that cannot say what that count was.

`engineering-protocols` records exactly this beside its events today, by hand: a `MoveStatus`'s
provenance `{"recorded": {"test_result": 1}}` in its journal, and a distinction between a move
**recorded** on evidence and one **asserted** without it. Moving its history onto this event log
(their wave G, `story:history-from-the-event-log`) needs the event to hold it.

## Acceptance

- `DomainEvent` gains `args: Map<String, Value>` — the operation's arguments as presented to
  `execute`, verbatim, after schema validation. A creation event carries the creation arguments.
- The field is written by the kernel, never defaulted, and round-trips through every provider;
  an event missing it is refused when parsed (the same rule as the envelope, R-87).
- `replay` ignores `args` for the fold and **checks** them where a rule would have: a replayed
  history whose event arguments would not have satisfied the operation's preconditions is refused
  (extending R-97), pinned by a test with a forged event whose `args` say `test_result: 0`.
- `event_envelope`'s derived ids include `args` in what they hash, so two events differing only in
  what they were decided on have different identities (R-88).
- `examples/aep/*.yaml` demonstrate it on `story`'s `implement` operation; `entity execute` prints
  it.

## Out of Scope

Anything the kernel would have to *fetch*. What enters as an argument is recorded; what did not enter
is not, and R-62 stands.

## Open Questions

Whether a large argument (a whole document body passed as `$args.body`) is stored in the event or
referenced. Decides: runtime owner. Default if nobody answers: **stored** — an event that references
something the store may not hold is a notification, not a record, which is R-89's own line.
