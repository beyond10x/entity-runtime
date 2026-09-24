---
format: aep.planning-md/2
id: review-result:service-semantics-source-pass-1
kind: review-result
status: active
title: Pure service semantics source examination pass 1
relations:
- reviews: task:pure-service-semantics
revision: 1
---
# Independent source examination — pure ER service semantics, pass 1

```
unit: story:pure-service-semantics / task:pure-service-semantics, source submission
      cb2c1a16602afec90912ec12fb33605d33abdb51, tree 901239ea8247c992ca030cef30e3d5ac337d7455,
      base b2d000ef6196b61e7d4af8217bf3165d7980e9ad
verdict: NEEDS-CHANGE — 2 blockers, 2 warnings, 2 further findings
cases: executed 288→296, red 7
origin: introduced 7 / pre-existing 0 / undecided 0
wrote-outside-worktree: 3 paths
needs-coordinator: `/` is at 99% with 8.8G free; one pre-existing store test failed with ENOSPC
                   and was not re-run
```

## 1. `git --no-pager diff --stat`

The tracked tree is unmodified. Both additions are untracked new test files inside the write scope
the brief assigned:

```console
$ git --no-pager status --porcelain
?? crates/entity-core/tests/service_review_one.rs
?? crates/entity-store/tests/service_review_one.rs

$ git --no-pager diff --stat
(no output — no tracked file changed)
```

| path | sha256 |
|---|---|
| `crates/entity-core/tests/service_review_one.rs` | `f4b251f5520b45a2dff8dcca9ba046093b02984a7ca4a07d9236d8f4b3cd442b` |
| `crates/entity-store/tests/service_review_one.rs` | `f7fd3466c3ff93117ad9cfe31b735df991f18b49780490eeac24e04a09d316bd` |

No production file, no design, no model, no manifest, no lock, no planning artifact and no existing
test was touched. `crates/entity-yaml/tests/service_review_one.rs` was not created: no finding needed
it. No file under attack was mutated, not even briefly.

**Origin, source-established.** Every symbol a finding below sits on is absent from the base. Read
with `git show`, without moving the tree:

```console
$ for s in RelationViaWrongShape observed_values_equal record_domain Semantics unobservable_literal; do
    git --no-pager show b2d000ef…:crates/entity-core/src/validation.rs \
                        b2d000ef…:crates/entity-core/src/runtime.rs | grep -c "$s"; done
0 0 0 0 0
$ git --no-pager show b2d000ef…:crates/entity-store/src/asynchronous/encoding.rs | grep -c record_domain
0
```

So every origin is `introduced`. **No case was executed against the base**, and none is claimed to
have been.

## 2. The cases, and the red output captured when each was written

Written before anything ran; each run alone, with `--exact --nocapture`, before the suites in part 3.
Every exit below is the command's own.

Common environment, from the brief: two jobs, tree-local `target/`, no `CARGO_TARGET_DIR`, no debug
info, no incremental, empty compiler wrappers, `lld` per process, `TMPDIR` at the review tree's
`target/review-tmp`, Rust 1.98.1 locked and offline.

```console
env TMPDIR="$PWD/target/review-tmp" CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 \
    CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_NET_OFFLINE=true \
    RUSTC_WRAPPER= RUSTC_WORKSPACE_WRAPPER= RUSTFLAGS='-C link-arg=-fuse-ld=lld' \
  cargo +1.98.1 test --locked --offline -p <pkg> --test service_review_one <case> -- --exact --nocapture
```

### 2.1 `service_equality_and_membership_read_an_authored_literal_through_the_literal_door` — RED

`crates/entity-core/tests/service_review_one.rs:64`

**Contract.** § 10.2.1: *"Operand origin is retained while evaluating: a number reached through a
reference uses `of_number`; an authored numeric literal uses `of_literal` … **Membership preserves
that distinction for each operand**."* § 10.4: *"`eq`, `ne`, `in` and `contains` over two numbers use
the same rule … a membership test that disagreed with `compare` would be a second numeric semantics
inside one document."* Pinned in § 11 as
`service_numeric_equality_membership_and_bounds_all_use_the_observation_rule`.

**Measured.** `evaluate_compare` picks the door per operand — `observe(value, written)`,
`crates/entity-core/src/runtime.rs:1517-1522`. `eq`, `ne`, `in` and `contains` route through
`equality(context)` → `observed_values_equal` (`:1379-1385,1591-1616`), which receives only the
resolved values and reads both through `of_number`; the authored form is gone by then.

The doors part on an authored integer past the `u64` span: `of_literal` keeps it at scale zero
unconditionally (`crates/entity-core/src/observed.rs:108-118`), `of_number` cannot reach
`as_i64`/`as_u64` and falls to the binary64's canonical decimal (`:72-81,198-206`).

```
running 1 test

thread 'service_equality_and_membership_read_an_authored_literal_through_the_literal_door' (1958347)
panicked at crates/entity-core/tests/service_review_one.rs:98:9:
assertion `left == right` failed: eq answers what compare answers
  left: True
 right: False

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 2 filtered out; finished in 0.00s
EXIT=101
```

The first assertion passed, so `compare` answered `false` — which is what the source answers — and
`eq` answered `true` about the same pair. The control at the literal `1`, where both doors agree, is
in the same case and never reached.

**What reaches it.** § 10.4 states these four operators are *"what the lowerer emits for ESS's
`Compare { eq | ne }`, `AnyOf` and `NoneOf`"*.

### 2.2 `a_references_many_carrier_whose_elements_are_not_the_targets_identity_kind_is_refused` — RED

`crates/entity-core/tests/service_review_one.rs:133`

**Contract.** § 2.1: *"for a `References` relation: `via` is not a declared field of **this**
definition, **or its kind is not the one § 8.1's row admits**"*. § 8.1's `References`/`Many` row:
*"`{type: array, items: <the target's identity kind>}`"*.

**Measured.** `validate_relations` (`crates/entity-core/src/validation.rs:734-765`) tests array-ness
and optionality and never reads `items`. `Registry::declared_relation_defects`
(`crates/entity-core/src/registry.rs:167-177`) short-circuits every `References` relation to the
field-claim check, on the comment *"The declaring definition already checked the carrier's shape"* —
true of array-ness only. The `Owns` row does compare kinds (`registry.rs:226-240`); the `References`
rows have no equivalent anywhere.

```
thread 'a_references_many_carrier_whose_elements_are_not_the_targets_identity_kind_is_refused'
(1959246) panicked at crates/entity-core/tests/service_review_one.rs:165:10:
the whole registry can compare the carrier against the target's identity: ()

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 2 filtered out; finished in 0.00s
EXIT=101
```

`()` is `Registry::validate_all`'s `Ok`: `item` declares an `integer` logical identity, `basket`
carries the relation in an array of `string`, and the registry accepts it.

**What reaches it.** The submission's own
`a_references_many_relation_requires_an_array_of_the_targets_identity_kind`
(`crates/entity-core/tests/service_identity.rs:456-483`) never registers the target and never asserts
anything about the element kind. It distinguishes an array carrier from a string carrier only, so the
name claims a check the body does not make — the § 11 row is not closed by the test cited for it.

### 2.3 `a_references_one_carrier_of_the_wrong_kind_is_refused` — RED

`crates/entity-core/tests/service_review_one.rs:174`

The `References`/`One` half of the same row. A `boolean` carrier for an `integer` identity registers
and passes `validate_all`, because the only shape test on that row is *not an array*
(`crates/entity-core/src/validation.rs:737-746`).

```
thread 'a_references_one_carrier_of_the_wrong_kind_is_refused' (1959429)
panicked at crates/entity-core/tests/service_review_one.rs:204:10:
a boolean carrier cannot hold an integer identity: ()

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 2 filtered out; finished in 0.00s
EXIT=101
```

### 2.4 `a_service_1_creation_without_branches_reconstructs_the_request_the_caller_sent` — RED

`crates/entity-store/tests/service_review_one.rs:57`

**Contract.** § 1.2: *"A `service/1` creation's *original request* is the caller's **arguments**, not
the fields the branch produced. Reconstructing it as `"fields"` would hand a retry a request the
caller never sent, which is what `original_request_comparison_bytes` exists to prevent."*

**Measured.** Four sites decide whether a `service/1` creation's input is its arguments or its fields,
and they do not use the same test:

| site | test |
|---|---|
| `decide_create`, `crates/entity-core/src/runtime.rs:427` | `is_service_1() && !create.outcomes.is_empty()` |
| `replay`, `crates/entity-core/src/replay.rs:140-142` | `is_service_1() && !create.outcomes.is_empty()` |
| `validate_entry_against_state`, `crates/entity-store/src/asynchronous/verify.rs:94-101` | `is_service_1() && !create.outcomes.is_empty()` |
| `record_domain` / `request_domain`, `crates/entity-store/src/asynchronous/encoding.rs:68-85` | `is_service_1()` **only** |

A `service/1` definition whose `create` declares no `outcomes` is admitted at registration:
`validate_outcomes` iterates an empty list and there is no creation analogue of `NoTransitions`
(`crates/entity-core/src/validation.rs:226,513-536`). For it, `decide_create` reads the caller's input
as the **fields** and records `arguments: {}` (`runtime.rs:481-484`), while
`original_request_comparison_bytes` takes the `er.request/2` arm on the framing alone
(`encoding.rs:174-185`).

```
thread 'a_service_1_creation_without_branches_reconstructs_the_request_the_caller_sent' (1959815)
panicked at crates/entity-store/tests/service_review_one.rs:65:5:
assertion `left != right` failed: two creations that differ in what the caller sent reconstruct to different requests
  left: "[\"er.request/2\",{\"arguments\":{},\"definition_version\":1,\"kind\":\"create\",\"recording\":{\"actor\":null,\"causation\":null,\"correlation\":null,\"record_id\":\"r-1\",\"recorded_at\":\"2026-09-16T00:00:00Z\"},\"subject\":[\"probe\",\"p-1\"]}]"
 right: "[\"er.request/2\",{\"arguments\":{},\"definition_version\":1,\"kind\":\"create\",\"recording\":{\"actor\":null,\"causation\":null,\"correlation\":null,\"record_id\":\"r-1\",\"recorded_at\":\"2026-09-16T00:00:00Z\"},\"subject\":[\"probe\",\"p-1\"]}]"

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
EXIT=101
```

Two creations differing in everything the caller sent reconstruct to identical bytes, and neither
carries a `fields` key.

**What reaches it.** This is the shape the submission's own probe helper builds for every value test
it ships: `probe()` at `crates/entity-core/tests/service_values.rs:23-38` sets `semantics: service/1`
and declares no `create` block at all, and `answer()` at `:45-54` calls `create` with fields.
`entity-executor` decides retry identity from these bytes — `match_committed`
(`crates/entity-executor/src/lib.rs:453-459`) against the stored request bytes and `match_imported`
(`:469-477`) against the reconstruction — so both operands lose the fields and a retry carrying
different fields is accepted as the same request.

### 2.5 `a_creation_branch_guarded_on_a_state_the_creation_cannot_be_in_is_skipped` — RED

`crates/entity-core/tests/service_review_one.rs:218`

**Contract.** § 4.3's first selection line: *"**The state test.** If `in_state` is declared and is not
the instance's current state, **skip the branch**"*, with § 4.2's step 10 for a creation: *"the state
is the lifecycle's `initial`"*.

**Measured.** `service_create` passes `state: None` to `select_outcome`
(`crates/entity-core/src/runtime.rs:541-546`), so the state test's `if let (Some, Some)` cannot match
(`:987-991`) and a branch guarded on `closed` selects while the instance is created in `held`.

```
thread 'a_creation_branch_guarded_on_a_state_the_creation_cannot_be_in_is_skipped' (1968151)
panicked at crates/entity-core/tests/service_review_one.rs:244:5:
assertion `left == right` failed: a branch guarded on `closed` cannot be the branch that creates an instance in `held`
  left: Some("guarded")
 right: Some("default")

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 6 filtered out; finished in 0.00s
EXIT=101
```

**What reaches it.** Registration admits the definition: `is_default_branch()` does not count a branch
declaring `in_state`, so `AmbiguousDefaultOutcome`'s last-position rule never looks at it either
(`crates/entity-core/src/validation.rs:550-564`). § 2.1 refuses `wrong_state` on a creation branch
(`WrongStateOnCreate`) and names no rule for `in_state`. **No lowerer path that emits it was found.**

### 2.6 `a_quantifier_binder_extends_the_enclosing_scope_without_replacing_a_fixed_root` — RED

`crates/entity-core/tests/service_review_one.rs:257`

**Contract.** § 2.2: *"A binder **extends** whichever of these scopes encloses it … the body
additionally reads `$line` and `$line.<path>` … **Nothing else changes**, which is what lets a body
mix element facts with free ones."*

**Measured.** The binder is consulted before the fixed roots at registration
(`crates/entity-core/src/validation.rs:944-956`) and at run time
(`crates/entity-core/src/runtime.rs:1876-1887`), so `as: id` replaces `$id` inside the body with the
bound element rather than adding a name beside it.

```
thread 'a_quantifier_binder_extends_the_enclosing_scope_without_replacing_a_fixed_root' (1969879)
panicked at crates/entity-core/tests/service_review_one.rs:268:5:
assertion `left == right` failed: `$id` is the storage address the enclosing scope gives it, not the bound element
  left: False
 right: True

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 6 filtered out; finished in 0.00s
EXIT=101
```

**What reaches it.** The implementor discloses it as a decision taken inside the contract
(`../service-implementation/handoff.md:82-85`), with the stated reason that the alternative *"would
make one binder name silently inert"*. No registration rule refuses a binder named after a fixed root.
It is a divergence from § 2.2's sentence with an argument behind it, so it is root's to route rather
than a defect to route back.

### 2.7 `an_unobservable_numeric_literal_is_refused_wherever_it_is_written` — RED

`crates/entity-core/tests/service_review_one.rs:288`

**Contract.** § 10.2.1: *"definition admission rejects unobservable numeric literals and bounds."*

**Measured.** `unobservable_literal` is reached from the `compare` arm only
(`crates/entity-core/src/validation.rs:1199-1201`). The case's control — the same `1e400` inside
`compare` — passed, so the mechanism exists and names the domain; the `eq` spelling registered.

```
thread 'an_unobservable_numeric_literal_is_refused_wherever_it_is_written' (1971322)
panicked at crates/entity-core/tests/service_review_one.rs:306:6:
the same literal is the same defect in `eq`: ValidatedDefinition(EntityDefinition { … invariants:
[RuleDefinition { name: Some("rule"), condition: Eq { eq: [String("$fields.x"), Number(1e+400)] },
message: Some("the rule") }] … semantics: Service1 … })

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 6 filtered out; finished in 0.00s
EXIT=101
```

Schema bounds **are** checked through the literal door
(`crates/entity-core/src/validation.rs:2101-2107`); the gap is the operator arms other than `compare`.

### 2.8 `a_leading_plus_ordinal_is_refused_at_registration_so_the_runtime_reading_is_unreachable` — GREEN

`crates/entity-core/tests/service_review_one.rs:325`. This case exists to decide a hypothesis, and it
**disproved** it — see part 4's withdrawn rows.

```
test a_leading_plus_ordinal_is_refused_at_registration_so_the_runtime_reading_is_unreachable ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 6 filtered out; finished in 0.00s
EXIT=0
```

## 3. The suite runs

Both run after part 2's cases existed, with `--no-fail-fast` so every target reports. Verbatim logs:
`suite-entity-core.log`, `suite-entity-store.log`, each ending in its own `EXIT=` line.

### `cargo +1.98.1 test --locked --offline --no-fail-fast -p entity-core` — EXIT=101

| target | result |
|---|---|
| `unittests src/lib.rs` | ok. 24 passed; 0 failed |
| `tests/purity.rs` | ok. 4 passed; 0 failed |
| `tests/replay.rs` | ok. 43 passed; 0 failed |
| `tests/requirements.rs` | ok. 69 passed; 0 failed |
| `tests/service_identity.rs` | ok. 23 passed; 0 failed |
| **`tests/service_review_one.rs`** | **FAILED. 1 passed; 6 failed** |
| `tests/service_semantics.rs` | ok. 39 passed; 0 failed |
| `tests/service_values.rs` | ok. 35 passed; 0 failed |
| `Doc-tests entity_core` | ok. 1 passed; 0 failed |

### `cargo +1.98.1 test --locked --offline --no-fail-fast -p entity-store` — EXIT=101

| target | result |
|---|---|
| `unittests src/lib.rs` | ok. 16 passed; 0 failed |
| `tests/async_recorded_adversary.rs` | ok. 2 passed; 0 failed |
| `tests/async_recorded_adversary_pass2.rs` | ok. 1 passed; 0 failed |
| `tests/both_providers.rs` | ok. 7 passed; 0 failed |
| `tests/concurrency.rs` | ok. 5 passed; 0 failed |
| **`tests/conformance.rs`** | **FAILED. 2 passed; 1 failed — environment, see below** |
| `tests/file_record_index.rs` | ok. 7 passed; 0 failed |
| `tests/projections.rs` | ok. 5 passed; 0 failed |
| `tests/service_framing.rs` | ok. 4 passed; 0 failed |
| **`tests/service_review_one.rs`** | **FAILED. 0 passed; 1 failed** |
| `Doc-tests entity_store` | ok. 0 passed; 0 failed |

**`the_file_provider_conforms` is not a finding.** It failed because the disk filled, and it says so
itself (`suite-entity-store.log:76`):

```
FileStore recorded history: "recorded creation: the store failed: writing
…/target/tmp/conformance-file/subjects/…/….json.writing.2010474.12:
No space left on device (os error 28)"
```

`/` was at 99% before the run and reached zero during it. It was **not re-run**: free space after
clearing this review's own scratch is 8.8G, which is at the brief's stop threshold, and root owns the
integration gate. It is attributed to the machine, not to the submission.

**Case counts.** `<before>` is not a pre-emptive suite run. It is these same two runs with this
review's two targets deselected by subtraction, naming which: `entity-core`'s
`tests/service_review_one.rs` (7) and `entity-store`'s `tests/service_review_one.rs` (1).

| | entity-core | entity-store | total |
|---|---|---|---|
| executed, this review's targets excluded | 238 | 50 | **288** |
| executed, all targets | 245 | 51 | **296** |
| red | 6 | 1 | **7** |

Six of the seven red are this review's; the seventh is the ENOSPC above. Nothing the submission wrote
failed on its own terms.

## 4. Findings

Each row covers commit `cb2c1a16602afec90912ec12fb33605d33abdb51`. Origin is source-established from
the base as part 1 shows; no case was run against the base.

| # | file:line | verdict | origin | what was measured | what reaches it |
|---|---|---|---|---|---|
| F1 | `crates/entity-core/src/runtime.rs:1591` | NEEDS-CHANGE | introduced | 2.1 — `eq` answers `True` where `compare` answers `False` about one pair; `service_review_one.rs:98`, exit 101 | § 10.4 names `eq`, `ne`, `in` and `contains` as what the lowerer emits for `Compare { eq \| ne }`, `AnyOf` and `NoneOf` |
| F2 | `crates/entity-store/src/asynchronous/encoding.rs:174` | NEEDS-CHANGE | introduced | 2.4 — two creations reconstruct to identical `er.request/2` bytes carrying `"arguments":{}` and no fields; `service_review_one.rs:65`, exit 101 | registration admits a `service/1` definition with no `create.outcomes`, and it is the shape the submission's own `probe()` builds (`tests/service_values.rs:23-38`). `entity-executor:453,469` decides retry identity from these bytes |
| F3 | `crates/entity-core/src/validation.rs:734` | NEEDS-CHANGE | introduced | 2.2 and 2.3 — `Registry::validate_all` returns `Ok` for an array-of-`string` carrier and for a `boolean` carrier against an `integer` identity; exits 101 | any registered pair of `service/1` definitions with a `References` relation. § 2.1's `RelationViaWrongShape` names the check; § 8.1's rows name the kind |
| F4 | `crates/entity-core/src/validation.rs:1199` | NEEDS-CHANGE | introduced | 2.7 — `1e400` in `compare` is refused and the same literal in `eq` registers; `service_review_one.rs:306`, exit 101 | any authored numeric literal outside the observation domain in `eq`, `ne`, `in`, `contains`, `gt`, `gte`, `lt` or `lte`. Schema bounds are covered |
| F5 | `crates/entity-core/src/runtime.rs:986` | CONFIRMED | introduced | 2.5 — a creation branch guarded on `closed` is selected while creating in `held`; `service_review_one.rs:244`, exit 101 | an admitted definition. `AmbiguousDefaultOutcome` does not count the branch, so its position is unconstrained too. **No lowerer path that emits `in_state` on a creation branch was found** |
| F6 | `crates/entity-core/src/runtime.rs:1876` | CONFIRMED | introduced | 2.6 — `as: id` makes `$id` the bound element inside the body; `service_review_one.rs:268`, exit 101 | any `service/1` quantifier whose binder is named after a fixed root. Disclosed as a decision in `../service-implementation/handoff.md:82-85` with a stated reason |
| F7 | `crates/entity-core/src/validation.rs:96` | INFEASIBLE | introduced | § 2.1 lists `number_observation` among the keys `SemanticsKeyNotAvailable` refuses on a `kernel/1` definition; no check exists | nothing. `NumberObservation` has one variant which is its own `Default`, so no document can carry a distinguishable value. The contract row is unimplementable as written; it is a document correction, not code |

### Withdrawn and disproved hypotheses

The preliminary report carried three further concerns. Each was tested or re-read, and none is a
finding.

| hypothesis | outcome |
|---|---|
| the run-time ordinal walk accepts a leading `+` that § 10.6's grammar does not (`runtime.rs:2018`) | **disproved as reachable.** Case 2.8 is green: `validate_reference_path` refuses `$fields.lines.+1` at registration (`validation.rs:1024-1029,1066-1071`), so no registered definition reaches the looser run-time reading. Two readers of one grammar, with the stricter one in front |
| a creation branch declaring `effect: none` records `DecisionEffect::Unchanged` (`runtime.rs:647-650`) | **withdrawn.** § 2.1's `MissingCreatesEffect` explicitly admits `None` on a creation branch and no accepted requirement says what `effect` such a record carries. § 5.2's sentence about the `Creates` variant is a rationale, not a rule, and promoting it would create a requirement rather than check one |
| `SemanticsKeyNotAvailable` omits `number_observation` | **kept, as F7 above, and only as `INFEASIBLE`** |

## 5. What was attacked and could not be broken

One line each; this is what the silence is worth.

- **Evaluation order, steps 0–15.** `decide` is straight-line and the numbered comments match: identity
  mirror (`runtime.rs:853`) before invariants (`:867`) before events (`:870-873`) before the response
  (`:876-879`). No pair is swapped.
- **`kernel/1` byte preservation.** Every added definition and record key carries `#[serde(default,
  skip_serializing_if = …)]`, and the `kernel/1` creation writes `arguments: Map::new()` under
  `skip_serializing_if = "Map::is_empty"` (`runtime.rs:112-113,481-484`).
- **`Observed` ordering and canonical text.** `exact_cmp`/`magnitude_cmp` never scale an operand into
  an overflow; `exact_text` is injective over the normalised `(units, scale)` pairs the module can
  produce — scale 0 never contains a point, scale > 0 always does, trailing fractional zeroes are
  normalised away. § 7.3.2's equivalence holds for `address`, which reads every operand through
  `of_number` alone.
- **Mixed-representation equality.** A `Repr::Exact` and a `Repr::Binary64` cannot carry one `f64` from
  one door: `of_binary64` is a function of the binary, and every integral value inside the `i128` span
  lands on `Exact`. The `total_cmp` fallback cannot report `Equal` across the arms.
- **Wrong-state selection.** `wrong_states` is the complement of the union of every branch's move
  sources, not of the selected branch's `from` (`runtime.rs:1017-1040`), and `admit_state` answers
  `InvalidTransition` where no `wrong_state` branch is declared — the `kernel/1` answer.
- **`UnspecifiedMoveSource`.** Produced only for a state neither in `wrong_states` nor in the selected
  branch's `from`, and it returns before any record, revision, event or response (`:1069-1078`).
- **Input guard before held state.** `select_outcome` evaluates `when` for every branch declaring no
  `in_state`, in declared order, before step 5 looks at the state.
- **Quantifier vacuity and short-circuit.** An unresolved or non-collection `in` is `Unknown`; an empty
  collection folds from `True`/`False`; the fold breaks once settled (`:1407-1441`). A map walks in
  canonical key order because `serde_json::Map` is ordered without `preserve_order`.
- **`compare`'s three-valued table.** Every § 10.4 row is reproduced, including boolean-against-boolean
  ordering as `Unknown` and cross-kind `eq` as `false` / `ne` as `true`.
- **`truthy`.** Four arms, with the numeric arm testing the carried binary64, so `1e-400` is falsy and
  its token is stored unrounded.
- **Framing refusal.** `record_framing` reads the tag out of raw bytes without parsing the payload
  (`encoding.rs:104-137`); the batch tag does not move.
- **`rehydrate`.** Refuses a `service/1` definition by name before any event is read
  (`replay.rs:210-219`).
- **Scale comparison.** `scale_compare` answers `None` for no containing scale and for two disagreeing
  scales, so an empty `scales` map is `Unknown` and never `false`.
- **Identity address totality.** Every admitted kind has a row, `json` is refused by name, a `null`
  inside a composite is refused, and a number outside the observation domain returns a named error
  rather than an invented address.
- **Purity.** `tests/purity.rs` passed (4 tests): `observed.rs` adds `f64` arithmetic and no clock,
  filesystem, network, environment, thread, randomness or unordered container.
- **The submission's own suite.** 238 cases outside this review's two targets, 0 failed on their own
  terms. `tests/requirements.rs` (69) and `tests/replay.rs` (43) both green.

## 6. Paths written outside the worktree

| path | what |
|---|---|
| `…/service-source-review-1/report.md` | this file |
| `…/service-source-review-1/suite-entity-core.log` | part 3's first run, verbatim, with its own `EXIT=101` |
| `…/service-source-review-1/suite-entity-store.log` | part 3's second run, verbatim, with its own `EXIT=101` |

All three are the evidence directory the brief assigned. Nothing was written to `/tmp`; `TMPDIR` was
the review tree's `target/review-tmp` for every command. Inside the worktree, `target/` was created by
these runs; `target/tmp` and `target/review-tmp` were removed after the runs to return disk, and both
are build scratch this review made.

## 7. Findings block

```findings
- file: crates/entity-core/src/runtime.rs
  line: 1591
  category: contract-drift
  severity: blocker
  verdict: NEEDS-CHANGE
  origin: introduced
  message: service/1 eq, ne, in and contains read both operands through the wire door, so an authored numeric literal past the u64 span answers True where compare answers False, against § 10.2.1's per-operand door rule and § 10.4's agreement requirement.
- file: crates/entity-store/src/asynchronous/encoding.rs
  line: 174
  category: contract-drift
  severity: blocker
  verdict: NEEDS-CHANGE
  origin: introduced
  message: request_domain switches on semantics alone while decide_create, replay and the anchored verifier also require a non-empty create.outcomes, so a service/1 definition with no creation branches reconstructs its original request as empty arguments and two different creations compare as one retry.
- file: crates/entity-core/src/validation.rs
  line: 734
  category: contract-drift
  severity: warning
  verdict: NEEDS-CHANGE
  origin: introduced
  message: a References carrier's kind is never compared with the target's identity kind in either validate_relations or Registry::validate_all, so § 2.1's RelationViaWrongShape admits every non-array kind on the One row and every element kind on the Many row.
- file: crates/entity-core/src/validation.rs
  line: 1199
  category: contract-drift
  severity: warning
  verdict: NEEDS-CHANGE
  origin: introduced
  message: § 10.2.1's rejection of unobservable numeric literals at definition admission is reached from the compare arm only, so the same 1e400 literal registers in eq, ne, in, contains and the ordering operators.
- file: crates/entity-core/src/runtime.rs
  line: 986
  category: boundary
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: select_outcome is passed no state at creation, so a creation branch declaring in_state is selected in a state its guard excludes and is counted as neither a default nor a state guard.
- file: crates/entity-core/src/runtime.rs
  line: 1876
  category: judgement
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: a quantifier binder is resolved before the fixed roots, so a binder named id replaces $id inside the body, where § 2.2 says a binder extends the enclosing scope and nothing else changes.
- file: docs/design/service-semantics-v0.1.md
  category: contract-drift
  severity: note
  verdict: INFEASIBLE
  origin: introduced
  message: § 2.1 lists number_observation among the keys SemanticsKeyNotAvailable refuses on a kernel-1 definition, but the enum has one variant which is its own default, so no document can carry a distinguishable value and the row cannot be implemented as written.
```
