---
format: aep.planning-md/3
id: story:binder-elements-carry-their-declaration
kind: story
status: draft
title: Quantifier binder elements carry their declaration at run time
owner: entity-runtime
relations:
- serves: vision:O2
- informed_by: story:a-condition-reads-the-length-of-a-text
- informed_by: review-result:er-w1-u1-adversary-pass-1
revision: 1
---
# Quantifier binder elements carry their declaration at run time

## Outcome

Inside a `service/1` quantifier (`for_all` / `exists`), a rule reading `$t.count` on a bound element
answers with the element's declared kind: a `string` element answers its number of Unicode scalar
values, a map element its number of entries, an array element its length. Today the run-time walk
reads binder elements without their declaration (`crates/entity-core/src/runtime.rs:2803`), so
registration refuses these forms by name instead.

## Why

- ESS `docs/design/string-alphabet-and-length.md` § 3 admits `forall … t.count <= 8` over a list of
  text; `story:a-condition-reads-the-length-of-a-text` (wave 1) refuses it in Entity Runtime because
  the element's declaration is lost at run time.
- Wave 1's adversary and security passes confirmed a pre-existing defect on the same path: a map
  element's `$m.count` registered and read the member named `count` (5) instead of the map's size
  (1). Wave 1 replaced that with a registration refusal (review-result
  `er-w1-u1-adversary-pass-1`, `er-w1-u1-security-pass-1`).

## Acceptance

- Named ESS core scenarios: text, map and array elements under `for_all` and `exists` answer their
  size; the two wave-1 refusals are lifted, and the cases that assert them are rewritten to assert
  the size answer.
- Untyped binder paths (a quantifier over a `json` path or a union payload) keep resolving to
  nothing (R-97); a test shows one.
- Requirement row pinned by a live test; `CHANGELOG.md` line.

## Out of scope

Any new quantifier operator.
