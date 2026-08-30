---
format: aep.planning-md/1
id: story:published-docs-match-runtime
kind: story
status: draft
title: Published documentation matches the 0.15 runtime
summary: The site, guides and repository overview accurately describe decisions, refusals, stores, replay and migration.
relations:
- decomposes: epic:the-shell
revision: 1
---
## Context

The landing page and guides contain stale behavior and counts, and a breaking File Store migration without an operator runbook would strand existing users.

## Acceptance

Every public example and contract statement matches executable 0.15.0 behavior, and the Docusaurus site gives a linked, tested File Store migration and rollback procedure without volatile counts.
