---
format: aep.planning-md/1
id: story:public-system-handbook
kind: story
status: active
title: Public system handbook and verified adoption paths
summary: Clarify subsystem coverage, model derivation, recording and retries, and publish checked examples.
relations:
- serves: vision:O2
- informed_by: story:published-docs-match-runtime
- decomposes: epic:generated-entity-surfaces
scope:
- confidence: cited
  path: .engineering/planning/journal.jsonl
- confidence: cited
  path: .engineering/planning/story/public-system-handbook.md
- confidence: cited
  path: CHANGELOG.md
- confidence: cited
  path: README.md
- confidence: cited
  path: website/docs
- confidence: cited
  path: website/sidebars.ts
- confidence: cited
  path: website:data/bootstrap
- confidence: cited
  path: website:sources.lock.json
revision: 5
---
## Context

The operator requested the same public docs and Website sweep completed for Connectors and Substrate. This is an interactive implementation session with publication authorized by that request. It documents existing behavior and introduces no runtime entity or contract.

The release baseline is 0.17.7 at 04e887baabde46d80cf97959534c952b8e235cb3. README links use the old facade; quickstart and library dependencies remain at 0.16.0; the crate overview omits entity-query. The generic CLI owns handwritten Clap commands in crates/entity-cli/src/main.rs, while generated commands and MCP delegate to StoredRuntime. Its exact retry path in crates/entity-shell/src/lib.rs differs from the generic execute path. The MCP guide still claims an identical retry is a revision conflict. No ESS system.yaml or ESS workspace dependency describes this runtime as a whole system.

## Scope

```yaml
scope:
  files:
    - README.md
    - CHANGELOG.md
    - website/docs/**
    - website/sidebars.ts
    - .engineering/planning/story/public-system-handbook.md
    - .engineering/planning/journal.jsonl
  related_repositories:
    - website
```

Website changes are restricted to deterministic source lock and Atlas-produced bootstrap data needed for publication. Existing runtime and Docs System pins remain the publication inputs. Application code, contracts, shared renderers, and unrelated planning artifacts are outside this sweep.

## Acceptance

The published Entity Runtime handbook explains the existing subsystems, typed command/event/history coverage and derivation limits, provides executable 0.17.7 examples with honest concurrency and authority semantics, and passes the repository, Website, Atlas delivery and browser accessibility checks.

## Verification plan

Run task check and task site-build with bounded build concurrency; verify copied quickstart, generated CLI, MCP retries and generated docs against current code; inspect light/dark desktop and narrow screens for readable diagrams and tables. Publish source first, refresh Website source lock, render the Atlas snapshot, run Website and Atlas gates, and verify live provenance. Keep evidence here and clean only task-owned generated output and reviewed managed-worktree IDs.

## Implementation evidence

The documentation now states authored versus derived coverage, generic CLI versus StoredRuntime retry semantics, domain events versus complete history, optional queries and transaction sessions, and trusted-host authority boundaries. The generated CLI example now includes submit before approve and verifies byte-identical retry output.

- task check: exit 0, including format, Clippy, tests, rustdoc, examples, requirements, pins, planning and notes. Local PostgreSQL integration was explicitly skipped because ENTITY_POSTGRES_URL is unset; the required CI gate supplies PostgreSQL.
- task site-build: exit 0 after correcting the landing-page link to respect its explicit slug.
- Copied Bash quickstart: accepted create/submit, expected policy refusal, durable human approval at revision 3.
- Copied generated CLI guide: matching 0.17.7 source, offline generation, create/submit/approve, get/list/events, byte-identical retry via cmp.
- Exact MCP guide operation: accepted call, identical replay, changed-intent record_conflict, new stale-request revision_conflict, one approval event.
- Generated documentation is byte-identical in two fresh output directories; the quickstart Mermaid block exactly matches entity graph output.

An initial gate run with CARGO_TARGET_DIR exposed the generator's fixed output-path assumption. The gate passed using its default output paths; the generated-CLI guide documents the supported environment. This is a documentation boundary, not a runtime fix. Build concurrency is bounded at two jobs and existing Cargo caches are reused. A temporary target symlink was unlinked before committing; no build artifacts enter the source.

Publication and live browser evidence will be recorded after source and Website delivery.
