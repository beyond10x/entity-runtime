# AGENTS.md — entity-runtime

The contract for changing **this** repository. Read it before changing anything.

Org-wide rules — repo naming, the language rule (anything that runs is Rust, not Python), the former-brand rule and the rule that a change to bytes another
repo verifies is a coordinated migration with an ADR — live in `atlas/AGENTS.md` and are not
restated here.

`README.md` orients a reader. This file says what must not break.

## Serves

The objectives of the collection this repository moves, by id from `atlas/ROADMAP.md` — the only
cross-repository roadmap, and the page that says what each id means and which evidence closes it:

- **O2 — decisions as data, with evidence.** State, lifecycle, legal moves, rules and events as data an IO-free kernel decides; the protocol's artifact model runs on it (atlas ADR 0002).

A change here that moves none of these is a question for the operator, not a task.
`atlas/scripts/check-map.sh` fails a repository whose `AGENTS.md` names no objective.

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
   *Enforced by* `crates/entity-core/tests/purity.rs`, which strips comments and string literals
   (so prose about `std::fs` is not a breach and a dereference at the start of a line is not
   mistaken for one), expands every `use` path (so `use std::{fs, env};` and
   `use std::env::var as fetch;` are both seen), and matches whole words (so `Operand::` is not a
   `rand::`). It is checked against fourteen plantings it must catch and eight lookalikes it must
   not, and a second test pins the dependency list — every dependency table, not just the literal
   `[dependencies]` — to `serde` and `serde_json`.
2. **Same inputs, same `Decision`, same bytes.** Ordered maps only; no `HashMap`/`HashSet`.
   *Enforced by* the same scan (`HashMap` and `HashSet` are banned tokens) and
   `the_same_inputs_produce_the_same_decision_byte_for_byte`.
3. **A refusal changes nothing.** Every kernel entry point takes the instance by shared reference
   and returns a new one; there is no code path that mutates the caller's.
   *Enforced by* the signatures of `create` and `execute` and by
   `a_refusal_leaves_the_caller_owned_instance_untouched`.
4. **The kernel never writes a lifecycle state except through an operation.** `lifecycle_state` is
   assigned in `create` and `execute` and nowhere else; there is no setter and no generic status
   write, and nothing is ever deleted.
   *Enforced by* those two functions being the only writers, and by `execute` refusing an instance
   whose state the definition does not declare (`UnknownState`,
   `an_instance_claiming_a_state_the_definition_does_not_declare_is_refused`). It is **not**
   enforced by the type: `EntityInstance` has public fields and deserialises, because an instance is
   data a store round-trips, so which instance reaches the kernel is the shell's responsibility
   (R-80). This invariant said "closed by the type" until the 0.1.0 review showed it was not;
   sealing the type is a breaking change and a story. Do not restate the stronger claim.
5. **Rules and templates see only what their scope allows, and every path is checked.** An
   invariant cannot read `$args`, `$old_fields`, `$from_state` or `$to_state`; a precondition
   cannot read `$state`, which would mean the state the operation is heading for; a creation event
   has neither arguments nor a previous state. Any reference to a field, nested property or
   argument the schema does not declare — at any depth — is refused at registration, in a rule and
   in a `set` or event template alike.
   *Enforced by* `validate_reference` and `validate_reference_path` in
   `crates/entity-core/src/validation.rs` and the tests
   `a_precondition_may_not_read_state_and_an_invariant_may_not_read_the_transition`,
   `a_nested_reference_path_is_checked_against_the_schema`,
   `a_template_the_scope_cannot_resolve_is_refused_at_registration`.
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
   link. Every member opts in with `[lints] workspace = true`; a new crate that omits that line
   is outside every lint here.
10. **Every requirement is pinned, and the pin exists and runs.**
    *Enforced by* `scripts/check-requirements.py` in the gate: every `R-nn` is referenced by a
    design under `docs/design/`, every cited test is a live `#[test]` function under `crates/`
    (not merely a `fn`, and not `#[ignore]`d), every row names its evidence, and every `R-nn` the
    register mentions has a row the checker can actually parse — a marker in an id cell once made
    21 rows invisible to all of the above.
11. **A definition document's keys are closed.** Every definition struct is
    `#[serde(deny_unknown_fields)]` and a condition carries exactly one known operator.
    *Enforced by* those attributes and by `Condition`'s hand-written `Deserialize`
    (`crates/entity-core/src/definition.rs`), plus
    `a_misspelled_definition_key_is_refused_rather_than_ignored` and
    `a_condition_carrying_two_operators_or_an_unknown_one_is_refused`. A key nobody reads is a rule
    nobody enforces.

## Gate

```console
task check
```

Ten steps, in this order: `fmt-check` · `clippy` (`--workspace --all-targets --locked
-D warnings`, which is what makes `missing_docs` fatal) · `test` · `doc-check`
(`RUSTDOCFLAGS=-D warnings`) · `example-check` (`entity validate examples/*.yaml` and
`examples/aep/*.yaml`, `examples/references/*.yaml`) · `req-check` · `pin-check` (every `PIN.md` under `crates/` still hashes to
what it records, in both directions — a moved copy and an unpinned file beside it) ·
`plan-check` (`protocol artifact validate`) · `postgres-check` (the Postgres provider's tests
against the server `ENTITY_POSTGRES_URL` names, or one printed line saying they did not run) ·
`notes-check`. Every cargo step runs `--locked`, so the gate judges the dependency
set the repository committed rather than one cargo re-resolved on the way past.

One check is deliberately **outside** the gate. `pin-check` holds the AEP fixture against its own
`PIN.md`; whether that fixture is still what `engineering-protocols` ships is a different question,
and answering it means cloning their repository. `.github/workflows/upstream-pin.yml` asks it weekly
(`scripts/check-upstream-pin.py <checkout>`, runnable locally against a sibling clone), so the gate
stays network-free and drift surfaces as its own red run rather than as a puzzling failure in an
unrelated step. It was added because the fixture went stale for real: `vision.yaml` landed upstream
and this repository stayed green while its equivalence test claimed to cover every ladder.

CI runs the first six through **one reusable workflow**, `.github/workflows/gate.yml`, which
`check.yml` and `release.yml` both call: a tag cannot be cut against a shorter gate than a pull
request had to pass. It also runs an `msrv` job on 1.85.0, because `rust-version` is a promise to
anyone who depends on these crates and nothing else would notice it breaking. `plan-check` is local
only — CI has no `protocol` binary. If you add a step, add it to both the Taskfile and `gate.yml`.

Land nothing that does not pass all seven. Read the gate's own exit status, not a pipeline's:
`task check 2>&1 | tail` reports `tail`'s.

**Prose states no count of the gate's suites or tests.** That number lives in exactly one place:
the gate's output.

## Branch protection

`main` carries a ruleset (`main: checks before merge`, id 21404415): `gate / Gate`,
`gate / MSRV 1.85` and `Build Docusaurus` must pass, the branch cannot be deleted, and history
cannot be rewritten. **`b10x-bot` and repository admins bypass it**, which is what keeps
`scripts/as-bot.sh push origin main` working — the rule exists for pull requests, where a
Dependabot bump used to be mergeable with a cancelled or failing site build.

If a check name changes, change it here too: a required check that no longer runs blocks every
pull request until the ruleset is edited (`gh api repos/beyond10x/entity-runtime/rulesets/21404415`).

## Boundaries

* **Vocabulary crosses to `engineering-protocols`; a dependency is a decision not yet taken.** No
  `Cargo.toml` here names a crate of theirs and none there names one of ours until an ADR in `atlas`
  says which way the arrow points. Both repositories are public.
* **Provider interfaces live outside `entity-core`.** A state store, an event store, a search
  index, a blob store — each is a crate that depends on the kernel, never the reverse.
* **The shell owns IO.** `entity-cli` reads files and stdin and prints; nothing else here does.
  If a new verb needs a clock, the clock is read in the CLI and passed in as an argument.
* **No step of `task check` calls a network service of its own** — nothing downloads a schema,
  resolves a remote `$ref` or calls an API — and no step spends money. The one exception is
  opted into by name: `postgres-check` talks to the server `ENTITY_POSTGRES_URL` names and to
  nothing when it is unset, and says which. Cargo may still populate its
  registry cache on a cold machine; `--locked` is what keeps that from changing what is built.
  `task site-build` is excluded from `check` because `npm ci` genuinely fetches.
* **Never commit a credential, a token or anything adopter-internal.**
* **This repository is not a live-evaluation subject today, and that is a fact rather than a
  policy.** It carries a planning store under `.engineering/` and could be driven, but it ships no
  step map — `engineering-protocols` owns `drivers/development/default.yaml` and is the subject the
  harness comparison actually runs against. If you are ever asked to drive a harness against this
  repository, do not rediscover the machinery: `metaharness`' `AGENTS.md` § *Live-evaluating our
  own harness* has the procedure, the reinstall-before-you-run rule and the traps that each cost a
  paid model run, and `engineering-protocols`' `AGENTS.md` § *Being the subject of a live harness
  evaluation* has what a subject repository has to get right. Both are paid runs; neither belongs
  in `task check`, whose no-money rule above would forbid it anyway.

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
| the order the adoption goes in, and the decisions taken | `docs/roadmap.md` |
| the AEP artifact model as definitions, and the pinned upstream it is checked against | `examples/aep/`, `crates/entity-yaml/tests/fixtures/aep-lifecycles/` |

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
* **Dependencies.** The workspace has five direct third-party crates: `serde`, `serde_json`,
  `serde_yaml_ng`, `clap`, and `postgres` in `entity-postgres` alone (no default features; the
  manifest says why). The kernel may use the first two — `crates/entity-core/tests/purity.rs`
  fails if that changes. `serde_yaml_ng` replaced `serde_yaml`, whose last release marks itself
  deprecated and receives no fixes; the reason is in the workspace manifest beside the line. Prefer
  no dependency, and justify a new one in the manifest beside the line that adds it.

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
