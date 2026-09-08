# Entity Runtime website

The Docusaurus site publishes a human-facing product handbook at
<https://beyond10x.github.io/entity-runtime/>. Public Markdown lives under `website/docs/`; the
repository-root `docs/` tree remains the engineering record and is neither rendered nor linked by
the site.

The site leads with Entity Runtime as the deterministic authority between agent proposals and
durable state. Content is organized into introductions, task guides, complete references, and
operator runbooks. Keep examples executable and describe only released behavior.

## Develop

```bash
npm ci
npm run start
```

The start/build scripts run the Rust `entity generate docs` command first so the refund reference
under `/examples/refund/` always reflects `examples/refund.yaml` and the current generator.

## Gate

```bash
npm run typecheck
npm run build      # onBrokenLinks: throw — a dangling link fails the build
```

Before publishing, also search `website/` for links into root `docs/`, `.engineering/`, requirements,
designs, plans, roadmaps, or reviews. Those are useful repository records, not public navigation.

The site is deliberately not a step of `task check`, which reaches no network; it is gated by
`.github/workflows/pages.yml` on every push and pull request, and deployed from `main`.

## Dependency overrides

`package.json` carries `overrides` because `package.json` cannot carry comments and each pin has a
reason that would otherwise be lost:

| override | why |
|---|---|
| `serialize-javascript` 7.1.1, `uuid` 11.1.1 | added with 0.15.0 (`70c4167`); that commit records no reason |
| `qs` 6.16.0 | GHSA-4mjr-xmp4-gh2g and GHSA-x5fp-wj9c-mxmx; `express` 4 pins `qs` 6.15.3 and the fixed version arrives only with `express` 5, which webpack-dev-server 5 does not take yet (2026-09-09) |

Drop an override when the requesting package moves past it: `npm ls <name>` says who still asks for
the old range. The remaining `image-size` advisories have no fixed release; `.github/workflows/audit.yml`
carries that exception with its expiry date.
