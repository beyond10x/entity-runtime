---
format: aep.planning-md/1
id: story:named-predicates
kind: story
status: draft
title: Named reusable predicates
summary: A definition declares predicates once and rules reference them by name.
relations:
- derived_from: epic:kernel
revision: 2
---
# Story: Named reusable predicates

## Outcome

A definition declares `predicates:` once and rules reference them by name, so *is_estimated* is
written once and used by three operations.

## Acceptance

`predicates: { is_estimated: { gt: [$fields.points, 0] } }` and `assert: { use: is_estimated }`
parse, validate (unknown name refused at registration; scopes still enforced per use) and evaluate
identically to the inlined condition; a test proves the equivalence.
