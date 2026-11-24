---
format: aep.planning-md/3
id: story:pin-rustc-with-rust-toolchain-toml
kind: story
status: draft
title: One rust-toolchain.toml pins the workspace rustc for the local gate and CI
summary: No rust-toolchain.toml exists; the Gate, Slow, release and pages jobs install stable
owner: entity-runtime
revision: 1
---
## Outcome

One `rust-toolchain.toml` at the repository root pins the rustc of the workspace, and the local
gate and every CI job that builds the workspace read it, so a new stable release cannot turn a
green local gate red on the forge.

## Why

`~/beyond10x/AGENTS.md` "The local gate equals the forge": `rust-toolchain.toml` pins one rustc and
both gates read it. Loom lost 48 minutes to rustc 1.100.0 on CI against 1.99.1 locally.

## Evidence

- No `rust-toolchain.toml` exists in the repository.
- `.github/workflows/gate.yml:55` (the Gate job), `slow.yml:58`, `release.yml:76` and `pages.yml:59`
  install `stable` through `dtolnay/rust-toolchain`, so each moves with every Rust release.
- Fixed versions today: `gate.yml:118` (MSRV 1.85.0, the declared `rust-version` in `Cargo.toml:28`),
  `gate.yml:158-160` and `kpi.yml:94-97,163-166` (1.91.0, the Eventlog toolchain), and the
  `cargo +1.91.0` calls in `Taskfile.yml`.

## Acceptance

- `rust-toolchain.toml` names an exact version for the workspace; the Gate, Slow lane, release and
  pages jobs install that version instead of `stable`.
- The 1.85 MSRV job and the 1.91.0 Eventlog jobs keep their own versions, and each version is
  named once in a place both the Taskfile and the workflow read.
- A check fails when a workflow names a toolchain that differs from the file.
- `slot task check` on the pinned version and the Gate job on the forge print the same
  `rustc --version`.

## Scope

- `rust-toolchain.toml` (new).
- `.github/workflows/gate.yml`, `slow.yml`, `release.yml`, `pages.yml`, `kpi.yml`.
- `Taskfile.yml` (the `+1.91.0` selectors).
- `AGENTS.md` (the section that names the toolchains).
