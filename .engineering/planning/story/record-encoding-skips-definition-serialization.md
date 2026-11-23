---
format: aep.planning-md/3
id: story:record-encoding-skips-definition-serialization
kind: story
status: implemented
title: Encoding a decision record does not serialize the whole definition to discard it
summary: About 12 % of read-invoke CPU after the 0.33.1 fix
owner: entity-runtime
relations:
- serves: vision:O2
revision: 4
transitions:
- {from: "draft", to: "proposed", at: "2026-11-23T23:19:52Z", actor: "human:timo", revision: 2, decided_on: {"recorded":{"test_result":1}}}
- {from: "proposed", to: "active", at: "2026-11-23T23:19:53Z", actor: "human:timo", revision: 3, decided_on: {"recorded":{"test_result":1}}}
- {from: "active", to: "implemented", at: "2026-11-23T23:19:53Z", actor: "human:timo", revision: 4, decided_on: {"recorded":{"test_result":1}}}
---
## Outcome

Encoding a decision record no longer serializes the whole entity definition into a value only to discard it, on the read and the write path, so a read invoke of the connectors shape spends no CPU on definition serialization.

## Evidence

- Measured during the 0.33.1 fix of the 0.33.0 execute slowdown (connectors issue 267: `er.execute_batch` 315-351 -> 420 ms per invoke, test profile). After the fix (a decode memo per definition digest and a per-record memory charge), perf on the debug build attributes about 12 % of the remaining CPU of one read invoke to serializing the definition while encoding a record.
- Bench shape: Eventlog SQLite, `ProviderTracked`, 9 definitions totalling 137,259 bytes, 50 seed events; one invoke = 3 single-action `execute_batch` calls plus `lookup_batch` and `read_history`.

## Acceptance

A counting test shows that encoding a record of a by-digest form (`er.record/5`) performs no serialization of the definition document; the KPI counter for that work is 0 and gated in `checks/kpi/baseline.json`.
