# Adversarial review — 0.1.0

**Scope.** Everything at tag `0.1.0` (`ac1f6c5`) plus `9cabed8`: the three crates, the tests that
pin the register, the gate scripts, the workflows, the published docs and site. **Method.** Two
passes: a hands-on pass that drove the built binary against the claims the documents make and
against inputs the design does not expect (§ 2), and an independent code-review pass over the
sources (§ 3). Every finding carries its reproduction or its `file:line`. Severity is about the
consequence for a user of the runtime, not about the size of the fix.

**Verdict.** 0.1.0 held its shape — the evaluation order, the refusal-changes-nothing property, the
determinism and the absence of IO are real — and it overclaimed in four places, refused less than it
said in nine, and left three gaps in the shell. **Everything below is addressed**: § 6 is the
disposition, one row per finding, with what changed and the test that now holds it. The one thing
deliberately *not* changed is the two-valued rule language, which is a design decision with a story
behind it rather than a defect.

## 1. What was verified and holds

| claim | how | result |
|---|---|---|
| every transcript in `docs/guide/getting-started.md` | `ticket.yaml` extracted from the page, each command run against `target/debug/entity` | byte-identical output, exit codes as stated |
| `docs/guide/cli.md` pipeline, Decision JSON, validation refusal | same | identical content (the page compacts the JSON layout) |
| README install line `cargo install --git … entity-cli` | run with `--tag 0.1.0` into a scratch root | `entity 0.1.0` |
| release assets | `SHA256SUMS` checked, linux x86_64 archive run | `entity validate examples/order.yaml` exit 0 |
| `rust-version = "1.85"` | `cargo +1.85.0 build --workspace --locked` | exit 0 — the declared MSRV builds |
| the gate | `task check` at `ac1f6c5` | exit 0, 7 steps |

## 2. Findings from the hands-on pass

### F-1 · high · A forged instance is executed as if the kernel had produced it

`EntityInstance` has public fields and derives `Deserialize` (`crates/entity-core/src/runtime.rs`,
`pub struct EntityInstance`). Nothing in `execute` checks that the instance's `lifecycle_state` is
one of the definition's states, that its `revision` is plausible, or that it was ever created.

```console
$ cat forged.json
{"entity":"order","version":1,"id":"ghost","lifecycle_state":"approved","revision":99,
 "fields":{"customer_id":"x","total_cents":1}}
$ entity execute --definition examples/order.yaml --instance @forged.json \
    --operation fulfill --arguments '{"tracking_number":"T"}' --format text
order ghost is fulfilled (revision 100); events: OrderFulfilled
$ echo $?
0
```

**Consequence.** The register's R-34 ("the lifecycle state is not a patchable field … only `create`
and `execute` write `lifecycle_state`") and `AGENTS.md` invariant 4 ("the lifecycle state has no
setter") are true of the kernel's *API* and false of the *type*: any caller can construct the state
it wants and the kernel will continue from it. Within the design — the shell owns persistence, so
the shell decides what instances exist — this is a trust boundary, not a bug. But the two documents
state it as a closed property, and a reader who believes them will not add the check to their
shell.

**Fix options.** (a) Restate R-34 and invariant 4 honestly: *the kernel never writes the state
except through an operation; what instance the shell hands it is the shell's responsibility* — and
add the check to the shell guidance. (b) Additionally make `execute` refuse an instance whose
`lifecycle_state` is not a declared state (`UnknownState`), which is cheap and catches the
`limbo` case with a correct message instead of `invalid_transition … from 'limbo'`. (c) Seal the
type — private fields, a constructor only `create` can reach, `Deserialize` through a validated
`RawInstance` — which is aep' *parse, then validate* rule (their invariant 2) and
the only option that makes the claim true as written. (c) is a design change and a story.

### F-2 · medium · A definition registered under an existing `(entity, version)` silently replaces it

`Registry::register` ends in `self.definitions.insert(key, definition)` (`registry.rs:32`) and the
doc comment says so. Through the CLI, two `--definition` files with the same identity make the last
one win with no message:

```console
$ entity execute --definition examples/order.yaml --definition order2.yaml \
    --instance @state.json --operation fulfill …
{ "kind": "validation", "errors": [ … ] }        # state.json was created under order.yaml
```

**Consequence.** R-12 makes `(entity, version)` the identity that instances are executed against.
Replacing a version in place means an instance created under one definition is executed under a
different one with the same name — the exact situation R-45's `EntityMismatch` exists to refuse,
made invisible. **Fix.** `register` refuses a duplicate key (`DefinitionError::DuplicateDefinition`)
and grows a `replace` for the caller who means it.

### F-3 · medium · Constraints that do not apply to a field's kind are accepted and ignored

```yaml
colour: { type: string, values: [red, green] }   # `values` only means something for enum
count:  { type: integer, min_length: 3 }          # `min_length` only for string
tags:   { type: string, items: { type: string } } # `items` only for array
```

```console
$ entity validate silent.yaml
silent.yaml: valid (silent v1)
$ entity create --definition silent.yaml --id s-1 \
    --fields '{"colour":"purple","count":1,"tags":"x"}' --format text
silent s-1 is a (revision 1); events: none
```

**Consequence.** The author believes `colour` is constrained to two values; it is not, and nothing
told them. `validate_field_definition` (`validation.rs`) checks `min>max`, enum-without-values and
array-without-items, but not the converse. **Fix.** Refuse a constraint on a kind it does not apply
to (`InvalidField … 'values' applies to enum, not string`), at registration. One match arm per
constraint.

### F-4 · medium · The requirements checker accepts any function name as a test

`scripts/check-requirements.py` builds `defined_tests` from `^\s*fn ([a-z][a-z0-9_]*)\s*\(` over
every `.rs` file. At `0.1.0` that set has 124 names, of which 57 are `#[test]` functions and 67 are
helpers (`definition`, `with`, `open_ticket`, `btree_to_map`, …). A register row citing `` `with` ``
passes the gate.

**Consequence.** Invariant 10 in `AGENTS.md` — "every cited test is a `fn` under `crates/`" — is
literally what the script checks and is weaker than what the register promises ("a backticked name
is a test function"). **Fix.** Match `#[test]` (and `#[tokio::test]`-style attributes, should any
appear) immediately before the `fn`; report the count of test functions rather than all functions.

### F-5 · medium · `serde_yaml` is deprecated upstream

```console
$ cargo tree --workspace | grep serde_yaml
serde_yaml v0.9.34+deprecated
```

`entity-yaml` and `entity-cli` depend on a crate whose last release marks itself deprecated and
which receives no security fixes. `aep` carries the same dependency, so this is
an org-level choice, but it is not written down anywhere here. **Fix.** Decide (`serde_yaml_ng`,
`serde_yml`, or stay and say why) and record it in the manifest beside the line, as `AGENTS.md`
§ Dependencies requires of a justified dependency.

### F-6 · low · An integer above `i64::MAX` is reported with a wrong reason

`validate_value` maps `as_u64()` through `as i64`, so `18446744073709551615` becomes `-1`:

```console
$ entity create --definition big.yaml --id b --fields '{"n": 18446744073709551615}'
{ "errors": [ { "message": "value -1 is below minimum 0", "path": "fields.n" } ], … }
```

The value is refused — with `min: 0` — but for the wrong reason, and without a `min` it is
*accepted* while any `max` comparison sees `-1`. **Fix.** Compare through `f64` for both branches
(`as_f64()` handles `u64`), or refuse integers outside `i64` explicitly.

### F-7 · low · The purity scan has blind spots the register does not mention

`crates/entity-core/tests/purity.rs`: `std::io`, `std::os`, `chrono`, `time::`, `libc` are not in
`BANNED`; a code line that begins with `*` (a dereferencing assignment) is dropped by the comment
filter; and the manifest check splits on the literal `[dependencies]`, so a
`[target.'cfg(unix)'.dependencies]` table or `[dependencies.foo]` form is invisible to it.
Simulated against the current filter: `std::io::stdin().read_line(…)` passes, `chrono::Utc::now()`
passes, `*slot = std::time::SystemTime::now();` is skipped as a comment.

**Consequence.** R-01's pin is real but narrower than "nothing in `entity-core` can perform IO".
**Fix.** Add the tokens; treat a `*`-prefixed line as a comment only when the previous code line
opened a `/*`; scan the whole manifest for any `dependencies` table.

### F-8 · low · `create` accepts an empty identity

```console
$ entity create --definition examples/order.yaml --id "" --fields '{…}' --format text
order  is draft (revision 1); events: OrderCreated
```

The design makes the id opaque to the kernel (R-71), which is right; opaque is not the same as
empty. **Fix.** Refuse an empty or whitespace-only id at `create`
(`CoreError::Validation` at path `id`), and say in the design that everything else about the id is
the shell's.

### F-9 · low · A misspelled operator is diagnosed as an untagged-enum mismatch

```console
$ entity validate typo.yaml           # assert: { gte_: [$fields.n, 0] }
error: typo.yaml: invalid entity YAML: invariants[0]: data did not match any variant of
untagged enum Condition at line 5 column 5
```

Line and column are there; the operator list is not. **Fix.** A custom `Deserialize` for
`Condition` that names the unknown key and lists the thirteen operators, or a post-parse check on
the raw value before it reaches serde.

### F-10 · low · `pages.yml` fails on upstream advisories unrelated to the change

`npm audit --audit-level=critical` runs on every build. A new critical advisory in a transitive
dependency turns the docs deploy red on a commit that touched a Markdown file. Inherited from
`agentic-principles/website`; worth a decision (keep as a tripwire, or move to a scheduled job).

### F-11 · info · MSRV

`rust-version = "1.85"` is declared, and `cargo +1.85.0 build --workspace --locked` exits 0 for
this review. CI does not run it, so the promise is checked by hand and will break silently the
first time a dependency raises its own MSRV; `aep` has a dedicated `msrv` job for
exactly this. **Fix.** Add the job.

### F-12 · info · `validate` stops at the first invocation error

`entity validate a.yaml missing.yaml c.yaml` exits `2` at `missing.yaml` and never reports `c.yaml`.
Definition *errors* are accumulated across files as designed; *IO* errors are not. Arguably right
(an unreadable path is the caller's mistake, not the definitions'); noted so the choice is a
choice.

## 3. Findings on the delivery pipeline (verified against GitHub)

Surfaced by the independent review's cross-file tracer on `9cabed8` and re-verified here with
`gh` on 2026-08-25.

### F-13 · medium · Two Dependabot PRs carry no site build result at all

Runs 32825028337 (PR #2, `actions/setup-node` 6→7) and 32825035871 (PR #3, `actions/checkout`
4→7) were cancelled by the shared concurrency group before `9cabed8` fixed it, and nothing
re-triggers a `pull_request` build when `main` changes. `gh pr checks 2` and `gh pr checks 3` show
`check: pass` only. Both PRs bump a major version of an action **used by `pages.yml` itself**.

**Consequence.** Either PR merges with the exact "cancelled reads like passed" hole the fix commit
describes, and the next docs push to `main` finds out. **Fix.** `gh run rerun 32825028337
32825035871`, or `@dependabot rebase` on both.

### F-14 · medium · Nothing requires the site build to pass before a merge

`gh api repos/beyond10x/entity-runtime/branches/main/protection` → `Branch not protected`;
`rules/branches/main` → `[]`. `Build Docusaurus` and `check` are advisory. PR #5 (`typescript`
6→7) is open and mergeable with `Build Docusaurus: FAILURE`.

**Consequence.** A docs PR whose build fails or is skipped by the `paths:` filter merges without a
blocker, and `main`'s deploy goes red — `onBrokenLinks: 'throw'` protects the build, not the
branch. Bot-only pushes make this less likely, not impossible. **Fix.** A ruleset on `main`
requiring `check` and `Build Docusaurus`; decide at the same time whether the org-wide bot-only
push rule (atlas `docs/bot-only-commits.md`) is switched on here.

### F-15 · low · Dependabot's Monday burst now runs up to ten concurrent site builds

Three ecosystems, limit 5 each, 06:00–06:30. Each matching PR runs `npm ci` and `npm audit`
against the registry; before `9cabed8` all but one were cancelled, now all run. Public-repo
minutes are free, so the cost is noise — and a registry rate limit turning several PR builds red
for nothing. Low confidence that it bites; recorded because the fix changed the failure mode
rather than removed it.

## 4. Findings from the whole-repository code-review pass

An independent review over the whole tree at `9cabed8`, eight finder angles into a verifier pass.
Its ranked findings, deduplicated against § 2 and § 3, and each reproduced here before being acted
on.

### F-16 · high · A definition's unknown keys were dropped, so a rule could enforce half of itself

`Condition` was `#[serde(untagged)]` and no definition struct set `deny_unknown_fields`. Three
consequences, all reproduced:

```console
$ entity validate dual.yaml      # invariants: [{ assert: { eq: [1,1], ne: [1,1] } }]
dual.yaml: valid (dual v1)       # parsed as `eq`; the `ne` branch vanished
$ entity validate typo2.yaml     # fields: { n: { type: integer, requried: true } }
typo2.yaml: valid (typo2 v1)
$ entity create --definition typo2.yaml --id t --fields '{}' --format text
typo2 t is a (revision 1); events: none      # `required` was never set
```

An operation carrying `precondition:` instead of `preconditions:` registered with no guard at all.
For a format whose module doc says it is "safe to load from a file somebody else wrote", the failure
is that the file's author and its reader disagree about what was written.

### F-17 · medium · `$state` in a precondition meant the state being moved *to*

`validate_rule_reference` treated `$state` and `$to_state` as available in every scope, and the
runtime aliased `$state` to the target state:

```console
$ cat st.yaml    # finish: draft -> done, precondition { eq: [$state, draft] }
$ entity create --definition st.yaml --id s --fields '{}'   | entity execute --definition st.yaml --instance - --operation finish
precondition_failed - must_be_draft
```

The natural reading — *we are currently in draft* — refuses every time it should pass, and
`eq: [$state, done]` passes always, so a guard on the current state guards nothing. `$to_state` was
symmetrically admitted inside invariants, which R-52 says it may not read.

### F-18 · medium · A `set` or event template was never checked at registration

```console
$ entity validate tpl.yaml    # create.emit.payload: { from: $from_state, who: $args.actor }
tpl.yaml: valid (tpl v1)
$ entity create --definition tpl.yaml --id x --fields '{}'
{ "kind": "template", "expression": "$from_state", ... }
```

A creation event has no previous state and no arguments. Both facts are known when the definition is
read; both surfaced as a run-time refusal on every single call, after preconditions and `set` had
already run.

### F-19 · medium · A nested reference path was accepted and then read `false` forever

`validate_known_root_field` checked only the first segment, so `$fields.address.countri` registered
against a schema that declares `address.country` and no `countri`. Every `create` then refused with
an invariant violation, from a file `entity validate` called valid. `$fields.title.length` — a path
into a string — behaved the same.

### F-20 · medium · A default declared inside an object was validated and never applied

```console
$ entity validate nested.yaml    # address: { type: object, properties: { country: { default: DE, required: true } } }
nested.yaml: valid (nested v1)
$ entity create --definition nested.yaml --id n --fields '{"address":{}}'
"path": "fields.address.country"    # required field is missing
```

The default was checked against its own field at registration — and then only top-level defaults
were ever filled in.

### F-21 · medium · `eq` compared a number's representation while `gte`/`lte` compared its value

```console
$ entity create --definition num.yaml --id n --fields '{"total": 100.0}' > n.json
$ entity execute --definition num.yaml --instance @n.json --operation check_eq
{ "kind": "precondition_failed", ... }         # eq: [$fields.total, 100]  -> false
$ entity execute --definition num.yaml --instance @n.json --operation check_rng --format text
num n is a (revision 2); events: none          # gte 100 AND lte 100       -> true
```

A definition tested with integer fixtures passes, then refuses real documents carrying `100.0`.

### F-22 · low · The release gate was shorter than the CI gate

`release.yml`'s gate ran fmt, clippy, test, examples and req-check — no `cargo doc` with
`RUSTDOCFLAGS=-D warnings`, which `task check` and `check.yml` both run. `AGENTS.md` § Releases said
release "re-runs the gate". A tag whose only defect was a broken intra-doc link would have shipped.
The same job used mutable action tags (`checkout@v4`, `download-artifact@v4`) while holding a token
that can publish, and left that token in `.git/config` for later steps.

### F-23 · low · `task check` claimed a network boundary nothing enforced

`AGENTS.md`: "Nothing in `task check` reaches the network". No cargo step passed `--locked` or
`--offline`, and there is no `.cargo/config.toml`, so a cold cache fetches from crates.io and a
manifest change silently rewrites `Cargo.lock` mid-gate. **CONFIRMED** by the review's verifier
against the Taskfile and `Cargo.lock`'s 34 registry entries.

### F-24 · low · Two tests asserted only that something failed

`a_missing_reference_makes_a_comparison_false_and_exists_is_the_presence_test` and
`numeric_comparisons_are_numeric_and_compare_false_otherwise` ended in
`assert_eq!(result.is_ok(), holds)`, against `AGENTS.md`'s own rule that a test asserts a reason. A
regression turning a false precondition into a validation error would have passed both.

### F-25 · info · Reuse and efficiency

Carried without separate reproduction, and addressed where the fix was structural rather than
cosmetic: `validate_object`/`validate_inline_object` were a 30-line copy-paste; three CLI sites
built a throwaway `Registry` to validate one definition; every `$fields.x` reference deep-copied the
whole field map, and `execute` copied it three times per call; the gate step list existed in three
hand-copied places. Left alone: the per-field error path allocation (measured cost small, listed as
a roadmap row rather than fixed blind).

## 5. Not found

## 5. Not found

* No path by which the kernel reaches a clock, the filesystem, the network or a random source at
  `0.1.0` — the blind spots in F-7 are in the *scan*, not in the code it scans (read in full, and
  the strengthened scan agrees).
* No ordering nondeterminism: every map in `entity-core` is a `BTreeMap` or a `serde_json::Map`
  built from one; the determinism test compares serialised bytes.
* No partial mutation on refusal: `execute` builds `new_fields` from a clone and returns before
  constructing the next instance on every error path.
* No mismatch between the eleven documented steps and the body of `execute`.

## 6. Disposition

Every finding, and what it produced. "Register" is the row in
[`docs/requirements.md`](../requirements.md); "pinned by" is the test that now fails if the defect
returns. The gate was green after each batch and is green now.

| id | disposition | register | pinned by |
|---|---|---|---|
| F-1 / B1 | **Restated, and narrowed.** R-34 and AGENTS invariant 4 no longer claim the type closes the state; they claim what is true — only `create` and `execute` write one. `execute` now refuses an instance whose state the definition does not declare. Sealing `EntityInstance` is a roadmap row with its reason | R-34 ✎, R-35 ✚ | `an_instance_claiming_a_state_the_definition_does_not_declare_is_refused`, `an_instance_carrying_a_state_the_definition_does_not_declare_is_refused_by_name` |
| F-2 | **Fixed.** `Registry::register` refuses a duplicate `(entity, version)`; `Registry::replace` is how to mean it | R-15 ✚ | `registering_over_an_existing_definition_is_refused_and_replace_is_how_to_mean_it`, `two_definitions_of_the_same_type_and_version_are_refused` |
| F-3 | **Fixed.** A constraint on a kind it does not govern is refused at registration | R-26 ✚ | `a_constraint_that_does_not_apply_to_its_kind_is_refused` |
| F-4 / C6 / B5 | **Fixed, and the checker gained a check.** `check-requirements.py` requires a live `#[test]`, refuses an `#[ignore]`d pin, and now also refuses a row it cannot parse — a marker in an id cell had made 21 rows invisible | AGENTS inv. 10 | the script's own run in `req-check` |
| F-5 | **Fixed.** `serde_yaml_ng` replaces the deprecated `serde_yaml`; the reason sits beside the line in the workspace manifest | AGENTS § Dependencies | `cargo tree` in the gate's build |
| F-6 / A1 / B6 | **Fixed.** Integers are compared as `f64` rather than coerced through `i64` | R-20 ✎ | `an_integer_beyond_i64_is_compared_numerically_not_wrapped` |
| F-7 / B3 | **Fixed.** The scan strips comments and strings, expands `use` paths, matches whole words, and is itself checked against 14 plantings and 8 lookalikes | R-01 ✎ | `the_scan_sees_every_evasion_it_is_meant_to_see`, `the_scan_does_not_fire_on_prose_or_lookalikes` |
| F-8 | **Fixed.** An empty or whitespace identity is refused | R-75 ✚ | `an_empty_identity_is_refused` |
| F-9 / A4 | **Fixed.** `Condition` has a hand-written reader that names the unknown operator and lists the twelve that exist | R-16 ✚ | `a_condition_carrying_two_operators_or_an_unknown_one_is_refused` |
| F-10 | **Fixed.** `npm audit` moved out of the documentation deploy into a weekly `audit.yml` | — | `.github/workflows/audit.yml` |
| F-11 | **Fixed.** An `msrv` job on 1.85.0 runs in the shared gate workflow | — | `.github/workflows/gate.yml` |
| F-12 / C4 | **Fixed.** `validate` reports every file and summarises; a file it cannot read is one of its findings | R-96 ✚ | `validate_reports_every_file_and_a_broken_one_is_a_finding_not_a_usage_error` |
| F-13 | **Open — needs the push.** Runs 32825028337 and 32825035871 are still cancelled. The workflow rewrite supersedes those two Dependabot PRs (every action is now pinned to the newest major by SHA); they should be closed once this lands | — | — |
| F-14 | **Open — needs the push.** A ruleset on `main` requiring `Gate / Gate` and `Build Docusaurus`, with a bypass for `b10x-bot` so direct bot pushes still work, can only name checks that have run under their new names | — | — |
| F-15 | **Fixed.** Dependabot updates are grouped per ecosystem, limit 3, and TypeScript majors are ignored while Docusaurus refuses them | — | `.github/dependabot.yml` |
| F-16 / A4 / a4 | **Fixed.** Every definition struct denies unknown fields; a condition carries exactly one known operator | R-16 ✚ | `a_misspelled_definition_key_is_refused_rather_than_ignored`, `a_condition_carrying_two_operators_or_an_unknown_one_is_refused` |
| F-17 / A3 / a3 | **Fixed.** Scopes are enforced in both directions: no `$state` in a precondition, no `$to_state` in an invariant | R-52 ✎ | `a_precondition_may_not_read_state_and_an_invariant_may_not_read_the_transition` |
| F-18 / A6 | **Fixed.** `set` values and event payloads are checked against their scope at registration | R-64 ✚ | `a_template_the_scope_cannot_resolve_is_refused_at_registration` |
| F-19 / a2 | **Fixed.** A reference path is walked through the schema, segment by segment | R-14 ✎ | `a_nested_reference_path_is_checked_against_the_schema` |
| F-20 / A2 / B2 / a5 | **Fixed.** Defaults are applied at every depth an object or array element reaches | R-22 ✎ | `a_default_declared_inside_an_object_is_applied` |
| F-21 / A5 | **Fixed.** `eq`/`ne`/`in`/`contains` compare numbers numerically | R-54 ✎ | `equality_is_numeric_so_an_integer_equals_the_same_number_written_with_a_decimal_point` |
| F-22 / B4 / c4 / C5 | **Fixed.** One reusable `gate.yml` that `check.yml` and `release.yml` both call; every action pinned by commit; `persist-credentials: false` on every checkout | AGENTS § Gate, § Releases | `.github/workflows/gate.yml` |
| F-23 / c5 | **Fixed both ways.** Every cargo step runs `--locked`, and the boundary is restated: no step calls a network service of its own, and cargo may still populate its cache | AGENTS § Boundaries | `.github/workflows/gate.yml`, `Taskfile.yml` |
| F-24 / c1 / c2 | **Fixed.** Both tests assert the variant through one `assert_verdict` helper | AGENTS § Conventions | `a_missing_reference_makes_a_comparison_false_and_exists_is_the_presence_test` |
| F-25 | **Fixed where structural.** One `validate_members` for schemas and nested objects; one `load_validated` in the CLI; fields held as a `serde_json::Map` so a reference clones a leaf rather than the map; `execute` copies the field map once instead of three times; `Operand` replaced by `Option`; `OneOrMany::as_slice` replaces a boxed iterator; the registry keys by name then version | R-05 ✎, R-71 ✎ | `fields_are_ordered_by_name_so_two_identical_decisions_serialise_alike` |
| c3 | **Fixed.** The register no longer states a count of tests in a suite the gate runs | AGENTS § Gate | — |

### Deliberately not changed

**Rules stay two-valued.** A reference that does not resolve reads `false`, not `unknown`. It is
enough for a lifecycle and not enough for an evidence gate that must tell *nobody looked* from *it is
wrong*, which is the first thing `aep` needs from this kernel. The reasoning is in
[`kernel-v0.1.md` § 4](../design/kernel-v0.1.md#4-the-condition-language) and the work is
`story:three-valued-conditions`.

**`EntityInstance` stays open.** Private fields and a validated `Raw` type would make R-34's
strongest reading true by construction. It is a breaking API change, and it would not remove the
shell's responsibility for which instance it loads — only move where that responsibility is written
down. Roadmap row, with the reason.
