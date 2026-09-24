---
format: aep.planning-md/2
id: review-result:operation-field-fulfillment-source-pass-1
kind: review-result
status: active
title: 'Complete operation-field source pass one: exact legacy retry conflict'
owner: Independent source reviewer
relations:
- reviews: story:service-operation-field-fulfillment
revision: 1
---
Independent complete operation-field source examination, pass 1 of 2.

Submitted bff1d7f8953aa932ca538913eb3b8e7ed66fbdfe; all 22 source hashes verified. Verdict needsrevision: one introduced exact-retry defect. Public Executor::execute accepts the request shape; recover_existing/match_committed runs before ordinary decision validation and comparison for er.request/1 through /3 omits new fulfillment coordinates. A compiled regression committed an empty-map service/1 request and retried its durable ID with an undeclared Set action; it received replayed success instead of RecordConflict. Isolated and suite commands each reached the assertion and exited 101.

The complete old-domain retry class is affected; /4 includes its map and is not implicated. Correction must preserve old literal bytes and refuse differing invalid retries. Reviewer added only crates/entity-executor/tests/operation_fulfillment_review_1.rs; source and prior gates were examined without source edits. The reviewer assignment and lease are closed. One final whole source examination remains after correction, no review reset.

```findings
- file: crates/entity-executor/src/lib.rs
  line: 721
  category: concurrency
  severity: blocker
  verdict: NEEDS-CHANGE
  origin: introduced
  message: committed er.request/1 through /3 retries omit the newly supplied fulfillment map, so Executor::execute accepts a request the ordinary path rejects as an exact replay
```
