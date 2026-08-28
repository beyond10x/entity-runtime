---
format: aep.planning-md/1
id: epic:the-store-an-adopter-runs-on
kind: epic
status: proposed
title: The store an adopter runs on
summary: 'What engineering-protocols'' storage waves F, G and H need from the SPI: enumeration, the decision basis on events, and a provider with a server — each with no adopter vocabulary in it.'
relations:
- decomposes: initiative:entity-runtime
revision: 4
---
# Epic: The store an adopter runs on

## Outcome

The first adopter's storage layer is this runtime's: `engineering-protocols` keeps one adapter over
`entity_store::Store` and one provider of its own, and every other way its plan is kept — SQLite,
Postgres, a markdown-plus-SQLite hybrid — is a provider tested here. What the adopter needs from
the SPI to get there is three capabilities, each with no `aep` in it.

## Context

`epic:centralized-and-hybrid-storage` and `epic:the-shell` shipped the SPI, three local providers,
a remote one and a hybrid, and `engineering-protocols` 0.27.0 adopted `entity-sqlite` behind its
contract. Its plan for the next three waves — `engineering-protocols/docs/plan/store-waves-f-g-h.md`
— names exactly three things the SPI cannot yet do for it: say what a store holds, record what an
operation was decided on, and run on a server. Each is a story here, sequenced before the wave that
needs it, and the arrow stays one way (`atlas/architecture/adr/0002`).

## Acceptance

The three stories ship, each with its requirement rows and pins; the adopter's waves F, G and H
cite the release that carried each; nothing in `entity-core`'s dependency list or purity scan
changes.
