---
format: aep.planning-md/3
id: blocker:current-coverage-lifecycle-admission
kind: blocker
status: cleared
title: Specification lifecycle does not yet accept current typed coverage
relations:
- blocks: executable-system-specification:er-library-contracts
withholds: ess_conformance
revision: 3
transitions:
- {from: "open", to: "cleared", at: "2026-09-28T09:29:30Z", actor: "human:timo", revision: 2}
---
The original move refused because the legacy lifecycle required ess_conformance while current reports import as ess_conformance_coverage_v1. AEP story:bind-current-coverage-to-specification-lifecycle now retains and re-admits exact suite/report originals, checks current-model complete whole-system coverage and records derived eligibility separately without changing the evidence kind. Its independent adversarial review and full repository gate passed.

On 2026-09-28 the actual ER pair was imported with archived originals and the CLI moved executable-system-specification:er-library-contracts from validated to conforming. The transition records ess_conformance_from_coverage: 1 beside three descriptive coverage records. docs/ess/evidence/final/release/aep-lifecycle-transition.log retains the successful result; the exact source bytes and implementation identity are unchanged. The blocker is cleared by observed execution, not an asserted legacy evidence count.
