---
format: aep.planning-md/1
id: task:address-independent-review-2026-09-08
kind: task
status: draft
title: Correct the findings of the 2026-09-08 full and independent reviews
summary: Eight review findings plus the blocker and nits two independent reviewers raised against the fixes, all corrected with regression tests
relations:
- decomposes: story:provider-integrity-hardening
- serves: vision:O2
revision: 2
---
## Outcome

Correct every finding of the 2026-09-08 full review and of the two independent reviews of its
corrections: the fold's missing invariant check, the generator's output-path assumption, the
`plan-check` guard's wrong binary, a literal instant operand that could never be read, and the
documentation, comment and changelog drift around them.

## Scope

- Cited: `crates/entity-core/src/replay.rs`, `validation.rs`, `runtime.rs` (visibility only);
  `crates/entity-cli/src/main.rs`; `crates/entity-xtask/src/main.rs`; `crates/entity-remote/src/{hybrid,lib}.rs`.
- Cited: `Taskfile.yml`, `.github/workflows/{gate,release}.yml`, `docs/design/kernel-v0.1.md` § 10.1
  and the `before`/`after` paragraph, `docs/requirements.md` rows R-55, R-59, R-97,
  `website/docs/guide/{definitions,generated-cli}.md`, `CHANGELOG.md`, `AGENTS.md` release recipe.
- Record: `docs/reviews/2026-09-08-full-review.md`.

## Acceptance

Every correction carries a behaviour-named test that was watched to fail under the one-line
mutation it exists to catch; `task check` with `ENTITY_POSTGRES_URL` naming PostgreSQL 17 exits 0;
`task site-build` exits 0; `python3 scripts/check-requirements.py` reports 0 findings.

## Authorization

The operator asked on 2026-09-08 for a full review, then for every finding to be fixed, then for an
independent review of the fixes and for those findings to be fixed as well. No release was
requested.

## Implementation evidence

Findings, corrections and the tests that pin them are tabulated in
`docs/reviews/2026-09-08-full-review.md`. Every new guard was verified by breaking it. The final
tree passed the full gate against a PostgreSQL 17.11 container and the website build. No lifecycle
status claim is made by this body.
