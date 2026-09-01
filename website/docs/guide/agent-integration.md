---
sidebar_position: 5
title: Connect an agent safely
description: Expose named lifecycle operations to an agent without giving the model authority over canonical state or provenance.
---

# Connect an agent safely

The safe integration boundary is intentionally small: the agent proposes an operation and domain
arguments; trusted code decides everything about authority and durable state.

## Divide the inputs by trust

| Agent may propose | Trusted shell supplies |
|---|---|
| operation name from an allowlist | validated definition and version |
| evidence-derived reason | canonical current instance |
| domain arguments explicitly delegated to it | entity identity and expected revision |
| a request to escalate | authenticated actor and authority role |
| | record ID, timestamp, correlation, causation |

Do not expose a tool that accepts an arbitrary instance, definition, actor, or state patch. If a
policy needs the actor's role, inject that value into the operation arguments after authentication.

## Recommended tool shape

```json
{
  "name": "operate_refund",
  "input": {
    "id": "refund-104",
    "operation": "approve",
    "arguments": {
      "reason": "customer supplied delivery evidence"
    }
  }
}
```

The tool handler should:

1. allow only the definitions and operations intended for this agent;
2. load the instance from the authoritative store by `id`;
3. inject trusted values such as `actor_role`;
4. call `Runtime::execute` or `entity execute`;
5. return typed refusal JSON unchanged when the request is refused;
6. atomically commit an accepted recorded decision at the loaded revision; and
7. trigger downstream work only after the commit succeeds.

## Handle outcomes by kind

- `invalid_transition`: refresh state and reconsider the plan.
- `validation`: repair the named argument paths.
- `precondition_failed`: the observed facts contradict policy; choose another operation or escalate.
- `precondition_unobservable`: gather the named missing facts before retrying.
- `StoreError::RevisionConflict`: another writer won; reload before proposing anything else.
- `StoreError::Unreachable`: do not treat the entity as absent and do not invent replacement state.

Never parse the human sentence on stderr. The CLI writes kernel refusals with a `kind`; a File Store
refusal is `{ "refused": true, "by": "store", "detail": "..." }`. Both use exit `1` for a valid
request that policy or storage refused. Embedded Rust callers match `CoreError` and `StoreError`
variants directly.

## Install the CLI skill

The installed command can render a compact Agent Skills document for its exact version:

```bash
entity skill --out .agents/skills/entity/SKILL.md
```

The file teaches safe input forms, exit meanings, recording metadata, and File Store migration. The
stdout and file forms are byte-identical. An existing path is left untouched unless that exact
replacement is authorized:

```bash
entity skill --out .agents/skills/entity/SKILL.md --force
```

The skill teaches command use; it grants no authority. Repository instructions, tool allowlists,
authentication, and trusted input injection still define what the agent may do.

For model and harness evaluation, [`entity mcp`](./mcp) projects the same validated argument
schemas into tools such as `refund.submit` and `refund.approve`. Mutating calls require recording
metadata and the revision the model observed, so stale intent is measurable rather than silently
applied to newer state.

## Test the boundary, not only the happy path

For every operation exposed to an agent, test:

- a legal transition;
- the same operation from an illegal state;
- invalid and missing arguments;
- every policy refusal;
- missing evidence and all reported unresolved paths;
- a concurrent revision conflict;
- retrying the same record ID with identical and different bytes; and
- that no refusal changed state or emitted an event.

See [Typed refusals](./refusals) for the complete outcome reference and
[The Rust libraries](./library) for an embedded shell.
