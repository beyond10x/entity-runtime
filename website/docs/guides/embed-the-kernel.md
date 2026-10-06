---
sidebar_position: 7
title: Embed the kernel
description: Call the deterministic kernel from Rust, choose a storage provider, and keep IO and trusted context in your own shell.
lede: Parse a definition, register it, and ask the kernel for decisions; the program below is a test the repository gate runs.
source: "crates/entity-runtime-docs/tests/embed_the_kernel.rs (run by cargo test), crates/entity-core, crates/entity-store, crates/entity-query"
---

# Embed the kernel

Entity Runtime is a workspace of narrow crates; use only the boundary your application needs. The
[crate list](../reference/crates.md) says what each one is. The crates are not on a registry —
depend on a release tag, and keep every Entity Runtime crate on the same tag:

```toml
[dependencies]
entity-core = { git = "https://github.com/beyond10x/entity-runtime", tag = "0.27.0" }
entity-yaml = { git = "https://github.com/beyond10x/entity-runtime", tag = "0.27.0" }
serde_json = "1"
```

## Decide in memory

This program drafts and submits the [getting-started](../getting-started.md) refund, is refused
when an agent approves it, and succeeds when a human does. It is
`crates/entity-runtime-docs/tests/embed_the_kernel.rs`, which `cargo test --workspace` runs, so it
compiles and passes on every change:

```rust title="crates/entity-runtime-docs/tests/embed_the_kernel.rs"
//! The program the "Embed the kernel" guide shows, run by the gate so the page cannot drift from
//! what the libraries do.

use entity_core::{CoreError, Registry, Runtime};
use serde_json::json;

const REFUND: &str = include_str!("../../../examples/refund.yaml");

#[test]
fn a_refund_is_drafted_submitted_refused_for_an_agent_and_approved_for_a_human() {
    // Parse, then register: registration is where a definition is validated.
    let definition = entity_yaml::from_str(REFUND).expect("the YAML parses");
    let mut registry = Registry::new();
    registry
        .register(definition)
        .expect("the definition is valid");
    registry
        .validate_all()
        .expect("every referenced type is registered");
    let runtime = Runtime::new(&registry);

    // The caller supplies the identity; the kernel invents none.
    let drafted = runtime
        .create(
            "refund",
            1,
            "refund-104",
            json!({ "order_id": "order-88", "amount_cents": 12_500, "evidence_count": 2 }),
        )
        .expect("creation is accepted");
    let submitted = runtime
        .execute(&drafted.instance, "submit", json!({}))
        .expect("submit is legal from draft");
    assert_eq!(submitted.instance.revision, 2);

    // An agent may propose approval; the rule refuses it, and nothing changes.
    let refused = runtime.execute(
        &submitted.instance,
        "approve",
        json!({ "actor_role": "agent", "reason": "customer supplied delivery evidence" }),
    );
    match refused {
        Err(CoreError::PreconditionFailed { rule, .. }) => {
            assert_eq!(rule.as_deref(), Some("large_refunds_need_a_human"));
        }
        other => panic!("expected a policy refusal, got {other:?}"),
    }
    assert_eq!(submitted.instance.lifecycle_state, "submitted");

    // The trusted shell, not the model, says a human approved.
    let approved = runtime
        .execute(
            &submitted.instance,
            "approve",
            json!({ "actor_role": "human", "reason": "supervisor verified the evidence" }),
        )
        .expect("a human may approve");
    assert_eq!(approved.instance.lifecycle_state, "approved");
    assert_eq!(approved.instance.revision, 3);
    assert_eq!(approved.events[0].event_type, "RefundApproved");
}
```

`Registry::register` is the validation boundary: execution only ever receives a
`ValidatedDefinition`. `validate_all` checks the references between registered types as a set. On
a refusal the caller still owns the unchanged instance. Match `CoreError` variants in code; the
`Display` text is for people and may be reworded.

## Build a trusted shell

Your shell owns every ambient and privileged fact:

<img
  src="/entity-runtime/img/trusted-shell-flow.svg"
  alt="The trusted shell loads canonical data, authenticates the actor, reads trusted time, and assigns provenance. Entity Runtime returns a decision or typed refusal. Refusals write nothing. Decisions are recorded, committed with a revision expectation, and only then lead to external side effects."
  loading="lazy"
/>

`EntityInstance` has public, serializable fields because providers round-trip it. That is not
permission to trust any deserialized instance: load canonical instances from a trusted provider and
let the kernel check the definition identity and the declared state.

`entity_shell::StoredRuntime` is that sequence for a synchronous store: it loads the subject,
recognizes an exact retry, decides, and commits a `RecordedCommit` at the expected revision.

## Store atomically

`RecordedCommit::new` binds a decision to a `Recording`; `Store::commit_recorded` checks
`Expect::Absent` or `Expect::Revision(n)` before writing state, history and events. For an ordered
multi-subject command, `AtomicBatchStore::commit_batch` on the Memory, SQLite or PostgreSQL store
lets every expectation see the earlier entries of the same batch, and rolls the whole batch back on
any conflict. [Storage and replay](../concepts/storage.md) compares every provider, including the
asynchronous `entity-executor` and the Eventlog-backed stores.

## Replay

`entity_core::replay` re-runs a complete decision record and compares the normalized input,
definition, result, changes and events. `rehydrate` folds a legacy event-only history against the
current definition and refuses any revision no operation could have produced; it is a migration
tool, not equivalent proof.

## Query documents

`entity_query::DocumentQueryProvider` is implemented by the Memory and PostgreSQL stores and the
Eventlog facades. A `DocumentQuery` selects one entity type, applies recursive JSON containment to
its fields and returns an identity-ordered page. The default limit is 100 and the maximum 1,000. A
cursor is bound to the query that issued it; one from another query is refused. Paging does not
hold a snapshot across calls.

`PostgresStore` also offers command sessions for transactional reads, writes, queries, events and
identity-range reservation. The `entity` command's `list` only enumerates stored ids.
