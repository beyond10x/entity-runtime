---
format: aep.planning-md/3
id: approval-record:takt-merge-delegation-d266
kind: approval-record
status: approved
title: takt may merge on this repository with no operator step (D266, goal A)
relations:
- decides: story:entity-command-yaml-output-writes-numbers
- decides: story:provider-feature-combinations-compile-and-are-gated
revision: 3
transitions:
- {from: "draft", to: "approved", at: "2026-10-30T07:33:17Z", actor: "human:timo", revision: 3}
---
## Decision

takt may merge a pull request on this repository with no operator step: `.engineering/delegations.yaml` delegates `repository.merge.judge_surface` and `repository.merge` to `agent:takt-delivery`, the same two entries, dates and owner as the llm repository's. takt's merge check admits only the exact capability string `repository.merge`; `repository.merge.judge_surface` alone does not admit a merge. The delegation is revocable by removing an entry.

It covers the two stories takt delivers on this repository in goal A: `story:entity-command-yaml-output-writes-numbers` and `story:provider-feature-combinations-compile-and-are-gated`.

## Instruction

Session `b10x-chef-orchestrator`, decision D266 (2026-10-30): the merge grant is not open. The operator set goal A as "bot PR merged to main, no operator step" (decisions.md 2026-10-29 21:29Z and 22:52Z). Add the delegation to entity-runtime mirroring llm's: same capability, same delegate, same constraints and format.

Goal A, quoted: "bot PR merged to main, no operator step".

## Setup

takt reads `.engineering/gates.yaml`, `.engineering/assertions-project.yaml`, `.engineering/commit-identity.conf` and, with this record, `.engineering/delegations.yaml`.
