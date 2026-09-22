# Service binding boundary v0.1

Status: complete implementation contract for the two Entity Runtime gaps required by ESS evolution
revision 1 M5/M6. The source baseline is Entity Runtime
`24d31cf1f97a3744db7e65c5629e056bc7a3a241`; the consuming lowerer baseline is ESS
`be604d874ee9e567ae104e565e7cbddf953885a9`. This page adds no SDK hosting, authentication,
query, effect, provider, or migration machinery. The existing pure `service/1` design and source
reviews remain closed.

The two additions form one boundary:

1. ER can finish a subjectless refusing branch from normalized input before a host loads an
   instance, or return an opaque continuation that binds the validated definition, normalized
   input, operation, and expected subject.
2. A new definition semantics version can say that one typed argument is copied into an optional
   creation field, event member, or response member exactly when that argument is present.

Both additions remain deterministic and IO-free. The host supplies an identity address and bound
values, performs the load requested by ER, and persists an accepted decision. It never evaluates a
guard, names an outcome, fabricates an instance, or rewrites a definition per request.

## Evidence and boundary

At the pinned revision, `Runtime::decide` and the free `decide` require an
`&EntityInstance`, and `decide` checks that instance before it normalizes arguments or selects a
branch (`crates/entity-core/src/runtime.rs:359-366,740-782`). That order cannot answer the real
`PayInvoice` case: `settled` has the positive-amount guard, `rejected` is the following default, and
`wrong-state` is separate (ESS `examples/billing/domains/invoice.yaml:298-340`). The accepted
realization returns `rejected` for a nonpositive amount before looking for the invoice
(`examples/billing-realization/src/invoice.rs:271-305`), including through the conformance boundary
(`examples/billing-realization/tests/conformance.rs:430-473`).

Templates currently resolve every member they are given; a missing reference is
`CoreError::Template`, and an object template inserts every declared key
(`crates/entity-core/src/runtime.rs:1928-2009`). A missing argument member and a member whose value is
JSON `null` are therefore already distinguishable during schema validation, but there is no
definition form that conditionally omits an output member. The real source reaches this gap:
`Invoice.note` and `Invoice.issued_at` are optional and omitted by creation mappings (ESS
`examples/billing/domains/invoice.yaml:118-123,242-250`), and `Visit.badge` is optional while
`RegisterVisit` declares no entity `sets` (ESS `examples/gatepass/domains/visit.yaml:90-91,172-189`).

Existing authority is retained:

- only a `ValidatedDefinition` or a registry lookup can enter either execution path
  (`crates/entity-core/src/registry.rs`);
- branch order, three-valued conditions, state admission, and wrong-state selection remain the
  accepted `select_outcome` then `admit_state` rules (`crates/entity-core/src/runtime.rs:981-1092`);
- a refusal produces no decision record, revision, state, event, or response
  (`crates/entity-core/src/runtime.rs:192-230,790-800`);
- accepted decisions retain the complete normalized command and definition snapshot, and `replay`
  recomputes then compares the complete record (`crates/entity-core/src/runtime.rs:101-168`;
  `crates/entity-core/src/replay.rs:113-190`).

## 1. Pure pre-load decision API

The following Rust interface is normative. Fields of `PreparedOperation` are private. It implements
`Debug` only; it does not implement `Clone`, `Serialize`, or `Deserialize`, and has no public
constructor. Its lifetime binds it to the exact validated definition used during preparation.

```rust
/// What ER can answer before a subject store is read.
#[derive(Debug)]
pub enum PreloadDecision<'definition> {
    /// A named refusing branch selected without subject facts.
    Refused(Refusal),
    /// Subject facts are required before ER can finish the decision.
    Load(PreparedOperation<'definition>),
}

/// The subject coordinate a host must load for a prepared operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreparedSubject<'prepared> {
    entity: &'prepared str,
    version: u32,
    id: &'prepared str,
}

impl PreparedSubject<'_> {
    pub fn entity(&self) -> &str;
    pub fn version(&self) -> u32;
    pub fn id(&self) -> &str;
}

/// An opaque continuation over one validated definition and one normalized request.
#[derive(Debug)]
pub struct PreparedOperation<'definition> {
    definition: &'definition ValidatedDefinition,
    operation: String,
    expected_id: String,
    arguments: serde_json::Map<String, serde_json::Value>,
}

impl PreparedOperation<'_> {
    pub fn subject(&self) -> PreparedSubject<'_>;
    pub fn normalized_arguments(&self) -> &serde_json::Map<String, serde_json::Value>;
    pub fn continue_with(
        self,
        instance: &EntityInstance,
    ) -> Result<Evaluation, CoreError>;
}

/// Normalizes and evaluates as much of one operation as is possible without an instance.
pub fn decide_before_load<'definition>(
    definition: &'definition ValidatedDefinition,
    expected_id: impl Into<String>,
    operation: &str,
    arguments: serde_json::Value,
) -> Result<PreloadDecision<'definition>, CoreError>;

impl Runtime<'_> {
    pub fn decide_before_load(
        &self,
        entity: &str,
        version: u32,
        expected_id: impl Into<String>,
        operation: &str,
        arguments: serde_json::Value,
    ) -> Result<PreloadDecision<'_>, CoreError>;
}
```

`PreparedSubject` has read-only accessors so the host can perform exactly one ordinary subject
lookup. It is not a second entity identity type and is never serialized. `normalized_arguments`
is exposed by shared reference for exact request/idempotency construction; it cannot be replaced.
Consuming `self` in `continue_with` makes the intended one-load flow explicit without pretending a
pure value could prevent a caller from preparing the same request again.

One new runtime refusal binds the loaded identity:

```rust
pub enum CoreError {
    // existing variants unchanged
    SubjectMismatch {
        entity: String,
        expected_id: String,
        actual_id: String,
    },
}
```

Entity and definition-version mismatches continue to use `EntityMismatch`. `SubjectMismatch` is
only the prepared-continuation check; direct `decide` keeps every existing refusal and its order.

### 1.1 Preparation order

`Runtime::decide_before_load` first performs the existing registry lookup. The free function then
runs these steps, stopping at the first refusal:

1. refuse a blank `expected_id` as the existing `Validation` error at `id`;
2. find the operation, or return `OperationNotFound`;
3. apply defaults and validate arguments through the existing `normalize_arguments` path;
4. if direct `decide` uses the implicit transition branch, return `Load`: this includes every
   `kernel/1` operation and every branchless `service/1` or `service/2` operation;
5. otherwise inspect non-`wrong_state` outcomes in declaration order under the rules below;
6. return either `Refused`, `Load`, or an existing selection error.

No state store, provider, clock, registry mutation, record construction, or event materialization
occurs in these steps.

For each non-`wrong_state` branch in order:

- An `in_state` branch immediately returns `Load`. Its state test precedes its `when` under the
  accepted selector, so the pre-load path must not evaluate that guard first.
- Evaluate `when` with the partial condition rules below. A final `NeedsSubject` returns `Load`.
  `Known(False)` advances to the next branch; `Known(Unknown)` returns the existing
  `OutcomeUnobservable` with the actual known-input diagnostics and does not try a later branch;
  `Known(True)` selects this branch. A syntactic subject reference alone does not determine the result.
- A selector-free default is selected when reached.
- A selected branch with `refuses` returns `PreloadDecision::Refused`. Registration already ensures
  it has no effect, set, event, or response. Any selected accepting branch returns `Load`, because
  even an unchanged acceptance increments the instance revision and produces a record.
- Reaching the end returns the existing `NoOutcomeSelected` without a load.

Partial evaluation walks the existing closed `Condition` AST and resolves references through the
same admitted paths; it adds no string parser or persisted expression language. Its internal value is
`Known(Truth)` or `NeedsSubject`. Available roots are `$args`, `$id`, `$entity`, `$version`, literals,
and binders over available collections. An unloaded subject root is unavailable, not an empty object,
an absent field, JSON null, or a genuinely unobserved input. Atomic conditions with an unavailable
operand return `NeedsSubject`; otherwise they use the existing exact evaluator, including its
unobserved-input diagnostics. In particular, `exists` on an unloaded subject is not `False`.

For `all`, fold from `Known(True)`; `Known(False)` dominates every other value. For `any`, fold from
`Known(False)`; `Known(True)` dominates every other value. Two known values combine through existing
Kleene `and`/`or`. Every remaining combination containing `NeedsSubject` stays `NeedsSubject`,
including `Known(Unknown)` with `NeedsSubject`: the eventual subject might provide the dominating
false or true. `not` negates a known truth and leaves `NeedsSubject` unchanged. Evaluate every child
and retain genuine known-input diagnostics, as the existing pure evaluator does; domination settles
the selector result without pretending an unavailable subject was observed.

Quantifiers over available collections bind each actual member and fold their partial body results
with the same all/any rules. Empty universal/existential collections yield `Known(True)`/`Known(False)`
even if the unexecuted body mentions the subject. An unavailable collection yields `NeedsSubject`;
a genuinely missing or invalid available collection retains the existing `Known(Unknown)` behavior.
Binder references preserve their actual availability; a body may combine an available binder with
an unavailable subject reference. No speculative subject value or algebraic theorem prover is needed.

Thus `any: [true, {eq: ["$fields.note", "x"]}]` can select a refusal without loading, while
`all: [false, {eq: ["$fields.note", "x"]}]` skips to a later branch. Reverse child order must give
the same result. A branch whose final result still needs subject facts stops the scan; ER must never
skip it to a later default whose answer could differ after loading.

This rule yields the required `PayInvoice` behavior. Positive amount selects `settled` and returns
`Load`; zero or negative amount makes that guard false and selects the subjectless `rejected`
default without a lookup. `wrong-state` remains unreachable until a positive request loads a real
invoice and `admit_state` sees its state.

### 1.2 Continuation order and equivalence

`continue_with` checks and executes in this order:

1. loaded entity/version against the borrowed definition (`EntityMismatch`);
2. loaded `id` against `expected_id` (`SubjectMismatch`);
3. loaded state against the definition (`UnknownState`);
4. run the same named-versus-implicit dispatch as direct `decide` with the stored normalized
   arguments; named selection starts from the first outcome, while an implicit operation selects
   its existing transition branch;
5. run existing state admission, refusal, preconditions, set, validation, revision, identity mirror,
   invariants, events, response, and decision construction in their accepted order.

The continuation does not store an outcome index. Re-running selection is intentional: ER remains
the only selector, and the subject-dependent prefix is now available. The definition reference,
operation name, identity, and normalized map are private and move together. A matching valid
instance produces an `Evaluation` and accepted record byte-for-byte equal to direct `decide` on the
same definition, operation, and normalized arguments. The ordering differs only where this contract
requires it: a complete subjectless refusal or pre-load input error can return before any instance
exists.

The host has no API that converts `Load` into an acceptance or refusal. If the subject is absent,
the host returns its existing undeclared/unknown-subject boundary result and does not call
`continue_with`; ER invents neither an instance nor an outcome for that absence.

## 2. Typed conditional presence

Conditional presence changes definition syntax and the meaning of produced fields, event payloads,
responses, and records. It therefore opts into a new definition semantics value. `service/1` is not
reinterpreted.

```rust
pub enum Semantics {
    Kernel1,
    Service1,
    #[serde(rename = "service/2")]
    Service2,
}

impl Semantics {
    pub fn has_service_semantics(self) -> bool;
    pub fn has_conditional_presence(self) -> bool;
}

/// A path to one conditionally present command argument.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PresentArgument {
    /// Dot-separated path below `$args`; the serialized form never includes `$args.`.
    pub argument: String,
}

pub struct OutcomeDefinition {
    // all existing fields unchanged
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub set_if_present: BTreeMap<String, PresentArgument>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub responds_if_present: BTreeMap<String, PresentArgument>,
}

pub struct EventDefinition {
    // existing event_type and payload unchanged
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub payload_if_present: BTreeMap<String, PresentArgument>,
}
```

The authored shape is closed and explicit:

```yaml
semantics: service/2
create:
  arguments:
    fields:
      bound:
        type: object
        required: true
        properties:
          b00000000: {type: string}
  outcomes:
    - name: accepted
      effect: creates
      set_if_present:
        note: {argument: bound.b00000000}
      emits:
        - type: InvoiceCreated
          payload: {invoice_id: "$fields.invoice_id"}
          payload_if_present:
            note: {argument: bound.b00000000}
      responds_if_present:
        note: {argument: bound.b00000000}
```

The source argument leaf is optional because its `FieldDefinition.required` defaults to false.
There is no `Value` catch-all, branch specialization, expression language, or per-request
definition edit in this addition. One declared argument path is the shared bound value for every
output position that names it.

### 2.1 Registration

The three maps are accepted only under `service/2`. `kernel/1` and `service/1` definitions carrying
one receive `SemanticsKeyNotAvailable` at the exact key. `service/2` inherits every `service/1`
definition, value, condition, identity, relation, numeric, and evaluation rule; implementation
sites currently gated by `is_service_1()` change to `has_service_semantics()` only when they mean
that inherited set. Exact framing selection continues to distinguish the variants.

Registration accumulates these new `DefinitionError` variants with existing defects:

```rust
ConditionalArgumentInvalid { path: String, argument: String, message: String },
ConditionalTargetInvalid { path: String, field: String, message: String },
ConditionalTargetConflict { path: String, field: String },
ConditionalSetOnOperation { operation: String, outcome: String, field: String },
```

For every `PresentArgument`, registration requires:

1. a nonempty dot-separated `argument` with no `$`, empty segment, array ordinal, map-key lookup,
   union dynamic selection, or open-schema segment;
2. a path through declared closed `object` fields in the command argument schema;
3. every parent on the path is required, so exactly the leaf controls presence;
4. the leaf is optional and has `DeclaredDefault::Absent`; a required leaf is an ordinary template,
   and a defaulted leaf is always materialized after normalization;
5. the destination key does not also occur in the ordinary `set`, `payload`, or `responds` map;
6. `set_if_present` appears only on a creation outcome, targets a declared optional entity field
   with no default, and its complete `FieldDefinition` equals the source leaf definition;
7. `responds_if_present` targets a declared optional response field with no default and the same
   complete field definition as the source leaf;
8. `payload_if_present` is attached to an event whose ordinary `payload` is an object. Its target
   key is nonblank and absent from that object. The source leaf definition is the payload member's
   type declaration.

Exact `FieldDefinition` equality includes kind, bounds, enum values, item/property schemas, map key,
union tag/variants, and relation target data. This prevents a string-shaped bound slot from being
copied into a differently constrained entity or response field. Event payloads have no separate ER
schema today; their conditional member is typed by the referenced argument leaf and value validation
at step 3.

Refusing outcomes may carry none of the three maps. Outcome observability includes nonempty
conditional maps. Required response completeness is still satisfied only by ordinary `responds`:
conditional members may target optional response fields only.

### 2.2 Runtime meaning

Argument defaults and validation still run before branch selection. Presence is then read from the
normalized argument object:

- a missing leaf is `Absent`;
- a present leaf is `Present(the exact validated Value)`;
- a present JSON `null` is `Present(null)`, never `Absent`. It proceeds only if the declared source
  field admits null; for the concrete optional String/Timestamp/Badge slots, existing value
  validation refuses it at the argument path before selection.

The result is not exposed as a generic public value. An internal closed enum makes the branch
explicit:

```rust
enum Presence<'value> {
    Absent,
    Present(&'value serde_json::Value),
}
```

At creation step 8, ordinary `set` members are resolved and then each `set_if_present` entry is
read in key order. `Present(v)` inserts the canonical clone; `Absent` inserts nothing. Defaults and
entity-schema validation then run unchanged. Operation outcomes cannot declare this map, so no new
patch/update meaning is introduced.

At event step 13, the ordinary payload resolves first to an object, then
`payload_if_present` inserts present members in key order. At response step 14, ordinary `responds`
resolves first, then `responds_if_present` does the same. Absence never produces a key and never
produces `null`. Reusing one `PresentArgument` path in several positions copies one normalized value;
there is no second host read or second binding decision.

The final entity fields, event payloads, and response map already live in `DecisionRecord`. The
normalized arguments record whether the leaf was missing, present with a value, or present with
`null`. `replay` therefore re-applies the same presence choice and its existing complete-record
comparison detects any changed argument, output presence, or value.

## 3. Formats and compatibility

Pre-load preparation is an internal Rust capability. `PreparedOperation` is deliberately not a
wire type, so it changes no definition, request, record, batch, store, or canonical byte format.
An SDK holds it in memory across its load future. Crossing a process boundary would require a
separate versioned protocol and is outside this contract.

Conditional presence is persisted meaning. The version table is:

| Definition semantics | Record domain | Request domain | Batch domain |
| --- | --- | --- | --- |
| `kernel/1` | `er.record/1` | `er.request/1` | `er.batch/1` |
| `service/1` | `er.record/2` | `er.request/2` | `er.batch/1` |
| `service/2` | **`er.record/3`** | **`er.request/3`** | `er.batch/1` |

`er.record/3` retains the complete existing record shape and includes its `service/2` definition
snapshot with the conditional maps. `er.request/3` uses the same create-with-`arguments` and execute
request shapes as `/2`; the new domain binds retry comparison to `service/2` normalization and
output meaning. Observations remain `/1`. Each batch member retains its own record domain, so
`er.batch/1` does not move.

Compatibility rules are exact:

- all new definition maps use empty defaults and skip serialization when empty;
- every existing `kernel/1` and `service/1` definition and record serializes byte-for-byte as
  before, and both modes keep their existing behavior;
- a pre-`service/2` definition reader refuses the unknown `service/2` enum value before execution;
- current readers refuse conditional keys written under `kernel/1` or `service/1` at registration;
- a record reader that knows at most `/2` refuses literal `er.record/3` at the first array element,
  before parsing its payload, and likewise refuses `er.request/3`;
- a store containing `/3` records must not be opened by an older build; no record is rewritten or
  downgraded, and there is no implicit `/2` fallback;
- a new reader accepts `/1`, `/2`, and `/3`, selecting the request shape by domain rather than by
  the incidental presence of a key.

The implementation extends `record_domain`, `request_domain`, original-request reconstruction,
retry verification, and their readers together. A `service/2` branchless creation records and
reconstructs `arguments`, as `service/1` does; it never falls back to kernel fields.

## 4. ESS lowerer and SDK `/4` handoff

After this contract is implemented and accepted, the ESS lowerer may update its exact ER pin. It
keeps the existing `LoweredService`, definition coordinates, command bindings, and ordered slots,
and adds one closed property to the binding value:

```rust
pub enum BoundPresence {
    Required,
    Optional,
}

pub struct BoundValue {
    pub target: BoundTarget,
    pub type_ref: ResolvedTypeRef,
    pub source: BoundSource,
    pub presence: BoundPresence,
}
```

`Required` emits a required argument slot and the existing ordinary template. `Optional` emits an
optional, no-default slot and one of `set_if_present`, `payload_if_present`, or
`responds_if_present`. A definition containing any conditional map uses `service/2`; a definition
that does not need conditional presence may remain `service/1`. The lowerer never writes conditional
keys under `service/1` and never specializes an optional host decision to absence.

One semantic bound value gets one slot. If an ESS response field is reused by an entity field,
event field, and response field, all three `PresentArgument`s name that same slot, so both presence
and value are shared. Independently omitted source fields remain independent slots; the lowerer does
not merge them because their types happen to match. Required and source-determined values remain in
ordinary templates.

The concrete fixture consequences are:

- `CreateInvoice` gets optional host slots for `note` and `issued_at`; absence leaves the entity
  members absent, while a present value is copied and validated;
- `RegisterVisit` gets an optional host slot for `badge` with the same rule;
- optional event and response members use their corresponding conditional maps;
- the previous `OptionalBoundOutputUnsupported` refusal no longer applies to these supported
  top-level positions. It remains available only for a source form outside this exact contract and
  must name that path; it cannot be used for the billing or gatepass fields above.

For an existing-instance mutation, SDK `/4` constructs `{input,bound}`, derives the ER storage
address through the accepted identity function, and calls `Runtime::decide_before_load`. `Refused`
is mapped to the selected ESS error and SDK-bound payload with no state lookup, append, or event.
`Load` exposes the bound `PreparedSubject`; the SDK loads exactly that subject and passes the result
only to `continue_with`. An absent subject returns the existing undeclared/unknown-subject result.
An accepted continuation is persisted through the recorded adapter using its `/2` or `/3` domain.

The SDK does not call its `/3` host outcome selector, inspect ER guards, or construct a fresh
continuation from copied fields. Creation continues through `decide_create`; conditional arguments
are already sufficient there because no subject load is needed. Billing email remains the existing
provider/external-effect binding and is not made an ER stateless command by this work.

## 5. Implementation surfaces

The target implementation is bounded to these existing crates and files:

| File | Required change |
| --- | --- |
| `crates/entity-core/src/definition.rs` | add `Service2`, service capability predicates, `PresentArgument`, and the three skipped-empty ordered maps |
| `crates/entity-core/src/validation.rs` | inherit service rules for `/2`; dependency classification; exact conditional path/parent/leaf/target/type/conflict checks; operation-set refusal |
| `crates/entity-core/src/runtime.rs` | public pre-load types/functions, private continuation, subject check, normalized continuation path, conditional materialization at steps 8/13/14 |
| `crates/entity-core/src/error.rs` | `SubjectMismatch` and the four closed definition defects with stable messages; wrong-semantics keys retain `SemanticsKeyNotAvailable` |
| `crates/entity-core/src/replay.rs` | treat `service/2` creation input as arguments and keep event-only rehydrate refused for both service versions |
| `crates/entity-core/src/lib.rs` | export the new public types and functions |
| `crates/entity-store/src/asynchronous/encoding.rs` | exact `/3` record/request selection, reconstruction, and old-domain refusal |
| `crates/entity-store/src/asynchronous/verify.rs` | replay/retry service `/2` creation arguments without changing `/1` or `/2` behavior |
| `crates/entity-executor/src/lib.rs` | recognize `/3` request comparison exactly where `/2` creation arguments are recognized |
| `docs/design/recorded-execution-encoding-v0.1.md` | append the accepted `/3` domain table and vectors; do not rewrite `/1` or `/2` rules |
| `docs/requirements.md` | add requirement rows pinned to the named tests below |
| `CHANGELOG.md` | one user-facing line under `Unreleased` |

No new crate or dependency is required. `entity-core` remains limited to `serde` and `serde_json`,
contains no IO or async runtime, and retains Rust 1.85. Provider schemas need no column or envelope
migration because their stored comparison bytes already carry a domain-tagged value.

## 6. Required verification

Add `crates/entity-core/tests/service_binding_boundary.rs` with these named behavioral tests:

- `a_nonpositive_payment_refuses_before_any_subject_load`
- `a_positive_payment_requires_the_exact_subject_and_continues_in_er`
- `a_preload_scan_stops_at_the_first_subject_dependent_branch`
- `implicit_kernel_and_branchless_service_operations_preload_then_match_direct_decide`
- `kleene_dominating_guards_refuse_without_loading_an_unknown_subject`
- `preload_quantifiers_preserve_empty_collections_and_available_binders`
- `unloaded_subject_existence_is_not_confused_with_an_absent_field`
- `a_state_guard_is_not_evaluated_before_the_subject_is_loaded`
- `an_unknown_input_guard_refuses_without_trying_a_later_default`
- `a_prepared_operation_binds_definition_input_operation_and_identity`
- `conditional_presence_distinguishes_absent_present_and_null_in_all_output_positions`
- `one_optional_argument_reuses_one_presence_and_value_across_state_event_and_response`
- `conditional_presence_refuses_wrong_types_and_missing_required_neighbors_before_selection`
- `service_2_replay_recomputes_presence_and_refuses_every_tampered_position`
- `kernel_1_and_service_1_keep_their_definitions_decisions_and_refusal_order`

The `PayInvoice` matrix covers positive, zero, and negative amounts against both known and unknown
invoice IDs. Zero and negative return `rejected` without invoking a counting load stub and produce
no record. Positive/known loads and settles from `Issued`; positive/known in `Draft`, `Paid`, or
`Cancelled` loads and returns `wrong-state`; zero or negative on those same instances still returns
`rejected` before state. Positive/unknown returns `Load`, after which the host reports absence and
does not call the continuation.

The continuation binding test passes a wrong entity/version, a wrong id, and an unknown-state
instance, asserting the exact order `EntityMismatch`, `SubjectMismatch`, `UnknownState`. It also
uses a rustdoc `compile_fail` example to pin that external code cannot construct, destructure, clone,
serialize, or replace fields of `PreparedOperation`. A matching continuation is compared with direct
`decide` for exact `Evaluation` and serialized accepted record equality.

Implicit-branch cases cover `kernel/1`, branchless `service/1`, and branchless `service/2`; each
returns `Load` and matches direct `decide` on a valid instance, including transition/precondition
refusals and exact accepted bytes. Named operations retain their separate ordered selection.
Partial-condition cases cover both child orders of dominating all/any, a later default refusal,
unknown subject identities, missing known inputs versus unloaded fields, `not`, nested quantifiers,
empty collections, and `Known(Unknown)` combined with subject dependency before/after loading.
They must not treat an unloaded field as missing for `exists` or prematurely turn dependency into
`OutcomeUnobservable`.

The presence test uses one `service/2` creation branch whose optional String argument is referenced
from `set_if_present`, one emitted event's `payload_if_present`, and
`responds_if_present`. With the key absent, all three outputs omit it. With it present, all three
contain the exact same value. With JSON `null` or an integer, String validation refuses at the
argument path before selection. A separate required ordinary slot proves that missing required data
still refuses, and fixed ordinary entity/event/response members prove determined values remain
unchanged. Repeat absent and present records through `replay`; tamper each normalized argument,
entity member, event member, and response member and require replay refusal.

Add `crates/entity-store/tests/service_2_framing.rs` and executor retry tests that pin literal
`er.record/3` and `er.request/3` vectors, mixed `/1`/`/2`/`/3` members inside unchanged
`er.batch/1`, request reconstruction for absent versus present slots, and refusal by a reader capped
at `/2` before an unreadable payload is parsed. Preserve the existing `/1` and `/2` literal vectors
unchanged. The capped-reader refusal and unchanged `/1`/`/2` vectors are written and observed red
before the production structs gain any conditional field; they are the required old-reader
compatibility decision, not a retrospective assertion.

The lowerer/SDK integration unit, after the pin moves, must compile the actual billing and gatepass
fixtures and assert `note`, `issued_at`, and `badge` optional slots plus the full payment matrix,
restart/replay, no lookup/append on subjectless refusals, and no `/3` host selector call.

Finite causal controls, each applied alone and reverted:

1. force load before the pre-load scan: the unknown-invoice zero/negative tests fail;
2. skip a subject-dependent branch and inspect a later default: the ordered dependency test fails;
3. evaluate an `in_state` branch guard before load: the state-guard test fails;
4. materialize absent as JSON `null`: all three absence assertions and record bytes fail;
5. ignore a present optional argument: all three presence assertions fail;
6. allocate/read a second value for one output: shared-value equality and replay fail;
7. frame `service/2` as `/2`: the literal domain and old-reader controls fail.
8. replace partial evaluation by a syntactic subject-reference scan: both dominating-guard and
   empty-quantifier cases fail;
9. scan an implicit operation's empty outcomes instead of returning `Load`: the three semantics
   equivalence cases fail.

Implementation runs focused core/store/executor suites, strict Clippy, formatting, rustdoc,
Rust 1.85 checks, and the repository `task check` with its ordinary PostgreSQL rule. The lowerer and
SDK run their own focused fixture tests and repository gates after pinning. This design assignment
runs only its ESS model validation/compile checks and performs no Cargo build.

## 7. Completion boundary

This contract is implementable without a new prerequisite. It closes the design question for both
approved gaps; it does not claim their source implementation, lowerer admission, durable adapter
acceptance, or SDK `/4` completion. The next action is the separately assigned Entity Runtime
implementation at this exact boundary, followed by the already pending lowerer final review after
root integrates the interface. No existing pure ER review is reopened and no additional review or
planning item is created here.
