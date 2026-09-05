---
sidebar_position: 7
title: Generate entity documentation
description: Produce a browsable reference, OpenAPI, and AsyncAPI from one validated definition set.
---

# Generate entity documentation

From the directory containing `refund.yaml` in the [quickstart](./getting-started):

```bash
entity generate docs \
  --definition refund.yaml \
  --out ./refund-reference
```

The output is a static bundle:

```text
refund-reference/
├── index.html
├── index.md
├── entities/refund.html
├── entities/refund.md
├── openapi.yaml
├── openapi.json
├── asyncapi.yaml
├── asyncapi.json
└── assets/style.css
```

Open `refund-reference/index.html` locally after generation. The OpenAPI and AsyncAPI files
are beside it; this guide does not publish a second generated copy of those contracts.
Use the release-pinned definition from the [quickstart](./getting-started) to reproduce the example.

## What an entity page explains

Each page shows every known version, property types and constraints, lifecycle graph, operation
transitions and arguments, named rule messages, emitted events, projections, and typed references.
The index shows relationships across the whole definition set.

The HTML pages embed deterministic SVG. Markdown pages carry Mermaid state diagrams and a Mermaid
reference flowchart, so they remain useful when copied into another documentation system.

## What the API files mean

OpenAPI describes the HTTP facade an adopter can implement: create, get, list, events, and named
operations. It is a contract, not a hidden server—Entity Runtime opens no HTTP listener. Operation
requests include `expected_revision` and recording provenance. Authentication and authorization
must be designed in the implemented facade; schema validity alone grants no authority.

AsyncAPI describes the domain events a successful decision materializes. Event payload properties
retain the schema of referenced fields and operation arguments rather than collapsing to arbitrary
JSON. It declares no broker: publishing is a shell responsibility after durable recording.

These outputs derive from Entity Runtime definitions, not ESS documents. See the
[system model](../system-model) for the complete projection boundary.

## Safe regeneration

The generator refuses an existing directory. `--force` works only when the directory carries the
Entity Runtime generator marker, then stages the complete replacement before publishing it. It
will not erase an arbitrary directory that happens to share the requested name.
