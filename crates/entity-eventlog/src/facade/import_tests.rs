//! What one legacy import through the facade observes of its destination.

use std::num::NonZeroU16;

use entity_core::{EntityInstance, Registry};
use entity_store::{
    LegacyStoreSnapshot,
    asynchronous::{
        HistoryOrigin, LegacyAnchor, LegacyCompleteness, LegacyOrderDeclaration, Subject,
        SubjectHistory,
    },
};
use eventlog_core::CaptureLimits;
use serde_json::json;
use time::OffsetDateTime;

use super::RecordedProviderFacade;
use crate::{
    EventlogOperationContext,
    adapter::CAPTURE_MODELS,
    sync::{
        BridgeConfig, CallWait, EventlogRecordedStoreProvisioner, ProvisionAuthority, ShutdownMode,
        ShutdownOutcome,
    },
};

const LIMITS: CaptureLimits = CaptureLimits {
    max_events: 1_024,
    max_blobs: 4_096,
    max_projection_rows: 4_096,
    max_payload_bytes: 16 * 1024 * 1024,
};

fn context(label: &str) -> EventlogOperationContext {
    EventlogOperationContext {
        subject: "import-cost".into(),
        actor: "entity-eventlog-test".into(),
        request_id: format!("request-{label}"),
        trace_id: format!("trace-{label}"),
        causation_id: None,
        causation_depth: 0,
        occurred_at: OffsetDateTime::UNIX_EPOCH,
    }
}

fn registry() -> Registry {
    let definition = serde_json::from_value(json!({
        "entity": "ticket",
        "version": 1,
        "schema": { "fields": { "title": { "type": "string", "required": true } } },
        "lifecycle": { "initial": "open", "states": ["open"] },
        "operations": {
            "touch": { "transitions": [{ "from": "open", "to": "open" }], "emits": [] }
        }
    }))
    .expect("definition parses");
    let mut registry = Registry::new();
    registry.register(definition).expect("definition validates");
    registry
}

fn imported(id: &str) -> SubjectHistory {
    SubjectHistory {
        subject: Subject::new("ticket", id).expect("subject"),
        origin: HistoryOrigin::Imported(LegacyAnchor {
            instance: EntityInstance {
                entity: "ticket".into(),
                version: 1,
                id: id.into(),
                lifecycle_state: "open".into(),
                revision: 2,
                fields: serde_json::from_value(json!({ "title": id })).expect("fields"),
            },
            completeness: LegacyCompleteness::AvailableEvidenceOnly,
            order: LegacyOrderDeclaration::PerKindOnly,
            evidence: Vec::new(),
        }),
        records: Vec::new(),
    }
}

fn observations(tenant: &str) -> usize {
    CAPTURE_MODELS
        .lock()
        .expect("counts")
        .get(tenant)
        .copied()
        .unwrap_or(0)
}

/// A legacy import of N fresh subjects observes its destination N + 1 times, and its closing
/// equivalence check once more: each subject is checked against what the subject before it
/// verified, instead of observing the destination again before it is written. Every subject
/// used to observe it before and after its append, 2N + 1 in all. An exact retry, every subject
/// replayed, observes it once, plus the closing check, instead of N + 1 times.
#[test]
fn a_legacy_import_observes_its_destination_once_per_subject_plus_once() {
    const SUBJECTS: usize = 4;
    let tenant = "import-cost-sequence";
    let mut facade = RecordedProviderFacade::provision(
        registry(),
        EventlogRecordedStoreProvisioner::SqliteMemory {
            prefix: "import_cost_sequence".into(),
            authority: ProvisionAuthority {
                logical_scope: "import-cost-scope".into(),
                tenant: tenant.into(),
                expected_stream_identity: None,
            },
            limits: LIMITS,
        },
        context("provision"),
        BridgeConfig {
            queue_capacity: NonZeroU16::new(4).expect("nonzero"),
        },
    )
    .expect("destination provisioned");
    let snapshot = LegacyStoreSnapshot::new(
        "legacy/import-cost",
        (0..SUBJECTS)
            .map(|index| imported(&format!("legacy-{index}")))
            .collect(),
    )
    .expect("legacy snapshot");

    let before = observations(tenant);
    let first = facade
        .import_legacy(snapshot.clone(), context("import"), CallWait::Forever)
        .expect("legacy import");
    let fresh = observations(tenant) - before;
    assert_eq!((first.subjects.len(), first.replayed), (SUBJECTS, 0));

    let before = observations(tenant);
    let retry = facade
        .import_legacy(snapshot, context("retry"), CallWait::Forever)
        .expect("exact retry");
    let replayed = observations(tenant) - before;
    assert_eq!((retry.subjects.len(), retry.replayed), (SUBJECTS, SUBJECTS));

    eprintln!("{SUBJECTS} subjects: {fresh} observations fresh, {replayed} replayed");
    assert_eq!(
        (fresh, replayed),
        (SUBJECTS + 2, 2),
        "a {SUBJECTS}-subject import observed its destination {fresh} times, its exact retry \
         {replayed} times"
    );
    assert_eq!(
        facade.shutdown(ShutdownMode::Drain, CallWait::Forever),
        ShutdownOutcome::Joined { provider: Ok(()) }
    );
}
