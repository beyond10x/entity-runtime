---
format: aep.planning-md/1
id: review-result:pure-service-semantics-source-pass-2
kind: review-result
status: active
title: Pure service semantics final source examination
relations:
- reviews: task:pure-service-semantics
revision: 1
---
# Independent source examination — pure ER service semantics, pass 2 of 2 (final)

```
unit: task:pure-service-semantics / story:pure-service-semantics, source submission
      f7904be51495caadd3cd2871169d63f3dfe89a84, tree 624d1a966fad873a55eb0482fae19d0f3af966d2,
      whole-implementation base b2d000ef6196b61e7d4af8217bf3165d7980e9ad,
      correction parent cb2c1a16602afec90912ec12fb33605d33abdb51
verdict: NEEDS-CHANGE — 0 blockers, 3 warnings, 1 note, plus 4 judgement notes
cases: executed 282→286, red 4
origin: introduced 3 / pre-existing 1 / undecided 0
wrote-outside-worktree: 3 paths
needs-coordinator: one case did not fail for the reason it was written for — § 2.4; the finding it
                   did produce is `pre-existing` and belongs outside this wave
```

**One premise in the finalize instruction is corrected before anything else is read.** Three of the
four reds are assertion reds. The fourth (§ 2.4) is an `expect()` red at the fixture's **parse**,
before its first assertion, and the raw log says so. The claim that case was written to decide is
therefore **undecided**, and what it measured instead is a different and pre-existing defect. This
is stated here rather than left to be noticed in the log.

## 1. `git --no-pager diff --stat`

No tracked file changed; the command produces no output. Both additions are untracked new test
files inside the write scope the brief assigned.

```console
$ git --no-pager status --porcelain
?? crates/entity-core/tests/service_review_two.rs
?? crates/entity-yaml/tests/service_review_two.rs

$ git --no-pager diff --stat
(no output)
```

| path | sha256 |
|---|---|
| `crates/entity-core/tests/service_review_two.rs` | `e2c915fc0b30a4b2556043762c4544ef47fcc5115a8f479ec840494018114b72` |
| `crates/entity-yaml/tests/service_review_two.rs` | `84f0902bac9f75668d60d632fd91da33921ddd45ff28b5ab03f04147aa37f0bf` |

**Every path in that diff is a test file.** No production, design, model, manifest, lock,
planning or existing-test file was touched; no existing case was deleted, skipped, weakened or
rewritten; no file under attack was mutated, not even briefly. No fixture directory was created —
every case builds its document inline, so none needs a file.

`crates/entity-store/tests/service_review_two.rs` and
`crates/entity-executor/tests/service_review_two.rs` were **not created**: those surfaces were
attacked and not broken (§ 5), and a green file there would be padding.

**Source identity.** `review-files.sha256` and `execution-files.sha256` — whole tracked and
untracked non-ignored manifests, 415 entries each, including `.engineering/planning/` — are byte
identical, both `5d61e5215cf544aaca4b860ec9a3f120a30a2d2b444be1681c5a823f8239a0c3`. The tested
source is the reviewed source plus exactly the two files above, and the two hashes in that manifest
match the two recorded in `READY.md` before anything ran. The build site named in every raw log is
the implementation worktree `…/ess-evolution-service-semantics-20260916`; the review worktree was
not the build site, which is why the compiler's paths read as they do.

## 2. The cases, and the red output captured when each ran alone

Written before anything was executed; run one at a time by root, each with `--exact`, before the
package suites in § 3. Raw logs and own exits under `execution/`.

### 2.1 `a_json_member_of_a_composite_identity_is_refused_at_every_depth` — RED

`crates/entity-core/tests/service_review_two.rs:109`, exit **101**.

**Contract.** § 7.3.3: *"a `json` leaf cannot occur, because `json` is refused as an identity kind
**at every depth** by `IdentityFieldNotAddressable`."* § 7.3 calls `address` *"total"*; § 7.4 says
*"The address is never empty or whitespace, so there is no second refusal … Every other kind is
admitted."*

**Measured.** `validate_identity` reads the declared identity field's own kind and nothing below it
(`crates/entity-core/src/validation.rs:693-712`; the `json` arm is `:704`). The control — a root
`json` identity — is refused and names the field, so the mechanism exists. A composite identity
carrying a `json` **member** registers.

```
running 1 test
test a_json_member_of_a_composite_identity_is_refused_at_every_depth ...
thread 'a_json_member_of_a_composite_identity_is_refused_at_every_depth' (2642345) panicked at
crates/entity-core/tests/service_review_two.rs:109:6:
a json leaf inside a composite identity is the same defect one level down: ValidatedDefinition(
EntityDefinition { entity: "probe", version: 1, schema: ObjectSchema { fields: {"key":
FieldDefinition { kind: Object, required: true, … properties: {"blob": FieldDefinition { kind: Json,
required: true, … }}, … }}, additional_fields: false }, … semantics: Service1,
identity: Some(IdentityDefinition { field: "key" }), … })

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 2 filtered out; finished in 0.00s
EXIT=101
```

The `Debug` above is the registered definition: `kind: Json` under `identity.field = "key"`.

**What reaches it.** Any hand-authored `service/1` definition with a composite identity. A `json`
value is validated by nothing (`crates/entity-core/src/validation.rs:2138`), so such a member may
hold a `null` or a number outside the observation domain; `address` then answers
`AddressError::NullInComposite` or `NumberOutsideSourceDomain`
(`crates/entity-core/src/identity.rs:142-174`) and step 11 reports `IdentityMismatch` carrying that
sentence where an address belongs (`crates/entity-core/src/runtime.rs:1118,1128-1132`). The public
`entity_core::identity::address` is the same function a binding calls (§ 7.5). § 10.2 says
`type: json` is not an admitted lowering target, so **no lowerer path that emits it was found.**

### 2.2 `an_unobservable_numeric_bound_is_refused_at_definition_admission_like_an_unobservable_default` — RED

`crates/entity-core/tests/service_review_two.rs:143`, exit **101**.

**Contract.** § 10.2.1: *"Under `service/1`, numeric schema admission refuses a value outside this
source-observation domain with a path-bearing `ValidationError`; **definition admission rejects
unobservable numeric literals and bounds.**"*

**Measured.** The control passed: the same unreadable number written as a `default` **is** refused
at definition admission and names the domain (`crates/entity-core/src/validation.rs:1872-1883`). A
`min`/`max` is read only when a value arrives (`:2196-2216`), and `validate_field_definition`'s only
bound check compares `min` against `max` (`:1779-1786`) without reading either through the literal
door. The definition registers.

```
running 1 test
test an_unobservable_numeric_bound_is_refused_at_definition_admission_like_an_unobservable_default ...
thread '…' (2642366) panicked at crates/entity-core/tests/service_review_two.rs:143:6:
an unreadable bound is the other half of the same sentence: ValidatedDefinition(EntityDefinition {
entity: "probe", version: 1, schema: ObjectSchema { fields: {"x": FieldDefinition { kind: Number,
required: true, default: Absent, … min: Some(Number(1e+400)), max: None, … }}, … },
… semantics: Service1, … number_observation: SourceNumber1 })

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 2 filtered out; finished in 0.00s
EXIT=101
```

`min: Some(Number(1e+400))` is in the registered definition. The token in the message is `1e+400`
and the authored lexeme was `1e400`: the decoder normalised it before the kernel saw it, which
§ 10.2.1 already says it does not promise to recover.

**What reaches it.** Any `service/1` schema carrying such a bound. The consequence is the one the
sentence exists to prevent: on a **required** field every value collects a second, redundant error;
on an **optional** field with no value ever supplied the bound answers nothing at any evaluation.
An ESS-authored bound would not be outside the domain, so **no lowerer path that emits one was
found** — the same reachability class root accepted for F4 in pass 1.

### 2.3 `a_map_declared_as_a_union_variant_still_addresses_its_size_and_not_its_count_key` — RED

`crates/entity-core/tests/service_review_two.rs:178`, exit **101**.

**Contract.** § 10.6: *"a `map`'s keys stay unaddressable (§ 10.4), so `{metadata: {count: "7"}}`
cannot be read as its own `count` key: the only `count` a `map` has is its size."* § 10.1 types a
union's `variants` with a full `FieldDefinition`; § 10.5 reaches a variant at `$fields.payee.value`
under the tag test.

**Measured.** The run-time walk continues under a union's content key with
`field.variants.get(segment)` (`crates/entity-core/src/runtime.rs:2079`), which looks a **variant
label** up by the **wire key**. No variant is called `value`, so the declared `map` is lost and the
payload is walked as an untyped object. Registration admits the address without checking it, because
`walk_field_path`'s union arm returns at the content key
(`crates/entity-core/src/validation.rs:1105-1117`).

```
running 1 test
test a_map_declared_as_a_union_variant_still_addresses_its_size_and_not_its_count_key ...
thread '…' (2642387) panicked at crates/entity-core/tests/service_review_two.rs:178:5:
assertion `left == right` failed: a declared map's only `count` is its size, whichever field
declares the map
  left: False
 right: True

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 2 filtered out; finished in 0.00s
EXIT=101
```

`$fields.payee.value.count` resolved to the map's own `"7"` key, so `compare` saw a string against
`1` — two scalar kinds, never equal — and the invariant answered false where the map's size `1`
would have answered true. This is a **wrong value**, not a refusal.

**What reaches it.** Every `$fields.<path>` in a `service/1` selector, precondition, invariant,
`set`, event payload or response walks this code. The construction needs a `map` declared as a
`union` variant; `ESS/examples/billing`'s `payee` union declares `Email` and `CompanyRef`, so **no
lowerer path that emits a map variant was found.** The general form — a union payload is walked with
whatever `variants.get(<wire key>)` happens to return — is § 6's third judgement note.

### 2.4 `a_service_1_numeric_literal_is_one_value_from_yaml_and_from_json` — RED, **but not for the claimed reason**

`crates/entity-yaml/tests/service_review_two.rs:87`, exit **101**.

The case was written to decide whether one authored literal is one value through both front doors.
It never reached its first assertion: `entity_yaml::from_str` refused the fixture.

```
running 1 test
test a_service_1_numeric_literal_is_one_value_from_yaml_and_from_json ...
thread '…' (2642480) panicked at crates/entity-yaml/tests/service_review_two.rs:87:49:
the YAML document is a definition: YamlError(Error("invariants[0].assert.compare.right: invalid
type: integer `100000000000000000000000001` as u128, expected unambiguous YAML", line: 19,
column: 16))

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
EXIT=101
```

Two things follow, and they are different.

**The hypothesis `READY.md` § 3 predicted is disproved for this input.** I predicted
`serde_yaml_ng` would resolve the scalar to an `f64` and collapse the authored decimal. It does not:
it offers the value as a **`u128`**. Whether the exactness would then have survived the second pass
is **inferred, not measured** — `serde_json`'s `Value` visitor accepts `u128` under
`arbitrary_precision`, which is enabled workspace-wide (`Cargo.toml:30`) — and no case here decided
it. **The claim that one authored `service/1` literal is one value from YAML and from JSON is
therefore undecided by this pass**, for the `u128` range and for the decimal range alike.

**What the log does establish** is *which reader refused*. `expected unambiguous YAML` is
`NoDuplicatesVisitor::expecting`'s own string (`crates/entity-yaml/src/lib.rs:84-86`), so the
refusal comes from the duplicate-key pre-pass (`:55-60`), whose visitor implements `visit_i64`,
`visit_u64` and `visit_f64` and **not** `visit_i128`/`visit_u128` (`:116-154`). Serde's default
`visit_u128` then returns `invalid_type`. That is F4 below.

**Origin, source-established without moving the tree.**
`git --no-pager diff --stat b2d000e..f7904be -- crates/entity-yaml/src/lib.rs Cargo.toml
crates/entity-core/src/number.rs` is **empty**: the YAML reader, the `arbitrary_precision` feature
and `number::compare` are all unchanged from the base, and the refusal does not read `semantics`.
So F4 reproduces at the base and is **pre-existing**. No case was run against the base.

## 3. The suite runs

Run after § 2's cases existed. Raw log `execution/packages.log`, own exit **101**.

```console
$ cargo test --locked --offline --no-fail-fast -p entity-core -p entity-yaml
…
error: 2 targets failed:
    `-p entity-core --test service_review_two`
    `-p entity-yaml --test service_review_two`
EXIT=101
```

| package | target | result |
|---|---|---|
| entity-core | `unittests src/lib.rs` | ok. 24 passed; 0 failed |
| entity-core | `tests/purity.rs` | ok. 4 passed; 0 failed |
| entity-core | `tests/replay.rs` | ok. 43 passed; 0 failed |
| entity-core | `tests/requirements.rs` | ok. 69 passed; 0 failed |
| entity-core | `tests/service_identity.rs` | ok. 29 passed; 0 failed |
| entity-core | `tests/service_review_one.rs` | ok. 7 passed; 0 failed |
| entity-core | **`tests/service_review_two.rs`** | **FAILED. 0 passed; 3 failed** |
| entity-core | `tests/service_semantics.rs` | ok. 43 passed; 0 failed |
| entity-core | `tests/service_values.rs` | ok. 38 passed; 0 failed |
| entity-core | `Doc-tests entity_core` | ok. 1 passed; 0 failed |
| entity-yaml | `unittests src/lib.rs` | ok. 0 passed; 0 failed |
| entity-yaml | `tests/aep_lifecycles.rs` | ok. 14 passed; 0 failed |
| entity-yaml | `tests/kernel_bytes.rs` | ok. 1 passed; 0 failed |
| entity-yaml | `tests/runtime.rs` | ok. 8 passed; 0 failed |
| entity-yaml | **`tests/service_review_two.rs`** | **FAILED. 0 passed; 1 failed** |
| entity-yaml | `Doc-tests entity_yaml` | ok. 1 passed; 0 failed |

`cargo fmt --all -- --check`: exit **0**, `execution/fmt.log` is zero bytes.
Strict Clippy over the two affected packages, `--all-targets -D warnings`: exit **0**,
`execution/clippy.log` carries no warning line. **The four reds are therefore behaviour, not typos**
— every one of them compiles under the repository's own lints.

**Nothing the submission wrote failed.** 282 cases outside this review's two targets, 0 failed.

### How `executed 282→286` was derived, and why it is not 461

`READY.md` § 2.3 proposed `cargo test … -- --skip service_review_two`. **That command would have
excluded nothing**: libtest's `--skip` matches **test function names**, and none of the four
functions contains `service_review_two` — the string is an integration *target* name. A count taken
that way would not have moved, which is the exact failure the `<before>→<after>` line exists to
catch. It was not run.

The count is taken by **target subtraction against the retained prior green log at the same source**,
`service-source-correction-1/checks/11-final-workspace-test.log` (exit 0):

| package | prior green, per target | this run, same targets | new target |
|---|---|---|---|
| entity-core | 24+4+43+69+29+7+43+38+1 = **258** | identical, **258** | `service_review_two` **3** |
| entity-yaml | 0+14+1+8+1 = **24** | identical, **24** | `service_review_two` **1** |
| total | **282** | **282** | **4** |

Every pre-existing target count is identical in the two logs, so the two new targets are the whole
difference: **282 → 286, red 4.**

**Scope of that number.** It counts the **two affected packages only**. The workspace figure the
implementing state reported green — 57 targets / 461 cases — is not the denominator here, because
this pass ran no workspace suite, no MSRV job and no `task check`; those remain root's, at root's
build token, and nothing in this report claims them.

## 4. Findings

Each row covers commit `f7904be51495caadd3cd2871169d63f3dfe89a84` plus the two untracked test files
of § 1. Origin is source-established by reading the base with `git show` / `git diff`; **no case was
run against the base**, and none is claimed to have been.

| # | file:line | verdict | severity | origin | what was measured | what reaches it |
|---|---|---|---|---|---|---|
| G1 | `crates/entity-core/src/validation.rs:704` | NEEDS-CHANGE | warning | introduced | 2.1 — a composite identity carrying a `json` member registers; `service_review_two.rs:109`, exit 101 | step 11 for every `service/1` create and execute (`runtime.rs:1118`) and the public `identity::address` a binding calls. § 7.3.3 promises the refusal at every depth and § 7.3 calls the function total; both are false for this definition. **No lowerer path found** — § 10.2 excludes `json` as a lowering target |
| G2 | `crates/entity-core/src/validation.rs:1779` | NEEDS-CHANGE | warning | introduced | 2.2 — `min: 1e400` registers while the same number as a `default` is refused at admission; `service_review_two.rs:143`, exit 101 | any `service/1` schema bound. On an optional field the bound is unevaluable at every evaluation and is never reported at all, which is what § 10.2.1's sentence refuses. **No lowerer path found** |
| G3 | `crates/entity-core/src/runtime.rs:2079` | NEEDS-CHANGE | warning | introduced | 2.3 — `$fields.<union>.value.count` reads a declared map's own `count` key instead of its size; `left: False, right: True`, exit 101 | every `service/1` `$fields.<path>` walk; registration admits the address (`validation.rs:1105-1117`). Needs a `map` declared as a union variant. **No lowerer path found** — `ESS/examples/billing`'s `payee` declares no map variant |
| G4 | `crates/entity-yaml/src/lib.rs:116` | CONFIRMED | note | **pre-existing** | 2.4 — `entity_yaml::from_str` refuses any definition carrying an integer scalar that `serde_yaml_ng` offers as `i128`/`u128`, with `NoDuplicatesVisitor`'s own *"expected unambiguous YAML"* message; `service_review_two.rs:87`, exit 101 | `entity-cli/src/main.rs:1415` `load_definition`, every `--definition <path>`, and `:1272` for the built-in definitions — so `entity validate` and every YAML-fed verb. Unchanged from `b2d000e` and independent of `semantics`, so it predates this unit and belongs outside this wave |

**G4's severity is `note` and its origin is `pre-existing` on purpose.** It holds, and it is not this
unit's. A guessed `pre-existing` is the one error nothing downstream catches, so the support is
stated rather than asserted: `crates/entity-yaml/src/lib.rs`, root `Cargo.toml` and
`crates/entity-core/src/number.rs` are absent from `git diff --stat b2d000e..f7904be`.

**Nothing else was raised.** Pass 1's F1–F5 corrections and root's F6/F7 dispositions are preserved
and were not reopened; § 5 records what was re-read and what held.

## 5. Attacked and could not be broken

One line each; this is what the silence is worth. All established by reading source at the reviewed
commit, none by execution.

- **F1, operand origin.** `equal_under` carries the authored spelling at every depth through
  `written_element` / `written_member`, and `in` / `contains` hand each side its own operand's
  spelling (`runtime.rs:1356-1400,1415-1446,1664-1705`). A literal list's third element is a
  literal; everything under a reference is a wire value.
- **F2, branchless creation.** `decide_create`, `replay`, the anchored verifier and
  `record_domain`/`request_domain` all switch on `semantics` alone (`runtime.rs:495-499`,
  `replay.rs:143`, `verify.rs:98`, `encoding.rs:68-85`), and a branchless creation records its
  normalized input as its arguments, so the four readers answer once.
- **F3, carrier kinds.** `same_carried_kind` descends an array's element kind at every level and
  through nothing else; both `References` rows and the `Owns` row compare against the related
  entity's identity kind in `Registry::validate_all` (`registry.rs:331-430`).
- **F4, unobservable literals.** `unobservable_condition_literals` is an exhaustive match over
  `Condition`, recurses into an operand at any depth and into a quantifier body
  (`validation.rs:1272-1349`). The three operators it skips read no number. The gap left is the
  schema bound, G2 — not an operator arm.
- **F5, creation state guard.** `service_create` passes `Some(initial)` to `select_outcome`
  (`runtime.rs:560-565`); the copied regression in the tree is green (`service_review_one.rs`, 7
  passed).
- **F6 / F7 dispositions.** Preserved, not reopened: binder shadowing follows `Element::rebind`
  (`runtime.rs:1956-1976`), and `NumberObservation` carries no distinguishable presence.
- **Every § 11 name exists.** All 110 test names the acceptance table lists resolve to a live
  `fn <name>(` under `crates/`; no `#[ignore]` appears anywhere in `crates/`.
- **Reference scopes.** `OutcomeSelector`, `CreateSelector`, `CreateSet` and
  `CreateOutcomeTemplate` admit exactly the sets § 2.2 tabulates, and `CreateTemplate` is unchanged
  (`validation.rs:886-930`).
- **Evaluation order.** `decide` is straight-line and runs 0–15 with the identity mirror at 11
  before the invariants at 12, the events at 13 and the response at 14 (`runtime.rs:746-924`). No
  pair is swapped.
- **Selection.** The input guard is answered before the held state for a branch declaring no
  `in_state`; a non-matching `in_state` skips without evaluating its `when`; `Unknown` refuses
  rather than falling through (`runtime.rs:1001-1035`). `wrong_states` is the complement of the
  union of every branch's move sources, and `admit_state` answers `InvalidTransition` where no
  `wrong_state` branch is declared (`:1038-1099`).
- **Identity address.** Every admitted kind has a row, `json` is refused by name at the root,
  composites recurse with sorted keys and canonical numeric text, and an unvalidated value receives
  a named error rather than an invented address (`identity.rs:102-208`). G1 is the one hole, and it
  is a registration hole rather than an address one.
- **Numbers.** `Observed`'s two doors, the `-0.0`/`0.0` collapse, the `1e-400` underflow arm and the
  exact-integer path past 2^53 all read as § 10.2.1 states them; `exact_cmp`/`magnitude_cmp` never
  scale an operand into an overflow (`observed.rs:65-345`).
- **Framing and old-reader refusal.** `record_framing` reads the tag out of raw bytes without
  parsing the payload; the batch tag does not move and each member carries its own; `DecisionCommand`
  and `EntityDefinition` are `deny_unknown_fields`, so a pre-`service/1` build refuses `arguments`
  and `semantics` by name (`encoding.rs:104-137,242-269`).
- **Store and executor.** Retry identity, the anchored verifier, batch member framing and
  `RecordedCommit::validate` were read against `Updates`, `Unchanged`, zero-event, refusing and
  branchless shapes. Nothing was found, which is why neither package has a case file.
- **`kernel/1` preservation.** Every added definition and record key is `#[serde(default)]` with a
  `skip_serializing_if` true for the `kernel/1` value; `kernel_bytes.rs` is green over the shipped
  examples, and `replay.rs` (43), `requirements.rs` (69) and `purity.rs` (4) are green.

## 6. Judgement findings — the residue, as text

Not written to the planning store, and no `aep plan artifact` command was run. Each holds; none was
made into a case, because doing so would have asserted a rule the contract does not state.

1. **`create.emit` is inert beside `create.outcomes`.** `service_create` materialises only the
   selected branch's `emits` (`runtime.rs:637-646`), so a `service/1` definition declaring both
   loses its `create.emit` event — while the same key **does** fire for a **branchless** `service/1`
   creation, which takes the other path (`runtime.rs:474-483`). Registration validates the key's
   references in `CreateTemplate` and refuses nothing (`validation.rs:200-213`). § 4.2 gives step 13
   to the branch, so the behaviour is contract-correct; what is missing is a refusal for a key that
   can never fire. `CONFIRMED` / `introduced` / note.
2. **`create.arguments` is inert on a branchless `service/1` creation.** That path validates the
   caller's input against `definition.schema`, so a declared creation-argument schema and its
   defaults are never read (`runtime.rs:432-445`). This is the shape the repository's own `probe()`
   helpers build. `CONFIRMED` / `introduced` / note.
3. **A `union` payload is walked untyped.** `walk_field_path`'s union arm returns at the content key
   (`validation.rs:1105-1117`) and the run-time walk continues with `variants.get(<wire key>)`
   (`runtime.rs:2079`). The map case is G3, the one measurable consequence; the general form is that
   a variant's declared kind is never the one the walk continues under, and the tag — which the
   value carries at run time — is never read to pick it. `CONFIRMED` / `introduced` / note.
4. **`er.request/2`'s observation shape is unreachable.** § 1.2 lists three shapes for the new
   request framing, but `request_domain` answers `er.request/1` for every observation
   (`encoding.rs:74,80-85`), because an observation carries no definition snapshot. Document
   over-specification, not a code defect. `INFEASIBLE` / `introduced` / note.

## 7. Paths written outside the worktree

| path | what |
|---|---|
| `…/service-source-review-2/report.md` | this file |
| `…/service-source-review-2/READY.md` | the pre-execution brief: commands, case names, predictions, hashes |
| `…/service-source-review-2/section11-names.txt` | the 110 § 11 test names extracted for the existence check in § 5 |

All three are the evidence directory the brief assigned. Nothing was written to `/tmp`. The
`execution/` tree and the two `*-files.sha256` manifests were written by root, not by this session.
Inside the worktree, the two untracked test files of § 1 and nothing else; this session created no
`target/` and ran no build.

## 8. Findings block

```findings
- file: crates/entity-core/src/validation.rs
  line: 704
  category: contract-drift
  severity: warning
  verdict: NEEDS-CHANGE
  origin: introduced
  message: validate_identity reads only the declared identity field's own kind, so a composite identity carrying a json member registers, against § 7.3.3's refusal at every depth and § 7.3's claim that address is total over admitted values.
- file: crates/entity-core/src/validation.rs
  line: 1779
  category: contract-drift
  severity: warning
  verdict: NEEDS-CHANGE
  origin: introduced
  message: a service/1 numeric schema bound outside the source observation domain registers and is read only when a value arrives, while the same number as a default is refused at definition admission, against § 10.2.1's rejection of unobservable literals and bounds at admission.
- file: crates/entity-core/src/runtime.rs
  line: 2079
  category: boundary
  severity: warning
  verdict: NEEDS-CHANGE
  origin: introduced
  message: the walk continues under a union's content key with variants.get(wire key) rather than the variant the tag names, so a map declared as a union variant loses its declaration and $fields.u.value.count reads the map's own count key instead of its size, against § 10.6.
- file: crates/entity-yaml/src/lib.rs
  line: 116
  category: boundary
  severity: note
  verdict: CONFIRMED
  origin: pre-existing
  message: NoDuplicatesVisitor implements visit_i64, visit_u64 and visit_f64 but not visit_i128/visit_u128, so entity_yaml::from_str refuses any definition carrying an integer scalar past the u64 span with a message about ambiguous YAML that names nothing the author can act on.
- file: crates/entity-core/src/runtime.rs
  line: 637
  category: judgement
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: a service/1 creation declaring both create.emit and create.outcomes registers and never materialises create.emit, while a branchless service/1 creation does fire it, so one validated key fires or does not on whether outcomes is empty.
- file: crates/entity-core/src/runtime.rs
  line: 432
  category: judgement
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: a branchless service/1 creation validates the caller's input against the entity schema, so a declared create.arguments schema and its defaults are never read.
- file: crates/entity-core/src/validation.rs
  line: 1105
  category: judgement
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: a path through a union's content key is admitted at registration without a declared kind and answered at run time with whatever variants.get(wire key) returns, so a variant's declared kind is never the one the walk continues under.
- file: crates/entity-store/src/asynchronous/encoding.rs
  line: 80
  category: contract-drift
  severity: note
  verdict: INFEASIBLE
  origin: introduced
  message: § 1.2 declares an observation shape for er.request/2 that request_domain can never produce, because an observation carries no definition snapshot and always frames as er.request/1.
```
