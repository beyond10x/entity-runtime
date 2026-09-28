---
format: aep.planning-md/3
id: story:er-shell-query-contracts
kind: story
status: implemented
title: Execute the shell and query library contracts
relations:
- derived_from: executable-system-specification:er-library-contracts
- serves: vision:O2
scope:
- confidence: cited
  path: checks/ess-conformance/src/query.rs
- confidence: cited
  path: checks/ess-conformance/src/shell.rs
- confidence: cited
  path: ess/domains/query.yaml
- confidence: cited
  path: ess/domains/shell.yaml
- confidence: cited
  path: ess/scenarios/query
- confidence: cited
  path: ess/scenarios/shell
revision: 9
transitions:
- {from: "draft", to: "proposed", at: "2026-09-28T02:06:57Z", actor: "human:timo", revision: 2, imported: true}
- {from: "proposed", to: "active", at: "2026-09-28T02:07:03Z", actor: "human:timo", revision: 3, imported: true}
- {from: "active", to: "implemented", at: "2026-09-28T03:25:19Z", actor: "human:timo", revision: 6, decided_on: {"recorded":{"test_result":2,"review_outcome":1}}, imported: true}
---
# Shell and query executable contracts

The accepted scope covers shared synchronous shell operations over supplied MemoryStore and FileStore, plus containment and pagination through query memory and ordered-input APIs. `ess/domains/shell.yaml` and `query.yaml`, their components and authored scenarios are composed by `ess/ess-inputs.yaml`. `docs/ess/shell-traceability.md` and `query-traceability.md` map normative/public contracts and actual implementation evidence to named scenarios.

Shell scenarios cover create/get/list/events/execute, missing subjects, stale intent and the second commit-time revision check, exact retries after state advances, recording validation, provider failures and stable error classification. Fault and race arrangements delegate successful operations to the actual provider. Query scenarios cover recursive containment, exact numbers, missing fields, page limits and identity order, full traversal of a fixed dataset, malformed and foreign cursors, changed page size and invalid consumed streams. No snapshot-isolation guarantee is inferred.

The adapters preserve lossless JSON and expose actual API results. Scenario state and FileStore directories are isolated. `docs/ess/evidence/mutations/shell/` and `query/` retain exact production mutations that fail their named scenarios; production files were restored. The worker's restored complete suite passes. Integrated reports are retained under `docs/ess/evidence/final/`; independent review is retained under `docs/ess/evidence/review/`.

The full repository gate and complete declared execution passed, with no failure, error, unsupported observation or skip. AEP admitted the final suite/29 and report under `docs/ess/evidence/final/release/`. The original format blocker is cleared; archived typed coverage now earns the conforming transition and normative adoption, clearing blocker:current-coverage-lifecycle-admission. This implementation does not change ER behavior or add other provider scope.
