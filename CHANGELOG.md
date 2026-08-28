# Changelog

Every change a user of the runtime sees, per release. Unreleased work sits at the top.

## [Unreleased]

Nothing yet.

## [0.11.0] — 2026-08-28

### Added

* **An event records what it was decided on.** `DomainEvent::args` is the operation's arguments,
  verbatim after defaults and validation — a creation event carries its fields. Written by the
  kernel, never defaulted, refused when missing. A precondition that read
  `$args.evidence.test_result >= 1` now leaves an event that says what the count was, so *what made
  this done* is in the log and not only in the shell that asked. R-110.

* **Replay checks the arguments.** A fold evaluates the emitting operation's preconditions against
  the event's arguments and the fields as they stood, and refuses a history whose arguments would
  have been refused — a forged `test_result: 0` does not reach `implemented` by the back door.
  R-97, extended.

### Changed

* **`DomainEvent` has a new required field.** An event written by 0.10.0 or earlier, or by hand,
  no longer parses; there is deliberately no default, because a key nobody wrote must not read as
  *decided on nothing*. A store holding pre-0.11.0 events is a store to migrate, not to read past.

* **Derived event identities carry a digest of the arguments**: `<entity>:<id>@<revision>#<index>~<args>`.
  Two events differing only in what they were decided on have different identities; sealing one
  decision twice still gives the same ones. Anything that pinned the old `#<index>`-terminated form
  changes.

## [0.10.0] — 2026-08-28

### Added

* **A store can say what it holds.** `StateProvider::ids(entity)` lists every identity stored under
  an entity type, sorted, so a shell can open a store it did not write and rebuild from it — the one
  question the SPI could not answer, and the reason `engineering-protocols`' SQLite backend refused
  any row it had not written itself. Every provider implements it: `MemoryStore`, `FileStore`,
  `SqliteStore`, `RemoteStore` and `Hybrid` (through its read path — an unreachable authority is
  `Unreachable`, never an empty list). The conformance suite gained three cases, and `Broken` now
  also lists an id it does not hold, so the suite is shown to catch that too. R-109.

* **`entity list --store <root> --entity <type>`** prints what a store holds, one id per line, or as
  JSON or YAML. A type nobody stored under prints nothing and exits 0.

### Changed

* **The wire is `entity.store/3`.** `Ask::Ids` and `Answer::Ids` are new variants on tagged enums a
  `/2` peer cannot decode, so the version moved and a `/2` peer is refused by name — the same rule
  0.9.0 applied when `Answer` grew. Both ends of a deployment upgrade together.

* **`StateProvider` has a new required method.** A provider outside this repository must implement
  `ids`; there is deliberately no default, because a default returning an empty list would let a
  provider claim to hold nothing while holding everything.

## [0.9.1] — 2026-08-26

### Changed

* **The README described a three-crate workspace that has eight.** `entity-store`, `entity-sqlite`,
  `entity-remote` and `entity-graph` — the whole storage half, shipped across 0.6.0 and 0.7.0 — were
  absent from *Where everything is*, so a reader arriving at the front page saw a kernel and a CLI
  and no way to keep anything.

* **Four guarantees added to *What holds*,** each with what pins it: state and events written
  together, `Unreachable` never reading as absent, a hybrid's policy being four words somebody typed
  with no `Default`, and replay reaching no state `execute` would refuse.

* `engineering-protocols` is described as the first adopter rather than the *intended* one. It takes
  `entity-core` as a dependency, expresses its eight lifecycles as definitions this kernel executes,
  and its `aep-backend-sqlite` is an adapter over `entity-sqlite`. The arrow still points one way.

## [0.9.0] — 2026-08-26

**A review of 0.8.0 — the release written to fix the previous review — found three more defects.**
A fix release nobody reviewed is the same shape as the thing it was fixing, so it was reviewed.

### Fixed

* **With the remote as authority, a refused *local* write was swallowed.** 0.8.0 fixed exactly this
  for `Authority::Local` and left the mirror case: the authority takes the write, the local copy
  refuses it — a full disk is enough — and the caller got an error while `divergences()` stayed
  empty and `catch_up()` was a no-op. Every later write then computed its expectation from the stale
  local revision and was refused by the authority for ever, with no record of why. It is recorded as
  a divergence now, saying which way round it happened.

* **`catch_up` could never clear a divergence against a replica at revision 0.** `None` and
  `Some(0)` were collapsed, so a replica genuinely holding a revision-0 instance was given
  `Expect::Absent`, which cannot match. Unreachable through `entity-core`, which creates at revision
  1 — and reachable for any third-party `Store` used as the replica, which is exactly who this
  protocol is for.

* **Three refusal messages had lost their line continuations** and carried 26 to 38 consecutive
  spaces into what a person reads.

### Changed

* **The wire version is `entity.store/2`.** `Answer` is a tagged enum with `deny_unknown_fields`, so
  adding a variant is a breaking wire change: a peer built against `/1` cannot decode
  `{"answer":"refused"}`. 0.8.0 added `Refused` and `Unreachable` and left the version at `/1`,
  which made the refusal undecodable by exactly the peer it exists to inform — it would have arrived
  as a decode failure, which is the `Backend` outcome that change set out to avoid.

  A `/1` peer is now refused by name, which is what a version is for.

## [0.8.0] — 2026-08-26

**A review of 0.6.0 and 0.7.0 found that six of their published claims were false.** Two independent
reviewers, run against the released commits; four defects were found by both. This release fixes
them and corrects the record. Nothing in 0.6.0 or 0.7.0 has been rewritten — a published section
stays as it was published, and the corrections are here.

### Corrections to 0.6.0 and 0.7.0

* **0.6.0's `### Fixed` section described two defects that never shipped.** *"Four requirements were
  registered and unchecked"* — the `R-90b` spelling it names exists nowhere in this repository's
  history except in that changelog entry; at 0.5.3 every row already matched the checker's pattern,
  and the four rows in question were **added** by 0.6.0, correctly numbered. *"A missing envelope
  field asserted something instead of refusing"* — `envelope.rs` did not exist before 0.6.0, so
  nothing released could have had the defect. Both describe things caught while the wave was being
  built, which the tag message says correctly and the changelog did not. The third entry, about an
  event that could not rebuild what it described, is genuine.

* **0.6.0 changed the CLI and said nothing.** `create` gained `--store`; `execute` gained `--store`,
  `--id`, `--entity`, `--correlation`, `--recorded-at`, `--causation` and `--actor`; `--instance`
  went from required to optional; and a store refusal now exits 1 with
  `{"refused": true, "by": "store", …}`. R-91 to R-93 are updated to match.

* **`catch_up` did not merge nothing — it merged by machine.** The claim appears five times across
  0.7.0's changelog, its tag, R-108 and the module's own documentation. See below for what it does
  now.

### Fixed

* **A forged creation event could enter any state, carrying any fields.** The first event of a
  history was exempt from every lifecycle check, so a fold reached a state `create` never produces —
  and installed fields of the wrong type, or fields the schema does not declare, without looking. A
  creation event is now held to `lifecycle.initial`, and the folded instance is validated against
  the schema. This was R-97's headline claim: *replay can reach no state `execute` would have
  refused.* R-97 gains five pins, including the two branches it claimed and nothing asserted.

* **`OnDivergence::Refuse` moved the local store and recorded nothing.** It wrote locally first and
  asked the replica second, so a replica that refused left an accepted write standing — unreplicated,
  unrecorded, and with the caller told the write had failed. Under `Refuse` the replica is now asked
  **first**: it is the side that can refuse for a reason the authority does not know about. The one
  case that remains — the replica accepts and the authority then refuses — is **recorded as a
  divergence** and documented, rather than described as impossible; undoing it needs a two-phase
  commit this crate does not have.

* **`catch_up` overwrote a replica that had moved on its own.** The expectation was derived from the
  replica's *current* revision, which made a conflict structurally unreachable: whatever the replica
  held, the local copy won and the function reported success. It now refuses to replay onto a replica
  at or ahead of this store's revision, and says so. What it cannot yet catch is stated in its own
  documentation rather than left for a reader to discover.

* **`catch_up` dropped divergences it could not examine, and duplicated events.** A local read that
  failed was treated as *the write is gone* — discarding the only record it happened. And the whole
  local log was replayed regardless of what the replica already held, producing a log with an event
  twice, which no longer folds. It now keeps what it could not read, and sends only what the replica
  has not seen.

* **A stale read that found nothing was reported as absent.** `Read` carries `was_stale`; the
  `StateProvider` trait has nowhere to put it, so every generic caller — including a hybrid nested
  inside another — saw `Ok(None)` where nothing had been learned. It is now
  `Unreachable`. A stale read that found a *value* still returns it: that is what the policy asked
  for.

* **A wire-version refusal was reported as unreachable.** A live, answering peer that refuses a
  version this build does not speak is not a peer you cannot reach — and a `ServeStale` policy would
  serve stale data for ever against a remote that is up and saying no. `Answer::Refused` is new.

* **`Unreachable` did not survive the wire.** A far side that could not reach *its own* store
  arrived here as an ordinary backend failure, so every `WhenUnreachable` policy downstream stopped
  applying. `Answer::Unreachable` is new, and carries the far side's provider name.

* **`entity-sqlite` failed roughly half of all concurrent writes, including writes to unrelated
  instances.** The transaction was `DEFERRED`, so the read took a shared lock that could not upgrade
  when two writers had both got that far; and with no busy timeout the loser was refused
  immediately. Worse, ~70% of genuine conflicts arrived as `Backend`, which this crate's own
  documentation tells callers means *stop retrying*. Now `IMMEDIATE`, with a five-second busy
  timeout: a second writer waits, and a real clash arrives as `RevisionConflict`.

* **A retried commit appended its events twice.** `FileStore` writes events before the state, so a
  failed state write left the expectation unchanged — and the retry any caller is entitled to make
  produced a log that no longer folds. ENOSPC was enough. The append is now idempotent.

* **`FileStore` could install a half-written file, and did not sync.** Every writer of one instance
  shared a temporary path, so one writer's rename could install an inode another was still filling.
  The name now carries the process id and a counter. Both the event append and the state write are
  now `fsync`ed — without which the module's stated recovery story inverts: the state lands and the
  event explaining it is lost.

### Changed

* **`entity-sqlite`'s rollback test now tears a write.** It asserted a refusal at the *pre-check*,
  which happens before either write — so there were no halves to roll back, and the assertion passed
  verbatim against `FileStore`, the provider whose documentation says it cannot make this promise.
  It now makes the event write fail after the instance write has landed. A second test asserts that
  `FileStore` **does** fail that case, so the first cannot quietly stop being evidence.

* `LoopbackTransport::store_mut` — so a test can move the far side independently, which is the only
  way to write a reconciliation test whose replica can actually conflict.

## [0.7.0] — 2026-08-26

Storage that is somewhere else, and storage that is in two places at once. The network lives in the
shell and nowhere else: `entity-core` is untouched, and this repository still opens no socket.

### Added

* **`entity-remote`: a store whose record of truth is a server.** The protocol is versioned and
  transport-agnostic — a request at a wire version this build does not know is refused **by name**,
  and a conflict crosses the wire as a conflict rather than flattening into a generic failure. A
  remote store passes the same conformance suite a local one does. R-105.

* **A store that could not be reached answers `Unreachable`, never absent.** *Absent* is a fact
  about the data; silence is a fact about the network. A provider that answers the first when it
  means the second is how a synchronisation deletes something. A silent remote refuses. R-104.

* **A hybrid store, whose behaviour is entirely the policy you typed.**
  `Policy::new(authority, read_path, when_unreachable, on_divergence)` — four words, all required,
  and **no `Default`**. A default here is a policy nobody chose being applied to somebody's data, so
  its absence is a requirement rather than a convention. With the remote as authority a refused
  remote write never reaches the local copy; refusing on divergence lets no write stand
  unreplicated. R-106.

* **A stale answer says it was stale, and a losing write is kept.** Serving a stale copy is
  something the policy asked for, and the answer carries `was_stale` at the point of use rather than
  leaving the caller to work it out. With the local store as authority, a replica write that loses
  becomes a recorded `Divergence` instead of being swallowed. R-107.

* **`catch_up` replays what the authority holds now.** Not what it held when the divergence was
  recorded. It keeps what it could not replay rather than reporting success, and it **merges
  nothing** — a divergence that comes back as a conflict stays outstanding for a person, because
  choosing between two conflicting values is a question about a domain this crate does not have.
  R-108.

### Notes

* **There is no HTTP client in this repository, deliberately.** `Transport` is the caller's to
  implement, which is what keeps the gate network-free; `LoopbackTransport` says in its own
  documentation that it stands in for exactly that and is not one.

## [0.6.0] — 2026-08-26

The shell. `entity-core` decided and this repository held nothing; R-80 described a shell that
loads, calls the kernel, then persists, appends and projects together, and no such shell existed.
Now three crates do, and the kernel is unchanged: the purity scan still finds no clock, no
filesystem and no network inside it.

### Added

* **A store writes an instance and its events together.** `Store::commit` takes a whole `Decision`,
  so a state cannot move without the event that explains it — there is no API that persists one
  half. `StateProvider` and `EventProvider` are the read halves, and they live in `entity-store`,
  outside the core, as R-82 said they must. R-83.

* **Every write says what it expected to find.** `Expect` is an argument, not a convention: a store
  holding a different revision refuses instead of overwriting, and the expectation is checked
  before anything is written, so a refused commit leaves no trace at all. Two executions from one
  revision leave exactly one accepted. R-84.

* **One conformance suite, run against every provider.** It lives in the crate that owns the traits
  and travels to each implementation, so `memory`, `file` and `sqlite` answer the same cases the
  same way — including that an instance nobody stored is **absent**, not an error. The suite is
  also run against a provider that is deliberately wrong, and has to both catch it and localise it,
  because a suite that only ever passes is a description of the implementation it was written
  against. R-85, R-101, R-102.

* **`entity-sqlite`: one `BEGIN`, both writes, one `COMMIT`.** A trait that claims a state and its
  events arrive together needs at least one implementor that a torn write can actually be tested
  against. A refused commit rolls back both halves, and the store survives being closed and
  reopened. R-103.

* **An event envelope, supplied by the shell.** `event_id`, `recorded_at`, `correlation`,
  `causation` and `actor`, outside `entity-core` because R-01 forbids the kernel to manufacture any
  of them. Correlation and causation are separate fields answering separate questions — *which flow
  was this* and *what caused this one event*. Identities are derived, so sealing one decision twice
  produces the same ids without a clock or a random source. R-86, R-88.

* **Replay: an instance rehydrated from its events.** A fold refuses any event whose transition the
  definition does not declare, whose `from_state` is not where the fold reached, whose revision does
  not follow, or which belongs to another instance — so replay can reach no state `execute` would
  have refused. R-97.

* **Projections are data the shell evaluates.** A definition declares `projections:` and performs
  none of them, because a projection reads across instances and the kernel is handed one. A
  projection naming a field the schema does not declare, or a state the lifecycle does not declare,
  is refused at registration rather than producing a read model that is silently always empty
  forever. A read model is the same bytes every run. R-98, R-99, R-100.

### Fixed

* **An event could not rebuild what it described.** `DomainEvent` recorded the operation but not
  the fields it wrote, so a `set:` was lost and a fold produced an instance the original run never
  had. An event that cannot rebuild what it describes is a notification, not a record.
  `from_state`, `to_state` and `changed` are now on the event. R-89 is new, and it is the
  requirement that makes replay meaningful rather than decorative.

* **A missing envelope field asserted something instead of refusing.** `serde` defaults a missing
  `Option` to `None`, so an envelope with no `actor` key deserialised as *nobody human caused this*
  — a claim, made by an absence. Every envelope field is now required, and an absent actor
  serialises as an explicit null rather than disappearing. R-87.

* **Four requirements were registered and unchecked.** Rows numbered `R-90b`-style did not match
  the requirement checker's `R-\d+` pattern, so they were invisible to `req-check` while looking
  exactly as registered as every other row. Renumbered; the count the gate reports is now the count
  it actually verifies.

## [0.5.3] — 2026-08-26

### Fixed

- **A deliberate line break is no longer eaten by the reflow.** A line ending in two spaces is
  Markdown's own way of asking for a break. The reflow ended the paragraph there, correctly, but
  **dropped the two spaces** — so the break survived only because GFM turns a bare newline into a
  `<br>`, which is the exact quirk this reflow exists to remove. Under any renderer that does not,
  the author's break was simply gone.

  Found by porting the tool to `engineering-protocols` and writing the self-test fresh. The case
  existed here already and **asserted the wrong expectation**, so the suite was holding the defect
  in place rather than catching it. A test that encodes a bug is worse than no test: it makes the
  bug look decided.

- **Release notes no longer break mid-sentence.** GitHub renders release bodies as **GFM**, and GFM
  turns a single newline into a `<br>`. `CHANGELOG.md` is hard-wrapped at 100 columns, so every one
  of those wraps was arriving as a literal line break — text snapping after "added" and before "the",
  in spots no author chose.

  Measured against GitHub's own `/markdown` endpoint rather than eyeballed:

  | release | stray `<br>` before | after |
  |---|---|---|
  | 0.4.0 | 55 | 0 |
  | 0.3.0 | 47 | 0 |
  | 0.5.0 | 28 | 0 |

  `scripts/changelog-section.py` extracts the tag's section and joins the continuation lines of each
  paragraph. **The file stays wrapped** — that is the right shape for something reviewed in a diff,
  and writing the CHANGELOG in one-line paragraphs would make every edit a whole-paragraph diff to
  please a renderer. Only the notes are reflowed.

  Left exactly as written: fenced code, tables, headings, blockquotes, list-item boundaries, and a
  line ending in two spaces, which is Markdown's own way of asking for a break. `--self-test` holds
  those seven shapes and runs in the gate and in the workflow, because generated notes are notes
  nobody proofreads.

  All seven published releases were re-rendered with the fix.

## [0.5.2] - 2026-08-25

### Added

* **`examples/aep/blocker.yaml`, the eleventh ladder.** `engineering-protocols` 0.18.0 added
  `blocker` — what is stopping something, typed by what would clear it. Upstream the *type* is the
  kind: `credential-blocker` and `person-blocker` share one ladder through their hyphen lineage.

  This kernel has no kind hierarchy, so the equivalence here is over the ladder itself, which is what
  the pinned document declares. `story:schema-fragments` is where a hierarchy would go, and it is
  worth saying that the adopter got the effect without one — their lineage rule is a naming
  convention their own type system reads, not something the definition format has to grow.

## [0.5.1] - 2026-08-25

### Added

* **`examples/aep/obligation.yaml`, the tenth ladder — and the first written entirely in names no
  Rust enum holds.** `engineering-protocols` 0.17.0 added `obligation`: a commitment on a clock
  nobody controls, `open → met | slipped`, where `slipped` opens on a date and `met` is terminal
  while `slipped` is not, because an obligation that slipped can still be met.

  The kind, all three of its rungs, and its dated guard needed no code in either repository. That is
  what the open-vocabulary work and the `before`/`after` operators were for, and it is the first
  thing to spend both at once.

  The equivalence test gained the matching half: a rung the pinned ladder **dates** must be dated
  here too, paired by `(target status, frontmatter key)`. Without it a definition could quietly drop
  the `after` precondition and only the upstream document would still say the rung waits.

  `scripts/check-upstream-pin.py` found the drift by itself, again — second time in two waves.

## [0.5.0] - 2026-08-25

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
