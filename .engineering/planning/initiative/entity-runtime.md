---
format: aep.planning-md/1
id: initiative:entity-runtime
kind: initiative
status: implemented
title: Schema-driven entity runtime
summary: Entity types declared as data and executed by an IO-free deterministic kernel, offered as a Rust library and a CLI; the foundation the engineering-protocols artifact model is to be driven by.
revision: 6
---
# Initiative: Schema-driven entity runtime

## Outcome

Business objects across the estate — orders, tickets, and the planning artifacts
`engineering-protocols` governs — are declared once as data (schema, lifecycle, operations, rules,
events) and executed by one IO-free, deterministic kernel, so that a state change is an operation
with a name and rules or it does not happen.

## Scope

The kernel (`entity-core`), the YAML adapter, the `entity` command, the requirements register that
pins every property to a test, and the phased programme that makes `engineering-protocols` the
first adopter. Storage, search and messaging adapters are in scope only as interfaces outside the
kernel.

## Success

`engineering-protocols` evaluates its artifact status moves through this kernel with verdicts
identical to today's on the org's own planning stores, and its gap register closes the rows about a
closed status vocabulary and unchecked completion claims without a Rust change to its enums.

## Constraints

The kernel never admits IO; a refusal never changes state; the lifecycle state never gains a setter.
Anything that needs a clock, an identifier or a store is the shell's.

## Implementation evidence

Moved to implemented on 2026-09-09 after an audit of every non-implemented artifact against `CHANGELOG.md`, `docs/roadmap.md` and the crates.

The success clause is met: the kernel decides `aep artifact move` with the verdict-equivalence suite in `crates/entity-yaml/tests/aep_lifecycles.rs`, and the status vocabulary was opened without a Rust enum change — `docs/roadmap.md` § 2 rows A–C and the phase 2/4 rows.
