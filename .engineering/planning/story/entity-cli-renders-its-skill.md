---
format: aep.planning-md/1
id: story:entity-cli-renders-its-skill
kind: story
status: draft
title: The entity CLI renders its own agent skill
summary: A deterministic entity skill teaches agents the installed CLI surface without duplicating it in a plugin.
relations:
- decomposes: epic:the-shell
revision: 1
---
## Context

Agents can discover `entity --help`, but they do not know when to validate a definition set, how store recording metadata works, or why File Store migration is out of place. The binary version that owns those rules should render the compact skill that teaches them.

## Acceptance

`entity skill` emits a deterministic version-stamped Agent Skills document, and `--out` writes identical bytes while refusing replacement unless `--force` is explicit.
