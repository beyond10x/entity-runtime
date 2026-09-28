---
format: aep.planning-md/3
id: blocker:current-coverage-lifecycle-admission
kind: blocker
status: open
title: Specification lifecycle does not yet accept current typed coverage
relations:
- blocks: executable-system-specification:er-library-contracts
withholds: ess_conformance
revision: 1
---
ER's final exact suite29/report2 is admitted successfully as ess_conformance_coverage_v1. The actual conforming move refuses because the shipped lifecycle requests the legacy ess_conformance kind. No replacement record is asserted and the specification remains validated. Upstream AEP story:bind-current-coverage-to-specification-lifecycle tracks the additive typed compatibility lane and negative guards. Authority adoption waits for actual CLI success against held current coverage and the correct model digest.
