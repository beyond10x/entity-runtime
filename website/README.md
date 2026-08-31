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
