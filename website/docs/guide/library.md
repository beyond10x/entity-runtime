---
sidebar_position: 10
title: Rust libraries
description: Embed the deterministic kernel, choose storage providers, and keep IO and trusted context in your shell.
---

# Rust libraries

Entity Runtime is a workspace of narrow crates. Use only the boundary your application needs.

| Crate | Purpose |
|---|---|
| `entity-core` | IO-free definitions, registry, runtime, decisions, events, and replay |
| `entity-yaml` | YAML text to validated definition data; no file IO |
| `entity-store` | provider traits, memory/file stores, envelopes, projections, conformance suite |
| `entity-sqlite` | transactional embedded provider |
| `entity-postgres` | transactional centralized provider |
| `entity-remote` | versioned store protocol, transport trait, and hybrid policy |
| `entity-graph` | deterministic lifecycle and reference renderings |
| `entity-surface` | IO-free JSON Schema, OpenAPI, AsyncAPI, and entity documentation projection |
| `entity-shell` | provider-backed create/get/list/events/execute shared by shells |
| `entity-mcp` | synchronous schema-derived MCP tools over caller-provided stdio |

The crates are currently consumed from the tagged repository:

```toml
[dependencies]
entity-core = { git = "https://github.com/beyond10x/entity-runtime", tag = "0.16.0" }
entity-yaml = { git = "https://github.com/beyond10x/entity-runtime", tag = "0.16.0" }
```

## Decide in memory

```rust
use entity_core::{CoreError, Registry, Runtime};
use serde_json::json;

let definition = entity_yaml::from_str(&yaml)?;
let mut registry = Registry::new();
registry.register(definition)?;
registry.validate_all()?;

let runtime = Runtime::new(&registry);
let drafted = runtime.create(
    "refund",
    1,
    "refund-104",
    json!({
        "order_id": "order-88",
        "amount_cents": 12_500,
        "evidence_count": 2
    }),
)?;
let submitted = runtime.execute(&drafted.instance, "submit", json!({}))?;

match runtime.execute(
    &submitted.instance,
    "approve",
    json!({
        "actor_role": "agent",
        "reason": "customer supplied delivery evidence"
    }),
) {
    Err(CoreError::PreconditionFailed { rule, .. }) => {
        assert_eq!(rule.as_deref(), Some("large_refunds_need_a_human"));
    }
    other => panic!("expected a policy refusal, got {other:?}"),
}
```

The caller-owned `submitted.instance` remains unchanged after the refusal.

## Build a trusted shell

Your shell owns all ambient and privileged facts:

<img
  src="/entity-runtime/img/trusted-shell-flow.svg"
  alt="The trusted shell loads canonical data, authenticates the actor, reads trusted time, and assigns provenance. Entity Runtime returns a decision or typed refusal. Refusals write nothing. Decisions are recorded, committed with a revision expectation, and only then lead to external side effects."
  loading="lazy"
/>

`EntityInstance` is serializable data with public fields because providers round-trip it. That is
not permission to trust any deserialized instance. Load canonical instances from a trusted provider
and let the kernel check definition identity and declared state.

## Store atomically

Use `RecordedCommit::new` to bind a decision to `Recording`, then call
`Store::commit_recorded`. The provider checks `Expect::Absent` or `Expect::Revision(n)` before
writing state, history, and events.

For an ordered multi-subject command, use `AtomicBatchStore::commit_batch` on memory, SQLite, or
PostgreSQL providers. All expectations see earlier entries in the same batch; any conflict or
provider failure rolls the batch back.

## Replay

`entity_core::replay` reruns complete decision records and compares normalized input, definition,
result, changes, and events. `rehydrate` folds legacy event-only histories and is a migration tool,
not equivalent proof.

Match `CoreError` and `StoreError` variants in code. Display strings are for people and may be
reworded.
