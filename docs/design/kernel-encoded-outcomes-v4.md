# Encoded strings in outcome profile 4

R-129 adds explicit string wire grammars to the pure kernel. The discriminators are
`entity-outcome-definition/4` and `entity-outcome-record/4`; this profile includes
profiles 2 and 3. Ordinary entity definitions and earlier outcome profiles refuse
the new metadata. No provider, dependency or consumer pin changes.

A string field can declare `encoding`; a map can declare `key_encoding` while
`items` still governs its values. Each selects one closed enum value:

| Encoding | Admitted spelling |
| --- | --- |
| `uuid_hyphenated` | ASCII hex groups of 8-4-4-4-12, exact hyphens, either case |
| `base64_padded` | Standard ASCII alphabet, complete four-character groups, at most two trailing padding characters |
| `decimal_text` | Optional minus, integer without leading zeroes, optional nonempty fraction |

Empty base64 is valid. Padding bits are not canonicalized or checked beyond the
wire grammar. UUID versions/variants are not restricted. Decimal strings preserve
negative zero, trailing zeroes and arbitrarily long digits; no floating point
conversion occurs. Exponents, plus signs, whitespace and non-ASCII digits refuse.

The concrete compatibility source is ESS e14d75fb
`crates/generate/ess-gen/src/types.rs` (UUID_PATTERN, BASE64_PATTERN and
DECIMAL_PATTERN); `ess-primitives/src/facts.rs` owns matching UUID/base64
input checks. This is wire validation, not a claim that ESS's number-valued
conformance Decimal and string-valued generated Decimal have been unified.

Metadata on the wrong kind refuses at definition admission. Unknown encodings and
explicit null metadata refuse during decoding; omission remains absent and emits
no new field in older serialized definitions. Recursion checks all nested and
dormant schemas and defaults before any command runs.

Runtime validation retains every key's exact spelling. A bad map key and a bad
value produce separate errors at the quoted key address; messages distinguish
key errors. Arrays, nullable values and selected union payloads recurse normally.
String length and encoding failures accumulate independently. Arguments, new
state, emitted events and typed business errors use the same validator.

Replay compares the complete re-decided record, including format and exact
encoded spelling. A string that denotes the same mathematical or decoded value
is not an interchangeable event. Projection emits the corresponding string
`pattern` and map `propertyNames` pattern; validation does not depend on
optional JSON Schema format assertion or content decoding.

## Evidence and remaining semantics

`encoded_outcomes.rs` covers accepted and refused wire values, recursive map
key/value accumulation, profile admission, defaults, dormant alternatives,
event/error output checks and altered replay records.
`encoded_schema_checks_map_keys_and_values_without_coercion` validates projected
schemas against independent expected values. A retained external Rust comparison
uses actual ESS-generated schemas and the published ER 03ca1caf reader.

Decimal arithmetic/comparison, non-string map key codecs and bounds, temporal
semantics, Binary64, nested type invariants and complete ESS lowering remain
separate requirements of ESS evolution. Existing string comparison semantics are
unchanged; this profile does not silently turn decimal text into a JSON number.
