---
format: aep.planning-md/3
id: story:creation-set-if-present-ignores-parent-defaults
kind: story
status: draft
title: A creation set_if_present leaf ignores argument-parent defaults
relations:
- serves: vision:O2
- informed_by: review-result:er-54-u1-adversary-pass-1
scope:
- confidence: cited
  path: CHANGELOG.md
- confidence: cited
  path: crates/entity-core/src/validation.rs
- confidence: cited
  path: crates/entity-core/tests/adversary_operation_set_if_present.rs
- confidence: inferred
  path: crates/entity-core/tests/service_binding_boundary.rs
- confidence: cited
  path: docs/design/service-binding-boundary-v0.1.md
- confidence: inferred
  path: docs/requirements.md
- confidence: inferred
  path: ess/generated/suite.json
- confidence: inferred
  path: ess/scenarios/core/conditional-presence-registration-requires-one-typed-optional-leaf.yaml
revision: 4
---
# A creation set_if_present leaf ignores argument-parent defaults

## Outcome

A creation outcome's `set_if_present` leaf is present only when the caller sent that leaf. A
required argument parent whose `default` carries the leaf no longer makes the leaf present for a
caller who omitted the parent, as design rule 3 of
`docs/design/service-binding-boundary-v0.1.md` says ("exactly the leaf controls presence").

## Why

Found by `review-result:er-54-u1-adversary-pass-1` (finding F2, origin pre-existing):
`present_argument_leaf` (`crates/entity-core/src/validation.rs:818` at `8b7d4dab`) does not look
at parent defaults, so a creation called without `bound` gets `note: "from-default"` from
`bound`'s default. The same gap on operation outcomes (F1) is closed inside
`story:operation-writes-an-optional-field-from-an-optional-argument`, because that story opened
the path; the creation side is older than it and is left to this story.

## Acceptance

- Registration refuses a creation `set_if_present` leaf whose argument parent declares a `default`
  that carries the leaf, with `ConditionalArgumentInvalid` and its path, or the design states why
  such a definition stays admitted and what it means.
- The adversary case
  `a_parent_default_cannot_make_a_creation_set_if_present_leaf_present_for_a_caller_who_sent_none`
  (`crates/entity-core/tests/adversary_operation_set_if_present.rs`) asserts the decided behaviour.
- A definition admitted before the change that this refuses is named in `CHANGELOG.md` as a
  narrowing; consumers' committed definitions are checked for such a parent default first.

## Out of scope

Operation outcomes (closed by the story named above).

## Same class, found by the implementor of the operation unit

A parent default also makes a leaf present without the caller sending it for `responds_if_present`
and `payload_if_present`, on creations and on operations. Both predate the operation unit, and
refusing them refuses definitions admitted before. This story covers them too: the acceptance above
applies to every `*_if_present` map, each refused or stated in the design.
