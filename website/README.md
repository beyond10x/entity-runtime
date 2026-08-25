# Entity Runtime website

The Docusaurus site publishes the repository's `docs/` tree — guide, vision, requirements register,
designs — at <https://beyond10x.github.io/entity-runtime/>. There is one copy of every document;
the site reads `../docs` and adds only the landing page.

## Develop

```bash
npm ci
npm run start
```

## Gate

```bash
npm run typecheck
npm run build      # onBrokenLinks: throw — a dangling link fails the build
```

The site is deliberately not a step of `task check`, which reaches no network; it is gated by
`.github/workflows/pages.yml` on every push and pull request, and deployed from `main`.
