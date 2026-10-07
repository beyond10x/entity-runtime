---
format: aep.planning-md/3
id: review-result:er-54-u3-adversary-pass-1
kind: review-result
status: active
title: 'Issue 54 U3 (set clears): adversary pass 1'
relations:
- reviews: story:set-clears-an-optional-field
revision: 1
---
unit: story:set-clears-an-optional-field, branch impl/set-clears-an-optional-field at c466fcd7 plus 4 untracked test files
verdict: NEEDS-CHANGE
cases: executed 602→611, red 4 (plus 6 cross-version probes in scratch, outside the suite, 2 red)
origin: introduced 4 / pre-existing 1 / undecided 0
wrote-outside-worktree: none
needs-coordinator: (1) aep-service pins 0.17.6 and reads `.events()` at `crates/aep-service-postgres/src/query.rs:64` (from a `git grep`); a hook refused my further reads in that repository, so I can't say whether it folds them. (2) F1's fix is in `entity-surface`, which is outside the unit's file assignment.

**1. Diff proof.** `git diff --stat` is empty: no tracked file changed. Untracked, all test files:
- `crates/entity-core/tests/clear_adversary.rs` (249 lines)
- `crates/entity-store/tests/adversary_clear_records.rs` (235 lines)
- `crates/entity-surface/tests/adversary_clear_events.rs` (152 lines)
- `crates/entity-eventlog/tests/adversary_clear_records.rs` (197 lines)

**First question, answered by running code**

| (a) store paths on this branch | result |
|---|---|
| Async memory recorded store (re-executes each append), then `verify_complete_store` | green under `/1`, `/2`, `/3`; `removed` is in the record bytes |
| `FileStore`: commit, reopen, `replay` | green under `/1`, `/2`, `/3` |
| Eventlog file provider (1.91.0): commit, exact retry, cold reopen, `verify_subject_history`, `replay` | green, `er.record/1` |

| (b) reader | field type `string` | field type `json` |
|---|---|---|
| 0.29.0 `replay` | refuses at step 9: `fields.snoozed_until: expected string` | refuses: `records[1]` differs from the recomputed decision |
| 0.29.0 `rehydrate` | refuses: "carries removal evidence" | same |
| 0.29.0 recorded verify | refuses: `CorruptHistory` | refuses: `CorruptHistory` |
| 0.29.0 `FileStore` load, records, events | accepts without verifying; the state is correct and `removed` is kept | same |
| 0.29.0 deciding `wake` on that store | refuses (`Validation`) | writes `{"cleared":true}` and appends it; this branch's `replay` of the store then refuses `records[2]` |
| 0.17.6 decode | `DecisionRecord` refuses the unknown `removed`; `DomainEvent` drops it | probe stopped after the string case |
| 0.17.6 `rehydrate` | folds silently to revision 2 with `snoozed_until` still present | not run |

**The path that drops `removed`** is `entity-core` before 0.19.0: its `DomainEvent` has no `removed` field and no closed key set, and the 0.17.x fold applies `changed` only. 0.18.x also drops the key, but its fold compares the event with the operation's `set`, so it should refuse; that is inferred, not run. 0.29.0 drops nothing.

**Which consumers reach it:** atlas (rev e5ee9d6c), bench (0.17.3) and aep-service (0.17.6) pin a pre-0.19 reader. A grep of their production code found no `rehydrate` call; the only call is in an aep test, which pins 0.25.0. The divergent-write row needs two installed versions on one store. I found no consumer flow that does that.

**2. Cases added**, each run alone with the red output captured before the suite:
- **`clear_adversary.rs`**:
  - `a_definition_without_the_one_key_cleared_mapping_decides_what_it_did_before` is red: `a definition the rule says is unaffected is refused: Some([SetAssignmentConflict { path: "operations.tag.set.extra", field: "extra", keywords: ["cleared", "increment"] }])`.
  - Two green probes: a clear plus a fulfillment `Remove` on one service/3 outcome (both named, replays); two operations emitting one event type fold, and a revision whose events disagree on `removed` is refused.
- **`adversary_clear_events.rs`**: three cases, all red. Each event is refused only for the undeclared `removed` key, for example `the published Woken message refuses the Woken event `wake` emits: event {…"removed":["snoozed_until"]…}, schema {"additionalProperties":false,…}`. The service/3 control is also red at base a70a22e5, run in a scratch copy of the base.
- **Store and Eventlog probes**: green. Three harness errors of mine were fixed before they went green: a payload reading the cleared field, a snapshot claiming completeness, and an event count.
- **Scratch probe** `build/scratch/adversary/xver/tests/xver.rs`: six cases linking this branch, 0.29.0 and 0.17.6 side by side. Two are red, as quoted in the (b) table.

**3. Suite**, run after the cases existed:
- `cargo test -p entity-core -p entity-store -p entity-surface -p entity-yaml --locked --no-fail-fast`: exit 101; 530 executed, 4 failed, all mine (`clear_adversary`, `adversary_clear_events`); 522 executed outside my files.
- `cargo +1.91.0 test -p entity-eventlog --all-features --locked --lib --test adversary_clear_records`: exit 0; 80 + 1 passed.
- So 602 executed before my cases and 611 after.

**4. Findings** (tree c466fcd7)

| # | origin | verdict | file:line | failing test | what reaches it |
|---|---|---|---|---|---|
| F1 | introduced | NEEDS-CHANGE | `crates/entity-surface/src/lib.rs:449` | `adversary_clear_events.rs::a_kernel_1_clear_event_is_admitted_by_the_asyncapi_message_it_is_published_under`, `…_openapi_domain_event_schema` | `entity generate docs` (`entity-cli/src/main.rs:410`) writes both contracts; every kernel/1 clear event fails them. Fix: declare an optional `removed` (array of unique strings) |
| F2 | pre-existing | CONFIRMED | `crates/entity-surface/src/lib.rs:449` | `…::a_service_3_remove_event_is_admitted_by_the_openapi_domain_event_schema`, red at base | any service/3 `Remove` event against the OpenAPI `DomainEvent` |
| F3 | introduced | INFEASIBLE | `docs/design/service-binding-boundary-v0.1.md:456` | `clear_adversary.rs::a_definition_without_the_one_key_cleared_mapping_decides_what_it_did_before` | the rule says only the one-key `cleared` mapping changes meaning, but `{cleared: true, increment: 1}` on a `json` field is now refused. No definition using it was found. Fix: narrow the sentence |
| F4 | introduced | INFEASIBLE | `docs/design/service-binding-boundary-v0.1.md:454` | `xver.rs::released_0_29_binary_on_a_branch_store_fails_closed_rather_than_deciding_the_clear_differently` | needs two installed versions on one store and a clear of an optional `json` or `object` field; no consumer flow found. Fix: refuse the clear on such fields, gate it behind a semantics version older builds refuse, or at least document the case |
| F5 | introduced | INFEASIBLE | `docs/design/service-binding-boundary-v0.1.md:456` | `xver.rs::a_0_17_6_event_fold_of_branch_events_fails_closed_or_matches_execute` | the doc claims older event folds refuse removal evidence; 0.17.x folds a different state instead. Consumers on 0.17.x: no `rehydrate` call found; aep-service is not established (needs-coordinator) |

Smaller detail: the doc's "refused by the byte comparison" is wrong for a `string` field. 0.29.0 refuses that one at step 9 instead.

**5. Attacked and could not break**
- The three store paths in (a).
- A clear combined with a fulfillment `Remove`.
- Events of one revision that disagree on `removed`.
- Defaults are applied only at creation (`runtime.rs:721`, `825`, `846`, `913`), so a cleared defaulted field stays absent.
- Identity fields are required, so they cannot be cleared.
- CLI and MCP refusals map by `kind()` plus the message (`main.rs:2001`), so no new match arm is needed.
- The refusals page lists all four new kinds.
- The CHANGELOG edit to the increment entry matches `SetAssignment::of`, and the increment adversary's check of that text is still green.
- `entity-yaml/tests/clear.rs` passes.

**6. Paths written outside the worktree:** none. Inside the git-ignored scratch dir `build/scratch/adversary/` (733M, which the coordinator can delete):
- archives of 0.29.0, 0.17.6 and the base; I added `package.workspace` to three manifests in those copies
- `er-base/target` (465M) and `xver/target` (189M)
- logs

```findings
- file: crates/entity-surface/src/lib.rs
  line: 449
  category: contract-drift
  severity: warning
  verdict: NEEDS-CHANGE
  origin: introduced
  message: the AsyncAPI message and OpenAPI DomainEvent schemas are closed objects without removed, so every kernel/1 clear event a definition emits is refused by the contract entity generate docs publishes for it
- file: crates/entity-surface/src/lib.rs
  line: 449
  category: contract-drift
  severity: note
  verdict: CONFIRMED
  origin: pre-existing
  message: the OpenAPI DomainEvent schema already refused service/3 fulfillment Remove events at the base
- file: docs/design/service-binding-boundary-v0.1.md
  line: 456
  category: contract-drift
  severity: note
  verdict: INFEASIBLE
  origin: introduced
  message: the compatibility rule says only the one-key cleared mapping changes meaning, but a cleared-plus-increment mapping that wrote an object at the base is now refused as SetAssignmentConflict
- file: docs/design/service-binding-boundary-v0.1.md
  line: 454
  category: contract-drift
  severity: warning
  verdict: INFEASIBLE
  origin: introduced
  message: a 0.29.0 binary given the same definition decides a clear of an optional json field as writing {"cleared":true} and appends it to a store this branch wrote, after which this branch's replay refuses the store
- file: docs/design/service-binding-boundary-v0.1.md
  line: 456
  category: contract-drift
  severity: note
  verdict: INFEASIBLE
  origin: introduced
  message: an entity-core 0.17.x reader drops removed from a clear event and its fold silently returns the cleared field still present, contrary to the note that older folds refuse removal evidence
```
