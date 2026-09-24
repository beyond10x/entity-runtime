---
format: aep.planning-md/2
id: review-result:service-binding-boundary-design-pass-1
kind: review-result
status: active
title: Service binding boundary complete design review pass 1
relations:
- reviews: task:service-binding-boundary
- reviews: story:service-binding-boundary
revision: 1
---
needs-revision

Covered proposal: `docs/design/service-binding-boundary-v0.1.md` at
`sha256:29f3320de12b56ab1bf3177bc1f227e1619611865f818c9962108bacb5d267cd`,
`docs/design/models/service-binding-boundary/system.yaml` at
`sha256:3354ec4adc6dfba82e280dd8af6ef4c151661eefa55f08c0d419bec803978c95`, and
`docs/design/models/service-binding-boundary/domains/contract.yaml` at
`sha256:9743dae6c6c7693b813f73df711976df4dcd0d396df70cbae2a9f18fdb07dc24`,
against Entity Runtime `24d31cf1f97a3744db7e65c5629e056bc7a3a241` and the corrected ESS
lowerer proposal at `sha256:c937aa9f039016a77e9448d8860980d554512da8d2b1aece42a48ce247a1a528`.

## Blockers

### 1. The public preload API has no valid path for implicit-branch operations

**Contract.** `decide_before_load` is public over any `ValidatedDefinition`, and preparation scans
the operation's non-`wrong_state` outcomes before returning `Refused`, `Load`, or a selection error
(`docs/design/service-binding-boundary-v0.1.md:102` and `:144`). Reaching the end returns
`NoOutcomeSelected` (`:172`). The contract gives no semantics restriction or empty-outcome case.

**Accepted behavior.** Direct `decide` uses named selection only for a service definition with a
nonempty outcome list. Every `kernel/1` operation and every admitted branchless `service/1`
operation instead uses the existing implicit transition branch
(`crates/entity-core/src/runtime.rs:764-810`). Branchless `service/1` is an intentional compatibility
shape, not malformed input; the accepted framing suite exercises its creation analogue and requires
the four readers to agree (`crates/entity-store/tests/service_framing.rs:202-333`). `service/2`
inherits this `service/1` behavior under the proposal.

**Counterexample.** Call `decide_before_load` for any valid `kernel/1` operation with a matching
transition, or a branchless `service/1` operation. The proposed scan sees an empty outcome slice and
returns `NoOutcomeSelected` before loading. Direct `decide` loads/selects the implicit transition and
can accept. This is a new behavior change at the public API, and the named compatibility test at
`:493` does not state that these definitions must return `Load` and continue through their implicit
branch.

**Bounded correction.** After operation lookup and argument normalization, return a prepared `Load`
for every operation whose direct path uses the implicit branch, and have the continuation execute
that same branch. Add preload/continuation equivalence cases for `kernel/1`, branchless `service/1`,
and branchless `service/2`, while leaving direct `decide` and their existing record bytes unchanged.

### 2. Syntactic dependency detection changes Kleene-decidable refusals into subject loads

**Contract.** A guard containing any `$fields` or `$from_state` reference immediately returns
`Load`; only a guard limited to pre-load roots is evaluated (`docs/design/service-binding-boundary-v0.1.md:162-167`).
The dependency walk descends through the complete condition AST (`:174-178`).

**Accepted behavior.** The existing evaluator deliberately evaluates all `all`/`any` operands and
combines them with Kleene truth (`crates/entity-core/src/runtime.rs:1259-1295`). `False` dominates a
conjunction and `True` dominates a disjunction (`crates/entity-core/src/truth.rs:37-57`). Existing
tests pin both exact mixed cases: `all: [false, <missing field comparison>]` is `False`, and
`any: [true, <missing field comparison>]` is `True`
(`crates/entity-core/tests/requirements.rs:1430-1446`). Outcome-selector registration admits
`$fields` under this same closed condition language (`crates/entity-core/src/validation.rs:849-850`).

**Counterexample.** A first refusing outcome guarded by
`any: [true, {eq: ["$fields.note", "x"]}]` is selected for every valid instance; direct `decide`
therefore returns that refusal regardless of the subject's fields. The proposed dependency walk sees
the field reference and returns `Load`. For an absent subject the SDK then returns unknown-subject
instead of the branch's refusal. The dual `all: [false, ...]` case can wrongly stop before a later
default refusal. Thus the new path does not preserve the accepted truth table or the promised rule
that a load is required only when subject facts are needed.

**Bounded correction.** Specify a partial condition evaluation that carries subject dependency
through the existing Kleene operators and quantifiers, allowing dominating `False`/`True` and empty
quantifications to settle a guard while returning `Load` only when the final selector answer still
depends on subject facts. Add both dominating-connective cases, including an unknown subject, to the
preload tests and a mutant that replaces the partial result with the current syntactic walk.

## Warning

### 3. One promised definition error has no triggering rule

The registration contract says conditional maps under `kernel/1` or `service/1` return the existing
`SemanticsKeyNotAvailable` at the exact key (`docs/design/service-binding-boundary-v0.1.md:281-287`),
then declares `ConditionalPresenceUnavailable` among the five new accumulated errors (`:289-297`).
The remaining admission rules assign the other four variants, but none assigns this one. A public
closed error variant with no defined trigger leaves stable matching and negative tests ambiguous.
Remove it or give it one condition distinct from `SemanticsKeyNotAvailable` and pin that condition's
exact path and precedence.

## Examination boundary

Read the three frozen deliverables, original brief and READY handoff, lowerer review 1 and correction
1, the corrected lowerer proposal, accepted Entity Runtime definition/validation/runtime/truth/replay
and store/executor framing paths, and the real billing and gatepass specifications and billing
realization/conformance boundary. The conditional-presence shapes, absent/present/null distinction,
shared-slot rule, creation-only state insertion, event/response materialization, replay comparison,
`service/2` plus `/3` version split, `/1` and `/2` byte preservation, batch framing, actual
PayInvoice ordering, and actual `note`/`issued_at`/`badge` fixture obligations yielded no further
finite finding.

No Cargo command, test, source/planning/model edit, live system, credential, external integration, or
scope extension was used. Repository and proposal trees remained read-only. The sole write was this
authorized report path. No additional fact remains for the coordinator to settle before correction.

```findings
- file: docs/design/service-binding-boundary-v0.1.md
  line: 172
  category: design
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: The preload scan has no implicit-branch case, so every kernel/1 operation and every branchless service/1 or service/2 operation reaches NoOutcomeSelected instead of returning Load and continuing through the branch that direct decide already executes.
- file: docs/design/service-binding-boundary-v0.1.md
  line: 162
  category: design
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: Returning Load for any syntactic subject reference disagrees with the accepted Kleene evaluator when False or True dominates that reference, so a guard whose answer is subject-independent can change a reachable refusal into unknown-subject.
- file: docs/design/service-binding-boundary-v0.1.md
  line: 292
  category: design
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: ConditionalPresenceUnavailable is declared as a new stable definition error but no admission rule can return it because wrong-semantics conditional maps are explicitly assigned SemanticsKeyNotAvailable, so the contract must remove the dead variant or define its distinct trigger and precedence.
```
