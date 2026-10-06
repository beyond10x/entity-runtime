---
sidebar_position: 5
title: Generate entity documentation
description: Produce browsable entity pages, OpenAPI and AsyncAPI from one validated definition set.
lede: entity generate docs writes a static bundle of entity pages and API contracts, and replaces only a directory it generated.
source: "crates/entity-surface, crates/entity-cli (generate docs, install_documentation), run with entity 0.27.0"
---

# Generate entity documentation

From the directory holding the [getting-started](../getting-started.md) `refund.yaml`:

```shell-session
$ entity generate docs --definition refund.yaml --out ./refund-reference
generated 10 file(s) for 1 definition(s) at ./refund-reference
$ find refund-reference -type f | sort
refund-reference/assets/style.css
refund-reference/asyncapi.json
refund-reference/asyncapi.yaml
refund-reference/entities/refund.html
refund-reference/entities/refund.md
refund-reference/.entity-runtime-docs.json
refund-reference/index.html
refund-reference/index.md
refund-reference/openapi.json
refund-reference/openapi.yaml
```

Open `refund-reference/index.html` in a browser; the bundle needs no server.

## What an entity page shows

Each entity page lists every registered version, the field types and constraints, the lifecycle
graph, each operation's transitions and arguments, named rule messages, emitted events,
projections and typed references. The index shows the relationships across the whole set. HTML
pages embed SVG drawn by `entity-graph`; Markdown pages carry Mermaid diagrams, so they still work
when copied into another documentation system.

## What the API files mean

`openapi.yaml` describes an HTTP facade an adopter can implement — create, get, list, events and one
call per operation. It is a contract, not a server: Entity Runtime opens no HTTP listener.
Operation requests carry `expected_revision` and recording provenance; authentication and
authorization belong to whoever implements the facade.

`asyncapi.yaml` describes the domain events a successful decision produces. Event payload
properties keep the schema of the fields and arguments they come from. It names no broker:
publishing is the shell's job, after the decision is recorded.

## Regenerate safely

The generator refuses an existing directory, and `--force` replaces only a directory that carries
its marker file, `.entity-runtime-docs.json`:

```shell-session
$ entity generate docs --definition refund.yaml --out ./refund-reference
error: ./refund-reference already exists; pass --force to replace a generated directory
$ mkdir other
$ entity generate docs --definition refund.yaml --out ./other --force
error: ./other is not marked as entity-runtime generated documentation and will not be replaced
$ entity generate docs --definition refund.yaml --out ./refund-reference --force
generated 10 file(s) for 1 definition(s) at ./refund-reference
```

Both refusals exit 2. The replacement is staged completely before it is published.
