# braces 3.0.4-b10x.1

`braces` 3.0.3 (npm, 2024-05-21, MIT — `LICENSE` unchanged) with a nesting-depth guard, carried
here because GHSA-vfj7-8cjw-p6xm (high, CVE-2026-93687: stack exhaustion on deeply nested patterns)
covers every version through 3.0.3 and upstream has released no fix; its two fix proposals,
micromatch/braces pull requests #78 and #79, were open on 2026-10-07. Docusaurus pulls it through
`chokidar` (`@docusaurus/core`) and `micromatch` (`@docusaurus/utils`).

## What changed

The six `lib/` hunks of micromatch/braces#78 at `97308a01`, applied to the published 3.0.3 files.
One `parse.js` hunk lands 8 lines earlier than in the pull request, because upstream `main` carries
unreleased `parse.js` changes since the 3.0.3 tag that this copy does not take.

| file | change |
|---|---|
| `lib/constants.js` | `MAX_AST_DEPTH: 100` |
| `lib/utils.js` | `assertDepth(depth)` throws `SyntaxError: AST nesting depth exceeds the maximum of 100` above it |
| `lib/parse.js` | checks the open-container count before each `(` and `{` opens another |
| `lib/compile.js`, `lib/expand.js`, `lib/stringify.js` | each recursive walker carries its depth and checks it on entry |

Outside those hunks, every file is the published one: `index.js` and `README.md` are unchanged,
and the README does not mention the limit. The manifest changes `version` and `description` only.

A pattern nesting more than 100 braces or parentheses is now refused where 3.0.3 accepted it or
overflowed. The parser counts containers and `compile` counts calls, so the deepest pattern
`braces()` compiles nests 99.

## Why #78 and not #79

- **It refuses with the error braces already throws.** Its length limit throws `SyntaxError`, so a
  caller that handles that refusal handles this one. #79 instead keeps over-deep nesting as literal
  text, which silently changes what a pattern means.
- **It bounds the walkers themselves.** The advisory cites the recursion in `compile.js` and
  `expand.js`; #78 guards those walkers and `stringify`, so the bound also holds for a caller-built
  AST, which `braces.compile`, `.expand` and `.stringify` accept without parsing. #79 guards the
  parser only.
- **It adds no API.** #79 adds a public `maxDepth` option, which a vendored copy would then have to
  keep compatible with whatever upstream finally ships.
- It is the fix the advisory's source report (micromatch/braces#70) recommends: a nesting cap in
  `parse`, analogous to the length cap.

## Installing and repacking

The version is `3.0.4-b10x.1` so that the advisory range `<= 3.0.3` no longer matches and any real
`3.0.4` supersedes it. A prerelease satisfies neither `chokidar`'s `~3.0.2` nor `micromatch`'s
`^3.0.3`, so `overrides.braces = $braces` is what makes both resolve to it.

This directory is the readable source; `vendor/braces-3.0.4-b10x.1.tgz` beside it is what
`package.json` installs (`dependencies.braces = file:…tgz`). It is a tarball for the reason
`vendor/image-size/B10X-PATCH.md` gives. After any change here, repack and name the tarball, so
that npm records its new integrity. A bare `npm install --package-lock-only` keeps the old integrity
at an unchanged version; `npm ci` then installs the old tarball from npm's cache, or fails with
`EINTEGRITY` where the cache is empty:

```bash
npm pack ./vendor/braces --pack-destination ./vendor --ignore-scripts
npm install --package-lock-only ./vendor/braces-3.0.4-b10x.1.tgz
```

## The check

`b10x-check.mjs` runs the hostile inputs in `b10x-hostile-inputs.mjs`: 4,999 nested braces and 4,999
nested parentheses (9,999 characters, under the 10,000-character limit) and a 10,000-level AST. Each
goes through `braces()`, `braces(…, { expand: true })`, `parse` and `stringify`, each in a fresh
process, and each must throw the guard's `SyntaxError`. Seven ordinary patterns must still give
3.0.3's results. The check runs against this directory and against every `braces` the lockfile
installs, each of which must be byte-identical to this directory. braces has no ESM build;
`index.js`, CommonJS, is its only entry point. `npm run vendor-check`, the site build and `pages.yml`
run the check.

`node vendor/braces/b10x-check.mjs <dir>` checks only the package in `<dir>`. Against published
3.0.3 on 2026-10-07 (Node 22) it failed all 11 hostile cases, 8 with `RangeError: Maximum call
stack size exceeded` and 3 by accepting the input, and passed all 7 ordinary patterns.

## Dropping this copy

When upstream publishes a release above 3.0.3 that closes GHSA-vfj7-8cjw-p6xm, remove
`vendor/braces/`, `vendor/braces-3.0.4-b10x.1.tgz`, the `braces` entries in `package.json`
(`dependencies`, `overrides`, and its command in `scripts.vendor-check`) and this note; then
`npm install`, `npm audit`, `npm run build`.
