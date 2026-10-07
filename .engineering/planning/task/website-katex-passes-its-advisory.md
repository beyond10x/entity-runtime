---
format: aep.planning-md/3
id: task:website-katex-passes-its-advisory
kind: task
status: implemented
title: The website's katex passes its advisory
relations:
- serves: vision:O2
- delivers: design:wave-issue-54-entity-core-features
revision: 4
transitions:
- {from: "draft", to: "proposed", at: "2026-10-07T09:06:34Z", actor: "human:timo", revision: 2, decided_on: {"recorded":{"test_result":1}}}
- {from: "proposed", to: "active", at: "2026-10-07T09:06:34Z", actor: "human:timo", revision: 3, decided_on: {"recorded":{"test_result":1}}}
- {from: "active", to: "implemented", at: "2026-10-07T09:06:34Z", actor: "human:timo", revision: 4, decided_on: {"recorded":{"test_result":1}}}
---
# The website's katex passes its advisory

## Outcome

`website/` resolves `katex` at 0.19.0, outside every published advisory range, so Dependabot's
security update for katex has nothing left to do and `npm audit` in `website/` reports no
vulnerability.

## Why

Dependabot's security updates for `npm_and_yarn` in `/website` failed (runs 37554297938 and
37529081590): the advisories cover katex below 0.18.2, and `@beyond10x/docs-system` and
`@docusaurus/theme-mermaid` hold it at `^0.16` (latest resolvable 0.16.47). The fix is an override,
as the run's own message names. 0.18.11 is deprecated on npm ("Accidentally published with
breaking changes. Use 0.19.0 instead."), so the override pins 0.19.0.

## Acceptance

- `website/package.json` overrides `katex` to 0.19.0; `website/package-lock.json` resolves 0.19.0.
- `npm audit` in `website/` prints `found 0 vulnerabilities` (before: 4 low).
- `task site-build` succeeds with the override.

## Evidence

Run in the wave's integration tree on 2026-10-07: `npm audit` → `found 0 vulnerabilities`, exit 0;
`task site-build` → `Generated static files in "build"`, exit 0; installed katex 0.19.0.
