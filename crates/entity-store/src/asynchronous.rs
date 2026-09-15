//! Asynchronous complete-record storage contracts.

mod encoding;
mod memory;
mod ports;
mod types;
mod verify;

pub use encoding::{
    batch_comparison_bytes, canonical_domain_bytes, member_id, original_request_comparison_bytes,
    record_comparison_bytes,
};
pub use memory::{AppendScript, MemoryRecordedStore};
pub use ports::{
    AsyncRecordedReader, AsyncRecordedStore, AsyncRecordedWriter, AsyncStateReader, BoxFuture,
};
pub use types::*;
pub(crate) use verify::{validate_entry_against_state, validate_imported_boundary};
pub use verify::{
    verify_complete_store, verify_imported_record, verify_store_histories, verify_subject_history,
    verify_subject_prefix,
};

#[cfg(test)]
mod crate_test_support {
    use std::{
        future::Future,
        pin::Pin,
        task::{Context, Poll, Waker},
    };

    pub(super) fn block_on<F: Future>(future: F) -> F::Output {
        let mut context = Context::from_waker(Waker::noop());
        let mut future = std::pin::pin!(future);
        match Future::poll(Pin::as_mut(&mut future), &mut context) {
            Poll::Ready(output) => output,
            Poll::Pending => panic!("reference-store future unexpectedly remained pending"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Expect, RecordedCommit, RecordedObservation, Recording};
    use entity_core::{Registry, Runtime};
    use serde_json::{json, Value};

    fn recording(record_id: &str) -> Recording {
        Recording {
            record_id: record_id.to_owned(),
            recorded_at: "2026-09-15T00:00:00Z".to_owned(),
            correlation: None,
            causation: None,
            actor: None,
        }
    }

    fn registry() -> Registry {
        let definition = serde_json::from_value(json!({
            "entity": "ticket",
            "version": 1,
            "schema": { "fields": { "title": { "type": "string", "required": true } } },
            "lifecycle": { "initial": "open", "states": ["open", "closed"] },
            "operations": {
                "close": {
                    "transitions": [{ "from": "open", "to": "closed" }],
                    "emits": [
                        { "type": "TicketClosed", "payload": { "id": "$id" } },
                        { "type": "TicketClosed", "payload": { "id": "$id" } }
                    ]
                }
            }
        }))
        .expect("definition parses");
        let mut registry = Registry::new();
        registry.register(definition).expect("definition validates");
        registry
    }

    fn creation(id: &str, record_id: &str) -> RecordedCommit {
        let registry = registry();
        let decision = Runtime::new(&registry)
            .create("ticket", 1, id, json!({"title": "one"}))
            .expect("creation succeeds");
        RecordedCommit::new(decision, &recording(record_id)).expect("recording is valid")
    }

    fn closed(id: &str, record_id: &str) -> RecordedCommit {
        let registry = registry();
        let created = Runtime::new(&registry)
            .create("ticket", 1, id, json!({"title": "one"}))
            .expect("creation succeeds");
        let decision = Runtime::new(&registry)
            .execute(&created.instance, "close", json!({}))
            .expect("close succeeds");
        RecordedCommit::new(decision, &recording(record_id)).expect("recording is valid")
    }

    fn observation(id: &str, record_id: &str, value: Value) -> RecordedObservation {
        RecordedObservation {
            entity: "ticket".to_owned(),
            id: id.to_owned(),
            revision: 1,
            envelope: recording(record_id)
                .seal(value)
                .expect("recording is valid"),
        }
    }

    fn member(expect: Expect, entry: RecordedEntry) -> AppendMember {
        let request_bytes =
            original_request_comparison_bytes(&entry).expect("complete request encodes");
        AppendMember::new(expect, entry, request_bytes)
    }

    #[test]
    fn async_ports_are_object_safe_and_return_send_futures() {
        fn accepts_object(_: &dyn AsyncRecordedStore) {}
        fn is_send<T: Send>(_: T) {}

        let store = MemoryRecordedStore::new();
        let subject = Subject::new("ticket", "one").expect("valid subject");
        accepts_object(&store);
        is_send(AsyncStateReader::load(&store, &subject));
        is_send(AsyncRecordedReader::lookup_record(&store, "record-1"));
        is_send(AsyncRecordedReader::history(&store, &subject));
        is_send(AsyncRecordedWriter::append(&store, AppendRequest::empty()));
    }

    #[test]
    fn canonical_comparison_bytes_pin_exact_numbers_nulls_and_coordinate_identities() {
        let subject = Subject::new("E\"\\", "x\n").expect("valid subject");
        assert_eq!(subject.coordinate_id(), "[\"E\\\"\\\\\",\"x\\n\"]");
        assert_eq!(
            BatchKey::Named("b".to_owned())
                .coordinate_id()
                .expect("valid key"),
            "[\"named\",\"b\"]"
        );
        assert_eq!(
            member_id(&BatchKey::Named("b".to_owned()), u64::MAX).expect("valid member"),
            "[[\"named\",\"b\"],18446744073709551615]"
        );

        let expected = "[\"er.record/1\",{\"kind\":\"observation\",\"observation\":{\"entity\":\"E\",\"envelope\":{\"actor\":null,\"causation\":null,\"correlation\":null,\"record\":{\"n\":100.0},\"record_id\":\"r\",\"recorded_at\":\"2026-09-15T00:00:00Z\"},\"id\":\"x\",\"revision\":1}}]";
        let exact = RecordedEntry::Observation(RecordedObservation {
            entity: "E".to_owned(),
            id: "x".to_owned(),
            revision: 1,
            envelope: recording("r")
                .seal(serde_json::from_str("{\"n\":100.0}").expect("exact JSON"))
                .expect("valid recording"),
        });
        assert_eq!(
            record_comparison_bytes(&exact).expect("encodes"),
            expected.as_bytes()
        );

        let spellings = ["100", "100.0", "-0", "1e9999"];
        let bytes: Vec<Vec<u8>> = spellings
            .iter()
            .map(|token| {
                let value = serde_json::from_str(&format!("{{\"n\":{token}}}"))
                    .expect("arbitrary precision JSON parses");
                record_comparison_bytes(&RecordedEntry::Observation(observation("x", "r", value)))
                    .expect("encodes")
            })
            .collect();
        for left in 0..bytes.len() {
            for right in left + 1..bytes.len() {
                assert_ne!(bytes[left], bytes[right], "{spellings:?}");
            }
        }
        let large_exponent = String::from_utf8(bytes[3].clone()).expect("UTF-8");
        assert!(large_exponent.contains("1e+9999"), "{large_exponent}");
        let maximum_revision = RecordedEntry::Observation(RecordedObservation {
            entity: "E".to_owned(),
            id: "x".to_owned(),
            revision: i64::MAX as u64,
            envelope: recording("max-revision")
                .seal(json!({"limit": i64::MAX}))
                .expect("recording"),
        });
        assert!(String::from_utf8(
            record_comparison_bytes(&maximum_revision).expect("maximum revision encodes")
        )
        .expect("UTF-8")
        .contains("9223372036854775807"));
        let above_maximum = RecordedEntry::Observation(RecordedObservation {
            entity: "E".to_owned(),
            id: "x".to_owned(),
            revision: i64::MAX as u64 + 1,
            envelope: recording("over-revision")
                .seal(json!({"limit": "too large"}))
                .expect("recording"),
        });
        assert!(matches!(
            above_maximum.validate(),
            Err(AsyncStoreError::InvalidInput(_))
        ));
    }

    #[test]
    fn reference_store_commits_complete_records_atomically_and_verifies_them() {
        let store = MemoryRecordedStore::new();
        let commit = RecordedEntry::Decision(creation("one", "create-one"));
        let request = AppendRequest::new(
            BatchKey::SingleRecord("create-one".to_owned()),
            vec![member(Expect::Absent, commit)],
        )
        .expect("valid append");
        let outcome = crate_test_support::block_on(AsyncRecordedWriter::append(&store, request))
            .expect("append succeeds");
        let receipt = outcome.receipt().expect("committed receipt");
        assert_eq!(receipt.members().len(), 1);

        let subject = Subject::new("ticket", "one").expect("subject");
        let history = crate_test_support::block_on(AsyncRecordedReader::history(&store, &subject))
            .expect("history is readable");
        let terminal = crate_test_support::block_on(AsyncStateReader::load(&store, &subject))
            .expect("state is readable")
            .expect("state exists");
        assert_eq!(
            verify_subject_history(&history, &terminal).expect("history verifies"),
            SubjectAssurance::VerifiedFromGenesis { subject }
        );

        store.tamper_decision_result_for_test("create-one");
        let tampered = crate_test_support::block_on(AsyncRecordedReader::history(
            &store,
            &Subject::new("ticket", "one").expect("subject"),
        ))
        .expect("tampered history remains readable");
        assert!(matches!(
            verify_subject_history(&tampered, &terminal),
            Err(AsyncStoreError::CorruptHistory { .. })
        ));
    }

    #[test]
    fn imported_evidence_reserves_ids_without_inventing_receipts_or_global_order() {
        let store = MemoryRecordedStore::new();
        let imported_commit = creation("legacy", "legacy-create");
        let subject = Subject::new("ticket", "legacy").expect("subject");
        let evidence = ImportedRecordEvidence::new(
            RecordedEntry::Decision(imported_commit.clone()),
            "legacy-source",
            "records/0",
            KnownLegacyOrder::PerKind(0),
        )
        .expect("valid evidence");
        let boundary = LegacyAnchor {
            instance: imported_commit.instance.clone(),
            completeness: LegacyCompleteness::AvailableEvidenceOnly,
            order: LegacyOrderDeclaration::PerKindOnly,
            evidence: vec![LegacyEvidence::Envelope(evidence.clone())],
        };
        store
            .seed_imported(SubjectHistory {
                subject: subject.clone(),
                origin: HistoryOrigin::Imported(boundary),
                records: Vec::new(),
            })
            .expect("boundary seeds");

        assert_eq!(
            crate_test_support::block_on(AsyncRecordedReader::lookup_record(
                &store,
                "legacy-create"
            ))
            .expect("lookup succeeds"),
            Some(RecordLookup::Imported(evidence))
        );
        let conflict = RecordedEntry::Observation(observation(
            "other",
            "legacy-create",
            json!({"same_id": "different kind"}),
        ));
        let error = crate_test_support::block_on(AsyncRecordedWriter::append(
            &store,
            AppendRequest::new(
                BatchKey::SingleRecord("legacy-create".to_owned()),
                vec![member(Expect::Revision(1), conflict)],
            )
            .expect("request shape is valid"),
        ))
        .expect_err("the imported global id is reserved");
        assert!(matches!(
            error,
            WriteFailure::NotCommitted(AsyncStoreError::RecordConflict { .. })
        ));
    }

    #[test]
    fn whole_batch_position_overflow_refuses_without_publishing_a_prefix() {
        let store = MemoryRecordedStore::new();
        store.set_next_store_position_for_test(u64::MAX);
        let request = AppendRequest::new(
            BatchKey::Named("overflow".to_owned()),
            vec![
                member(
                    Expect::Absent,
                    RecordedEntry::Decision(creation("one", "one")),
                ),
                member(
                    Expect::Absent,
                    RecordedEntry::Decision(creation("two", "two")),
                ),
            ],
        )
        .expect("request shape is valid");
        let error = crate_test_support::block_on(AsyncRecordedWriter::append(&store, request))
            .expect_err("the second allocation overflows");
        assert!(matches!(
            error,
            WriteFailure::NotCommitted(AsyncStoreError::PositionExhausted { .. })
        ));
        assert!(crate_test_support::block_on(AsyncStateReader::load(
            &store,
            &Subject::new("ticket", "one").expect("subject")
        ))
        .expect("load succeeds")
        .is_none());
    }

    #[test]
    fn explicit_set_assurance_names_its_scope_subjects_and_import_boundaries() {
        let store = MemoryRecordedStore::new();
        let genesis = RecordedEntry::Decision(creation("genesis", "genesis-create"));
        let request = AppendRequest::new(
            BatchKey::SingleRecord("genesis-create".to_owned()),
            vec![member(Expect::Absent, genesis)],
        )
        .expect("append shape");
        crate_test_support::block_on(AsyncRecordedWriter::append(&store, request))
            .expect("genesis append");

        let imported_commit = creation("legacy", "legacy-create");
        let imported_subject = Subject::new("ticket", "legacy").expect("subject");
        store
            .seed_imported(SubjectHistory {
                subject: imported_subject.clone(),
                origin: HistoryOrigin::Imported(LegacyAnchor {
                    instance: imported_commit.instance.clone(),
                    completeness: LegacyCompleteness::AvailableEvidenceOnly,
                    order: LegacyOrderDeclaration::PerKindOnly,
                    evidence: vec![LegacyEvidence::Envelope(
                        ImportedRecordEvidence::new(
                            RecordedEntry::Decision(imported_commit),
                            "source",
                            "records/0",
                            KnownLegacyOrder::PerKind(0),
                        )
                        .expect("evidence"),
                    )],
                }),
                records: Vec::new(),
            })
            .expect("boundary seeds");

        let snapshot = crate_test_support::block_on(AsyncRecordedReader::complete_snapshot(
            &store,
            "reference-store",
        ))
        .expect("complete snapshot");
        let assurance =
            crate_test_support::block_on(verify_complete_store(&store, "reference-store"))
                .expect("snapshot verifies");
        assert_eq!(assurance.scope, "reference-store");
        assert_eq!(assurance.coverage, StoreCoverage::CompleteSnapshot);
        assert_eq!(assurance.subjects.len(), 2);
        assert!(assurance
            .subjects
            .contains(&SubjectAssurance::VerifiedAfterBoundary {
                subject: imported_subject,
                anchor_revision: 1,
            }));

        let selected = CompleteStoreSnapshot {
            scope: "caller-selected".to_owned(),
            coverage: StoreCoverage::ExplicitSet,
            histories: vec![snapshot.histories[0].clone()],
        };
        let selected_assurance = verify_store_histories(&selected).expect("selection verifies");
        assert_eq!(selected_assurance.coverage, StoreCoverage::ExplicitSet);
        assert_eq!(selected_assurance.subjects.len(), 1);
    }

    #[test]
    fn editable_complete_markers_never_replace_a_direct_provider_capture() {
        let store = MemoryRecordedStore::new();
        let entry = RecordedEntry::Decision(creation("one", "create-one"));
        crate_test_support::block_on(AsyncRecordedWriter::append(
            &store,
            AppendRequest::new(
                BatchKey::SingleRecord("create-one".to_owned()),
                vec![member(Expect::Absent, entry)],
            )
            .expect("append shape"),
        ))
        .expect("append succeeds");

        let editable = crate_test_support::block_on(AsyncRecordedReader::complete_snapshot(
            &store,
            "reference-store",
        ))
        .expect("provider capture");
        assert!(matches!(
            verify_store_histories(&editable),
            Err(AsyncStoreError::InvalidInput(_))
        ));
        let assurance =
            crate_test_support::block_on(verify_complete_store(&store, "reference-store"))
                .expect("direct provider capture verifies");
        assert_eq!(assurance.coverage, StoreCoverage::CompleteSnapshot);
        assert_eq!(assurance.subjects.len(), 1);
    }

    #[test]
    fn all_revision_bearing_imported_evidence_is_bounded_before_publication() {
        let anchor = creation("legacy", "anchor");
        let later = closed("legacy", "later-decision");
        let later_envelope = ImportedRecordEvidence::new(
            RecordedEntry::Decision(later.clone()),
            "source",
            "decisions/1",
            KnownLegacyOrder::PerKind(1),
        )
        .expect("complete imported envelope");
        let later_event = later
            .envelope
            .record
            .events
            .first()
            .expect("close emits an event")
            .clone();
        let cases = [
            LegacyEvidence::Envelope(later_envelope),
            LegacyEvidence::Decision(later.envelope.record),
            LegacyEvidence::Event(later_event),
        ];

        for evidence in cases {
            let store = MemoryRecordedStore::new();
            let subject = Subject::new("ticket", "legacy").expect("subject");
            let error = store
                .seed_imported(SubjectHistory {
                    subject: subject.clone(),
                    origin: HistoryOrigin::Imported(LegacyAnchor {
                        instance: anchor.instance.clone(),
                        completeness: LegacyCompleteness::AvailableEvidenceOnly,
                        order: LegacyOrderDeclaration::PerKindOnly,
                        evidence: vec![evidence],
                    }),
                    records: Vec::new(),
                })
                .expect_err("post-anchor evidence refuses");
            assert!(matches!(error, AsyncStoreError::CorruptHistory { .. }));
            assert!(
                crate_test_support::block_on(AsyncStateReader::load(&store, &subject))
                    .expect("state lookup")
                    .is_none()
            );
            assert!(
                crate_test_support::block_on(AsyncRecordedReader::lookup_record(
                    &store,
                    "later-decision"
                ))
                .expect("identity lookup")
                .is_none()
            );
            assert!(
                crate_test_support::block_on(AsyncRecordedReader::history(&store, &subject))
                    .expect("history lookup")
                    .records
                    .is_empty()
            );
        }
    }

    #[test]
    fn partial_prior_imported_evidence_remains_allowed_across_all_typed_variants() {
        let anchor = closed("legacy", "anchor-at-two");
        let earlier = creation("legacy", "earlier-create");
        let earlier_envelope = ImportedRecordEvidence::new(
            RecordedEntry::Decision(earlier.clone()),
            "source",
            "decisions/0",
            KnownLegacyOrder::PerKind(0),
        )
        .expect("complete imported envelope");
        let anchor_event = anchor
            .envelope
            .record
            .events
            .first()
            .expect("close emits an event")
            .clone();
        let subject = Subject::new("ticket", "legacy").expect("subject");
        let store = MemoryRecordedStore::new();
        store
            .seed_imported(SubjectHistory {
                subject: subject.clone(),
                origin: HistoryOrigin::Imported(LegacyAnchor {
                    instance: anchor.instance,
                    completeness: LegacyCompleteness::AvailableEvidenceOnly,
                    order: LegacyOrderDeclaration::PerKindOnly,
                    evidence: vec![
                        LegacyEvidence::Envelope(earlier_envelope),
                        LegacyEvidence::Decision(earlier.envelope.record),
                        LegacyEvidence::Event(anchor_event),
                    ],
                }),
                records: Vec::new(),
            })
            .expect("partial evidence at or before the anchor is allowed");
        assert!(
            crate_test_support::block_on(AsyncRecordedReader::lookup_record(
                &store,
                "earlier-create"
            ))
            .expect("identity lookup")
            .is_some()
        );
        assert_eq!(
            crate_test_support::block_on(AsyncStateReader::load(&store, &subject))
                .expect("state lookup")
                .expect("anchor state")
                .revision,
            2
        );
    }

    #[test]
    fn bare_imported_decisions_and_events_must_name_the_anchor_subject() {
        let anchor = closed("legacy", "anchor-at-two");
        let mut wrong_decision = creation("other", "wrong-decision").envelope.record;
        wrong_decision.revision = 1;
        let mut wrong_event = anchor
            .envelope
            .record
            .events
            .first()
            .expect("close emits an event")
            .clone();
        wrong_event.id = "other".to_owned();

        for evidence in [
            LegacyEvidence::Decision(wrong_decision),
            LegacyEvidence::Event(wrong_event),
        ] {
            let store = MemoryRecordedStore::new();
            let subject = Subject::new("ticket", "legacy").expect("subject");
            let error = store
                .seed_imported(SubjectHistory {
                    subject: subject.clone(),
                    origin: HistoryOrigin::Imported(LegacyAnchor {
                        instance: anchor.instance.clone(),
                        completeness: LegacyCompleteness::AvailableEvidenceOnly,
                        order: LegacyOrderDeclaration::PerKindOnly,
                        evidence: vec![evidence],
                    }),
                    records: Vec::new(),
                })
                .expect_err("cross-subject bare evidence refuses");
            assert!(matches!(error, AsyncStoreError::CorruptHistory { .. }));
            assert!(
                crate_test_support::block_on(AsyncStateReader::load(&store, &subject))
                    .expect("state lookup")
                    .is_none()
            );
        }
    }

    #[test]
    fn record_and_receipt_component_mismatches_are_integrity_refusals() {
        let store = MemoryRecordedStore::new();
        let request = AppendRequest::new(
            BatchKey::SingleRecord("create-one".to_owned()),
            vec![member(
                Expect::Absent,
                RecordedEntry::Decision(creation("one", "create-one")),
            )],
        )
        .expect("append shape");
        crate_test_support::block_on(AsyncRecordedWriter::append(&store, request)).expect("append");
        store.tamper_receipt_subject_for_test("create-one", "other");
        let subject = Subject::new("ticket", "one").expect("subject");
        let history = crate_test_support::block_on(AsyncRecordedReader::history(&store, &subject))
            .expect("history read");
        let terminal = crate_test_support::block_on(AsyncStateReader::load(&store, &subject))
            .expect("state read")
            .expect("state");
        assert!(matches!(
            verify_subject_history(&history, &terminal),
            Err(AsyncStoreError::CorruptHistory { .. })
        ));
    }
}
