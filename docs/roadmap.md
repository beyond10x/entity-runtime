# Roadmap — driving `engineering-protocols`

**Status: proposed sequencing, not requirements.** The requirements register
([`requirements.md`](requirements.md)) says what 0.2.1 guarantees; the design
([`design/engineering-protocols-adoption-v0.1.md`](design/engineering-protocols-adoption-v0.1.md))
says what the adoption would look like; the planning store (`protocol artifact list`) holds the work.
This page says **in what order, blocked on what, and why that order**. Nothing here is accepted by
`engineering-protocols`.

Evidence dates: this tree at `4b6f2a1`, `engineering-protocols` at `79b641c` (its `main` head on
2026-08-25 — the same commit the adoption design pins).

## 1. The blocking fact

`engineering-protocols` **has never been told this repository exists.**

```console
$ cd ../engineering-protocols && grep -rln "entity-runtime\|entity-core" \
    --include='*.md' --include='*.yaml' --include='*.rs' . | grep -v ^./target
$ echo $?
1
```

Zero hits across its documents, its artifact YAML and its crates. Phase 0 of the adoption design is
not *awaiting a verdict* — it has not been **put** to the other side. Every later phase is gated on a
decision nobody there has been asked to make, so the whole programme currently has exactly one live
edge, and it is a document that has not been sent.

## 2. Critical path

Four items. **All four were decided on 2026-08-25** (§ 7); what remains is order of work.

| # | item | state | reversible until |
|---|---|---|---|
| **D** | phase 1: the eight lifecycles as definitions | **shipped** — 8 definitions, 64 edges, 11 tests, in the gate | always, it is `examples/` |
| **C** | `story:three-valued-conditions` — the one semantics change | decided down to its three open questions (§ 4a) | it ships in a release |
| **A** | put the mapping to `engineering-protocols`, carrying D as evidence | decided; sent after D | it is a document |
| **B** | the dependency arrow | decided: [`atlas/architecture/adr/0002`](https://github.com/beyond10x/atlas/blob/main/architecture/adr/0002-the-entity-runtime-dependency-arrow.md) | phase 2 adds the manifest line |

Phases 2, 3 and 4 sit behind all four.

## 3. What to send, and where it lands

Not a new plan page. `engineering-protocols` already has the story that asks this repository's
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
| gap register :77 | custom kinds cannot share a lifecycle ladder (`parent()` is over built-ins) | definition reuse |
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
at `4b6f2a1`:

| § 4 item | verdict |
|---|---|
| **three-valued rules** | correct, load-bearing, and **under-specified in three places** — below |
| **accumulating definition validation** | correct, cheap, independent of everything else; R-13 today reports one defect per attempt (`requirements.md:44`) while value validation already accumulates (`requirements.md:56`) |
| **typed references** | correct as a goal, **not a phase-2 blocker**. The design's own words: *"until it does, the shell keeps validating edges as `protocol artifact relate` does today"*. It belongs with relations (phase 3+), not on the critical path |

### 4a. Three-valued: three questions, decided 2026-08-25

`story:three-valued-conditions` is well formed (a `Truth` result, Kleene `all`/`any`/`not`, R-54
revised). Three decisions were missing; all three are now taken and written into the story. Each was
small before the type ships and expensive after:

1. **An `unobservable` refusal needs an address. → It carries every unresolved path, as data.** `CoreError::PreconditionFailed` carries
   `rule: Option<String>` and a `message` (`crates/entity-core/src/error.rs:318-326`) and has no
   field for *which reference did not resolve*. Telling an operator "go and observe" without naming
   what to observe reproduces, in a type, exactly the prose-rule failure `engineering-protocols`
   exists to end. The `Unobservable` counterpart should carry the unresolved path(s).
2. **`null` has no verdict. → A present `null` is `Unknown`, `exists` included.** `exists` is `resolve_operand(..).is_some()`
   (`crates/entity-core/src/runtime.rs:375`) and `lookup` returns `Some(Value::Null)` for a key that
   is present and null (`runtime.rs:634-645`). Argument schema validation catches that for a typed
   required argument (`runtime.rs:236`) but not for a `json`-kind field. YAML front matter spells
   *nobody filled this in* as `key:` — a present null, and a blank field must never satisfy the gate
   that exists to stop exactly that. `exists` therefore becomes three-valued, revising the story's
   earlier sentence; "three-valued *fields* are out of scope" stays true.
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
* **The pin is held by prose.** Every cross-repo claim in the adoption design cites
  `engineering-protocols@79b641c` by file and line, and **nothing checks it** — this repository pins
  requirements to tests mechanically (`scripts/check-requirements.py`) and pins this by hand.
  Phase 1's story already requires a committed fixture rather than a sibling checkout; **commit that
  fixture now** (the eight `artifacts/lifecycles/*.yaml` plus the sha) and the pin becomes a thing
  the gate can hold.
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
[`examples/aep/`](https://github.com/beyond10x/entity-runtime/tree/main/examples/aep) — 8
definitions, 64 edges, 11 tests, `example-check` and `cargo test` both in `task check`. The
equivalence was verified in both directions by breaking it: an invented edge fails naming the edge,
and a rung added to the pinned fixture fails naming what the definitions do not express.

The old edge is still in the store: `protocol artifact` has `relate` and no `unrelate`, so an edge
can be added and never removed. `story:aep-mapping-review informed_by
story:aep-lifecycles-as-definitions` records the real order beside it. Small, and worth carrying into
the phase-0 message — *nothing is deleted* is their principle, and this is where it bites an author.

## 6. The higher roadmap

Phases 0–4 are the design's. Two more follow from their gap register and are written down nowhere:

| phase | what | why it follows |
|---|---|---|
| **5** | the kernel's `Decision.events` become the markdown store's journal | D-P3 there: the store *"has no journal, no audit join and no history"*. The kernel already emits an event per operation; phase 2 puts it on the write path anyway |
| **6** | the four lifecycle concepts their protocol cannot express (gap register :73) | three of the four are lifecycle-shaped — a decision with a default and an expiry, time-based transitions, a typed blocker. The clock stays the shell's (R-05); the *shape* is a definition |

### Kernel work, ranked by whether the adopter forces it

| story | forced by | when |
|---|---|---|
| `three-valued-conditions` | their invariant 5 | now — critical path |
| `accumulating-definition-validation` | their invariant 3 | now — independent, cheap |
| `typed-references` | `artifacts/relations/relations.yaml` | phase 3 |
| `event-envelope` | their `DomainEvent` correlation/causation | phase 5 |
| `explain-verb` | their `protocol explain` UX parity | after phase 2 |
| `schema-fragments` | gap register :77, shared ladders | phase 4, authoring |
| `definition-json-schema` | their `cargo xtask schema` convention | any time |
| `projections`, `replay-from-events`, `definition-migrations`, `provider-spi`, `named-predicates`, `static-template-validation`, `pedantic-lints` | nothing there | after adoption is real |

The last row is the point of the ranking: **seven of fourteen kernel stories are not asked for by the
only named adopter.** Doing them first would grow a kernel nobody has yet agreed to use.

## 7. Decided, 2026-08-25

| question | decision | recorded in |
|---|---|---|
| what goes to `engineering-protocols`, and when | build phase 1, send it as the evidence | `story:aep-mapping-review`, `story:aep-lifecycles-as-definitions` |
| the dependency arrow | `engineering-protocols` takes `entity-core` as a Cargo dependency; this repository takes nothing back, ever | `atlas/architecture/adr/0002` |
| a present `null` | `Unknown`, `exists` included | `story:three-valued-conditions` |
| an `Unknown` refusal's address | names every unresolved path; `all`/`any` stop short-circuiting when the outcome is `Unknown` | `story:three-valued-conditions` |

Nothing in steps 1–3 of the ADR's order publishes a dependency, so the arrow is reversible until
phase 2 adds the manifest line.
