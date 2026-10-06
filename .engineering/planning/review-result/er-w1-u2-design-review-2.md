---
format: aep.planning-md/3
id: review-result:er-w1-u2-design-review-2
kind: review-result
status: active
title: 'U2 bounded-open design: independent review 2 (machine paths redacted)'
relations:
- reviews: story:recorded-open-checkpoint-design
revision: 1
---
needs-revision

All seven fixes from pass 1 hold. The parts added in this revision bring two new warnings and four notes. Reviewed: worktree er-w1-u2 (base b7362882, uncommitted changes), the design document and `~/.cache/er-w/u2/eventlog-issue.md`. The issue text is the same as the design's lines 484–605.

docs/design/recorded-open-checkpoint-v0.1.md:389 — The new checkpoint variants are said to use "commands the specification already declares", with "`Reopen` (the drain writes a checkpoint at the head)". After fix #1 that is no longer true. No checkpoint is written until the enable call runs (:471-474, :545-547). `ess/provider-tracking/domains/operations.yaml:4-86` declares no enable command, no discard command and no fixture that truncates the tail. As written, each variant falls back to today's complete reopen and passes without ever touching a checkpoint. The design has to name the new spec command or commands, since the specification is written before the code. — Read in the spec and the harness: `Provision` calls `RecordedProviderFacade::provision`, `checks/ess-conformance/src/provider.rs:140`.

docs/design/recorded-open-checkpoint-v0.1.md:548 — `enable_durable_continuity` puts triggers only on the tables that exist when it runs (:465-468). A projection table created later gets no trigger, for example by `create_projections` or by a future index table. A checkpoint taken after that table exists binds the new `schema_version`. A foreign SQL edit to the table then moves neither the epoch nor the token, so a restore answers `Unchanged` and bounded reads serve rows nobody verified. The issue should require two things: a restored checkpoint continues only if every requested projection table carries the provider's trigger, and `create_projections` on an enabled store installs the trigger or refuses. — Read from code: `create_projections` at `sync.rs:569-572`; the projection check at `capture.rs:639-647` only counts triggers.

docs/design/recorded-open-checkpoint-v0.1.md:214 — The fourth drain condition compares against "the persisted record's" position without requiring that record to be valid. Suppose a damaged record fails its digest but its position still decodes, and that position is above the head. Every drain is then blocked, condition 3 never takes effect, and every open pays a complete verification until someone runs the discard call. The comparison should apply only to a record whose digest and authority hold.

docs/design/recorded-open-checkpoint-v0.1.md:264 — `discard_open_checkpoint` "removes the checkpoint record", but the snapshot interface has no delete. eventlog-core `lib.rs:901-942` offers only save, generation, checked save and load; only `forget_tenant` removes snapshots. So discard has to overwrite the record with a tombstone, and conditions 3 and 4 must treat that tombstone as absent. A second problem: a handle that was open before the discard, and has not read since, writes its earlier record again when it drains. That brings the refusal back. The discard procedure should require that no `ProviderTracked` handle is live.

docs/design/recorded-open-checkpoint-v0.1.md:460 — `AGENTS.md:222-236` is cited as the list of "the consumers' pins" for consumers that open Eventlog SQLite files. That list (aep, aep-service, atlas, bench) contains no consumer of `entity-eventlog`. It also leaves out connectors, the consumer that raised #55 and connectors#101.

docs/design/recorded-open-checkpoint-v0.1.md:254 — The text now says that for "every row but one", a forged checkpoint can cost no more than today's open. That is not complete. A record with a recomputed digest whose provider bytes understate usage still restores, because epoch, token, instance, position and schema version can all be read from the file. `checkpoint_usage` then gives `admit_growth` totals that are too low, so the handle writes past its read bounds, which R-151 says it never does (`docs/requirements.md:224`). Only a deliberate attacker who knows the provider's encoding reaches this, and the same attacker could exceed the bounds directly.

**Fixes from pass 1 that hold** (citations checked against Eventlog `6983cc25` and `main`):

| # | What I checked | Result |
|---|---|---|
| 1 | blob check `lib.rs:953-963`, reached through `:318` and `:395`; same check on `main` at `lib.rs:950-961`; `inline_admin.rs:106`; `capture.rs:639-647`; `inspection.rs:351-360`; `capture.rs:149-159` | all correct; extra tables do not trip `refuse_foreign_tables` (`lib.rs:414-437`, which checks only the events table) |
| 2 | stamp read at `capture.rs:135-137`; acceptance at :598-599; "in the group's transaction" at :555 | present |
| 3 | conditions at :212-214; the race is stated; `lib.rs:2062-2066` | correct |
| 4 | claim corrected; the recovery is chosen and the reason is given | holds |
| 5 | token and the acceptance case at :597 | present |
| 6 | one copy of the totals; open falls back when `checkpoint_usage` returns `None` | holds |
| 7 | plain `append` is not tracked (`lib.rs:1150-1174`; the tracking hooks are only in `atomic_group.rs:116/169/217`) | confirmed, and confirmed by a run: `a_recorded_refusal_ends_tracked_continuity_where_an_own_write_is_advanced` (green). After the handle's own committed write the next read takes no capture; after a recorded refusal it takes one more capture and one more model build. |

**Other checks that held:** a disable followed by enable cannot carry an old checkpoint across, because both change `schema_version` and enable creates a new instance. The drain's own snapshot write does not touch any table the triggers cover. The five existing tamper scenarios still pass unchanged.

**What I changed:** `crates/entity-eventlog/tests/review_open_checkpoint.rs` grew from 354 to 439 lines, with one new case and the `touch` helper it uses. It is the only file I touched in the worktree. Formatting and clippy `-D warnings` are clean, and all 5 cases pass with `--include-ignored` (exit 0). `shared_clock_cost.rs` still hashes `326e4850…`.

**Written outside the worktree:**
- `~/.cache/er-w/u2/review/t5.log`
- `~/.cache/er-w/u2/review/clippy-2.log`
- `~/.cache/er-w/u2/review/all-2.log`
- debug test binaries in `~/.cache/b10x-target/er-w1-u2/debug/`

```findings
- file: docs/design/recorded-open-checkpoint-v0.1.md
  line: 389
  category: contract-drift
  severity: warning
  verdict: NEEDS-CHANGE
  origin: introduced
  message: "The checkpoint variants claim to use commands the spec already declares, but no checkpoint is written until the enable call runs and the tracking domain declares no enable, discard or truncation command, so the variants pass without exercising a checkpoint."
- file: docs/design/recorded-open-checkpoint-v0.1.md
  line: 548
  category: judgement
  severity: warning
  verdict: NEEDS-CHANGE
  origin: introduced
  message: "A projection table created after enable_durable_continuity has no trigger, and nothing makes a restore return Complete in that case, so a later checkpoint answers Unchanged over edits to that table that were never recorded."
- file: docs/design/recorded-open-checkpoint-v0.1.md
  line: 214
  category: boundary
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: "Drain condition 4 compares against the persisted record's position even when that record failed its digest, so a damaged record above the head is never replaced and every open stays complete."
- file: docs/design/recorded-open-checkpoint-v0.1.md
  line: 264
  category: judgement
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: "discard_open_checkpoint cannot remove the record because the snapshot interface has no delete, so it must overwrite with a tombstone, and a handle opened before the discard writes its earlier record again at drain."
- file: docs/design/recorded-open-checkpoint-v0.1.md
  line: 460
  category: contract-drift
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: "AGENTS.md:222-236 lists no entity-eventlog consumer and omits connectors, so it does not identify who opens Eventlog SQLite files."
- file: docs/design/recorded-open-checkpoint-v0.1.md
  line: 254
  category: judgement
  severity: note
  verdict: INFEASIBLE
  origin: introduced
  message: "A record with a recomputed digest whose provider bytes understate usage restores normally, and checkpoint_usage then lets admit_growth write past the read bounds, so a forged checkpoint can do more than cost a complete open."
```
