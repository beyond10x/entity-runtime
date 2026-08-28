# Next waves — the store an adopter runs on

**Status: accepted 2026-08-28 by the operator, with every default taken.** Written the same day
against this tree at `dc5b25a` (0.9.1) and `engineering-protocols` at `82a80e5` (0.27.3);
`story:store-enumeration` is the first of the three in progress, and the stories carry the record.

`engineering-protocols` has planned three storage waves — F, G, H — whose end state is that its
storage layer is this runtime's: one adapter over `entity_store::Store`, one provider of its own, and
every other store a type instantiated over a provider tested here. The plan, its evidence and its
decisions are on their page: `engineering-protocols/docs/plan/store-waves-f-g-h.md`. This page holds
only what that plan asks of this repository, and when.

## What the adopter needs, and before which wave

| their wave | story here | what it adds | why the SPI cannot do it today |
|---|---|---|---|
| F | `story:store-enumeration` | `StateProvider::ids` — what a store holds | `Store` is `load`/`revision_of`/`events`/`commit` (`crates/entity-store/src/lib.rs:147-190`); nothing can hydrate from a store it did not write |
| G | `story:events-carry-what-they-were-decided-on` | `DomainEvent::args` — what the rules read | `DomainEvent` records what changed, not what it was decided on (`crates/entity-core/src/runtime.rs:51-78`) |
| H | `story:postgres-provider` | `entity-postgres`, opt-in in the gate | the gate reaches no network; a provider needing a server needs a rule for when it runs |

All three decompose `epic:the-store-an-adopter-runs-on`. Each is a kernel or SPI capability with no
adopter vocabulary in it, which is the test `atlas/architecture/adr/0002` sets.

## Decisions, with the default if nobody answers

| question | default |
|---|---|
| `ids` on `StateProvider` or `Store`? | `StateProvider` |
| store a large argument on the event, or reference it? | store it — R-89's own line |
| gate behaviour when `ENTITY_POSTGRES_URL` is set and the server is down | fail, not skip |
| wire version for the new `Ask` | `entity.store/3`, refused by name to a `/2` peer — the same rule 0.9.0 applied |

## Housekeeping this page should not carry, but somebody should

`docs/roadmap.md` § 1 states a "blocking fact" that is false — `task:roadmap-page-is-current`.
