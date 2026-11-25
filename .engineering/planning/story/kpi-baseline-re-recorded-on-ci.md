---
format: aep.planning-md/3
id: story:kpi-baseline-re-recorded-on-ci
kind: story
status: draft
title: KPI baseline is re-recorded on CI after the File Store v3 change
summary: The KPI counters job lists re-record items for the File Store v3 counters and no CI workflow records the baseline
owner: entity-runtime
relations:
- informed_by: story:kpi-baseline-matches-main
scope:
- confidence: cited
  path: .github/workflows
- confidence: cited
  path: AGENTS.md
- confidence: cited
  path: checks/kpi/baseline.json
revision: 3
---
## Outcome

`checks/kpi/baseline.json` holds the counters that the File Store v3 code produces, recorded on a CI
runner, so the `KPI counters` job prints no re-record item for them and a later regression is
measured from today's values.

## Context

Found in pull request #106 (Entity Runtime 0.34.0). The `KPI counters` job passes, and its output
lists re-record items:

- `file.first_write.fsyncs` 8 -> 11 and `file.first_write_of_type.fsyncs` 4 -> 6 are changed.
- The large File Store v3 read and write byte counters improved.
- `executor.fresh_create.alloc_bytes` matches its baseline and needs no re-record.

The baseline has no path to a new value today. `.github/workflows/kpi.yml` runs only
`er-kpi check` (job `counters`) and `er-kpi ab` (job `timing`); no CI workflow runs `er-kpi record`.
Hand-editing `checks/kpi/baseline.json` is barred, and the KPI measurement is a CI job and not a
workstation one, so the re-record is stuck until a workflow records it.

Related stories: `story:kpi-baseline-matches-main` is the earlier stale-baseline case, recorded
before the baseline had any CI path; `story:kpi-timing-job-enforces-or-drops-its-limit`, drafted on
the 0.34.0 preparation branch, is the other open KPI CI gap and covers the timing job only.

## Acceptance

- A dispatch of a CI workflow runs `er-kpi record` on a runner and uploads the resulting
  `checks/kpi/baseline.json` as a workflow artifact.
- A person or the bot commits that file in a change whose diff touches only
  `checks/kpi/baseline.json`.
- After that change the `KPI counters` job prints no re-record item for `file.first_write.fsyncs`
  and `file.first_write_of_type.fsyncs`, and none for the File Store v3 read and write byte
  counters.
- `executor.fresh_create.alloc_bytes` keeps its recorded value.

## Out of Scope

- The timing A/B job and its limit (`story:kpi-timing-job-enforces-or-drops-its-limit`).
- Making any KPI job a required check; that is a ruleset change.
- Changing a counter's definition or the code the counters measure.

## Scope

- `.github/workflows/` (a new dispatch workflow that records and uploads the baseline).
- `checks/kpi/baseline.json` (the committed result).
- `AGENTS.md` (the KPI section, where it says how the baseline is recorded).
