---
format: aep.planning-md/3
id: story:read-invoke-cpu-at-or-under-0-32-0
kind: story
status: implemented
title: A connectors read invoke costs no more CPU on Entity Runtime than on 0.32.0
summary: 0.33.1 read invoke 171-177 CPU ms vs 0.32.0 138-143 (connectors, 2026-11-23)
relations:
- serves: vision:O2
revision: 4
transitions:
- {from: "draft", to: "proposed", at: "2026-11-23T23:19:56Z", actor: "human:timo", revision: 2, decided_on: {"recorded":{"test_result":1}}}
- {from: "proposed", to: "active", at: "2026-11-23T23:19:56Z", actor: "human:timo", revision: 3, decided_on: {"recorded":{"test_result":1}}}
- {from: "active", to: "implemented", at: "2026-11-23T23:19:56Z", actor: "human:timo", revision: 4, decided_on: {"recorded":{"test_result":1}}}
---
## Outcome

A connectors read invoke on Entity Runtime 0.34.0 costs no more CPU than on 0.32.0 (138-143 ms
per invoke on the measurement below), with `er.execute_batch` wall time and blob bytes no worse
than 0.33.1.

## Evidence (connectors session, 2026-11-23 17:56-17:59 CET)

- Setup: connectors-host lib test `read_invoke_cost_by_store_size`, release profile, a store of
  50 recorded events, 100 read invokes per run, 3 runs per pin, the three binaries at once in
  rotating start order, host load 52-67.
- `er.execute_batch` ms per read invoke (3 calls per invoke), median of 3 rounds: 0.32.0 122.9,
  0.33.0 129.5, 0.33.1 110.3.
- Blob bytes per read invoke: 78,488 (0.32.0), 24,500 (0.33.0), 24,500 (0.33.1).
- CPU ms per read invoke (whole invoke, median of 100), rounds 1-3: 0.32.0 143, 140, 138;
  0.33.0 192, 188, 182; 0.33.1 177, 175, 171. About 25 % more CPU than 0.32.0, not split by
  call kind.
- 0.33.1 fixed part of the 0.33.0 slowdown (cause 23cd3e50, fix 7ad58845: definition decode memo,
  per-record charge reuse).

## Acceptance

- A probe or KPI counter in `checks/kpi` that reproduces the CPU difference between 0.32.0 and
  the head on the same shape (50 recorded events, read invokes of 3 `execute_batch` calls),
  attributed by call kind.
- The head's CPU per read invoke at or under 0.32.0's on that probe, measured with interleaved
  A/B rounds and the load average stated; no counter regresses.
