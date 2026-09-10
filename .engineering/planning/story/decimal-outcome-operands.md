---
format: aep.planning-md/1
id: story:decimal-outcome-operands
kind: story
status: draft
title: Compare declared decimal wire values as exact numeric operands
relations:
- depends_on: story:optional-outcome-fields
scope:
- confidence: cited
  path: CHANGELOG.md
- confidence: cited
  path: crates/entity-core
- confidence: cited
  path: docs/design/kernel-decimal-operands-v9.md
- confidence: cited
  path: docs/requirements.md
revision: 4
---
## Source authority

ESS Decimal has an exact decimal-string native wire contract, already represented by ER StringEncoding::DecimalText. Numeric comparisons cannot treat that string as text. Existing ESS predicate literals retain the admitted Number value after parsing; the unrelated story:primitive-canonical-serialization owns changing those readers and writers. This prerequisite extends existing typed reference/condition/value definitions, not a new domain entity.

## Acceptance

An opt-in outcome profile compares declared decimal-string references by exact numeric value through input selection and value/entity invariants, preserves original wire spellings in state/events/records, reproduces decisions on complete replay, and leaves profiles 1-8 and prior readers unchanged.

## Design

R-134, docs/design/kernel-decimal-operands-v9.md. Outcome profile 9 adds explicit $decimal.args.amount (and corresponding scoped fields/bound references) only in predicate operands. Registration requires a declared String field with DecimalText encoding, including nullable wrappers; no arbitrary text/JSON coercion or template conversion. Missing/null is unobserved. Grammar-valid values become exact JSON-number observations internally, while source values and literals are unchanged. Existing numeric comparison/membership and scalar truthiness contracts apply. No arithmetic, normalization, time/IO or dependency is added.

## Scope

crates/entity-core; docs/design/kernel-decimal-operands-v9.md; docs/requirements.md; CHANGELOG.md. Verify independent numeric boundary vectors, local scopes, malformed values, refusal/replay, exact old-reader bytes, guard mutation and targeted compatibility/lint/docs/MSRV/requirements. No full, database, ownership, remote correctness or release gate.

## State

Initial status retained under repository lifecycle policy. ESS adoption must opt into the published profile explicitly, compare representable cases against its existing conformance oracle, and state legacy literal/canonical limits without claiming the canonical-number migration is complete.

## Verification

Implemented exact decimal observations in profile 9, retaining typed identity and all profile-8 capabilities. Five independent behavioral tests cover exact ordering/equality/membership, unchanged wire spelling, absence/null/grammar and existing finite-conversion truthiness, profile/schema/template refusal, lexical value invariants and quantifiers, state/event production and full replay tampering.

Local verification: cargo test --offline --locked -p entity-core -p entity-graph -p entity-surface passed including doctests; strict entity-core Clippy all targets, Rust 1.85.0 check all targets, rustdoc with denied warnings, formatting and diff checks passed. Requirement checker: 109 requirements, 390 test functions, zero findings; changelog self-test 7/7. Build targets are task-owned under /tmp, two jobs with debug information and incremental compilation disabled. No full, database, ownership, remote correctness or release gate was run.

The independent old-reader probe compares representative definition/record bytes and replay for profiles 1–8 against exact published f9f0545e66fbde40e6eb36e92d71f325dfd0d86c, and proves that reader refuses profile-9 definitions/records. This is compatibility evidence for the probe corpus, not a claim of exhaustive equivalence. Replacing decimal parsing with f64 made the precision test fail on 9007199254740993 versus 9007199254740992; production code was restored byte-for-byte before the passing package run. Initial fixture spelling errors were corrected before verification.

Logs: local-evidence:ess-evolution-20260910/er-decimal-{tests,packages,mutation,probe,clippy,msrv,rustdoc}.log. Probe source and locked dependencies are retained under er-decimal-probe/. ESS adoption and its legacy representable-value conformance remain the next step; ESS literal readers and canonical-number migration are unchanged.

## Publication

Implementation c05710cab9f1aa4ff9086d620997ce18c0f08a32 is published on origin/feat/decimal-outcome-operands through standalone b10x-gates bot. Both author and committer were verified as b10x-bot[bot]. Remote main eaf43090636abce025649e565e5af271c4513eae remains an ancestor. The remote acknowledged the App-authorized branch-creation bypass; no protection was changed and no PR or remote correctness gate was triggered.

The compatibility probe now consumes both current c05710cab9f1aa4ff9086d620997ce18c0f08a32 and previous f9f0545e66fbde40e6eb36e92d71f325dfd0d86c from exact public Git revisions, with no worktree path dependency. It passes representative profiles 1–8 byte/replay comparisons and old-reader profile-9 refusal; local-evidence:ess-evolution-20260910/er-decimal-published-probe.log. Preserve that source/lock/log, reclaim the three owned /tmp build targets, then finish and garbage-collect only wt-cedb56a53ec3 after publishing this receipt. Story remains draft under the repository's specific-move policy; ESS adoption continues in its existing active lowering story.
