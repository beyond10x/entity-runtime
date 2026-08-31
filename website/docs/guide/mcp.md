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

The result is the persisted recorded decision. If the store now holds revision 3, the same call is
an actionable `revision_conflict` tool error and changes nothing. Kernel refusals likewise carry a
stable `kind`, boundary, and human detail.

## Security boundary

- The mounted definition set and store path are operator configuration, never model inputs.
- `actor` is recorded provenance, not authentication. The host must derive or validate it.
- The server mints no identity, timestamp, or authority.
- Tool names are strict and collisions with `create`, `get`, `list`, or `events` are refused at
  startup.
- Stdout contains protocol messages only; diagnostics use stderr.
