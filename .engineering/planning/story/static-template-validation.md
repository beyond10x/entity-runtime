---
format: aep.planning-md/3
id: story:static-template-validation
kind: story
status: implemented
title: Template paths are validated at registration
summary: A set value or event payload that references an undeclared field or argument is refused when the definition is registered, as rule references already are.
relations:
- derived_from: epic:kernel
revision: 5
transitions:
- {from: "draft", to: "proposed", at: "2026-08-26T01:36:31Z", actor: "timo", revision: 3, imported: true}
- {from: "proposed", to: "active", at: "2026-08-26T01:36:31Z", actor: "timo", revision: 4, imported: true}
- {from: "active", to: "implemented", at: "2026-08-26T01:36:46Z", actor: "timo", revision: 5, decided_on: {"recorded":{"test_result":1,"artifact":1}}, imported: true}
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
