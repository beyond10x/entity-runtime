---
format: aep.planning-md/3
id: blocker:er-ess-suite27-evidence
kind: blocker
status: cleared
title: AEP cannot admit direct-return ESS suite evidence
relations:
- serves: vision:O2
- blocks: executable-system-specification:er-library-contracts
withholds: ess_conformance
revision: 3
transitions:
- {from: "open", to: "cleared", at: "2026-09-28T08:35:02Z", actor: "human:timo", revision: 3}
---
# Direct-return ESS evidence admission

The original ESS 0.37 prototype needed suite/26 or inventory suite/27, which AEP 0.61.1 refused with UnsupportedSuiteVersion. That exact refusal and the original evidence remain under docs/ess/evidence/prototype-0.37. No suite was relabeled or weakened to bypass it.

The operator subsequently authorized the narrow compatibility work as part of ER's mainline integration, current planning-store adoption, cleanup and release. ESS 0.38 had already assigned different semantics to formats16/26/27, so beyond10x/ess#185 ports direct returns onto source17 and suite28/29 at ac6fc6fe2f39b43f016e4d3a9edecb3573f7d6a1, preserving all old meanings.

AEP's separately governed story:admit-er-direct-return-evidence and beyond10x/aep#61 add a closed reader for ER's actual response types. The reader preserves original suite bytes, exact numeric lexemes, model identity, scenario counts, inventory and parent-chain checks. Unsupported types and unknown fields remain refusals.

The fresh ER implementation run passed all 414 declared scenarios with zero failures, errors, unsupported observations or skips. The fixed CLI successfully imported its exact report and inventory suite as ess_conformance_coverage_v1 evidence against executable-system-specification:er-library-contracts. The model digest is a13e6c07be82c913363ea52b8042c6e379343acfda7fcc03a212c0942569da5d and the suite digest is sha256:a6f8ceb64b95f76d33f75c741fb3b67770ad6130243504b77761921be6832548. The original run and admission output remain under docs/ess/evidence/final. A separate final release run records any subsequent release-manifest identity changes without rewriting that evidence.

This resolves the reader admission gap. Full release gates, upstream/mainline merges and publication remain governed by story:adopt-current-plan-and-release-contracts; clearing this blocker does not assert those future actions succeeded.
