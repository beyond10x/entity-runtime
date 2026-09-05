---
sidebar_position: 6
title: Mount entities as MCP tools
description: Give an agent a schema-derived, stored tool surface with typed refusals and revision checks.
---

# Mount entities as MCP tools

Use the local stdio server when evaluating whether a model can understand and operate a definition:

```json
{
  "mcpServers": {
    "refunds": {
      "command": "entity",
      "args": [
        "mcp",
        "--definition", "/srv/entities/refund.yaml",
        "--store", "/srv/entities/refund-store"
      ]
    }
  }
}
```

The client discovers:

```text
refund.create
refund.get
refund.list
refund.events
refund.submit
refund.approve
refund.reject
```

The input schema for each named operation comes from that operation's declared arguments. The
server supports MCP 2026-07-28 and initialization-era 2025-11-25 clients.

## A stored operation call

```json
{
  "id": "refund-104",
  "expected_revision": 2,
  "arguments": {
    "actor_role": "human",
    "reason": "supervisor verified the evidence"
  },
  "recording": {
    "record_id": "request-104-approved",
    "recorded_at": "2026-08-31T10:04:00Z",
    "actor": "supervisor-7"
  }
}
```

The subject must first have been created and submitted, as in the [quickstart](./getting-started).
This call is a trusted-host example: the model must not choose its own `actor_role` or impersonate
`supervisor-7` through the recording fields.

The result is the persisted recorded decision. In 0.17.7, repeating this exact accepted call returns
the original revision-3 commit, even if state has since advanced. Keep the original expected
revision, arguments, and recording metadata. Changing intent under the same record ID is a
`record_conflict`; a new request based on a stale revision is a `revision_conflict`.
Kernel refusals likewise carry a stable `kind`, boundary, and human detail.
See [retry boundaries](./storage#retry-boundaries).

## Security boundary

- The mounted definition set and store path are operator configuration, never model inputs.
- `actor` is recorded provenance, not authentication. The host must derive or validate it.
- The server mints no identity, timestamp, or authority.
- Tool names are strict and collisions with `create`, `get`, `list`, or `events` are refused at
  startup.
- Stdout contains protocol messages only; diagnostics use stderr.

## Coverage

`events` reads emitted domain events; it does not return the complete decision/observation history.
The server exposes no general query, transaction-session, observation-write, or event-publication
tool. Those are separate provider or host integrations in the [system model](../system-model).
