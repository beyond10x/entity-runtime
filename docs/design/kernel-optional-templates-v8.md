# Optional outcome properties, profile 8

R-133 extends outcome definitions and records with opt-in format 8. It retains profile 7's
required typed identity contract, value validation, predicates, event ordering and replay.
Existing profiles retain their vocabulary, bytes and strict missing-reference behavior.

## Explicit omission

At an object property or top-level state assignment, `$optional.args.input.note` resolves the
already declared `$args.input.note` reference. Missing means omit that property; present null
means include null. Any other present value is copied exactly. The same rule applies to other
references already admitted in that template's scope. Unknown paths and unavailable scopes are
registration errors, never absence. `$optional.optional...` is not a nested operator.

An omitted top-level state assignment removes its named field on change; an assignment not
present in `set` retains previous state as before. Nested objects are complete assigned values,
so a missing member is absent in the replacement object. Required state or payload properties
still fail normal schema validation if omitted. Identity correspondence and invariants still
apply before an accepted record is returned. Refusals never mutate the caller's instance.

Only object properties can disappear. Root event/error payloads and direct array elements cannot
use this reference form; nested objects inside arrays may contain optional properties, preserving
every array position. Plain `$args.input.note` remains strict even in profile 8. Escaping the leading
dollar (`$$optional.args.input.note`) preserves literal text. Actual input strings are values,
never recursively interpreted as templates after reference resolution.

## Implementation boundary

Registration carries a profile-8 template capability through the existing definition validator.
The runtime's field resolver returns `Option<Value>` to represent absence without a JSON sentinel.
It suppresses only a valid reference's absence; other reference errors propagate. Legacy public
definition registration does not admit this operator. Prepared outcome operations remain private;
the complete versioned outcome record is the replay authority, including omitted changed fields.
The existing domain event `changed` map contains written values, not a deletion sentinel. It is
not sufficient to rehydrate profile-8 removals, just as event-only history cannot represent earlier
outcome profiles' zero-event decisions. Consumers must retain and replay complete outcome records;
no legacy event-only replay or private prepared-operation snapshot is promoted to that authority.

## Evidence

Independent vectors cover absent/null/value properties through creation, replacement, events,
business errors and full replay; nested optional parents, literal dollar strings and arrays;
required-output and unknown-path refusals; tampering that changes omission to null; profile 1-7
refusals and unchanged bytes. A guard mutation must make the absence/null distinction test fail.
The ESS adapter and applications consume the published capability separately.
