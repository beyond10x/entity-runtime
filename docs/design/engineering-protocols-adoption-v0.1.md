# Driving engineering-protocols — design v0.1

**Status: proposed.** How `engineering-protocols`' artifact model would be expressed as entity
definitions and executed by this kernel, what that would fix, and what has to change here first.
Nothing in this document is accepted for either repository until a plan page or a story in
`engineering-protocols` accepts it (their `AGENTS.md` § *Which documents are normative*). This
repository's side of the work is tracked in its own planning store under
`epic:drive-engineering-protocols`.

## 1. The claim

`engineering-protocols` already models its planning artifacts the way this kernel models entities —
typed things with a lifecycle, legal moves, relations and events — but it does so **in Rust, with
closed enums**, and it ships the data-shaped parts as advisory YAML beside the code. Concretely
(all paths in `engineering-protocols` at `79b641c`):

| in `engineering-protocols` today | where | consequence recorded there |
|---|---|---|
| `ArtifactStatus` is a closed ten-variant enum | `crates/aep-domain/src/artifact.rs:707` | `docs/plan/gap-register.md:70` — "the status vocabulary could not hold [`correction-owed`]"; `expired`/`failed`/`blocked` "flatten onto rungs that mean something else" |
| `protocol artifact move` consults a `LifecycleRegistry` and nothing else | `crates/aep-domain/src/artifact.rs:1496`, `crates/aep-backend-markdown/src/document.rs:115-142` | gap register :39 — "a story's `implemented` is a claim nothing checks" |
| commands are hand-written `CommandKind` variants (`aep.entity.create/v1`, `aep.entity.archive/v1`, …) | `crates/aep-domain/src/command.rs:104-112` | adding a kind-specific operation is a Rust change and a release |
| lifecycles, kinds and relations are YAML files the validator does not fully read | `artifacts/lifecycles/*.yaml`, `artifacts/kinds/*.yaml`, `artifacts/relations/relations.yaml` (header: "advisory until the artifact validator reads these files") | rules written down twice, enforced once |
| four lifecycle concepts the protocol cannot express: a decision with a default and an expiry, time-based transitions, a typed blocker | gap register :73 | hand-rolled in scripts `explain` cannot see |
| custom kinds cannot share a lifecycle ladder (`parent()` is over built-ins only) | gap register :77 | adopters copy ladders |

Every row is a place where a *definition* — schema, open state vocabulary, operations with
preconditions, invariants, events — would carry what a Rust enum carries now. That is what "drive"
means here: the artifact model becomes data this kernel executes, and `engineering-protocols` keeps
what is genuinely its own — the evidence model, three-valued predicates, capabilities, the driver.

## 2. The mapping

| AEP concept | entity-runtime concept | note |
|---|---|---|
| entity type `aep.design/v1` (`entity.rs` module doc) | `EntityDefinition { entity: "design", version: 1 }` | identity by `(entity, version)`, as already |
| `EntityId` opaque, ≥ 12 chars (`entity.rs:52`) | `EntityInstance::id`, opaque to the kernel | the length rule would be the shell's; the kernel never parses an id |
| `ArtifactStatus` (closed enum) | `lifecycle.states` per definition | `correction-owed` becomes a line in a YAML file |
| `artifacts/lifecycles/story.yaml` transitions map | one operation per edge, e.g. `propose: draft → proposed`, `activate: proposed → active`, `implement: active → implemented` | an edge gains arguments, preconditions and events; `protocol artifact move --to implemented` becomes `execute --operation implement` |
| `protocol artifact move` (status only) | an operation with `preconditions` | *`implemented` requires evidence* is `preconditions: [{ exists: $args.evidence_ref }]` — or, with three-valued rules (§ 4), a predicate over facts |
| `aep.entity.archive/v1`, `aep.entity.supersede/v1`, **no delete** (`command.rs:706-710`) | terminal states `archived`, `superseded`; no operation leaves them; no delete exists to call | R-34 makes the absence structural |
| `DomainEvent` with correlation/causation (`domain_event.rs`) | `DomainEvent` (fact) + the shell's envelope | the split is already how `domain_event.rs` argues it: "an event is not an audit record" |
| a denied command → audit record, **no event** (`domain_event.rs` table) | `Err(CoreError)` → shell records the refusal; no `Decision`, no events (R-04) | identical contract |
| `Raw*` → validated via `TryFrom`, accumulate (`AGENTS.md` invariants 2, 3) | parse → `Registry::register` validates; value validation accumulates (R-23) | definition validation stops at the first defect today — `story:accumulating-definition-validation` |
| clock-free, RNG-free domain; `BTreeMap` only (invariants 8, 9) | R-01, R-05, pinned by `tests/purity.rs` | same discipline, same kind of scan |
| `artifacts/kinds/*.yaml` `required_sections` | schema fields with `required: true` on the artifact's body model | when the body is modelled as fields; a first step keeps `body` as `json` |
| `artifacts/relations/relations.yaml` source/target pairings | typed references (roadmap: `type: ref`) | not in 0.1; the shell keeps validating edges until it is |

## 3. Phases

Each phase is a story here and would be a story there; none starts until the operator says so.

| phase | what | evidence of done |
|---|---|---|
| 0 | this document; the mapping is reviewed by both repositories | accepted or refused, with the reason, on a plan page in `engineering-protocols` |
| 1 | the eight `artifacts/lifecycles/*.yaml` re-expressed as eight definitions under `examples/aep/`, with one operation per edge and no rules | an equivalence test: for every kind, the set of `(from, operation, to)` edges the definition yields equals the transitions map in the YAML at the pinned commit; `entity validate examples/aep/*.yaml` exit 0 |
| 2 | `protocol artifact move` evaluated by this kernel behind the existing CLI, refusing what it refuses today and nothing more | the markdown store's status moves produce identical accept/refuse verdicts on the planning stores of `engineering-protocols` and `agentic-principles` (98 + 6 artifacts on 2026-08-25) |
| 3 | preconditions on `implement` and `accept`: evidence must be present | gap register :39 closes with a mechanism, not a verdict |
| 4 | open status vocabulary: `correction-owed` and friends added as data | gap register :70 closes without a Rust change |

Phases 2–4 need § 4 first.

## 4. What must change here before phase 2

**Three-valued rules — done.** `engineering-protocols` invariant 5: *`Unknown` is not `False`* — a
fact nobody observed reads `?`, never `✗`, and only `True` permits a transition. This kernel's rules
were two-valued: a missing reference made a comparison `false` (R-54 as it read then). For a
lifecycle ladder that is harmless; for *`implemented` requires evidence* it is wrong in exactly the
way that invariant exists to prevent — "nobody has looked lately" refused with the same message as
"it is broken".

Shipped as R-57 and R-58, and specified in [`kernel-v0.1.md` § 4.1](kernel-v0.1.md#41-three-values-and-which-questions-can-have-them):
a `Truth { True, False, Unknown }` result with Kleene `all`/`any`/`not`; a rule holds only when
`True`; and the refusal distinguishes the two — `PreconditionFailed` against a new
`PreconditionUnobservable`, which carries every unresolved address rather than the first.

`Unknown` is confined to questions about a **value** — the comparisons — where a reference that
resolves to nothing, including a key **present and null**, leaves nothing to read. `exists` asks
about the **store** and stays two-valued (R-58): the kernel holds the instance, so it can always
see whether a key carries a value. That is the split `engineering-protocols` already makes without
naming it — its predicate language has six comparison operators and no presence operator, and its
only candidate-shaped `Unknown` is `ValueAbsent`. R-54's short-circuit clause was revised with it;
the old wording is quoted in the register. `story:three-valued-conditions`.

The `Truth` type is taken from `aep-domain::predicate::Truth` rather than designed here — same
variant names, same Kleene tables, same *only `True` satisfies*. Two kernels that disagreed about
what `Unknown` means would disagree about whether a gate passed, and this seam is exactly what
phase 2 runs through.

This was the one change in this list that alters kernel semantics; the others add.

**Accumulating definition validation.** Invariant 3 there; R-13 refuses correctly but reports one
defect per attempt. `story:accumulating-definition-validation`.

**Typed references.** Relations are the artifact graph's edges; the kernel has no `ref` kind. Until
it does, the shell keeps validating edges as `protocol artifact relate` does today.
`story:typed-references`.

## 5. Boundaries that hold whatever happens

* **Vocabulary crosses; a dependency is a decision.** Both repositories are public. No
  `Cargo.toml` in either names the other until an ADR in `atlas` says which way the arrow points —
  a kernel that depends on its adopter, or an adopter that vendors its kernel, are both wrong
  shapes and the ADR is where that is argued.
* **Wire-visible identifiers are a coordinated migration.** `aep.entity.archive/v1` is verified by
  adopters. If phase 2 changes what a status move is called on the wire, that is an ADR in `atlas`
  naming every relying party, not an edit (atlas `AGENTS.md` § *Cross-repo changes*).
* **The kernel stays IO-free through all of it.** Reading a planning store, resolving a `git+ssh`
  protocol source, checking an evidence horizon against a clock — all shell. R-01 does not bend for
  an adopter, however important.
* **Nothing adopter-internal is written here.** The planning stores named in phase 2 are the org's
  own; a third party's store is evidence the operator holds, not a fixture in this tree.
