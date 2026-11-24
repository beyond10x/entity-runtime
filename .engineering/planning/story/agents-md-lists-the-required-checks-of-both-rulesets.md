---
format: aep.planning-md/3
id: story:agents-md-lists-the-required-checks-of-both-rulesets
kind: story
status: draft
title: AGENTS.md lists the five checks the two rulesets require before a merge
summary: AGENTS.md names ruleset 21404415 and three checks; ruleset 23511348 also requires Security and privacy and planning validate
owner: entity-runtime
revision: 1
---
## Outcome

`AGENTS.md` lists every check that is required before a merge to `main`, from every ruleset that
applies, and says where each ruleset lives.

## Why

"Branch protection" in `AGENTS.md` names one ruleset and three checks. A second ruleset also
requires two, so a contributor reading the file expects a merge to need fewer checks than it does,
and a renamed check in the second ruleset would not be found by the instruction "if a check name
changes, change it here too".

## Evidence

Read with `gh api repos/beyond10x/entity-runtime/rules/branches/main` on 2026-11-24:

- ruleset 21404415 (`main: checks before merge`): `gate / Gate`, `gate / MSRV 1.85`,
  `Build Docusaurus`. This is the whole list `AGENTS.md` names ("Branch protection", line 310
  onward).
- ruleset 23511348: `common / Security and privacy`, `planning validate`. `AGENTS.md` does not
  name it.
- Pull request 105 shows all five checks, each `SUCCESS`.
- `AGENTS.md` also says `KPI / KPI counters` "is meant to become a required check (not yet in the
  ruleset)"; no ruleset names it yet.

## Acceptance

- The "Branch protection" section names the two rulesets by id and the five required checks
  by their exact context strings.
- A script or task compares that list with `gh api repos/beyond10x/entity-runtime/rules/branches/main`
  and fails on a difference, or the section says the comparison is by hand and names the command.
- No ruleset is changed by this story: the rulesets are the owner's.

## Scope

- `AGENTS.md` ("Branch protection", and the KPI paragraph that says "not yet in the ruleset").
- `Taskfile.yml` or `crates/entity-xtask` if the comparison is a task.
