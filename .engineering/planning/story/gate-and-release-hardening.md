---
format: aep.planning-md/3
id: story:gate-and-release-hardening
kind: story
status: draft
title: The gate and release path enforce what they claim
summary: Rust checks close manifest, test-pin, protocol, supply-chain, token and release provenance blind spots.
relations:
- decomposes: epic:the-shell
scope:
- confidence: cited
  path: .github/workflows/gate.yml
- confidence: cited
  path: .github/workflows/release.yml
- confidence: inferred
  path: AGENTS.md
- confidence: inferred
  path: CHANGELOG.md
- confidence: cited
  path: Taskfile.yml
- confidence: inferred
  path: crates/entity-core/tests/requirements.rs
- confidence: inferred
  path: crates/entity-xtask
- confidence: inferred
  path: crates/scan-support/src/lib.rs
- confidence: inferred
  path: docs/design/service-semantics-v0.1.md
- confidence: inferred
  path: docs/requirements.md
- confidence: cited
  path: scripts/changelog-section.py
- confidence: cited
  path: scripts/check-pin.py
- confidence: cited
  path: scripts/check-requirements.py
revision: 4
---
## Context

Source scans miss build surfaces, requirement pins accept dead tests, local and CI examples drift, website dependency policy admits known highs, and release/bot workflows expose broader authority than their operations need.

## Acceptance

One Rust-owned gate detects every planted enforcement bypass, local and CI surfaces agree, website exceptions are exact and expiring, bot credentials exist only during push, and a release tag cannot disagree with Cargo or the changelog.

## Open items

Audited 2026-09-09; left where it is because the acceptance is only partly met.

Met: the release tag must equal the workspace version and have a dated changelog section (`release.yml` provenance job), every checkout runs with `persist-credentials: false`, `npm audit` runs with no exceptions since 2026-09-09. Not met: “one Rust-owned gate” — `req-check`, `pin-check` and `notes-check` are still Python under `scripts/`; `scan-support` is the only Rust checker with plantings.
