---
format: aep.planning-md/3
id: story:projection-keys-read-collection-addresses
kind: story
status: draft
title: Projection keys read collection addresses, or refuse them for new definitions only
owner: entity-runtime
relations:
- serves: vision:O2
- informed_by: review-result:er-w1-u1-adversary-pass-2
revision: 1
---
# Projection keys read collection addresses, or refuse them for new definitions only

## Outcome

A `service/1` projection keyed on a collection address (`<array>.count`, `<array>.<n>`,
`<map>.count`) either files each instance under the address's value, or is refused for new
registrations while stored definitions that already use it keep replaying.

## Why

Wave 1's adversary and security passes on `story:a-condition-reads-the-length-of-a-text` confirmed a
pre-existing defect: projection keys are checked with the `service/1` rule walk, which admits these
addresses (R-148, since 0.19.0), but `entity_store::project`'s `key_of`
(`crates/entity-store/src/projection.rs:70-81`) walks object members only, so the read model files
no instance. Refusing the forms at registration (wave 1, correction 1) broke replay of stored
histories, because `replay` re-validates each stored definition snapshot
(`crates/entity-core/src/replay.rs:131`); correction 2 reverted it (review-result
`er-w1-u1-adversary-pass-2`). Nothing in this repository or on aep, aep-service, atlas or bench
`origin/main` calls `project` (security pass 1).

## Acceptance

- Either `key_of` resolves the three address forms (tests per form, red before), or registration of
  a **new** definition refuses them while a stored snapshot that holds them still replays to the
  same bytes (the two adversary cases `…_still_replays` stay green).
- The wave-1 case that pins today's "registers and projects nothing" behaviour is rewritten to the
  new behaviour.
- Requirement row and `CHANGELOG.md` line.

## Out of scope

Text length as a projection key (refused since wave 1; no stored record can hold it).
