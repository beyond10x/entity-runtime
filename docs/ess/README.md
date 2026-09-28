# Executable ER library contracts

[`ess/ess-inputs.yaml`](../../ess/ess-inputs.yaml) is the canonical composition entry point for
the selected Entity Runtime library contracts. It lists the source declarations and every
authored scenario explicitly. [Release evidence](evidence/final/release/README.md) has been admitted;
authority adoption waits for the specification lifecycle to accept current typed coverage evidence.
Requirement IDs and design documents retain their traceability and rationale.

| Behavioral domain | In-process component | Contract register |
|---|---|---|
| `entity.core` | `entity-core` | [Core](core-traceability.md) |
| `entity.store` | `entity-store` | [Store](store-traceability.md) |
| `entity.executor` | `entity-executor` | [Executor](executor-traceability.md) |
| `entity.shell` | `entity-shell` | [Shell](shell-traceability.md) |
| `entity.query` | `entity-query` | [Query](query-traceability.md) |

Other ER crates remain covered by the existing repository gates and are outside this behavioral
specification. The composition also reuses the `entity.service` and `entity.recording` declarations
from the existing [service-semantics](../../ess/service-semantics/system.yaml) and
[recorded-execution](../../ess/recorded-execution/system.yaml) entry points. Those entry points
remain available. The [service-binding inventory](../design/models/service-binding-boundary/system.yaml)
is diagnostic material: inventory membership alone is not an executable behavioral assertion.
The corresponding requirements are traced to actual scenarios in the core register.

## Calls, values and observations

[`domains/core.yaml`](../../ess/domains/core.yaml) owns `entity.core.JsonDocument` and the nominal
definition, instance, decision and record document types. Consuming domains reference that owner.
These are lossless serialized public Rust values carried as ESS text. Definitions are dynamic
inputs; a particular example entity is a fixture, not a compiled domain in the adapter.

The checker enables `serde_json/arbitrary_precision`. JSON numeric tokens cross the adapter
without conversion through `f64` or ESS's numeric representation. ER's own `kernel/1` exact
comparison and `service/1` through `service/3` source-number behavior are then exercised by the
real APIs, including finite `binary64`, signed zero, rounding and out-of-domain refusals.
This does **not** claim native ESS `Binary64` admission: the document contract preserves ER's
actual wire input and asserts literal actual output. Unsupported native ESS numeric semantics
must not be replaced with a weaker numeric assertion.

Source declarations use `ess/17`; authored scenarios use `ess-scenario/4` and typed `response`
assertions. Each library command's `returned` outcome means that the Rust function returned its
result, including a typed error or refusal. The returned fields distinguish success, refusal,
decode failure and the concrete public error. A malformed synthesized document is observed as
such; the adapter does not substitute a valid fixture. Adapter faults remain runner errors.

[`ConformanceTarget`](../../checks/ess-conformance/src/target.rs) dispatches commands to the real
library adapters. Each scenario receives fresh state and isolated temporary storage. Expected
values live in the scenario files. Reads invoke public read APIs; observations include actual
state, history and receipts. Core decisions contain returned events and records: the pure kernel
does not persist or publish them. The replay fixture retains copies of actual returned records
in the scenario and passes those records to `replay`. It is not a storage capability.

| Implementation | Capability exercised |
|---|---|
| `MemoryStore` | Synchronous state, events and recorded history; ordered atomic batches |
| `FileStore` | Complete single-subject replacement, recorded history, reopen and migration; no multi-subject atomic-batch claim |
| `MemoryRecordedStore` | Async recorded ports, mixed atomic append, receipt/history recovery and existing failure controls |
| `StoredRuntime` | Shared synchronous operations over supplied memory and file providers |
| Query memory provider and ordered-input API | Recursive containment, stable identity ordering and cursor continuation |

Injected provider failures and competing commits are explicit arrangements at public provider
boundaries. Optional executor ports are identified in its register; a failure-only port fixture
does not establish successful persistence. Pagination describes continuation through ordered
identities and complete traversal of a fixed dataset. It makes no snapshot-isolation guarantee
when the dataset changes between requests.

## Regeneration and execution

The standalone [checker manifest](../../checks/ess-conformance/Cargo.toml) has its own lockfile
and pins all ESS libraries to one revision. The exact `rev` in that manifest is authoritative;
`PIN_FROM_CHECKER_MANIFEST` below means that same revision, not a release label. Install the
matching ESS CLI when inspecting these declarations:

```console
cargo install --locked --git https://github.com/beyond10x/ess --rev PIN_FROM_CHECKER_MANIFEST ess-cli
```

The initial integration baseline was ESS 0.37.0. The current pin ports its governed direct-return
extension onto ESS 0.38.0 with fresh source and suite versions, preserving the released formats.
Historical prototype suites retain their original identities under `evidence/prototype-0.37/`;
current execution uses inventory suite/29. No ESS dependency is added to a production crate. New executable tooling is Rust;
the checker's command line uses clap derive.

Run commands from the repository root. Regeneration compiles the full manifest, combines
generated and authored scenarios, requires nonempty complete declared coverage, and repeats
generation to compare exact bytes:

```console
cargo run --locked --manifest-path checks/ess-conformance/Cargo.toml -- regenerate
```

A changed inventory or assertion contract requires an explicit review reason:

```console
cargo run --locked --manifest-path checks/ess-conformance/Cargo.toml -- regenerate --coverage-review 'Describe the reviewed behavioral change and its coverage impact'
```

Commit the declarations, authored scenarios, canonical model and suite, and reviewed
[`ess/coverage.json`](../../ess/coverage.json) together. Coverage version 2 binds each scenario ID
to a digest of its **entire compiled scenario**, including inputs and expected observations.
Deleting a scenario or weakening an assertion under the same ID cannot silently preserve the
baseline. The reason records the review; the checker cannot establish that a human performed it.

The complete conformance check also requires committed generated bytes to match regeneration,
checks the coverage baseline, then executes the admitted suite through ESS's Rust runner:

```console
cargo run --locked --manifest-path checks/ess-conformance/Cargo.toml -- check
```

For a retained suite or one exact scenario, use `run`. A selected run supports diagnosis and
mutation evidence; it does not replace `check`:

```console
cargo run --locked --manifest-path checks/ess-conformance/Cargo.toml -- run --suite ess/generated/suite.json --scenario entity.core/authored/deterministic-kernel-record-bytes --reports target/ess-core-scenario
```

`--root` selects another checkout explicitly. `--reports` selects the evidence directory; the
default is `target/ess-conformance`. Completion requires a nonempty run with no failed, error,
unsupported or skipped scenario. The runner prints the actual counts; this guide does not pin
them. The repository's `task check` runs `ess-check`; CI executes the same checker and retains
its report artifacts. The MSRV lane also builds the isolated checker on Rust 1.85.

## Evidence and implementation identity

Each execution retains the exact admitted `suite.json`, canonical `report.json`, detailed
`run.json`, `implementation.txt` and actual response transcript `observations.json`. Reports keep
their suite association and model digest. Retain the corresponding canonical
[`model.json`](../../ess/generated/model.json), coverage baseline and reviewed source revision
with the evidence, including the selected suites used for mutations.

Report timestamps come from an epoch-anchored monotonic clock at the checker edge. Scenario
timestamps remain explicit fixture inputs; deterministic regeneration concerns model and suite
bytes, while execution evidence records the time the run actually occurred.

The checker [build script](../../checks/ess-conformance/build.rs) embeds a digest of the selected
production sources, adapter/checker sources, relevant manifests and lockfiles. Before running,
the checker recomputes that digest and refuses a stale executable. The implementation identity
combines this compile-time source identity with the SHA-256 of the actual executable. A branch
name or `HEAD` alone would miss uncommitted mutation changes; the retained identity includes them.
The digest is an association mechanism, not a claim that every possible dependency or compiler
configuration has been independently attested.

For each domain, preserve the deliberate production mutation patch, the named scenario's failing
run and the restored passing run. Keep code/contract disagreements and ESS limitations explicit
in the appropriate traceability register and governed work. A passing declared suite does not
close a documented coverage gap. Final authority adoption requires the full repository gate,
complete conformance and mutation evidence, and validated AEP records.
