---
format: aep.planning-md/3
id: story:entity-eventlog-tests-under-budget
kind: story
status: implemented
title: entity-eventlog tests run under 5 s each, killed at 60 s
summary: provider_facades publication crash 23.1 s, small_store_cost owning build 18.2 s, seeded_open advanced model 15.2 s, small_store_cost warm handle 14.2 s; no nextest kill at 60 s
owner: entity-runtime
relations:
- serves: vision:O2
revision: 4
transitions:
- {from: "draft", to: "proposed", at: "2026-11-23T21:53:44Z", actor: "human:timo", revision: 2, decided_on: {"recorded":{"test_result":1}}}
- {from: "proposed", to: "active", at: "2026-11-23T21:53:45Z", actor: "human:timo", revision: 3, decided_on: {"recorded":{"test_result":1}}}
- {from: "active", to: "implemented", at: "2026-11-23T21:53:45Z", actor: "human:timo", revision: 4, decided_on: {"recorded":{"test_result":1}}}
---
## Outcome

Every `entity-eventlog` test finishes in under 5 s, and the test runner kills a test at 60 s and
prints the ten slowest, as `~/beyond10x/AGENTS.md` "Tests stay fast" requires.

## Finding

Test budget defects (`~/beyond10x/AGENTS.md` "Tests stay fast": a test over 5 s is named with its
time; a step over budget is a defect fixed before the next feature). Found while gating #94; no
review finding id.

## Evidence

er-j round 7 nextest run of `entity-eventlog` (`er-j/r7-times.log`, 253 tests, Summary 74.6 s):

| time | test | file |
|---|---|---|
| 23.1 s | `provider_facades::file_facade_groups_are_process_atomic_and_survive_a_publication_crash` | `crates/entity-eventlog/tests/provider_facades.rs:799` |
| 18.2 s | `adapter::small_store_cost::adversary_an_owning_build_answers_every_rewritten_capture_as_a_borrowing_build` | `crates/entity-eventlog/src/adapter/small_store_cost.rs:597` |
| 15.2 s | `adapter::seeded_open::a_model_advanced_by_appended_events_is_the_model_a_whole_build_produces` | `crates/entity-eventlog/src/adapter.rs:6617` |
| 14.2 s | `adapter::small_store_cost::adversary_a_warm_handle_answers_every_rewritten_capture_as_a_cold_build` | `crates/entity-eventlog/src/adapter/small_store_cost.rs:461` |

- Earlier: er-d2 measured `provider_facades` at 51.8 s and the `entity-eventlog` test time at 166 s
  (master log `assignment.md` line 102: "over 120 s, provider_facades 51.8 s pre-existing ->
  story").
- `0a661071` (A2): "The three model-rebuild unit tests take 78-93 s together on this tree and
  80-131 s at HEAD (one thread each, host load 80-145 on 20 cores)".
- No `.config/nextest.toml` in the repository, so no runner `slow-timeout` with
  `terminate-after` kills a test at 60 s yet.

## Acceptance

A nextest run of `entity-eventlog --all-features` under a committed `.config/nextest.toml` with a
60 s `slow-timeout`/`terminate-after` lists none of the four tests above at or over 5 s in its ten
slowest.
