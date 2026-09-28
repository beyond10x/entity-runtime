---
format: aep.planning-md/2
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
revision: 6
---
# Shell and query executable contracts

The accepted scope covers shared synchronous shell operations over supplied MemoryStore and FileStore, plus containment and pagination through query memory and ordered-input APIs. `ess/domains/shell.yaml` and `query.yaml`, their components and authored scenarios are composed by `ess/ess-inputs.yaml`. `docs/ess/shell-traceability.md` and `query-traceability.md` map normative/public contracts and actual implementation evidence to named scenarios.

Shell scenarios cover create/get/list/events/execute, missing subjects, stale intent and the second commit-time revision check, exact retries after state advances, recording validation, provider failures and stable error classification. Fault and race arrangements delegate successful operations to the actual provider. Query scenarios cover recursive containment, exact numbers, missing fields, page limits and identity order, full traversal of a fixed dataset, malformed and foreign cursors, changed page size and invalid consumed streams. No snapshot-isolation guarantee is inferred.

The adapters preserve lossless JSON and expose actual API results. Scenario state and FileStore directories are isolated. `docs/ess/evidence/mutations/shell/` and `query/` retain exact production mutations that fail their named scenarios; production files were restored. The worker's restored complete suite passes. Integrated reports are retained under `docs/ess/evidence/final/`; independent review is retained under `docs/ess/evidence/review/`.

Acceptance requires the full repository gate and complete declared execution, with no failure, error, unsupported observation or skip. AEP suite/27 evidence admission and final normative adoption remain explicitly blocked by blocker:er-ess-suite27-evidence; this implementation does not change ER behavior or add other provider scope.
