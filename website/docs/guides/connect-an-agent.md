---
sidebar_position: 3
title: Connect an agent
description: Expose named lifecycle operations to an agent without giving the model authority over canonical state or provenance.
lede: The agent proposes an operation and its domain arguments; trusted code supplies everything about authority and durable state.
source: "crates/entity-shell (StoredRuntime, ShellError::kind), crates/entity-cli (skill, refusal output), crates/entity-cli/assets/entity-skill.md, run with entity 0.27.0"
---

# Connect an agent

The safe integration boundary is small: the agent proposes an operation and domain arguments;
trusted code decides everything about authority and durable state. [The decision
boundary](../concepts/decision-boundary.md) explains why.

## Divide the inputs by trust

| The agent may propose | The trusted shell supplies |
|---|---|
| an operation name from an allowlist | the validated definition and its version |
| an evidence-derived reason | the canonical current instance |
| the domain arguments delegated to it | the entity id and the expected revision |
| a request to escalate | the authenticated actor and its authority role |
| | the record id, timestamp, correlation and causation |

Do not expose a tool that accepts an arbitrary instance, definition, actor or state patch. When a
rule needs the caller's role — `actor_role` in the refund example — inject it into the operation
arguments after authentication.

## Write the tool handler

A tool input the model may fill:

```json
{
  "id": "refund-104",
  "operation": "approve",
  "arguments": { "reason": "customer supplied delivery evidence" }
}
```

The handler:

1. allows only the definitions and operations meant for this agent;
2. loads the instance from the authoritative store by `id`;
3. keeps the revision the proposal was based on, and injects trusted values such as `actor_role`;
4. calls `StoredRuntime::execute` with that expected revision and a `Recording` it built;
5. returns a refusal's JSON unchanged;
6. reports success only after the recorded decision committed at the expected revision; and
7. starts downstream work only after that commit.

`StoredRuntime` (in `entity-shell`) performs the load, the retry check, the decision and the
recorded commit; do not commit its result a second time. The [embed the kernel](./embed-the-kernel.md)
guide shows the kernel calls underneath.

## Handle outcomes by kind

Every refusal the shell returns carries a stable `kind` and the boundary that refused:

| `kind` | Boundary | What the agent should do |
|---|---|---|
| `invalid_transition` | kernel | refresh the state and reconsider the plan |
| `validation` | kernel | repair every named argument path |
| `precondition_failed` | kernel | the facts contradict the policy: choose another operation or escalate |
| `precondition_unobservable` | kernel | gather the facts named in `unresolved`, then retry |
| `refused` | kernel | a declared outcome refused; read its `error` and `reason` |
| `revision_conflict` | store | another writer won: reload before proposing anything else |
| `record_conflict` | store | the record id already names a different request; never pick a new id silently |
| `store_unreachable` | store | do not treat the entity as absent and do not invent state |

The `entity` command prints a kernel refusal as JSON with its `kind` and a store refusal as
`{"refused": true, "by": "store", "kind": …, "detail": …}`, both with exit `1`, and a sentence on
standard error. Programs match the JSON fields, never the sentence. Rust callers match `CoreError`
and `ShellError` variants. [Typed refusals](../reference/refusals.md) lists every kind.

## Install the CLI skill

`entity skill` renders an Agent Skills document for the exact installed version: safe input forms,
exit codes, recording metadata and File Store migration.

```bash
entity skill --out .agents/skills/entity/SKILL.md
```

Standard output and the file are byte-identical. An existing file is left alone unless that exact
replacement is asked for:

```shell-session
$ entity skill --out .agents/skills/entity/SKILL.md
error: .agents/skills/entity/SKILL.md already exists; pass --force to replace that exact file
$ entity skill --out .agents/skills/entity/SKILL.md --force
```

The skill teaches command use; it grants no authority. Repository instructions, tool allowlists,
authentication and trusted input injection still decide what the agent may do.

For model evaluation, [`entity mcp`](./mount-mcp-tools.md) projects the same argument schemas into
tools such as `refund.approve`. Its operation tools take recording metadata and the revision the
model observed, so stale intent is refused rather than applied to newer state. The MCP server
passes those fields through as the model wrote them: an untrusted agent needs a wrapper that sets
or checks them first.

## Test the boundary, not only the happy path

For every operation exposed to an agent, test:

- a legal transition, and the same operation from an illegal state;
- invalid and missing arguments;
- every policy refusal, and missing evidence with every unresolved path;
- a concurrent revision conflict;
- retrying one record id with identical and with different bytes; and
- that no refusal changed state or emitted an event.
