# Changelog

Every change a user of the runtime sees, per release. Unreleased work sits at the top.

## [Unreleased]

### Changed — behaviour you may be relying on

* **A rule that compares against something nobody recorded is now `unknown`, not `false`.** A
  condition evaluates to `True`, `False` or `Unknown`, and a rule holds only when `True`. The
  refusal is a new `PreconditionUnobservable` / `InvariantUnobservable` carrying **every** address
  it could not read, sorted, rather than the first. *Nobody looked* and *it is wrong* used to be
  one message; sending an operator to fix a review that was never written is what that cost.
* **`exists` is unchanged.** `Unknown` is a property of the *question*, not of the operator asking
  it. Asking whether there is a value at an address is a question about the store, which the
  kernel can always answer — so `exists` stays two-valued and `not: { exists: … }` still means what
  it reads as. Only questions about a *value* — `eq`, `ne`, `gt`, `gte`, `lt`, `lte`, `in`,
  `contains` — can come back `unknown`, and only when there is no value to read. If a missing value
  should refuse plainly rather than stall the gate, guard the comparison in the same rule:
  `all: [{exists: $fields.x}, {eq: [$fields.x, v]}]`, which `False` dominance decides.
* **A key present with nothing after it is not a value.** `review:` with a blank after it is how
  YAML spells *nobody filled this in*, so `exists` reports `false` for it and a comparison against
  it reports `unknown`. Schema validation cannot catch this for a `json`-kind field, where `null`
  is legal. A `null` written as a literal in a definition is still a value.
* **`all` and `any` no longer short-circuit.** Kleene's connectives are order-independent, so the
  answer is unchanged; what changes is that one refusal now names all three missing facts instead
  of three refusals naming one each. R-54's deterministic short-circuit clause was revised with the
  rest of the row, and the wording it replaced is quoted in the register.
* `entity`'s JSON refusal gains `precondition_unobservable` and `invariant_unobservable`, each with
  an `unresolved` array. Exit codes are unchanged: a refusal is still `1`.

Nothing about a lifecycle ladder changes. Every rule that never compares against a missing value
evaluates exactly as it did — including both invariants in `examples/order.yaml`.

### Added

* **`Truth { True, False, Unknown }`, public**, with Kleene `and`/`or`/`not` and `is_satisfied`.
  The variant names and tables are taken from `engineering-protocols`' own
  `aep-domain::predicate::Truth` rather than designed here — two kernels that disagreed about what
  `Unknown` means would disagree about whether a gate passed.
* `docs/requirements.md` gains **R-57** (three-valued evaluation) and **R-58** (which questions can
  be `Unknown` and which cannot), and `docs/design/kernel-v0.1.md` § 4.1 specifies both, including
  the rejected first draft that put the choice in the operator instead. R-50, R-51, R-53 and R-54
  were revised; each replaced wording is quoted beneath its table.
* **The eight AEP lifecycles, as entity definitions.** `examples/aep/*.yaml` expresses every
  lifecycle document `engineering-protocols` ships — `story`, `epic`, `initiative`, `task`,
  `design`, `specification`, `architecture-decision-record`, `review-result` — as data this kernel
  executes, one operation per edge of each ladder. Phase 1 of
  [`docs/design/engineering-protocols-adoption-v0.1.md`](docs/design/engineering-protocols-adoption-v0.1.md);
  no rules yet, because a precondition worth writing needs a rule that can say `unknown`.
* **An equivalence test that makes the translation checkable, not asserted.**
  `crates/entity-yaml/tests/aep_lifecycles.rs` compares each definition's `(from, to)` edge set
  against the upstream `transitions` map, read from a committed fixture pinned at `79b641c`
  (`crates/entity-yaml/tests/fixtures/aep-lifecycles/PIN.md`) rather than from a sibling checkout.
  A definition that invents an edge and a ladder that grows one upstream both fail, by name. The
  gate runs it, and `example-check` now validates `examples/aep/` too.
* **[`docs/roadmap.md`](docs/roadmap.md)** — what order the adoption goes in, blocked on what, and
  the four decisions taken on 2026-08-25: phase 1 ships before phase 0 and is its evidence; the
  dependency arrow points from `engineering-protocols` to `entity-core` and never back; a present
  `null` will not count as a value; and an unobservable refusal will name every unresolved path.

Nothing in the kernel changed, and nothing here publishes a dependency in either direction.

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
