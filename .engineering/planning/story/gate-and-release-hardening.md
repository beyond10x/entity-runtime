---
format: aep.planning-md/1
id: story:gate-and-release-hardening
kind: story
status: draft
title: The gate and release path enforce what they claim
summary: Rust checks close manifest, test-pin, protocol, supply-chain, token and release provenance blind spots.
relations:
- decomposes: epic:the-shell
revision: 1
---
## Context

Source scans miss build surfaces, requirement pins accept dead tests, local and CI examples drift, website dependency policy admits known highs, and release/bot workflows expose broader authority than their operations need.

## Acceptance

One Rust-owned gate detects every planted enforcement bypass, local and CI surfaces agree, website exceptions are exact and expiring, bot credentials exist only during push, and a release tag cannot disagree with Cargo or the changelog.
