---
format: aep.planning-md/2
id: story:er-core-conformance-integration
kind: story
status: active
title: Core contracts and the admitted Rust conformance gate
relations:
- derived_from: executable-system-specification:er-library-contracts
- serves: vision:O2
scope:
- confidence: cited
  path: .github/workflows/gate.yml
- confidence: cited
  path: Taskfile.yml
- confidence: cited
  path: checks/ess-conformance/src/core.rs
- confidence: cited
  path: checks/ess-conformance/src/main.rs
- confidence: cited
  path: checks/ess-conformance/src/target.rs
- confidence: cited
  path: ess/domains/core.yaml
- confidence: cited
  path: ess/ess-inputs.yaml
- confidence: cited
  path: ess/scenarios/core
revision: 5
---
# Core contracts and admitted conformance integration

The accepted ER plan covers kernel/1 and service/1–3 through actual Rust library return observations. `ess/ess-inputs.yaml` is the canonical composition; `ess/domains/core.yaml` owns lossless document types and pure-kernel commands. Existing service and recording entry points remain available, and the service-binding inventory remains diagnostic.

`checks/ess-conformance` is a standalone Rust/clap workspace with its own lockfile and one exact ESS revision, including the separately governed direct-return extension. No production dependency changes are introduced. Adapters translate public inputs and actual outputs without deriving expected results from the implementation. Generated and authored suites are admitted and executed by ESS's Rust runner. The checker retains exact suites, reports, implementation hashes and actual response transcripts. Build-time source identity refuses stale executables; coverage version 2 requires explicit review for changes to a scenario's entire contract, including same-ID assertion removal. Evidence time is sampled at the checker edge; fixture times remain explicit inputs.

`docs/ess/core-traceability.md` connects requirement IDs, design clauses, public APIs, implementation and named scenarios. The clause audit added registration, default/constraint/reference, service branch, exact numeric/identity, precondition/invariant, fulfillment, direct/prepared continuation, deterministic record and complete/legacy replay cases. Static purity and dependency checks remain in the existing Rust suite.

`docs/ess/evidence/mutations/core/` retains the exact red production mutation. `docs/ess/evidence/review/` retains the independent initial review and successful correction recheck: stale sources refuse execution, always-refusing Rehydrate fails a successful replay scenario, and removing an assertion without changing its ID requires coverage review. `docs/ess/evidence/final/` is the intended final full-suite evidence location. `task check` and CI run deterministic regeneration and all admitted scenarios, with independent MSRV coverage for the checker.

Final acceptance requires the full repository gate and the declared complete conformance report. AEP suite/27 admission is blocked by blocker:er-ess-suite27-evidence. This body does not claim authority adoption or a conforming specification until that evidence can be admitted. Native ESS Binary64 remains unsupported; the runtime's actual exact JSON inputs and returns are the explicit lossless document contract, not weakened native numeric assertions.
