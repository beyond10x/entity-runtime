# Selected creation identity v0.1

Status: binding amendment for ESS evolution M5–M6 selected creation identity.

This amendment preserves the logical identity authored by the accepting creation outcome that Entity Runtime selects. It adds one pure kernel entrypoint and one explicit lowerer binding mode. Existing definitions, supplied-address entrypoints, decision records, request framing, replay, and canonical bytes retain their current meanings.

## Kernel entrypoint

Entity Runtime adds free and registry-backed `create_derived` and `decide_create_derived` functions. They accept the same validated definition and input as the existing creation functions but no storage address. They are available only when the validated definition declares a logical identity. Existing `create` and `decide_create` continue to require a caller-supplied address and execute their current path unchanged.

For a branched `service/1` creation, the derived path performs the established steps once and in the established order:

1. normalize and validate creation arguments;
2. select exactly one outcome with Entity Runtime's selector;
3. return a selected refusal without deriving an address;
4. resolve the selected branch's assignments and conditional assignments;
5. apply defaults, canonicalize, and validate the complete fields;
6. read the declared logical identity field and call the existing total `entity_core::identity::address` function for its declared kind;
7. construct the instance at that address, run the existing identity mirror and invariants, then materialize events, response, decision, and record.

Selection is not repeated. The host never interprets a predicate and supplies no candidate address. Invalid or absent identity data is a typed `creation_identity_unavailable` error. For branchless `service/1`, the validated input is the fields and address derivation occurs from those fields before the unchanged decision construction.

## Pre-address `$id`

The pre-address selector and selected assignment scopes have no `$id`. `TemplateContext` represents that absence explicitly; resolving `$id` before derivation returns the existing typed template error naming `$id` and the circular dependency. It never receives an empty string, sentinel, tentative address, or value from another branch.

The existing supplied-address functions continue to expose their supplied `$id` to the same selector and assignment scopes as before. Definitions and bytes therefore stay compatible. A definition that needs `$id` before fields exist remains executable through the old API and is explicitly refused by the derived API. After derivation, invariants, events, and responses observe the real derived address normally.

## Records, replay, and request compatibility

Derived and supplied creation are two ways to establish the same storage address before the decision is recorded. A derived decision carries the existing `DecisionCommand::Create`, `DecisionRecord.id`, instance id, and event ids without a new key or discriminator. Calling the supplied API with that exact derived address and the same arguments must produce a byte-identical `Decision`.

Replay continues to use the recorded address through the supplied-address path, recomputes the complete decision, and byte-compares the record. Original request reconstruction therefore retains the same subject, definition version, normalized arguments, and recording data. No `er.record/*`, `er.request/*`, or batch domain changes.

## Lowerer binding

The lowerer keeps the existing shared creation binding when every accepting outcome has a structurally equal identity source. Structural equality includes the complete typed source: input or response coordinate, exact literal value, generated/undetermined ownership, and conversion source and reason. Its one shared source and representative observation retain current slot allocation and bytes.

When accepting outcomes use unequal sources, the lowerer emits a distinct `SelectedOutcome` creation binding. It contains a closed ordered map from every accepting outcome name to that outcome's authored identity value and exact observed event coordinate. The definition's branch assignments remain the executable authority; this map is the host contract and drift evidence. No source is selected or evaluated by the host.

A `SelectedOutcome` binding instructs the host to call `decide_create_derived`. A shared binding instructs it to resolve the established single source, derive its address through `identity::address`, and call the existing supplied-address API. Missing, duplicate, refusing-only, or mismatched outcome coordinates are compile-time diagnostics. Equivalent generated mappings retain one slot; unequal mappings do not get conflated into one slot.

## SDK `/4` representation

The unpublished closed `/4` binding mirror carries the creation mode explicitly as either `shared` with its source and representative observation or `selected_outcome` with the complete ordered outcome map. A reader never infers the mode by counting sources. Because no `/4` reader has shipped, this is a source amendment rather than reinterpretation of persisted bytes.

## Compatibility and controls

Required proofs cover:

- both unequal typed-input branches, including different event occurrences, select and publish their own identity;
- equivalent shared generated/input mappings retain the existing binding and behavior;
- `$id` in a selector and in a selected assignment is refused by the derived API, while the same definition still executes through the supplied API;
- derived and supplied decisions are byte-identical when their final address agrees;
- record replay and original-request comparison retain the subject and request;
- existing creation, operation fulfillment, adapter composition, and old canonical byte fixtures remain unchanged.

R-154 pins the selected creation identity capability, its pre-address refusal, and its unchanged
decision, replay and request contracts.
