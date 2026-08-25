# Changelog

Every change a user of the runtime sees, per release. Unreleased work sits at the top.

## [Unreleased]

Nothing yet.

## [0.2.1] - 2026-08-25

### Fixed

* **The 0.2.0 archives report `entity 0.1.0`.** The changelog was cut, the tag was written and the
  workspace version was never bumped, so five platforms' binaries went out claiming to be the
  release before them. 0.2.1 is 0.2.0 with its own version number — nothing else in the runtime
  changed — and with the check that would have caught it: a test comparing the binary's version to
  the newest released heading in this file, which the gate runs. Use 0.2.1; 0.2.0's archives are
  correct code under the wrong name.

## [0.2.0] - 2026-08-25

An adversarial review of 0.1.0 — a hands-on pass against the shipped binary and an independent
multi-angle code review — found defects in the kernel, claims the documents made that the code did
not keep, and gaps in the shell. All of it is addressed here; the record is
[`docs/reviews/2026-08-25-adversarial-review.md`](docs/reviews/2026-08-25-adversarial-review.md).

### Changed — behaviour you may be relying on

* A definition with a key the model does not declare is now **refused** rather than ignored:
  `requried: true` left a field optional, and a `precondition:` that should have been
  `preconditions:` left an operation unguarded. A condition must carry exactly one known operator —
  `{eq: …, ne: …}` used to parse as `eq` and drop the rest.
* A **precondition may no longer read `$state`**. It resolved to the state the operation was heading
  for, so `eq: [$state, draft]` on a `draft → submitted` transition refused every time it should
  have passed. Use `$from_state` and `$to_state`, which say which one they mean. An invariant may
  no longer read `$to_state`.
* `eq`, `ne`, `in` and `contains` now compare numbers **numerically**, so `100` equals `100.0` and
  they agree with `gt`/`gte`/`lt`/`lte`. A definition tested with integer fixtures used to refuse
  the same document written with a decimal point.
* A `set` value or event payload whose reference its scope could never resolve — `$args.*` in a
  creation event, an argument the operation does not declare, `$now` — is refused when the
  definition is **registered**, not on every execution.
* A reference path is checked in full: `$fields.address.countri` and `$fields.title.length` are
  refused at registration. They used to register and then read `false` for every instance.
* `Registry::register` refuses a definition whose `(entity, version)` is already registered;
  `Registry::replace` is how to mean it. Two `--definition` files of one type used to let the last
  one silently win.
* A constraint on a kind it does not govern (`values` on a `string`, `items` on an `object`,
  `min_length` on an `integer`) is refused instead of ignored.
* `EntityInstance.fields` is a `serde_json::Map` rather than a `BTreeMap`, which removes the
  conversions that copied every field to read one of them. Ordering is unchanged: by name.
* `entity validate` reports **every** file it is given, whatever went wrong with the one before it,
  and exits `1` — a file it cannot read or parse is one of its findings rather than a usage error.
  It no longer prints a JSON refusal after its report.
* The YAML reader is `serde_yaml_ng`; `serde_yaml` 0.9.34 is published as deprecated and receives
  no fixes.

### Added

* `unknown_state`: an instance claiming a lifecycle state the definition does not declare is
  refused by name, before the operation is looked at.
* An empty or whitespace identity is refused at `create`.
* `EntityDefinition::validate`, so a tool can check a definition without building a registry.
* `Registry::replace` and `Registry::versions`.
* A condition with an unknown operator now says which operator, and lists the twelve that exist,
  instead of reporting that the data matched no variant of an untagged enum.
* `entity` parses inline and piped **JSON as JSON** before trying YAML, so surrogate-pair escapes
  (what `json.dumps` and `jq -a` emit) are accepted; and refuses a second flag reading standard
  input rather than handing it an empty document.
* `entity graph --format dot` escapes names, so a state containing a quote produces valid DOT.
* Defaults declared inside an object are applied, at every depth an object already reaches.
* Integers outside the range of a 64-bit signed value are compared numerically rather than wrapped.

### Fixed — in the guarantees themselves

* The purity scan (R-01) was evadable by a grouped import, an alias, `std::io`, `include_str!` or a
  line beginning with `*`. It now strips comments and string literals, expands every `use` path and
  matches whole words, and is checked against fourteen plantings and eight lookalikes.
* The requirements checker accepted any `fn` as a pin, and could not parse a row whose id cell
  carried a marker — 21 rows were checked by nothing. Both are checks now.
* R-34 and AGENTS.md invariant 4 claimed the lifecycle state was closed *by the type*. It is closed
  by the kernel's own writes; the documents now say which, and `unknown_state` closes the gap that
  could be closed without sealing the type.
* The release workflow ran a shorter gate than CI — no `cargo doc` — so a tag could ship with
  broken intra-doc links. Both now call one reusable gate workflow, which also runs an MSRV job.
* Release and Pages workflows pin every action by commit; the release job no longer persists a
  token in `.git/config`.

## [0.1.0] - 2026-08-25

The first release: the kernel, the YAML adapter and the `entity` command, with every requirement
pinned. Rules are two-valued (a missing reference reads `false`); three-valued evaluation is
`story:three-valued-conditions`.

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
