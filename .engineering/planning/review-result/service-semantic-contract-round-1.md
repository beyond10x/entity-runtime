---
format: aep.planning-md/2
id: review-result:service-semantic-contract-round-1
kind: review-result
status: active
title: Service semantic contract independent review, round 1
relations:
- reviews: task:service-semantic-contract
revision: 1
---
needs-revision

Path convention: unprefixed paths are repository-relative in the ER proposal tree; `ESS/` is
repository-relative in the ESS semantic-source tree.

## Findings

docs/design/service-semantics-v0.1.md — the body does not say what `DecisionCommand::Create { fields }` holds for a `service/1` creation, so a record either cannot re-select the creation branch or silently changes the meaning of a named key in `er.record/1` and `er.request/1`, both of which § 1 says do not move — docs/design/service-semantics-v0.1.md:262

docs/design/service-semantics-v0.1.md — § 10 refuses `Map<K,V>` and union outright, which excludes `billing.invoice.Invoice` itself (`payee` is a union, `metadata` is a `Map<String, String>`), so the contract cannot lower the entity the required acceptance fixture is built around — ESS/examples/billing/domains/invoice.yaml:44,113,121

docs/design/service-semantics-v0.1.md — § 4 step 1 takes the `wrong_state` branch whenever the instance's state is outside the union of `Moves.from`, without evaluating any selector, so an `in_state` branch naming a state outside that union becomes unreachable and § 2 names no registration refusal for the overlap — docs/design/service-semantics-v0.1.md:186

docs/design/service-semantics-v0.1.md — § 2 makes `RelationViaUnknown` and `RelationCardinalityMismatch` single-definition registration refusals over `via`, but ESS puts an `Owns` relation's `via` on the target entity, so every `Owns` relation is refused at registration and a `Many` `Owns` carries a scalar `ref` rather than an `array` of `ref` — ESS/crates/specify/ess-compiler/src/ir.rs:414

docs/design/service-semantics-v0.1.md — `OutcomeEffect` declares no `Creates` variant while § 6 names `DecisionEffect::Created` and the typed model names `EffectKind::Creates`, so a creation branch carries `effect: None` and `UnobservableOutcome` refuses a zero-event creation branch that `kernel/1` admits today — docs/design/service-semantics-v0.1.md:141

docs/design/service-semantics-v0.1.md — the body names no representation and no refusal for a command's declared return value, `ResolvedCommand.response`, which the crosswalk lists among what needs one before general lowering — ESS/crates/specify/ess-compiler/src/ir.rs:810

docs/design/service-semantics-v0.1.md — § 2 defines `ScopeKind::OutcomeSelector` as the precondition scope minus `$state`, but that scope already excludes `$state` and admits `$to_state`, which is undetermined at step 4 because the selected branch's effect is what produces it — crates/entity-core/src/validation.rs:279,318

docs/design/service-semantics-v0.1.md — § 13 presents component selection as an open decision root must settle, but `ess-service-contract::extract` already takes the component as a caller argument and § 13's own closing sentence is the answer — ESS/crates/specify/ess-service-contract/src/lib.rs:121

docs/design/service-semantics-v0.1.md — § 4 calls the existing contract eleven steps and claims new steps 6–13 are that contract, but `kernel-v0.1.md` § 6 lists twelve steps (0–11) and new step 10, the identity mirror, is an insertion — docs/design/kernel-v0.1.md:344

## What I read

Three proposal artifacts (`docs/design/service-semantics-v0.1.md`, `ess/service-semantics/system.yaml`,
`ess/service-semantics/domains/service.yaml`), `task:service-semantic-contract` revision 3, the
critic rubric, `ESS-EVOLUTION.md` step 6, `source-routing-result.md`, the author handoff plus its
`hashes.txt` and `ess-specify-validate.txt`; in ER `crates/entity-core/src/{definition,runtime,replay,validation,number,timestamp}.rs`,
`crates/entity-store/src/asynchronous/{verify,encoding}.rs`, `crates/entity-executor/src/lib.rs`,
`Cargo.toml`, `docs/design/{kernel,recorded-execution,recorded-execution-encoding}-v0.1.md`; in ESS
`crates/specify/ess-compiler/src/ir.rs`, `crates/specify/ess-primitives/src/{predicate,facts}.rs`,
`crates/specify/ess-domain/src/{command,types}.rs`, `crates/specify/ess-service-contract/src/lib.rs`,
`examples/billing/domains/invoice.yaml`, `examples/gatepass/domains/visit.yaml`. Read tools and
content search only; no build, no execution, no write outside this file.

## Confirmed, so not findings

Both author corrections hold against source. Empty `AnyOf` maps to ER `in` with a literal empty list:
ESS answers `Unknown` unobserved and `False` observed (`ESS/crates/specify/ess-primitives/src/predicate.rs:528`),
ER answers the same (`crates/entity-core/src/runtime.rs:633`); `NoneOf` through Kleene `not` matches
on both sides. Numeric bounds use `number::compare`, not `f64` (`crates/entity-core/src/validation.rs:1001`);
the `f64` remark at `:899` is about the integer kind test. The Binary64 refusal is source-backed: ESS
orders Binary64 by `total_cmp`, ER calls `-0.0` and `0` equal. The present-null refusal holds — every
field kind but `json` rejects `Value::Null` (`crates/entity-core/src/validation.rs:888-967`). The
`RefusalMutatesState` and `UnobservableOutcome` mirrors match ESS `RefusalMutatedState` and
`EmptyChange`. The precedence example is real: a `Draft` invoice with a non-positive amount satisfies
both `rejected` and `wrong-state`. Byte preservation through `default` + `skip_serializing_if`, and
old-reader rejection through the existing `deny_unknown_fields`, are both sound for `kernel/1`.

## What I could not establish

No shell in this session, so I did not recompute the three sha256 digests; the design text I read is
consistent with `hashes.txt` but the digests are the author's claim, not mine.

I did not determine how an ESS `Timestamp` value reaches `FactValue`. If it arrives as `Text`, ESS
ordering routes through `facts.scales()` and answers `Unknown` without a declared scale, while § 10
maps the same predicate to ER `before`/`after`, which answers `True`/`False`. Unresolved, and it
would be a divergence in the unsafe direction if `Text` is what it is.

Outside my lane, stated and not counted in the verdict: § 2 requires the selector-free branch to be
the last non-`wrong_state` branch, while ESS `Otherwise` is position-independent, so a lowerer must
reorder and no section says it may.

```findings
- file: "docs/design/service-semantics-v0.1.md"
  line: 262
  category: "semantics"
  severity: "blocker"
  verdict: "needs-revision"
  origin: "introduced"
  message: "the body does not say what `DecisionCommand::Create { fields }` holds for a `service/1` creation, so a record either cannot re-select the creation branch or silently changes the meaning of a named key in `er.record/1` and `er.request/1`, both of which § 1 says do not move"
- file: "ESS/examples/billing/domains/invoice.yaml"
  line: 113
  category: "semantics"
  severity: "blocker"
  verdict: "needs-revision"
  origin: "introduced"
  message: "§ 10 refuses `Map<K,V>` and union outright, which excludes `billing.invoice.Invoice` itself (`payee` is a union, `metadata` is a `Map<String, String>`), so the contract cannot lower the entity the required acceptance fixture is built around"
- file: "docs/design/service-semantics-v0.1.md"
  line: 186
  category: "semantics"
  severity: "blocker"
  verdict: "needs-revision"
  origin: "introduced"
  message: "§ 4 step 1 takes the `wrong_state` branch whenever the instance's state is outside the union of `Moves.from`, without evaluating any selector, so an `in_state` branch naming a state outside that union becomes unreachable and § 2 names no registration refusal for the overlap"
- file: "ESS/crates/specify/ess-compiler/src/ir.rs"
  line: 414
  category: "semantics"
  severity: "blocker"
  verdict: "needs-revision"
  origin: "introduced"
  message: "§ 2 makes `RelationViaUnknown` and `RelationCardinalityMismatch` single-definition registration refusals over `via`, but ESS puts an `Owns` relation's `via` on the target entity, so every `Owns` relation is refused at registration and a `Many` `Owns` carries a scalar `ref` rather than an `array` of `ref`"
- file: "docs/design/service-semantics-v0.1.md"
  line: 141
  category: "semantics"
  severity: "blocker"
  verdict: "needs-revision"
  origin: "introduced"
  message: "`OutcomeEffect` declares no `Creates` variant while § 6 names `DecisionEffect::Created` and the typed model names `EffectKind::Creates`, so a creation branch carries `effect: None` and `UnobservableOutcome` refuses a zero-event creation branch that `kernel/1` admits today"
- file: "ESS/crates/specify/ess-compiler/src/ir.rs"
  line: 810
  category: "semantics"
  severity: "blocker"
  verdict: "needs-revision"
  origin: "introduced"
  message: "the body names no representation and no refusal for a command's declared return value, `ResolvedCommand.response`, which the crosswalk lists among what needs one before general lowering"
- file: "crates/entity-core/src/validation.rs"
  line: 279
  category: "semantics"
  severity: "blocker"
  verdict: "needs-revision"
  origin: "introduced"
  message: "§ 2 defines `ScopeKind::OutcomeSelector` as the precondition scope minus `$state`, but that scope already excludes `$state` and admits `$to_state`, which is undetermined at step 4 because the selected branch's effect is what produces it"
- file: "ESS/crates/specify/ess-service-contract/src/lib.rs"
  line: 121
  category: "semantics"
  severity: "warning"
  verdict: "needs-revision"
  origin: "introduced"
  message: "§ 13 presents component selection as an open decision root must settle, but `ess-service-contract::extract` already takes the component as a caller argument and § 13's own closing sentence is the answer"
- file: "docs/design/kernel-v0.1.md"
  line: 344
  category: "semantics"
  severity: "warning"
  verdict: "needs-revision"
  origin: "introduced"
  message: "§ 4 calls the existing contract eleven steps and claims new steps 6–13 are that contract, but `kernel-v0.1.md` § 6 lists twelve steps (0–11) and new step 10, the identity mirror, is an insertion"
```
