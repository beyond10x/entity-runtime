//! Adversarial probes of R-165 through the store paths: a `{cleared: true}` decision is the first
//! `kernel/1`, `service/1` or `service/2` decision to carry a non-empty `removed`, under the
//! `er.record/1`, `/2` and `/3` framings that never carried one before. Each probe asks whether such
//! a record encodes in the framing `record_domain` gives it, verifies by re-execution, and replays
//! from a reopened file store to the bytes `execute` returned.

use std::{
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
    task::{Context, Poll, Waker},
};

use entity_core::{replay, Decision, Registry, Runtime};
use entity_store::{
    asynchronous::{
        original_request_comparison_bytes, read_record_in_domain, record_comparison_bytes,
        record_domain, request_domain, verify_complete_store, AppendMember, AppendRequest,
        AsyncRecordedWriter, BatchKey, MemoryRecordedStore, RecordedEntry,
    },
    EventProvider, Expect, FileStore, HistoryProvider, RecordedCommit, Recording, StateProvider,
    Store,
};
use serde_json::{json, Value};

fn block_on<F: Future>(future: F) -> F::Output {
    let mut context = Context::from_waker(Waker::noop());
    let mut future = std::pin::pin!(future);
    match Future::poll(Pin::as_mut(&mut future), &mut context) {
        Poll::Ready(output) => output,
        Poll::Pending => panic!("reference-store future unexpectedly remained pending"),
    }
}

fn recording(record_id: &str) -> Recording {
    Recording {
        record_id: record_id.to_owned(),
        recorded_at: "2026-10-07T00:00:00Z".to_owned(),
        correlation: None,
        causation: None,
        actor: None,
    }
}

fn scratch(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("adversary-clear-records")
        .join(name);
    let _ = std::fs::remove_dir_all(&root);
    root
}

/// A reminder under `semantics`, whose `wake` clears `snoozed_until` and emits `Woken`.
fn registry(semantics: &str) -> Registry {
    let mut document = json!({
        "entity": "reminder",
        "version": 1,
        "schema": { "fields": {
            "title": { "type": "string", "required": true },
            "snoozed_until": { "type": "string" }
        }},
        "lifecycle": { "initial": "active", "states": ["active"] },
        "operations": { "wake": {
            "transitions": [{ "from": "active", "to": "active" }],
            "set": { "snoozed_until": { "cleared": true } },
            "emits": [{ "type": "Woken", "payload": { "title": "$fields.title" } }]
        }}
    });
    if semantics != "kernel/1" {
        document["semantics"] = json!(semantics);
        document["operations"]["wake"] = json!({
            "outcomes": [{
                "name": "woken",
                "effect": "updates",
                "set": { "snoozed_until": { "cleared": true } },
                "emits": [{ "type": "Woken", "payload": { "title": "$fields.title" } }]
            }]
        });
    }
    let mut registry = Registry::new();
    registry
        .register(serde_json::from_value(document).expect("a definition document"))
        .expect("registers");
    registry
}

/// Create, wake (clearing a present field) and wake again (clearing an absent one).
fn decisions(semantics: &str) -> Vec<Decision> {
    let registry = registry(semantics);
    let runtime = Runtime::new(&registry);
    let created = runtime
        .create(
            "reminder",
            1,
            "r-1",
            json!({ "title": "call back", "snoozed_until": "2026-10-08T09:00:00Z" }),
        )
        .expect("created");
    let woken = runtime
        .execute(&created.instance, "wake", json!({}))
        .expect("cleared");
    let again = runtime
        .execute(&woken.instance, "wake", json!({}))
        .expect("cleared again");
    vec![created, woken, again]
}

fn commits(semantics: &str) -> Vec<RecordedCommit> {
    decisions(semantics)
        .into_iter()
        .enumerate()
        .map(|(at, decision)| {
            RecordedCommit::new(decision, &recording(&format!("{semantics}-{at}")))
                .expect("records")
        })
        .collect()
}

fn text(value: &impl serde::Serialize) -> String {
    serde_json::to_string(value).expect("serializes")
}

#[test]
fn a_clear_record_frames_in_the_domain_of_its_semantics_and_verifies_in_the_memory_recorded_store()
{
    for (semantics, record, request) in [
        ("kernel/1", "er.record/1", "er.request/1"),
        ("service/1", "er.record/2", "er.request/2"),
        ("service/2", "er.record/3", "er.request/3"),
    ] {
        let store = MemoryRecordedStore::new();
        let commits = commits(semantics);
        let terminal = commits[2].instance.clone();
        for (at, commit) in commits.into_iter().enumerate() {
            commit.validate().expect("structurally valid");
            let expect = if at == 0 {
                Expect::Absent
            } else {
                Expect::Revision(at as u64)
            };
            let entry = RecordedEntry::Decision(commit);
            assert_eq!(record_domain(&entry), record, "{semantics}");
            assert_eq!(request_domain(&entry), request, "{semantics}");
            let bytes = record_comparison_bytes(&entry).expect("encodes");
            let body = read_record_in_domain(record, &bytes).expect("reads in its own domain");
            if at > 0 {
                assert_eq!(
                    body["commit"]["envelope"]["record"]["removed"],
                    json!(["snoozed_until"]),
                    "{semantics} record {at}: {body}"
                );
                assert_eq!(
                    body["commit"]["envelope"]["record"]["events"][0]["removed"],
                    json!(["snoozed_until"]),
                    "{semantics} record {at}"
                );
            }
            let request_bytes = original_request_comparison_bytes(&entry).expect("request");
            let append = AppendRequest::new(
                BatchKey::SingleRecord(format!("{semantics}-{at}")),
                vec![AppendMember::new(expect, entry, request_bytes)],
            )
            .expect("append request");
            // The memory store re-executes each record on the state before it.
            block_on(AsyncRecordedWriter::append(&store, append))
                .unwrap_or_else(|error| panic!("{semantics} record {at} commits: {error}"));
        }
        block_on(verify_complete_store(&store, "clear"))
            .unwrap_or_else(|error| panic!("{semantics} store verifies: {error}"));
        assert_eq!(
            text(&terminal.fields),
            r#"{"title":"call back"}"#,
            "{semantics}"
        );
    }
}

#[test]
fn a_file_store_reopened_returns_the_clear_records_and_they_replay_to_the_stored_instance() {
    for semantics in ["kernel/1", "service/1", "service/2"] {
        let root = scratch(&semantics.replace('/', "-"));
        let commits = commits(semantics);
        {
            let mut store = FileStore::open(&root);
            for (at, commit) in commits.iter().enumerate() {
                let expect = if at == 0 {
                    Expect::Absent
                } else {
                    Expect::Revision(at as u64)
                };
                store
                    .commit_recorded(commit, expect)
                    .unwrap_or_else(|error| panic!("{semantics} record {at}: {error}"));
            }
        }
        let reopened = FileStore::open(&root);
        let loaded = reopened
            .load("reminder", "r-1")
            .expect("loads")
            .expect("present");
        assert_eq!(text(&loaded), text(&commits[2].instance), "{semantics}");
        let records: Vec<_> = reopened
            .records("reminder", "r-1")
            .expect("records")
            .into_iter()
            .map(|envelope| envelope.record)
            .collect();
        assert_eq!(records.len(), 3);
        assert_eq!(
            text(&records[1].removed),
            r#"["snoozed_until"]"#,
            "{semantics}"
        );
        let replayed = replay(&records).expect("the reopened records replay");
        assert_eq!(text(&replayed), text(&loaded), "{semantics}");
        let events: Vec<Value> = reopened
            .events("reminder", "r-1")
            .expect("events")
            .iter()
            .map(|event| serde_json::to_value(event).expect("event"))
            .collect();
        // The probe definition emits nothing on creation: one `Woken` per `wake`.
        assert_eq!(events.len(), 2, "{semantics}");
        assert_eq!(
            events[0]["removed"],
            json!(["snoozed_until"]),
            "{semantics}"
        );
        assert_eq!(
            events[1]["removed"],
            json!(["snoozed_until"]),
            "{semantics}"
        );
    }
}
