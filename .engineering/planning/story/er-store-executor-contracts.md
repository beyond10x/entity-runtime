---
format: aep.planning-md/2
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
revision: 6
---
# Store and executor executable contracts

The accepted plan specifies the synchronous memory/file contracts, asynchronous recorded-memory ports, history verification, projections, library migration and executor orchestration. `ess/domains/store.yaml` and `executor.yaml`, corresponding components and authored scenarios are composed by `ess/ess-inputs.yaml`. `docs/ess/store-traceability.md` and `executor-traceability.md` retain requirement IDs, normative clauses, actual APIs and named acceptance cases.

The adapters invoke MemoryStore, FileStore and MemoryRecordedStore directly. Literal returned documents include actual state, event/history evidence and receipts. FileStore scenarios explicitly cover only its single-subject atomicity boundary. Async cases distinguish absence/unavailability, retry/conflict, ordered mixed appends, observation revisions, imported evidence and complete-snapshot assurance. Executor cases exercise request validation, recovery using saved definitions and original receipts, current-authority ordering, cancellation and uncertain commits without duplicate effects. Optional refusal-recording and fork/merge ports are explicit fixtures; no successful persistence capability is attributed to a provider that lacks it.

The final worker suite passed. `docs/ess/evidence/mutations/store/` and `executor/` retain exact failing production mutations, original suite associations and implementation identity. `docs/ess/evidence/review/store-executor-adversary.md` records independent refusal-recording and projection-version probes; the final fixtures reject both mutations and the restored run passes. Production files are restored and unchanged.

The full integrated gate and final suite evidence are retained under `docs/ess/evidence/final/` when complete. Specification adoption remains blocked by AEP's suite/27 admission gap, separately recorded as blocker:er-ess-suite27-evidence. No runtime behavior correction or additional provider scope is claimed.
