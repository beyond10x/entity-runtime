---
format: aep.planning-md/3
id: story:service-response-is-checked-against-its-schema
kind: story
status: draft
title: A service response is checked against its declared schema
owner: entity-runtime
relations:
- serves: vision:O2
- informed_by: review-result:er-w2-u1-adversary-pass-1
revision: 1
---
# A service response is checked against its declared schema

## Outcome

A `service/1` branch's response is validated against the operation's declared `response` schema
before the decision is returned, as `docs/design/service-semantics-v0.1.md:708` already says;
a response field stricter than the value it is filled from refuses the decision by name.

## Why

Wave 2's adversary pass (review-result `er-w2-u1-adversary-pass-1`) confirmed a pre-existing gap:
`materialize_response` (`crates/entity-core/src/runtime.rs:1695`) never checks the response, so a
declared `max_length` was not enforced at the base `c07f1c10`, and the new `alphabet` key is not
either. Cases pinning today's behaviour live in `crates/entity-core/tests/adversary_text_alphabet.rs`.

## Acceptance

- Response fields are checked against the declared schema; the two pinned cases are rewritten to
  assert the refusal.
- Stored decisions whose responses would now be refused still replay: replay re-validates the
  stored definition and re-decides (`replay.rs:131`), so the design states whether the check
  applies to replay or only to new decisions, and a test pins it.
- Requirement row and `CHANGELOG.md` line.
