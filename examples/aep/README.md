# The AEP artifact model, as entity definitions

Nine definitions, one per lifecycle document `engineering-protocols` ships. This is **phase 1** of
[`../../docs/design/engineering-protocols-adoption-v0.1.md`](../../docs/design/engineering-protocols-adoption-v0.1.md),
and its claim is deliberately narrow: *these say exactly what their ladders say*.

| definition | upstream ladder | states | operations | edges |
|---|---|---|---|---|
| `initiative.yaml`, `epic.yaml`, `story.yaml`, `task.yaml` | the four-beat ladder — refine, agree, work, done | 6 | 6 | 9 |
| `design.yaml`, `specification.yaml` | review is the only way out of draft | 7 | 7 | 12 |
| `architecture-decision-record.yaml` | proposed, then accepted or refused; both endings kept | 4 | 3 | 3 |
| `review-result.yaml` | a fact once written | 2 | 1 | 1 |
| `vision.yaml` | `design`'s ladder with `implemented` removed — a vision is replaced, never finished | 6 | 6 | 9 |

## What is here, and what is not

* **The lifecycle is the point.** A status vocabulary that today is a ten-variant Rust enum
  (`ArtifactStatus`) is nine YAML files here, so `correction-owed` — the rung an adopter needed
  and could not have — costs a line and an operation rather than a release of a crate.

  Not *here*, though: the equivalence test binds these files to the pinned ladder in both
  directions, so adding that rung to `story.yaml` fails until `engineering-protocols` adds it too.
  That is the guard working, and these definitions cannot lead upstream by construction — which is
  the whole reason they are safe to send as evidence. Upstream opened the status *vocabulary* on
  2026-08-25, so adding the rung no longer needs a release of theirs — but no lifecycle document
  declares `correction-owed` yet, and until one does `protocol artifact move --to correction-owed`
  refuses there too. The cost moved from a release to a line; nobody has written the line.
* **`status` is not a field**, and `additional_fields` is `false` everywhere. The kernel owns the
  lifecycle state; nothing can move an artifact by editing it. A test asserts this per kind.
* **No rules yet.** *`implemented` requires evidence* is phase 3, and it waits on
  `story:three-valued-conditions`: a gate that cannot tell *nobody looked* from *it is wrong* would
  refuse both with the same sentence.
* **The body is `json`.** `artifacts/kinds/*.yaml` upstream declares `required_sections`; modelling
  those as fields is a later step.
* **Every move emits `ArtifactMoved`.** The kernel produces the fact; who records it, with what
  correlation and causation, is the shell's — and is what a journal would fold.

## How the equivalence is held

[`../../crates/entity-yaml/tests/aep_lifecycles.rs`](../../crates/entity-yaml/tests/aep_lifecycles.rs)
reads the upstream documents from a committed fixture pinned at `4e6279b` and compares edge sets in
both directions. An edge these definitions invent fails; an edge the upstream ladder grows and these
do not express fails too — which is the whole reason the fixture is committed rather than read from
a checkout that happens to be beside this one.

```console
$ cargo test -p entity-yaml --test aep_lifecycles
running 11 tests ... ok
$ entity validate examples/aep/*.yaml
9 file(s), 0 invalid
```

## Status

**Proposed, and unread by the repository it maps.** Nothing in `engineering-protocols` knows these
files exist; `story:aep-mapping-review` is the phase that changes that, and these definitions are
what it sends. Until then this is one repository's reading of another's documents, kept honest by a
test.
