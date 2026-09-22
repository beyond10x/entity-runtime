---
title: Service semantics and recorded execution
description: Select outcomes in the kernel, retain complete decisions, and retry through explicit storage authority.
---

Definitions may opt into `semantics: service/1`, `service/2` or `service/3`.
Existing definitions retain their earlier behavior; selecting a newer format is explicit.

## Outcomes belong to the definition

A service operation declares ordered, named outcomes. Each can have a guard, an effect, field
changes, events, a response or a refusal. The kernel selects the outcome from validated inputs.
The caller requests an operation and cannot choose its outcome.

`decide` and `decide_create` return a refusing branch as `Evaluation::Refused`.
The retained `create` and `execute` entry points expose that refusal through `CoreError::Refused`.

Service definitions can declare logical identity separately from the storage address, typed
relations, maps, adjacent unions and finite binary64 values. Registration checks the definition
and reference shapes. The shell still establishes whether a referenced instance exists.

## What each format adds

| Semantics | Additional contract | Complete record/request framing |
|---|---|---|
| `service/1` | Named outcomes, effects and responses | `er.record/2`, `er.request/2` |
| `service/2` | Preserve absence when copying an optional creation argument | `er.record/3`, `er.request/3` |
| `service/3` | Typed Set, Preserve and optional Remove actions after selecting the loaded outcome | `er.record/4`, `er.request/4` |

Events, responses and replay use the same resulting values. Exact retry retains the original
actions. Older formats keep their own bytes; do not open a store with a reader that predates its
record framing.

`create_derived` and `decide_create_derived` derive a storage address from the selected
creation outcome's validated logical identity. A circular pre-address `$id` read refuses.

## Record the complete decision

Recorded execution retains the normalized request, definition snapshot, result, response and
events with explicit metadata. The async ports and executor support exact single-command and
named-batch retries. They do not select a clock, credential or tenant for the application.

Keep the record identity, request, metadata and expectation stable across a retry, including after
an uncertain commit. A changed request under an occupied identity is a conflict. A refusal never
authorizes a second write under a new identity.

## Provision, open and import separately

The Eventlog-backed File, SQLite and PostgreSQL facades preserve complete receipts, observations,
queries and atomic groups. Opening an existing authority and provisioning a new one are distinct
operations. The host supplies storage authority, tenant/scope, capture bounds and deadlines.

The retained `FileStore`, `SqliteStore` and `PostgresStore` are still available. They expose
read-only typed acquisition for explicit **out-of-place** import. Merely opening a newer provider
does not convert legacy data.

An imported boundary records what was acquired; it is not proof of execution from genesis.
Source-bound anchors distinguish identical histories acquired from different sources.
`AsyncImportedAnchorWriter::import_anchors` imports a bounded batch atomically. If the batch
would exceed the handle's read bounds, `BatchExceedsReadBounds` asks the caller to divide it.

See [storage](storage.md) for provider selection and [release highlights](../releases.md) for the
compatibility changes in 0.19.0.
