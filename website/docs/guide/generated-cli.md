---
sidebar_position: 8
title: Generate a Rust CLI
description: Compile validated definitions into a host-platform command with direct entity operations.
---

# Generate a definition-specific Rust CLI

```bash
entity generate rust-cli \
  --definition refund.yaml \
  --name refundctl \
  --out ./bin/refundctl \
  --runtime-source /src/entity-runtime
```

The output is the executable for the current platform. Its generated Clap-derived source remains
under `build/entity-runtime/refundctl` by default for inspection and reproducible rebuilding.

Generation uses a matching local runtime checkout and runs Cargo with `--locked --offline`. Populate
the Cargo cache before generation; the `entity` command itself does not fetch code.

## Commands from the definition

```bash
refundctl --store ./refund-store refund create \
  --id refund-104 \
  --fields '{"order_id":"order-88","amount_cents":12500,"evidence_count":2}' \
  --record-id request-104-created --recorded-at 2026-08-31T10:00:00Z \
  --actor support-api

refundctl --store ./refund-store refund get --id refund-104 --format text
refundctl --store ./refund-store refund list
refundctl --store ./refund-store refund events --id refund-104
```

Definition operations become direct subcommands:

```bash
refundctl --store ./refund-store refund approve \
  --id refund-104 --expected-revision 2 \
  --arguments '{"actor_role":"human","reason":"supervisor approved"}' \
  --record-id request-104-approved --recorded-at 2026-08-31T10:04:00Z \
  --actor supervisor-7
```

The generated command embeds the exact definitions, validates them on startup, and uses File Store
v2. A stale revision, rule refusal, or invalid value produces no partial state or events.

`--force` replaces only the exact output binary and a generated source directory carrying the
generator marker.
