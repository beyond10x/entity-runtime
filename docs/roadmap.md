# Roadmap — driving AEP

Serves **O2** of `atlas/ROADMAP.md`, the collection's objectives; this page orders the work inside this repository.

**Status: a record of a sequencing that is now done, not requirements.** The requirements register
([`requirements.md`](requirements.md)) says what 0.13.0 guarantees; the design
([`design/aep-adoption-v0.1.md`](design/aep-adoption-v0.1.md))
says what the adoption would look like; the planning store (`aep artifact list`) holds the work.
This page says **in what order, blocked on what, and why that order**.

*Until 2026-08-28 this paragraph ended "Nothing here is accepted by `aep`."* That
is no longer true and is the reason this page was rewritten: phases 0–4 have shipped, the mapping has
a verdict on their side, and five crates of this repository are in their manifest. § 1 is the current
reading; what it replaced is kept under it.

Evidence dates: **re-read 2026-08-28** against this tree at `ddee747` (tag `0.13.0`) and
`aep` at tag `0.31.0` (`1419f1c`); their `main` moved again the same day, so the tag is what is cited rather than a head. Every date and count
below was checked against those two trees; where one had gone stale it is corrected in place and the
superseded text is kept rather than deleted. The previous reading — this tree at `4b6f2a1`,
`aep` at `79b641c`, 2026-08-25 — is three days old and predates their 0.13.0, the
release that took the dependency; every tag from `0.13.0` to `0.31.0` has been cut since. That
`79b641c` is on none of their branches today is its own small lesson about citing a bare commit
across a repository boundary: a tag survives a history rewrite and a hash does not.

## 1. Where this stands — verified 2026-08-28

**Phases 0 to 4 of the adoption design have shipped, and `aep` depends on this
repository.** The programme this page was written to sequence is done; what is left is the storage
layer, which is § 6.

| fact | evidence |
|---|---|
| `aep` takes **five** crates of this repository | `aep/Cargo.toml:112-116` — `entity-core`, `entity-store`, `entity-sqlite`, `entity-postgres`, `entity-remote`, declared once in `[workspace.dependencies]` |
| all five at **one pin**, the release tag `0.13.0` | the same five lines; their gate's `dep-check` (`cargo xtask deps`) fails if the lockfile ever resolves two pins or two versions, after two kernels were compiled side by side there for two releases |
| the arrow is **one way**, and permanent | no `Cargo.toml` in this repository names a crate of theirs — `grep -rn 'aep-\|aep' --include=Cargo.toml .` returns nothing. [`atlas/architecture/adr/0002`](https://github.com/beyond10x/atlas/blob/main/architecture/adr/0002-the-entity-runtime-dependency-arrow.md): *"`entity-runtime` takes nothing from `aep`, at any version, forever"* |
| **phase 0** — the mapping has a verdict | **accepted in part**, 2026-08-28, in `aep`' own store: `story:entity-runtime-mapping` § *Verdict — 2026-08-28*. Accepted for states, initial states and edges; **explicitly not for the verbs** — the eleven operation names in [`examples/aep/`](https://github.com/beyond10x/entity-runtime/tree/main/examples/aep) stay ours and unendorsed |
| the verdict has a test on **their** side too | `aep/crates/aep-backend-markdown/tests/entity_runtime_equivalence.rs` — our `examples/aep/*.yaml`, pinned at our tag `0.13.0`, compared against their `artifacts/lifecycles/*.yaml`; six tests, eleven kinds, **77 edges** in both directions. Each repository now holds a pinned copy of the other's documents |
| **phase 1** — every ladder as a definition | shipped: 11 definitions, 77 edges, **14 tests** (`cargo test -p entity-yaml --test aep_lifecycles`), against their `artifacts/lifecycles/*.yaml` pinned at `3de6e07`. In the gate through `test`, `example-check` and `pin-check` |
| **phase 2** — `aep artifact move` decided by this kernel | shipped there in `aep` 0.13.0, 2026-08-25 (`f20c9d6`), with `crates/aep-backend-markdown/tests/kernel_equivalence.rs` holding the kernel's verdict identical to the lookup it replaced over all 800 ordered status pairs |
| **phase 3** — a rung may cost evidence | shipped on both sides: `requires:` per rung upstream, three-valued rules here (R-57, R-58), and `PreconditionUnobservable` naming every address nobody supplied. Their gap register `:39` closed its mechanism half 2026-08-25 and its provenance half 2026-08-26 |
| **phase 4** — the status vocabulary opened | shipped in `aep` 0.13.0: `ArtifactStatus` carries `Other(String)` and the *ladder* gates a status write instead of the enum. Their gap register `:70`'s vocabulary half, and the last instance of `:76` |

**One correction to how it happened.** The ADR's order of moves put the verdict (its step 3) before
the manifest line (step 4). It went the other way: the operator instructed phase 2 directly, the
dependency landed on 2026-08-25, and the verdict was written on 2026-08-28 — three days behind the
thing it was meant to gate. The ADR records the departure itself, in its § *Taken, 2026-08-25*. The
cost of that ordering was bounded and stayed bounded: removing the dependency is *"deleting one
module and one manifest line"*, and nothing was built on either side that a refusal would have
stranded.

### Superseded 2026-08-28 — the old § 1, kept verbatim

The section below is what this page said until 2026-08-28. It is kept because it records the
sequencing decision that followed from it — build phase 1 first and send it as the evidence (§ 5) —
and a page that quietly deletes the premise of its own decisions cannot be audited. **Its central
claim is false and has been since 2026-08-25.** The grep it prints returns their README, their
`AGENTS.md`, two `Cargo.toml`s, a concepts page and release posts; the zero-hit result was true when
it was run and stopped being true the same week.

<details>
<summary>The blocking fact — as written, and now wrong</summary>

#### 1. The blocking fact

`aep` **has never been told this repository exists.**

```console
$ cd ../aep && grep -rln "entity-runtime\|entity-core" \
    --include='*.md' --include='*.yaml' --include='*.rs' . | grep -v ^./target
$ echo $?
1
```

Zero hits across its documents, its artifact YAML and its crates. Phase 0 of the adoption design is
not *awaiting a verdict* — it has not been **put** to the other side. Every later phase is gated on a
decision nobody there has been asked to make, so the whole programme currently has exactly one live
edge, and it is a document that has not been sent.

</details>

## 2. Critical path

Four items. **All four were decided on 2026-08-25** (§ 7), and **all four are now done** —
re-read 2026-08-28. Kept as the record of what was on the critical path and how each left it.

| # | item | state | reversible until |
|---|---|---|---|
| **D** | phase 1: every lifecycle as a definition | **shipped** — 11 definitions, 77 edges, 14 tests, in the gate; refreshed to `aep` `3de6e07` (0.18.0). *(Read 9/73/11 until 2026-08-28; the counts were a release behind two more ladders.)* | always, it is `examples/` |
| **C** | `story:three-valued-conditions` — the one semantics change | **shipped** — `Truth`, two new refusals, R-57/R-58 | it ships in a release |
| **A** | put the mapping to `aep`, carrying D as evidence | **done, and answered** — `story:entity-runtime-mapping` § *Verdict — 2026-08-28*: accepted in part, and not for the verbs | it is a document |
| **B** | the dependency arrow | **taken, and widened** — five crates now, one tag, one way: `entity-core`, `entity-store`, `entity-sqlite`, `entity-postgres`, `entity-remote` at `0.13.0` (`aep/Cargo.toml:112-116`); [`atlas/architecture/adr/0002`](https://github.com/beyond10x/atlas/blob/main/architecture/adr/0002-the-entity-runtime-dependency-arrow.md) | — the manifest lines exist |

Phase 3 is **done** on both sides — a ladder may declare what a rung costs, and the refusal tells
*nobody looked* from *it does not hold*. Phase 2 is **done**: `aep artifact move` is decided by
this kernel, with an 800-pair verdict equivalence test in their repository. **Phase 4 is done too**,
and this sentence used to say it and phase 3 were next: `ArtifactStatus` carries `Other(String)` and
the ladder gates the write, shipped in their 0.13.0. Nothing of phases 0–4 is outstanding; § 6 is
what follows.

## 3. What to send, and where it lands

Not a new plan page. `aep` already has the story that asks this repository's
question: **`story:open-vocabulary-audit`** (`.engineering/planning/story/open-vocabulary-audit.md`),
whose acceptance is *"one table over every adopter-facing declaration — open or closed, and for each
closed one the guarantee the closure buys"*, opened by an adopter's meta-defect: *things the docs
invite an adopter to declare keep turning out to be fixed in the engine.*

The mapping is the other half of that table — for each closed vocabulary, **what it would cost to
open it**. Attach the verdict there (`informed_by: story:entity-runtime-mapping` or equivalent)
rather than opening a page that competes with it.

**Lead with their backlog, not with our mapping.** The case is strongest stated as *how many of your
own open rows are "a Rust enum cannot say this"*:

| their row | what it cannot say | closed by data |
|---|---|---|
| gap register :70 | `correction-owed`; `expired`/`failed`/`blocked` flatten onto other rungs | phase 4 |
| gap register :73 | a decision with a default and an expiry; time-based transitions; a typed blocker | § 6, phase 6 |
| ~~gap register :77~~ | ~~custom kinds cannot share a lifecycle ladder (`parent()` is over built-ins)~~ | **closed there, not here.** `parent()` resolves a custom kind through its hyphen lineage (`crates/aep-domain/src/artifact.rs:529`); the row is now at `:102` of their register and carries its *closed by code* line as of 2026-08-28 |
| gap register :39 | a story's `implemented` is a claim nothing checks | phase 3 |
| `story:open-vocabulary-audit` | the meta-defect itself | the mapping |

Their stories `decision-with-default`, `time-based-transitions`, `blocker-relation`,
`outbound-claims-and-status-vocabulary` are four separate Rust changes there and four YAML edits
here. That is the argument.

**Name the collision in the same message.** Their `story:journal-backed-store` (gap register D-P3)
reroutes the markdown store's writes through `CommandService`; phase 2 reroutes the same store's
*verdicts* through this kernel. Whichever ships first without the other in view builds that seam
twice.

## 4. § 4 of the design — reviewed

The design's § 4 names three things that must change here before phase 2. Reviewed against the code
at `4b6f2a1`; **all three have since shipped, re-read at `ddee747` on 2026-08-28** — three-valued
rules as R-57/R-58 and `PreconditionUnobservable`, accumulating validation as `DefinitionErrors`, and
typed references as `type: ref` with `entity`, `inverse` and `acyclic` (R-27/R-28). The table below
is the review as it read when the work was owed:

| § 4 item | verdict |
|---|---|
| **three-valued rules** | correct, load-bearing, and **under-specified in three places** — below |
| **accumulating definition validation** | **shipped** — `Registry::register` returns `DefinitionErrors`, every defect at once, and one broken ladder is one finding rather than one per transition it invalidates |
| **typed references** | correct as a goal, **not a phase-2 blocker**. The design's own words: *"until it does, the shell keeps validating edges as `aep artifact relate` does today"*. It belongs with relations (phase 3+), not on the critical path |

### 4a. Three-valued: three questions, decided 2026-08-25

`story:three-valued-conditions` is well formed (a `Truth` result, Kleene `all`/`any`/`not`, R-54
revised). Three decisions were missing; all three are now taken and written into the story. Each was
small before the type ships and expensive after:

1. **An `unobservable` refusal needs an address. → It carries every unresolved path, as data.** `CoreError::PreconditionFailed` carries
   `rule: Option<String>` and a `message` (`crates/entity-core/src/error.rs:318-326`) and has no
   field for *which reference did not resolve*. Telling an operator "go and observe" without naming
   what to observe reproduces, in a type, exactly the prose-rule failure `aep`
   exists to end. The `Unobservable` counterpart should carry the unresolved path(s).
2. **`null` has no verdict. → A present `null` is not a value.** `lookup` returns
   `Some(Value::Null)` for a key that is present and null (`runtime.rs:634-645`), and argument
   schema validation catches that for a typed required argument (`runtime.rs:236`) but not for a
   `json`-kind field. YAML front matter spells *nobody filled this in* as `key:` — a present null,
   and a blank field must never satisfy the gate that exists to stop exactly that.

   **Amended when it was built.** The decision as first taken read *"`Unknown`, `exists`
   included"*, which would have made `exists` three-valued and never `False`. That was wrong, and
   a two-valued `absent` operator bolted on beside it to compensate was wronger: the two were not
   each other's negation, and the kernel would have been claiming it could not tell whether a
   field was set — which is false, since it holds the instance. `Unknown` belongs to the
   **question**, not the operator: `exists` asks about the store and stays two-valued (`false` for
   a present null); every comparison asks about a value and is `Unknown` when there is none to
   read. Same guarantee, one operator, no asymmetry. See `kernel-v0.1.md` § 4.1, which records the
   rejected draft so nobody re-proposes it. "Three-valued *fields* are out of scope" stays true.
3. **Kleene changes what short-circuiting costs. → Collect every address; short-circuit goes.** R-54 pins `all`/`any` to short-circuit
   deterministically (`requirements.md:91`). Under Kleene the truth value is order-independent, but
   *which unresolved reference gets reported by (1)* is not. `all`/`any` evaluate every operand when
   the outcome is `Unknown`, so one refusal names all three missing facts instead of three refusals
   naming one each. R-54's short-circuit clause is revised to say so.

### 4b. What § 4 does not list and phase 2 needs

* **The ADR in `atlas` is a precondition, not a boundary.** § 5 files it under *boundaries that hold
  whatever happens*; in practice no line of phase 2 can be written until it exists, because phase 2
  *is* one repository calling the other. Its stated precondition has already lapsed:
  `story:aep-move-through-kernel` says *"this repository's visibility is undecided"* — it has been
  public since 2026-08-25 (`atlas/log/2026-08-25.md`). Both repositories are public; the question is
  now only arrow direction, and `atlas/architecture/adr/` holds one ADR to pattern it on.
* ~~**The pin is held by prose.**~~ **Done, and then done twice.** Every cross-repo claim in the
  adoption design cited `aep@79b641c` by file and line with nothing checking it.
  The fixture is now committed — `crates/entity-yaml/tests/fixtures/aep-lifecycles/` with a
  `PIN.md` the gate's `pin-check` recomputes on every run, and `.github/workflows/upstream-pin.yml`
  asking weekly whether the copy is still what upstream ships, outside the gate so nothing here
  reaches the network. As of 2026-08-28 **they hold the mirror image**:
  `aep/crates/aep-backend-markdown/tests/fixtures/entity-runtime-aep/`, our
  `examples/aep/*.yaml` pinned at our tag `0.13.0`, with its own sha per file. Neither repository can
  now move its half of the mapping without the other's gate saying so.
* **An `explain` verb is *not* needed for phase 2.** Worth stating because it looks like it is:
  `execute` is pure and a refusal changes nothing (R-04), so attempting the move *is* a safe dry run.
  `story:explain-verb` stays an ergonomics item.

## 5. One reordering — done

`story:aep-lifecycles-as-definitions` (phase 1) declared `depends_on: story:aep-mapping-review`
(phase 0). **The edge is inverted**, decided 2026-08-25.

A paper review of a mapping table is weak evidence. Eight definitions plus an equivalence test that
proves *the definitions yield exactly the transitions your YAML declares* is the artefact that makes
the review decidable — and it costs `examples/` in this tree, changes nothing in theirs, and is
thrown away for free if the verdict is no. Phase 1 ships **as** phase 0's evidence, and it has:
[`examples/aep/`](https://github.com/beyond10x/entity-runtime/tree/main/examples/aep) — **11
definitions, 77 edges, 14 tests** (read 8/64/11 until 2026-08-28, when three more ladders had
landed), `example-check` and `cargo test` both in `task check`. The equivalence was verified in both
directions by breaking it: an invented edge fails naming the edge, and a rung added to the pinned
fixture fails naming what the definitions do not express.

**It worked.** The verdict came back on 2026-08-28 accepting exactly what the test pins — states,
initial states and edges — and refusing exactly what it could not pin: the verbs. A paper review
would have had no way to draw that line.

The old edge is still in the store: `aep artifact` has `relate` and no `unrelate`, so an edge
can be added and never removed. `story:aep-mapping-review informed_by
story:aep-lifecycles-as-definitions` records the real order beside it. Small, and worth carrying into
the phase-0 message — *nothing is deleted* is their principle, and this is where it bites an author.

## 6. What follows — `epic:the-store-an-adopter-runs-on`

Phases 0–4 are the design's and are done (§ 1). **What comes next is not another phase of the
adoption; it is the storage layer**, and it is tracked as
[`epic:the-store-an-adopter-runs-on`](https://github.com/beyond10x/entity-runtime/blob/main/.engineering/planning/epic/the-store-an-adopter-runs-on.md)
in this repository's own store, with its plan page at
[`plan/next-waves-the-adopters-store.md`](plan/next-waves-the-adopters-store.md) (accepted
2026-08-28) and the adopter's side at `aep/docs/plan/store-waves-f-g-h.md`. Three
capabilities, each with no `aep` in it: say what a store holds, record what an operation was decided
on, and run on a server. **Read that epic, not this section, for what is being built.**

The two phases below were written here before that epic existed. Kept, with what happened to each:

| phase | what | why it follows | 2026-08-28 |
|---|---|---|---|
| **5** | the kernel's `Decision.events` become the markdown store's journal | D-P3 there: the store *"has no journal, no audit join and no history"*. The kernel already emits an event per operation; phase 2 puts it on the write path anyway | **overtaken, and mostly done there.** They built the journal themselves first (`ab48bc8`, their 0.19.0) and the `CommandService` envelopes behind it (their 0.27.0, wave D). Wave H is where history moves onto an event log this repository supplies — `story:events-carry-what-they-were-decided-on` under the epic above |
| **6** | the four lifecycle concepts their protocol cannot express (gap register :73) | three of the four are lifecycle-shaped — a decision with a default and an expiry, time-based transitions, a typed blocker. The clock stays the shell's (R-05); the *shape* is a definition | **two of the four have landed upstream as ladders**, not as kernel work: `obligation` (`ac30a24`) and `blocker` (`6409587`), both expressed in `examples/aep/` and compared by the equivalence test. The decision-with-default and the expiry still need a clock read at the edge |

### Kernel work, ranked by whether the adopter forces it

| story | forced by | when |
|---|---|---|
| `three-valued-conditions` | their invariant 5 | **shipped** — `Truth`, R-57/R-58; it was the only blocker they owned |
| `accumulating-definition-validation` | their invariant 3 | **shipped** |
| `typed-references` | `artifacts/relations/relations.yaml` | **shipped** — `type: ref` with `entity`, `inverse` and `acyclic`; R-27/R-28 |
| `event-envelope` | their `DomainEvent` correlation/causation | phase 5 |
| `explain-verb` | their `protocol explain` UX parity | still open; phase 2 shipped without it, as § 4b predicted |
| `schema-fragments` | ~~gap register :77~~, shared ladders | **not forced any more** — they closed the shared-ladder gap themselves through kind lineage (`artifact.rs:529`). Authoring convenience only |
| `definition-json-schema` | their `cargo xtask schema` convention | any time |
| `projections`, `replay-from-events`, `definition-migrations`, `provider-spi`, `named-predicates`, `static-template-validation`, `pedantic-lints` | nothing there | after adoption is real |

`entity-graph` is not in that table because no adopter asked for it either — it is here because the
vision claims a definition can be *rendered by tooling that never parses code*, and until it existed
the only rendering was a list of arrows. It is also the first thing that makes typed references
visible, which is the argument for having built them.

The last row is the point of the ranking: **seven of fourteen kernel stories are not asked for by the
only named adopter.** Doing them first would grow a kernel nobody has yet agreed to use.

## 7. Decided, 2026-08-25

| question | decision | recorded in |
|---|---|---|
| what goes to `aep`, and when | build phase 1, send it as the evidence | `story:aep-mapping-review`, `story:aep-lifecycles-as-definitions` |
| the dependency arrow | `aep` takes `entity-core` as a Cargo dependency; this repository takes nothing back, ever | `atlas/architecture/adr/0002` |
| a present `null` | not a value — `exists` reports `false`, a comparison reports `unknown` (amended from *"`Unknown`, `exists` included"* when built; § 4a) | `story:three-valued-conditions` |
| an `Unknown` refusal's address | names every unresolved path; `all`/`any` stop short-circuiting when the outcome is `Unknown` | `story:three-valued-conditions` |

Nothing in steps 1–3 of the ADR's order publishes a dependency, so the arrow is reversible until
phase 2 adds the manifest line.
