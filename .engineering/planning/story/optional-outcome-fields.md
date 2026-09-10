---
format: aep.planning-md/1
id: story:optional-outcome-fields
kind: story
status: draft
title: Preserve omitted optional properties in outcome templates
relations:
- depends_on: story:typed-outcome-identity
scope:
- confidence: cited
  path: CHANGELOG.md
- confidence: cited
  path: crates/entity-core
- confidence: cited
  path: docs/design/kernel-optional-templates-v8.md
- confidence: cited
  path: docs/requirements.md
revision: 4
---
## Source authority

ESS evolution requires optional state/event/error fields to retain omission separately from null. Its lowerer currently refuses these fields at crates/realize/ess-entity-runtime/src/outcomes.rs::Builder::fields. ER's runtime::resolve_template currently refuses any missing reference; definition validation treats all template leaves identically. This story extends existing Outcome/FieldDefinition/template types, not a new domain entity.

## Acceptance

An opt-in outcome definition creates, changes, emits and refuses with omitted optional object properties preserved distinctly from explicit null; full record replay reproduces that distinction, rejects tampering, and older profile definitions/records keep their bytes and refusals.

## Design

See docs/design/kernel-optional-templates-v8.md, requirement R-133. Outcome definition/record profile 8 retains profile 7 typed identity and all earlier value semantics. The template string $optional.args.path (or another already scoped reference after $optional.) omits only its containing object property when the underlying declared reference is absent; a present null remains null. Plain $args.path remains strict. Explicit optional assignments to top-level state remove that field on change when absent; omitted assignments still retain prior state. Array elements and root event/error payloads cannot be omitted. Escaped $$optional... is literal text. Validate source paths and template positions at registration; missing required output still fails schema validation. No catch-all suppression of template failures, default injection or state mutation on error.

## Scope

crates/entity-core; docs/design/kernel-optional-templates-v8.md; docs/requirements.md; CHANGELOG.md. Preserve all current dependency pins and published formats 1-7. Use targeted core/affected surface tests, explicit old-reader/old-byte checks, guard mutation, strict lint/docs/format and requirements checks. No full, database, ownership, remote correctness or release gate.

## State

This interactive operator-authorized prerequisite remains in its initial status under repository lifecycle policy. ESS lowering and Connectors adoption remain separate follow-on evidence; this kernel change alone does not complete either.

## Verification evidence

The opt-in profile-8 implementation preserves absence/null/value through creation, replacement and top-level field removal, ordered events and business errors. Ordinary missing references still fail, required output schemas still fail on omitted required fields, and unavailable/unknown paths plus array/root omissions are rejected at registration. Typed identity remains required. Three independent optional-template tests pass; all targeted entity-core/entity-graph/entity-surface tests and doctests pass (local-evidence:ess-evolution-20260910/er-optional-packages.log). Strict all-target core Clippy, Rust 1.85.0 all-target checking, strict rustdoc, formatting, the 108-requirement/385-test-function register check and 7 release-note shapes pass.

A deliberate mutation converted absent optional references into Some(Null). The absence/null/replay test failed with the exact unexpected null properties in state and nested arrays (er-optional-mutation.log, exit 101). The original runtime file was restored byte-for-byte before the passing package run. No mutation remains.

The standalone Rust compatibility probe compares the current source to published profile-7 commit 38132946a924d276b6d528e18d4d1ed10841ee8f. Profiles 1-7 produce byte-identical definitions and records and replay under both readers; the exact prior reader rejects profile-8 definitions and records. Probe source/lock: local-evidence:ess-evolution-20260910/er-optional-probe; output er-optional-probe.log. The new dependency is local during implementation and will be changed to the exact published source before worktree retirement.

Domain events retain their existing changed-value map; complete outcome records are the replay authority for field removals, as already required for zero-event decisions. No standalone legacy event rehydration capability is claimed. ESS lowering, generated wiring and Connectors adoption remain follow-on work. No full, database, ownership, remote correctness or release gate ran. No dependency lockfile changed.

## Published source

Implementation f9f0545e66fbde40e6eb36e92d71f325dfd0d86c is published on origin/feat/optional-outcome-fields through standalone b10x-gates bot, with exact organization bot author and committer verified. The App-authorized feature-branch creation succeeded; no branch protection, hooks or gate configuration changed and no pull request or release was created.

The standalone compatibility probe now pins both libraries by their exact public Git revisions: current f9f0545e66fbde40e6eb36e92d71f325dfd0d86c and prior 38132946a924d276b6d528e18d4d1ed10841ee8f. Its rebuilt published-source run passed (local-evidence:ess-evolution-20260910/er-optional-published-probe.log); no dependency points into the retiring worktree. ESS is consuming this published prerequisite in story:ess-entity-runtime-lowering. Main integration, SDK/application adoption and broader evolution are not claimed by this feature publication.
