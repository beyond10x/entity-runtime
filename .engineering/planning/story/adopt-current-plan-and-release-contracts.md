---
format: aep.planning-md/3
id: story:adopt-current-plan-and-release-contracts
kind: story
status: implemented
title: Adopt current planning storage and release executable ER contracts
relations:
- decomposes: initiative:entity-runtime
- serves: vision:O2
scope:
- confidence: cited
  path: .engineering
- confidence: cited
  path: .github/workflows/planning.yml
- confidence: cited
  path: .github/workflows/release.yml
- confidence: cited
  path: AGENTS.md
- confidence: cited
  path: Cargo.lock
- confidence: cited
  path: Cargo.toml
- confidence: cited
  path: Taskfile.yml
- confidence: cited
  path: checks/ess-conformance
- confidence: cited
  path: crates
- confidence: cited
  path: crates/entity-xtask
- confidence: cited
  path: docs/ess
- confidence: cited
  path: ess
revision: 17
transitions:
- {from: "draft", to: "proposed", at: "2026-09-28T07:50:10Z", actor: "human:timo", revision: 6}
- {from: "proposed", to: "active", at: "2026-09-28T07:50:10Z", actor: "human:timo", revision: 7}
- {from: "active", to: "implemented", at: "2026-09-28T10:41:15Z", actor: "human:timo", revision: 17, decided_on: {"recorded":{"test_result":8,"review_outcome":2}}}
---
## Acceptance

The executable five-library contract is conforming under admitted exact ESS evidence, ER uses the latest released Git-native AEP planning layout and a current compatible pinned CLI in CI, all work is merged into origin/main, the next ER version has a verified annotated tag, successful required checks and published release artifacts, and this effort's managed worktrees are cleaned with recovery proof.

## Authorization

The operator explicitly requested mainline integration, upgrading ER's own planning store, cleanup and a new ER version. That includes resolving the previously pending narrow AEP evidence-reader compatibility work, integrating the ESS extension with current upstream, importing successful evidence and completing the specification's authority adoption. Production ER behavior corrections remain outside this effort.

## Scope and sequence

Use AEP's documented Eventlog-to-Git migrator with --verify, preserve every artifact, transition, body and evidence count, and only then use current AEP for further writes. Update the planning workflow and version gate for aep.project/5 without --against. Reconcile the direct-return ESS extension with 0.38.0's already-allocated format versions, then pin the published compatible extension consistently in the isolated checker. Regenerate exact model/suite/report evidence and preserve historical mutation evidence. Resolve the admission blocker through actual successful report import before adopting authority. Run the full ER gate, plan validation and CI, merge through bot authority, cut the next patch version because public runtime behavior is unchanged, and verify release assets before cleanup.

The existing ER initiative and executable-system-specification:er-library-contracts retain the broader behavioral plan and evidence. Release documentation publication is asynchronous and outside the source-release completion boundary.

## Release outcome

ER PR 47 merged into origin/main at 72455539c0756290d03fd5ef28b30512a729c457. The bot's annotated 0.25.1 tag points there; release run 36409629222 passed every required gate and platform build. The bot published release 398152467 on 2026-09-28 after all five archives and SHA256SUMS matched the exact run bundle. Names, sizes and digests were verified again after publication. The native Linux binary reports entity 0.25.1. Durable observed identities and checksums are in docs/ess/evidence/release-0.25.1/.

The five-library specification is conforming under the retained 414-case passing evidence. ER now uses aep.project/5 Git storage, preserving the migration's artifact, transition and evidence counts. CI and the final store validation use published, fully checked AEP revision 18a18a3f1cfa110dc5c9a675b3e9956f74c29bb7, including current committed-history guards.

ESS PR 185 is merged. AEP PR 61 remains draft under a separate recorded delivery blocker: later upstream documentation history contains a GitHub branch-update commit Gates cannot attribute as an exact merged-PR commit. Publication was stopped. The published AEP revision ER consumes is unaffected; the later unpublished integration and its governed blocker are preserved in the managed aep/er-suite-evidence-admission archive. This record does not claim that separate upstream PR is merged.

Both ESS trees were removed by managed GC. AEP's archived integration is retired with verified recovery proof. The final ER record branch and read-only CI observation tree have an explicit cleanup handoff: publish this record, verify its required checks and mainline merge, then finish and GC those exact trees. Their final operational results are retained in the local release evidence directory; this record does not claim those post-publication actions have already occurred. Documentation publication remains asynchronous and unverified.
