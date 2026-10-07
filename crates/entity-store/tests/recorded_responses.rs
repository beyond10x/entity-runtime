//! A decision recorded before step 14 checked a `service/1` response against its declared schema
//! (R-169) may carry a response that check refuses. The recorded-store verifier recomputes a
//! stored decision as `entity_core::replay` does, so such a history still verifies, and a recorded
//! response its branch does not answer is still refused.

use std::path::Path;

use entity_core::{Decision, DecisionRecord, EntityInstance};
use entity_store::{
    asynchronous::{validate_entry_against_state, AsyncStoreError, RecordedEntry},
    Expect, RecordedCommit, Recording,
};
use serde_json::json;

/// The history the entity-core fixture holds: a creation and an operation the base kernel
/// answered with responses outside their declared `max_length` and `alphabet`.
fn recorded_before_responses_were_checked() -> Vec<DecisionRecord> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../entity-core/tests/fixtures/responses-recorded-before-they-were-checked.json");
    serde_json::from_str(&std::fs::read_to_string(path).expect("the fixture is readable"))
        .expect("the fixture is a list of decision records")
}

fn entry(at: usize, record: DecisionRecord) -> RecordedEntry {
    let decision = Decision {
        instance: record.result.clone(),
        events: record.events.clone(),
        record,
    };
    let recording = Recording {
        record_id: format!("r-{at}"),
        recorded_at: "2026-10-01T00:00:00Z".to_owned(),
        correlation: None,
        causation: None,
        actor: None,
    };
    RecordedEntry::Decision(RecordedCommit::new(decision, &recording).expect("structurally valid"))
}

fn verify(records: Vec<DecisionRecord>) -> Result<Option<EntityInstance>, AsyncStoreError> {
    let mut state = None;
    for (at, record) in records.into_iter().enumerate() {
        let expect = if at == 0 {
            Expect::Absent
        } else {
            Expect::Revision(at as u64)
        };
        state = validate_entry_against_state(&entry(at, record), expect, state.as_ref())?;
    }
    Ok(state)
}

#[test]
fn a_stored_decision_recorded_before_responses_were_checked_still_verifies() {
    let state = verify(recorded_before_responses_were_checked())
        .unwrap_or_else(|error| panic!("a history recorded before the check verifies: {error}"));
    assert_eq!(
        state.expect("an instance").fields["last"],
        json!("12x"),
        "the verified state is the one the operation recorded"
    );
}

#[test]
fn a_stored_response_its_branch_does_not_answer_is_still_refused() {
    let mut records = recorded_before_responses_were_checked();
    records[1]
        .response
        .as_mut()
        .expect("responds")
        .insert("echo".to_owned(), json!("12"));
    match verify(records) {
        Err(AsyncStoreError::CorruptHistory { detail, .. }) => assert!(
            detail.contains("differs from recomputation"),
            "refused for another reason: {detail}"
        ),
        other => panic!("a tampered response is refused as corrupt history: {other:?}"),
    }
}
