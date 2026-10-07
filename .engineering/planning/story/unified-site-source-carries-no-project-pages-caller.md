---
format: aep.planning-md/3
id: story:unified-site-source-carries-no-project-pages-caller
kind: story
status: implemented
title: A unified-site source carries no project Pages caller
owner: entity-runtime
relations:
- serves: vision:O2
scope:
- confidence: cited
  path: .github/workflows/b10x-docs-site.yml
- confidence: cited
  path: AGENTS.md
- confidence: cited
  path: website/README.md
revision: 7
transitions:
- {from: "draft", to: "proposed", at: "2026-10-07T00:15:23Z", actor: "human:timo", revision: 5}
- {from: "proposed", to: "active", at: "2026-10-07T00:15:23Z", actor: "human:timo", revision: 6}
- {from: "active", to: "implemented", at: "2026-10-07T00:28:52Z", actor: "human:timo", revision: 7, decided_on: {"recorded":{"test_result":1,"verification":1}}}
---
# A unified-site source carries no project Pages caller

## Outcome

`.github/workflows/b10x-docs-site.yml` is gone, so Atlas's documentation reconcile no longer
refuses this repository. `AGENTS.md` and `website/README.md` say that nothing here publishes the
`b10x-project-site` artifact while the repository is a unified-site source.

## Why

Atlas's docs reconcile refuses the file: "./entity-runtime/.github/workflows/b10x-docs-site.yml is
not this repository's project Pages caller". This repository is still a unified-site source, and
Atlas admits one Pages caller per unified repository (atlas `src/docs.rs:355-366`, as reported to
this repository). The documentation overhaul (`867b190f`) added the file. The pages already
deployed at `/entity-runtime/` stay up; the unified site keeps serving `/docs/entity-runtime/`.

## Acceptance

- `.github/workflows/b10x-docs-site.yml` is deleted and no file outside the planning store names it
  except the `AGENTS.md` sentence recording its removal.
- Atlas's `docs reconcile --check` no longer reports the file (run by the organization side).

## Out of scope

Leaving the unified site (Stage C of the `docs` skill): a later, separate wave.
