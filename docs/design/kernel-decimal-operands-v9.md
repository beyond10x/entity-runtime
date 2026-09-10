# Decimal predicate operands, profile 9

R-134 adds opt-in outcome definition/record format 9, retaining all profile-8 semantics.
`$decimal.args.amount` observes the declared `$args.amount` decimal-text value as an exact
JSON number for predicate evaluation. Other already scoped field/bound references work the same
way. This is an explicit operand codec, not a change to ordinary text references or stored values.

Registration requires the referenced field to be String with DecimalText encoding, after nullable
wrappers are unwrapped. Unknown paths, unavailable scopes, unrestricted JSON/text and another
encoding are refusals. The operator is available in predicate operands only; templates cannot use
it to change output wire types. `$$decimal...` remains a literal dollar-prefixed string.

Missing and null references remain unobserved. Decimal grammar admission remains the existing
DecimalText contract: no exponent spelling, plus sign or leading integer zeroes. The internal
numeric observation preserves every digit. Numeric equality/order and membership use the kernel's
exact number comparison, including scale and signed-zero equivalence. Existing scalar truthiness
continues to require a finite binary64 observation and therefore retains its underflow/overflow
behavior; this profile does not redefine that separate operator. No input/state/event value is
normalized, and full records retain the original string and compiled predicate literal.

## ESS boundary

The new native target can compare decimal wire strings without narrowing them to floating point.
ESS's existing predicate literal parser and persisted Number writer are not changed by this
capability. A literal already admitted as 1.0 remains 1.0; lost authored digits cannot be reconstructed
by the lowerer. Conformance's legacy numeric candidate representation likewise has narrower
precision than native decimal strings. Native exact-wire vectors and legacy representable vectors
establish different facts; neither is evidence that ESS's canonical-serialization migration shipped.

## Verification

Independent cases cover decimal order (2 versus 10), differing scales, signed zero, adjacent large
integers, precision beyond binary64, tiny/large magnitudes, missing/null, local invariants and
quantified bindings. Registration and value-shape failures name the reason. Complete replay rejects
changed input, predicate and emitted/state evidence. Compare profiles 1-8 against the exact previous
reader, show it refuses profile 9, and break the exact conversion to verify test sensitivity.
