---
format: aep.planning-md/3
id: story:adopt-current-plan-and-release-contracts
kind: story
status: active
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
revision: 15
transitions:
- {from: "draft", to: "proposed", at: "2026-09-28T07:50:10Z", actor: "human:timo", revision: 6}
- {from: "proposed", to: "active", at: "2026-09-28T07:50:10Z", actor: "human:timo", revision: 7}
---
## Acceptance

The executable five-library contract is conforming under admitted exact ESS evidence, ER uses the latest released Git-native AEP planning layout and a current compatible pinned CLI in CI, all work is merged into origin/main, the next ER version has a verified annotated tag, successful required checks and published release artifacts, and this effort's managed worktrees are cleaned with recovery proof.

## Authorization

The operator explicitly requested mainline integration, upgrading ER's own planning store, cleanup and a new ER version. That includes resolving the previously pending narrow AEP evidence-reader compatibility work, integrating the ESS extension with current upstream, importing successful evidence and completing the specification's authority adoption. Production ER behavior corrections remain outside this effort.

## Scope and sequence

Use AEP's documented Eventlog-to-Git migrator with --verify, preserve every artifact, transition, body and evidence count, and only then use current AEP for further writes. Update the planning workflow and version gate for aep.project/5 without --against. Reconcile the direct-return ESS extension with 0.38.0's already-allocated format versions, then pin the published compatible extension consistently in the isolated checker. Regenerate exact model/suite/report evidence and preserve historical mutation evidence. Resolve the admission blocker through actual successful report import before adopting authority. Run the full ER gate, plan validation and CI, merge through bot authority, cut the next patch version because public runtime behavior is unchanged, and verify release assets before cleanup.

The existing ER initiative and executable-system-specification:er-library-contracts retain the broader behavioral plan and evidence. Release documentation publication is asynchronous and outside the source-release completion boundary.
