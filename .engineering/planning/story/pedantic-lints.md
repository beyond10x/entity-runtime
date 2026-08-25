---
format: aep.planning-md/1
id: story:pedantic-lints
kind: story
status: draft
title: clippy::pedantic in the gate
summary: Raise the workspace lint level to pedantic and fix or justify every hit.
relations:
- derived_from: epic:kernel
revision: 2
---
# Story: clippy::pedantic in the gate

## Outcome

The workspace lint level is `clippy::pedantic`, warnings fatal, and every remaining `allow` states
its reason beside the item.

## Acceptance

`[workspace.lints.clippy] pedantic = "warn"`; `task check` green; no `allow` without a comment.
