---
format: aep.planning-md/1
id: story:versioned-outcome-decisions
kind: story
status: draft
title: Execute and replay named command outcomes in the pure kernel
relations:
- informed_by: story:verifiable-decision-replay
- serves: vision:O2
scope:
- confidence: cited
  path: CHANGELOG.md
- confidence: cited
  path: crates/entity-core/src
- confidence: cited
  path: crates/entity-core/tests
- confidence: cited
  path: docs/design/kernel-outcomes-v1.md
- confidence: cited
  path: docs/requirements.md
revision: 3
---
## Outcome
Implement the next pure-kernel requirement from the ESS evolution semantic crosswalk: an opt-in versioned command/decision profile with kernel-owned named outcome selection, atomic multi-event creation, accepted and refused observations, and replay that recomputes the selection. Preserve the legacy definition and decision reader bytes and APIs.

## Authority
The operator-authorized ESS evolution migration step 3 requires missing semantics in ER before lowering. Crosswalk: ESS docs/design/ess-evolution/semantic-crosswalk.md at 568ee56936f26eb1704479886cadf9d9f3ffef19. This extends the existing entity/command/decision model; it introduces no new business entity or reverse ESS dependency. This interactive run does not claim operator approval for lifecycle moves or branch-protection changes.

## Acceptance
The pure kernel executes and replays named conditional/default/external/wrong-state outcomes with exact events and state, records zero-mutation observations including refused creation, refuses ambiguous or unevaluable selection and altered records, and keeps old definitions/readers behavior intact.

## Scope
Cited: crates/entity-core/src, crates/entity-core/tests, docs/design/kernel-outcomes-v1.md, docs/requirements.md and CHANGELOG.md. The new profile initially reuses the existing closed schema and predicate vocabulary; its missing nullable/map/union/quantifier and codec capabilities remain explicit prerequisites for complete ESS lowering. No runtime application is opted in by this change.

## Verification
Use independent conditional/create/refusal/replay vectors, old-reader refusal and old-kernel tests, the kernel purity check, strict affected Clippy, Rustdoc and Rust 1.85 compilation. Break a selection/replay guard and verify its corresponding test fails before restoring it. Do not run a full gate or remote persistence workflow.

## Implemented profile

entity_core::outcome now exposes closed entity-outcome-definition/1 and entity-outcome-record/1 envelopes. Registration checks every branch, prepares changing branches through the existing kernel, restricts creation/input reference scopes, and checks typed event/error payload schemas. Selection evaluates normalized inputs and separately declared Boolean observations: Unknown refuses rather than becoming otherwise, and multiple true branches remain an ambiguity with no implicit priority. Create returns one revision with all events; Change reuses the ordinary operation machinery. Observe and business Refuse preserve optional state, including a refused creation with no invented entity/revision. Replay reruns original invocations and rejects substituted definitions, identity changes, altered outcome names, results and event vectors. Existing readers and persisted types retain their contracts.

The nine independent outcome tests pass, along with all existing core unit/requirements/replay tests and the purity checks. Strict affected all-target Clippy, Rustdoc with warnings denied, and Rust 1.85 all-target compilation pass. Logs: local-evidence:ess-evolution-20260910/er-outcomes-final.log, er-outcomes-kernel-final.log, er-outcomes-clippy-final.log, er-outcomes-rustdoc.log and er-outcomes-msrv-final.log. The requirement checker reports no findings (er-outcomes-requirements.log). No full gate ran and no live provider or application was started.

## Failed checks and corrections

The first new reader round-trip test refused the existing serialized event key type because its closed-key list mistakenly named the Rust field event_type; the key list was corrected and round-trip replay passes. Disabling complete record comparison made replay_recomputes_selection_events_results_and_definition_identity fail on the altered selected outcome. The guard was restored byte-for-byte before the final core run; mutation output is er-outcomes-replay-mutation.log.

The affected purity run found that its existing substring scanner parsed the letters use inside Refuse as an import and underflowed while parsing braces. The scanner now recognizes a keyword boundary and whitespace, retaining actual imports across spaces, tabs and newlines. A regression includes refusal identifiers beside actual forbidden imports. The final kernel and purity checks pass; er-outcomes-kernel-tests.log retains the earlier failure.

## Remaining work

This is an opt-in entity-bound kernel profile, not full ESS execution or an application migration. Nullable typed values, quantified predicates, typed maps/unions, codec/primitive equivalence, subjectless outcome envelopes and multi-entity storage assembly still require implementation. ESS selection ambiguities remain visible rather than acquiring first-match semantics. The lowerer, SDK opt-in reader/adoption, current provider support for no-instance observations, SQL compatibility facades and real application proof remain unfinished. The ER story remains in its initial lifecycle state under this repository's operator-controlled move policy. No new consumer pin, release, main merge, protection change or approval is claimed.
