---
format: aep.planning-md/3
id: story:connectors-read-invoke-cpu-rerun-on-0-34-0
kind: story
status: draft
title: Connectors' read-invoke CPU on 0.34.0 is at or under 0.32.0
summary: Connectors reruns its measured_invoke on 0.34.0; the read_invoke probe did not reproduce its +25 %.
revision: 1
---
## Outcome

Connectors' read-invoke measurement (`measured_invoke` in connectors-host
`store_cost_tests.rs`) runs on Entity Runtime 0.34.0, and its process CPU per invoke is at or
under the same measurement on 0.32.0.

## Why

Connectors measured 0.33.1 at +25 % CPU per invoke while its `execute_batch` wall time went down
(110 vs 123 ms). The `read_invoke` probe in `checks/kpi/src/invoke.rs` (nine definitions, 99 KB,
30 invokes of three `execute_batch`, one `lookup_batch`, one `read_history`) does not reproduce
it: on that shape 0.32.0 costs 63.1 ms CPU per invoke and the head of
`story:read-invoke-cpu-at-or-under-0-32-0` 28.6 ms. The extra CPU sits in work the probe does not
do. Candidates: connectors' metadata steps (inspect, activate), checkpoint writes per invoke, the
fresh-process authority read, other threads.

## Acceptance

- Connectors' measurement, run on 0.32.0 and 0.34.0 in interleaved rounds on the same host,
  reports 0.34.0 CPU per invoke at or under 1.05x of 0.32.0.
- If it does not: the probe in `checks/kpi/src/invoke.rs` gains the step that differs, and it
  reproduces the difference before any fix.

## Scope

- `checks/kpi/src/invoke.rs` only if the probe has to grow.
