//! IO-free, deterministic kernel for schema-driven, lifecycle-governed entities.
//!
//! An entity type is **data**: a schema, a lifecycle, operations with their own argument schemas,
//! preconditions, invariants and the events an operation emits. The kernel has no compiled
//! knowledge of any particular entity type; it registers definitions at run time and applies one
//! rule to all of them:
//!
//! ```text
//! definition + instance + operation + arguments  ->  Decision { instance, events }
//! ```
//!
//! Nothing here reads a clock, generates an identifier, touches a filesystem, opens a socket or
//! spawns a task. Identifiers, timestamps and everything else the outside world knows are passed
//! in as arguments; the same inputs produce the same [`Decision`] every time. The caller — the
//! *shell* — persists the instance, appends the events, updates projections and publishes. The
//! kernel decides; it never acts.
//!
//! # A complete round trip
//!
//! ```
//! use entity_core::{CoreError, Registry, Runtime};
//! use serde_json::json;
//!
//! let definition = serde_json::from_value(json!({
//!     "entity": "ticket",
//!     "version": 1,
//!     "schema": { "fields": {
//!         "title": { "type": "string", "required": true },
//!         "resolution": { "type": "string" }
//!     }},
//!     "lifecycle": { "initial": "open", "states": ["open", "closed"] },
//!     "invariants": [{
//!         "name": "closed_requires_resolution",
//!         "assert": { "any": [ { "ne": ["$state", "closed"] }, { "exists": "$fields.resolution" } ] },
//!         "message": "a closed ticket states how it was resolved"
//!     }],
//!     "operations": { "close": {
//!         "arguments": { "fields": { "resolution": { "type": "string", "required": true } } },
//!         "transitions": [ { "from": "open", "to": "closed" } ],
//!         "set": { "resolution": "$args.resolution" },
//!         "emits": [ { "type": "TicketClosed", "payload": { "id": "$id", "resolution": "$fields.resolution" } } ]
//!     }}
//! }))?;
//!
//! let mut registry = Registry::new();
//! registry.register(definition)?;
//! let runtime = Runtime::new(&registry);
//!
//! let opened = runtime.create("ticket", 1, "t-1", json!({ "title": "Login fails" }))?;
//! assert_eq!(opened.instance.lifecycle_state, "open");
//! assert_eq!(opened.instance.revision, 1);
//!
//! let closed = runtime.execute(&opened.instance, "close", json!({ "resolution": "fixed" }))?;
//! assert_eq!(closed.instance.lifecycle_state, "closed");
//! assert_eq!(closed.instance.revision, 2);
//! assert_eq!(closed.events[0].event_type, "TicketClosed");
//! assert_eq!(closed.events[0].payload["resolution"], json!("fixed"));
//!
//! // A closed ticket cannot be closed again: the lifecycle refuses before anything is evaluated.
//! let refused = runtime.execute(&closed.instance, "close", json!({ "resolution": "again" }));
//! assert!(matches!(refused, Err(CoreError::InvalidTransition { .. })));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! # Where things live
//!
//! | module | holds |
//! |---|---|
//! | [`EntityDefinition`] and its parts | the definition model — what a YAML or JSON document deserialises into |
//! | [`Registry`] | validated definitions, keyed by `(entity, version)` |
//! | [`Runtime`], [`create`], [`execute`] | the kernel: the only functions that produce a [`Decision`] |
//! | [`DefinitionError`], [`ValidationError`], [`CoreError`] | every refusal, typed |
//!
//! # Evaluation order of an operation
//!
//! 1. the instance's `(entity, version)` must match the definition;
//! 2. the operation must exist;
//! 3. arguments are defaulted, then validated against the operation's argument schema;
//! 4. a transition is selected from the instance's current lifecycle state;
//! 5. preconditions are evaluated against the current state and the arguments;
//! 6. `set` assignments are resolved — all of them against the pre-operation fields;
//! 7. the resulting fields are validated against the entity schema;
//! 8. the next instance is constructed (new state, revision + 1);
//! 9. invariants are evaluated against the next state;
//! 10. events are materialised from their templates;
//! 11. the [`Decision`] is returned.
//!
//! A refusal at any step returns the typed error and nothing else: the caller's instance is never
//! touched and no partial event list escapes.

mod definition;
mod error;
mod registry;
mod runtime;
mod validation;

pub use definition::{
    Condition, CreateDefinition, EntityDefinition, EventDefinition, FieldDefinition, FieldKind,
    LifecycleDefinition, ObjectSchema, OneOrMany, OperationDefinition, RuleDefinition,
    TransitionDefinition, CONDITION_OPERATORS,
};
pub use error::{CoreError, DefinitionError, ValidationError};
pub use registry::Registry;
pub use runtime::{create, execute, Decision, DomainEvent, EntityInstance, Runtime};
