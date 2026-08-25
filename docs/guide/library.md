# The library

`entity-core` is the kernel as a Rust crate. It has two dependencies, `serde` and `serde_json`,
and performs no IO of any kind — a source scan in its own test suite keeps it that way. Everything
that reads a clock, mints an identifier or touches a store is the caller's, and the caller is what
this page calls the **shell**.

```toml
[dependencies]
entity-core = { git = "https://github.com/beyond10x/entity-runtime", package = "entity-core" }
entity-yaml = { git = "https://github.com/beyond10x/entity-runtime", package = "entity-yaml" }  # optional
```

## Types

| type | is |
|---|---|
| `EntityDefinition` | the definition model; deserialises from YAML/JSON; validated on registration |
| `Registry` | validated definitions keyed by `(entity, version)` |
| `Runtime<'_>` | the kernel over a `Registry`; `create` and `execute` |
| `EntityInstance` | `{ entity, version, id, lifecycle_state, revision, fields }`; `fields` is a name-ordered `serde_json::Map` |
| `DomainEvent` | `{ entity, version, id, revision, type, payload }` — the fact, no envelope |
| `Decision` | `{ instance, events }` — the only thing the kernel produces |
| `DefinitionError` | the definition is malformed (at registration) |
| `ValidationError` | one value failed its schema; always in a `Vec` |
| `CoreError` | every run-time refusal, one variant each |

The free functions `create(&definition, id, fields)` and `execute(&definition, &instance, op,
args)` do the same as the `Runtime` methods without a registry lookup.

## A round trip

```rust
use entity_core::{CoreError, Registry, Runtime};
use serde_json::json;

let definition = entity_yaml::from_str(&std::fs::read_to_string("ticket.yaml")?)?; // IO: yours
let mut registry = Registry::new();
registry.register(definition)?;                    // DefinitionError if malformed or already registered
let runtime = Runtime::new(&registry);

let opened = runtime.create("ticket", 1, "t-1", json!({ "title": "Login fails", "points": 3 }))?;
let started = runtime.execute(&opened.instance, "start", json!({ "assignee": "alice" }))?;
let closed = runtime.execute(&started.instance, "close", json!({ "resolution": "fixed" }))?;

assert_eq!(closed.instance.lifecycle_state, "closed");
assert_eq!(closed.instance.revision, 3);
assert_eq!(closed.events[0].event_type, "TicketClosed");

match runtime.execute(&closed.instance, "close", json!({ "resolution": "again" })) {
    Err(CoreError::InvalidTransition { operation, state }) => {
        assert_eq!((operation.as_str(), state.as_str()), ("close", "closed"));
    }
    other => panic!("expected a lifecycle refusal, got {other:?}"),
}
```

`opened.instance` is untouched by the later calls: every entry point takes the instance by
shared reference and returns a new one. Match refusals on the variant, never on the message.

## Writing a shell

The kernel decides; the shell acts. The shape every shell has:

```text
load definition(s) ─┐
load instance ──────┤                                        ┌─ store decision.instance
gather ids/time ────┴─▶ Runtime::execute(&instance, op, args) ─┼─ append decision.events (+ envelope)
                          │                                   ├─ update projections / search
                          └─ Err(refusal) ─▶ record it; change nothing   └─ publish
```

Four rules the kernel cannot enforce for you:

1. **Hand it an instance it produced.** `EntityInstance` is data your store round-trips, so its
   fields are public and it deserialises; the kernel checks the type, the version and that the
   state is one the definition declares, and cannot check more than that. Loading an instance from
   somewhere you trust is the shell's job — which is the same reason storing the instance and its
   events is one job.

2. **Store the instance and the events together.** A transaction, an outbox, or an event store
   that is also the state store — how is yours; that they land together is the contract.
3. **Compare `revision` before you store.** The kernel numbers revisions; an optimistic-concurrency
   check (`WHERE revision = expected`) is one line in the shell and catches two shells racing.
4. **Add the envelope at the edge.** `DomainEvent` carries the fact only. Event id, recorded-at
   time, correlation, causation and actor are yours to stamp when you record it, because the kernel
   could only have invented them.

Time and identity enter as arguments. An operation that needs `occurred_at` declares it in its
`arguments` schema; the shell reads the clock and passes the value.

## Determinism, as a test

Because nothing in the kernel is ambient, a decision is reproducible:

```rust
let a = runtime.execute(&instance, "close", json!({ "resolution": "fixed" }))?;
let b = runtime.execute(&instance, "close", json!({ "resolution": "fixed" }))?;
assert_eq!(a, b);
assert_eq!(serde_json::to_string(&a)?, serde_json::to_string(&b)?); // same bytes, too
```

Ordered maps throughout — fields are a `serde_json::Map` without `preserve_order`, and `HashMap` is
a banned word in the kernel's sources — make the serialised form stable, which is what lets a shell
store a Decision and diff it later.

## Definitions from JSON

`EntityDefinition` derives `Deserialize`, so a definition may come from anywhere serde can read:

```rust
let definition: entity_core::EntityDefinition = serde_json::from_value(json!({
    "entity": "light",
    "lifecycle": { "initial": "off", "states": ["off", "on"] },
    "schema": {},
    "operations": {
        "switch_on":  { "transitions": [ { "from": "off", "to": "on" } ] },
        "switch_off": { "transitions": [ { "from": "on",  "to": "off" } ] }
    }
}))?;
```

`entity-yaml` exists only so the kernel never links a YAML parser; it is `from_str(&str)` and
nothing more.

## What is not here yet

Typed references between entities, replay from an event history, a provider SPI with an in-memory
store, an event envelope type, `explain` (per-rule verdicts without executing), and three-valued
rules. Each is a story in the repository's planning store; the
[kernel design § 11](../design/kernel-v0.1.md#11-crates) says where each will live.
