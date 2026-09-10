# Typed identity, outcome profile 7

R-132. `story:typed-outcome-identity` extends the named outcome envelope with
`entity-outcome-definition/7` and `entity-outcome-record/7`. The definition requires
`identity: {field: <required-entity-field>}`. Profiles 1–6 forbid that metadata and
preserve their existing bytes and raw string identity behavior. Explicit null and
unknown contract fields are refused.

The named field is the logical identity, governed by its ordinary recursive schema
and value invariants. It must be required and fully typed. JSON catchalls, untyped
additional properties and defaults at any depth are refused. Nullable identities
may explicitly carry null; omission of the required identity field is not null.
The adapter may nest the remaining domain fields under a distinct typed object to
keep source field names separate from the identity role. This is a source identity
representation, not a generated replacement domain identity.

The instance key is `identity/1:` followed by compact JSON for a typed codec node:
`"null"`, or a single-key object whose tag is `boolean`, `number`, `string`, `array`
or `object`. Numbers carry their token spelling as text, arrays carry ordered nodes,
and objects carry a sorted map from original keys to nodes. Object keys are sorted
recursively; array order and scalar spelling are retained. These tags distinguish
codec vocabulary from domain data, including JSON-library numeric-carrier keys. There is
no Unicode, UUID, decimal-text or numeric coercion. Text `1` and number `1` have
different keys. Numeric tokens preserve the kernel's arbitrary-precision JSON
representation. Key parsing rejects alternate whitespace, object order, duplicate
keys or spelling that does not reproduce the exact canonical key. Encoding is a
public pure operation, not generation, hashing, storage or authority selection.

At invocation, decode the key and validate its value against the complete identity
schema, before selection. A supplied previous instance must contain that same
value in the designated field as well as have the exact key. Creation must set it
explicitly. A change may retain it or assign the exact same value, but cannot change
or remove it. Validate the resulting identity before returning its record. Refusals
and accepted observations preserve their optional state and still use a validated
identity key. Entity predicates use the typed field, so integer identities remain
numeric operands rather than the key's JSON text. Existing `$id` deliberately
remains the opaque transport key; lowerers must not mistake it for the domain value.

The same checks execute during complete record replay. A record's definition and
identity must remain fixed, and recomputation verifies its state, events and outcome.
The first definition is still caller-trusted; this is not history authentication.
Legacy create/execute APIs and provider layouts remain unchanged. Source input and
observed-event correspondence belongs to complete lowered command wiring, which
must preserve the source's explicit instance link and all emitted occurrences.

Independent vectors cover canonical key injectivity, exact values, typed admission,
creation/change/refusal, identity predicates, replay and malformed contracts. The
identity correspondence guard is checked by mutation, and the previous reader is
pinned independently for old-profile byte preservation and profile-7 refusal.
