---
format: aep.planning-md/1
id: story:store-enumeration
kind: story
status: implemented
title: A provider can say what it holds
summary: StateProvider::ids — every identity a store holds for an entity type — so a shell can hydrate from a store it did not write; every provider, the suite, Broken, and entity list.
relations:
- decomposes: epic:the-store-an-adopter-runs-on
revision: 7
---
# Story: A provider can say what it holds

## Outcome

A shell can open a store it did not write and rebuild from it. Before this it could ask about an
instance it already knew the id of, and nothing else.

## Context

`Store` was `load`, `revision_of`, `events`, `commit` (`crates/entity-store/src/lib.rs`). Every
question needed `(entity, id)`. A shell hydrating a process from a populated store had no id to ask
with — which is why `engineering-protocols`' `aep-backend-sqlite` refused a row it did not write and
documented *"point this at an empty database until then"*. `entity-cli --store` had the same gap one
step earlier: it could `create` and `execute` but could not list.

Needed by the adopter's wave F (`story:sqlite-hydrates-on-open`).

## Acceptance

- `StateProvider` gains `ids(&self, entity: &str) -> Result<Vec<String>, StoreError>` — every
  identity the store holds for that entity type, sorted, so two calls and two providers agree
  byte for byte (invariant 2). **Done** — required, not defaulted; the doc on the method says why.
- Every provider implements it: `MemoryStore`, `FileStore`, `SqliteStore`, `RemoteStore` (a new
  `Ask` variant — and therefore `entity.store/3`, refused by name to a `/2` peer), `Hybrid`
  (through its read path; an unreachable authority is `Unreachable`, never an empty list). **Done**
  — `Ask::Ids`/`Answer::Ids`, `WIRE_VERSION = "entity.store/3"` with the reason on the constant;
  `Hybrid::ids` mirrors `Hybrid::events`' read path. Pinned by `ids_cross_the_wire_intact`,
  `a_peer_at_the_previous_wire_version_is_refused_by_name`,
  `an_unreachable_authority_lists_unreachable_never_nothing`,
  `every_provider_lists_what_it_holds_sorted`.
- The conformance suite gains the cases — nothing stored is an empty list, not an error; a stored
  instance appears; a refused commit adds nothing — and `Broken` is extended so
  `a_broken_provider_is_caught` catches a provider that lists an id it does not hold. **Done** —
  three cases (9 in the suite, was 6); `Broken::ids` lists `ghost-nobody-stored`, and the
  "listed, sorted, and only that" case catches it by `load`ing every listed id.
- `entity list --store <root> --entity <type>` in the CLI, in text and JSON. **Done** — and YAML;
  `list_says_what_a_store_holds_and_nothing_for_a_type_nobody_stored`.
- Requirement rows added to `docs/requirements.md`, pinned; `store-v0.1.md` gains the section.
  **Done** — R-109; R-91 and R-105 gain the new pins; `store-v0.1.md` § 11, and § 9 says when the
  wire version moves.

Guard verified by breaking it (2026-08-28): with `MemoryStore::ids` returning an empty list,
`every_provider_lists_what_it_holds_sorted` and `the_memory_provider_conforms` fail —
*"`listed-a` was committed and is not listed: []"* — and pass again reverted.

## Decision taken

`ids` on `StateProvider`, the default: an event log with no state is not something a shell hydrates
*from*, and the state side is what knows what exists. `FileStore` makes the same point concretely —
it lists `<id>.json` files and not `<id>.events.jsonl`, so an event log whose state never landed
(its documented crash window) is not listed as an instance.

## Out of Scope

Queries, filters, pagination. An enumeration is the primitive a projection or a search index folds
from; the fold is the shell's (R-98).

## Open Questions

None outstanding; the one that was open is decided above.
