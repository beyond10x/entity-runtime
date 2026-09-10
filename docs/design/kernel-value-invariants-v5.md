# Value invariants in outcome profile 5

R-130 adds local rules to typed values through `entity-outcome-definition/5` and
`entity-outcome-record/5`. It includes profiles 2–4. Earlier profiles and ordinary
entity definitions refuse the new field metadata, including an explicitly empty
list. Missing metadata emits no new serialized field; explicit null refuses.

The source requirement is ESS `ResolvedBody::Newtype::invariants` and
`ResolvedBody::Struct::invariants` in `ess-compiler/src/ir.rs`, checked against
the newtype representation or struct fields by `ess-domain/src/types.rs`.
An entity-level invariant cannot enforce those rules at every nested argument,
state, event and error boundary.

## Scope and evaluation

A field can carry `invariants: [RuleDefinition, ...]`. Each existing closed rule
has an optional name/message and an `assert` condition. Its fixed lexical
`$bound.value` binding denotes the complete value. For example:

```json
{
  "type": "object",
  "properties": {
    "start": {"type": "integer", "required": true},
    "end": {"type": "integer", "required": true}
  },
  "invariants": [
    {"name": "ordered", "assert": {"lte": ["$bound.value.start", "$bound.value.end"]}}
  ]
}
```

Registration checks every rule and every path, including dormant union
alternatives. Only the local value and quantifier bindings are readable.
Identity, entity version, lifecycle state, command arguments, sibling values and
outer value bindings are unavailable. Quantifiers preserve ordinary lexical
capture and shadowing: rebinding `value` hides the outer value for that body.

Defaults are applied before checking the resulting typed value. A missing
optional field causes no evaluation. A nullable wrapper with rules on its inner
type admits null without running those inner rules; rules deliberately placed on
the wrapper inspect null as their local value. Only the selected union payload
runs its value rules, while every declared alternative is checked at registration.

Value rules run after the value and its children have passed structural and local
validation. A failed child already refuses the containing value; parent rules
are skipped to avoid cascade errors over invalid operands. Independent sibling
values are still visited, and all local rules on an admitted value are evaluated.
`true` passes; `false` and `unknown` refuse distinctly. Validation errors carry
the complete value path, rule name or index, optional message, and unresolved
local addresses for unknown results. This does not change the existing
entity-invariant failure type or evaluation order.

The shared predicate evaluator accepts a value context without an entity
definition. No synthetic domain entity, identity generation, IO, or dependency is
introduced. Existing entity/event contexts continue to supply their definitions.

Arguments, new state, events, business errors and declared defaults all use the
same recursive value validator. Replay re-evaluates the complete definition and
command and compares the full record. Replacing a value, changing the definition
between records, or downgrading only a record profile cannot bypass that comparison.
Replay uses the first record's definition; authenticating that initial definition
remains the caller's responsibility. Rewriting a whole history consistently is
not something semantic replay alone can detect.

## Projection and evidence

JSON Schema still describes structure. The explicit
`x-entity-value-invariants` extension retains `binding: "$bound.value"` and
the complete `rules` array. A schema validator does not execute the ER predicate
language; consumers must satisfy this recorded runtime obligation through ER.
The surface test demonstrates both the retained annotation and that boundary.

Core behavior is pinned by `value_invariants.rs`; existing legacy and outcome
suites cover the shared evaluator-context change. The retained independent Rust
probe compiles authored ESS newtype/struct invariants and compares manually
expected true/false/unknown results at root and map/list locations. It also uses
published ER 3af07dc to compare legacy and profile-1/2/3/4 bytes and old-reader
refusals. Removing local rule evaluation must make the probe fail.

This adds rule placement, not new predicate operators or automatic ESS lowering.
Decimal-text ordering, scales, truthiness, temporal/Binary64 semantics, typed
identity and full service/Connectors migration remain separate evolution
requirements. The future lowerer must retain nominal type and conversion
identity while mapping every source invariant into the appropriate local scope.
