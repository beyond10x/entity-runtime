# Nullable collections and quantified outcomes

Owner: `story:nullable-collection-outcomes`. R-127 governs the explicit
`entity-outcome-definition/2` and `entity-outcome-record/2` profile. It extends the
[named-outcome contract](kernel-outcomes-v1.md), using the existing kernel machinery.
Legacy definitions and outcome profile 1 reject this vocabulary at registration; their
serialization, accepted inputs and replay semantics are unchanged.

## Values

`type: nullable` requires `items`, the complete type of a non-null value. `required`
still controls whether the containing object must supply the field. Explicit null is a
value and is never replaced with a default; non-null objects and collections receive
their existing nested defaults and validation. Constraints belong on the inner type,
not on the nullable wrapper. Nested nullable types remain transparent to reference
validation and preserve the same property path.

`type: map` requires `items`, the complete type of every value in a JSON object. Keys
are strings and are not interpreted as reference paths or binder fields. Every supplied
value is validated, including nested defaults, numeric bounds, enums and required
properties. This does not establish ESS's non-string map-key codecs or tagged unions.

The existing `items` representation is reused, so old fields acquire no serialized keys.
All schema surfaces participate: arguments, entity fields, event and business-error
payloads, and nested values. Schema errors accumulate through the existing validator.
No `json` replacement stands in for a typed nullable or collection element.

## Quantification

Two closed condition operators have the same operand shape:

```json
{"forall": {"over": "$args.lines", "bind": "line", "body": {"gt": ["$bound.line.amount", 0]}}}
```

`any_element` asks whether at least one element satisfies its body. The existing `exists`
operator remains a presence question. `over` must reference a declared Array or Map,
possibly wrapped in Nullable. An arbitrary JSON field or untyped additional property is
not a declared collection. Binder names are identifiers and live only under `$bound` in
their body. Nested binders may shadow a name; other outer binders and ordinary scoped
references remain visible. A binder never grants access to otherwise forbidden state or
arguments. Templates outside a quantified body cannot read it.

Empty forall is true; empty any_element is false. Missing/null collections produce Unknown,
even with a constant body. Null elements remain elements; a value comparison over one is
Unknown while presence can be false. Conjunction/disjunction use existing Kleene truth,
including a decisive false/true beside Unknown. Evaluation is pure and deterministic;
map values are traversed in key order, without a key/value entry wrapper. Unknown paths
retain their lexical `$bound` spelling; concrete ordinal diagnostics are not added here.

Registration checks every body and reference, including a body whose collection is empty
in the present invocation. Collection kind and binder scope are validated before effects.
Direct cardinality/ordinal expressions and ESS-specific scalar operators/codecs are separate
remaining lowering work, not aliases silently added to legacy references.

## Version and execution boundary

Only the private outcome preparation path admits version-2 schemas/operators. Public legacy
registration cannot produce an executable handle containing them. Prepared branch programs
remain private, and all result schema/invariant/event checks run in the same kernel path as
before. Version 2 records the original command and definition and replays by recomputation.
Changing either format discriminator or substituting a definition is refused; no old event
stream is reinterpreted as complete command history. Existing profile-1 bytes stay unchanged.

## Verification

Independent JSON definitions exercise nullable fields, defaults, typed map errors and null
payloads through actual create/change/selection/replay. Quantifier vectors include empty,
missing, null, nested and shadowed binders, free/outer references and dormant invalid bodies.
Format downgrade/unknown-key and unchanged legacy cases protect the opt-in boundary. A
mutation that changes quantifier truth or removes typed validation must fail the matching
test. The operator excluded the full gate; affected-package and compatibility checks remain.
