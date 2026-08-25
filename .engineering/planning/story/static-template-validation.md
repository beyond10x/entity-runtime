---
format: aep.planning-md/1
id: story:static-template-validation
kind: story
status: draft
title: Template paths are validated at registration
summary: A set value or event payload that references an undeclared field or argument is refused when the definition is registered, as rule references already are.
relations:
- derived_from: epic:kernel
revision: 2
---
# Story: Template paths are validated at registration

## Outcome

A `set` value or an event payload that references an undeclared field or argument is refused when
the definition is registered, as rule references already are (R-14), instead of failing at the
first execution (R-63).

## Acceptance

A test registers a definition whose event payload reads `$args.nonexistent` and asserts a
`DefinitionError::InvalidTemplate` (or the accumulated equivalent) naming the path; R-63 stays
as the run-time backstop for paths that cannot be checked statically (`$fields.some.deep.path`
into a `json` field).
