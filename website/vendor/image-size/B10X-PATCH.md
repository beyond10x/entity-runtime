# image-size 2.0.3-b10x.1

`image-size` 2.0.2 (npm, 2025-04-02, MIT — `LICENSE` unchanged) with three loop guards, carried
here because upstream has published nothing since and its `main` differs from `v2.0.2` by a Readme
only (checked 2026-09-09). Docusaurus pulls it through `@docusaurus/mdx-loader`.

| advisory | parser | guard |
|---|---|---|
| GHSA-w3rx-r6r6-pgpr | ICNS | an entry whose recorded length is below the 8-byte entry header is refused instead of leaving the offset where it was |
| GHSA-5p2g-fcmc-qvqq | HEIF | an `ispe` box of size below 8 is refused instead of setting the next offset to its own |
| GHSA-5p2g-fcmc-qvqq | JXL | a `jxlp` box of size below 12 is refused instead of setting the next offset to its own |

The guards are in every `dist` file that bundles the parsers (16 files: the four entry points and
`types/index` in `.mjs` and `.cjs`, plus `types/{icns,heif,jxl}`); nothing else in `dist` differs
from the published tarball. The version is `2.0.3-b10x.1` so that advisory ranges of `<= 2.0.2` no
longer match and so that any real `2.0.3` supersedes it.

This directory is the readable source; `vendor/image-size-2.0.3-b10x.1.tgz` beside it is what
`package.json` installs (`dependencies.image-size = file:…tgz`, `overrides.image-size = $image-size`
so `@docusaurus/mdx-loader`'s `^2.0.2` resolves to it). A `file:` *directory* dependency trips npm
12's override dedupe (`Invalid Version` in `Link.canDedupe`), which is why it is a tarball. The
manifest inside drops `scripts`, `devDependencies` and `packageManager` — `npm pack` would otherwise
run upstream's `yarn build` — and is otherwise the published one. After any change here, repack:

```bash
npm pack ./vendor/image-size --pack-destination ./vendor --ignore-scripts
npm install --package-lock-only
```

`b10x-check.mjs` feeds the three inputs that hang 2.0.2 (`b10x-hostile-inputs.mjs`) and two honest
files through the ESM build, the CJS build and the installed tarball; `npm run vendor-check`, the
site build and `pages.yml` run it.

To drop this copy: when upstream publishes a release above 2.0.2 that closes both advisories,
remove `vendor/`, the `image-size` entries in `package.json` (`dependencies`, `overrides`,
`scripts.vendor-check`, the `build` step), the `pages.yml` step and this note; then
`npm install`, `npm audit`, `npm run build`.
