# Entity Runtime

> Let agents propose. Let deterministic rules decide.

Entity Runtime decides whether a proposed state change is legal. You declare an entity type as
data — its fields, lifecycle states, named operations, rules and events — and a deterministic Rust
kernel answers each request with the complete next decision or a typed refusal that changes
nothing. The kernel reads no clock, file or network; storage, identity, time and side effects stay
with the application around it. It ships as Rust libraries and one command, `entity`.

**Documentation: <https://beyond10x.github.io/entity-runtime/>** —
[getting started](https://beyond10x.github.io/entity-runtime/docs/getting-started) ·
[guides](https://beyond10x.github.io/entity-runtime/docs/guides/model-policy-as-data) ·
[CLI reference](https://beyond10x.github.io/entity-runtime/docs/reference/cli) ·
[crates](https://beyond10x.github.io/entity-runtime/docs/reference/crates) ·
[status](https://beyond10x.github.io/entity-runtime/docs/status)

**Status:** [0.28.0](https://github.com/beyond10x/entity-runtime/releases/tag/0.28.0), released
2026-10-07. The API is in development; a minor release may change it. The
[status page](https://beyond10x.github.io/entity-runtime/docs/status) lists what is shipped, with
the test that holds each item, and what is planned.

## Install

Every [release](https://github.com/beyond10x/entity-runtime/releases) carries `entity` for Linux
(x86_64, aarch64), macOS (x86_64, arm64) and Windows (x86_64) with a `SHA256SUMS` file. Or build it
with Rust 1.85 or newer:

```console
cargo install --git https://github.com/beyond10x/entity-runtime --tag 0.28.0 --locked entity-cli
```

```console
$ entity validate examples/refund.yaml
examples/refund.yaml: valid (refund v1)
1 file(s), 0 invalid
```

The libraries are not on a registry; depend on them by Git tag
([embed the kernel](https://beyond10x.github.io/entity-runtime/docs/guides/embed-the-kernel)).

## Build from source

Requires Rust 1.85 or newer and [go-task](https://taskfile.dev). `entity-eventlog` and the
provider contract need Rust 1.91.0 as well, which `task check` calls by name (`cargo +1.91.0`).
PostgreSQL tests run when `ENTITY_POSTGRES_URL` names a server and print that they were skipped
otherwise.

```console
task check                                   # the full gate
cargo run -p entity-cli --locked -- --help   # the entity command from this checkout
task site-build                              # the documentation site (needs network for npm)
```

The documentation site's source is [`website/`](website/); the requirements register, designs,
executable ESS contracts and reviews are the engineering record under [`docs/`](docs/) and
[`ess/`](ess/). Contributors and coding agents read [`AGENTS.md`](AGENTS.md) before changing
anything; the [changelog](CHANGELOG.md) records every change a user sees.

## License

Apache-2.0. See [LICENSE](LICENSE).

<!-- b10x-docs:start -->
## Documentation

[Entity Runtime documentation](https://beyond10x.github.io/docs/entity-runtime/) · [Start](https://beyond10x.github.io/) · [Ecosystem](https://beyond10x.github.io/ecosystem/) · [Impact](https://beyond10x.github.io/changes/) · [Releases](https://beyond10x.github.io/releases/)
<!-- b10x-docs:end -->
