# Pure service semantics v0.1 — branches, refusals, responses, identity, values and relations in the kernel

Status: proposed, 2026-09-16, under ESS evolution revision 1 step 6 (M5/M6), for
`task:service-semantic-contract`. This document is the single contract for the ER semantics ESS
service lowering needs. It adds closed definition keys, closed field kinds, closed condition
operators, one new kernel result, named refusals and one new record framing version. It introduces
no IO, no ESS or AEP dependency in `entity-core`, no SQL, no adapter, and no generic registry.
Accepting it does not close M5/M6 and does not itself implement anything.

## 0. How to read a citation here

Unprefixed paths are **repository-relative in this repository**: `crates/entity-core/src/runtime.rs:315`
is a file of the entity-runtime tree this document sits in.

`ESS/` prefixed paths are **repository-relative in the ESS repository**, at the revision recorded as
`f1af8280338b97d862a6c474ec50f78d5157d71c` in the measured crosswalk
(`local-evidence:.ess-evolution/waves/0009-service-convergence/source-routing-result.md`, its § *Source
vector*). That crosswalk's ten rows are not re-surveyed here; where this document contradicts one, § 15
says so and why. `SDK/` is the Service SDK repository, read only.

Two statements below are **executable witnesses** rather than readings: a model was authored and the
installed `ess specify validate` was run over it. Each is marked *witness Wn*. The models are in
`local-evidence:.ess-evolution/waves/0009-service-convergence/semantic-contract/completion/witnesses/`
and the verbatim output and exit code of every run is in that directory's
`../ess-specify-validate.txt`. Where this document writes `witnesses/<name>/`, that is the path it
means.

The ESS side of the boundary is `ESS/crates/specify/ess-service-contract`, which hands a consumer whole
`ResolvedCommand` / `ResolvedEntity` / `PlannedCapability` values borrowed from an immutable `EssIr`
(`ESS/crates/specify/ess-service-contract/src/lib.rs:34-118`). Everything below is what an ER target
needs from this side so that `ess-entity-runtime` can project those values without inventing meaning.

## 1. Three version axes, and the format versions that do move

| Axis | Where | What it identifies | Moves here? |
| --- | --- | --- | --- |
| **Definition identity** | `EntityDefinition.version: u32` (`crates/entity-core/src/definition.rs:32-34`) | *Which entity type*. An instance is bound to `(entity, version)` and executes against that definition only (`crates/entity-core/src/runtime.rs:29-33`, `ensure_instance_matches` at `:827-853`). | No |
| **Definition semantics** | new `EntityDefinition.semantics` | *Which document rules apply* — which keys exist, which operators exist, and how the kernel evaluates them. | New: `service/1` |
| **Record framing** | `er.record/1`, `er.request/1`, `er.batch/1` (`docs/design/recorded-execution-encoding-v0.1.md:41-70`) | *How a stored decision is spelled for comparison.* | **Yes — see § 1.2** |

`version` must not carry a semantics change: two definitions differing only in evaluation rules are
still one entity type to every stored instance, and bumping `version` to signal a document shape would
claim a new type nobody created an instance under. So the opt-in is its own closed key:

```rust
/// Which document rules a definition is read under. Absent means `Kernel1`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum Semantics {
    #[default]
    #[serde(rename = "kernel/1")]
    Kernel1,
    #[serde(rename = "service/1")]
    Service1,
}
```

on `EntityDefinition` as
`#[serde(default, skip_serializing_if = "Semantics::is_kernel_1")] pub semantics: Semantics`.

### 1.1 Byte preservation for `kernel/1`

Every definition key and every record key this contract adds is `#[serde(default)]` with a
`skip_serializing_if` that is true for the value a `kernel/1` definition or record has. A `kernel/1`
definition, its snapshot inside a `DecisionRecord` (`crates/entity-core/src/runtime.rs:124,469-474`)
and every committed fixture therefore serialize to the bytes they serialize to today. `create`'s
existing `{"emit":null}` spelling is untouched. This is a required test, not a hope: § 11 pins it over
the committed fixtures.

The same rule governs **behaviour**, and it is the harder half. Every semantic change in § 4, § 7 and
§ 10 — the move-source rule, the identity address, the numeric domain, the new operators, collection
addressing — is reached only from a `Semantics::Service1` definition. A `kernel/1` definition gets the
evaluation it gets today, refusal variant for refusal variant. § 4.2, § 10.2.1 and § 10.6 name the
three places where that gating is not a compile-time distinction and must be a read of
`definition.semantics`: the evaluation order's inserted steps, the numeric reading a predicate and a
schema bound use, and the two collection address forms.

`DomainEvent` (`crates/entity-core/src/runtime.rs:53-94`) gains **no key**. Its `args` key does change
meaning on a `service/1` creation event — § 5.4 — which is why § 1.2 moves the framing rather than
claiming the event is untouched.

### 1.2 The framing versions this contract mints, and why

`docs/design/recorded-execution-encoding-v0.1.md:4-5` states the governing rule for these three
formats: *"A future change to their shapes, framing or scalar spelling needs a new version domain and
explicit migration."* `RecordValue` is defined as the existing Rust type's serialized fields **in full**
(`:46-48`). A `service/1` decision's record carries keys `er.record/1` has never carried — `outcome`,
`effect`, `response`, and a `command` whose `create` shape has an `arguments` key. That is a shape
change, so it takes a new domain:

| Framing | `kernel/1` decision | `service/1` decision |
| --- | --- | --- |
| Record comparison | `er.record/1`, byte identical to today | **`er.record/2`** |
| Original-request reconstruction | `er.request/1`, byte identical to today | **`er.request/2`** |
| Batch | `er.batch/1`, unchanged | `er.batch/1`, unchanged |

The batch tag does **not** move. `BatchValue` nests each `RecordValue` as a whole tagged array
(`docs/design/recorded-execution-encoding-v0.1.md:66-69`), so a reader that walks a batch meets the
`er.record/2` tag and refuses there, by the name of the framing it does not know. Moving the batch tag
as well would put two tags on one refusal and would rewrite the comparison bytes of a batch whose
members are all `/1`. § 11 pins both halves.

`er.record/2` is also where a `service/1` **storage address** lives. For a definition whose identity
field is text-like, that address is the `s:`-prefixed value of § 7.3.1 — a new value domain inside a
new framing, and not a claim that any `er.record/1` byte changed. Nothing rewrites a stored `/1`
envelope and no `kernel/1` `id` moves, because a `kernel/1` definition cannot declare `identity`
(§ 2.1). § 11 pins the two claims separately.

`er.request/2` carries the two command shapes, with the create shape replaced:

```text
{"kind":"create","subject":[entity,id],"definition_version":v,"arguments":normalized_arguments,"recording":R}
{"kind":"execute","subject":[entity,id],"expected_revision":p,"operation":op,"arguments":normalized_arguments,"recording":R}
```

Observations retain `er.request/1`. They carry no definition snapshot from which to select
`semantics`; `record_domain` and `request_domain` therefore select `/1` for observations under
both modes (`crates/entity-store/src/asynchronous/encoding.rs:68-85`). The earlier third `/2`
observation row was a documentation error; this correction changes no recorded bytes.

A `service/1` creation's *original request* is the caller's **arguments**, not the fields the branch
produced. Reconstructing it as `"fields"` would hand a retry a request the caller never sent, which is
what `original_request_comparison_bytes` exists to prevent
(`crates/entity-store/src/asynchronous/encoding.rs:85-141`).

**A `service/1` creation that declares no branches is the case where the two coincide, and it is
stated rather than left to be inferred.** Registration admits such a definition — `create.outcomes`
is a list and an empty one is not a defect — and with no branch to produce them the creation reads
its input **as** its fields, exactly as a `kernel/1` one does. Those same normalized values are what
the caller sent, so they are what it records as its `arguments` and what `er.request/2`
reconstructs. This is what keeps four readers on one test: the framing, the creation path, `replay`
and the anchored verifier all switch on `semantics` alone. Reading the input as the fields while
recording no arguments would reconstruct **every** branchless creation as the same empty request,
which is precisely the collapse this section exists to prevent, and narrowing the framing to
`er.request/1` instead would spell a `service/1` record in a domain whose shape it does not have.
§ 11 pins it.

### 1.3 Old-reader rejection, before implementation

Two mechanisms, both already present, and each pinned by a named test in § 11 before any of this is
implemented:

* **Definitions and records**: every definition and record struct is `#[serde(deny_unknown_fields)]`
  (`crates/entity-core/src/definition.rs:104,121`, `crates/entity-core/src/runtime.rs:98,121`;
  AGENTS.md invariant 11). A build that predates `service/1` refuses a `service/1` definition or record
  by naming the unknown key rather than reading a definition whose branches it would silently not
  evaluate. The same attribute refuses a `service/1` **condition operator**: `Condition`'s hand-written
  `Deserialize` admits exactly one known operator and names what it found (AGENTS.md invariant 11), so
  `for_all`, `for_any`, `truthy` and `compare` are refused by name by a pre-`service/1` build.
* **Framings**: an `er.record/1` reader meets the literal string `er.record/2` in the first array
  element and refuses the document, without parsing its second element at all.

The operational rule that follows: a store holding `service/1` records must not be opened by a
pre-`service/1` build. Nothing rewrites an existing stored envelope.

## 2. What is added to the definition model

All of it in `crates/entity-core/src/definition.rs`, all `deny_unknown_fields`, all ordered
(`BTreeMap`/`Vec`), none of it a property bag.

```rust
pub struct EntityDefinition {
    // existing: entity, version, schema, lifecycle, invariants, create, operations, projections
    pub semantics: Semantics,                              // § 1
    pub identity: Option<IdentityDefinition>,              // § 7
    pub relations: BTreeMap<String, RelationDefinition>,   // § 8
    pub scales: BTreeMap<String, Vec<String>>,             // § 10.3, ordered, lowest rank first
    pub number_observation: NumberObservation,             // § 10.2.1, which reading rule answers numbers
}

/// Which versioned source-number observation rule a `service/1` definition is answered under.
///
/// Frozen into the definition, and so into the `DecisionRecord`'s definition snapshot, because a
/// later ESS read-door stage must not change what an already recorded decision meant — § 10.2.1.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum NumberObservation {
    #[default]
    #[serde(rename = "source-number/1")]
    SourceNumber1,
}

/// The schema field that carries the instance's logical identity.
pub struct IdentityDefinition { pub field: String }

pub struct RelationDefinition {
    pub kind: RelationKind,          // Owns | References
    pub target: String,              // an entity name
    pub cardinality: Cardinality,    // One | Many
    pub via: String,                 // the field carrying it — on the TARGET for Owns, § 8
}

pub struct CreateDefinition {
    pub emit: Option<EventDefinition>,     // existing, `kernel/1` spelling
    pub arguments: ObjectSchema,           // new: the creation command's input
    pub response: ObjectSchema,            // new: the creation command's declared response, § 5.3
    pub outcomes: Vec<OutcomeDefinition>,  // new: ordered named branches
}

pub struct OperationDefinition {
    // existing: arguments, transitions, preconditions, set, emits
    pub response: ObjectSchema,            // new: the command's declared response, § 5.3
    pub outcomes: Vec<OutcomeDefinition>,  // new: ordered named branches
}

/// One named branch of a creation or an operation.
pub struct OutcomeDefinition {
    pub name: String,
    pub when: Option<Condition>,            // ESS When, and SubjectState's optional input guard
    pub in_state: Option<String>,           // ESS SubjectState's state
    pub wrong_state: bool,                  // ESS WrongState
    pub effect: OutcomeEffect,
    pub set: BTreeMap<String, Value>,
    pub emits: Vec<EventDefinition>,
    pub responds: BTreeMap<String, Value>,  // § 5.3
    pub refuses: Option<RefusalDefinition>,
}

pub enum OutcomeEffect {
    None,                                   // accepts and changes no state (default)
    Creates,                                // § 5.2 — creation branches only
    Moves { to: String, from: OneOrMany<String> },
    Updates,                                // § 6
}

pub struct RefusalDefinition { pub error: String, pub message: Option<String> }
```

`OperationDefinition.transitions` gains `#[serde(default)]` so a `service/1` operation may carry
branches instead; its serialization is unchanged and the existing `NoTransitions` refusal
(`crates/entity-core/src/validation.rs:150-158`) still fires for every `kernel/1` operation and for a
`service/1` operation that declares neither transitions nor outcomes.

**`kernel/1` is `service/1` with one implicit branch.** A `kernel/1` operation reads as a single
unnamed outcome whose applicability is the existing transition selection and whose effect is `Moves`.
The implementation is one evaluation path, not two; what `kernel/1` keeps is the **exact refusal
variants it returns today** (`InvalidTransition`, `PreconditionFailed`, …), which § 4 pins.

### 2.1 Refused at registration

New `DefinitionError` variants, accumulated like every other defect
(`crates/entity-core/src/validation.rs:16-51`).

| Refusal | Condition | Mirrors |
| --- | --- | --- |
| `SemanticsKeyNotAvailable` | a `kernel/1` definition carries `identity`, `relations`, `scales`, `create.arguments`, `create.response`, `create.outcomes`, `operations.*.response` or `operations.*.outcomes`, or any condition uses `for_all`, `for_any`, `truthy` or `compare` | — |
| `EmptyOutcomeName` / `DuplicateOutcome` | a branch name is blank, or repeats within one creation or operation | — |
| `AmbiguousDefaultOutcome` | more than one branch declares neither `when` nor `in_state` nor `wrong_state`, or the one that does is not the last non-`wrong_state` branch (§ 4.3) | ESS one-default rule, `ESS/crates/specify/ess-domain/src/command.rs:1615-1631` |
| `DuplicateWrongStateOutcome` | more than one `wrong_state` branch in one operation | `ESS/crates/specify/ess-domain/src/command.rs:1728-1751` |
| `WrongStateOnCreate` / `WrongStateWithSelector` | `wrong_state` on a creation branch, or beside this branch's own `when`/`in_state` | — |
| `WrongStateWithStateGuard` | a `wrong_state` branch in an operation that also declares any `in_state` branch | `ESS/crates/specify/ess-domain/src/command/subject_state.rs:31-37`; `ESS/docs/design/subject-state-outcome-guards.md:38-40` |
| `WrongStateUnreachable` | `wrong_states(operation)` (§ 4.3) is empty and a `wrong_state` branch is declared | `ESS/crates/specify/ess-domain/src/entity.rs:1042-1102` |
| `GuardStateOutsideMove` | an `in_state` branch whose effect is `Moves` names a state not in that branch's own `from` | `ESS/crates/specify/ess-domain/src/command/subject_state.rs:87-103,206-228` |
| `CreatesEffectOnOperation` / `MissingCreatesEffect` | `effect: Creates` on an operation branch, or a creation branch whose effect is not `Creates` or `None` | `ESS/crates/specify/ess-compiler/src/ir.rs:559-572` |
| `RefusalMutatesState` | a branch declares `refuses` together with any of `effect != None`, `set`, `emits`, `responds` | ESS `RefusalMutatedState`, `ESS/crates/specify/ess-domain/src/command.rs:1268-1312` |
| `UnobservableOutcome` | a branch declares no `emits`, no `set`, no `responds`, no `refuses` **and** `effect == None`, unless it is an accepting `wrong_state` branch (§ 5.2) | a **weakened** ESS `EmptyChange`, `ESS/crates/specify/ess-domain/src/command.rs:1246-1266` — see § 5.2 |
| `UnknownOutcomeState` | `in_state`, `effect.to` or `effect.from` names a state the lifecycle does not declare | — |
| `ResponseFieldUnknown` | a `responds` key the operation's `response` schema does not declare | — |
| `OutcomeResponseIncomplete` | an accepting branch does not determine every `required` field of the `response` schema | `ESS/crates/specify/ess-compiler/src/ir.rs:810-812` |
| `IdentityFieldUnknown` / `IdentityFieldNotAddressable` | `identity.field` is not a declared **required** field, or its kind is one § 7.3's address function does not admit | `ESS/crates/verify/ess-conformance/src/input.rs:153-155` |
| `RelationViaUnknown` / `RelationViaWrongShape` | for a `References` relation: `via` is not a declared field of **this** definition, or it is not a list on the `Many` row, or **at `Registry::validate_all`** its kind is not the one § 8.1's row admits. The kind half is the registry's on both rows because it is a claim about the *target's* identity field (§ 8.2). **Optionality is not part of this test for a `References`/`One` carrier**: both `required: true` and `required: false` are admitted, because `carried_types` admits both the target's identity type and `Optional<it>` | `ESS/crates/specify/ess-domain/src/entity.rs:1392-1407` |
| `RelationTargetMissing` / `RelationSecondOwner` / `RelationCarrierWrong` / `RelationFieldClaimedTwice` | at `Registry::validate_all`: an unregistered target; two definitions owning one entity; an `Owns` `via` that is not a correctly shaped **required** field of the **target** — its kind is the **source's** identity kind, once, on both cardinalities, so an array carrier is refused where that identity is not itself a list and admitted where it is; one field carrying two relations | `ESS/crates/specify/ess-domain/src/entity.rs:1213-1274,1392-1407` |
| `RelationCarrierOptionality` | a `References`/`Many` carrier or an `Owns` carrier declared `required: false` — `carried_types` offers `Optional` for the `References`/`One` row and for no other | `ESS/crates/specify/ess-domain/src/entity.rs:1392-1407` |
| `MapKeyNotText` / `MapValueMissing` / `UnionTagCollides` / `UnionVariantMissing` | the new field kinds of § 10.1 | `ESS/crates/generate/ess-gen/src/types.rs:527-548,696-733` |
| `QuantifierBindInvalid` / `QuantifierOverNotCollection` / `QuantifierBodyScope` | the quantifier of § 10.4: `as` is not one non-empty path segment; `in` does not name an `array` or `map` field; the body reads an address neither the enclosing scope nor the binder admits | `ESS/crates/specify/ess-domain/src/expression.rs:687-701`; `ESS/crates/specify/ess-primitives/src/predicate.rs:383-391` |
| `ConditionTooDeep` | a condition nests deeper than 32 | `ESS/crates/specify/ess-primitives/src/predicate.rs:292-298` |
| `CompareOperandNotAddressable` | a `compare` operand is a literal object or array (§ 10.4: ESS compares scalars only) | `ESS/crates/verify/ess-conformance/src/decision.rs:171-181` |
| `ScaleUnnamed` / `ScaleEmpty` | a `scales` key is blank, or its value list is empty | — |

**`number_observation` is not on that list, and cannot be.** An earlier draft of this row named it
beside the other four keys. `NumberObservation` (§ 2) has exactly one variant, which is its own
`Default` and is skipped on serialization, so a definition document that spells
`number_observation: source-number/1` deserializes to the same typed value as one that omits the key
and no check downstream of `serde` can tell the two apart. The row was therefore unimplementable as
written rather than unimplemented, and it is removed rather than left standing as a claim nothing
enforces. What is **kept** is everything that does not depend on that distinction: `service/1`
remains an opt-in, a variant name this build does not know is still refused at decoding, `kernel/1`
definitions still omit the key from their bytes, and the snapshotted rule still decides what a
recorded decision meant (§ 10.2.1). Adding a second variant or a presence flag to recover the
refusal is **not** done here: it would change a format to enforce a rule nothing needs. If a later
`source-number/2` lands (§ 12), the key becomes distinguishable on its own and the refusal can be
stated then.

Three refusals earlier drafts of this document declared are **removed**, each because it refused
something the source admits:

| removed | it refused | replaced by |
| --- | --- | --- |
| `AmbiguousMoveSource` | a command the installed tool admits — witness W2 | the run-time `UnspecifiedMoveSource`, § 4.4 |
| `IdentityFieldNotText` | every non-text identity — witness W1 | the per-kind address function, § 7.3 |
| `IdentityAddressEmpty` | an empty or whitespace `String` identity, which ESS admits | the total `s:` rule, § 7.3.1 — the address can no longer be empty, so the check is unreachable rather than relaxed |

One refusal is **added** beside them: `RelationCarrierOptionality`, in the table above, which keeps
`Owns` and `References`/`Many` at `required: true` now that `References`/`One` admits both.

### 2.2 Reference scopes

`ScopeKind` (`crates/entity-core/src/validation.rs:266-304`) gains four variants. The existing four
are unchanged — in particular `CreateTemplate` keeps its exact current admitted set
(`$id`, `$entity`, `$version`, `$state`, `$to_state`, `$fields`, `$fields.<path>`;
`crates/entity-core/src/validation.rs:282-284`) and stays the scope every `kernel/1` creation event
payload is checked in.

| Scope | May read | Why not the rest |
| --- | --- | --- |
| `OutcomeSelector` | `$id`, `$entity`, `$version`, `$from_state`, `$args`, `$args.<path>`, `$fields`, `$fields.<path>` | Not `$state` and not `$to_state`: at selection the destination state is what the **selected branch's effect produces**, so a selector reading it would read a value the selection is deciding. Not `$old_fields`: nothing has been written yet, so it is `$fields` under a second name. This is **not** "the precondition scope minus `$state`" — the precondition scope already excludes `$state` and already admits `$to_state` (`crates/entity-core/src/validation.rs:278-281,318-319`), which is exactly the value a selector may not see. |
| `CreateSelector` | `$id`, `$entity`, `$version`, `$args`, `$args.<path>` | At creation there is no instance: no fields, no from-state, and the initial state is a constant of the lifecycle rather than a fact about a subject. |
| `CreateSet` | `$id`, `$entity`, `$version`, `$state`, `$to_state`, `$args`, `$args.<path>` | Not `$fields`: at creation the fields are what `set` is producing. |
| `CreateOutcomeTemplate` | `$id`, `$entity`, `$version`, `$state`, `$to_state`, `$args`, `$args.<path>`, `$fields`, `$fields.<path>` | The creation branch's `emits` payloads and its `responds` map, and **only** a `service/1` creation branch's. Both are materialised after the branch's `set`, so both read the **post-`set` fields**; both are published to a caller that sent the arguments, so both read `$args`. Not `$old_fields` and not `$from_state`: at creation there is no previous instance. |

**A binder extends whichever of these scopes encloses it**, and only for the duration of the body:
inside `for_all: {in: $fields.lines, as: line, that: …}` the body additionally reads `$line` and
`$line.<path>`, checked against the element schema of `$fields.lines`.

**Every address that does not match the binder passes through to the enclosing scope untouched**,
which is what lets a body mix element facts with free ones
(`ESS/crates/specify/ess-primitives/src/predicate.rs:425-428`). An address that **does** match is the
bound element, and that holds for a binder spelled like one of the fixed roots as well: `as: id`
makes `$id` the element inside the body, not the storage address. This sentence read *"nothing else
changes"* before, which was vague enough to be read as the fixed roots taking precedence; they do
not. `Element::rebind` rewrites **any** matching first namespace and passes every other namespace
through (`ESS/crates/specify/ess-primitives/src/predicate.rs`, around `:435`), and § 10.4 says the
same in its own words — `$<bind>` is the element, every other address passes through, and a nested
`as` equal to an enclosing one is admitted with the inner one winning. Giving a fixed root
precedence over a binder, or refusing a binder named after one, would be narrower than the source,
so neither is done. § 11 pins the shadowing and the pass-through separately.

### 2.3 Why the creation template scope carries the arguments

A `service/1` creation event or response must be able to publish a creation **argument that no field
stores**, and the existing `CreateTemplate` scope cannot: it admits no `$args` at all
(`crates/entity-core/src/validation.rs:282-284`), and `create` builds its `TemplateContext` with
`args: &empty` (`crates/entity-core/src/runtime.rs:290-299`).

The regression is in the required billing fixture rather than in a constructed example.
`billing.invoice.CreateInvoice`'s `accepted` branch declares

```yaml
payload:
  billing.invoice.InvoiceCreated:
    account_id: input.account_id
    customer_email: input.customer_email
    amount: input.amount
sets:
  account_id: input.account_id
  total: input.amount
```

(`ESS/examples/billing/domains/invoice.yaml:229-245`). `customer_email` is in the **payload** and not
in `sets`, and the comment beside `sets` says which omissions are deliberate. Under `CreateTemplate`
the lowerer would have to invent a field to hold it, which changes the entity the source declared.
`CreateOutcomeTemplate` reads `$args.input.customer_email` directly, and the `account_id` line reads
either the argument or `$fields.account_id` — the lowerer emits the argument form, so one rule covers
both blocks.

Three consequences, all positive rather than by subtraction:

* `create`'s `TemplateContext.args` carries the **normalized creation arguments** for a `service/1`
  definition, and stays the empty map for a `kernel/1` one. `kernel/1` creation event payloads are
  therefore resolved in exactly the context they are resolved in today.
* `responds` on an **operation** branch is resolved in the existing `OperationTemplate` scope;
  `responds` on a **creation** branch is resolved in `CreateOutcomeTemplate`. Both at step 14 of
  § 4.2.
* `emits` on an **operation** branch stays `OperationTemplate`; `emits` on a `service/1` **creation**
  branch is `CreateOutcomeTemplate`. `CreateDefinition.emit` — the `kernel/1` spelling — stays
  `CreateTemplate`, unchanged and untouched.

Creation selects its input and event keys according to whether `create.outcomes` is empty:

| `create.outcomes` | Input schema | Emitted events |
| --- | --- | --- |
| Non-empty | `create.arguments` | The selected branch's `emits` |
| Empty (branchless) | `definition.schema` | `create.emit` |

Consequently, `create.emit` is unused beside outcomes, and `create.arguments` and its defaults
are unused on the branchless path. These combinations remain admitted. Branchless creation
records its normalized input as arguments, preserving the request reconstruction in § 1.2.

## 3. What stays outside

Authentication, authorization, hosting, HTTP, queries, content policy and external effects remain SDK
bindings and are not represented here. `projections` keep their existing meaning and are still
executed by the shell (`crates/entity-core/src/definition.rs:59-65`). The kernel still has no clock, no
identifier generator and no lookup (AGENTS.md invariants 1 and 7): nothing below adds one. `scales`
(§ 10.3) is a **declaration carried in the definition**, not a lookup: it is snapshotted into every
`DecisionRecord` with the rest of the definition and replay reads the snapshot.

## 4. Evaluation order

### 4.1 What the existing contract actually is

`docs/design/kernel-v0.1.md:344-357` lists **twelve** steps, numbered 0 to 11. This contract inserts
branch selection and the identity mirror into that list; it does not restate it as something it is not.
Two documents are wrong about it and the implementation corrects both in the same commit:

* `AGENTS.md` invariant 8 calls it "the eleven-step evaluation order" (`AGENTS.md:111`);
* the code checks `EntityMismatch` **before** `UnknownState` inside `ensure_instance_matches`
  (`crates/entity-core/src/runtime.rs:827-850`), which is the reverse of `kernel-v0.1.md`'s 0/1
  numbering.

The implementation keeps the code's order, corrects both documents' numbering, and § 11 names the test
that pins the code's order so the documents cannot drift back.

### 4.2 The `service/1` order, numbered

**Sixteen steps, numbered 0 to 15, and these numbers are the ones every other section of this
document cites.** A refusal at any step returns before the next.

The *legacy* column is the corrected twelve-step `kernel/1` order of § 4.1 — the code's order, so
`EntityMismatch` is legacy 0 and `UnknownState` is legacy 1, which is the correction
`kernel-v0.1.md:344-346` takes in the same commit. A step whose legacy column says **new** is this
contract's insertion; the four insertions are 4, 6, 11 and 14 and nothing else moves.

```text
  #  step                                                     refusal                 legacy
  0  instance (entity, version) matches the definition        EntityMismatch          0
  1  instance carries a state the definition declares         UnknownState            1
  2  operation exists                                         OperationNotFound       2
  3  arguments: defaults, then validation                     Validation              3
  4  input selection                                          NoOutcomeSelected       new
                                                              OutcomeUnobservable
  5  state admissibility of the selected branch               InvalidTransition       4
                                                              UnspecifiedMoveSource
  6  a refusing branch returns here                           Evaluation::Refused     new
  7  preconditions, against current state + arguments         PreconditionFailed      5
  8  the selected branch's set, against pre-operation fields  Template                6
  9  resulting fields validated against the schema            Validation              7
 10  next instance: state from the branch's effect, +1 rev    —                       8
 11  identity mirror, when declared                           IdentityMismatch        new
 12  invariants, against the next state                       InvariantViolation      9
 13  the selected branch's events, in declaration order       Template                10
 14  the selected branch's response, in schema order          Template                new
 15  Evaluation::Accepted(Decision)                           —                       11
```

**Identity, invariants, events and responses have one order and it is this one: 11, 12, 13, 14.**
The identity mirror runs before the invariants so that a rule judging the instance judges one whose
address and identity field already agree; the response is materialised after the events so that both
read one set of post-`set` fields and the events keep the position `kernel/1` gives them, last before
the decision.

**`kernel/1` keeps this order and its refusal names exactly.** Its single implicit branch makes step 4
a no-op, makes step 5 answer `InvalidTransition` and only `InvalidTransition` (§ 4.3), and makes
steps 6, 11 and 14 absent. Renumbering steps 5 to 15 against the legacy column recovers the twelve-step
list unchanged.

**Creation** runs 3, 4, 6, 8, 9, 10, 11, 12, 13, 14, 15 and omits 0, 1, 2, 5 and 7. It uses
`CreateSelector` at step 4 in place of `OutcomeSelector`, `CreateSet` at step 8 in place of the
operation `set` scope, and `CreateOutcomeTemplate` (§ 2.2) at steps 13 and 14. At step 10 the state is
the lifecycle's `initial` and the revision is 1 rather than a successor.

### 4.3 Selection, exactly

Two sets, both computed at registration from the definition alone, and both `service/1`-only:

```text
move_sources(op) = ⋃ { branch.effect.from : branch ∈ op.outcomes, branch.effect is Moves }
wrong_states(op) = lifecycle.states \ move_sources(op)
```

That is `StateMachine::wrong_states` (`ESS/crates/specify/ess-domain/src/entity.rs:287-300`) with the
union taken over the same set `EssIr::wrong_states` takes it over — every outcome of the command that
carries a transition (`ESS/crates/specify/ess-compiler/src/ir.rs:1723-1733`). ESS's map is keyed by
entity because one ESS command may move several; an ER operation is declared inside one definition and
acts on one instance, so the map collapses to one row and nothing is dropped.

**Input selection.** Walk the non-`wrong_state` branches in **declared order**. For each branch, in
this order:

1. **The state test.** If `in_state` is declared and is not the instance's current state, **skip the
   branch**. Its `when`, if it has one, is not evaluated: a guard whose branch the held state has
   already excluded cannot make the command unobservable.
2. **The selector test.** Otherwise the branch's selector is `when` if it declares one, and
   **`True`** if it does not. `True` takes the branch; `Truth::False` skips it; `Truth::Unknown`
   refuses (below).

Three shapes fall out of those two lines, and each is a different branch of the source:

| branch | selector | ESS spelling |
| --- | --- | --- |
| no `in_state`, `when` declared | `when`, evaluated normally | `When(p)` |
| `in_state` declared, `when` declared | the state test, then `when` evaluated normally | `SubjectState { state, predicate: Some(p) }` |
| `in_state` declared, no `when` | the state test, then **`True`** | `SubjectState { state, predicate: None }` |
| neither — the default, declared last | `True` if reached | `Otherwise` |

**A matching `in_state` with no `when` is a `True` selector, and that is the source's own reading
rather than a convenience.** ESS's finite selection analysis builds one `StateGuard { state,
predicate }` per branch and then evaluates `guard.predicate.unwrap_or(&always)`, where `always` is
`Predicate::Always` (`ESS/crates/specify/ess-domain/src/command/finite.rs:143-146,166-171`); the
held-state half is the separate `input.selected.retain(…)` that keeps a guard only where its declared
state equals the case's state (`:180-184`). A bare state guard is therefore selected in exactly the
states it names, for every admitted input. `OutcomeCondition::SubjectState`'s `predicate` is an
`Option` precisely so that this shape can be written
(`ESS/crates/specify/ess-domain/src/command.rs:327-333`).

**No registration rule narrows the admitted set to reach this.** `AmbiguousDefaultOutcome` still
counts only branches declaring neither `when` nor `in_state` nor `wrong_state`, so a bare state guard
is not a second default and does not have to be last. The selector-free default stays a separate
construct with its own position rule, below.

* A `when` evaluating to `Truth::Unknown` returns `OutcomeUnobservable { operation, outcome,
  unresolved }`, naming every address that resolved to nothing. Selection stops there; no later branch
  is tried. Reading an unanswerable guard as *not this branch* would hand the command to a branch the
  author wrote for a different fact, which is the collapse the three-valued rules exist to prevent
  (`crates/entity-core/src/runtime.rs:572-582`, `crates/entity-core/src/definition.rs:428-444`), and it
  is the same rule ESS's own runner follows: an `Unknown` guard is `Decision::Unevaluable`, which is
  explicitly *not* "try another candidate" (`ESS/crates/verify/ess-conformance/src/decision.rs:37-54`).
* No branch taken — `NoOutcomeSelected { operation }`.

**State admissibility.** If the selected branch's effect is not `Moves`, the selected branch is the
result. If it is `Moves { from, .. }` and the instance's state is in `from`, the selected branch is the
result. Otherwise:

| the instance's state | answer |
| --- | --- |
| in `wrong_states(op)` — **no** move of this operation starts there | the `wrong_state` branch, if the operation declares one; otherwise `InvalidTransition { operation, state }`, which is what `kernel/1` returns today (`crates/entity-core/src/runtime.rs:371-383`) |
| not in `wrong_states(op)` — some **other** branch of this operation moves from there, but not the one the input selected | `UnspecifiedMoveSource { operation, outcome, state, from }` — § 4.4 |

**Why the wrong-state test is the union and not the selected branch's own `from`.** Because that is
what the source says, in three places that agree: the IR's own definition of the condition — *"taken
when the subject is resting in a state none of this command's moves start from"*
(`ESS/crates/specify/ess-compiler/src/ir.rs:539-544`); the computation
(`ESS/crates/specify/ess-compiler/src/ir.rs:1719-1743` over
`ESS/crates/specify/ess-domain/src/entity.rs:287-300`); and the design page in normative prose —
*"WrongState retains its complement-of-move-sources meaning"*
(`ESS/docs/design/subject-state-outcome-guards.md:38`). The previous draft of this document tested the
selected branch's `from` instead and refused, as `AmbiguousMoveSource`, every operation where the two
could differ. Witness W2 shows that refusal excludes a specification the installed tool admits, so it
is removed; § 4.4 is what replaces it.

**Two different state tests, and they are not in competition.** The `in_state` test above is *inside*
selection, at step 4, and applies only to a branch that declares `in_state` — ESS's `SubjectState`.
The state-admissibility test below is step 5, after a branch has been selected, and asks whether the
selected branch's **move** can start where the instance rests. `WrongStateWithStateGuard` (§ 2.1)
keeps the two constructs apart in one operation, so no branch meets both.

**Why the input guard is answered before the held state.** ESS's own reference implementation of
`examples/billing` evaluates the input guard first and the held state second: `pay_invoice` answers
`rejected` for a non-positive amount **before** it looks at the invoice's state, and answers
`wrong-state` only for an amount that passed
(`ESS/crates/verify/ess-conformance/src/reference.rs:624-643`), and says so in its own words —
*"The order matters and it is the specification's: `settled` is guarded by `when: amount.amount > 0`
and `rejected` is the branch with no guard, so a non-positive amount is refused whatever state the
invoice is in"* (`:617-623`). That target's module documentation states it is not privileged to cheat
the suite and that a wrong-state answer given for anything inconvenient would be the cheat (`:18-37`).
The rule above reproduces all four `examples/billing` commands and that target exactly.

**Why an `in_state` branch is never made unreachable.** `WrongStateWithStateGuard` refuses a
`wrong_state` branch in an operation that declares any `in_state` branch, which is ESS's own rule:
*"A command using explicit state guards cannot also declare WrongState: overlapping precedence is not
inferred"* (`ESS/docs/design/subject-state-outcome-guards.md:38-40`, enforced at
`ESS/crates/specify/ess-domain/src/command/subject_state.rs:31-37`). The two constructs never meet, so
the state-admissibility step cannot skip a state-guarded branch.

**Why the default is last.** ESS `Otherwise` is *"the default branch: taken when no conditional
outcome matched"*, at most one per command
(`ESS/crates/specify/ess-domain/src/command.rs:334-336`) — a **position-independent** meaning. ER
selects by declared order, so the only ER declaration order that spells that meaning is *last among the
non-`wrong_state` branches*. `AmbiguousDefaultOutcome` requires it, and the lowerer places it there.
This is a spelling rule for one ESS construct, not a reordering of an order ESS gave.

**What ESS proves about overlap, and what it does not.** In a subject-state command every (held state,
input) pair must select exactly one branch, proved by enumeration
(`ESS/crates/specify/ess-domain/src/command/subject_state.rs:109-204`). In a command with no default,
the same finite proof runs over the input alone
(`ESS/crates/specify/ess-domain/src/command.rs:1609-1632,1639-1656`). In a command **with** a default
and open input guards, ESS proves nothing about two `When` guards both holding. Declared order is
therefore this contract's tie-break for that residual case; it is the same rule `kernel/1` already
applies to transitions (`crates/entity-core/src/runtime.rs:371-379`), and it is listed in § 15 as a
choice rather than a reading.

### 4.4 The one case the source leaves unanswered, witnessed

**Witness W2.** The model `witnesses/witness-move/` in this section's evidence directory declares
`witness.move.Ticket` with states
`[Open, Held, Closed, Archived]` and transitions `hold: Open→Held`, `close_fast: Open→Closed`,
`close_slow: Held→Closed`, `archive: Closed→Archived`, and the command `witness.move.CloseTicket` with
three outcomes in this order:

| outcome | condition | effect |
| --- | --- | --- |
| `closed-fast` | `when: urgent` | `moves: Ticket.close_fast`, which starts from `Open` |
| `closed-slow` | none — the default | `moves: Ticket.close_slow`, which starts from `Held` |
| `wrong-state` | `wrong_state: true` | reports `TicketStateConflict` |

`ess specify validate` answers `witness v1 — 2 file(s), valid`, exit 0. So this is a specification the
installed tool admits, and the previous draft's `AmbiguousMoveSource` refused it.

Its unanswered pair is finite and exact:

> **A ticket resting in `Held`, with input `urgent: true`.**
> Input selection takes `closed-fast`: its guard is the only one that holds, and `closed-slow` is the
> default. `close_fast` starts only from `Open`.
> `move_sources(CloseTicket) = {Open} ∪ {Held} = {Open, Held}`, so
> `wrong_states(CloseTicket) = {Closed, Archived}` and `Held` is **not** one of them — the
> `wrong-state` branch is by ESS's own definition not taken.
> The selected branch cannot move. **No ESS rule says what happens.**

The mirror case is `Open` with `urgent: false`, which selects `closed-slow` and cannot take
`close_slow`.

The gap is reachable because the two validations that would close it do not apply here. ESS's
per-(state, input) proof that the selected branch can move — `validate_move(selected[0], case.state)`
at `ESS/crates/specify/ess-domain/src/command/subject_state.rs:201`, and its coarser fallback over
every state at `:150-159` — runs only for a command that uses `SubjectState`
(`uses()` at `:12-17`, applied at `:69`). `CloseTicket` uses `When` and `Otherwise`, so neither runs.
`validate_wrong_state_is_reachable` (`ESS/crates/specify/ess-domain/src/entity.rs:1042-1102`) asks only
whether `wrong_states` is non-empty, and `{Closed, Archived}` is not.

**What this contract does: refuse by name at run time, and admit every command the source admits.**
`UnspecifiedMoveSource { operation, outcome, state, from }` produces no decision, no record, no
revision, no event and no response — the same nothing a refusal produces (§ 5.1) — and names the four
facts a reader needs to repair the specification. It is a statement about the *specification*, which is
why it is not a branch result and not a `Refusal`: no branch of the model claimed this case.

Three alternatives, each rejected for a stated reason:

| alternative | why not |
| --- | --- |
| take the `wrong_state` branch anyway | contradicts the condition's own definition: `Held` **is** a state a move of this command starts from (`ESS/crates/specify/ess-compiler/src/ir.rs:539-544`) |
| fall through to the next applicable branch | invents a precedence the source does not have, and the source proves selection is unique wherever it can prove anything (`.../subject_state.rs:180-200`, `.../command.rs:1671-1700`) |
| refuse at registration, as `AmbiguousMoveSource` did | refuses `CloseTicket`, which the installed tool admits — witness W2. Deciding which (state, input) pairs are reachable needs a finite guard analyser `entity-core` does not have (§ 15.1) |

§ 15.2 records this as a choice with the consequence of reversing it, and the handoff carries the
witness for root.

#### The rest of the toolchain, checked

The gap above was established from the two **validations**. The scenario synthesizer and the
conformance runner are the other two places an answer could live, and neither produces one. Read this
session:

| path | what it does with (state, input) | does it answer the pair? |
| --- | --- | --- |
| `ESS/crates/verify/ess-conformance/src/synthesize.rs:1483-1497` (`prepare`) | for a `Moves` branch, the states it will arrange the subject in are `transition.from` — `closed-fast` is arranged in `Open` and in nothing else | no: `Held` is never arranged for `closed-fast` |
| `ESS/…/synthesize.rs:1962-1969` (`prepare_state_input`) | walks candidate states and `continue`s past any state the selected branch's transition does not start from | no: it **skips** exactly this pair rather than deciding it |
| `ESS/…/synthesize.rs:2994-3000` (the illegal-move family) | iterates the states of `ir.wrong_states(command)` only | no: `wrong_states(CloseTicket) = {Closed, Archived}` and `Held` is not one |
| `ESS/crates/verify/ess-conformance/src/decision.rs:22-55` | decides **one guard against one candidate input** — `Satisfied` / `Refuted` / `Unevaluable` | no: it has no held state and no move at all |
| `ESS/crates/verify/ess-conformance/src/runner.rs` | executes the scenario steps a suite already contains | no: it selects no outcome of its own; the only `evaluate` it runs is a projection selection (`:1892-1898`) |
| `ESS/crates/verify/ess-conformance/src/reference.rs:560-690` | the hand-written `examples/billing` target, command by command | no: it is one example's implementation, not a rule |

So the gap is **finite and complete**: for the pair (`Held`, `urgent: true`) the source neither
validates it, nor synthesizes a scenario for it, nor runs it, nor answers it in a reference target.
`UnspecifiedMoveSource` is retained. This is not a survey: it is the six paths that could have
answered it, each read and each recorded above.

## 5. Named outcomes, refusals, responses and the caller's place

The kernel selects the branch. The caller supplies **evidence**, never a branch name — a definition
whose guard reads an argument that is an outcome name is not detectable by the kernel and is a lowering
refusal, § 10.

### 5.1 Entry points

`execute` and `create` keep their signatures and their `Result<Decision, CoreError>` return, so every
existing caller compiles and behaves as it does now. A refusing branch reaches them as a typed
`CoreError::Refused { outcome, error, message }`. New callers use the branch-aware entry points, which
share one implementation with the old two:

```rust
pub enum Evaluation { Accepted(Decision), Refused(Refusal) }
pub struct Refusal { pub outcome: String, pub error: String, pub message: Option<String> }

pub fn decide_create(d: &ValidatedDefinition, id: String, input: Value) -> Result<Evaluation, CoreError>;
pub fn decide(d: &ValidatedDefinition, i: &EntityInstance, op: &str, args: Value) -> Result<Evaluation, CoreError>;
// and Runtime::decide_create / Runtime::decide over the registry (crates/entity-core/src/runtime.rs:198-245).
```

A refusal produces **no** `DecisionRecord`, no revision, no state, no events and no response, so nothing
about it reaches a store. That is not a simplification: ESS refuses a specification whose refusing
outcome emits or declares a subject
(`ESS/crates/specify/ess-domain/src/command.rs:1268-1312`), so an ESS refusal has nothing durable to
record. `Refusal` carries the error's **name**; ESS determines no error field values
(`ESS/crates/specify/ess-compiler/src/ir.rs:702-704`), so the error's payload is a binding obligation
(§ 9), not a kernel invention.

### 5.2 Observability, creation effects, and the one place ER is weaker than ESS

`OutcomeEffect::Creates` is a first-class variant, and a creation branch must declare `Creates` or
`None`. `DecisionEffect::Created` (§ 6) is what it produces. Without the variant a creation branch
would carry `effect: None` and the record could not say a creation happened.

ER's `UnobservableOutcome` fires only when a branch declares **no** `emits`, **no** `set`, **no**
`responds`, **no** `refuses` and `effect == None`. An accepting `wrong_state` branch is exempt, which is
ESS's own exemption (`ESS/crates/specify/ess-domain/src/command.rs:1246-1253`).

This is **weaker** than ESS's `EmptyChange`, which refuses any outcome with no event and no error
(`:1253-1266`), and the weakening is required rather than convenient: ER `kernel/1` admits a creation
that emits nothing at all (`CreateDefinition.emit` is an `Option`,
`crates/entity-core/src/definition.rs:336-341`), and § 1 promises `kernel/1` behaviour does not move.
So **a valid zero-event creation stays valid**, and a `service/1` creation branch with
`effect: Creates` and no events is admitted for the same reason. ESS's stricter rule is not imported
into the kernel; it lives where it already is, on the ESS side of the lowering, and the lowerer
therefore never produces such a branch from a valid specification.

An **accepted** branch that changes nothing — ESS's `refuses: false` wrong-state branch
(`ESS/crates/specify/ess-compiler/src/ir.rs:705-712`) — is an ordinary decision with `effect: none`,
zero events, no changed fields and revision + 1. Zero-event decisions are already first-class
(`docs/design/recorded-execution-v0.1.md:64-67`), so this needs no new mechanism.

### 5.3 The declared command response

`ResolvedCommand.response` is a closed list of declared fields
(`ESS/crates/specify/ess-compiler/src/ir.rs:810-812`), and `ResolvedPayloadValue::ResponseField` reads
one of them into an event payload or a `sets` entry (`:742-749`). Both halves need a home.

* **The shape** is `OperationDefinition.response` / `CreateDefinition.response`, an `ObjectSchema` —
  the same closed, per-field-kind type the arguments and the instance already use.
* **The value** is `OutcomeDefinition.responds`, a template map resolved at **step 14** of § 4.2 and
  validated against `response`. The scope is `OperationTemplate` for an operation branch and
  `CreateOutcomeTemplate` for a creation branch (§ 2.2), so a creation response may read a creation
  argument no field stores — the same reason the creation event payload may, § 2.3. Every accepting
  branch determines every required response field, or `OutcomeResponseIncomplete` refuses the
  definition.
* **A refusing branch determines none.** `RefusalMutatesState` covers `responds`, for the same reason
  it covers `emits`.
* **The record** carries it: `DecisionRecord.response: Option<Map<String, Value>>`, so replay
  recomputes and byte-compares it like every other product of the decision.
* **`Decision` gains no key.** A caller reads `decision.record.response`.

A response field the *implementation* determines rather than the model — `ResolvedPayloadValue::Generated`
and a `ResponseField` whose value no branch computes — is host-determined data and enters as
`$args.bound.<field>` under the `ResponseFieldSupplied` obligation of § 9. The kernel never invents one.

### 5.4 What a creation event's `args` says

`materialize_event` records, as a creation event's `args`, whatever the decision was decided on
(`crates/entity-core/src/runtime.rs:302-310,855-860`). Under `kernel/1` a creation has no arguments, so
that is the fields; `rehydrate` relies on it and refuses a creation event whose `changed` and `args`
differ (`crates/entity-core/src/replay.rs:351-363`). Under `service/1` a creation **does** have
arguments, and they are what `args` records.

That is a change of meaning for an existing key, so it is not made silently:

* the record framing moves to `er.record/2` (§ 1.2);
* `rehydrate` refuses a `service/1` definition **by name**, before it reads any event
  (§ 12), so the `changed == args` check never sees a `service/1` creation event and never issues a
  diagnostic about a shape it was not written for;
* § 11 pins both.

## 6. Update without a fictitious transition, and what the record says

`OutcomeEffect::Updates` produces a decision whose `to_state` equals its `from_state` and which
declares no transition. No self-transition is synthesized, at registration or at run time. The
distinction is written into the record rather than inferred from `from_state == to_state`, because a
definition may legitimately declare a self-transition:

```rust
pub struct DecisionRecord {
    // existing: definition, command, entity, id, revision, from_state, to_state, result, changed, events
    pub outcome: Option<String>,             // the branch the kernel selected, None for kernel/1
    pub effect: Option<DecisionEffect>,      // Created | Moved | Updated | Unchanged, None for kernel/1
    pub response: Option<Map<String, Value>>,// § 5.3, None for kernel/1
}

pub enum DecisionCommand {
    Create {
        fields: Map<String, Value>,          // existing meaning: the normalized resulting fields
        arguments: Map<String, Value>,       // new: the creation command's input; empty for kernel/1
    },
    Execute { operation: String, arguments: Map<String, Value> },   // unchanged
    LegacyImport,                                                    // unchanged
}
```

`outcome`, `effect` and `response` are `#[serde(default, skip_serializing_if = "Option::is_none")]`;
`Create.arguments` is `#[serde(default, skip_serializing_if = "Map::is_empty")]`. A `kernel/1` record is
therefore byte identical to today's.

`fields` keeps its exact meaning — the normalized resulting creation fields — so `replay`,
`verify` and `encoding` read what they have always read. `arguments` is what re-selects the branch on
replay. The two are redundant by construction and cannot drift: `replay` recomputes the whole decision
and byte-compares the record (`crates/entity-core/src/replay.rs:161-166`), so a record whose `fields`
are not what its `arguments` produce is refused.

`DecisionCommand` gains **no variant**, so no match site loses a case. It gains one field on one
variant, which makes the five `Create { fields }` patterns
(`crates/entity-core/src/replay.rs:134`, `crates/entity-store/src/asynchronous/verify.rs:91,108`,
`crates/entity-store/src/asynchronous/encoding.rs:99`, `crates/entity-executor/src/lib.rs:616`) a
**compile error** until each is updated — loud, enumerable, and exactly the five sites § 12 lists.

## 7. Identity: a logical typed value, and the address it is stored at

### 7.1 What ESS actually admits, measured

**Witness W1.** `EntitySpec.identity` is a `Field` — a name and a `TypeRef`
(`ESS/crates/specify/ess-domain/src/entity.rs:730-741`) — and the only check `EntitySpec::validate`
makes of that type is that the registry can resolve it
(`ESS/crates/specify/ess-domain/src/entity.rs:823-835`). There is no restriction on its kind, and the
installed tool confirms it. `witnesses/witness-identity/` declares thirteen entities, one per candidate
identity type, and `witnesses/witness-identity-composite/` three more:

| identity type | `ess specify validate` |
| --- | --- |
| `Integer`, `Decimal`, `Boolean`, `Bytes`, `Timestamp`, `Duration` | admitted, `ess/1` |
| a newtype over `Integer`, over `Bytes` | admitted, `ess/1` |
| an `enum`, `Optional<String>`, `List<String>` | admitted, `ess/1` |
| a `struct`, a tagged `union`, `Map<String, String>` | admitted, `ess/1` |
| `Binary64`, and a newtype over it | admitted under `ess/2`; under `ess/1` refused by name — `[unsupported_format_version] entity witness.ident.Binary64Keyed.identity.type: Binary64 requires specification format ess/2` |

So the previous draft's rule — *"Admitted identity types are the text-shaped ones … A numeric or `Bytes`
identity is a lowering refusal"* — closed twelve of the thirteen rows the source admits, on the stated
ground that *"old ER uses string IDs"*. That is removed.

### 7.2 The two things, kept apart

ER holds **one** identity notion today: `EntityInstance.id`, a caller-supplied opaque non-empty
`String` outside the field map (`crates/entity-core/src/runtime.rs:34-36,264-269`), and a `ref` field's
value, a non-empty non-whitespace string and nothing more
(`crates/entity-core/src/validation.rs:955-965`). This contract keeps that type unchanged and adds the
*second* notion beside it:

* **The logical identity** is an ordinary declared field of the schema, in its own kind:
  `{invoice_id: {type: string}}`, `{seq: {type: integer}}`, `{pair: {type: object, properties: …}}`.
  It is what a guard reads, what a view projects, what a relation carrier is typed as (§ 8), and what
  ESS's `instance:` link is type-checked against
  (`ESS/crates/specify/ess-domain/src/entity.rs:1167-1172`).
* **The storage address** is `EntityInstance.id` and a `ref` field's value: the text an ER store keys
  by. `identity: { field: <name> }` declares which schema field the address is derived from.

### 7.3 The address function: total, per kind, and collision-free

`address(v)` is a **total** function from a logical identity value to the address text, defined by the
declared **kind** of the identity field. Every kind § 7.4 admits has a row; `json` is the one kind that
is refused, and it is refused at registration rather than left without a spelling.

| kind of the identity field | `address(v)` |
| --- | --- |
| `string`, `enum`, `ref` | `"s:"` followed by the string's own contents, unquoted and unescaped — § 7.3.1 |
| `integer`, `number`, `binary64` | `observed(v).exact_text()` — § 7.3.2. One function, not three |
| `boolean` | `false` or `true` |
| `array`, `object`, `map`, `union` | its canonical JSON text — § 7.3.3 |

#### 7.3.1 Text-like identities carry an `s:` prefix

A `service/1` storage address may encode logical text, and this row is where it does. `address("")`
is `"s:"`, `address(" ")` is `"s: "`, and `address("s:x")` is `"s:s:x"`. The rule is total and
injective on the whole `String` domain, including the empty and whitespace values ESS admits —
`Primitive::String` carries no pattern (`ESS/crates/generate/ess-gen/src/types.rs:490`).

What this costs and what it does not:

* **No `kernel/1` id, `ref` value or record byte moves.** A `kernel/1` definition cannot declare
  `identity` at all — `SemanticsKeyNotAvailable`, § 2.1 — so there is no `kernel/1` instance whose
  address this function computes. Every committed fixture keeps the bytes it has.
* **A `service/1` text address is a new address and is never advertised as an unchanged legacy byte
  string.** It lives inside the `er.record/2` framing of § 1.2, which is a new version domain for a
  new record shape; nothing rewrites an `er.record/1` document. § 11 pins that the `/1` fixtures are
  byte-identical and that a `service/1` text identity's `id` is `s:`-prefixed, as two separate tests.
* **The public surfaces keep the declared value.** The SDK's logical identifier, an event payload
  field, and a relation carrier field (§ 8.1) all carry the identity's **declared logical value** —
  `"INV-1"`, not `"s:INV-1"`. `$id` remains the storage coordinate and nothing else; a published
  logical identity comes from the typed identity field or from the bound logical value, never from
  the address. A binding derives the address from the logical value through this same pure function,
  exported as `entity_core::identity::address`, rather than spelling the prefix itself.
* **The SDK consequence is an opt-in `/4` binding.** `service-definition/3` and `service-runtime-ir/3`
  are the current formats and version 0.4 "accepts only the new `/3` definition and runtime formats
  and intentionally provides no compatibility reader for prior generated artifacts"
  (`SDK/README.md:184-186,222`). A `/3` binding has no address function and therefore cannot address a
  `service/1` entity; carrying `IdentitySupplied` (§ 9) with the address derivation is a `/4` change
  on the SDK side, opt-in and separately owned (§ 12's exclusions).
* **No accepted preservation constraint contradicts this.** § 1.1's promise is over `kernel/1`
  definitions, snapshots and fixtures, and it is kept. There is no stored `service/1` record to
  preserve, because `service/1` does not exist yet. This completes the `String` identities § 7.1 already
  admitted; it adds no kind.

#### 7.3.2 The three numeric rows are one function

`observed(v)` is § 10.2.1's `source-number/1` observation of the stored token, and `exact_text()` is its
canonical spelling: for a value carried exactly, `units × 10⁻ˢᶜᵃˡᵉ` written with a sign, no exponent,
no trailing fractional zero and one spelling per value; for a value no `(i128, u8)` spells, the
shortest decimal that round-trips its binary64. That is `Number::exact_text` reproduced —
`ESS/crates/specify/ess-primitives/src/facts.rs:204-213`, whose two arms are `exact_text(units, scale)`
(`:382-396`) and `format!("{value}")`. The method exists in the source and is public; it is not an
assumed API.

Per kind, what that spells:

* `integer` — a canonical decimal integer, `-?(0|[1-9][0-9]*)`. `1`, `1.0` and `1e0` are one *value*
  and so one address, which is what ESS answers (`ESS/…/facts.rs:486-491`).
* `number` — the exact decimal of the observed value.
* `binary64` — **the missing row, supplied.** Its admitted domain is a **finite** binary64 and nothing
  else: `NaN` and the infinities are refused by ESS's own constructor
  (`ESS/crates/specify/ess-primitives/src/facts.rs:162-169`) and by the `binary64` field kind
  (§ 10.1), so there is no non-finite value for `address` to be undefined on.
  **Signed zero**: `-0.0` and `0.0` observe as one value — `canonical_decimal(-0.0)` is `(0, 0)`, the
  same pair `0.0` gives (`ESS/…/facts.rs:313-320`) — so `address` spells both `0` and the two are one
  instance, which is what ESS's *"`-0.0` and `0.0` are one value"* requires
  (`ESS/…/facts.rs:481-485`). The field's own bytes keep the sign; § 10.2.1 says where.

**Canonical identity equality agrees with the declared logical identity domain**, and that is the
property the mirror step depends on: for two values `a`, `b` of one numeric kind,

```text
observed(a).cmp(&observed(b)) == Equal   ⟺   address(a) == address(b)
```

with the three cases checked rather than asserted. Two exactly-carried values compare by their
normalised `(units, scale)` and `exact_text` is injective on that pair, so the two agree. Two
binary64-carried values compare by `total_cmp` and spell their shortest round-tripping decimal, which
is injective on the `f64` values that reach that arm — neither is `NaN`, and neither is a zero, because
`canonical_decimal` has an answer for every zero. A carried-exactly value and a binary64-carried one
never compare equal, because the representation is a function of the carried binary64
(`ESS/…/facts.rs:256-279`), and they never spell the same text either: every `exact_text` output is a
decimal with no exponent, at most 255 places and digits an `i128` holds, which is exactly the grammar
`exact_of_decimal_text` accepts (`ESS/…/facts.rs:327-368`) — so a binary64-carried value whose
`Display` produced such a text would have had an exact carrier and would not be on that arm.

#### 7.3.3 Composite recursion, exactly

`array`, `object`, `map` and `union` address as canonical JSON, at **every** depth:

* every object's and map's keys in sorted order — the order ER already canonicalizes an object into
  (`crates/entity-core/src/runtime.rs:995-1008`);
* every array's elements in index order;
* a `union` as the adjacent-tagged object of § 10.1, which is an object and so takes the object rule;
* every **string** leaf as a JSON string with the standard escaping, which is where the `s:` prefix
  does **not** appear: the prefix is the top-level text rule of § 7.3.1, and a composite's injectivity
  comes from JSON quoting instead;
* every **numeric** leaf — `integer`, `number` **and `binary64`** — written by § 7.3.2's
  `observed(v).exact_text()`, so a composite identity containing a `binary64` member has a spelling
  and the signed-zero collapse applies inside it as well;
* every **boolean** leaf as `false` or `true`;
* a `json` leaf cannot occur, because `json` is refused as an identity kind at every depth by
  `IdentityFieldNotAddressable`.

An object identity with `additional_properties: true` admits undeclared JSON leaves, so the same
refusal applies at any depth. Closed objects and typed maps remain admitted; nonidentity fields
and `kernel/1` keep their existing open-object behavior.

**Collision freedom.** Within one entity type the identity field has one declared kind, so exactly one
row applies, and each row is injective on its own value domain: `s:` prepended to an injection is an
injection; the numeric rows are § 7.3.2's checked equivalence; `boolean` has two values and two
addresses; canonical JSON is injective because JSON string escaping is and every other leaf is covered
by the rows above. Two *rows* can collide with each other — a `boolean` identity `true` and a `string`
identity `"true"` would both have spelled `true` before the prefix, and the prefix now separates even
those — and in any case they cannot meet, because the kind is fixed per entity type.

**Replay.** Nothing new is stored. The record already carries `id` and the creation's `fields`, so
replay recomputes `address(fields[identity.field])` and byte-compares it against `id` like every other
product of the decision (`crates/entity-core/src/replay.rs:161-166`). Because the address is derived
from the observation rule, `number_observation` (§ 10.2.1) is part of the definition snapshot and replay
reads the snapshot, so a later reading rule cannot move an already recorded address.

### 7.4 The mirror step, and its one refusal

At creation and after every branch's `set` — **step 11 of § 4.2**:

```text
address(fields[identity.field]) == id    or    IdentityMismatch { field, id, value }
```

**The address is never empty or whitespace, so there is no second refusal.** Each row produces a
non-empty, non-whitespace string: `"s:"` and everything after it for the text-like kinds, a decimal
digit string for the numeric kinds, `false`/`true` for `boolean`, and a JSON document opening with
`[` or `{` for the composites. The previous draft's `IdentityAddressEmpty { field }` refused an empty
`String` identity, which ESS admits; the `s:` rule of § 7.3.1 admits it too, so that refusal is
**removed** rather than kept as an unreachable check. What `EntityInstance::id` and a `ref` value
refuse (`crates/entity-core/src/runtime.rs:264-269`,
`crates/entity-core/src/validation.rs:958-962`) is still refused, and no address can trip it. § 11 pins
that an empty-string identity is admitted and addresses to `s:`.

`IdentityFieldNotAddressable` refuses an identity field of kind `json`, whose value the address
function cannot be injective over because `json` types nothing. Every other kind is admitted.

**`Optional<T>` identity.** ESS admits it (witness W1) and ESS's own conformance refuses to instantiate
an entity whose identity is null (`ESS/crates/verify/ess-conformance/src/input.rs:153-155`). So it
lowers to a **required** ER field of `T`'s kind rather than to a refusal: the source's type says the
value may be absent and the source's runtime says an instance's may not, and `required: true` is the
spelling of the second. `IdentityFieldUnknown` covers the case where the lowerer emits an optional one.

### 7.5 Where the identity comes from

ESS `creates:` observes the new identity in a field of an emitted event
(`ESS/crates/specify/ess-compiler/src/ir.rs:604-619`) because the implementation assigns it. The kernel
has no identifier generator (AGENTS.md invariant 7), so the identity is the host's: the binding plan
carries `IdentitySupplied`, the host passes the **address** as `id` and the **logical value** as a
creation argument, the branch's `set` writes it into the identity field, and the creation event's
payload maps that event field to `$fields.<identity.field>` — the **logical** value, which is what a
later scenario, a view and a relation carrier all read. **Step 11** then checks the round trip rather
than assuming it.

The host does not spell the address itself. `entity_core::identity::address` is the one pure function
of § 7.3 and the binding calls it, so the `s:` prefix, the numeric canonical text and the composite
recursion have exactly one implementation and a binding cannot drift from the kernel's mirror check.
Deriving an address is the only thing a binding does with it: the address is `$id`, the storage
coordinate, and it is never published as the entity's identifier.

The admission grammar of a text identity — `Uuid` in canonical hyphenated form, `Bytes` as padded
base64 (`ESS/docs/design/review-primitive-semantics.md:39-40`) — is the `TimestampSpelling`-family
obligation of § 9 and is not a kernel check. Two consequences, both stated: a `Uuid` identity cannot
collide through two spellings, because only one spelling is admitted; a `Timestamp` identity **can**,
because `Primitive::Timestamp` publishes a `format` and no pattern
(`ESS/crates/generate/ess-gen/src/types.rs:513`), so `…T00:00:00Z` and `…T00:00:00.000Z` are two
addresses for one instant — and they are also two *values* to ESS, which compares a `Timestamp` as text
(§ 10.3). ER and ESS agree; neither is right about instants, and that is the source's to fix.

## 8. Relations

`RelationDefinition` preserves everything ESS resolves — name, `Owns`/`References`, target, `One`/`Many`
and `via` (`ESS/crates/specify/ess-compiler/src/ir.rs:403-416`) — as a declaration beside the existing
typed `ref` field (`crates/entity-core/src/definition.rs:173-204`), which keeps its meaning unchanged.

### 8.1 Which definition carries the field, and what shape it has

ESS states this in one table and it is reproduced here without reinterpretation: the carrier is on the
**target** for `owns` and on the **source** for `references`
(`ESS/crates/specify/ess-compiler/src/ir.rs:414-415`,
`ESS/crates/specify/ess-domain/src/entity.rs:1243-1248`), and the admitted carrier type is
`carried_types` (`ESS/crates/specify/ess-domain/src/entity.rs:1392-1407`).

| `kind` | `cardinality` | ESS carrier type | Carrying definition | ER `via` field |
| --- | --- | --- | --- | --- |
| `Owns` | `One` **or** `Many` | the **source's** identity type, unwrapped — **no `Optional` offered** | the **target** | the source's identity kind (§ 7.2), **`required: true`** |
| `References` | `One` | the target's identity type, **or `Optional<it>`** | **this** definition | the target's identity kind, `required: false` when the source wraps it in `Optional`, `required: true` when it does not |
| `References` | `Many` | `List<target identity type>` — **no `Optional` offered** | **this** definition | `{type: array, items: <the target's identity kind>}`, **`required: true`** |

**Optionality is carried across, not flattened.** `carried_types` returns a two-element list for the
`References`/`One` row — `target.identity.type_ref` and `TypeRef::Optional(target.identity.type_ref)` —
and a one-element list for the other two
(`ESS/crates/specify/ess-domain/src/entity.rs:1392-1407`). The previous draft wrote `required` "either
way" for that row, which refused an entity whose reference is not yet set: a source-admitted shape,
turned away by the ER schema and by `RelationViaWrongShape`. The three rows now lower one for one:

* the lowerer reads the declared carrier field's type and emits `required: false` for the `Optional`
  spelling and `required: true` for the bare one;
* `RelationViaWrongShape` checks the carrier's **kind** and admits both optionalities on the
  `References`/`One` row (§ 2.1);
* `RelationCarrierOptionality` refuses `required: false` on the other two rows, because the source
  offers no `Optional` there — so `Owns` and `References`/`Many` keep their own distinct shapes rather
  than all three being widened together;
* an absent optional carrier is *nothing observed*, not a null: the key is absent and a guard reading
  it answers `Unknown` (§ 10.2, *Absence versus null*). `RelationExistence` (§ 9) is asked about a
  carrier that is present, and about no other.

The `Owns` row is the one an earlier draft of this document had wrong, and the example says why in
its own words: `via:` names the field on the target "because that is where an owner's identity lives on
the thing it owns", and `cardinality:` "says how many invoices one account has, and says nothing about
that field, which is one account whether the account has one invoice or a thousand"
(`ESS/examples/billing/domains/invoice.yaml:79-88`). A `Many` `Owns` therefore carries the owner's
identity **once**: `carried_types` answers `vec![source.identity.type_ref.clone()]` for
`(RelationKind::Owns, _)` — one arm for both cardinalities, with no `List` wrapper on either
(`ESS/crates/specify/ess-domain/src/entity.rs:1392,1398`).

**That sentence is about the cardinality, not about the identity's own type, and the two must not be
confused.** An earlier draft of this paragraph read *"a `Many` `Owns` carries a scalar, never an
array"*, which is true of the cardinality and false as a claim about the carrier's kind: § 7.1's
witness W1 admits a `List<String>` identity and § 7.3.3 gives it an address, and for such an owner
`carried_types` returns that same `List<String>` — so the carrier **is** an array, once. Reading the
phrase as *the carrier is never an array* would contradict this table's own identity-kind rule and
refuse a specification the installed tool admits. What `Many` does not do is wrap it: an owner
identified by `List<String>` is carried by one `List<String>` and never by a `List<List<String>>`.

The same distinction runs through the other two rows, because `TypeRef::List(Box<TypeRef>)` nests
(`ESS/crates/specify/ess-domain/src/types.rs:126-137`) and the carrier is compared against the whole
`TypeRef` — `accepted.contains(&field.type_ref)`
(`ESS/crates/specify/ess-domain/src/entity.rs:1267-1268`). Written out for a target identified by
`List<String>`:

| row | ESS carrier type | ER `via` field |
| --- | --- | --- |
| `Owns` / `One` or `Many` | `List<String>` | `{type: array, items: {type: string}}`, `required: true` |
| `References` / `One` | `List<String>` or `Optional<List<String>>` | `{type: array, items: {type: string}}`, `required: true` or `false` |
| `References` / `Many` | `List<List<String>>` | `{type: array, items: {type: array, items: {type: string}}}`, `required: true` |

So a carrier's kind is compared **through an array's element kind at every level**, which is what
makes the `References`/`Many` row's outer array the cardinality and its element the identity. It is
not compared through an `object`'s properties, a `map`'s key and value or an `enum`'s values: this
table says *kind*, and those would be a structural comparison it does not state.

The **kind** column is the second correction. The previous draft wrote `{type: ref, entity: …}` in every
row, which is right only when the related entity's identity is text (witness W1 shows twelve other
cases). The carrier's ER kind is the **related entity's identity field's kind**, which is what makes the
ER carrier type-check the same claim `carried_types` type-checks: `billing.invoice.Invoice.account_id`
is typed `billing.invoice.AccountId`, a newtype over `Uuid`, so its ER kind is `string` and `ref` is the
available legacy spelling for non-empty text (`ESS/examples/billing/domains/invoice.yaml:20-22,108-109`).
The new lowerer uses the target identity's declared value kind, however: `string` for `Uuid` or
`String`, and `integer` for an integer identity. It must not replace an arbitrary logical text
carrier with `ref`: `ref` requires non-empty text and would reject the empty `String` identity § 7
admits. `FieldKind::Ref` keeps its existing meaning and validation; the new relation descriptor
carries the target and binding obligation without narrowing the carrier's logical value domain.

### 8.2 Which layer enforces which claim

| Claim | Enforced by | Scope |
| --- | --- | --- |
| a `References` `via` exists on this definition, is a list on the `Many` row, and carries the table's optionality | `EntityDefinition::validate` | one definition |
| **every carrier has the table's kind** — the target's identity kind on both `References` rows, the source's on the `Owns` row, compared through an array's element kind at every level; an `Owns` `via` exists on the **target** and is `required: true`; the target is registered; no entity has two owners; no field carries two relations | `Registry::validate_all` | one registry |
| the referenced instance exists; `Many` membership; owner-delete behaviour | **binding obligation** | outside the kernel |

**Why the kind is the registry's on every row, including the two a definition declares itself.** The
table types each carrier by the *related* entity's identity field, and one definition does not hold
the other. A single-document check can see that a `References`/`Many` carrier is a list and that it
is `required`, and nothing more; asked about the kind it can only guess, and the guess an earlier
draft made — *refuse every array carrier on the `References`/`One` row* — refused precisely the
source-admitted case where the target's identity is itself a `List<T>`. A check that cannot know
must not answer, so it delegates and the registry compares.

A carrier field holds the related entity's **logical identity value**, in its declared kind. The
binding that resolves a relation derives that instance's storage address with
`entity_core::identity::address` (§ 7.3) before it looks anything up; the carrier itself is never the
address. That is one more reason the third row is a binding obligation: resolving a relation means
addressing a second instance, and the kernel is handed one.

The third row is not promised in the kernel and not quietly dropped: it is a named obligation the
binding plan must carry (§ 9), because the kernel is handed one instance and cannot see a graph
(`crates/entity-core/src/definition.rs:196-204`, R-01). There is no global graph IO anywhere in
`entity-core`, and none is added.

`Registry::validate_all` is the right home for the `Owns` checks because for an `owns` relation the
carrying document is not the one the author is reading — which is the same reason ESS puts them in
`validate_relations` over the whole entity map rather than on `EntitySpec`
(`ESS/crates/specify/ess-domain/src/entity.rs:1213-1274`), and why the second claimant of a field is
reported in name order there (`:1215-1219,1254-1265`).

**What one registry holds** is exactly the entity closure of one selected ESS component: its owned
domains' entities, plus every relation target, plus the **source** of every `Owns` relation whose target
is already in the set (`ESS/crates/specify/ess-service-contract/src/lib.rs:586-622`). That closure is
what makes the single-owner rule decidable, and it is determined by the caller's component argument —
§ 13.

## 9. Everything the world knows enters as named binding data

No clock, no generator, no lookup, no response value, no external cause reaches the kernel. A
`service/1` operation's argument schema is exactly two declared object fields, which is collision-free
without a reserved prefix and keeps ESS's own two surfaces distinguishable:

* `input` — the ESS command input, field for field (`ResolvedCommand.input`,
  `ESS/crates/specify/ess-compiler/src/ir.rs:808-809`). A guard reads `$args.input.<path>`.
* `bound` — everything the host determined: `ResolvedPayloadValue::ResponseField` and `Generated`
  (`ESS/crates/specify/ess-compiler/src/ir.rs:742-770`), the evidence behind a
  `ResolvedCondition::External` cause (`:534-538`), any clock reading, and the identity of § 7.

An `External` outcome therefore lowers to an ordinary guard over a declared `bound` field — the host
reports *what happened*, the kernel still selects the branch. The binding plan obligations this
contract requires, as a closed list:

| Obligation | What the host must supply |
| --- | --- |
| `IdentitySupplied` | the `id` of a creation, its logical value as a creation argument, and the creation event field that publishes it (§ 7.5) |
| `ResponseFieldSupplied` | a declared response field no branch determines |
| `GeneratedFieldSupplied` | a `Generated` payload or `sets` field |
| `ExternalEvidenceSupplied` | the `bound` field standing for an `External` cause |
| `TimestampSpelling` | the wire spelling of every `Timestamp` and `Duration` field — § 10.3 |
| `UuidSpelling` / `BytesSpelling` | the canonical hyphenated UUID and the padded base64 ESS admits, and only those (`ESS/docs/design/review-primitive-semantics.md:39-40,257-271`) |
| `DecimalSpelling` | the exact decimal text of every `Decimal` field — § 10.2.1 |
| `Binary64Spelling` | that every `binary64` value is a token binary64 carries — § 10.2.1 |
| `ScaleDeclaration` | the ordered scales the protocol supplies for text comparison, frozen into `EntityDefinition.scales` — § 10.3 |
| `RelationExistence` / `RelationCardinality` / `RelationOwnership` | § 8's third row |
| `ErrorPayload` | the field values of a declared error |
| `RevisionExpectation` | which revision a command expects |

`RevisionExpectation` stays a binding decision because ER owns revision 1 at creation and + 1 per
successful operation while ESS declares no runtime counter
(`crates/entity-core/src/runtime.rs:39-41,411-419`).

## 10. Exact values, predicates and invariants

### 10.1 Three new closed field kinds

`FieldKind` (`crates/entity-core/src/definition.rs:264-294`) gains `Map`, `Union` and `Binary64`. All
three are closed and finite; none is a property bag, and none admits a value the schema has not typed.

```rust
pub enum FieldKind {
    String, Integer, Number, Boolean, Enum, Array, Object, Json, Ref,
    Map, Union, Binary64,
}
```

**`map`.** The value is a JSON object. `items` — already "the element definition", and reused because
both kinds answer *what is inside* — is the **value** schema and is required. A new
`key: Option<MapKey>` names the key's spelling and is required on a `map` and refused elsewhere.

```rust
pub enum MapKey { String, Boolean, Integer, Decimal, Timestamp, Duration, Uuid, Bytes }
```

Keys are checked by their spelling, because a JSON object's keys are always strings: `Boolean` admits
exactly `"false"` and `"true"`, `Integer` admits decimal integer text, `String` checks nothing, and the
rest are checked as the matching text primitive. This is ESS's own key projection, variant for variant
(`ESS/crates/generate/ess-gen/src/types.rs:527-548`). A key kind that is not on this list is
`MapKeyNotText`.

`ESS Map<K,V>` lowers to `{type: map, key: <K>, items: <V>}`. Nothing is dropped: `V` is a full
`FieldDefinition`, so a `Map<String, Money>` types its values as an object with `amount` and
`currency`.

**`union`.** ESS unions are tagged, always, and ESS's own wire layout is **adjacent** tagging: the
variant label under the tag key, the payload under `value`, or under `content` when the tag is itself
called `value` (`ESS/crates/generate/ess-gen/src/types.rs:83-88,181-188,696-733`). ER reproduces that
layout exactly:

```rust
pub struct FieldDefinition {
    // ...
    pub key: Option<MapKey>,                          // `map` only, required there
    pub tag: Option<String>,                          // `union` only, required there
    pub variants: BTreeMap<String, FieldDefinition>,  // `union` only, required non-empty there
}
```

The content key is **derived**, not declared: `"value"`, or `"content"` when `tag == "value"`. Deriving
it is what keeps ER and ESS from disagreeing about it. A value of a `union` field is an object with
exactly the tag key and — unless the variant's payload is optional — the content key; the tag's value
names a declared variant, and the content satisfies that variant's definition. Anything else is a
validation error naming the tag it found. `UnionTagCollides` refuses a `tag` equal to the derived
content key of a nested variant object; `UnionVariantMissing` refuses an empty `variants`.

**`binary64`.** A JSON number, which is ESS's own wire node for the primitive
(`ESS/crates/generate/ess-gen/src/types.rs:499-502`), held as its token so the sign of zero survives in
the bytes — § 10.2.1. It is a distinct kind rather than a use of `number` because the two have different
admitted domains and different binding obligations, and because `number` is where `Decimal` lands.

`billing.invoice.Invoice` is lowerable with these kinds: `payee` is a `union` over
`{person: Email, company: CompanyRef}` tagged `kind`, and `metadata` is a `map` with key `String` and a
`string` value (`ESS/examples/billing/domains/invoice.yaml:44-49,112-121`). Nothing in that entity is
refused by type — which is the condition for the required billing acceptance fixture to exist at all.

### 10.2 Numbers: the source's domain, exactly

**The source's numbers are exact, and this is the correction that matters most.** The previous draft of
this document refused every `Binary64` predicate on the stated ground that *"`number::compare` calls
`-0.0` and `0` equal while ESS Binary64 preserves signed zero"*. The premise is false in the half that
mattered. `ess_primitives::facts::Number` is not `Number(f64)`; it carries an exact decimal
`units × 10⁻ˢᶜᵃˡᵉ` beside the binary64 it serialises as, its `PartialEq` is `cmp(..) == Equal`, and its
`cmp` is the exact value (`ESS/crates/specify/ess-primitives/src/facts.rs:32-46,454-485`;
`ESS/docs/design/review-primitive-semantics.md:182-239`). The design page states the consequence in
one line: *"**`-0.0` and `0.0` are one value** … and it is what a guard `amount == 0` has to mean. The
signed zero `Primitive::Binary64` promises survives where the model actually promises it — in the
**bytes**"* (`:229-234`).

ER already holds what is needed to answer that. The workspace enables `serde_json`'s
`arbitrary_precision` (`Cargo.toml:30`), so ER holds the **token**; `number::compare` orders by exact
decimal value at any exponent (`crates/entity-core/src/number.rs:6-55`) and its own test pins
`("-0.0", "0", Ordering::Equal)` (`:173`); `values_equal` routes number equality through it
(`crates/entity-core/src/runtime.rs:669-671`) and schema bounds use it rather than `f64`
(`crates/entity-core/src/validation.rs:994-1016`). **Binary64 predicates are admitted — equality and
ordering both** — and signed zero survives in ER's bytes for the same reason it survives in ESS's: the
token is what is written.

Where the two do **not** already agree is the *reading* of a token, and § 10.2.1 is the rule that
closes it. `number::compare` is an exact reading of the token; the source's reading is the token as its
own two doors observe it, and those two readings differ on a decimal binary64 does not carry. That
difference is not one-directional and cannot be bounded by a sentence — § 10.2.1.

**Stored values are never converted and never rounded.** No rule below rewrites a token: the authored
token is what a field holds, what an event payload publishes, what a `DecisionRecord` compares by and
what replay reads back. What § 10.2.1 adds is a rule for *reading* a token as the value a predicate is
answered about, which is a pure function of the token and writes nothing.

| ESS primitive | ER kind | admitted domain | why |
| --- | --- | --- | --- |
| `Integer` | `integer` | `[i64::MIN, i64::MAX]` | ESS admits exactly that range and `is_integral` **is** `as_i64().is_some()` (`ESS/docs/design/review-primitive-semantics.md:34,104-114`). ER's `is_i64() \|\| is_u64()` (`crates/entity-core/src/validation.rs:902-908`) is wider by the `u64` tail, so `service/1` narrows it: `IntegerBeyondSourceRange`. `kernel/1` is untouched. |
| `Decimal` | `number` | the exact decimal the token spells | ESS's abstract value is *"an exact decimal, `units × 10⁻ˢᶜᵃˡᵉ`"* (`:35`). ER holds the token exactly. |
| `Binary64` | `binary64` | a finite binary64, carried as its token | `:36`. `Binary64Spelling` (§ 9) is what keeps the token one binary64 carries; `NaN` and infinities are refused by ESS's own constructor (`ESS/crates/specify/ess-primitives/src/facts.rs:163-169`) and by the `binary64` kind. |

#### 10.2.1 `source-number/1`: how a `service/1` predicate reads a stored number

The previous draft bounded the numeric gap as one-directional — *"it refuses a command ESS would
permit rather than permitting one ESS would refuse"* — and that claim is **false**, in both
directions, because a negated or `ne` guard reverses it. With `amount` authored
`1.0000000000000000001`, ESS observes exactly `1` and answers `ne 1` **false**; a reader comparing the
token exactly answers **true** and takes an accepting branch ESS does not take. Rewording the bound
would not fix it. What fixes it is a reading rule.

**`service/1` answers every numeric predicate on the value the source would have observed, and stores
the token the author wrote.** The two are separate and both are exact:

| | rule |
| --- | --- |
| **stored** | the authored token, byte for byte, in the field, the event payload, the response, the `DecisionRecord` and the replay comparison. Nothing rounds it. `arbitrary_precision` is what keeps it (`Cargo.toml:30`) |
| **observed** | `source-number/1`, below — a pure function of the token, used by every `service/1` predicate and by nothing else |

`crates/entity-core/src/observed.rs` is the whole of it. One module, three public items, no evaluator,
no registry, no configuration:

```rust
/// The value a `service/1` predicate reads a stored JSON number as.
#[derive(Debug, Clone, Copy)]
pub struct Observed(Repr);

/// `units × 10^-scale` beside the binary64 the source writes, or a magnitude no such pair spells.
#[derive(Debug, Clone, Copy)]
enum Repr {
    Exact { units: i128, scale: u8, binary: f64 },
    Binary64(f64),
}

impl Observed {
    /// The wire and field path: a JSON number as the source's document reader takes it.
    pub fn of_number(value: &serde_json::Number) -> Option<Self>;
    /// The literal path: a number written inside a condition, as the source's predicate parser takes it.
    pub fn of_literal(text: &str) -> Option<Self>;
    /// The exact value, and the binary64 only where there is no exact value.
    pub fn cmp(self, other: Self) -> std::cmp::Ordering;
    /// ESS's `is_truthy` numeric arm, which tests the carried binary64.
    pub fn is_zero(self) -> bool;
    /// The canonical spelling of § 7.3.2.
    pub fn exact_text(self) -> String;
}
```

**The two doors, reproduced.** Each is the source function named beside it, and `entity-core` gains no
ESS dependency: the rules are copied as code, the citations are what a reviewer checks them against,
and AGENTS.md invariant 1's two-crate dependency list is unchanged.

| step | rule | source |
| --- | --- | --- |
| `of_number` | `as_i64()` → exact signed integer; else `as_u64()` → exact unsigned integer; else parse the retained number spelling as finite `f64` with Rust's correctly rounded parser → *of binary64*; out of range → `None` | `Node::from_value`'s number arm, `ESS/crates/specify/ess-primitives/src/node.rs:46-66`, under the actual ESS CLI decoding profile below |
| `of_literal` | parse the text as `f64`, requiring **finite**; read the authored decimal by the grammar below; **keep the authored value only when its scale is 0 or it equals the canonical decimal of that `f64`**; otherwise *of binary64* | `Number::parse_decimal`, `ESS/…/facts.rs:223-254` |
| *of binary64* | `canonical_decimal(v)` → `Exact`; `None` → `Binary64(v)` | `Repr::of_binary64`, `ESS/…/facts.rs:282-288` |
| `canonical_decimal(v)` | integral and `-2^63 ≤ v ≤ 2^63` → `(v as i128, 0)`; otherwise the authored-decimal read of `format!("{v}")` — the shortest decimal that round-trips | `ESS/…/facts.rs:301-321` |
| authored-decimal read | `[+-]?digits[.digits][eE[+-]?digits]`; `None` when the text is not that grammar, when the digits do not fit an `i128`, or when the scale exceeds **255**; trailing zeroes normalised away so one value has one pair | `exact_of_decimal_text` and `normalise`, `ESS/…/facts.rs:325-378` |
| `cmp` | both `Exact` → compare the two decimals digit by digit; otherwise `f64::total_cmp` of the carried binaries | `Number::cmp` and `exact_cmp`, `ESS/…/facts.rs:486-491,402-434` |
| `is_zero` | the carried binary64 `== 0.0` | `FactValue::is_truthy`'s numeric arm, `ESS/…/facts.rs:631-637` |

**`integer` is integer-first and exact.** `of_number` tries `as_i64` and `as_u64` before it reaches any
binary64, so an integer token is carried as the integer it is, at scale 0, and `cmp` compares the
scaled integers. `9007199254740993` and `9007199254740992` are two values with two orderings, not one
`f64` — which is the source's own stated fix, review finding F08 (`ESS/…/facts.rs:33-44`).

**The feature configurations do not read every token alike.** A coordinator Rust witness at the
locked `serde_json 1.0.151` demonstrates `946.3702156715110866946` reading as binary64 bits
`408d92f633a24cda` with default features and `408d92f633a24cdb` through arbitrary-precision
`Number::as_f64`. The default decoder uses its own significand/exponent arithmetic, not Rust's
correctly rounded `str::parse`. `-0` also selects different integer/binary64 arms. Calling ER's
`Number::as_f64` therefore does not reproduce the source reader.

**The source profile is the actual ESS CLI, not a guessed default-feature build.** Locked/offline
`cargo tree -e features -p ess-cli -i serde_json` shows `float_roundtrip`, contributed by
`jsonschema` and `jsonschema-value`. The measured CLI profile reads the witness as
`408d92f633a24cdb`, agreeing with ER. The isolated `ess-primitives` default-feature profile lacks
that feature and reads `408d92f633a24cda`; there is no single token-to-value answer shared by every
possible source build. These profiles are recorded separately, not claimed equivalent.

`source-number/1` therefore fixes the current service-producing CLI's reading: integer-first,
then correctly rounded finite binary64, followed by the source `Number` canonical representation.
Use Rust's number parser for that fallback rather than feature-sensitive `serde_json` decoding;
Cargo feature unification must not change a recorded definition's answer. This changes neither ESS
profile nor its existing serialized artifacts. Standalone library callers supply typed values under
their selected input contract; equivalence to a different raw-token reader is not inferred.
The literal door retains `Number::parse_decimal`'s separate exact-integer/round-trip rule.

`of_number` is fallible because ER can hold `1e400` while the source wire reader refuses it.
Under `service/1`, numeric schema admission refuses a value outside this source-observation domain
with a path-bearing `ValidationError`; definition admission rejects unobservable numeric literals
and bounds. Do not panic, saturate or invent a value. No new kernel/1 restriction follows. Identity
addressing is total over the resulting admitted numeric domain; a binding presented with an
unvalidated value receives a typed error rather than an invented address.

Operand origin is retained while evaluating: a number reached through a reference uses
`of_number`; an authored numeric literal uses `of_literal` (and the lowerer emits the source's
already-resolved literal spelling). Membership preserves that distinction for each operand, and
schema bounds use the literal door. This requires no persisted generic property bag: the existing
condition/template syntax distinguishes a reference from a literal. Reading both through the wire
door would erase the source distinction this rule is intended to preserve.

Stored-byte preservation starts at the kernel's received `Value` and its canonical record bytes.
It does not promise recovery of a raw JSON lexeme that a caller's decoder already normalized,
such as an exponent's plus sign or integer `-0`. No observation step changes the received value;
the admitted `binary64` spelling `-0.0` retains its sign in that value and in the record.

**The versioning, and why it is in the definition.** `EntityDefinition.number_observation` (§ 2) is a
closed enum whose only variant today is `source-number/1`. It is part of the definition snapshot
inside every `DecisionRecord`, so replay answers a recorded decision under the rule that decided it.
The source has filed the decimal half of its read door as its own open work —
*"Filed as story:primitive-canonical-serialization"*
(`ESS/docs/design/review-primitive-semantics.md:312-320`) — and when that lands it is a **new variant**
here, chosen by a new definition. It cannot change what a `service/1` replay already means, and
authoring that variant is outside this contract.

**What shares the rule, and what must not.** Under `service/1`:

* `compare`'s numeric row (§ 10.4) — `Observed::cmp`;
* `truthy`'s numeric arm (§ 10.4) — `!Observed::is_zero()`;
* numeric equality and numeric membership: `eq`, `ne`, `in` and `contains` over two numbers, which are
  what the lowerer emits for ESS's `Compare { eq | ne }`, `AnyOf` and `NoneOf`;
* schema `min`/`max` bounds on an `integer`, `number` or `binary64` field, because an ESS bound is an
  authored literal the source reads through the same door;
* § 7.3.2's identity address and identity equality, so a `binary64` or `number` identity addresses the
  value the predicates answer about.

Under `kernel/1`, **nothing changes**: `number::compare` (`crates/entity-core/src/number.rs:6-55`),
`values_equal` (`crates/entity-core/src/runtime.rs:669-671`), `gt`/`gte`/`lt`/`lte`
(`:739-756`), `in`/`contains` and `validate_number` (`crates/entity-core/src/validation.rs:994-1016`)
keep every answer they give today. `number::compare` is not deleted and not rewritten; `Observed` sits
beside it and is reached only from a `Semantics::Service1` definition.

**The three counterexamples, answered.**

| case | ESS | `service/1` | stored |
| --- | --- | --- | --- |
| `amount` authored `1.0000000000000000001`, guard `ne 1` | **false**. `parse_decimal` reads `(10000000000000000001, 19)`, whose scale is not 0 and which is not `canonical_decimal(1.0) = (1, 0)`, so the value collapses to the binary64 `1.0` and is exactly `1` (`ESS/…/facts.rs:243-253`) | **false**. `of_literal` and `of_number` both reach `of binary64` with `1.0` | the token `1.0000000000000000001`, unchanged, in the field, the event and the record |
| the same guard negated — `not: { ne: [amount, 1] }` | **true** | **true** — the negation of one answer, not a second rule | as above |
| a binary64-underflow token such as `1e-400` in `truthy` | **false**. The scale 400 exceeds 255 so no exact value is read, the `f64` parse is `0.0`, and `is_truthy`'s numeric arm tests the carried binary64 | **false**. `is_zero` tests the same carried binary64 | the token `1e-400`, unchanged |
| `9007199254740993` against `9007199254740992` | two values, ordered | two values, ordered — the exact integer path, never an `f64` | both tokens, unchanged |

The third row is the one the previous draft got wrong in the other direction: it read the exact token,
answered *nonzero*, and diverged from ESS's *false*. The rule above answers `false` and still stores
`1e-400`, which is the whole shape of this correction — **parity in the answer, exactness in the
bytes**.

**One `f64` lives in `observed.rs`, and it is not a conversion of a stored value.** ESS's reading is
binary64-mediated in two places — the `as_f64` fallback of the wire door and `canonical_decimal`'s
shortest-round-trip spelling — so reproducing the *answer* requires reproducing those. Both are
deterministic: the versioned wire observation and separate literal parser retain their specified answers,
and `f64`'s `Display` supplies the shortest round-tripping decimal. Nothing written by the kernel passes through them:
`observed.rs` returns an `Ordering`, a `bool` and a canonical text, and never a number a field, event,
response or record is built from. `tests/purity.rs` is unaffected — its banned list is clocks,
filesystem, network, environment, process, thread, randomness, asynchrony and unordered iteration
(`crates/entity-core/tests/purity.rs:21-56`), and `f64` is none of them — and the dependency list
stays `serde` and `serde_json`.

**`DecimalSpelling` and `Binary64Spelling` (§ 9)** keep their meaning: they are what a binding declares
about the tokens it emits. They are no longer load-bearing for agreement, because agreement is now the
reading rule rather than a restriction on what may be written.

**Absence versus null.** ER distinguishes an absent key from a declared `null`
(`DeclaredDefault`, `crates/entity-core/src/definition.rs:207-234`), and a reference resolving to a
present `null` counts as *nothing observed*, not as a value
(`crates/entity-core/src/runtime.rs:769-797`). ESS facts have no null
(`ESS/crates/specify/ess-primitives/src/facts.rs:563-573`), so the mapping is total in one direction:
ESS `Optional` absent is ER key-absent. A `null` in a lowered instance is refused by the existing
per-kind validation (`crates/entity-core/src/validation.rs:888-967`), and `type: json` is not an
admitted lowering target, so there is no hole where a null could be admitted as a value.

**Unknown versus false** is unchanged and is what makes the mapping safe: questions about the store
(`exists`) are two-valued, questions about a value are three-valued, and an unanswerable rule refuses
rather than answers `false` (`crates/entity-core/src/runtime.rs:572-614`,
`crates/entity-core/src/truth.rs`).

### 10.3 Primitive mapping, and the text scale context

The governing ESS fact for the text-shaped primitives is `ScalarKind::of`: `String`, `Timestamp`,
`Duration`, `Uuid` and `Bytes` are all **`Text`** to the predicate evaluator
(`ESS/crates/specify/ess-domain/src/expression.rs:26-38`), and a `FactValue` is only
`Bool | Number | Text` (`ESS/crates/specify/ess-primitives/src/facts.rs:563-573`).

| ESS | ER | Admitted |
| --- | --- | --- |
| `String` | `string` | everything, including `min_length`/`max_length` from newtype invariants |
| `Boolean` | `boolean` | equality; ordering is `Unknown`, § 10.4 |
| `Integer` | `integer` | equality, ordering, bounds — read under `source-number/1`, integer-first, narrowed to the `i64` range (§ 10.2.1) |
| `Decimal` | `number` | equality, ordering, bounds — read under `source-number/1`; the authored token is stored unchanged |
| `Binary64` | `binary64` | equality and ordering under `source-number/1`, `-0.0 == 0.0`, sign kept in the bytes (§ 10.2.1) |
| `Timestamp`, `Duration`, `Uuid`, `Bytes` | `string` | equality on both sides; ordering through the scale context below, which is `Unknown` when no declared scale contains both values |
| `Optional<T>` | `required: false` | as above |
| `List<T>` | `array` + `items` | membership through `in`/`contains`; quantification and `.count`, § 10.4 and § 10.6 |
| `Map<K,V>` | `map` + `key` + `items` | § 10.1; quantification over values and `.count`, § 10.4 and § 10.6 |
| newtype over a primitive | the underlying kind | plus its invariants, § 10.5 |
| struct | `object` + `properties` | |
| enum | `enum` + `values` | |
| union | `union` + `tag` + `variants` | § 10.1 |

**Text ordering is implemented, not refused, and the previous draft's refusal is reversed.** Ordering
two `Text` values in ESS routes through `facts.scales()` and answers by rank when one declared scale
contains both, `Truth::Unknown` when none does or when two disagree
(`ESS/crates/specify/ess-primitives/src/predicate.rs:556-567`,
`ESS/crates/specify/ess-primitives/src/facts.rs:1008-1028`). The previous draft read *"an ESS
specification declares no scales at all"* as the end of the sentence and refused every `Timestamp`,
`String`, `Duration`, `Uuid` and `Bytes` comparison at lowering. The sentence does not end there. It
reads: *"**An ESS specification declares none**, so every `<`, `<=`, `>` and `>=` between two text
values is unevaluable **until `InputFacts::with_scales` supplies one**"*
(`ESS/crates/verify/ess-conformance/src/decision.rs:191-196`), and the setter's own documentation names
what supplies it — *"something outside the specification — an AEP protocol, whose `scales:` this
takes"* (`ESS/crates/verify/ess-conformance/src/input.rs:391-402`).

So the scale set is **context, and its absence is a value of that context, not a missing feature**:

* `EntityDefinition.scales: BTreeMap<String, Vec<String>>` — scale name to its values, lowest rank
  first, reproducing `ess_primitives::facts::Scales` field for field
  (`ESS/crates/specify/ess-primitives/src/facts.rs:968-975`). It is a *declaration*, so it is
  snapshotted into every `DecisionRecord` with the rest of the definition and replay reads the
  snapshot: no lookup, no IO, no clock (§ 3).
* `scale_compare(l, r)` scans every declared scale for one containing both values and answers `None`
  when none does **or when two disagree** — ESS's `Scales::compare` exactly, including the ambiguity
  arm (`ESS/crates/specify/ess-primitives/src/facts.rs:1008-1028`).
* **The exact no-scale case**: `scales` empty, or no scale containing both — the ordering is
  `Truth::Unknown`. Never `false`. That is the case every ESS specification is in today, and it is now
  *answered with Unknown* rather than refused at lowering.
* The `ScaleDeclaration` obligation (§ 9) is where a binding that carries an AEP protocol declares the
  protocol's `scales:` into the definition it lowers to.

`before`/`after` (`crates/entity-core/src/definition.rs:477-495`) stay `kernel/1` operators and are
**not** a lowering target. They answer `True`/`False` for any pair they can parse, where ESS answers by
rank or `Unknown`, and chronological order is not scale order. An ESS `Timestamp` comparison lowers to
`compare` (§ 10.4), like every other text comparison. This closes the construct rather than refusing
it; what remains open is that ESS itself gives an instant no chronological order, which § 7.5 and § 15
record as the source's.

### 10.4 Predicate mapping, and the four new operators

`Predicate` at `ESS/crates/specify/ess-primitives/src/predicate.rs:326-374`, evaluated at `:512-537`;
both sides already share one Kleene table (`crates/entity-core/src/truth.rs:18-101`).

`CONDITION_OPERATORS` (`crates/entity-core/src/definition.rs:418-421`) gains four, all `service/1`-only
and all refused in a `kernel/1` definition by `SemanticsKeyNotAvailable`:

```text
"all","any","not","exists","eq","ne","gt","gte","lt","lte","in","contains","before","after",
"compare","truthy","for_all","for_any"
```

| ESS | ER |
| --- | --- |
| `Always` / `Never` | `true` / `false` |
| `All` / `Any` / `Not` | `all` / `any` / `not` |
| `Defined` | `exists` — two-valued on both sides (`ESS/…/predicate.rs:527`) |
| `Compare { left, op, right }` | **`compare`**, below |
| `Truthy` | **`truthy`**, below |
| `Forall` / `Exists` | **`for_all` / `for_any`**, below |
| `AnyOf` / `NoneOf` | `in: [$path, [values…]]` and `not: {in: …}` — including **empty** `values`. ER's `In` resolves both operands and answers `Unknown` when the needle resolved to nothing and `false` when it resolved and the literal list is empty (`crates/entity-core/src/runtime.rs:633-643`, `resolve_operand` at `:780-825`), which is exactly ESS's table (`ESS/…/predicate.rs:528-533`). No kernel change and no expansion into equality nodes. |

#### `compare` — the exact three-valued comparison

```yaml
compare: { left: $fields.issued_at, op: lt, right: $args.input.deadline }
```

`op` is a closed enum `{eq, ne, lt, lte, gt, gte}`. The table is `Predicate::evaluate_compare`
(`ESS/crates/specify/ess-primitives/src/predicate.rs:540-584`) line for line:

| case | answer |
| --- | --- |
| either operand resolves to nothing (absent, or present and `null`) | `Unknown` |
| either resolved operand is not a scalar — an array, an object, a `map`, a `union` | `Unknown`, ESS's `PathNotScalar`: *"A list, a map, a union and a struct have no scalar spelling … so this is unevaluable by construction"* (`ESS/crates/verify/ess-conformance/src/decision.rs:171-181`) |
| both are numbers | observe each operand through its own wire/literal door (§ 10.2.1), then `op.accepts(left.cmp(right))`; absent observations answer `Unknown`, never panic. Admitted schema values and definition literals have already passed the finite-domain check |
| both are strings and `op ∈ {lt, lte, gt, gte}` | `scale_compare(l, r)`: `Some(ord)` → `op.accepts(ord)`; `None` → `Unknown` (§ 10.3) |
| `op ∈ {lt, lte, gt, gte}` and any other pair of scalars | `Unknown`, ESS's `TypesNotOrdered` — *"Cross-type … and also the same type where no ordering exists, such as a boolean against a boolean"* (`ESS/crates/verify/ess-conformance/src/decision.rs:203-213`) |
| `op ∈ {eq, ne}` | scalar equality: numbers by `Observed::cmp` being `Equal`, strings by bytes, booleans by value, two different scalar kinds never equal |

`eq`, `ne`, `in` and `contains` over two numbers use the same rule when they appear in a `service/1`
definition, because they are what the lowerer emits for ESS's `Compare { eq | ne }`, `AnyOf` and
`NoneOf` and a membership test that disagreed with `compare` would be a second numeric semantics
inside one document. Their `kernel/1` answers are untouched (§ 10.2.1).

`compare` exists as its own operator because two of those rows are **not** what ER's existing operators
answer, and the difference is `Unknown` against `false`. `gt`/`gte`/`lt`/`lte` answer two-valued `false`
for non-numeric operands (`crates/entity-core/src/runtime.rs:739-756`), and `eq`/`ne` compare arrays and
objects structurally (`:669-689`) where ESS answers `Unknown`. Both keep their meaning for `kernel/1`;
neither is a lowering target. `CompareOperandNotAddressable` refuses a literal array or object operand
at registration, so the second row is reachable only through a reference.

#### `truthy` — ESS's bare-path reading, reproduced

```yaml
truthy: $fields.is_recurring
```

`FactValue::is_truthy` has three arms and `truthy` has the same three
(`ESS/crates/specify/ess-primitives/src/facts.rs:629-637`):

| the resolved value | answer |
| --- | --- |
| nothing resolved | `Unknown` — ESS's `Truthy` is `facts.fact(path).map_or(Truth::Unknown, …)` (`ESS/…/predicate.rs:524-526`) |
| a boolean | the boolean |
| a number | **the value is not zero** |
| a string | **non-empty and not the four characters `false`** |
| anything else — an array, an object | `Unknown`, for the `PathNotScalar` reason above |

The numeric arm is `!Observed::of_number(v).is_zero()` — § 10.2.1 — which is ESS's
`number.get() != 0.0` against the **carried binary64**, arm for arm
(`ESS/crates/specify/ess-primitives/src/facts.rs:631-637`). The previous draft refused `Truthy` over
text or a number on the ground that *"reproducing that in the kernel would import a float comparison
the kernel does not otherwise have"*, and the draft before this one admitted it by testing the exact
token instead. Both are wrong in the underflow class: a token such as `1e-400` is exactly nonzero and
its carried binary64 is `0.0`, so the exact test answers **true** where ESS answers **false**. Reading
through `Observed` answers `false` and still stores `1e-400`. § 11 pins that pair.

The text arm is a literal string test and needs no discussion; ESS's own comment calls it *"the fact is
present, therefore true"*, with `"false"` as the one written exception.

#### `for_all` / `for_any` — the closed quantifier

The source's own YAML form, keys included
(`ESS/crates/specify/ess-primitives/src/predicate.rs:44-61`):

```yaml
for_all:
  in: $fields.lines          # the collection
  as: line                   # the name the body binds each element to
  that: { compare: { left: $line.quantity, op: gt, right: 0 } }

for_any:
  in: $fields.lines
  as: line
  that: { eq: [$line.description, "deposit"] }
```

```rust
pub struct Quantifier {
    pub over: Value,           // `in`:  a reference to an `array` or `map` field
    pub bind: String,          // `as`:  one non-empty path segment
    pub body: Box<Condition>,  // `that`
}
```

The outer key is renamed and the inner three are not. ESS spells the existential quantifier `exists:`
with a mapping, and ER's `exists` is already the *store* question — ESS's `Defined`. Overloading one key
on the shape of its operand would be ambiguous, because `exists:` legitimately takes an object operand
today. So the pair is `for_all`/`for_any`, stated as a spelling rule with the source key named, exactly
as § 4.3 states the `Otherwise` one.

**Evaluation**, which is `Quantified::evaluate` (`ESS/…/predicate.rs:393-421`) element for element:

| case | `for_all` | `for_any` |
| --- | --- | --- |
| `in` resolves to nothing, or to a value that is not an `array` or a `map` | `Unknown` | `Unknown` |
| an empty collection | **`True`** — vacuous | **`False`** |
| otherwise | fold `and` from `True` over the elements | fold `or` from `False` over the elements |

The unobserved row is the one the source writes a paragraph about: *"An unobserved collection evaluates
to `Truth::Unknown`, never to the vacuous truth an empty one would give: nobody looked and there was
nothing to look at [are different]"* (`ESS/…/predicate.rs:67-70`), and `cardinality` answers `None` for
anything that is not a whole count for the same reason (`ESS/…/facts.rs:1044-1057`).

**Elements, in order.** An `array`'s elements in index order. A `map`'s **values** in canonical key
order, which is the order ER already canonicalizes an object into
(`crates/entity-core/src/runtime.rs:995-1008`); ESS quantifies a `Map` over its values and publishes no
key order (`ESS/crates/specify/ess-domain/src/expression.rs:232,699-701`), so ER declares one rather
than leaving it to a map's iteration. Map **keys** are not addressable: ESS's path resolution admits
only `count` and quantification on a `Shape::Map` (`:451-478`), and `QuantifierBodyScope` refuses a body
address that reaches for one.

**Short-circuit.** Evaluation stops as soon as the fold is settled — `False` for `for_all`, `True` for
`for_any` — which is the source's rule and the source's reason: *"Stopping is not an optimisation:
walking a collection whose verdict is already settled would report causes from elements nobody is
waiting on"* (`ESS/…/predicate.rs:412-415`). This is the one place `service/1` departs from ER's own
stated habit for `all`/`any`, which deliberately walks every operand to collect every unobserved
address (`crates/entity-core/src/runtime.rs:578-582`). The two rules give the same **truth**; they
differ only in which addresses a later `OutcomeUnobservable` names, and the quantifier follows the
source. `all`/`any` keep ER's rule.

**Binding and nesting.** Inside the body, `$<bind>` is the element itself and `$<bind>.<path>` is a
path into it; every other address passes through to the enclosing scope untouched. Nesting one
quantifier inside another is how an inner body still reaches the outer element: the inner rewrites its
own binder and hands the result outward (`ESS/…/predicate.rs:423-446`). A nested `as` equal to an
enclosing one is **admitted, and the inner one wins**, because that is what the source's rewrite does;
ER does not refuse it, because refusing it would be narrower than the source. § 11 pins both the nesting
and the shadowing.

**Depth.** A condition nests at most 32 deep, `ConditionTooDeep` beyond it — ESS's
`MAX_PREDICATE_DEPTH`, and the same number *"across the workspace"* for the same reason: a document
nested past it is refused with a code and a limit rather than by the deserializer
(`ESS/…/predicate.rs:292-298`).

### 10.5 Invariants

ESS entity invariants (`ESS/crates/specify/ess-compiler/src/ir.rs:384-385`) become ER entity invariants
once every path and operator above is admitted, and after § 10.4 that is every one of them. A nominal
type's own invariants (`ResolvedBody::Newtype`/`Struct`,
`ESS/crates/specify/ess-compiler/src/ir.rs:302-318`) are rewritten at the field's path into the
entity's invariant list rather than dropped:

| where the nominal type sits | the rewrite |
| --- | --- |
| a required field | the predicate at `$fields.<path>` |
| an **optional** field | `any: [{not: {exists: $fields.x}}, <predicate>]` — exact, because `exists` is two-valued |
| inside a `List<T>` | `for_all: {in: $fields.<path>, as: e, that: <predicate at $e>}` |
| inside a `Map<K,V>`'s value | the same, over the map's values |
| inside a `union` variant | admitted when the variant is reachable by a fixed path — `$fields.payee.value` under the tag test — and under a `for_all` when it is inside a collection |

The previous draft refused the two collection rows *"because that is the quantifier case"*. The
quantifier now exists, so they close. `billing.invoice.Money`'s `amount >= 0`
(`ESS/examples/billing/domains/invoice.yaml:32-33`) reaches
`billing.invoice.LineItem.unit_price` inside `Invoice.lines`, a `List<LineItem>`, and lowers to a
`for_all` over `$fields.lines`.

### 10.6 Collection addressing

Two address forms exist in ESS and not in ER, and a lowered invariant or guard needs both. Both are
`service/1`-only, read from `definition.semantics`, and neither is reachable from a `kernel/1`
definition.

| address | resolves to | source |
| --- | --- | --- |
| `<collection>.count` | the number of elements of an `array`, or of values of a `map`, as a JSON number | `ESS/crates/specify/ess-domain/src/expression.rs:458-472` — typed `Integer`, and selecting past it is refused |
| `<array>.<n>` | the element at index `n`, where `n` is `0` or a digit string with no leading zero | `ESS/crates/specify/ess-domain/src/expression.rs:473-476,497-502` |

ER's `lookup` walks objects only — `value.as_object()?.get(segment)` — so both forms resolve to nothing
today (`crates/entity-core/src/runtime.rs:965-976`) and every existing `kernel/1` definition keeps that
answer. Under `service/1` they resolve as above, and a `map`'s keys stay unaddressable (§ 10.4), so
`{metadata: {count: "7"}}` cannot be read as its own `count` key: the only `count` a `map` has is its
size. `validate_reference_path` gains the same two forms so an address into a collection is checked at
registration, which is invariant 5's requirement and not a new rule.

## 11. Acceptance

Executable, decisive, and runnable without Eventlog, a provider or a network. Focused first, then the
repository gate:

```console
cargo test --locked -p entity-core
cargo test --locked -p entity-store -p entity-executor
task check
ess specify validate --path ess/service-semantics
```

plus the CI MSRV job on 1.85.0 — this contract adds no dependency and no post-1.85 language feature, so
`entity-core` stays on the workspace minimum (`Cargo.toml:26`) and its dependency list stays `serde` and
`serde_json` (AGENTS.md invariant 1).

Required cases, each named for the behaviour it protects (AGENTS.md § Conventions), each asserting a
variant rather than `is_err`. Every row of § 14's coverage table has at least one.

| Test | Pins |
| --- | --- |
| `a_kernel_1_definition_and_record_serialize_to_the_bytes_they_serialized_to_before_branches` | § 1.1, over the committed fixtures |
| `a_kernel_1_decision_still_frames_as_er_record_1_and_er_request_1` | § 1.2, byte fixtures |
| `a_service_1_decision_frames_as_er_record_2_and_its_request_carries_arguments_not_fields` | § 1.2 |
| `a_service_1_creation_without_branches_reconstructs_the_request_the_caller_sent` | § 1.2, the branchless row: two creations differing in what the caller sent reconstruct to different bytes |
| `a_branchless_service_1_creation_reconstructs_the_normalized_input_the_caller_sent` | § 1.2, the same row with repeated input and a defaulted field |
| `the_framing_the_creation_path_replay_and_the_verifier_read_one_service_1_test` | § 1.2, the four readers, over a branchless and a branched creation alike |
| `a_service_1_creation_record_whose_arguments_do_not_produce_its_fields_is_refused` | § 1.2, § 6: the two halves are redundant by construction, so a forged one is refused by both recomputations |
| `a_branchless_service_1_retry_carrying_other_input_is_not_the_request_already_committed` | § 1.2, the retry the reconstruction exists to decide |
| `an_er_record_1_reader_refuses_an_er_record_2_document_by_naming_the_framing` | § 1.2, § 1.3 |
| `a_batch_of_kernel_1_records_keeps_its_er_batch_1_bytes_while_a_service_member_refuses_at_the_record_tag` | § 1.2 |
| `a_pre_service_reader_refuses_a_service_1_definition_by_naming_its_unknown_field` | § 1.3, against a struct mirroring the `kernel/1` shape |
| `a_kernel_1_definition_using_a_service_operator_is_refused_at_registration` | § 1.3, § 10.4 `SemanticsKeyNotAvailable` |
| `the_kernel_checks_entity_mismatch_before_unknown_state_and_both_documents_say_so` | § 4.1, the code's order and the two corrected documents |
| `a_service_1_operation_runs_the_sixteen_steps_in_the_numbered_order` | § 4.2, asserting the sequence of observable refusals across steps 0 to 15 |
| `the_identity_mirror_runs_before_the_invariants_and_the_response_after_the_events` | § 4.2, steps 11, 12, 13, 14; fails if any pair is swapped |
| `renumbering_a_service_1_run_against_the_legacy_column_recovers_the_twelve_step_order` | § 4.2, the mapping, over a `kernel/1` definition |
| `an_input_guard_answers_before_the_held_state_is_tested` | § 4.3, the `pay_invoice` shape; fails if the two are swapped |
| `the_wrong_state_set_is_the_complement_of_every_move_source_not_of_the_selected_branch` | § 4.3, over the W2 lifecycle |
| `a_selected_move_branch_whose_from_excludes_the_state_answers_the_wrong_state_branch` | § 4.3 |
| `a_selected_move_branch_with_no_wrong_state_branch_still_answers_invalid_transition` | § 4.3, `kernel/1` parity |
| `the_close_ticket_witness_registers_and_refuses_only_the_state_input_pair_the_source_leaves_open` | § 4.4 `UnspecifiedMoveSource`; the same operation answers normally for `Open`+urgent and `Held`+not-urgent |
| `an_operation_mixing_a_state_guard_and_a_wrong_state_branch_is_refused_at_registration` | § 2.1 `WrongStateWithStateGuard` |
| `a_state_guard_naming_a_state_its_move_does_not_start_from_is_refused` | § 2.1 `GuardStateOutsideMove` |
| `a_wrong_state_branch_whose_moves_cover_every_state_is_refused_at_registration` | § 2.1 `WrongStateUnreachable` |
| `a_selector_free_branch_that_is_not_last_is_refused` | § 2.1 `AmbiguousDefaultOutcome`, § 4.3 |
| `an_unobservable_guard_refuses_the_command_instead_of_taking_the_next_branch` | § 4.3, naming the unresolved addresses |
| `an_outcome_selector_may_not_read_to_state` | § 2.2 |
| `a_creation_set_may_not_read_fields` | § 2.2 |
| `a_matching_in_state_branch_with_no_when_is_taken` | § 4.3, the `True` selector; fails if a bare state guard is unselectable |
| `a_matching_in_state_branch_whose_when_is_false_is_skipped` | § 4.3, the false-predicate control |
| `a_non_matching_in_state_branch_is_skipped_without_evaluating_its_when` | § 4.3, an unobservable guard on a skipped branch does not refuse the command |
| `a_selector_free_default_is_still_separate_from_a_bare_state_guard` | § 4.3, `AmbiguousDefaultOutcome` counts neither the state guard nor its position |
| `a_creation_branch_event_payload_reads_a_creation_argument_no_field_stores` | § 2.3, the billing `customer_email` shape |
| `a_creation_branch_response_reads_a_creation_argument_and_a_post_set_field` | § 2.3, § 5.3, `CreateOutcomeTemplate` |
| `a_kernel_1_creation_event_payload_still_cannot_read_args` | § 2.3, `CreateTemplate` unchanged |
| `branches_are_tried_in_declared_order_and_the_first_applicable_one_is_taken` | § 4.3 |
| `a_refusing_branch_produces_no_record_no_event_no_response_and_no_revision` | § 5.1 |
| `an_accepted_branch_that_changes_nothing_still_advances_one_revision_with_zero_events` | § 5.2 |
| `a_creation_branch_with_the_creates_effect_and_no_events_is_admitted_and_records_created` | § 5.2, zero-event creation |
| `a_kernel_1_creation_that_emits_nothing_still_validates_and_still_creates` | § 5.2, no regression |
| `an_update_branch_keeps_its_state_and_records_updated_without_a_transition` | § 6 |
| `a_declared_response_is_materialised_from_the_selected_branch_and_replays_byte_for_byte` | § 5.3 |
| `a_branch_that_leaves_a_required_response_field_undetermined_is_refused_at_registration` | § 2.1 `OutcomeResponseIncomplete` |
| `a_refusing_branch_that_declares_responds_is_refused_at_registration` | § 2.1 `RefusalMutatesState` |
| `a_creation_branch_emits_zero_one_or_many_events_in_declaration_order` | § 2, event multiplicity |
| `a_service_1_history_is_refused_by_rehydrate_before_any_event_arg_is_read` | § 5.4, § 12 |
| `an_integer_identity_addresses_by_its_canonical_decimal_and_replays` | § 7.3.2 |
| `a_binary64_identity_addresses_by_its_canonical_text_and_negative_zero_is_one_address` | § 7.3.2, the row the previous draft had no spelling for |
| `a_binary64_identity_whose_value_is_not_finite_is_refused_at_the_field_kind` | § 7.3.2, the admitted domain |
| `a_struct_identity_addresses_by_canonical_json_with_sorted_keys_and_normalized_numbers` | § 7.3.3 |
| `a_composite_identity_containing_a_binary64_member_addresses_recursively` | § 7.3.3, the recursion |
| `a_text_identity_addresses_with_the_s_prefix_and_an_empty_string_identity_is_admitted` | § 7.3.1, `""` → `s:`; the address is never empty |
| `every_kernel_1_fixture_id_and_ref_value_is_byte_identical_after_this_contract` | § 7.3.1, § 1.1 — the separate claim, over the committed fixtures |
| `a_relation_carrier_and_an_event_payload_publish_the_logical_identity_not_the_address` | § 7.3.1, § 7.5, § 8.2 |
| `two_numeric_identity_spellings_of_one_value_are_one_address` | § 7.3.2, `1` / `1.0` / `1e0`, and `-0.0` / `0` |
| `identity_equality_and_the_address_agree_for_every_numeric_identity_kind` | § 7.3.2, the stated equivalence, over both representation arms |
| `an_identity_field_that_stops_mirroring_the_id_after_set_is_refused` | § 7.4 `IdentityMismatch`, at step 11 |
| `an_owns_relation_carrier_is_typed_as_the_owners_identity_kind_not_always_as_a_ref` | § 8.1 |
| `an_owns_relation_is_validated_against_the_targets_field_and_not_the_declarers` | § 8.1 |
| `a_many_owns_relation_carried_by_an_array_field_is_refused` | § 8.1 |
| `a_references_many_relation_requires_an_array_of_the_targets_identity_kind` | § 8.1 |
| `an_optional_references_one_carrier_lowers_to_required_false_and_registers` | § 8.1, the source-admitted shape the previous draft refused |
| `a_non_optional_references_one_carrier_lowers_to_required_true` | § 8.1 |
| `an_optional_owns_or_references_many_carrier_is_refused` | § 2.1 `RelationCarrierOptionality` |
| `every_references_row_compares_its_carrier_against_the_targets_identity_kind` | § 8.1, § 8.2: both `References` rows, scalar, named and composite identities, matching and mismatching |
| `a_list_identity_is_carried_by_a_list_on_every_row` | § 8.1's worked `List<String>` table: `Owns` at both cardinalities, `References`/`One`, and `References`/`Many` as an array of arrays |
| `a_references_many_carrier_of_a_list_identity_is_one_array_deeper_than_the_identity` | § 8.1: the outer array is the cardinality, so one array of `String` is the wrong type for a `List<String>` identity |
| `an_owns_carrier_is_the_owners_identity_once_on_both_cardinalities` | § 8.1: `carried_types`' single `(Owns, _)` arm, and the array refusal that survives for a scalar owner identity |
| `an_unset_optional_reference_answers_unknown_rather_than_false` | § 8.1, absence versus null |
| `a_second_owner_of_one_entity_is_refused_by_validate_all` | § 8.2 |
| `one_field_carrying_two_relations_is_refused_by_validate_all` | § 8.2 |
| `a_map_field_types_its_values_and_refuses_an_untyped_member` | § 10.1 |
| `a_map_with_an_integer_key_refuses_a_key_that_is_not_integer_text` | § 10.1 |
| `a_union_field_accepts_the_adjacent_tagged_form_and_refuses_an_unknown_tag` | § 10.1 |
| `a_union_tagged_value_reads_its_content_under_content` | § 10.1, the derived content key |
| `the_billing_invoice_entity_lowers_with_every_field_typed_and_none_refused` | § 10.1, the acceptance fixture's precondition |
| `a_binary64_predicate_is_admitted_and_negative_zero_equals_zero` | § 10.2, § 10.2.1 |
| `a_binary64_field_keeps_the_sign_of_its_zero_through_creation_event_and_replay` | § 10.2.1, the bytes |
| `an_exact_decimal_survives_creation_set_event_and_replay_unrounded` | § 10.2.1, the stored half |
| `a_service_integer_beyond_the_i64_range_is_refused_while_kernel_1_still_admits_it` | § 10.2 `IntegerBeyondSourceRange` |
| `a_decimal_binary64_does_not_carry_answers_ne_false_and_its_negation_true` | § 10.2.1, `1.0000000000000000001` against `1`, both directions |
| `that_same_token_is_stored_and_replayed_byte_for_byte_after_answering_equal` | § 10.2.1, parity in the answer and exactness in the bytes, in one test |
| `a_binary64_underflow_token_is_falsy_and_is_still_stored_unrounded` | § 10.2.1, `1e-400` in `truthy` |
| `two_adjacent_integers_past_2_53_are_distinguished_on_the_exact_integer_path` | § 10.2.1, `9007199254740993` against `9007199254740992` |
| `the_versioned_observation_matches_the_actual_source_cli_profile` | § 10.2.1: measured `946.3702156715110866946` CLI bits `408d92f633a24cdb`; default-feature library bits `408d92f633a24cda` are not conflated. Preserve separate wire/literal rules, signed zero, and refuse wire `1e400` without a panic |
| `a_reference_to_an_empty_logical_text_identity_preserves_the_declared_carrier` | § 8.1: a `string` relation carrier accepts the empty logical identity; binding derives storage address `s:`; legacy `ref` validation is unchanged |
| `service_numeric_equality_membership_and_bounds_all_use_the_observation_rule` | § 10.2.1, `eq`/`ne`/`in`/`contains`/`min`/`max` answer what `compare` answers |
| `kernel_1_number_operators_and_bounds_answer_exactly_what_they_answer_today` | § 10.2.1, the untouched half |
| `a_record_replays_under_the_number_observation_its_definition_snapshot_names` | § 10.2.1, § 2, the versioned rule |
| `a_timestamp_comparison_with_no_declared_scale_is_unknown_and_not_false` | § 10.3, the exact no-scale case |
| `a_timestamp_comparison_inside_a_declared_scale_answers_by_rank` | § 10.3 |
| `two_declared_scales_that_disagree_about_one_pair_answer_unknown` | § 10.3, `Scales::compare`'s ambiguity arm |
| `the_declared_scales_are_snapshotted_into_the_record_and_replay_reads_the_snapshot` | § 10.3, § 3 |
| `compare_answers_unknown_where_gt_answers_false_for_two_non_numbers` | § 10.4, the divergence that makes `compare` its own operator |
| `compare_over_a_resolved_array_operand_is_unknown` | § 10.4 `PathNotScalar` |
| `truthy_over_text_is_true_unless_empty_or_the_word_false` | § 10.4 |
| `truthy_over_a_number_is_the_zero_test_and_over_nothing_is_unknown` | § 10.4 |
| `for_all_over_an_empty_collection_is_true_and_for_any_is_false` | § 10.4 |
| `for_all_over_an_unobserved_collection_is_unknown_rather_than_vacuously_true` | § 10.4, the distinction the source writes a paragraph about |
| `a_nested_quantifier_body_reaches_the_outer_element_and_an_inner_binder_shadows_it` | § 10.4 |
| `a_quantifier_binder_named_after_a_fixed_root_shadows_it_inside_the_body` | § 2.2, § 10.4: `as: id` makes `$id` the element, which is `Element::rebind`'s any-matching-namespace rule |
| `a_quantifier_body_reads_the_fixed_roots_its_binder_does_not_name` | § 2.2, the other half: every address the binder does not match passes through to the enclosing scope |
| `for_all_over_a_map_walks_its_values_in_canonical_key_order` | § 10.4 |
| `a_quantifier_body_reading_an_address_outside_its_scope_is_refused_at_registration` | § 2.1 `QuantifierBodyScope` |
| `a_condition_nested_past_thirty_two_is_refused_with_its_limit` | § 2.1 `ConditionTooDeep` |
| `a_nominal_invariant_inside_a_list_lowers_to_a_for_all_over_that_path` | § 10.5 |
| `an_empty_any_of_is_unknown_when_unobserved_and_false_when_observed` | § 10.4 |
| `a_collection_count_resolves_under_service_1_and_resolves_to_nothing_under_kernel_1` | § 10.6 |
| `an_array_ordinal_address_resolves_under_service_1_only` | § 10.6 |
| `every_complete_branch_decision_replays_byte_for_byte_from_its_record` | `crates/entity-core/src/replay.rs:113-170` over a create-plus-branch history |

Fault sensitivity, applied and reverted (AGENTS.md § Conventions): swap the input guard and the state
test; test the selected branch's `from` instead of the union; swap two branches; make a bare `in_state`
guard fall through instead of selecting; drop one element from a `for_all` walk; make an unobserved
collection answer vacuously true; make `compare` answer `false` where it must answer `Unknown`; make a
`service/1` numeric comparison call `number::compare` instead of `Observed::cmp`; make `is_zero` test
the exact value instead of the carried binary64; round a stored token to what the observation read;
drop one event from a branch; change one `set` value; make a refusing branch emit; remove the identity
mirror check; drop the `s:` prefix from a text address; address a `binary64` identity with `f64`'s
`Display` instead of the canonical text; resolve a creation event payload in `CreateTemplate` instead
of `CreateOutcomeTemplate`; lower an `Optional` `References`/`One` carrier to `required: true`; move an
`Owns` `via` check back onto the declaring definition; move step 11 after step 12 or step 14 before
step 13. Each must fail its named test above. `tests/purity.rs` and the requirement pins must pass unchanged;
the implementation adds its rows to `docs/requirements.md` in the same commit, which is what
`scripts/check-requirements.py` enforces.

## 12. Implementation surfaces

| File | Change |
| --- | --- |
| `crates/entity-core/src/definition.rs` | `Semantics`, `NumberObservation`, `IdentityDefinition`, `RelationDefinition`, `OutcomeDefinition`, `OutcomeEffect`, `RefusalDefinition`, `MapKey`, `Quantifier`, `CompareOp`; `FieldKind::{Map, Union, Binary64}` and the `key`/`tag`/`variants` keys; `Condition::{Compare, Truthy, ForAll, ForAny}` and the four new entries in `CONDITION_OPERATORS`; new keys on `EntityDefinition` (`semantics`, `identity`, `relations`, `scales`, `number_observation`), `CreateDefinition`, `OperationDefinition` |
| `crates/entity-core/src/observed.rs` | **new.** `Observed`, its private `Repr`, `of_number`, `of_literal`, `cmp`, `is_zero`, `exact_text` — § 10.2.1. No public type beyond `Observed`; no trait, no registry, no configuration |
| `crates/entity-core/src/identity.rs` | **new.** typed, fallible `address(kind, value)` — § 7.3, total for admitted values; the kernel's mirror step and every binding call the same function, and an unvalidated caller receives a named error |
| `crates/entity-core/src/validation.rs` | the § 2.1 refusals; `ScopeKind::{OutcomeSelector, CreateSelector, CreateSet, CreateOutcomeTemplate}` and the binder extension, with `CreateTemplate` unchanged; `map`, `union` and `binary64` value validation; the `service/1` integer range and the `service/1` bounds path through `Observed`; identity and `References` relation checks including carrier optionality; the two collection address forms in `validate_reference_path` |
| `crates/entity-core/src/registry.rs` | the `Owns` carrier, second-owner, target and field-claimed-twice checks in `validate_all` |
| `crates/entity-core/src/runtime.rs` | input selection including the bare state guard and state admissibility, `move_sources`/`wrong_states`; `Evaluation`, `Refusal`, `decide`/`decide_create`; the four new operators in `evaluate_condition`, routing the `service/1` numeric rows through `Observed`; `scale_compare`; the `service/1` arms of `lookup`; `DecisionRecord.outcome`/`.effect`/`.response`; `DecisionCommand::Create.arguments`; creation from `create.arguments` and the `service/1` `TemplateContext.args`; the mirror step at step 11 |
| `crates/entity-core/src/error.rs` | `Refused`, `NoOutcomeSelected`, `OutcomeUnobservable`, `UnspecifiedMoveSource`, `IdentityMismatch`, and the new `DefinitionError` variants including `RelationCarrierOptionality` |
| `crates/entity-core/src/replay.rs` | `replay` re-selects the branch and compares `outcome`/`effect`/`response` and the identity address; the `Create { fields }` pattern at `:134`; `rehydrate` refuses a `service/1` definition **by name before reading any event** — event-only folding cannot see which branch ran, and a `service/1` definition has no legacy history to fold |
| `crates/entity-store/src/asynchronous/encoding.rs` | `er.record/2` and `er.request/2` selection by the record's `semantics`; the `Create { fields }` pattern at `:99` |
| `crates/entity-store/src/asynchronous/verify.rs` | the anchored verifier reruns through the same path; the `Create { fields }` patterns at `:91,108` |
| `crates/entity-executor/src/lib.rs` | the `Create { fields }` pattern at `:616` |
| `crates/entity-cli`, `crates/entity-surface`, `crates/entity-graph` | render the new keys, kinds and operators; no behaviour |
| `docs/design/kernel-v0.1.md` | § 6's step 0/1 numbering corrected to the code's order (§ 4.1) |
| `AGENTS.md` | invariant 8's "eleven-step" corrected to twelve (§ 4.1) |
| `docs/design/recorded-execution-encoding-v0.1.md` | the `er.record/2` / `er.request/2` shapes and their byte fixtures |
| `CHANGELOG.md`, `docs/requirements.md` | a user-visible line and the new pins |

Not in this contract: SQL, the Eventlog adapter, `ess-service-contract`, `ess-entity-runtime`, SDK `/4`
composition and every HTTP or persistence acceptance. Those consume this and are separately owned.
The SDK `/4` step is named rather than left implicit: `service-definition/4` and
`service-runtime-ir/4` are what carry `IdentitySupplied`'s address derivation (§ 7.3.1) and the
`service/1` obligations of § 9, and `/3` — which "accepts only the new `/3` definition and runtime
formats" (`SDK/README.md:222`) — cannot address a `service/1` entity, because it has no address
function. Opt-in, and nothing about it rewrites a `/3` artifact.

Also outside: a second `NumberObservation` variant. The source's filed decimal read-door stage
(`ESS/docs/design/review-primitive-semantics.md:312-320`) would be `source-number/2`, authored against
that stage when it lands, and § 10.2.1 is what stops it changing a `service/1` replay retroactively.

## 13. Component selection is a caller argument, not an open decision

`ess-service-contract::extract` takes the component as an explicit argument and returns
`ServiceDiagnostic::UnknownComponent` when the caller names one the IR does not declare
(`ESS/crates/specify/ess-service-contract/src/lib.rs:24-32,120-139`). There is nothing here for root to
settle: the ER target is built **per explicitly named component**, and the SDK
`ServiceDefinition.service` identity is never used to infer one.

What follows is determinate rather than chosen. One registry holds the entity closure of § 8.2, which
`include_entity_closure` computes from the selected component's owned domains
(`ESS/crates/specify/ess-service-contract/src/lib.rs:586-622`). The single-owner rule of § 8.2 and the
relation obligations of § 9 are scoped to that closure.

## 14. Coverage of the ten approved dimensions

Every row names where this contract settles it and the acceptance test that decides it.

| Dimension | Settled at | Decided by |
| --- | --- | --- |
| Creation | § 2 `create.arguments`/`create.outcomes`; § 2.3 the creation template scope carrying the arguments; § 5.2 `Creates` and zero-event creation; § 5.4 the creation event's `args`; § 6 `Create.arguments` in the record | `a_creation_branch_with_the_creates_effect_and_no_events_is_admitted_and_records_created`, `a_creation_branch_event_payload_reads_a_creation_argument_no_field_stores`, `a_creation_branch_response_reads_a_creation_argument_and_a_post_set_field`, `a_kernel_1_creation_that_emits_nothing_still_validates_and_still_creates`, `a_service_1_decision_frames_as_er_record_2_and_its_request_carries_arguments_not_fields` |
| Update | § 6 `OutcomeEffect::Updates`, no synthesized self-transition | `an_update_branch_keeps_its_state_and_records_updated_without_a_transition` |
| Transition | § 2 `OutcomeEffect::Moves { to, from }`; § 4.3 the union wrong-state rule and state admissibility; § 4.4 the one unanswered pair | `the_wrong_state_set_is_the_complement_of_every_move_source_not_of_the_selected_branch`, `a_selected_move_branch_whose_from_excludes_the_state_answers_the_wrong_state_branch`, `the_close_ticket_witness_registers_and_refuses_only_the_state_input_pair_the_source_leaves_open` |
| Predicate | § 10.3 the scale context, § 10.4 `compare`, `truthy`, `for_all`/`for_any`, § 10.6 collection addressing | `compare_answers_unknown_where_gt_answers_false_for_two_non_numbers`, `a_timestamp_comparison_with_no_declared_scale_is_unknown_and_not_false`, `truthy_over_a_number_is_the_zero_test_and_over_nothing_is_unknown`, `for_all_over_an_unobserved_collection_is_unknown_rather_than_vacuously_true` |
| Invariant | § 10.5, including the optional-field rewrite, the collection rewrites and the `union` variant path | `a_nominal_invariant_inside_a_list_lowers_to_a_for_all_over_that_path`, `the_billing_invoice_entity_lowers_with_every_field_typed_and_none_refused` |
| Identity | § 7 the logical typed identity, § 7.3's total address function over every admitted kind including `binary64`, the `s:` rule for text, the mirror step at step 11 and where the identity comes from | `an_integer_identity_addresses_by_its_canonical_decimal_and_replays`, `a_binary64_identity_addresses_by_its_canonical_text_and_negative_zero_is_one_address`, `a_composite_identity_containing_a_binary64_member_addresses_recursively`, `a_text_identity_addresses_with_the_s_prefix_and_an_empty_string_identity_is_admitted`, `every_kernel_1_fixture_id_and_ref_value_is_byte_identical_after_this_contract`, `an_identity_field_that_stops_mirroring_the_id_after_set_is_refused` |
| Relation | § 8.1 the carrier table over the related entity's identity kind, with `References`/`One` optionality carried across; § 8.2 the three enforcement layers | `an_owns_relation_carrier_is_typed_as_the_owners_identity_kind_not_always_as_a_ref`, `an_optional_references_one_carrier_lowers_to_required_false_and_registers`, `an_optional_owns_or_references_many_carrier_is_refused`, `an_owns_relation_is_validated_against_the_targets_field_and_not_the_declarers`, `a_second_owner_of_one_entity_is_refused_by_validate_all` |
| Exact value | § 10.1 `map`, `union` and `binary64`; § 10.2 the numeric domain and signed zero; § 10.2.1 `source-number/1`; § 10.3 the primitive table | `a_binary64_predicate_is_admitted_and_negative_zero_equals_zero`, `a_binary64_field_keeps_the_sign_of_its_zero_through_creation_event_and_replay`, `a_decimal_binary64_does_not_carry_answers_ne_false_and_its_negation_true`, `that_same_token_is_stored_and_replayed_byte_for_byte_after_answering_equal`, `a_binary64_underflow_token_is_falsy_and_is_still_stored_unrounded`, `two_adjacent_integers_past_2_53_are_distinguished_on_the_exact_integer_path`, `a_union_field_accepts_the_adjacent_tagged_form_and_refuses_an_unknown_tag` |
| Outcome | § 4.2 the numbered order; § 4.3 selection, including the bare state guard; § 5.1 refusal as a typed result; § 5.3 the declared response | `an_input_guard_answers_before_the_held_state_is_tested`, `a_matching_in_state_branch_with_no_when_is_taken`, `a_service_1_operation_runs_the_sixteen_steps_in_the_numbered_order`, `a_refusing_branch_produces_no_record_no_event_no_response_and_no_revision`, `a_declared_response_is_materialised_from_the_selected_branch_and_replays_byte_for_byte` |
| Event multiplicity | § 2 ordered `emits` on creation and operation alike; duplicates preserved | `a_creation_branch_emits_zero_one_or_many_events_in_declaration_order` |

## 15. What is decided here rather than read from source

Six, and each says what would change if root decided otherwise. Everything else in this document is
cited to a line of ESS or ER source, or to a witness in the evidence directory.

1. **Declared order is the tie-break between two `When` guards that both hold.** ESS proves unique
   selection only for a subject-state command and for a command with no default
   (§ 4.3's last paragraph); with a default present it proves nothing. ER needs a total rule, and
   declared order is the one `kernel/1` already applies to transitions. *If root prefers a refusal:* add
   a registration check that no two selector-bearing branches can both hold, which needs a finite guard
   analyser in `entity-core` and is a larger addition than this contract makes.

2. **A selected branch that cannot move from a state no `wrong_state` answers is a named run-time
   refusal, not a registration refusal and not a branch result.** The source admits the command
   (witness W2) and answers nothing for that one (state, input) pair. `UnspecifiedMoveSource` names the
   four facts and produces nothing durable. *If root prefers the previous draft's registration
   refusal:* `AmbiguousMoveSource` comes back and `witness.move.CloseTicket` — a specification the
   installed tool admits at exit 0 — is refused at registration, which is a smaller admitted set, not a
   different answer. *If root prefers a branch result:* it has to be the `wrong_state` branch, which
   contradicts the condition's own definition. § 4.4 is the whole argument and the witness.

3. **Elements of a `map` are quantified in canonical key order.** ESS quantifies a `Map` over its values
   and publishes no order for them. The truth of a `for_all` or `for_any` does not depend on the order;
   which unobserved addresses a refusal names does, because the fold short-circuits. *If root prefers no
   order:* the quantifier would have to stop short-circuiting over a map, which makes a map and an array
   behave differently for no stated reason.

4. **`er.record/2` and `er.request/2` rather than additive keys on `/1`.** The encoding document's own
   rule requires a new version domain for a shape change
   (`docs/design/recorded-execution-encoding-v0.1.md:4-5`), and a `service/1` record is one. *If root
   prefers `/1` with `skip_serializing_if`:* old `/1` bytes still do not move, but a closed `/1` reader
   refuses at the unknown record key rather than at the framing tag, which is a later and less legible
   refusal.

5. **A `service/1` text-like identity addresses as `"s:"` + its logical text.** The source admits an
   empty and a whitespace `String` identity and ER's storage address may not be empty or whitespace
   (`crates/entity-core/src/runtime.rs:264-269`), so a total injective rule needs a prefix. No accepted
   preservation constraint contradicts it: § 1.1's promise is over `kernel/1`, which cannot declare
   `identity` at all, and there is no stored `service/1` record to preserve. *If root prefers the
   identity function:* the empty and whitespace values come back as a refusal, which is a divergence
   one value wide against a source-admitted `String`, and § 7.4's `IdentityAddressEmpty` returns with
   it. The consequences of the choice taken are § 7.3.1's five bullets, including the opt-in SDK `/4`.

6. **`service/1` answers numeric predicates under `source-number/1` while storing the authored token.**
   The two readings — exact token, and the token as the source's own two doors observe it — disagree
   on a decimal binary64 does not carry, in **both** directions, and one of them has to be the answer.
   Reproducing the source's reading is what makes a lowered guard mean what it meant.
   *If root prefers the exact token as the answer:* `1.0000000000000000001 ne 1` is `true` in ER and
   `false` in ESS, and ER takes accepting branches ESS does not take — the finding this replaces.
   *If root prefers rounding the stored value to the observation:* the record no longer carries what
   the author wrote and `arbitrary_precision` buys nothing. § 10.2.1 is the rule and its four
   counterexamples.

An earlier draft named a seventh — which ESS component an ER target is built from — and § 13 shows the
extractor already answers it.

**Nothing in this contract is a lowering refusal of a supported source construct.** Every construct the
ESS source admits is represented: `Map<K,V>`, tagged unions, `Binary64`, the declared command response,
`ResolvedEffect::Creates`, zero-event creation, `Owns` relations, an `Optional` `References`/`One`
carrier, every identity type of witness W1 including an empty `String` and a `Binary64`, a bare
`SubjectState` guard with no input predicate, a creation payload reading an argument no field stores,
`Forall`/`Exists`, `Truthy` over text and numbers, ordered text comparison including `Timestamp`, and
every command `wrong_state` shape including witness W2's. What remains are three **named divergences in
the answer**, each with the source line that says the source is the one moving:

| divergence | direction | the source's own statement |
| --- | --- | --- |
| a `Timestamp` has no chronological order | both sides agree, and both are wrong about instants | `Primitive::Timestamp` publishes a `format` and no pattern (`ESS/crates/generate/ess-gen/src/types.rs:513`) and compares as `Text` (`ESS/crates/specify/ess-domain/src/expression.rs:32-36`) |
| an outcome's `emits` with no error | ER admits where ESS's `EmptyChange` refuses | required by `kernel/1` byte preservation; ESS's stricter rule stays on the ESS side (§ 5.2) |
| one (state, input) pair of a `When`/`Otherwise` command whose selected branch cannot move | ER refuses by name; ESS answers nothing at all | the six paths of § 4.4, each read and each silent |

Two rows the previous draft carried are **gone rather than reworded**: the decimal read-door row,
because § 10.2.1 makes ER answer what ESS answers; and the empty-string identity row, because
§ 7.3.1 makes ER admit what ESS admits.

## 16. ESS typed coordinate model

`ess/service-semantics/system.yaml` and `domains/service.yaml` give the new nouns of § 2, § 4, § 5,
§ 7, § 8 and § 10 — outcome, selector, effect, evaluation step, refusal result, declared response,
template scope, source-number observation, binding obligation,
relation carrier, logical identity and its address, numeric domain, condition operator, quantifier and
scale — a typed home, authored from the decisions above rather than imported from a wire format, in the
same shape as `ess/recorded-execution/` (`docs/design/recorded-execution-v0.1.md:159-165`). It is a
coordinate model: the complete definition and record payloads remain the existing concrete Rust types,
and model validation authorizes no lossy projection of them.
