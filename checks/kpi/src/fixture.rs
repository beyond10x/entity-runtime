//! Definitions, recordings and contexts every scenario shares. Fixed values only: a counter must
//! read the same on every run, so nothing here takes a clock or a random source.

use std::num::NonZeroU16;

use entity_core::{EntityDefinition, Registry};
use entity_eventlog::{sync::BridgeConfig, EventlogOperationContext};
use entity_store::{asynchronous::Subject, Recording};
use eventlog_core::CaptureLimits;
use time::OffsetDateTime;

/// The shipped counter: create, `view` (one field change, one event).
pub const COUNTER: &str = include_str!("../../../examples/counter.yaml");
/// The review's P2 ticket: one required string, no operations.
pub const TICKET: &str = include_str!("../fixtures/ticket.yaml");
/// The review's E10 300-state chain.
pub const GRAPH_300: &str = include_str!("../fixtures/graph-300.yaml");
/// The review's E03 service/1 parcel with an event on its create outcome.
pub const OUTCOME: &str = include_str!("../fixtures/outcome.yaml");

/// Capture limits large enough for every fixture here.
pub const LIMITS: CaptureLimits = CaptureLimits {
    max_events: 20_000,
    max_blobs: 80_000,
    max_projection_rows: 80_000,
    max_payload_bytes: 256 * 1024 * 1024,
};

/// One parsed, validated definition.
pub fn definition(text: &str) -> EntityDefinition {
    let definition = entity_yaml::from_str(text).expect("fixture definition parses");
    definition.validate().expect("fixture definition validates");
    definition
}

/// A registry holding the given definitions.
pub fn registry(texts: &[&str]) -> Registry {
    let mut registry = Registry::new();
    for text in texts {
        registry
            .register(definition(text))
            .expect("fixture registers");
    }
    registry.validate_all().expect("fixture set validates");
    registry
}

/// Recording metadata with a fixed timestamp.
pub fn recording(record_id: &str) -> Recording {
    Recording {
        record_id: record_id.to_owned(),
        recorded_at: "2026-11-22T00:00:00Z".to_owned(),
        correlation: None,
        causation: None,
        actor: Some("er-kpi".to_owned()),
    }
}

/// Eventlog operational facts with a fixed time.
pub fn context(label: &str) -> EventlogOperationContext {
    EventlogOperationContext {
        subject: "er-kpi".to_owned(),
        actor: "er-kpi".to_owned(),
        request_id: format!("request-{label}"),
        trace_id: format!("trace-{label}"),
        causation_id: None,
        causation_depth: 0,
        occurred_at: OffsetDateTime::UNIX_EPOCH,
    }
}

/// A subject coordinate.
pub fn subject(entity: &str, id: &str) -> Subject {
    Subject::new(entity, id).expect("fixture subject")
}

/// The bridge queue every Eventlog facade here uses.
pub fn bridge() -> BridgeConfig {
    BridgeConfig {
        queue_capacity: NonZeroU16::new(8).expect("nonzero"),
    }
}

/// A current-thread runtime for the async ports.
pub fn block_on<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("runtime")
        .block_on(future)
}
