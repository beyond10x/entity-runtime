---
format: aep.planning-md/3
id: review-result:er-current-store-release-review
kind: review-result
status: active
title: Current planning store and bot release review
relations:
- reviews: story:adopt-current-plan-and-release-contracts
revision: 1
---
unit: ER current-store migration and bot release preparation
verdict: no actionable finding in the bounded review; not release approval
cases: 0 → 3 reviewer probe cases executed; red 0; repository gate not rerun
origin: introduced 0 / pre-existing 0 / undecided 0
wrote-outside-worktree: four assigned scratch paths listed below; managed lease metadata only otherwise
needs-coordinator: final upstream pins, full gate, actual CI and bot release verification remain coordinator work

`git --no-pager diff --stat` for the reviewed source/configuration paths showed the following existing coordinator changes. The reviewer made no repository edits, including no planning edits:

```text
 .engineering/project.yaml       |  2 +-
 .github/workflows/planning.yml  | 10 ++--------
 .github/workflows/release.yml   | 29 +++++++++++++++--------------
 AGENTS.md                       | 19 +++++++++++++++++--
 Taskfile.yml                    | 10 +++-------
 crates/entity-xtask/src/main.rs |  4 ++--
 6 files changed, 40 insertions(+), 34 deletions(-)
```

Reviewed the integration working tree at HEAD `701c071ca74fc50a117407df2640a585d41f11c8`, against that HEAD and published baseline `dcb58d2`. The review covered migration records, planning configuration and CI, the local AEP version guard, release workflow, and repository release guidance. Changing upstream ESS/AEP implementations, final version bump, final evidence admission, and final authority adoption were outside this unit.

The retained official migrator verification reports preservation of 120 artifacts, 218 transitions, and 111 evidence files. All 120 current documents declare `aep.planning-md/3`; exactly 218 transitions remain marked imported. The current totals of 220 transitions and 112 evidence files include the release story's two subsequent moves and its additional evidence. The project selects `aep.project/5` with the Git store. HEAD retains 6,363 former state paths in Git history. I inspected the migrator's dry-run, verification and current-validation logs rather than rerunning migration or issuing AEP planning commands. Six pre-existing review-record warnings remain visible in the retained validation log; the validator concluded valid.

Three explicit Rust probe cases drove the actual repository `entity-xtask` binary. AEP 0.61.1 was refused with exit 1 and the literal 0.63.1 minimum diagnostic; installed AEP 0.63.1 succeeded; empty shell resolution was refused with exit 2. All three passed. The Taskfile resolves the binary before Cargo changes PATH, and the next command uses the same shell environment. The earlier retained floor mutation demonstrates that restoring 0.26.0 admits the incompatible old CLI. Planning CI builds its pinned AEP and uses the current validation command without the retired `--against` option. The coordinator has already identified the final upstream pin update as pending.

The release workflow has read-only contents permission and no release-creation step. Its bundle job depends on provenance, the reusable gate, and the entire five-target matrix. Each build contributes one archive containing the CLI and documentation/license files. Artifact selection `build-*` excludes both the gate's ESS evidence and any prior release bundle. The pinned download action lists the latest artifacts before filtering and fails on digest mismatch by default; this was checked in its [exact source](https://github.com/actions/download-artifact/blob/3e5f45b2cfb9172054b4087a40e8e0b5a5461e7c/src/download-artifact.ts). No Actions rerun was initiated in this review.

Bundle layout is `notes.md` at the artifact root and `dist/` containing five archives plus `SHA256SUMS`, because multiple upload paths retain their common-root structure. This follows the [pinned upload action's documented path behavior](https://github.com/actions/upload-artifact/blob/043fb46d1a93c77aae656e7c1c64a875d1fc6a0a/README.md#upload-using-multiple-paths-and-exclusions). Bot publication should consume that layout from the exact successful run/artifact ID, verify the complete six-asset set and archive checksums, then verify the published asset digests, bot author and annotated tag. Those are already explicit coordinator responsibilities and release guidance. A delayed retry must still verify all five archives; successful historical matrix jobs alone do not establish that retained artifacts remain available.

The completion boundary matches repository and workspace instructions: a queued tag or successful build is insufficient; the exact release and required artifacts must be verified. Atlas and website publication are asynchronous and do not block this source release. No personal-account GitHub write is proposed by the reviewed workflow. Connectors discovery returned `invalid_configuration`; the coordinator was informed before the bounded read of official public action documentation/source. No GitHub mutation occurred.

Review identity (SHA-256):

```text
3f3c6bd43b2aec698717fea1a7cfd551cd14522d772883b3be2bcfc9ce0e2ecd  .github/workflows/release.yml
291723ebb0994166f9b28d48b252275cb983f20e6158785f65541b3f989c50e9  .github/workflows/planning.yml
dd45880546ee79a17ddda217adaddac45c5939081011cadf4b08e791dadedb3c  Taskfile.yml
74a7762fb0f7e1f1d064e9848fe4979ccbd126fa9b99a33eb45d875a05d11cae  crates/entity-xtask/src/main.rs
767843e7db335b15ff1dadcb48b0b6a68689a618e325a9cf35b4219abf2e61e9  .engineering/project.yaml
```

All reviewer-written scratch paths (`$TASK_SCRATCH` is the coordinator-assigned ER release scratch directory; its exact local mapping is retained in a separate, non-publishable scratch manifest):

- `$TASK_SCRATCH/review-version-guard.rs`
- `$TASK_SCRATCH/review-version-guard`
- `$TASK_SCRATCH/review-version-guard.log`
- `$TASK_SCRATCH/er-release-review.md`

The reviewer released its own lease. After review, the coordinator removed the stale workflow comment claiming CI holds a publication token; that comment-only edit does not change the reviewed release behavior.

```findings
[]
```
