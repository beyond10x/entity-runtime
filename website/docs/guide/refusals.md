---
sidebar_position: 9
title: Typed refusals
description: Understand kernel, definition, and storage refusal kinds and decide whether to repair, gather evidence, reload, or escalate.
---

# Typed refusals

A refusal means the runtime understood the request and declined to produce or store the proposed
change. Refusals are ordinary control flow for an agent integration, not partial failures.

## Kernel refusals

| Kind | Meaning | Typical response |
|---|---|---|
| `entity_not_registered` | no validated definition for `(entity, version)` | load the intended definition set |
| `entity_mismatch` | instance and definition identities differ | refuse the caller-supplied instance |
| `unknown_state` | instance claims a state absent from its definition | repair the trusted store or migration |
| `revision_exhausted` | another successful revision cannot be represented | stop; do not wrap or reset history |
| `operation_not_found` | definition declares no such operation | inspect allowed operations and replan |
| `invalid_transition` | operation is unavailable from the current state | reload context and choose a legal operation |
| `validation` | fields or arguments violate their schema | repair every returned path |
| `precondition_failed` | observed facts contradict an operation rule | choose another operation or escalate |
| `precondition_unobservable` | rule needs facts that were not observed | gather every path in `unresolved` |
| `invariant_violation` | the resulting entity would be invalid | do not bypass; fix modeling or input |
| `invariant_unobservable` | resulting validity depends on missing facts | gather or model the evidence explicitly |
| `template` | a runtime template path cannot resolve | repair definition/input; never substitute null |

Definition parsing and registration failures use `kind: definition` and include `defect` plus a
`defects` array when multiple independent defects were accumulated. Fix the definition before
exposing it to an agent.

## Store refusals

Rust callers match `StoreError` variants. CLI File Store operations serialize
`{ "refused": true, "by": "store", "detail": "..." }`; the human detail preserves the provider's
reason, but the CLI does not expose the Rust variant as a JSON `kind`.

| Store outcome | Meaning | Response |
|---|---|---|
| `RevisionConflict` | stored revision differs from the expectation | reload and re-run the decision |
| `RecordConflict` | record ID already names different bytes | investigate idempotency misuse; choose no replacement ID silently |
| `Unreachable` | provider could not be contacted | retry or follow declared offline policy; never treat as absent |
| `Backend` | provider itself failed | surface an operational error; do not spin on policy retries |

## False versus unobservable

These outcomes intentionally differ:

```json
{
  "kind": "precondition_failed",
  "rule": "large_refunds_need_a_human",
  "reason": "refunds above 5000 cents require a human actor"
}
```

The facts were present and policy said no.

```json
{
  "kind": "precondition_unobservable",
  "rule": "reviewed",
  "unresolved": ["$fields.review_score"]
}
```

Policy could not answer because evidence was absent. An agent should not handle those situations the
same way.

## The no-change guarantee

Kernel entry points take the instance by shared reference and return a new instance only on
success. Stores check expectations before committing. Therefore:

- no refused rule leaves assigned fields behind;
- no refused invariant emits events;
- no revision conflict overwrites the winning state; and
- no failed atomic batch commits a prefix.

Match variants or JSON `kind` fields. Human messages may improve without preserving exact wording.
