---
format: aep.planning-md/3
id: review-result:er-published-release-verification
kind: review-result
status: active
title: Independent verification of the published ER 0.25.1 release
relations:
- reviews: story:adopt-current-plan-and-release-contracts
revision: 1
---
approve

The [published release](https://github.com/beyond10x/entity-runtime/releases/tag/0.25.1) matches retained metadata and all six asset digests; archive checksums pass. The exact [release run](https://github.com/beyond10x/entity-runtime/actions/runs/36409629222) completed successfully across all ten jobs.

The README accurately separates the draft [AEP PR 61](https://github.com/beyond10x/aep/pull/61) blocker. AEP removal and preserved commits match its archive manifest; remaining ER cleanup is explicitly pending publication.

No builds, mutations, or publication performed. Review lease released.

```findings
[]
```
