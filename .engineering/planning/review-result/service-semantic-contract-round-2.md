---
format: aep.planning-md/2
id: review-result:service-semantic-contract-round-2
kind: review-result
status: active
title: Service semantic contract final independent review, round 2
relations:
- reviews: task:service-semantic-contract
revision: 1
---
needs-revision

Path convention: unprefixed paths are repository-relative in the ER proposal tree
(`entity-runtime/ess-evolution-service-semantics-20260916`); `ESS/` is repository-relative in the ESS
semantic-source tree (`ess/ess-evolution-scope-20260915`). Line numbers are this session's reads.

## Findings

docs/design/service-semantics-v0.1.md — § 7.3 calls `address(v)` a total function but its table has no row for the `binary64` kind, while § 7.4 admits every identity kind except `json` and the model enumerates `Binary64` as an `IdentityValueKind` against five `AddressRule` variants, so a `Binary64`-keyed entity has a declared identity kind and no address spelling — docs/design/service-semantics-v0.1.md:627

docs/design/service-semantics-v0.1.md — § 2.2 adds `OutcomeSelector`, `CreateSelector` and `CreateSet` and leaves a creation branch's `emits` payload on the existing `CreateTemplate` scope, which carries `args: None` and admits no `$args`, so a `service/1` creation cannot publish a creation argument that is not also written to a field — `billing.invoice.CreateInvoice`'s `accepted` branch puts `customer_email: input.customer_email` in the event payload and not in `sets` — docs/design/service-semantics-v0.1.md:244

docs/design/service-semantics-v0.1.md — § 8.1's `References`/`One` row lowers the carrier to a field that is `required` "either way", but ESS's `carried_types` admits `Optional<target identity>` for that row, so an entity whose reference is not yet set is admitted by the source and refused by the ER schema, and § 2.1's `RelationViaWrongShape` refuses the `required: false` spelling a lowerer would need — ESS/crates/specify/ess-domain/src/entity.rs:1399

docs/design/service-semantics-v0.1.md — § 4.3's input selection takes "the first branch whose `when` holds" and names a branch with neither `when` nor `in_state` as the default, and says nothing about a branch whose `in_state` matches and which declares no `when`, so the shape ESS spells `SubjectState { state, predicate: None }` is unselectable and answers `NoOutcomeSelected` — docs/design/service-semantics-v0.1.md:326

docs/design/service-semantics-v0.1.md — § 10.2 bounds the decimal divergence as one-directional, "it refuses a command ESS would permit rather than permitting one ESS would refuse", which a negated or `ne` guard reverses: with `amount` authored `1.0000000000000000001`, ESS's `parse_decimal` collapses it to `1` and answers `ne 1` false, ER compares the token exactly and answers true, so ER takes an accepting branch ESS does not take — ESS/crates/specify/ess-primitives/src/facts.rs:243

docs/design/service-semantics-v0.1.md — § 4.2's evaluation-order list carries no numbers while § 2.2 and § 5.3 place the response at "step 12" and § 7.4 places the identity mirror at "step 13", and the list puts the identity mirror before the response, so no numbering makes both cross-references true — docs/design/service-semantics-v0.1.md:288

## Round-1's nine findings, checked against source

All nine are addressed in the current proposal. Seven are corrected outright; two carry a residual
defect listed above.

| # | round-1 finding | disposition | checked against |
| --- | --- | --- | --- |
| 1 | `DecisionCommand::Create { fields }` says nothing for a `service/1` creation | corrected — `arguments` added beside `fields`, framing moved to `er.record/2`/`er.request/2`, `er.batch/1` unmoved | `crates/entity-store/src/asynchronous/encoding.rs:60-140`, `docs/design/recorded-execution-encoding-v0.1.md:4-5` |
| 2 | `Map<K,V>` and union refused, so `billing.invoice.Invoice` cannot lower | corrected — `FieldKind::{Map, Union}`, adjacent tagging, derived content key | `ESS/examples/billing/domains/invoice.yaml:44-49,112-121` |
| 3 | `wrong_state` taken without evaluating a selector | corrected — the wrong-state set is the complement of the union of move sources, which is what the IR computes | `ESS/crates/specify/ess-compiler/src/ir.rs:1719-1743`, `ESS/crates/specify/ess-domain/src/entity.rs:287-300` |
| 4 | `Owns` `via` is on the target; a `Many` `Owns` carries a scalar | corrected — carrier table copied from `carried_types`, `Owns` checks moved to `Registry::validate_all` | `ESS/crates/specify/ess-domain/src/entity.rs:1245-1248,1392-1407`; residual: the `References`/`One` row, finding 3 above |
| 5 | no `Creates` variant; zero-event creation refused | corrected — `OutcomeEffect::Creates`, `UnobservableOutcome` weakened with the accepting-`wrong_state` exemption | `ESS/crates/specify/ess-domain/src/command.rs:1246-1266` |
| 6 | no representation for `ResolvedCommand.response` | corrected in shape, value, completeness, record and replay; residual: the create-side value scope, finding 2 above | `ESS/crates/specify/ess-compiler/src/ir.rs:810-812` |
| 7 | `OutcomeSelector` defined by subtraction and admitting `$to_state` | corrected — three scopes enumerated positively, `$to_state` excluded | `crates/entity-core/src/validation.rs:266-327` |
| 8 | § 13 presented component selection as an open decision | corrected — the extractor's caller argument is the answer | `ESS/crates/specify/ess-service-contract/src/lib.rs:24-32,120-139` (author's citation; not re-read this session) |
| 9 | "eleven steps" against `kernel-v0.1.md`'s twelve | corrected, and the code-order claim holds: `ensure_instance_matches` checks `EntityMismatch` before `UnknownState` while the design document numbers them the other way | `crates/entity-core/src/runtime.rs:831-850` against `docs/design/kernel-v0.1.md:345-346`; residual: finding 6 above |

Both of round-1's unresolved questions are resolved correctly. An ESS `Timestamp` is `ScalarKind::Text`
(`ESS/crates/specify/ess-domain/src/expression.rs:28-38`), and the scale sentence does continue past
"an ESS specification declares none" to "until `InputFacts::with_scales` supplies one"
(`ESS/crates/verify/ess-conformance/src/decision.rs:191-196`), so treating the scale set as declared
context with `Truth::Unknown` for the empty case is the source's reading and not an invention. The
position-independent `Otherwise` is handled as a spelling rule, and `AmbiguousDefaultOutcome` enforces
it.

## The root's six concerns, answered

**1. Is any round-1 finding left as an unadmitted narrowed subset?** No. The document's § 15 claims
"nothing in this contract is a lowering refusal of a supported source construct", and I found no
construct-level refusal that survives. The two narrowings I did find are not declared as refusals and
are not in § 15's divergence table: the `References`/`One` optional carrier (finding 3) and the
unselectable bare state guard (finding 4).

**2. Source-supported quantifiers, nested invariants, Binary64, truthiness, three-valued comparison,
every identity, the wrong-state rule.** Concretely implemented, and the source readings check out:

* `for_all`/`for_any` reproduce `Quantified::evaluate` including the unobserved-versus-empty split and
  the short-circuit (`ESS/crates/specify/ess-primitives/src/predicate.rs:393-421`), and the binder
  rewrite and inner-shadowing rule match `Element::rebind` (`:435-445`).
* Quantifying a `Map` over its **values** is right: the checker's shape for `TypeRef::Map(_, value)` is
  `Shape::Map(value)` (`ESS/crates/specify/ess-domain/src/expression.rs:232,698-701`).
* `<collection>.count` and `<array>.<n>` match the source exactly, including that an ordinal is
  admitted for a `List` only and that a `Map`'s keys are unaddressable
  (`ESS/crates/specify/ess-domain/src/expression.rs:456-478,497-502`).
* The `compare` table is `Predicate::evaluate_compare` row for row
  (`ESS/crates/specify/ess-primitives/src/predicate.rs:540-584`), and the `Unknown`-versus-`false`
  reason for making it a separate operator is real.
* `truthy` reproduces `FactValue::is_truthy`'s three arms
  (`ESS/crates/specify/ess-primitives/src/facts.rs:631-637`).
* **Binary64 is correctly admitted.** The root's caution is well placed and the document survives it:
  `Number` carries an exact `units × 10⁻ˢᶜᵃˡᵉ` beside the binary64 and `Ord::cmp` uses the exact value,
  falling back to `total_cmp` only where no exact value exists
  (`ESS/crates/specify/ess-primitives/src/facts.rs:45-67,486-491`), and `-0.0 == 0.0` is stated there
  in the source's own words. ER's `number::compare` answers the same and its test pins `("-0.0","0")`
  (`crates/entity-core/src/number.rs:8-55,173`). The previous rounds' refusal was reasoning from a
  `Number(f64)` that no longer exists.
* **But the numeric agreement is not total, and the document's bound on the gap is wrong** — finding 5.
  `parse_decimal` keeps an authored decimal only when it is an integer or equals the canonical decimal
  of its binary64 (`ESS/crates/specify/ess-primitives/src/facts.rs:243-253`); everything else collapses.
  `is_truthy`'s `number.get() != 0.0` is safe **inside ESS**, because a `Repr::Exact` carries a zero
  binary64 only when its exact value is zero (`Repr::exact`'s postconditions, `:256-279`). It is not
  safe across the boundary: ER holds the token, so a token that underflows binary64 is truthy in ER and
  falsy in ESS. § 10.4 names that class; § 10.2 mis-states its direction.
* The union wrong-state rule and the `UnspecifiedMoveSource` gap are argued from source and the
  argument holds for the two validations the document names — `validate_move` runs only for a command
  that uses `SubjectState` (`ESS/crates/specify/ess-domain/src/command/subject_state.rs:12-17,69,201`)
  and `validate_wrong_state_is_reachable` only asks whether `wrong_states` is non-empty
  (`ESS/crates/specify/ess-domain/src/entity.rs:1042-1102`). Witness W2 is a real model and the
  archived `ess specify validate` output shows both halves. I did not read the scenario synthesizer, so
  see *What I could not establish*.

**3. Finite, well-defined operators and types.** Yes. `FieldKind` gains three closed variants, `MapKey`
is an eight-variant enum taken from ESS's own key projection, the union layout is adjacent with a
derived content key, `CONDITION_OPERATORS` gains exactly four names, the binder is one path segment,
depth is capped at 32, and no registry, facet or untyped bag appears anywhere. Empty, missing and null
are each given an answer. The one place the type surface is incomplete is the identity address function
— finding 1.

**4. Old meanings and canonical bytes.** The version axes are separated correctly and the framing
change is the encoding document's own rule rather than a choice. Old-reader rejection has two
mechanisms and both exist today. The creation event's `args` change of meaning is found, named, and
walled off by refusing a `service/1` definition in `rehydrate` before any event is read. The
idempotency payload is not discussed by name, and it does not need to be: retries compare
`record_comparison_bytes` / `original_request_comparison_bytes`
(`crates/entity-store/src/asynchronous/memory.rs:200-207`, `.../types.rs:408`), both of which are
framed from the record, so a `service/1` retry compares `/2` bytes against `/2` bytes and a
cross-framing retry is a mismatch rather than a silent match.

**5. Identity, address, relations, outcomes.** The logical-identity/storage-address split is the right
shape and witness W1 is a real measurement rather than an inference. The collision argument is sound
within a fixed kind, and "two rows can collide and cannot meet" is stated rather than left implicit.
The gap is that the table does not cover every kind it admits — finding 1. The empty-string divergence
is one value wide, named in § 7.4 and again in § 15's table; I found no source rule obliging ER to
admit it, so it is a stated divergence rather than a narrowing. Outcomes are selected by the kernel
from evidence, `Updates` synthesizes no self-transition, and event multiplicity is ordered and
unbounded.

**6. Can the tests expose losses?** Mostly. § 11's 76 cases and 13 fault mutations do assert variants,
do cover fields, outcomes, responses, guards, values, invariants, references and ordering, and are
runnable without Eventlog or a network. Three of the six findings above have no test that would catch
them, which is the same statement as the finding: a `binary64` identity, a creation payload reading an
argument, and an optional `References` carrier are each absent from § 11.

## What I read

The three deliverables in full (`docs/design/service-semantics-v0.1.md`, 1398 lines;
`ess/service-semantics/system.yaml`; `ess/service-semantics/domains/service.yaml`), the critic rubric,
`semantic-contract-review-1.md`, the original author brief, the correction-1 brief and handoff, the
completion brief and handoff, `completion/hashes.txt`, `completion/ess-specify-validate.txt` and the
`witnesses/witness-move/` model. In ER: `crates/entity-core/src/{runtime,validation,number}.rs`,
`crates/entity-store/src/asynchronous/encoding.rs`, `crates/entity-store/src/envelope.rs`,
`docs/design/kernel-v0.1.md` § 6. In ESS: `crates/specify/ess-primitives/src/{facts,predicate}.rs`,
`crates/specify/ess-domain/src/{expression,entity}.rs`, `crates/specify/ess-compiler/src/ir.rs`,
`crates/verify/ess-conformance/src/{decision,reference}.rs`, `examples/billing/domains/invoice.yaml`.
Read tools and content search only: no shell, no build, no test, no network, no source or planning
edit, no lease, and no write outside this file.

## What I could not establish

* No shell in this session, so I did not recompute the three sha256 digests in
  `completion/hashes.txt` and did not re-run `ess specify validate`. The witness models are real files
  and the transcript is internally consistent, but the exit codes are the author's claim.
* I read the two ESS validations the document says do not cover witness W2's (state, input) pair and
  confirmed neither applies. I did **not** read the scenario synthesizer or the conformance runner's
  outcome selection, so "no ESS rule says what happens" is established for validation, not for the
  whole toolchain. If the synthesizer answers that pair, § 4.4's choice becomes a divergence.
* Witness W1 shows `Binary64` is an admitted identity type only under `format: ess/2`. I did not
  determine whether any component in M5/M6 scope declares `ess/2`, so finding 1's reachability today is
  unknown; the contract admits the kind regardless, which is why it is still a finding.
* § 13's extractor citations are the author's; I did not re-read
  `ESS/crates/specify/ess-service-contract/src/lib.rs` this session.

```findings
- file: "docs/design/service-semantics-v0.1.md"
  line: 627
  category: "semantics"
  severity: "blocker"
  verdict: "needs-revision"
  origin: "introduced"
  message: "§ 7.3 calls `address(v)` a total function but its table has no row for the `binary64` kind, while § 7.4 admits every identity kind except `json` and the model enumerates `Binary64` as an `IdentityValueKind` against five `AddressRule` variants, so a `Binary64`-keyed entity has a declared identity kind and no address spelling"
- file: "docs/design/service-semantics-v0.1.md"
  line: 244
  category: "semantics"
  severity: "blocker"
  verdict: "needs-revision"
  origin: "introduced"
  message: "§ 2.2 adds `OutcomeSelector`, `CreateSelector` and `CreateSet` and leaves a creation branch's `emits` payload on the existing `CreateTemplate` scope, which carries `args: None` and admits no `$args`, so a `service/1` creation cannot publish a creation argument that is not also written to a field — `billing.invoice.CreateInvoice`'s `accepted` branch puts `customer_email: input.customer_email` in the event payload and not in `sets`"
- file: "ESS/crates/specify/ess-domain/src/entity.rs"
  line: 1399
  category: "semantics"
  severity: "blocker"
  verdict: "needs-revision"
  origin: "introduced"
  message: "§ 8.1's `References`/`One` row lowers the carrier to a field that is `required` either way, but ESS's `carried_types` admits `Optional<target identity>` for that row, so an entity whose reference is not yet set is admitted by the source and refused by the ER schema, and § 2.1's `RelationViaWrongShape` refuses the `required: false` spelling a lowerer would need"
- file: "docs/design/service-semantics-v0.1.md"
  line: 326
  category: "semantics"
  severity: "blocker"
  verdict: "needs-revision"
  origin: "introduced"
  message: "§ 4.3's input selection takes the first branch whose `when` holds and names a branch with neither `when` nor `in_state` as the default, and says nothing about a branch whose `in_state` matches and which declares no `when`, so the shape ESS spells `SubjectState { state, predicate: None }` is unselectable and answers `NoOutcomeSelected`"
- file: "ESS/crates/specify/ess-primitives/src/facts.rs"
  line: 243
  category: "semantics"
  severity: "blocker"
  verdict: "needs-revision"
  origin: "introduced"
  message: "§ 10.2 bounds the decimal divergence as one-directional, refusing a command ESS would permit rather than permitting one ESS would refuse, which a negated or `ne` guard reverses: with `amount` authored `1.0000000000000000001`, ESS's `parse_decimal` collapses it to `1` and answers `ne 1` false, ER compares the token exactly and answers true, so ER takes an accepting branch ESS does not take"
- file: "docs/design/service-semantics-v0.1.md"
  line: 288
  category: "semantics"
  severity: "warning"
  verdict: "needs-revision"
  origin: "introduced"
  message: "§ 4.2's evaluation-order list carries no numbers while § 2.2 and § 5.3 place the response at step 12 and § 7.4 places the identity mirror at step 13, and the list puts the identity mirror before the response, so no numbering makes both cross-references true"
```
