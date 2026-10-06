---
format: aep.planning-md/3
id: design:wave-issues-54-55-unblock-dependents
kind: design
status: draft
title: 'Wave proposal: unblock ESS lowering (#54) and the recorded-store open (#55)'
owner: entity-runtime
refs:
- provider: github
  reference: beyond10x/entity-runtime#54
- provider: github
  reference: beyond10x/entity-runtime#55
relations:
- designs: story:a-condition-reads-the-length-of-a-text
- designs: story:recorded-open-checkpoint-design
revision: 2
---
# Wave proposal: unblock ESS lowering (#54) and the recorded-store open (#55)

## Stage and authority

Skill version 0.19.2 — `aep:implementing`, wave mode. **Stage 2: approved and running.**

- 2026-10-06: the operator approved the wave ("I approve the wave"), option A of the proposal:
  wave 1 as the verb computed it.
- 2026-10-06: the operator set the goal "wave A+B completed in sequence". Read as: wave 1 (option
  A), then `story:a-text-field-declares-its-alphabet` (what option B added) as its own wave after
  wave 1 closes, not in parallel. The commit line below applies to each of the two waves.
  `story:recorded-open-verifies-a-checkpoint-and-its-suffix` is in neither unless the design story
  leaves it unblocked.

Session run: interactive.

## Stage 2 record

| unit | story | stage | branch | head | worktree | build dir | scratch |
|---|---|---|---|---|---|---|---|
| integration | — | opening commit | `wave/er-54-55-w1` | (opening commit) | `~/.local/state/worktree/trees/b10x/entity-runtime/er-w-int` (id `er-w-int`) | none (gate runs here at close: `~/.cache/b10x-target/er-w-int`) | `~/.cache/er-w/int` |
| U1 | `story:a-condition-reads-the-length-of-a-text` | planned | `impl/a-condition-reads-the-length-of-a-text` | — | id `er-w1-u1` | `~/.cache/b10x-target/er-w1-u1` | `~/.cache/er-w/u1` |
| U2 | `story:recorded-open-checkpoint-design` | planned | `impl/recorded-open-checkpoint-design` | — | id `er-w1-u2` | `~/.cache/b10x-target/er-w1-u2` | `~/.cache/er-w/u2` |

## Dependents this unblocks

| dependent | waits on | Entity Runtime pin today |
|---|---|---|
| ESS lowering (`ess-entity-runtime`, ess `crates/generate/ess-entity-runtime/src/subset.rs` at `a81a8729d`) | the five `Needs::EntityCore` rows of GitHub #54 | `tag = "0.24.1"` (ess `origin/main`) |
| connectors (beyond10x/connectors#101, #103) | a bounded open, GitHub #55 | `tag = "0.26.0"` (connectors `origin/main`) |

Each lands for a dependent only through an Entity Runtime release that the dependent then pins.
A release is not part of any wave.

## The computed waves (`aep plan artifact waves --kind story --status draft --format json`)

Selection path: the verb (aep 0.68.0), over typed scope entries written from six
`aep:story-scoper` reports. Coordinator-owned shared files (`CHANGELOG.md`, `docs/requirements.md`,
`ess/ess-inputs.yaml`, `ess/coverage.json`, `ess/generated/suite.json`,
`docs/ess/core-traceability.md`) are declared in each story's Scope and are not unit scope.

| wave | units | serves | scope |
|---|---|---|---|
| 1 | `story:a-condition-reads-the-length-of-a-text` · `story:recorded-open-checkpoint-design` | O2 · O2 | inferred · cited |
| 2 | `story:a-text-field-declares-its-alphabet` · `story:recorded-open-verifies-a-checkpoint-and-its-suffix` | O2 · O2 | inferred · inferred |
| 3 | `story:operation-writes-an-optional-field-from-an-optional-argument` | O2 | cited |
| 4 | `story:set-increments-a-numeric-field` | O2 | inferred |
| 5 | `story:set-clears-an-optional-field` (depends on 3 and 4) | O2 | inferred |

**Proposed now: wave 1 only** (N = 2). Later waves are replanned after wave 1 closes.

`story:recorded-open-verifies-a-checkpoint-and-its-suffix` sits in wave 2 by the verb, but it
depends on the design story and, if the design picks the Eventlog option, on a
`dependency-blocker` the design files. It is not ready until then.

### Disagreement between the verb and the scopers' reading

The five #54 stories collide at file level on `crates/entity-core/src/validation.rs`,
`runtime.rs`, `definition.rs` and `error.rs`, so the verb serialises them into five waves. The
scopers' function-level reading says text length (`runtime.rs:2945-2970`, `validation.rs:1417-1512`)
and alphabet (`definition.rs:333-441`, `validation.rs:2129-2323`, `2603-2626`) touch disjoint
functions and could run together under the declared-range split the skill allows. The verb wins;
this page follows it.

## Wave 1 units

| unit | story | lands in | build |
|---|---|---|---|
| U1 | `story:a-condition-reads-the-length-of-a-text` | `crates/entity-core` path walk + registration path check; new `ess/scenarios/core/service1-text-count-*.yaml` | `cargo test -p entity-core`, `task ess-check` |
| U2 | `story:recorded-open-checkpoint-design` | `docs/design/recorded-open-checkpoint-v0.1.md` (new); release-mode probe in `crates/entity-eventlog/tests/shared_clock_cost.rs`; `docs/ess/evidence/provider-tracking/open-cost-baseline.txt` | `cargo test -p entity-eventlog --release --all-features` (Rust 1.91 toolchain per `eventlog-runtime-check`) |

Dispatch types: `aep:implementor` per unit, then `aep:adversary` per unit; `aep:security-reviewer`
for U1 (kernel change). U2's design gets one independent review (`aep:adversary` reading the
design, recorded as a `review-result`).

Integration branch: `wave/er-54-55-w1`. Unit branches: `wave/er-54-55-w1-u1-text-count`,
`wave/er-54-55-w1-u2-open-design`. Worktrees through `worktree create`; build directories
`~/.cache/b10x-target/er-w1-u1`, `~/.cache/b10x-target/er-w1-u2`, one per unit; scratch roots
`~/.cache/er-w1/u1`, `~/.cache/er-w1/u2`. Paths are recorded here when created.

## Pre-flight, read 2026-10-06

| check | value | verdict |
|---|---|---|
| free disk on `/` | 18G (`df -h /`), was 31G at session start | above the 10G floor; one release-mode `entity-eventlog` build plus one `entity-core` build; re-read before each build |
| primary checkout | `main` at `7926ec45`, dirty: this session's planning-store writes only | goes into the opening store commit |
| linked worktrees | 5 besides the primary: `docs-refresh-20260922-entity-runtime` (2 unpublished commits), `ess-evolution-store-recovery-entity-runtime-20260923` (content already on main, superseded), `textblob-entity-runtime` (clean, detached at `44c14c05`), `wt-9345cf37e990` (clean, at main), `wt-747b1108ca67` (this session's docs overhaul, in progress) | none is a previous wave of this one; none is touched |
| previous build dirs | `~/.cache/b10x-target/entity-runtime-docs` 358M (docs overhaul) | not this wave's |
| compiler cache | `rustc-wrapper = "/usr/bin/sccache"` in `~/.cargo/config.toml` | wired |
| one measured build | not measured this session | measure U1's `cargo test -p entity-core` first |
| model budget | not stated by the operator | default N ≤ 4; proposed N = 2 |

## Commits approval authorises

On the integration branch `wave/er-54-55-w1`: one opening store commit (this session's
reconciliation and the drafted stories, critic records and this page), one commit per unit (more
only for adversary-round corrections), the merges of each green unit into the integration branch,
the closing store commit. Then: push of `wave/er-54-55-w1` through `b10x-gates bot`, one
bot-authored PR to `main`, and its merge through the App after its required checks
(`gate / Gate`, `gate / MSRV 1.85`, `Build Docusaurus`) pass. Nothing else: no tag, no release, no
consumer re-pin, no Eventlog change, no wave 2.

## Deliberately left out

- The 11 unassessed drafts (`story:aep-markdown-materialized-view`, `story:definition-json-schema`,
  `story:definition-migrations`, `story:explain-verb`, `story:gate-and-release-hardening`,
  `story:named-predicates`, `story:one-git-url-one-rev-across-the-workspace`,
  `story:pedantic-lints`, `story:provider-feature-combinations-compile-and-are-gated`,
  `story:schema-fragments`, `story:seeded-open-under-one-second`): no dependent waits on them.
- `story:seeded-open-under-one-second`: lands on the same `entity-eventlog` open path as #55; its
  driver (an AEP migration apply on an Entity Runtime store) predates AEP's git store and was not
  re-checked.
- The docs overhaul: running separately in `wt-747b1108ca67`, uncommitted; landed separately.

## Verb output, verbatim

```json
{
  "waves": [
    {
      "wave": 1,
      "artifacts": [
        {
          "id": "story:a-condition-reads-the-length-of-a-text",
          "inferred": true,
          "scope": [
            {
              "confidence": "cited",
              "path": "crates/entity-core/src/runtime.rs"
            },
            {
              "confidence": "cited",
              "path": "crates/entity-core/src/validation.rs"
            },
            {
              "confidence": "inferred",
              "path": "crates/entity-core/tests/service_values.rs"
            },
            {
              "confidence": "inferred",
              "path": "docs/design/kernel-v0.1.md"
            },
            {
              "confidence": "inferred",
              "path": "docs/design/service-semantics-v0.1.md"
            }
          ]
        },
        {
          "id": "story:recorded-open-checkpoint-design",
          "inferred": false,
          "scope": [
            {
              "confidence": "cited",
              "path": "crates/entity-eventlog/tests/shared_clock_cost.rs"
            },
            {
              "confidence": "cited",
              "path": "docs/design/recorded-open-checkpoint-v0.1.md"
            }
          ]
        }
      ]
    },
    {
      "wave": 2,
      "artifacts": [
        {
          "id": "story:a-text-field-declares-its-alphabet",
          "inferred": true,
          "scope": [
            {
              "confidence": "cited",
              "path": "crates/entity-core/src/definition.rs"
            },
            {
              "confidence": "inferred",
              "path": "crates/entity-core/src/error.rs"
            },
            {
              "confidence": "cited",
              "path": "crates/entity-core/src/validation.rs"
            },
            {
              "confidence": "inferred",
              "path": "crates/entity-surface/src/lib.rs"
            },
            {
              "confidence": "inferred",
              "path": "docs/design/kernel-v0.1.md"
            }
          ]
        },
        {
          "id": "story:recorded-open-verifies-a-checkpoint-and-its-suffix",
          "inferred": true,
          "scope": [
            {
              "confidence": "cited",
              "path": "AGENTS.md"
            },
            {
              "confidence": "inferred",
              "path": "checks/ess-conformance"
            },
            {
              "confidence": "cited",
              "path": "crates/entity-eventlog/src/adapter.rs"
            },
            {
              "confidence": "cited",
              "path": "crates/entity-eventlog/src/adapter/tracked.rs"
            },
            {
              "confidence": "cited",
              "path": "crates/entity-eventlog/src/facade.rs"
            },
            {
              "confidence": "cited",
              "path": "crates/entity-eventlog/src/sync.rs"
            },
            {
              "confidence": "cited",
              "path": "crates/entity-eventlog/tests/shared_clock_cost.rs"
            },
            {
              "confidence": "inferred",
              "path": "docs/design/eventlog-recorded-adapter-v0.1.md"
            },
            {
              "confidence": "inferred",
              "path": "docs/design/eventlog-recorded-sync-bridge-v0.1.md"
            },
            {
              "confidence": "cited",
              "path": "ess/provider-tracking"
            }
          ]
        }
      ]
    },
    {
      "wave": 3,
      "artifacts": [
        {
          "id": "story:operation-writes-an-optional-field-from-an-optional-argument",
          "inferred": false,
          "scope": [
            {
              "confidence": "cited",
              "path": "crates/entity-core/src/definition.rs"
            },
            {
              "confidence": "cited",
              "path": "crates/entity-core/src/error.rs"
            },
            {
              "confidence": "cited",
              "path": "crates/entity-core/src/runtime.rs"
            },
            {
              "confidence": "cited",
              "path": "crates/entity-core/src/validation.rs"
            },
            {
              "confidence": "cited",
              "path": "crates/entity-core/tests/service_binding_boundary.rs"
            },
            {
              "confidence": "cited",
              "path": "docs/design/service-binding-boundary-v0.1.md"
            },
            {
              "confidence": "cited",
              "path": "ess/scenarios/core/conditional-presence-registration-requires-one-typed-optional-leaf.yaml"
            }
          ]
        }
      ]
    },
    {
      "wave": 4,
      "artifacts": [
        {
          "id": "story:set-increments-a-numeric-field",
          "inferred": true,
          "scope": [
            {
              "confidence": "cited",
              "path": "crates/entity-core/src/definition.rs"
            },
            {
              "confidence": "inferred",
              "path": "crates/entity-core/src/error.rs"
            },
            {
              "confidence": "inferred",
              "path": "crates/entity-core/src/lib.rs"
            },
            {
              "confidence": "inferred",
              "path": "crates/entity-core/src/number.rs"
            },
            {
              "confidence": "cited",
              "path": "crates/entity-core/src/replay.rs"
            },
            {
              "confidence": "cited",
              "path": "crates/entity-core/src/runtime.rs"
            },
            {
              "confidence": "cited",
              "path": "crates/entity-core/src/validation.rs"
            },
            {
              "confidence": "inferred",
              "path": "crates/entity-core/tests/replay.rs"
            },
            {
              "confidence": "inferred",
              "path": "crates/entity-core/tests/requirements.rs"
            },
            {
              "confidence": "inferred",
              "path": "crates/entity-core/tests/service_semantics.rs"
            },
            {
              "confidence": "inferred",
              "path": "docs/design/kernel-v0.1.md"
            }
          ]
        }
      ]
    },
    {
      "wave": 5,
      "artifacts": [
        {
          "id": "story:set-clears-an-optional-field",
          "inferred": true,
          "scope": [
            {
              "confidence": "inferred",
              "path": "crates/entity-core/src/definition.rs"
            },
            {
              "confidence": "inferred",
              "path": "crates/entity-core/src/error.rs"
            },
            {
              "confidence": "inferred",
              "path": "crates/entity-core/src/lib.rs"
            },
            {
              "confidence": "inferred",
              "path": "crates/entity-core/src/replay.rs"
            },
            {
              "confidence": "cited",
              "path": "crates/entity-core/src/runtime.rs"
            },
            {
              "confidence": "cited",
              "path": "crates/entity-core/src/validation.rs"
            },
            {
              "confidence": "inferred",
              "path": "docs/design/kernel-v0.1.md"
            },
            {
              "confidence": "inferred",
              "path": "docs/design/service-semantics-v0.1.md"
            }
          ]
        }
      ]
    }
  ],
  "collisions": [
    {
      "a": "story:a-condition-reads-the-length-of-a-text",
      "b": "story:a-text-field-declares-its-alphabet",
      "path": "crates/entity-core/src/validation.rs",
      "confidence": "cited"
    },
    {
      "a": "story:a-condition-reads-the-length-of-a-text",
      "b": "story:a-text-field-declares-its-alphabet",
      "path": "docs/design/kernel-v0.1.md",
      "confidence": "inferred"
    },
    {
      "a": "story:a-condition-reads-the-length-of-a-text",
      "b": "story:operation-writes-an-optional-field-from-an-optional-argument",
      "path": "crates/entity-core/src/runtime.rs",
      "confidence": "cited"
    },
    {
      "a": "story:a-condition-reads-the-length-of-a-text",
      "b": "story:operation-writes-an-optional-field-from-an-optional-argument",
      "path": "crates/entity-core/src/validation.rs",
      "confidence": "cited"
    },
    {
      "a": "story:a-condition-reads-the-length-of-a-text",
      "b": "story:set-clears-an-optional-field",
      "path": "crates/entity-core/src/runtime.rs",
      "confidence": "cited"
    },
    {
      "a": "story:a-condition-reads-the-length-of-a-text",
      "b": "story:set-clears-an-optional-field",
      "path": "crates/entity-core/src/validation.rs",
      "confidence": "cited"
    },
    {
      "a": "story:a-condition-reads-the-length-of-a-text",
      "b": "story:set-clears-an-optional-field",
      "path": "docs/design/kernel-v0.1.md",
      "confidence": "inferred"
    },
    {
      "a": "story:a-condition-reads-the-length-of-a-text",
      "b": "story:set-clears-an-optional-field",
      "path": "docs/design/service-semantics-v0.1.md",
      "confidence": "inferred"
    },
    {
      "a": "story:a-condition-reads-the-length-of-a-text",
      "b": "story:set-increments-a-numeric-field",
      "path": "crates/entity-core/src/runtime.rs",
      "confidence": "cited"
    },
    {
      "a": "story:a-condition-reads-the-length-of-a-text",
      "b": "story:set-increments-a-numeric-field",
      "path": "crates/entity-core/src/validation.rs",
      "confidence": "cited"
    },
    {
      "a": "story:a-condition-reads-the-length-of-a-text",
      "b": "story:set-increments-a-numeric-field",
      "path": "docs/design/kernel-v0.1.md",
      "confidence": "inferred"
    },
    {
      "a": "story:a-text-field-declares-its-alphabet",
      "b": "story:operation-writes-an-optional-field-from-an-optional-argument",
      "path": "crates/entity-core/src/definition.rs",
      "confidence": "cited"
    },
    {
      "a": "story:a-text-field-declares-its-alphabet",
      "b": "story:operation-writes-an-optional-field-from-an-optional-argument",
      "path": "crates/entity-core/src/error.rs",
      "confidence": "inferred"
    },
    {
      "a": "story:a-text-field-declares-its-alphabet",
      "b": "story:operation-writes-an-optional-field-from-an-optional-argument",
      "path": "crates/entity-core/src/validation.rs",
      "confidence": "cited"
    },
    {
      "a": "story:a-text-field-declares-its-alphabet",
      "b": "story:set-clears-an-optional-field",
      "path": "crates/entity-core/src/definition.rs",
      "confidence": "inferred"
    },
    {
      "a": "story:a-text-field-declares-its-alphabet",
      "b": "story:set-clears-an-optional-field",
      "path": "crates/entity-core/src/error.rs",
      "confidence": "inferred"
    },
    {
      "a": "story:a-text-field-declares-its-alphabet",
      "b": "story:set-clears-an-optional-field",
      "path": "crates/entity-core/src/validation.rs",
      "confidence": "cited"
    },
    {
      "a": "story:a-text-field-declares-its-alphabet",
      "b": "story:set-clears-an-optional-field",
      "path": "docs/design/kernel-v0.1.md",
      "confidence": "inferred"
    },
    {
      "a": "story:a-text-field-declares-its-alphabet",
      "b": "story:set-increments-a-numeric-field",
      "path": "crates/entity-core/src/definition.rs",
      "confidence": "cited"
    },
    {
      "a": "story:a-text-field-declares-its-alphabet",
      "b": "story:set-increments-a-numeric-field",
      "path": "crates/entity-core/src/error.rs",
      "confidence": "inferred"
    },
    {
      "a": "story:a-text-field-declares-its-alphabet",
      "b": "story:set-increments-a-numeric-field",
      "path": "crates/entity-core/src/validation.rs",
      "confidence": "cited"
    },
    {
      "a": "story:a-text-field-declares-its-alphabet",
      "b": "story:set-increments-a-numeric-field",
      "path": "docs/design/kernel-v0.1.md",
      "confidence": "inferred"
    },
    {
      "a": "story:operation-writes-an-optional-field-from-an-optional-argument",
      "b": "story:set-clears-an-optional-field",
      "path": "crates/entity-core/src/definition.rs",
      "confidence": "inferred"
    },
    {
      "a": "story:operation-writes-an-optional-field-from-an-optional-argument",
      "b": "story:set-clears-an-optional-field",
      "path": "crates/entity-core/src/error.rs",
      "confidence": "inferred"
    },
    {
      "a": "story:operation-writes-an-optional-field-from-an-optional-argument",
      "b": "story:set-clears-an-optional-field",
      "path": "crates/entity-core/src/runtime.rs",
      "confidence": "cited"
    },
    {
      "a": "story:operation-writes-an-optional-field-from-an-optional-argument",
      "b": "story:set-clears-an-optional-field",
      "path": "crates/entity-core/src/validation.rs",
      "confidence": "cited"
    },
    {
      "a": "story:operation-writes-an-optional-field-from-an-optional-argument",
      "b": "story:set-increments-a-numeric-field",
      "path": "crates/entity-core/src/definition.rs",
      "confidence": "cited"
    },
    {
      "a": "story:operation-writes-an-optional-field-from-an-optional-argument",
      "b": "story:set-increments-a-numeric-field",
      "path": "crates/entity-core/src/error.rs",
      "confidence": "inferred"
    },
    {
      "a": "story:operation-writes-an-optional-field-from-an-optional-argument",
      "b": "story:set-increments-a-numeric-field",
      "path": "crates/entity-core/src/runtime.rs",
      "confidence": "cited"
    },
    {
      "a": "story:operation-writes-an-optional-field-from-an-optional-argument",
      "b": "story:set-increments-a-numeric-field",
      "path": "crates/entity-core/src/validation.rs",
      "confidence": "cited"
    },
    {
      "a": "story:recorded-open-checkpoint-design",
      "b": "story:recorded-open-verifies-a-checkpoint-and-its-suffix",
      "path": "crates/entity-eventlog/tests/shared_clock_cost.rs",
      "confidence": "cited"
    },
    {
      "a": "story:set-clears-an-optional-field",
      "b": "story:set-increments-a-numeric-field",
      "path": "crates/entity-core/src/definition.rs",
      "confidence": "inferred"
    },
    {
      "a": "story:set-clears-an-optional-field",
      "b": "story:set-increments-a-numeric-field",
      "path": "crates/entity-core/src/error.rs",
      "confidence": "inferred"
    },
    {
      "a": "story:set-clears-an-optional-field",
      "b": "story:set-increments-a-numeric-field",
      "path": "crates/entity-core/src/lib.rs",
      "confidence": "inferred"
    },
    {
      "a": "story:set-clears-an-optional-field",
      "b": "story:set-increments-a-numeric-field",
      "path": "crates/entity-core/src/replay.rs",
      "confidence": "inferred"
    },
    {
      "a": "story:set-clears-an-optional-field",
      "b": "story:set-increments-a-numeric-field",
      "path": "crates/entity-core/src/runtime.rs",
      "confidence": "cited"
    },
    {
      "a": "story:set-clears-an-optional-field",
      "b": "story:set-increments-a-numeric-field",
      "path": "crates/entity-core/src/validation.rs",
      "confidence": "cited"
    },
    {
      "a": "story:set-clears-an-optional-field",
      "b": "story:set-increments-a-numeric-field",
      "path": "docs/design/kernel-v0.1.md",
      "confidence": "inferred"
    }
  ],
  "unassessed": [
    "story:aep-markdown-materialized-view",
    "story:definition-json-schema",
    "story:definition-migrations",
    "story:explain-verb",
    "story:gate-and-release-hardening",
    "story:named-predicates",
    "story:one-git-url-one-rev-across-the-workspace",
    "story:pedantic-lints",
    "story:provider-feature-combinations-compile-and-are-gated",
    "story:schema-fragments",
    "story:seeded-open-under-one-second"
  ],
  "cycles": []
}
```
