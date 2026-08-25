# AGENTS.md — entity-runtime

The contract for changing **this** repository. Read it before changing anything.

Org-wide rules — repo naming, the former-brand rule and the rule that a change to bytes another
repo verifies is a coordinated migration with an ADR — live in `atlas/AGENTS.md` and are not
restated here.

`README.md` orients a reader. This file says what must not break.

## What this repository is

A **library and a command**: `entity-core`, an IO-free deterministic kernel that executes entity
types declared as data; `entity-yaml`, the text-to-definition adapter; `entity-cli`, the `entity`
command that is the reference shell around the kernel. It is not a database, a message bus, a
workflow engine or a scripting runtime, and it holds no credential and reaches no network.

## Which documents are normative

* [`docs/requirements.md`](docs/requirements.md) — the register. Every row names a test, a type or
  a manifest that pins it; `design` alone marks a gap and is a story.
* [`docs/design/kernel-v0.1.md`](docs/design/kernel-v0.1.md) — the kernel's semantics. Where code
  and this document disagree, the document wins until a later revision says otherwise.

[`docs/design/engineering-protocols-adoption-v0.1.md`](docs/design/engineering-protocols-adoption-v0.1.md)
is **proposed**: it is accepted only by a plan page or a story in `engineering-protocols`, and this
repository's side of it is worked through `epic:drive-engineering-protocols` in the planning store.
Do not implement phase 2 or later from it without that acceptance.

## Invariants

Each carries what actually enforces it, because a rule nothing checks has already drifted
somewhere. Do not write an enforcement here that you cannot point at.

1. **The kernel does no IO.** No clock, identifier generator, filesystem, network, environment,
   thread, async runtime or random source in `entity-core`.
   *Enforced by* `crates/entity-core/tests/purity.rs`: a banned-token scan over every source line
   of the crate (comments excluded), checked against a planted offence so a scan that has stopped
   seeing anything fails on it; and a second test pinning the dependency list to `serde` and
   `serde_json`.
2. **Same inputs, same `Decision`, same bytes.** Ordered maps only; no `HashMap`/`HashSet`.
   *Enforced by* the same scan (`HashMap` and `HashSet` are banned tokens) and
   `the_same_inputs_produce_the_same_decision_byte_for_byte`.
3. **A refusal changes nothing.** Every kernel entry point takes the instance by shared reference
   and returns a new one; there is no code path that mutates the caller's.
   *Enforced by* the signatures of `create` and `execute` and by
   `a_refusal_leaves_the_caller_owned_instance_untouched`.
4. **The lifecycle state has no setter.** `lifecycle_state` is assigned in `create` and `execute`
   and nowhere else; there is no generic status write and no delete.
   *Enforced by* the type — nothing in the public API sets the field — and by R-34's row in the
   register. Adding such an API is a design change, not a feature.
5. **Rules see only what their scope allows.** An invariant cannot read `$args`, `$old_fields` or
   `$from_state`; any rule referencing an undeclared field or argument is refused at registration.
   *Enforced by* `validate_rule_reference` in `crates/entity-core/src/validation.rs` and the tests
   `an_invariant_may_not_read_arguments_or_previous_state`,
   `a_rule_referencing_an_undeclared_field_is_refused`.
6. **Value validation accumulates.** An object with four broken values reports four errors, each
   with a path.
   *Enforced by* `validate_object` returning `Vec<ValidationError>` and
   `validation_accumulates_every_field_error`, which asserts an exact set of paths, not "is an
   error".
7. **No `$now`, no `uuid()`, no lookup in a template or a rule.** What the world knows enters as
   an argument.
   *Enforced by* the closed reference set in `resolve_expression_optional` and
   `an_unresolvable_template_reference_is_an_error_not_a_null`.
8. **The eleven-step evaluation order is the contract.** `InvalidTransition` before
   `PreconditionFailed`; invariants after `set`; events last.
   *Enforced by* `execute`'s straight-line body and
   `an_operation_not_declared_from_the_current_state_is_refused_before_its_preconditions`,
   `fields_are_revalidated_after_set`,
   `a_failed_invariant_after_an_operation_yields_no_decision_and_no_events`. There is no
   workspace-wide check that a refactor keeps the order; the design (§ 6) is what a reviewer reads.
9. **Every public item is documented and there is no `unsafe`.**
   *Enforced by* `missing_docs = "warn"` and `unsafe_code = "forbid"` in `[workspace.lints]`,
   raised to errors by the gate's `-D warnings`; the `doc-check` step fails on a broken intra-doc
   link. All three members opt in with `[lints] workspace = true`; a new crate that omits that line
   is outside every lint here.
10. **Every requirement is pinned, and the pin exists.**
    *Enforced by* `scripts/check-requirements.py` in the gate: every `R-nn` is referenced by a
    design under `docs/design/`, every cited test is a `fn` under `crates/`, every row names its
    evidence.

## Gate

```console
task check
```

Seven steps, in this order: `fmt-check` · `clippy` (`--workspace --all-targets -D warnings`, which
is what makes `missing_docs` fatal) · `test` · `doc-check` (`RUSTDOCFLAGS=-D warnings`) ·
`example-check` (`entity validate examples/*.yaml`) · `req-check` · `plan-check`
(`protocol artifact validate`). CI (`.github/workflows/check.yml`) runs the first six; it has no
`protocol` binary.

Land nothing that does not pass all seven. Read the gate's own exit status, not a pipeline's:
`task check 2>&1 | tail` reports `tail`'s.

**Prose states no count of the gate's suites or tests.** That number lives in exactly one place:
the gate's output.

## Boundaries

* **Vocabulary crosses to `engineering-protocols`; a dependency is a decision not yet taken.** No
  `Cargo.toml` here names a crate of theirs and none there names one of ours until an ADR in `atlas`
  says which way the arrow points. Both repositories are public.
* **Provider interfaces live outside `entity-core`.** A state store, an event store, a search
  index, a blob store — each is a crate that depends on the kernel, never the reverse.
* **The shell owns IO.** `entity-cli` reads files and stdin and prints; nothing else here does.
  If a new verb needs a clock, the clock is read in the CLI and passed in as an argument.
* **Nothing in `task check` reaches the network** and no step spends money.
* **Never commit a credential, a token or anything adopter-internal.**

## The website

`website/` is a Docusaurus site that renders **this repository's `docs/` tree** — there is one copy
of every document, and the site adds only the landing page. It is published at
<https://beyond10x.github.io/entity-runtime/> by `.github/workflows/pages.yml` on every push to
`main`; pull requests get the build without the deploy. `onBrokenLinks: 'throw'` means a dangling
link in `docs/` fails that build. `task site-build` runs the same locally; it is deliberately not a
step of `task check`, which reaches no network.

## Where work is tracked

| what | where |
|---|---|
| the store — initiative, epics, stories, ADRs | `.engineering/planning/`, validated by `protocol artifact validate` |
| the requirements and their pins | `docs/requirements.md` |
| designs, normative and proposed | `docs/design/` |
| the adopter's guide — what the site's navbar points at | `docs/guide/` |
| what a user of the runtime sees change | `CHANGELOG.md` |

## Planning artifacts

Plan items are markdown files under `.engineering/planning/<kind>/<slug>.md`: YAML frontmatter the
`protocol` CLI owns, and a body the agent and operator own. The repository-local skill at
`.agents/skills/planning/SKILL.md` carries the full model and store conventions.

Kinds, relations, statuses and legal moves come from validated lifecycle documents. Ask the CLI —
`protocol artifact kinds`, `relations`, `lifecycle <kind>`, `list`, `board`, `graph` — instead of
reciting them. Before the first planning-store write of a session, run `protocol artifact list`.

1. **A status changes only through `protocol artifact move`.** Never edit `status:` directly.
2. **Never edit a planning-store file directly.** `new` creates, `relate` links, `move` moves,
   `body <id> --from <path|->` writes prose.
3. **After a batch, run `protocol artifact validate` and relay its output verbatim.**
4. **A refusal is an answer.** Relay the legal moves the CLI names; do not route around it.
5. **An already-satisfied or wrong request still gets an artifact** recording the finding.

New artifacts start in the lifecycle's initial state. Lifecycle moves are claims about project
state: propose them and wait for the operator unless the operator asked for the specific move.
`protocol` must be on `PATH`; if it is absent, do not improvise machine-owned frontmatter.

## Conventions

* **Tests are named after the behaviour they protect**, not the function they call:
  `a_failed_precondition_yields_no_decision_and_names_the_rule`. A test cited by the register is
  part of the public record; renaming one means editing the register in the same commit.
* **Every test asserts a reason**: match the variant (`assert_eq!(error, CoreError::…)` or
  `matches!`), never `is_err()`.
* **Verify a guard by breaking it** before trusting it: apply the one-line mutation it is meant to
  catch, watch it fail with a message that names the defect, revert.
* **Rust CLIs use `clap`'s derive API.** Hand-rolled argument parsing is not accepted.
* **Task runner is `Taskfile.yml`** (go-task). Do not add a Makefile.
* **Comments explain why.** Doc comments on public items say what the type is *for*, and where a
  design decision is embedded in it, why.
* **Dependencies.** The workspace has four direct third-party crates: `serde`, `serde_json`,
  `serde_yaml`, `clap`. The kernel may use the first two. Prefer no dependency, and justify a new
  one in the manifest beside the line that adds it.

## Changelog

`CHANGELOG.md` is maintained with the work. Every change a *user of the runtime* sees — a new
operator, a changed refusal, a new CLI verb, a rule that now refuses what it used to allow — gets a
line under `## [Unreleased]` in the same commit. Write it for the person hitting the behaviour.

## Releases

The bare-version tag is an org-wide convention (atlas § *Naming*): `0.1.0`, never `0.1.0-slug`.
The full gate comes first — component gates are not enough. The tag is annotated, points at the
commit that delivered the work, and its `CHANGELOG.md` heading matches the version.

Pushing the tag is the release: `.github/workflows/release.yml` re-runs the gate, builds the
`entity` command for Linux (x86_64, aarch64), macOS (x86_64, arm64) and Windows (x86_64), and
creates the GitHub Release with the archives, a `SHA256SUMS` file and the tag's `CHANGELOG.md`
section as its notes. A tag with no changelog section gets generated notes and a line saying so —
cut the section first.

```console
task check
$EDITOR CHANGELOG.md                      # move [Unreleased] under ## [X.Y.Z] - YYYY-MM-DD
scripts/as-bot.sh commit -F msg.txt
scripts/as-bot.sh tag -a X.Y.Z -F tag.txt
scripts/as-bot.sh push origin main X.Y.Z
```

## Commits

* **Commits land as the org bot.** `scripts/as-bot.sh commit …` and `scripts/as-bot.sh push …`
  run git with `b10x-bot[bot]` authorship and the App's installation token (`scripts/bot-token.sh`
  mints it from `~/.config/b10x/`). See `atlas/docs/bot-only-commits.md`.
* Conventional prefixes: `feat:`, `fix:`, `docs:`, `refactor:`, `test:`, `chore:`.
* Title, blank line, then a body explaining what changed and why. No title-only commits.
* Ticket references go in a `Refs:` tagline at the end of the body, never in the title.
* Write messages through a file or a quoted heredoc (`git commit -F -` with `<<'MSG'`), never
  `-m "…"` with backticks in the text.
