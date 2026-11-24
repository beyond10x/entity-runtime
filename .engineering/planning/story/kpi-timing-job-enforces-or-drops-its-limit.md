---
format: aep.planning-md/3
id: story:kpi-timing-job-enforces-or-drops-its-limit
kind: story
status: draft
title: The KPI timing job fails a slowdown past a CI-measured limit, or claims no limit
summary: KPI timing runs with continue-on-error and a 2.0x limit set from workstation noise, so its verdict turns nothing red
owner: entity-runtime
relations:
- informed_by: story:kpi-baseline-matches-main
revision: 1
---
## Outcome

A pull request that slows a gated KPI past its limit fails a CI check, or the repository states in
one place that the timing A/B is informational and carries no limit. Today it does neither: the
job measures, applies a limit and then discards the verdict.

## Why

`KPI / KPI timing` runs `er-kpi ab`, which fails above each gated KPI's `max_ratio`, but the job has
`continue-on-error: true` and is not a required check, so a failed ratio turns nothing red. The
limit is 2.0 on every gated KPI, so it can only catch a doubling; it was set because an A/A run on a
shared 20-CPU workstation moved by up to 1.94x. A CI runner's own noise has not been measured.

## Evidence

- `.github/workflows/kpi.yml:119-123` (job `timing`, `continue-on-error: true`) and `:175-200`
  (`er-kpi ab --base "$BASE" --head HEAD --rounds 3`).
- `checks/kpi/kpis.yaml` carries `max_ratio: 2.0` on all 13 gated KPIs (from line 296).
- `AGENTS.md` (KPI section): "it is informational (`continue-on-error`) and never a required
  check", and "2.0: an A/A run ... moved by up to 1.94x".
- Per the host rule, KPI timing runs only as a CI job; no number from the workstation can set a
  limit.

## Acceptance

- An A/A run of the timing job on CI (the same commit on both sides), repeated at least 10 times,
  is recorded with its largest per-KPI ratio.
- `max_ratio` per KPI is set from that measurement, and the job either fails on a ratio above it
  (with `continue-on-error` removed) or is documented as informational with its limit removed.
- A planted slowdown of one gated KPI turns the job red on CI.
- Making the job a required check is a ruleset change and stays with Timo.

## Scope

- `.github/workflows/kpi.yml` (the `timing` job).
- `checks/kpi/kpis.yaml` (`max_ratio`).
- `AGENTS.md` (the KPI section).
