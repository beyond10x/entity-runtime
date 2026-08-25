# Changelog

Every change a user of the runtime sees, per release. Unreleased work sits at the top.

## [Unreleased]

### Fixed

* **`entity-graph`'s boundary test enforced nothing.** An independent review of 0.4.0 added a real
  `tokio` dependency *and* a real `std::fs::read_to_string` inside `escape()`, and all three of the
  crate's own tests passed. Two holes: the scanner read the `"` inside a char literal as opening a
  string, so everything after `if character == '"'` was invisible — which happened to be the entire
  escaping function the test existed to protect; and the manifest check split on the literal
  `[dependencies]` heading, so `[dependencies.tokio]` was not a dependency to it.

  Both holes were ones `entity-core`'s purity scan documents closing. The crate shipped a weaker
  hand-rewrite of a guard that already existed, which is the whole lesson: the scanner now lives
  once, in `scan-support`, used by both crates' tests, with the review's two plantings beside it as
  the proof it still works. Verified by planting both again and watching them fail.

  Writing it a third time was the obvious move and the wrong one.

* **R-95 was broken for SVG and HTML.** A state name carrying a control character produced a
  document no XML parser and no browser accepts, from a definition `entity validate` had passed.
  XML 1.0 permits no escape for most characters below `U+0020` — `&#1;` is as invalid as the raw
  byte — so they are **replaced** with `U+FFFD`, which is visible in the drawing and valid in the
  document. Dropping them silently would make two different names draw the same box. R-95's only
  pin was for DOT; it now has one for each format.

* **A reference graph could silently drop an edge.** `Graph::references` keyed its edges by display
  label, so a nested ref `a` → `b` and a field literally named `a.b` collapsed into one and the
  second overwrote the first — hiding a dangling reference that `Registry::validate_all` refuses,
  which is the one thing that picture must never do. Array items now append `[]`, as
  `entity-core`'s own `relation_targets` does, and edges are collected in a list.

* **Layout and renderer disagreed about duplicate node ids** — the layout took the last, the
  renderer the first, so an edge could leave one box and be drawn into another. Not reachable
  through either constructor, but `Graph`'s fields are public. Both take the last now.

* Two files declaring the same entity drew the same reference edge twice, with two overlaid labels.

### Added

* **`before` and `after`, for ordering two instants.** ISO-8601 — `2026-08-25`, or
  `2026-08-25T12:00:00[.fff][Z]`, with a space accepted for the `T`. The clock is still read at the
  edge and handed in as an argument; there is no `$now` and there will not be, because a definition
  that could ask what time it is stops being replayable.

  **An instant this kernel cannot read is `unknown`, not `false`** — and the refusal names the
  operand. This is the one place the two comparison families deliberately differ: `gt` on two
  non-numbers is `false` because *these are not numbers* is an observation anybody can make, while
  *this is not a timestamp I can read* is a statement about the reader. Answering `false` would let
  `after: [$args.now, $fields.due]` quietly report "not yet due" for a value nobody understood.

  An explicit offset — `+02:00` — is refused rather than normalised. Comparing it with a naive
  instant has no correct answer, and a shell that has offsets has a clock to normalise with. No date
  library: every one of them ships a `now()`, which is the thing R-01 exists to keep out.

  R-59 is new; R-53 and R-55 revised.

## [0.4.0] - 2026-08-25

### Added

* **`examples/aep/` gains the evidence preconditions phase 3 asks for.** `story`'s `implement` and
  `architecture-decision-record`'s `accept` now cost at least one `test_result`, evaluated
  three-valued — so *nobody presented one* refuses as unobservable naming
  `$args.evidence.test_result`, and *a count was presented and it is short* refuses as failed. The
  first sends somebody to produce a record; the second to argue about the one that exists. That
  distinction is the whole of `engineering-protocols` gap-register `:39`, and it is why three-valued
  rules were built before this.

  Only the guarded operations declare an `evidence` argument, so passing one to `propose` is refused
  as an argument the operation does not take — the schema doing its job rather than a special case.
  No edge changed.

  The equivalence test gained a second half to match: a rung the pinned ladder charges for must be
  charged for here too, paired by `(target status, evidence kind)`. Not by count or wording —
  upstream declares `at_least` on a status and these definitions declare a `gte` on a verb-named
  operation, so pinning the sentence would pin a translation rather than the claim. Verified by
  deleting the precondition and watching it fail.

  The pin moves to `engineering-protocols` `a193caa`, where `artifacts/lifecycles/story.yaml` now
  carries that requirement for real. `scripts/check-upstream-pin.py` found the drift on its own,
  two days after being written for exactly this.

### Changed — behaviour you may be relying on

* **`entity graph` takes several definition files and two more formats.** The positional argument is
  now a list, `--references` switches the subject, and `--format` accepts `svg` and `html` beside
  `text` and `dot`. `text` is byte-identical to before. Two DOT details changed: the graph is named
  `"<entity> v<version>"` rather than `"<entity>"`, so two versions of one entity no longer produce
  two files claiming to be the same graph; and each node emits its `label` explicitly, because a
  node's id and its label are separate things in the reference graph and both have to survive a
  quote. Passing several files without `--references` is a usage error rather than a guess.

### Added

* **`entity-graph`, a fourth crate, and the picture nobody could draw before.** `entity graph
  --references` draws entity types as boxes and `ref` fields as the edges between them — the reason
  typed references were built first. `Graph::lifecycle` draws what `graph` always drew; both go
  through one layout and four emitters.

  **No layout engine.** Calling graphviz would make a drawing depend on which `dot` is installed, so
  a picture could change without the definition changing — and a picture nobody can reproduce is not
  reviewable in a pull request. The layering is integer arithmetic: longest-path from the entry,
  with back edges classified first by depth-first search so a ladder that loops still lays out. Every
  coordinate is a `usize`; a test scans the crate's own sources for floats, IO, clocks and hash maps,
  and another reads the manifest to hold it to its single dependency.

  A target type nothing declares is still drawn, marked as undeclared: leaving it out would hide
  exactly what `Registry::validate_all` refuses.

* **Typed references between entities.** A field may be `type: ref` with an `entity` naming the type
  it points at, so a definition can say that an order's `customer` is a customer and a story's
  `epic` is an epic. `inverse` labels how the other side reads the edge; `acyclic` declares that it
  may not form one. `examples/references/` is a mutually-referencing pair, and `entity inspect`
  shows the target, the label and the flag whether they are written on the field or on an array's
  `items`.

  **Cardinality is the array machinery that already exists** — one reference is `type: ref`, several
  is `type: array` with `items` of kind `ref`. An earlier draft had a `relations:` block beside
  `schema` with its own `cardinality` key; it was two ways to say one thing, which is the defect
  this model refuses everywhere else, and it was dropped. `docs/design/kernel-v0.1.md` § 3.5 records
  that.

  **The kernel checks the declaration and the shape of an identity, and stops.** Whether an instance
  carrying that identity exists, what state it is in, what revision — those are questions about
  *another instance*, and `execute` is handed exactly one (R-01). Resolving one by lookup would mean
  the same inputs could produce different decisions at different moments, which is the property that
  makes a decision replayable (R-02). Resolution stays the shell's.

  `Registry::validate_all` asks the one cross-definition question the kernel can answer: does every
  `ref`, at any depth in a schema or an operation's arguments, point at a type the registry holds?
  It reports every missing target rather than the first. It is **not** part of `register`, because
  two types that reference each other are ordinary and a registration-time check would make them
  impossible to register in either order. `entity` calls it once the registry is assembled.

  R-20 gains `ref`; R-26 covers the three new constraints; R-27 and R-28 are new.

* **`examples/aep/vision.yaml`, and a check that notices when upstream moves.** `engineering-protocols`
  0.14.0 added a ninth lifecycle — a vision is `design`'s ladder with `implemented` removed, because
  a vision is replaced rather than finished. The pinned fixture and the definitions follow it, and
  the equivalence test now covers nine ladders and 73 edges.

  The reason it needed noticing at all is the interesting part: nothing was red. `pin-check` holds
  the committed fixture against its own `PIN.md` and says nothing about whether that fixture is
  still what upstream ships, so this repository was green while its equivalence test asserted
  agreement about eight ladders and nine existed. `scripts/check-upstream-pin.py` answers the other
  question — a ladder whose rungs moved, one upstream ships and nothing pins, one pinned and gone —
  and `.github/workflows/upstream-pin.yml` runs it weekly against a fresh clone.

  It is **not** a gate step, deliberately. `task check` reaches no network, and a check that had to
  clone somebody else's repository would make every local run depend on being online. Drift gets its
  own red run rather than arriving as a puzzling failure somewhere else.

## [0.3.0] - 2026-08-25

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
  an `unresolved` array, and its `definition` refusal gains a `defects` array beside the existing
  `defect`. Exit codes are unchanged: a refusal is still `1`.
* `CoreError::Definition` now carries `DefinitionErrors` rather than one `DefinitionError`, and
  `Registry::register`/`replace`/`EntityDefinition::validate` return it. A caller that wants one
  defect reads `.first()`.

Nothing about a lifecycle ladder changes. Every rule that never compares against a missing value
evaluates exactly as it did — including both invariants in `examples/order.yaml`.

### Added

* **Registration reports every defect, not the first.** `Registry::register`, `Registry::replace`
  and `EntityDefinition::validate` return `DefinitionErrors` — a non-empty list of typed
  `DefinitionError`s — and `entity validate` prints them all, so fixing a definition takes one pass
  rather than one run per fault. Value validation has reported every failing field since 0.1.0
  (R-23); this is the same for the definition itself. A check whose prerequisite already failed is
  skipped, so a lifecycle with a duplicate rung is one finding rather than one per transition it
  invalidates. Comparing a `DefinitionErrors` to a single `DefinitionError` holds only when it
  carries exactly that one, which is what keeps a single-defect assertion honest.
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
