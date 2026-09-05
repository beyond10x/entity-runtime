---
format: aep.planning-md/1
id: story:public-system-handbook
kind: story
status: implemented
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
revision: 7
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

## Publication and delivery evidence

- Source PR https://github.com/beyond10x/entity-runtime/pull/10 merged at 513d3bc0c44d7ae0109df90935d984bd3f278afb. The exact reviewed source head was 6d731a74f8392158606ff614d53bc51c49127263. All required checks passed, including PostgreSQL and MSRV. GitHub records b10x-bot[bot] as merge author, web-flow as committer, and verified provenance; the merge introduces no extra source diff.
- Website commits 5114beb7d2c83e058499a5ee7f1c2d0333891a93 and 36b13eca761bc9fa012f9ba440b67b94dc8d408c publish the deterministic source lock and Atlas snapshot. Concurrent agentplugins documentation publication was incorporated by rebasing and regenerating both conflicting derivatives. All directly authored commits have both bot author and committer.
- The final Website full local gate passed: source/bootstrap contracts, build, search, code rendering, accessibility, navigation, crawl and production provenance. Its CI run is https://github.com/beyond10x/website/actions/runs/33981702074.
- Atlas verify-portal passed for the final Website artifact: 23 locked public sources, 24 surfaces, 50 delivery records.
- Atlas verify-pages passed after content publication: 36 repository states, 25 Pages repositories, 50 delivery routes, one Website commit.
- The organization content pipeline had already published this exact source while Website snapshot work was finishing; no duplicate dispatch was necessary. Atlas publication 33981286598 and root deployment https://github.com/beyond10x/beyond10x.github.io/actions/runs/33981499139 completed successfully.
- Live https://beyond10x.github.io/PROVENANCE.json binds sourceCommits.entity-runtime to 513d3bc0c44d7ae0109df90935d984bd3f278afb, source set 5c397b34ea000b8ca00605dea0d9f3cda5bd9a657d1336f2fb7b5eef728395e8, and Atlas control 7b67e8e2437ec9956135930435875a8a76139c3f. The existing Website runtime remains 815fad1b977992d01695f6b5c79495c02576212b and the Docs System pin remains 1c8c31697e87235dda8bec9467264b22a7fa0c95.
- Local artifact and live browser checks passed for system-model and storage at 1440, 320, 390 and 720 CSS pixels (the last at scale 2), in explicitly verified light and dark themes. Diagrams retain native label size; tables preserve semantic headers and named, focusable overflow; keyboard panning reaches the final columns. No whole-page horizontal overflow or page errors were observed. Desktop diagram and mobile dark-table screenshots were inspected.

The broad read-only reconcile against primary checkouts encountered pre-existing collector drift: `refused: Docs System collection failed for agentide: /home/timo/beyond10x/agentide/b10x.docs.yaml has unsupported schema b10x-docs/v4`. No unrelated primary checkout or collector was changed. Exact Website collection, portal artifact, and live delivery gates passed.

## Completion

The handbook is live at https://beyond10x.github.io/docs/entity-runtime/system-model/. This completion record changes planning evidence only and requires no new documentation bundle, source refresh, runtime migration or release tag. Temporary servers were stopped and task-generated site outputs are being removed before managed worktree finish and exact-ID garbage collection. Primary checkout edits that predated this task are preserved.
