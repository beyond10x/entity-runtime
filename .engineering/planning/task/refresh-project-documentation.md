---
format: aep.planning-md/1
id: task:refresh-project-documentation
kind: task
status: draft
title: Refresh released documentation and automatic project publication
relations:
- serves: vision:O2
revision: 2
---
## Context

Implement the operator-approved documentation refresh for Eventlog 0.3.0, Entity Runtime 0.19.0 and AEP 0.57.0. Preserve runtime behavior, public URLs, historical release records and passive portal bundles.

## Scope

Refresh this repository's README, public guides, navigation and contributor guidance; preserve validation checks and upload an exact main-commit b10x-project-site artifact including hidden files.

## Delivery order

1. Publish and verify build support and refreshed documentation in each source repository.
2. Generate the site callers through Atlas; land each caller before the Atlas roster/control change.
3. Validate Atlas reconciliation, provenance and Website portal contracts, then publish coordination changes.
4. Verify automatic deployments, exact source provenance, representative pages/assets and desktop/mobile layouts. Report portal propagation independently.

## Acceptance

The refreshed guides match the named releases, repository gates pass, and the three automatic project sites serve verified source commits at /eventlog/, /entity-runtime/ and /aep/ without root publication redeploying them.

## Evidence

Starting remote main: 13f88d982f8ac90651e4023bdf6286332d042b33.

## Verified before build-support publication

The full task check exited zero, including PostgreSQL conformance and the Eventlog adapter lane.
The site passed type checking, its vendored image guard and Docusaurus link/build validation.
The refund quickstart ran against the checksummed 0.19.0 release binary: agent approval refused
with large_refunds_need_a_human; the stored human approval reached revision 3 with RefundApproved.
The passive manifest excludes ignored generated refund examples; the standalone build retains them.
Generated caller installation and live verification remain pending.
