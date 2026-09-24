---
format: aep.planning-md/2
id: task:eventlog-provider-pin-verify-once
kind: task
status: implemented
title: Pin entity-eventlog to the verify-once Eventlog provider
owner: entity-runtime
relations:
- decomposes: story:eventlog-recorded-adapter-and-bridge
- serves: vision:O2
revision: 7
---
## Done when

`entity-eventlog` builds and its gate passes against the Eventlog provider at
c698923038de4413e0bbba3cd91ae108607d6be9 (story:file-eventlog-verifies-once-per-open in the
Eventlog store: history and blobs verified once per open, a transaction re-checks only what
changed), replacing the pin f802eb8b01b44ba04a93394b20f0c07391f7757a in
`crates/entity-eventlog/Cargo.toml` (four `git` dependencies) and in `Cargo.lock`, with no other
dependency change and no source change outside the pin.

## Why

Measured 2026-09-20 on a 64-artifact migrated AEP planning store: one `aep plan artifact list`
made 218 Eventlog transactions, 227,452 blob opens and read 2,863 MB, 79.7 s wall, because the
f802eb8 provider re-verified the whole store per transaction. The ER adapter makes one transaction
per entity read. With c698923, 200 read transactions after open cost 7 ms instead of 18.6–25.6 s
(Eventlog implementation report §6).

## Acceptance

- Every manifest in the workspace that names `github.com/beyond10x/eventlog` reads
  c698923038de4413e0bbba3cd91ae108607d6be9: `crates/entity-eventlog/Cargo.toml` (four `rev`
  values), `crates/entity-sqlite/Cargo.toml` (one), `crates/entity-postgres/Cargo.toml` (two),
  `crates/entity-cli/Cargo.toml` (one). **Eight sites**, one commit — counted by grep after
  independent pass 1 found the earlier count of seven one short. Two revs of one git URL are two
  crates to cargo, and each of those three crates depends on `entity-eventlog` and on
  `eventlog-core` directly and passes provider types across that boundary, so a split pin does not
  compile under `eventlog-facade` or `eventlog-providers`.
- `Cargo.lock`: every `git+https://github.com/beyond10x/eventlog?rev=…` source line reads the new
  rev; no other package changes.
- `task check` per step exit 0 (fmt-check, clippy, test, doc-check, example-check, req-check), with
  the same lanes that ran at 8b175736; PostgreSQL-dependent lanes stated as run or skipped.
- The three optional feature builds `task check` does not enable exit 0:
  `cargo check --locked -p entity-cli --features eventlog-providers`,
  `-p entity-sqlite --features eventlog-facade`, `-p entity-postgres --features eventlog-facade`.
- One measured number: the entity-eventlog test lane that opens a file store and performs repeated
  reads, wall time at 8b175736 versus after the pin, or the adapter's own benchmark if one exists.
- The CHANGELOG entry names the path the measurement supports. Independent pass 1 measured the read
  path unchanged through the adapter (230.2/245.2 ms vs 249.7/237.2 ms) and the commit path
  shortened (48 commits in 29.97/53.22 s vs 74.86/65.54 s).

## Scope

cited: crates/entity-eventlog/Cargo.toml, crates/entity-sqlite/Cargo.toml,
crates/entity-postgres/Cargo.toml, crates/entity-cli/Cargo.toml, Cargo.lock, CHANGELOG.md.

The three manifests beyond `entity-eventlog` were added 2026-09-21 by the sub-operator after the
implementor demonstrated that leaving them at f802eb8 resolves two `eventlog-core` crates and fails
three optional feature builds with E0308 (implementation-report.md §0, §3).
