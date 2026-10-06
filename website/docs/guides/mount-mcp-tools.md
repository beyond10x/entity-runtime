---
sidebar_position: 4
title: Mount MCP tools
description: Give an agent schema-derived, stored tools with typed refusals and revision checks over stdio.
lede: entity mcp turns a definition set and a File Store into MCP tools whose schemas come from the definitions.
source: "crates/entity-mcp/src/lib.rs, crates/entity-cli (mcp), run with entity 0.27.0"
---

# Mount MCP tools

`entity mcp` serves newline-delimited JSON-RPC on standard input and output. The definition set and
the store path are operator configuration, never model input. A client configuration:

```json
{
  "mcpServers": {
    "refunds": {
      "command": "entity",
      "args": ["mcp", "--definition", "/srv/entities/refund.yaml", "--store", "/srv/entities/refund-store"]
    }
  }
}
```

The server answers `server/discover` with protocol versions `2026-07-28` and `2025-11-25`, and
`initialize` with `2025-11-25`. Standard output carries protocol messages only; diagnostics go to
standard error.

## Drive it from a file

To see exactly what a client sees, write the requests to `requests.jsonl`, one per line, with the
[getting-started](../getting-started.md) `refund.yaml` beside it:

```json
{"jsonrpc":"2.0","id":1,"method":"server/discover"}
{"jsonrpc":"2.0","id":2,"method":"tools/list"}
{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"refund.create","arguments":{"id":"refund-104","fields":{"order_id":"order-88","amount_cents":12500,"evidence_count":2},"recording":{"record_id":"request-104-created","recorded_at":"2026-08-31T10:00:00Z","actor":"support-api"}}}}
{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"refund.submit","arguments":{"id":"refund-104","expected_revision":1,"recording":{"record_id":"request-104-submitted","recorded_at":"2026-08-31T10:01:00Z","actor":"support-agent"}}}}
{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"refund.approve","arguments":{"id":"refund-104","expected_revision":2,"arguments":{"actor_role":"agent","reason":"customer supplied delivery evidence"},"recording":{"record_id":"request-104-approved-by-agent","recorded_at":"2026-08-31T10:02:00Z","actor":"support-agent"}}}}
{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"refund.approve","arguments":{"id":"refund-104","expected_revision":2,"arguments":{"actor_role":"human","reason":"supervisor verified the evidence"},"recording":{"record_id":"request-104-approved","recorded_at":"2026-08-31T10:04:00Z","actor":"supervisor-7"}}}}
{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"refund.approve","arguments":{"id":"refund-104","expected_revision":2,"arguments":{"actor_role":"human","reason":"supervisor verified the evidence"},"recording":{"record_id":"request-104-approved","recorded_at":"2026-08-31T10:04:00Z","actor":"supervisor-7"}}}}
{"jsonrpc":"2.0","id":8,"method":"tools/call","params":{"name":"refund.reject","arguments":{"id":"refund-104","expected_revision":2,"arguments":{"reason":"too late"},"recording":{"record_id":"request-104-rejected","recorded_at":"2026-08-31T10:05:00Z","actor":"supervisor-7"}}}}
```

```bash
entity mcp --definition refund.yaml --store ./mcp-store < requests.jsonl > responses.jsonl
```

The tools are one per entity type for `create`, `get`, `list` and `events`, and one per operation;
each operation's input schema comes from its declared arguments:

```shell-session
$ jq -r 'select(.id==2) | .result.tools[].name' responses.jsonl
refund.approve
refund.create
refund.events
refund.get
refund.list
refund.reject
refund.submit
$ jq -c 'select(.id==2) | .result.tools[] | select(.name=="refund.approve") | .inputSchema.required' responses.jsonl
["id","expected_revision","arguments","recording"]
```

The agent's approval of a refund above 5000 cents is refused by the rule; the human approval
commits revision 3:

```shell-session
$ jq -c 'select(.id==5) | .result.structuredContent' responses.jsonl
{"by":"kernel","detail":"precondition 'large_refunds_need_a_human' failed for operation 'approve': refunds above 5000 cents require a human actor","kind":"precondition_failed","refused":true}
$ jq -c 'select(.id==6) | .result.structuredContent | {revision: .instance.revision, state: .instance.lifecycle_state, record: .envelope.record_id}' responses.jsonl
{"revision":3,"state":"approved","record":"request-104-approved"}
```

Request 7 repeats request 6 exactly and gets the identical original result back, with nothing
written twice. Request 8 is a new request decided on a stale revision:

```shell-session
$ cmp <(jq -c 'select(.id==6)|.result' responses.jsonl) <(jq -c 'select(.id==7)|.result' responses.jsonl) && echo identical
identical
$ jq -c 'select(.id==8) | .result.structuredContent' responses.jsonl
{"by":"store","detail":"refund refund-104: expected revision 2, found revision 3","kind":"revision_conflict","refused":true}
```

## Security boundary

- `actor` is recorded provenance, not authentication. The server writes what the call says, so
  a model must not be able to set `actor_role` or the recording fields itself; put a wrapper that
  derives or checks them in front of an untrusted agent.
- The server mints no identity, timestamp or authority.
- Tool names are strict; a definition whose operation would collide with `create`, `get`, `list`
  or `events` is refused at startup.
- Reusing a record id for a different request is `record_conflict`; a new request on an old
  revision is `revision_conflict`.

## Coverage

`events` reads emitted domain events, not the complete decision and observation history. The
server has no query, transaction, observation-write or event-publication tool, and it stores only
into a File Store.
