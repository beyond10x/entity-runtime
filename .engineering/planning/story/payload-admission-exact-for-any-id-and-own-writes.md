---
format: aep.planning-md/3
id: story:payload-admission-exact-for-any-id-and-own-writes
kind: story
status: draft
title: Payload admission holds for any event-id length and after own writes
summary: 'A2 follow-up: the bound assumes 36-character event ids; the held estimate grows after own writes until the next capture'
owner: entity-runtime
revision: 1
---
## Outcome

The payload-byte admission of the Eventlog adapter neither assumes a 36-character event id nor
lets a handle's held estimate grow past the exact count after its own writes, so a write that fits
`max_payload_bytes` is admitted and one that does not is refused on any provider and after any
number of own writes.

## Finding

Follow-up of review A2 (Entity Runtime 0.32.0: "Entity Runtime can append beyond its own Eventlog
read budget"). Decision A2 = B, count in `entity-eventlog` (Timo 03:16:30 CET 2026-11-22). Master
log `assignment.md` line 120: "Draft story: bound assumes 36-char event ids; held estimate grows
after own writes until next capture."

## Evidence

- `0a661071` "fix(eventlog): refuse writes past the payload-byte cap" (merged in #93).
- Row bound uses "the widest physical reference: a 36-character event id, the UUID every Eventlog
  provider mints, and u64::MAX positions" (commit body); code `widest_physical`,
  `crates/entity-eventlog/src/adapter.rs:4657-4659` (`event_id: "0".repeat(36)`). A provider that
  mints a longer id makes the bound a lower bound, and a write can pass the cap.
- Estimate drift, measured on SQLite (commit body): one-subject write adds 5,312 payload bytes,
  admitted at a bound of 5,445 (+133); two-subject import adds 6,546, bound 6,774 (+228). "After its
  own write a FullVerification handle holds 6041 estimated against 5908 exact." The difference
  accumulates per own write until the next capture recounts it.
- A refused estimate is rechecked against a fresh observation (`capture_model`) before refusing
  (`crates/entity-eventlog/src/adapter.rs:1561`), so drift costs a capture per refusal near the
  cap rather than a wrong refusal.
- Tests: `adapter::payload_tests`, e.g.
  `an_estimated_payload_is_counted_exactly_before_it_refuses_a_write`.

## Acceptance

A test with a provider whose event ids are longer than 36 characters refuses a write that takes the
store past `max_payload_bytes`, and a test that performs 32 own writes on one handle shows the held
payload equal to a fresh capture's exact count after each write.
