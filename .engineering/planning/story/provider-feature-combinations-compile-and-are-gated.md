---
format: aep.planning-md/1
id: story:provider-feature-combinations-compile-and-are-gated
kind: story
status: draft
title: Every declared provider-feature combination of entity-eventlog compiles and a gate step checks them
revision: 1
---
## Outcome

`cargo check -p entity-eventlog --features postgres,sync-bridge` compiles, and a gate step runs the
provider-feature combinations the crate declares so that a combination nobody builds by hand cannot
silently break.

## Why

Found 2026-09-21 by unit 10 of wave-validate-v2-20260920 (story:recorded-store-imports-a-batch-with-one-capture)
while running "the three feature checks": at 8569da2, `--features postgres,sync-bridge` fails with
seven compile errors, verified against the base file, pre-existing, and run by no gate step. The
repository defines one feature-gated step (`eventlog-runtime-check`, `--all-features` on 1.91) plus
`postgres-check`; `--all-features` masks a broken pair because every feature is on at once.

The unit reported it rather than fixing it, which was right for its bound.

## Acceptance

- The seven errors are named in the story or the commit, and the fix is the smallest one that
  makes the pair compile; no behaviour change to either feature.
- `Taskfile.yml` gains a step that checks the declared provider-feature combinations
  (`file`, `sqlite`, `postgres`, each with and without `sync-bridge`, and `--all-features`),
  named in `AGENTS.md` with the others so a brief can cite it.
- `task check` green; the step's exits recorded per combination.
