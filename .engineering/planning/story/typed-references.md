---
format: aep.planning-md/1
id: story:typed-references
kind: story
status: draft
title: Typed references between entities
summary: A field kind ref naming an entity type, with the checking boundary (kernel input vs shell) decided.
relations:
- derived_from: epic:kernel
revision: 2
---
# Story: Typed references between entities

## Outcome

A field can be declared `type: ref` with an `entity`, so a definition says that an order's
`customer` is a customer, and the runtime can check it.

## Context

The open design question is *where* the check runs. The kernel cannot load the referenced instance
(R-01), so either the reference is checked by the shell before `execute`, or the referenced
instance's existence enters the kernel as an input. Decide, record it in `kernel-v0.1.md` § 12's
successor, then build.

## Acceptance

`type: ref` parses and validates at registration (the named entity type must be registered);
the decided checking boundary is implemented and tested; `entity inspect` shows the reference.
