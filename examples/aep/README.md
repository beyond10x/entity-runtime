# The AEP artifact model, as entity definitions

One definition per lifecycle document AEP ships. This is **phase 1** of
[`../../docs/design/aep-adoption-v0.1.md`](../../docs/design/aep-adoption-v0.1.md),
and its claim is deliberately narrow: *these say exactly what their ladders say*.

| definition | upstream ladder | states | operations | edges |
|---|---|---|---|---|
| `initiative.yaml`, `epic.yaml`, `story.yaml`, `task.yaml` | the four-beat ladder — refine, agree, work, done | 6 | 6 | 9 |
| `executable-system-specification.yaml` | validate, revise and conform; conformance requires ESS evidence | 5 | 5 | 10 |
| `design.yaml`, `specification.yaml` | review is the only way out of draft | 7 | 7 | 12 |
| `architecture-decision-record.yaml` | proposed, then accepted or refused; both endings kept | 4 | 3 | 3 |
| `review-result.yaml` | a fact once written | 2 | 1 | 1 |
| `vision.yaml` | `design`'s ladder with `implemented` removed — a vision is replaced, never finished | 6 | 6 | 9 |
| `obligation.yaml` | a commitment on a clock nobody controls; `slipped` opens on a date, `met` is terminal and `slipped` is not | 3 | 2 | 3 |
| `blocker.yaml` | what is stopping something; upstream the *type* is the kind, and the lineage carries it | 2 | 1 | 1 |

## What is here, and what is not

* **The lifecycle is the point.** A status vocabulary that today is a ten-variant Rust enum
  (`ArtifactStatus`) is expressed by the YAML definitions here, so `correction-owed` — the rung an adopter needed
  and could not have — costs a line and an operation rather than a release of a crate.

  Not *here*, though: the equivalence test binds these files to the pinned ladder in both
  directions, so adding that rung to `story.yaml` fails until AEP adds it too.
  That is the guard working, and these definitions cannot lead upstream by construction — which is
  the whole reason they are safe to send as evidence. Upstream opened the status *vocabulary* on
  2026-08-25, so adding the rung no longer needs a release of theirs — but no lifecycle document
  declares `correction-owed` yet, and until one does `aep artifact move --to correction-owed`
  refuses there too. The cost moved from a release to a line; nobody has written the line.
* **`status` is not a field**, and `additional_fields` is `false` everywhere. The kernel owns the
  lifecycle state; nothing can move an artifact by editing it. A test asserts this per kind.
* **Evidence gates are explicit.** Story implementation requires test evidence, and executable
  system specification conformance requires ESS evidence. The equivalence test checks those
  conditions alongside the lifecycle edges.
* **The body is `json`.** `artifacts/kinds/*.yaml` upstream declares `required_sections`; modelling
  those as fields is a later step.
* **Every move emits `ArtifactMoved`.** The kernel produces the fact; who records it, with what
  correlation and causation, is the shell's — and is what a journal would fold.

## How the equivalence is held

[`../../crates/entity-yaml/tests/aep_lifecycles.rs`](../../crates/entity-yaml/tests/aep_lifecycles.rs)
reads the upstream documents from the committed fixture identified in
[`PIN.md`](../../crates/entity-yaml/tests/fixtures/aep-lifecycles/PIN.md) and compares edge sets in
both directions. An edge these definitions invent fails; an edge the upstream ladder grows and these
do not express fails too — which is the whole reason the fixture is committed rather than read from
a checkout that happens to be beside this one.

```console
cargo test -p entity-yaml --test aep_lifecycles --locked
entity validate examples/aep/*.yaml
```

Atlas compares these committed fixtures with current AEP source through `atlas compatibility aep`.
The scheduled consumer check lives above both repositories. This repository's gate reads only its
committed fixture and keeps the same result without an AEP checkout.

## Status

**Proposed, and unread by the repository it maps.** Nothing in AEP knows these
files exist; `story:aep-mapping-review` is the phase that changes that, and these definitions are
what it sends. Until then this is one repository's reading of another's documents, kept honest by a
test.
