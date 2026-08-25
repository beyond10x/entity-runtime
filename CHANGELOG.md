# Changelog

Every change a user of the runtime sees, per release. Unreleased work sits at the top.

## [Unreleased]

### Added

* `entity-core`: the kernel. Entity types registered from data — schema, lifecycle, operations
  with argument schemas, preconditions, invariants, `set` assignments and events — and executed as
  `definition + instance + operation + arguments → Decision { instance, events }`. No IO, no clock,
  no identifiers; a refusal returns a typed `CoreError` and changes nothing.
* `entity-core`: the condition language — `all`, `any`, `not`, `exists`, `eq`, `ne`, `gt`, `gte`,
  `lt`, `lte`, `in`, `contains`, literal booleans — and the template references `$id`, `$entity`,
  `$version`, `$state`/`$to_state`, `$from_state`, `$args[.path]`, `$fields[.path]`,
  `$old_fields[.path]`, with `$$` as the escape.
* `entity-yaml`: `from_str(&str) -> EntityDefinition`.
* `entity`: the command — `validate`, `inspect`, `graph`, `create`, `execute`; exit `0` decided,
  `1` refused (JSON refusal on stdout), `2` bad invocation. A printed `Decision` is accepted back as
  the next `--instance`.
* `examples/order.yaml`: the worked example, validated by the gate.
* Releases: every version tag builds the `entity` command for Linux (x86_64, aarch64), macOS
  (x86_64, arm64) and Windows (x86_64) and publishes a GitHub Release with the archives, a
  `SHA256SUMS` file and this file's section for the version as its notes.
* `docs/guide/`: getting started, the definition language, the command, the library — published
  with the vision, the requirements register and the designs at
  <https://beyond10x.github.io/entity-runtime/>.
