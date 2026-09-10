---
format: aep.planning-md/1
id: story:typed-outcome-identity
kind: story
status: draft
title: Bind typed entity identity to canonical instance keys and replay
relations:
- depends_on: story:scalar-predicate-semantics
scope:
- confidence: cited
  path: CHANGELOG.md
- confidence: cited
  path: Cargo.lock
- confidence: cited
  path: Cargo.toml
- confidence: cited
  path: crates/entity-core
- confidence: cited
  path: crates/entity-eventlog/Cargo.lock
- confidence: cited
  path: docs/design/kernel-typed-identity-v7.md
- confidence: cited
  path: docs/requirements.md
revision: 6
---
## Source and outcome

ESS semantic-crosswalk.md and ResolvedEntity.identity / ResolvedInstance require a typed domain identity, with supplied input or an observed event field as its declared source. ER currently accepts an unrelated nonempty string id. The new outcome profile must enforce the correspondence in the kernel, including before creation/refusal, changes and replay. Existing EntityDefinition field schemas, EntityInstance identity/fields and outcome Definition/Invocation/Record are the typed homes; this extends their identity contract rather than introducing a separate domain entity.

Declare which required typed entity field carries identity. Its schema validates the logical identity; the string instance key is an injective deterministic identity/1 encoding of the canonical JSON value. This allows numbers and typed aggregate values without pretending they are text operands: entity predicates read the declared identity field with its real type. No JSON catchall, implicit defaults or additional untyped identity properties are admitted. The lowerer retains the original source identity and creates the explicit typed field layout. The kernel verifies the identity field against the invocation key in both previous and resulting state; caller-supplied keys, mutated identities and invalid values cannot escape as accepted records. Creation has to bind the identity explicitly; update may retain it.

## Acceptance

Independent vectors show type-distinct, exact large-integer, Unicode/dollar-string and ordered aggregate identities round-trip through canonical keys, schema validation, creation/update/refusal and complete replay; mismatched or malformed keys, wrong identity values, identity mutation and tampered history are refused with named reasons. Profile 7 requires a closed explicit identity contract, profiles 1 through 6 forbid it, and an independently pinned previous reader preserves old bytes while refusing new definitions and records. Verify the correspondence guard by mutation. This is a prerequisite to complete ESS assembly and SDK delegation, not an application-adoption claim.

## Scope and verification

Own crates/entity-core/src/outcome.rs and its identity helper module/tests, docs/design/kernel-typed-identity-v7.md, docs/requirements.md and CHANGELOG.md. Integrate current remote release eaf43090636abce025649e565e5af271c4513eae while preserving its release/docs source bytes. Use targeted core/surface compatibility, strict Clippy, rustdoc, formatting, requirements and Rust 1.85 checks. No full or expensive remote gate. This is one prerequisite story, not a multi-story decomposition; the local move policy keeps it draft until an operator requests a transition. Full service assembly, source input/event binding, subjectless outcomes, missing value semantics and actual connectors_v2 adoption remain required.

## Canonical key representation

The implementation uses an explicit typed JSON codec under identity/1: null or single-tag boolean/number/string/array/object nodes. Numeric token spelling is a string in the codec; object values are recursively typed nodes in a sorted map. This preserves object keys even when they resemble the JSON library's private numeric carrier, instead of passing a domain object through serde_json::Value deserialization as codec authority. Decode must reproduce the exact encoded key, refusing duplicate keys, alternate ordering/whitespace, unknown tags or numeric spelling changes. Identity admission still uses the original declared field schema. This is an injective key representation, not a replacement domain format or a value normalization rule.

## Implementation and verification

Implemented outcome definition/record profile 7 with closed identity metadata, a typed deterministic key codec, required fully typed identity fields without defaults, and identity admission before selection. Creation explicitly assigns identity; the kernel verifies previous and resulting identity correspondence and refuses changes. Entity predicates continue reading the actual typed identity field. Existing profiles forbid the metadata and preserve their raw-key semantics and bytes.

Targeted entity-core/entity-surface suites and doctests pass (local-evidence:ess-evolution-20260910/er-identity-packages.log). The restored identity cases pass (er-identity-restored.log). Strict Clippy of both packages and all targets, strict rustdoc, Rust 1.85 all-target checks, repository formatting and the unchanged requirements checker pass (er-identity-clippy.log, er-identity-rustdoc.log, er-identity-msrv.log, er-identity-fmt.log, er-identity-requirements.log). No full, ownership, database or expensive remote gate ran.

The independent er-identity-probe uses authored ESS source compiled at ESS 6516192fe8d047640ff14ea8d5cc1f3c697a2820: nominal positive integer, UUID and compound identities retain their actual lowered identity schemas. Manually expected valid/invalid values test large integers, nominal invariants, exact UUID grammar, compound field invariants and closed properties before creation and replay (er-identity-probe.log). This is explicit fixture wiring, not completed command assembly or application adoption. The independently pinned ER 9a5693602b4a7c896b0601d58420e79796ead2a9 reader preserves legacy entity and profile-1-through-6 definition/record bytes, while refusing profile 7 and identity metadata injected into profile 6.

Sensitivity was verified by replacing the identity correspondence refusal with success. The test then returned a created instance with identity 8 under the key for 7, rather than IdentityMismatch, and exited 101 (er-identity-mutation.log). The original helper was restored byte-for-byte, and the focused identity cases pass again. The separate key codec also preserves an object key resembling the JSON library's private number carrier; this codec check is not a claim about every legacy record reader.

Integrated current remote release eaf43090636abce025649e565e5af271c4513eae into the feature candidate. Its README and website bytes remain identical. The sole merge conflict was the changelog: retained both all unpublished feature notes and the incoming 0.18.1 release section. The excluded Eventlog adapter lock changes only the four local path-package versions to 0.18.1; locked offline metadata resolves, and the adapter's own package/version and external pins remain unchanged. This avoids leaving the release merge with a stale standalone lockfile. Release-note checks pass (er-identity-notes.log).

The story remains draft under the repository's explicit status-move rule. Publishing a source branch does not merge or release it. Complete ESS command identity/input/event wiring, other missing kernel capabilities, SDK delegation, persistence adoption, and actual connectors_v2 runtime proof remain required by the full goal.

## Published source and pinned comparison

Published the implementation and release-baseline integration as 38132946a924d276b6d528e18d4d1ed10841ee8f on feat/typed-outcome-identity through standalone b10x-gates bot. Both author and committer are the organization bot, and the remote branch advertises that exact commit. This is feature-source publication, not main integration or release; no PR or remote correctness workflow was started.

The retained comparison now pins current ER to published 38132946a924d276b6d528e18d4d1ed10841ee8f, the old reader to published 9a5693602b4a7c896b0601d58420e79796ead2a9, and ESS to local committed 6516192fe8d047640ff14ea8d5cc1f3c697a2820. ESS is fetched from the local repository because its public publication remains blocked; no mutable worktree path remains in the probe dependencies. The exact-pinned comparison passes (local-evidence:ess-evolution-20260910/er-identity-pinned.log). Retain its Cargo manifest, lock, authored fixture and Rust source for reproduction. Runtime behavior is unchanged from the code commit; this section records publication and reproducible comparison evidence.
