---
format: aep.planning-md/3
id: story:website-dependencies-pass-npm-audit
kind: story
status: implemented
title: The website's locked dependencies pass npm audit at high
owner: entity-runtime
relations:
- serves: vision:O2
scope:
- confidence: inferred
  path: .github/workflows/pages.yml
- confidence: cited
  path: website/package-lock.json
- confidence: cited
  path: website/package.json
- confidence: cited
  path: website/vendor
revision: 8
transitions:
- {from: "draft", to: "proposed", at: "2026-10-06T23:24:53Z", actor: "human:timo", revision: 6}
- {from: "proposed", to: "active", at: "2026-10-06T23:24:53Z", actor: "human:timo", revision: 7}
- {from: "active", to: "implemented", at: "2026-10-06T23:42:22Z", actor: "human:timo", revision: 8, decided_on: {"recorded":{"test_result":1,"verification":1}}}
---
# The website's locked dependencies pass `npm audit --audit-level=high`

## Outcome

`npm audit --audit-level=high` in `website/` exits 0 on the committed `package-lock.json`, so the
weekly `Audit` workflow (`.github/workflows/audit.yml`) is green again. The site still builds
(`task site-build`) and every vendored guard is exercised by `npm run vendor-check`.

## Why

The scheduled `Audit` run of 2026-10-05 failed at `npm audit (website)` with 37 vulnerabilities
(3 low, 34 high); the runs of 2026-09-21 and 2026-09-28 passed. On `main` at `a5e40968` a local
`npm audit --audit-level=high` reports 56 (4 low, 13 moderate, 20 high, 19 critical). After a
non-forcing `npm audit fix`, 47 remain, and their roots are:

| package | severity | advisory | upstream fix |
|---|---|---|---|
| `braces` 3.0.3 (via `chokidar`, `micromatch`) | high | GHSA-vfj7-8cjw-p6xm, stack exhaustion on deeply nested patterns | none: `<= 3.0.3`, no patched version; upstream fix PRs micromatch/braces#78 and #79 are open |
| `tinypool` 1.1.1 (via `@docusaurus/core`) | critical | GHSA-5gmw-xhrv-c9v3, GHSA-85c8-ppgw-ccpr | 2.1.2 |
| `serialize-javascript` 7.1.1 (pinned by `overrides`) | low | GHSA-gfhx-hw2g-v5hg | 7.1.2 |
| `postcss-selector-parser` | moderate | GHSA-rj75-hqrm-r3gf | 7.1.6 |
| `katex` (via `mermaid`) | low | GHSA-238p-pmpm-9mq7 | none |

`npm audit fix --force` would install `@docusaurus/preset-classic` 3.7.0, a downgrade, and is not
an option.

## Acceptance

- `npm audit --audit-level=high` in `website/` exits 0; its output is kept as evidence.
- `braces` is carried as a vendored tarball with depth guards, in the pattern of
  `website/vendor/image-size/B10X-PATCH.md`: a readable source directory, a `B10X-PATCH.md`
  saying what changed and how to drop it, a version above 3.0.3, a hostile input that overflows
  the stack on 3.0.3 and is refused or handled by the vendored copy, run by `npm run vendor-check`.
- `tinypool` resolves to a patched version through `overrides`; `serialize-javascript` moves to
  7.1.2.
- `task site-build` exits 0.

## Out of scope

Moderate and low advisories the threshold does not fail on, unless their fix is a plain override.
Changing the workflow's threshold or adding an exception.
