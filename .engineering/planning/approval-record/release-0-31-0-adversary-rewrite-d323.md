---
format: aep.planning-md/3
id: approval-record:release-0-31-0-adversary-rewrite-d323
kind: approval-record
status: approved
title: The rewritten creation set_if_present adversary test in 0.31.0 is accepted as reviewed (D323)
relations:
- decides: task:release-0-31-0
revision: 3
transitions:
- {from: "draft", to: "approved", at: "2026-10-31T08:37:52Z", actor: "human:timo", revision: 3}
---
## Decision

Release 0.31.0 stands without a 0.31.1. `verify-release.sh` fails its adversary-test check on `a_parent_default_makes_a_creation_set_if_present_leaf_present_for_a_caller_who_sent_none_until_creation_set_if_present_ignores_parent_defaults` in `crates/entity-core/tests/adversary_operation_set_if_present.rs`, because the rewriting hunk names no story id on added or context lines. The orchestrator read `git diff 0.30.3 0.31.0` for that file and accepts the rewrite as reviewed.

The old test asserted the defect "until story:creation-set-if-present-ignores-parent-defaults lands". PR 85 landed that story, and the same case now asserts the stricter refusal as `a_parent_default_cannot_make_a_creation_set_if_present_leaf_present_for_a_caller_who_sent_none`.

## Instruction

Session `b10x-chef-orchestrator`, decision D323 (2026-10-31), option (a): accept the check failure as reviewed, no 0.31.1; the release stands.
