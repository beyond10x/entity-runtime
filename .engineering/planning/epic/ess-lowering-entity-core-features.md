---
format: aep.planning-md/3
id: epic:ess-lowering-entity-core-features
kind: epic
status: draft
title: Entity-core features the ESS lowering refuses today
summary: 'increment, cleared, Optional-from-Optional writes, text length and alphabet in the definition language (GitHub #54)'
owner: entity-runtime
refs:
- provider: github
  reference: beyond10x/entity-runtime#54
relations:
- decomposes: initiative:entity-runtime
- serves: vision:O2
revision: 1
---
# Epic: Entity-core features the ESS lowering refuses today

## Outcome

ESS lowers a specification to Entity Runtime definitions (`ess-entity-runtime`). Since ESS 0.53.0
(beyond10x/ess#231) every construct the lowering cannot express is refused by name and listed in
the generated lowerable-subset table. Five of those refusals name entity-core as the missing piece
(GitHub beyond10x/entity-runtime#54). This epic adds each of them to the definition language, so a
specification using them can keep its commands inside the lowered definitions.

| ESS construct | ESS refusal (ess `crates/generate/ess-entity-runtime/src/subset.rs` at `a81a8729d`) | story |
|---|---|---|
| `sets: {field: {increment: n}}` | `INCREMENT`, `subset.rs:299-304`, needs "arithmetic over a stored value" | `story:set-increments-a-numeric-field` |
| `{cleared: true}` in an operation's `sets:` | `CLEARED`, `subset.rs:305-310`, needs "a removal a definition states" | `story:set-clears-an-optional-field` |
| an `updates:` writing an Optional field from an Optional input | `OPTIONAL_UPDATE`, `subset.rs:326-331`, needs "`PresentArgument` on an operation write" | `story:operation-writes-an-optional-field-from-an-optional-argument` |
| `<text>.count` in a guard | `TEXT_COUNT`, `subset.rs:317-325`, needs "the length of a text; `count` reads arrays and maps only" | `story:a-condition-reads-the-length-of-a-text` |
| a String newtype's `alphabet:` | `ALPHABET`, `subset.rs:311-316`, needs "a condition over the characters of a text" | `story:a-text-field-declares-its-alphabet` |

The semantics ESS already decided for the two text constructs are in ESS
`docs/design/string-alphabet-and-length.md`: `.count` counts Unicode scalar values; an alphabet is
a set of characters, membership per scalar value, no normalization, no case folding, the empty text
satisfies every alphabet.

## Acceptance

- Each story lands spec-first: its behaviour is first stated as named scenarios in this
  repository's ESS specification (`ess/ess-inputs.yaml`, core domain), then implemented, and
  `task check`'s `ess-check` executes them.
- Each story adds its requirement rows to `docs/requirements.md`, pinned by live tests
  (`req-check`), and a line under `CHANGELOG.md` `## [Unreleased]`.
- Invariants 1, 2, 5, 7 and 8 of `AGENTS.md` still hold: no IO, same inputs same bytes, every
  reference path checked at registration, no `$now`, and the evaluation order unchanged except
  where a story names the step it extends.
- One Entity Runtime release carries all five; ESS's lowering change that consumes them is ESS
  work after that release and is not part of this epic.

## Out of scope

- Changing `ess-entity-runtime` (the ESS repository).
- The other `Needs::EntityCore` rows in `subset.rs` that #54 does not name.
- Re-pinning consumers (aep, aep-service, atlas, bench). New definition keys and enum variants are a
  0.x minor change; consumers move when they choose.
