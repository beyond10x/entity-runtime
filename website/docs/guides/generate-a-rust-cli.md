---
sidebar_position: 6
title: Generate a Rust CLI
description: Build a definition-specific command, run a complete refund workflow with it, and recover an accepted request.
lede: entity generate rust-cli compiles a command with one subcommand per entity and operation, embedding the definitions it was built from.
source: "crates/entity-cli (generate rust-cli, generated_main), crates/entity-shell (StoredRuntime), run with entity 0.27.0"
---

# Generate a Rust CLI

The generator derives entity and operation subcommands from validated definitions, writes a Rust
crate that uses clap derive, and compiles it for the host. The command embeds the definitions,
validates them on startup and runs stored operations through `StoredRuntime` on a File Store. It
opens no HTTP server and selects no database.

## Prepare a matching source checkout

Generation needs Git, Rust and Cargo, a source checkout of the same Entity Runtime version as the
`entity` you run, and that checkout's dependencies in Cargo's cache. In a fresh directory:

```bash
git clone --depth 1 --branch 0.27.0 \
  https://github.com/beyond10x/entity-runtime.git runtime-source
cargo fetch --manifest-path runtime-source/Cargo.toml --locked
cp runtime-source/examples/refund.yaml refund.yaml
```

Fetching is the explicit network step. Generation itself runs Cargo with `--locked --offline`.

## Build the command

```bash
entity generate rust-cli \
  --definition refund.yaml \
  --name refundctl \
  --out ./bin/refundctl \
  --runtime-source ./runtime-source
```

It prints Cargo's build output, then the binary and the retained source (shown with the working
directory abbreviated to `…`):

```text
generated refundctl at ./bin/refundctl; source retained at …/build/entity-runtime/refundctl
```

The crate and its lockfile stay under `build/entity-runtime/refundctl` (`--build-dir` moves them)
so the binary can be rebuilt. The generator builds into that directory's own `target` and installs
the executable Cargo reports, so `CARGO_TARGET_DIR` or a `.cargo/config.toml` override changes
neither where the build lands nor where the result is found.

## Run a refund through it

The actor names, roles and timestamps are sample data; in a real tool handler trusted code derives
them.

```bash
./bin/refundctl --store ./refund-store refund create \
  --id refund-104 \
  --fields '{"order_id":"order-88","amount_cents":12500,"evidence_count":2}' \
  --record-id request-104-created --recorded-at 2026-08-31T10:00:00Z \
  --actor support-api > created.json
./bin/refundctl --store ./refund-store refund submit \
  --id refund-104 --expected-revision 1 \
  --record-id request-104-submitted --recorded-at 2026-08-31T10:01:00Z \
  --actor support-agent > submitted.json
./bin/refundctl --store ./refund-store refund approve \
  --id refund-104 --expected-revision 2 \
  --arguments '{"actor_role":"human","reason":"supervisor verified the evidence"}' \
  --record-id request-104-approved --recorded-at 2026-08-31T10:04:00Z \
  --actor supervisor-7 > approved.json
```

```shell-session
$ ./bin/refundctl --store ./refund-store refund get --id refund-104 --format text
refund refund-104 is approved (revision 3)
$ ./bin/refundctl --store ./refund-store refund list
refund-104
$ ./bin/refundctl --store ./refund-store refund events --id refund-104 | jq -c '.[] | {type: .type, revision}'
{"type":"RefundDrafted","revision":1}
{"type":"RefundSubmitted","revision":2}
{"type":"RefundApproved","revision":3}
```

Every operation subcommand requires `--expected-revision`; `get` reads the current instance and
`events` the emitted domain events, not the full history.

## Recover an accepted request

Repeat the approval exactly — same revision, arguments and recording — and the original commit
comes back; a new request on the old revision is refused:

```shell-session
$ ./bin/refundctl --store ./refund-store refund approve \
    --id refund-104 --expected-revision 2 \
    --arguments '{"actor_role":"human","reason":"supervisor verified the evidence"}' \
    --record-id request-104-approved --recorded-at 2026-08-31T10:04:00Z \
    --actor supervisor-7 > retried.json
$ cmp approved.json retried.json && echo identical
identical
$ ./bin/refundctl --store ./refund-store refund reject \
    --id refund-104 --expected-revision 2 --arguments '{"reason":"too late"}' \
    --record-id request-104-rejected --recorded-at 2026-08-31T10:05:00Z --actor supervisor-7
refused: refund refund-104: expected revision 2, found revision 3
```

The refusal exits 1. `./bin/refundctl refund approve --help` lists the generated flags.

## Regenerate

`--force` replaces only the exact output binary and a source directory that carries the
generator's marker. Editing the YAML beside an existing binary changes nothing: regenerate to
change the embedded model.
