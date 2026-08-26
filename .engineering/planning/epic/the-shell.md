---
format: aep.planning-md/1
id: epic:the-shell
kind: epic
status: implemented
title: 'The shell: this repository can hold what it decides about'
summary: entity-core decides and nothing here holds anything. R-80 describes a shell that loads, calls the kernel, then persists, appends, projects and publishes together; no such shell exists. R-82 places the provider interfaces outside the core and docs/requirements.md:175 says the crate does not exist yet.
relations:
- decomposes: initiative:entity-runtime
revision: 4
---
# Epic: The shell

## Outcome

Somebody can run this runtime against something that survives the process exiting. Today
`entity execute` prints a `Decision` and forgets it: the state it produced has nowhere to go, so
every demonstration of this repository is a demonstration of one operation in isolation. After this
epic, a decision is committed — instance and events together — read back, folded back, and projected
into a read model, by a crate that is not `entity-core` and never becomes one.

## Why Now

R-80 and R-82 describe this shell in the requirements register and nothing implements them;
`docs/requirements.md:175` says so in as many words. That is the register claiming a boundary the
code does not have, which is the defect this repository's own gate exists to catch — and it has been
sitting in the register since 0.1.0. The second reason is `engineering-protocols`: phase 5 of the
adoption roadmap gives their planning store this kernel's events as its journal, and it cannot take
a journal from a kernel with nothing to write it into.

## Scope

`entity-store` — the provider traits, the memory and file implementations, the event envelope, the
projection evaluator and the conformance suite. `entity-sqlite` — one implementor that can write
state and events in a single transaction, so the together-or-not-at-all claim has something to be
tested against. The kernel gains a fold (`replay`) and events gain the fields a fold needs.
The CLI gains `--store`.

## Out of Scope

Search and blob providers, which R-82 names and no adopter has asked for. A server: nothing here
opens a socket, and the network is the next epic's problem. Migrations between definition versions,
which is its own story and its own decision about who advances an instance.

## Risks

The traits are the seam every later provider is narrowed against, so getting them wrong is expensive
in a way the code inside them is not — mitigated by writing the conformance suite *with* the traits
and running it against a deliberately broken provider, which is what would show a suite that passes
anything. The second risk is purity drift: a store crate is exactly where a clock or a filesystem
call gets pulled back into the core by a convenient re-export. `entity-core`'s scan holds it, and
this epic does not relax it.

## Done When

`Store::commit` takes a whole `Decision`; three providers pass one suite; the suite catches a
provider that is deliberately wrong; a definition's declared projections produce the same read model
every run; an instance folds back out of its events; and `entity-core` still reaches no clock, no
filesystem and no network.
