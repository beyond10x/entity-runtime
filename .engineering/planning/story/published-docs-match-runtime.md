---
format: aep.planning-md/1
id: story:published-docs-match-runtime
kind: story
status: implemented
title: Published documentation matches the 0.15 runtime
summary: The site, guides and repository overview accurately describe decisions, refusals, stores, replay and migration.
relations:
- decomposes: epic:the-shell
- serves: vision:O2
revision: 5
---
## Context

The landing page and guides contain stale behavior and counts, and a breaking File Store migration without an operator runbook would strand existing users.

## Acceptance

Every public example and contract statement matches executable 0.15.0 behavior, and the Docusaurus site gives a linked, tested File Store migration and rollback procedure without volatile counts.

## Implementation evidence

Moved to implemented on 2026-09-09 after an audit of every non-implemented artifact against `CHANGELOG.md`, `docs/roadmap.md` and the crates.

The site was rewritten for the runtime and pins a release (0.17.7 at the time of this move, 0.18.0 with this release), with the File Store migration and rollback guide linked — `website/docs/**`, gated by `pages.yml`.
