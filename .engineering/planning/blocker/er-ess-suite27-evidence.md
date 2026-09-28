---
format: aep.planning-md/2
id: blocker:er-ess-suite27-evidence
kind: blocker
status: open
title: AEP cannot admit direct-return ESS suite evidence
relations:
- serves: vision:O2
- blocks: executable-system-specification:er-library-contracts
withholds: ess_conformance
revision: 1
---
# ESS suite evidence admission

The canonical ER checker needs direct library return observations, added by the separately governed ESS story `direct-library-return-observations`. Those observations require ESS suite/26, or suite/27 with declared coverage. AEP 0.61.1 refuses the exact successful runner report paired with its original suite bytes:

```text
error: UnsupportedSuiteVersion at $.suite.version: ess-conformance/27
```

This blocks machine-validated `ess_conformance` evidence and the specification's conforming status. It does not indicate an ER runtime behavior failure. Do not relabel the suite, strip observations, or substitute manually asserted conformance evidence.

Resolution requires an AEP report-reader compatibility change that validates the new suite vocabulary and preserves exact report/suite association, counts and model identity. The operator has been asked whether to include this third-repository compatibility work in the ER/ESS implementation effort. Continue unaffected conformance, mutation, correctness-gate and review work while scope is pending.
