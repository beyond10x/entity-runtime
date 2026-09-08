---
format: aep.planning-md/1
id: task:generic-cli-store-verbs-share-the-shell
kind: task
status: implemented
title: entity create/execute --store go through the shared stored runtime
summary: The generic CLI evaluated again on every invocation and could not return an accepted operation for an exact retry; it now uses StoredRuntime like MCP and the generated CLI
relations:
- decomposes: story:recorded-command-contract
- serves: vision:O2
revision: 5
---
## Outcome

`entity create --store` and `entity execute --store` run through `entity_shell::StoredRuntime`, the
layer MCP and the generated CLI already use, so the three surfaces share one exact-retry rule and
one refusal vocabulary.

## Scope

- Cited: `crates/entity-cli/src/main.rs` (`Command::Execute::expected_revision`,
  `From<ShellError> for Failure`, `Failure::StoreRefused { kind, detail }`), `crates/entity-cli/Cargo.toml`
  (`entity-shell` dependency), `crates/entity-cli/tests/cli.rs`, `website/docs/guide/{cli,storage}.md`.

## Acceptance

`an_exact_execute_retry_through_the_store_returns_the_original_record_after_state_has_advanced`,
`a_retry_with_the_same_record_id_but_different_intent_is_a_record_conflict` and
`execute_with_a_stale_expected_revision_is_refused_before_the_kernel_runs` pass; the first fails when
`execute --store` is routed back to a plain re-evaluation. Exit codes are unchanged: kernel refusal 1,
store refusal 1 with `{"refused":true,"by":"store","kind":…}`, bad recording input 2.

## Authorization

The 2026-09-08 review recorded that the generic CLI re-evaluated on every invocation and could not
recover an accepted operation by record id (`website/docs/guide/cli.md` said so); the operator asked
for every untracked follow-up to be fixed in this change set.

## Implementation evidence

Mutation: bypassing `StoredRuntime::execute` made the exact-retry test fail with exit 1 where 0 was
expected. 28 CLI tests pass. Recorded in `docs/reviews/2026-09-08-full-review.md`. No lifecycle
status claim is made by this body.
