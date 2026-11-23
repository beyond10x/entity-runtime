---
format: aep.planning-md/3
id: story:kpi-baseline-matches-main
kind: story
status: draft
title: The KPI baseline holds the counters main produces
summary: '27 counters read ''improved: re-record'' and one is new; re-record checks/kpi/baseline.json with task kpi-record in its own change'
owner: entity-runtime
revision: 1
---
## Outcome

`checks/kpi/baseline.json` holds the counters the code on `main` produces, re-recorded with
`task kpi-record` in a change of its own, so `task kpi` reports `ok` for every counter on `main`
and a later regression is measured from today's values rather than from the pre-fix ones.

## Finding

KPI baseline left stale by the review fixes. Decision er-j D3 = A, "re-record baseline separately"
(master log `assignment.md` line 127). No single review finding id; the stale counters are
improvements made by the review fixes merged in #93.

## Evidence

- The baseline was recorded by kpi-er on the pre-fix code (`6c5563b9` "feat: CI-gated KPI baseline
  for the cost findings of the review"); the fix branches merged on top of it in #93.
- er-j round 7 `task kpi` on `feat/record-definition-digest` (`er-j/r7-kpi.log`): "no counter
  regressed", EXIT 0, with 27 counters marked `improved: re-record` and one `new`
  (`surface.asyncapi.outcome_messages` 0 -> 1). Examples: `file.execute.read_bytes.prior32`
  1,210,379 -> 605,237; `eventlog.projection.batch_blob_bytes.k32` 1,091,648 -> 23,170;
  `memory.commit_batch_one.alloc_bytes.n1024` 2,033,234 -> 3,727;
  `postgres.query_mid_page.shared_buffers.n16384` 382 -> 3;
  `graph.mermaid.endpoint_comparisons.chain300` 89,999 -> 898.
- `task kpi-record` (`Taskfile.yml:147`): "re-record checks/kpi/baseline.json (counters twice,
  timing over rounds)".
- While these stay stale, a regression of up to the recorded improvement passes the gate unseen
  (a counter at half its baseline can double back without failing).

## Acceptance

On `main` after the change, `task kpi` prints no counter with the verdict `improved: re-record`,
and the change's diff touches only `checks/kpi/baseline.json`.
