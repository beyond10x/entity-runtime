---
format: aep.planning-md/3
id: review-result:issue-51-continuity-design
kind: review-result
status: active
title: Independent review of the refined capture continuity design
relations:
- reviews: story:bounded-batch-and-facade-reads
revision: 1
---
unit: GitHub issue #51 refined continuity design; ER working tree f57bf975 plus proposed Eventlog capture extension
verdict: nothing found; implementation safety unproven in this design-only pass
cases: executed 0→0, red 0
origin: introduced 0 / pre-existing 0 / undecided 0
wrote-outside-worktree: none
needs-coordinator: preserve the refinements below in normative prose and verify their executable controls

1. git --no-pager diff --stat

```text
 crates/entity-eventlog/src/adapter.rs        | 42 +++++++++++++++++-
 crates/entity-eventlog/src/adapter/scoped.rs | 15 +++++--
 crates/entity-eventlog/src/facade.rs         | 62 +++++++++++++++++++++++++-
 crates/entity-eventlog/src/lib.rs            |  4 +-
 crates/entity-eventlog/src/sync.rs           | 65 ++++++++++++++++++++++++----
 5 files changed, 172 insertions(+), 16 deletions(-)
```

This is the implementor's in-progress shared-tree diff, not this review's changes. The reviewer wrote only this ignored report and managed its own worktree lease. No source, test, specification or planning file was edited. The previously authored performance test is outside this read-only subtask.

2. Cases added

None, as explicitly assigned. No test or build was executed. This is a bounded design pass, not verification of unfinished implementation.

3. Scope and authority

Read Eventlog's proposed docs/design/consistent-tenant-capture.md:263-464 and additive crates/eventlog-core/src/capture.rs checkpoint/update types and default trait method. Read ER target/issue-51-scratch/runtime-design.md:3-17. Apply the coordinator's additional issuance restriction: SQLite returns no checkpoint when cold schema verification sees SQL triggers or foreign-key cascades; complete fallback remains available. The operator accepts SQL-visible change tracking, mandatory full verification on open/reopen, and exclusion of raw database/WAL file modification outside SQLite from warm continuity guarantees.

4. Findings

No unresolved design counterexample found under those refined assumptions. Implementation correctness is unproven: the safety fact reached source/document reasoning, not an executed conformance case in this pass. This is not approval and supplies no signed verification evidence.

Two clarifications were relayed while the implementation is still being written:

- ER runtime-design.md:11 must require both directions of materialization agreement. Checking only rows returned in a delta would miss an omitted required record/batch/subject row. The runtime author confirmed the intended design computes every expected changed row, checks all supplied before/after values, and checks that every expected final row exists. Unrelated before==after rows may be retained as harmless accounting; unrelated new values may not enter the verified cache. The author reports the implementation follows this rule, which this design pass has not executed.
- Eventlog consistent-tenant-capture.md:372-405 predates the latest no-trigger/no-cascade refinement. The provider author was asked to state the issuance restriction and cold re-establishment requirement explicitly. The report evaluates the refined contract supplied by the coordinator; it does not treat unfinished prose as a finished production defect.

5. Defensive reasoning and required executable controls

Opaque authority: capture.rs:47-59 permits extension providers through Any, while the SQLite payload must remain private and carry the exact Arc issuer identity. Cloning the issued capability preserves authority; arbitrary new payloads, foreign connections, changed tenant/specification/limit tuples and reopened issuers cannot satisfy that private payload/scope check. A real foreign-token/scope test remains required; public generic construction alone is not a forgery.

Cumulative limits: design:343-345 and :424-429 bind exact limits and complete usage, requiring checked additions and before/after row-size adjustments. Limits over the delta alone would admit a complete observation beyond the caller's cap. Controls must independently exceed each whole-capture cap after individually small appends and include orphan blobs, deletion/reinsertion, present null and returned-to-base rows.

Projection completeness: design:394-396 and :431-436 require every provider-controlled mutation's original/final values, then consumer validation. The clarified exact expected-row check is necessary to reject omissions, not merely unexpected additions. Controls must omit one record, batch and subject delta in turn and compare tracked versus full refusal.

Concurrent stamp windows: design:377-405 obtains the external stamp inside BEGIN IMMEDIATE and retains that pre-commit external stamp when publishing journal state under the connection mutex. A stamp freshly read after COMMIT could absorb a rival SQL commit and hide it. A deterministic post-COMMIT/pre-publication interleaving must demonstrate full fallback; wall-clock concurrency loops alone do not prove that window was reached.

Unaccounted writes: design:391-412 restricts continuity to acknowledged instrumented groups and invalidates after failed/uncertain operations or other provider paths. Repeated blob bindings must not increase usage; actual new orphan bindings must. Same-connection total changes, external data-version changes and schema changes have distinct jobs. The no-trigger/no-cascade issuance restriction prevents indirect SQLite writes from escaping interface-level mutation collection. SQL event/blob/row rewrites preserving head/count remain mandatory refusal controls after fallback.

Consumer prefix proof: runtime-design.md:9-13 retains the verified base, generation and token as one cache authority. Only proven unchanged prefixes admit linear suffix replay. Full new batch membership, exact positions, fresh global identities, predecessor replay and exact projection updates are necessary before the token advances. Failure must discard the tentative advance; a racing generation must retry. Imports, lineage and unsupported changes must go through complete verification.

Existing limitations distinguished from new claims: design:439-443 expressly does not promise an append precondition covering all history between a verified read and a later write. A rival corruption may therefore race an acknowledged write, but cannot yield silently verified cached success afterward. Complete snapshot/history outputs necessarily clone returned data; the flat-cost claim must isolate fixed-size batches and unchanged verification from output size. Raw file bypass is the accepted warm-policy exclusion, not a newly discovered flaw. Default FullVerification callers retain their existing guarantee.

6. Outside-worktree writes

None. The only review artifact is target/issue-51-scratch/design-review/report.md. Worktree lifecycle metadata is maintained by the mandated CLI. No build, external integration, planning write or commit occurred.

```findings
[]
```
