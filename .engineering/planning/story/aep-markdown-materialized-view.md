---
format: aep.planning-md/1
id: story:aep-markdown-materialized-view
kind: story
status: draft
title: AEP Markdown is a materialized view
summary: Make the Entity Runtime store authoritative and render engineering-protocols planning Markdown as a deterministic, one-way, optionally committed projection.
relations:
- derived_from: epic:drive-engineering-protocols
- informed_by: epic:the-store-an-adopter-runs-on
revision: 2
---
## Outcome

The canonical AEP planning state lives as Entity Runtime instances and recorded decisions. The
human-readable planning tree is a deterministic materialized view rendered by
`engineering-protocols`, not a second persistence model and not an input used to hydrate the
canonical store.

## Context

The current Markdown representation combines four concerns: human review, persistence, import and
policy enforcement. Once the complete planning record is stored canonically — entities, relations,
accepted and refused decisions, evidence, audit data and idempotency records — those concerns can
be separated. Markdown remains valuable as a review surface, but it no longer needs authority.

"Build artifact" does not imply "gitignored". A projection may be committed for native GitHub
diffs and links, or generated on demand for a CLI, CI report or site. In either mode it must be
reproducible from the canonical store.

The Markdown body authored for a story or ADR remains canonical field data. What is derived is the
complete document: frontmatter, lifecycle state, revision, relations, generated sections and path
placement.

## Proposed boundary

Entity Runtime owns the generic store contract, complete decision history, replay, revisions,
atomic persistence and store enumeration. Its file provider should keep unrelated subjects in
separate, confined records rather than one repository-wide JSONL stream, so unrelated branches do
not conflict on every append.

Engineering Protocols owns the AEP-to-entity mapping, the planning-specific Markdown renderer and
the command that materializes that view. The renderer must not move into `entity-core`, and a
Markdown edit must never become an implicit canonical write.

## Materialization contract

The proposed command is `protocol sync`, or a more explicitly directional name such as
`protocol artifact render`. It reads one consistent canonical snapshot and renders the complete
desired output tree. Its contract is:

- identical logical store state produces byte-identical Markdown bytes and paths;
- rendering uses a staging area before installing output;
- an owned manifest identifies stale generated paths, so no unrelated file is removed;
- generated metadata contains a projection format version and stable source revision or digest,
  but no volatile timestamp;
- `--check` performs no writes and refuses missing, changed or orphaned output;
- the first implementation renders the full tree; incremental rendering is only an optimisation;
- a projection failure never rolls back a successful canonical command, because the view can be
  repaired by rerunning materialization.

Ordinary commands write only through the AEP `CommandService`. They may automatically refresh
configured views as a convenience, but success of the canonical write and success of projection
refresh remain distinguishable outcomes.

## Repository policy

The initial migration should keep generated Markdown committed and gate it with
`protocol sync --check`. That preserves current GitHub review, links and no-binary browsing while
making its non-authoritative status explicit. Once pull-request rendering and artifact-diff tooling
exist, adopters may choose an uncommitted projection without changing the data model.

## Migration shape

1. Define and verify the complete canonical AEP record set.
2. Implement a pure store-to-Markdown renderer and compare it with the existing planning tree.
3. Add drift checking while Markdown is still the source representation.
4. Coordinate the cross-repository authority flip and any verified-byte change through the atlas
   ADR process.
5. Retain committed generated Markdown initially; reconsider that policy only after equivalent
   review tooling exists.

## Acceptance

For a committed canonical AEP store snapshot, the protocol CLI renders a byte-stable Markdown tree,
detects every projection drift without writing in check mode, and has no path by which editing the
rendered Markdown changes canonical entity state.

## Open questions

- Whether the public verb is `sync`, `render` or `materialize`.
- Which projection configurations are repository policy and which are adopter-selectable.
- How a reviewer sees semantic entity changes when generated Markdown is not committed.
- Whether projection source digests bind to individual subjects, a snapshot manifest or both.
