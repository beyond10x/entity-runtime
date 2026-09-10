# Finite Binary64 values, profile 10

R-135 adds outcome definition/record format 10 with all profile-9 capabilities.
A Number field may declare `number_encoding: binary64`; earlier profiles refuse it.
The field remains distinct from unrestricted exact JSON Number and decimal-text String.

Convert numeric input once with nearest-even f64 parsing and require finiteness. Preserve
subnormals, negative zero and signed underflow; overflow and non-numbers refuse. Persist the
shortest round-tripping numeric spelling with a floating marker (decimal point or exponent),
including -0.0. Original spelling is not retained. Existing scalar predicates compare the resulting
numeric observations; no arithmetic or new mixed-number operator is introduced.

Normalize declared fields recursively in arguments, defaults, creation/change assignments and
schema-typed event/error payloads before their validation/invariants. Do not fill new defaults on
change or payload normalization. Previous state and typed identity keys must already be normalized:
reject instead of silently changing an existing identity or history. Construct a key from a decoded
or checked normalized value; the existing schema-free identity_key function does no conversion.
A failed admission leaves caller state untouched. Complete replay compares normalized invocations,
state, events and errors, retaining exact negative-zero serialization.

`ObjectSchema::decode_json` is a pure typed source decoder. It retains RawValue tokens through
objects, arrays, maps, nullable wrappers and adjacent-tag payload selection. An encoded leaf accepts
an actual numeric token only, checks finite conversion, and emits a normalized JSON Number.
Quoted numbers and private Number/RawValue marker objects cannot impersonate source numbers.
The decoder does not replace definition registration or whole-object execution validation.
Programmatic Value callers still receive finite normalization at execution, but a sign already lost
by earlier generic JSON parsing cannot be reconstructed. Neither default Value serde nor old record
readers are changed; original-token ingestion must use the explicit decoder.

Number bounds remain the existing exact numeric constraints over the normalized value. A standalone
schema projection emits `format: double` and `x-entity-number-encoding: binary64` as codec metadata;
JSON Schema does not enforce nearest-even conversion or serializer policy. Old definitions omit the
new field and retain their bytes. The only dependency change enables serde_json's raw_value feature
inside entity-core; there is no new dependency or IO.

Independent qualification uses authored IEEE bit patterns, tests each container and execution edge,
checks malformed/nonfinite/refused data and replay tampering, and compares old-profile corpus bytes
against the exact published profile-9 reader. Mutation must demonstrate sensitivity to rounding or
signed-zero loss. This capability does not itself complete ESS Binary64 ingestion, synthesis,
conformance or application adoption.
