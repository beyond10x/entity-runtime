---
format: aep.planning-md/3
id: story:er-store-executor-contracts
kind: story
status: implemented
title: Execute storage and asynchronous executor contracts
relations:
- derived_from: executable-system-specification:er-library-contracts
- serves: vision:O2
scope:
- confidence: cited
  path: checks/ess-conformance/src/executor.rs
- confidence: cited
  path: checks/ess-conformance/src/store.rs
- confidence: cited
  path: ess/domains/executor.yaml
- confidence: cited
  path: ess/domains/store.yaml
- confidence: cited
  path: ess/scenarios/executor
- confidence: cited
  path: ess/scenarios/store
revision: 9
transitions:
- {from: "draft", to: "proposed", at: "2026-09-28T02:07:21Z", actor: "human:timo", revision: 2, imported: true}
- {from: "proposed", to: "active", at: "2026-09-28T02:07:27Z", actor: "human:timo", revision: 3, imported: true}
- {from: "active", to: "implemented", at: "2026-09-28T03:25:11Z", actor: "human:timo", revision: 6, decided_on: {"recorded":{"test_result":1,"review_outcome":1}}, imported: true}
---
# Store and executor executable contracts

The accepted plan specifies the synchronous memory/file contracts, asynchronous recorded-memory ports, history verification, projections, library migration and executor orchestration. `ess/domains/store.yaml` and `executor.yaml`, corresponding components and authored scenarios are composed by `ess/ess-inputs.yaml`. `docs/ess/store-traceability.md` and `executor-traceability.md` retain requirement IDs, normative clauses, actual APIs and named acceptance cases.

The adapters invoke MemoryStore, FileStore and MemoryRecordedStore directly. Literal returned documents include actual state, event/history evidence and receipts. FileStore scenarios explicitly cover only its single-subject atomicity boundary. Async cases distinguish absence/unavailability, retry/conflict, ordered mixed appends, observation revisions, imported evidence and complete-snapshot assurance. Executor cases exercise request validation, recovery using saved definitions and original receipts, current-authority ordering, cancellation and uncertain commits without duplicate effects. Optional refusal-recording and fork/merge ports are explicit fixtures; no successful persistence capability is attributed to a provider that lacks it.

The final worker suite passed. `docs/ess/evidence/mutations/store/` and `executor/` retain exact failing production mutations, original suite associations and implementation identity. `docs/ess/evidence/review/store-executor-adversary.md` records independent refusal-recording and projection-version probes; the final fixtures reject both mutations and the restored run passes. Production files are restored and unchanged.

The full integrated gate and final suite evidence passed and are retained under `docs/ess/evidence/final/release/`. AEP admitted the exact current pair and the original format blocker is cleared. The exact archived typed coverage earned the conforming transition, cleared blocker:current-coverage-lifecycle-admission and enabled the declared ESS authority. No runtime behavior correction or additional provider scope is claimed.
