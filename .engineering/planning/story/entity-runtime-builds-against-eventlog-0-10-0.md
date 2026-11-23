---
format: aep.planning-md/3
id: story:entity-runtime-builds-against-eventlog-0-10-0
kind: story
status: implemented
title: Entity Runtime builds against Eventlog 0.10.0
summary: 'After the Eventlog 0.10.0 release: bump, reason codes in fixtures (S8), Projector::fingerprint (L35), P1 docs, re-measure P2 blob opens'
owner: entity-runtime
relations:
- depends_on: story:kpi-baseline-matches-main
- serves: vision:O2
revision: 4
transitions:
- {from: "draft", to: "proposed", at: "2026-11-23T14:35:01Z", actor: "human:timo", revision: 2, decided_on: {"recorded":{"test_result":1}}}
- {from: "proposed", to: "active", at: "2026-11-23T14:35:01Z", actor: "human:timo", revision: 3, decided_on: {"recorded":{"test_result":1}}}
- {from: "active", to: "implemented", at: "2026-11-23T14:35:02Z", actor: "human:timo", revision: 4, decided_on: {"recorded":{"test_result":1}}}
---
## Outcome

Entity Runtime builds against Eventlog 0.10.0 and takes up what that release changes for it:
redaction reasons in its fixtures are codes, its inline projector supplies a fingerprint, its
documentation says PostgreSQL commits a guarded group with its blobs, and the blob opens of an
empty File-backed list are measured again.

## Findings

Consumer follow-ups after the Eventlog release (master log `assignment.md`: "Consumer follow-ups
after eventlog release" and line 86):

- S8 (Eventlog: "can persist personal information in a redaction reason"); Eventlog `7311d67e`
  makes `validate_redaction_reason` an identity grammar and providers refuse free text (S8 = C,
  code).
- L35 (Eventlog: "A warm Tree restart can serve rows from an earlier projector configuration");
  Eventlog `3daa19de` adds `Projector::fingerprint`; "projectors without fingerprint never
  warm-open; entity-runtime should implement Projector::fingerprint after eventlog release".
- P1 (Entity Runtime: "uploads blobs in separate PostgreSQL transactions"); Eventlog el-d
  implements the guarded blob group on PostgreSQL (P1 = A); this repository's docs still say
  PostgreSQL lacks it.
- P2 (Entity Runtime: "Eventlog list builds histories that cannot contribute an ID"); `2fb64ee8`
  fixed the adapter side, and "P2 blob opens need eventlog P8 (el-c done) -> re-measure after
  eventlog bump" (line 86).

## Evidence

- Pin today: `eventlog-core = { git = ..., tag = "0.9.4" }`, `crates/entity-eventlog/Cargo.toml:27`.
- S8: free-text reason `"controlled projection dirtying"` at
  `crates/entity-eventlog/tests/providers.rs:684-688`, the only `.redact(` call with a text reason
  under `crates/`.
- L35: `impl Projector for ErRecordedProjector` at `crates/entity-eventlog/src/projection.rs:102`
  supplies no fingerprint.
- P1: `docs/design/eventlog-recorded-errors-import-v0.1.md:385-402` ("On a provider that takes the
  port's default — `eventlog-postgres` — ... the blobs are uploaded by this adapter's own fallback
  loop"); adapter comments at `crates/entity-eventlog/src/adapter.rs:2888` and `:4058`.
- P2: review measured 771 blob opens for an empty list with 64 unrelated tickets and 99 with 8
  (`review.txt` P2); KPI counters `eventlog_file.list_empty.blob_read_opens.t64` / `.t8` read 606 /
  81 on `feat/record-definition-digest` (`er-j/r7-kpi.log`; 603 / 78 before #94's definition blob).

## Acceptance

With the Eventlog pin at 0.10.0, `entity-eventlog` tests pass with only reason codes in fixtures, a
Tree warm reopen with `ErRecordedProjector` is served from cache (a test shows no replay) and one
with a changed projector configuration replays, the P1 passages name PostgreSQL among the providers
that commit a guarded group with its blobs, and `task kpi-record` records the new
`eventlog_file.list_empty.blob_read_opens` values in the same change.
