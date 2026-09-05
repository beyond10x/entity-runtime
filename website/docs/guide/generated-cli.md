---
sidebar_position: 8
title: Generate a Rust CLI
description: Build a definition-specific command, execute a complete refund workflow, and retry the original recorded request.
---

# Generate a definition-specific Rust CLI

The generator derives entity and operation subcommands from validated definitions and compiles a
host-platform executable. It embeds those definitions and delegates stored operations to
`StoredRuntime`. The top-level `entity` command and the generator are authored Rust; see the
[derivation map](../system-model#what-is-actually-derived).

## Prepare a matching source checkout

Install `entity` 0.17.7 from the [quickstart](./getting-started). Generation also needs Git,
Rust/Cargo, a matching runtime source checkout, and cached Cargo dependencies. These Bash commands
use a fresh directory so both the output and demonstration store start empty:

```bash
set -euo pipefail
refund_cli_dir="$(mktemp -d)"
cd "$refund_cli_dir"
git clone --depth 1 --branch 0.17.7 \
  https://github.com/beyond10x/entity-runtime.git runtime-source
cargo fetch --manifest-path runtime-source/Cargo.toml --locked
cp runtime-source/examples/refund.yaml refund.yaml
entity --version
```

Fetching dependencies is an explicit preparation step. Generation itself runs Cargo offline,
creates a lockfile, and builds with `--locked --offline`. Retain the generated source and lockfile
for rebuilding; dependency resolution during the initial generation uses the available cache.

## Build the command

```bash
env -u CARGO_TARGET_DIR -u CARGO_BUILD_TARGET entity generate rust-cli \
  --definition refund.yaml \
  --name refundctl \
  --out ./bin/refundctl \
  --runtime-source ./runtime-source
```

In 0.17.7 the generator expects Cargo's default host output location. Unsetting these overrides
prevents Cargo from placing the binary somewhere the generator will not find; a custom Cargo
configuration must likewise leave the generated project's target directory and build target at
their defaults.

The executable is `./bin/refundctl`. Its Clap-derived source and embedded YAML remain under
`build/entity-runtime/refundctl`. The command validates those definitions on startup and uses File
Store v2. It does not open an HTTP server or select a database service.

## Create, submit, and approve

These actor names, roles, and timestamps are demonstration inputs. In a real tool handler, derive
authority and recording metadata in trusted code.

```bash
./bin/refundctl --store ./refund-store refund create \
  --id refund-104 \
  --fields '{"order_id":"order-88","amount_cents":12500,"evidence_count":2}' \
  --record-id request-104-created --recorded-at 2026-08-31T10:00:00Z \
  --actor support-api

./bin/refundctl --store ./refund-store refund submit \
  --id refund-104 --expected-revision 1 \
  --record-id request-104-submitted --recorded-at 2026-08-31T10:01:00Z \
  --actor support-agent

./bin/refundctl --store ./refund-store refund approve \
  --id refund-104 --expected-revision 2 \
  --arguments '{"actor_role":"human","reason":"supervisor verified the evidence"}' \
  --record-id request-104-approved --recorded-at 2026-08-31T10:04:00Z \
  --actor supervisor-7 > approved.json

./bin/refundctl --store ./refund-store refund get --id refund-104 --format text
./bin/refundctl --store ./refund-store refund list
./bin/refundctl --store ./refund-store refund events --id refund-104
```

The subject is now approved at revision 3. `approved.json` is the persisted recorded commit.
`get` reads the current instance; `events` reads emitted domain events, not complete history or
observations. The generated command exposes neither arbitrary state patches nor provider
transaction/query capabilities.

## Recover an accepted request

Repeat the exact approval request with the original expected revision and recording metadata:

```bash
./bin/refundctl --store ./refund-store refund approve \
  --id refund-104 --expected-revision 2 \
  --arguments '{"actor_role":"human","reason":"supervisor verified the evidence"}' \
  --record-id request-104-approved --recorded-at 2026-08-31T10:04:00Z \
  --actor supervisor-7 > retried.json
cmp approved.json retried.json
```

The retry returns the original accepted commit; it makes no new transition and appends no duplicate
event. A new request with a stale revision is refused. Reusing a recorded identity with changed
intent is a record conflict. This behavior differs from generic
[`entity execute --store`](./storage#retry-boundaries), which loads current state and evaluates again.

Use `./bin/refundctl --help` and `./bin/refundctl refund approve --help` to inspect the generated
surface. All example files remain under `$refund_cli_dir` for inspection.

## Regeneration

`--force` authorizes replacement of the exact output binary and a source directory carrying the
generator marker. Regenerate from changed definitions and rebuild explicitly; editing a YAML file
beside an existing binary does not change its embedded model.
