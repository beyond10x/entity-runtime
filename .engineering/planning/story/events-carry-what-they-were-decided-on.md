---
format: aep.planning-md/1
id: story:events-carry-what-they-were-decided-on
kind: story
status: implemented
title: An event records the arguments the operation was decided on
summary: DomainEvent gains args — what the rules read when they permitted the operation — written by the kernel, checked by replay, hashed into the derived id.
relations:
- decomposes: epic:the-store-an-adopter-runs-on
revision: 7
---
# Story: An event records the arguments the operation was decided on

## Outcome

Reading an event log, a person can see not only that an instance moved from `active` to
`implemented` but what the rules read when they permitted it — the evidence count, the date, the
approver — so *what made this done* is in the log and not only in the shell that asked.

## Context

`DomainEvent` carried `entity, version, id, revision, type, from_state, to_state, changed`
(`crates/entity-core/src/runtime.rs`). `changed` is the fields the operation wrote (R-89). The
**arguments** — what the world knew, entering as `$args` because the kernel has no clock and no
lookup (invariant 7, R-62) — were not on it. A precondition that read `$args.evidence.test_result >= 1`
left an event that could not say what that count was.

`engineering-protocols` recorded exactly this beside its events by hand: a `MoveStatus`'s
provenance `{"recorded": {"test_result": 1}}` in its journal, and since its 0.28.0 in the event's
`payload` under `decided_on`. Moving its history onto this event log (their wave G,
`story:history-from-the-event-log`) needs the event to hold it.

## Acceptance

- `DomainEvent` gains `args: Map<String, Value>` — the operation's arguments as presented to
  `execute`, verbatim, after schema validation. A creation event carries the creation arguments.
  **Done** — `materialize_event` takes them from `execute`'s validated arguments and from
  `create`'s validated fields; `an_event_records_the_arguments_it_was_decided_on_and_a_creation_records_its_fields`.
- The field is written by the kernel, never defaulted, and round-trips through every provider;
  an event missing it is refused when parsed (the same rule as the envelope, R-87). **Done** — no
  `#[serde(default)]`; `an_event_missing_its_arguments_is_refused_when_parsed`; every provider
  round-trips the whole `DomainEvent` as JSON, and the provider suite passes on all of them.
- `replay` ignores `args` for the fold and **checks** them where a rule would have: a replayed
  history whose event arguments would not have satisfied the operation's preconditions is refused
  (extending R-97), pinned by a test with a forged event whose `args` say `test_result: 0`. **Done**
  — `arguments_refused` in `replay.rs` finds every operation that declares the transition and emits
  the event type and evaluates its preconditions against the event's arguments and the fields as
  they stood; `a_replayed_event_whose_arguments_would_not_have_satisfied_the_preconditions_is_refused`
  covers the forged `0` and the unobservable `{}`. Guard verified by breaking it: with the check
  disabled the test fails; restored, it passes.
- `event_envelope`'s derived ids include `args` in what they hash, so two events differing only in
  what they were decided on have different identities (R-88). **Done** —
  `<entity>:<id>@<revision>#<index>~<fnv1a-64 of the arguments' canonical JSON>`, hand-rolled
  rather than a hashing crate; `two_events_differing_only_in_what_they_were_decided_on_have_different_identities`.
- `examples/aep/*.yaml` demonstrate it on `story`'s `implement` operation; `entity execute` prints
  it. **Done** — `implement` already costs `$args.evidence.test_result >= 1`;
  `implement_records_the_evidence_it_was_decided_on` drives a story from `create` to `implemented`
  through the CLI and reads `args.evidence.test_result == 1` off the printed event.

R-110 added; R-88 and R-97 extended; `kernel-v0.1.md` § 10.1 and `store-v0.1.md` § 6 carry the
reasoning. Released as 0.11.0.

## Decision taken

The open question's default: a large argument is **stored** on the event, not referenced — an event
that references something the store may not hold is a notification, not a record (R-89's own line).

## Out of Scope

Anything the kernel would have to *fetch*. What enters as an argument is recorded; what did not enter
is not, and R-62 stands.

## Open Questions

None outstanding; the one that was open is decided above.
