---
format: aep.planning-md/1
id: task:roadmap-page-is-current
kind: task
status: draft
title: docs/roadmap.md states a fact that stopped being true
summary: § 1 'The blocking fact' says engineering-protocols has never heard of this repository; it has three of its crates. Rewrite with the old section kept as superseded.
relations:
- decomposes: epic:the-store-an-adopter-runs-on
revision: 3
---
# Task: `docs/roadmap.md` states a fact that stopped being true

## What

`docs/roadmap.md` § 1 is titled *"The blocking fact"* and says `engineering-protocols` *"has never
been told this repository exists"*, with a zero-hit grep as evidence. The grep now returns their
README, `AGENTS.md`, two `Cargo.toml`s, a concepts page and release posts. § 2's evidence dates are
2026-08-25 and phases 0–4 are marked variously; `docs/plan/next-wave-the-shell.md` § *Housekeeping*
already names this.

## Done when

§ 1 states what is true (phases 0–4 shipped, three crates adopted, the arrow one way); § 6 points at
`epic:the-store-an-adopter-runs-on` for what follows; every evidence date is re-read; nothing is
deleted — the old § 1 moves to a *Superseded* note so the record of the sequencing decision stays.
