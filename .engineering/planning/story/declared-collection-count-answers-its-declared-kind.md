---
format: aep.planning-md/3
id: story:declared-collection-count-answers-its-declared-kind
kind: story
status: draft
title: A declared collection answers count only for its declared kind
owner: entity-runtime
relations:
- serves: vision:O2
- informed_by: story:a-condition-reads-the-length-of-a-text
revision: 1
---
# A declared collection answers count only for its declared kind

## Outcome

Under `service/1`, `<path>.count` on a field declared as a map answers only when the stored value is
an object, and on a field declared as an array only when it is an array; a stored value of another
kind answers nothing (the rule is unobservable), as a declared `string` holding a non-text already
does after wave 1 (R-160).

## Why

Found in wave 1's correction round for `story:a-condition-reads-the-length-of-a-text`, measured with a
probe test that was run once and deleted: a declared map holding a 7-element array answered 7, and a
declared array holding `{count: 7}` answered 7. Pre-existing since R-148. Only a stored instance that
does not conform to its definition reaches it: arguments are validated before any rule reads them,
and no store validates instances on load. A patch was written in the wave's scratch directory and
deliberately not applied, because it changes R-148's behaviour.

## Acceptance

- Two tests, red before the fix: a declared map holding an array and a declared array holding an
  object each make a rule reading `.count` unobservable.
- R-148 amended and pinned; `CHANGELOG.md` line.

## Out of scope

Validating stored instances on load.
