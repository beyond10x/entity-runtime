//! What one handle has already verified, remembered under exactly what it verified.
//!
//! Every read still observes the authority afresh — a complete capture or a per-entity read — and
//! every observation is still verified. What is not repeated is work whose answer is a function of
//! bytes this handle has already put through it: hashing a blob against the digest its reference
//! names, decoding it, and replaying a subject history this handle has already verified record by
//! record. Each is remembered only under its complete input — the blob's digest *and* its exact
//! bytes *and* the domain it was admitted in; the subject's exact origin and every stored record of
//! the verified prefix — so a read that differs in one byte or one coordinate from what was
//! verified is verified from nothing, exactly as before.
//!
//! None of this is persisted, and nothing here changes what a read answers or refuses: a
//! remembered answer is only ever the answer the verifying path gave to the same input.

use std::collections::BTreeMap;

use entity_core::EntityInstance;
#[cfg(test)]
use entity_store::asynchronous::SubjectHistory;
use entity_store::asynchronous::{
    HistoryOrigin, RecordForm, RecordedEntry, StoredRecord, Subject, definition_comparison_bytes,
    record_definition,
};

use super::shared::{ModelHistory, SharedRecord};
use crate::encoding::RecordedEntryWrapper;

/// Bytes a handle may hold in memory before it forgets everything and starts again.
///
/// Forgetting costs a re-verification and nothing else, so the bound is only there to keep a
/// long-lived handle over a growing store from holding a second copy of all of it forever.
const MEMORY_BYTES: usize = 64 * 1024 * 1024;

/// What one decoded record or entry wrapper is charged beyond its raw bytes: a fixed part and a
/// multiple of those bytes.
///
/// Measured with a counting allocator on the fixture `tests` pins, allocator bookkeeping of 16
/// bytes an allocation included: a decoded record retains 12.9 KB at 1.4 KB raw and 2.43–2.6
/// times its raw bytes from 32 KB up (x9.3 at the smallest, x2.51 at 65 KB, x1.04 for a record
/// that is one 64 KB string); a small service record 16.4 KB at 2.3 KB raw. 16 KiB plus three
/// times the raw bytes is above every one of them.
const DECODED_FIXED: usize = 16 * 1024;
const DECODED_PER_BYTE: usize = 3;

/// What one map entry and its owned key cost beyond what it holds, charged per remembered blob,
/// subject, record and batch member subject.
const ENTRY: usize = 256;

fn decoded_charge(raw: usize) -> usize {
    DECODED_FIXED.saturating_add(raw.saturating_mul(DECODED_PER_BYTE))
}

/// What a record decoded from `bytes` holds beyond them: the definition an `er.record/5` record
/// names by digest, which its decode holds in full. Such a record is charged as the record that
/// embeds it, so a small record naming a large definition does not pass the cap uncounted.
fn named_definition_bytes(bytes: &[u8], entry: &RecordedEntry) -> usize {
    if RecordForm::of_record(bytes) != RecordForm::DefinitionByDigest {
        return 0;
    }
    record_definition(entry)
        .and_then(|definition| definition_comparison_bytes(definition).ok())
        .map_or(0, |document| document.len())
}

impl Decoded {
    /// What this decode of `bytes` is charged against the cap.
    fn charge(&self, bytes: &[u8]) -> usize {
        let raw = bytes.len();
        match self {
            Self::Entry(_) => decoded_charge(raw),
            Self::Record(entry) => decoded_charge(raw + named_definition_bytes(bytes, entry)),
            Self::BatchSubjects(subjects) => subjects
                .iter()
                .map(|subject| ENTRY + subject.entity.len() + subject.id.len())
                .sum(),
        }
    }
}

/// What a remembered stored record is charged: its decode, both byte strings and its receipt.
fn record_charge(record: &StoredRecord) -> usize {
    decoded_charge(
        record.record_bytes.len() + named_definition_bytes(&record.record_bytes, &record.entry),
    )
    .saturating_add(record.record_bytes.len())
    .saturating_add(record.request_bytes.len())
    .saturating_add(4 * ENTRY)
}

/// What a remembered history is charged beyond its records: its subject, its origin and its
/// terminal state.
///
/// The memory keeps its own copy of the origin and of the terminal. An imported origin holds the
/// whole anchor — its state and every evidence envelope — so it is charged as a decode of what it
/// encodes to, however large. A terminal reached by records is no larger than the record that
/// produced it, which holds the state it reached; a history with no records yet ends on the
/// anchor's state, which is charged as a decode of what it encodes to.
fn history_charge(history: &impl RememberedHistory, terminal: &EntityInstance) -> usize {
    let largest = history
        .records()
        .map(|record| record.record_bytes.len())
        .max();
    let origin = match history.origin() {
        HistoryOrigin::Genesis => 0,
        HistoryOrigin::Imported(anchor) => decoded_charge(encoded_len(anchor)),
    };
    let terminal = largest.unwrap_or_else(|| encoded_len(terminal));
    ENTRY
        .saturating_add(origin)
        .saturating_add(decoded_charge(terminal))
}

/// The bytes `value` encodes to as JSON, counted without keeping them.
pub(super) fn encoded_len(value: &impl serde::Serialize) -> usize {
    struct Count(usize);
    impl std::io::Write for Count {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0 = self.0.saturating_add(bytes.len());
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut count = Count(0);
    // A verified origin and state always encode; one that did not would be charged what it had
    // encoded so far, which only means remembering it is forgotten sooner or later than exact.
    let _ = serde_json::to_writer(&mut count, value);
    count.0
}

/// A decode of one verified blob, by the decoder its domain names.
#[derive(Clone)]
pub(super) enum Decoded {
    /// An `er.recorded_entry` wrapper.
    Entry(Box<RecordedEntryWrapper>),
    /// A complete record, already held to its own canonical bytes.
    Record(Box<RecordedEntry>),
    /// The subjects of a committed group's members, in member order, from a group whose every
    /// member was held to the record blob its reference binds.
    BatchSubjects(Vec<Subject>),
}

struct Remembered {
    domain: &'static str,
    bytes: Vec<u8>,
    decoded: Option<Decoded>,
    /// For a record blob: the request blob bytes this handle held to be this record's request.
    request: Option<Vec<u8>>,
    /// What this entry is charged against the cap, everything it holds included.
    charged: usize,
}

/// A history this memory can remember, however its records are held.
///
/// A model's history shares each record with the model's other indexes, and the memory keeps one
/// more reference to that allocation instead of a copy of it. A history that owns its records —
/// only this module's cap test remembers one — is copied in record by record.
pub(super) trait RememberedHistory {
    fn subject(&self) -> &Subject;
    fn origin(&self) -> &HistoryOrigin;
    fn records(&self) -> impl Iterator<Item = &StoredRecord>;
    fn len(&self) -> usize;
    /// Every record, as the memory holds it.
    fn shared(&self) -> Vec<SharedRecord>;
}

impl RememberedHistory for ModelHistory {
    fn subject(&self) -> &Subject {
        &self.subject
    }
    fn origin(&self) -> &HistoryOrigin {
        &self.origin
    }
    fn records(&self) -> impl Iterator<Item = &StoredRecord> {
        self.records.iter().map(|record| &**record)
    }
    fn len(&self) -> usize {
        self.records.len()
    }
    fn shared(&self) -> Vec<SharedRecord> {
        self.records.clone()
    }
}

#[cfg(test)]
impl RememberedHistory for SubjectHistory {
    fn subject(&self) -> &Subject {
        &self.subject
    }
    fn origin(&self) -> &HistoryOrigin {
        &self.origin
    }
    fn records(&self) -> impl Iterator<Item = &StoredRecord> {
        self.records.iter()
    }
    fn len(&self) -> usize {
        self.records.len()
    }
    fn shared(&self) -> Vec<SharedRecord> {
        self.records
            .iter()
            .cloned()
            .map(std::sync::Arc::new)
            .collect()
    }
}

struct VerifiedHistory {
    origin: HistoryOrigin,
    /// The verified records, each the allocation the model that verified it holds. The charge
    /// below still counts each in full: once that model is replaced, this is what keeps it alive.
    records: Vec<SharedRecord>,
    terminal: EntityInstance,
    /// What this history is charged against the cap, every record it holds included.
    charged: usize,
}

/// What one handle has verified: blobs by digest and bytes, and subject histories by value.
pub(super) struct VerifiedMemory {
    blobs: BTreeMap<String, Remembered>,
    histories: BTreeMap<Subject, VerifiedHistory>,
    held: usize,
    /// The most `held` may reach before everything is forgotten.
    cap: usize,
}

impl Default for VerifiedMemory {
    fn default() -> Self {
        Self::with_cap(MEMORY_BYTES)
    }
}

impl VerifiedMemory {
    fn with_cap(cap: usize) -> Self {
        Self {
            blobs: BTreeMap::new(),
            histories: BTreeMap::new(),
            held: 0,
            cap,
        }
    }

    /// Whether this handle already verified exactly `bytes` as the blob `digest` names in `domain`.
    pub(super) fn admitted(&self, digest: &str, domain: &str, bytes: &[u8]) -> bool {
        self.blobs
            .get(digest)
            .is_some_and(|remembered| remembered.domain == domain && remembered.bytes == bytes)
    }

    /// Remembers that `bytes` were just verified as the blob `digest` names in `domain`.
    pub(super) fn admit(&mut self, digest: &str, domain: &'static str, bytes: &[u8]) {
        if self.admitted(digest, domain, bytes) {
            return;
        }
        let charged = ENTRY + digest.len() + bytes.len();
        if charged > self.cap {
            self.forget_blob(digest);
            return;
        }
        self.make_room(charged);
        if let Some(replaced) = self.blobs.insert(
            digest.to_owned(),
            Remembered {
                domain,
                bytes: bytes.to_vec(),
                decoded: None,
                request: None,
                charged,
            },
        ) {
            self.held = self.held.saturating_sub(replaced.charged);
        }
        self.held = self.held.saturating_add(charged);
    }

    /// The decode remembered for exactly `bytes` under `digest`, if this handle verified them.
    ///
    /// The bytes are compared, not trusted from the digest: a caller that has not yet verified the
    /// digest of what it holds is answered only if it holds the bytes this handle verified.
    pub(super) fn decoded(&self, digest: &str, bytes: &[u8]) -> Option<&Decoded> {
        self.blobs
            .get(digest)
            .filter(|remembered| remembered.bytes == bytes)
            .and_then(|remembered| remembered.decoded.as_ref())
    }

    /// Remembers the decode of a blob this handle verified as exactly `bytes`.
    pub(super) fn remember(&mut self, digest: &str, bytes: &[u8], decoded: Decoded) {
        let adding = decoded.charge(bytes);
        let replacing = self
            .blobs
            .get(digest)
            .filter(|remembered| remembered.bytes == bytes)
            .and_then(|remembered| remembered.decoded.as_ref())
            .map_or(0, |previous| previous.charge(bytes));
        let Some(blob) = self
            .blobs
            .get(digest)
            .filter(|remembered| remembered.bytes == bytes)
            .map(|remembered| remembered.charged)
        else {
            return;
        };
        if blob.saturating_sub(replacing).saturating_add(adding) > self.cap {
            // The blob and this decode could never be held together: neither is kept.
            self.forget_blob(digest);
            return;
        }
        // Forgetting everything forgets the blob this decode belongs to, and then there is
        // nothing to attach it to: a decode is only ever remembered beside its verified bytes.
        self.make_room(adding.saturating_sub(replacing));
        if let Some(remembered) = self
            .blobs
            .get_mut(digest)
            .filter(|remembered| remembered.bytes == bytes)
        {
            let previous = remembered
                .decoded
                .replace(decoded)
                .map_or(0, |previous| previous.charge(bytes));
            remembered.charged = remembered
                .charged
                .saturating_sub(previous)
                .saturating_add(adding);
            self.held = self.held.saturating_sub(previous).saturating_add(adding);
        }
    }

    /// Whether this handle already held `request` to be the request of exactly `record`.
    pub(super) fn request_held(&self, digest: &str, record: &[u8], request: &[u8]) -> bool {
        self.blobs.get(digest).is_some_and(|remembered| {
            remembered.bytes == record && remembered.request.as_deref() == Some(request)
        })
    }

    /// Remembers that `request` was just held to be the request of exactly `record`.
    pub(super) fn hold_request(&mut self, digest: &str, record: &[u8], request: &[u8]) {
        let Some(blob) = self
            .blobs
            .get(digest)
            .filter(|remembered| remembered.bytes == record)
            .map(|remembered| {
                let replaced = remembered.request.as_ref().map_or(0, Vec::len);
                remembered.charged.saturating_sub(replaced)
            })
        else {
            return;
        };
        if blob.saturating_add(request.len()) > self.cap {
            self.forget_blob(digest);
            return;
        }
        self.make_room(request.len());
        let Some(remembered) = self
            .blobs
            .get_mut(digest)
            .filter(|remembered| remembered.bytes == record)
        else {
            return;
        };
        let replaced = remembered
            .request
            .replace(request.to_vec())
            .map_or(0, |bytes| bytes.len());
        remembered.charged = remembered
            .charged
            .saturating_sub(replaced)
            .saturating_add(request.len());
        self.held = self
            .held
            .saturating_sub(replaced)
            .saturating_add(request.len());
    }

    /// How many leading records of `history` this handle has verified, and the state they reach.
    ///
    /// Answered only when the origin is the verified origin and those records are the verified
    /// records, field for field — entry, coordinates, receipt, expectation, lineage and both
    /// stored byte strings. `history` must already be in store order, as a verified one is.
    pub(super) fn verified_prefix(
        &self,
        history: &ModelHistory,
    ) -> Option<(usize, EntityInstance)> {
        let verified = self.histories.get(&history.subject)?;
        let length = verified.records.len();
        (length > 0
            && length <= history.records.len()
            && verified.origin == history.origin
            && verified.records[..] == history.records[..length])
            .then(|| (length, verified.terminal.clone()))
    }

    /// Remembers a history this handle just verified to reach `terminal`.
    ///
    /// `verified` is the prefix [`Self::verified_prefix`] answered for this history, if any: those
    /// records are already remembered, equal by value, so the history grows without being charged
    /// for them again. It still takes the given history's allocation of every record, the prefix
    /// included: after a whole rebuild the prefix it remembered belongs to the previous model, and
    /// keeping it would hold that model's copy of each record alive beside the current one.
    pub(super) fn remember_history(
        &mut self,
        history: &impl RememberedHistory,
        terminal: &EntityInstance,
        verified: Option<usize>,
    ) {
        let charged = history_charge(history, terminal)
            .saturating_add(history.records().map(record_charge).sum::<usize>());
        if charged > self.cap {
            // Holding it would pass the cap even with everything else forgotten. What this memory
            // remembered of the subject belongs to an earlier model, so it goes too.
            if let Some(forgotten) = self.histories.remove(history.subject()) {
                self.held = self.held.saturating_sub(forgotten.charged);
            }
            return;
        }
        if let Some(length) = verified
            && length <= history.len()
            && let Some(remembered) = self.histories.get(history.subject())
            && remembered.records.len() == length
        {
            let growth = charged.saturating_sub(remembered.charged);
            if self.held.saturating_add(growth) <= self.cap
                && let Some(remembered) = self.histories.get_mut(history.subject())
            {
                remembered.records = history.shared();
                remembered.terminal = terminal.clone();
                remembered.charged = charged;
                self.held = self.held.saturating_add(growth);
                return;
            }
        }
        let replacing = self
            .histories
            .get(history.subject())
            .map_or(0, |remembered| remembered.charged);
        self.make_room(charged.saturating_sub(replacing));
        if let Some(replaced) = self.histories.insert(
            history.subject().clone(),
            VerifiedHistory {
                origin: history.origin().clone(),
                records: history.shared(),
                terminal: terminal.clone(),
                charged,
            },
        ) {
            self.held = self.held.saturating_sub(replaced.charged);
        }
        self.held = self.held.saturating_add(charged);
    }

    /// The records remembered for `subject`'s verified history, for a test of what they share.
    #[cfg(test)]
    pub(super) fn remembered_records(&self, subject: &Subject) -> Option<&[SharedRecord]> {
        self.histories
            .get(subject)
            .map(|history| history.records.as_slice())
    }

    /// Forgets the blob `digest` names, with its decode and request, if this memory holds it.
    fn forget_blob(&mut self, digest: &str) {
        if let Some(forgotten) = self.blobs.remove(digest) {
            self.held = self.held.saturating_sub(forgotten.charged);
        }
    }

    fn make_room(&mut self, adding: usize) {
        if self.held.saturating_add(adding) > self.cap {
            *self = Self::with_cap(self.cap);
        }
    }
}

#[cfg(test)]
mod tests {
    use entity_core::{Registry, Runtime};
    use entity_store::{
        Expect, RecordedCommit, Recording,
        asynchronous::{
            BatchKey, RecordPosition, RecordReceipt, original_request_comparison_bytes,
            record_comparison_bytes, record_comparison_bytes_in,
        },
    };
    use serde_json::json;

    use super::*;

    /// The kind `small_store_cost` declares, with `fields` long-named optional fields.
    fn registry(fields: usize) -> Registry {
        let mut schema = serde_json::Map::new();
        schema.insert(
            "title".into(),
            json!({ "type": "string", "required": true }),
        );
        for index in 0..fields {
            schema.insert(
                format!("alpha_field_{index:04}_carrying_a_long_declared_name"),
                json!({ "type": "string", "max_length": 64 }),
            );
        }
        let definition = serde_json::from_value(json!({
            "entity": "alpha",
            "version": 1,
            "schema": { "fields": schema },
            "lifecycle": { "initial": "open", "states": ["open", "closed"] },
            "operations": {
                "touch": {
                    "transitions": [{ "from": "open", "to": "open" }],
                    "arguments": { "fields": { "note": { "type": "string" } } },
                    "set": { "title": "$args.note" },
                    "emits": []
                }
            }
        }))
        .expect("definition parses");
        let mut registry = Registry::new();
        registry.register(definition).expect("definition validates");
        registry
    }

    fn created(fields: usize) -> RecordedEntry {
        let decision = Runtime::new(&registry(fields))
            .create("alpha", 1, "one".to_owned(), json!({ "title": "one" }))
            .expect("creation");
        let recording = Recording {
            record_id: "c".into(),
            recorded_at: "2026-09-25T00:00:00Z".into(),
            correlation: None,
            causation: None,
            actor: None,
        };
        RecordedEntry::Decision(RecordedCommit::new(decision, &recording).expect("commit"))
    }

    fn stored(entry: &RecordedEntry, subject: &Subject) -> StoredRecord {
        let position = RecordPosition {
            subject: 1,
            store: 1,
        };
        StoredRecord {
            entry: entry.clone(),
            position,
            receipt: RecordReceipt {
                record_id: entry.record_id().to_owned(),
                subject: subject.clone(),
                kind: entry.kind(),
                revision: 1,
                position,
                batch_key: BatchKey::Named("seed".into()),
                member_index: 0,
            },
            expect: Expect::Absent,
            request_bytes: original_request_comparison_bytes(entry).expect("request bytes"),
            lineage: None,
            record_bytes: record_comparison_bytes_in(RecordForm::DefinitionEmbedded, entry)
                .expect("record bytes"),
        }
    }

    /// Live heap bytes, allocator bookkeeping of 16 bytes an allocation included, that a clone of
    /// each shape retains — measured with a counting global allocator outside this workspace,
    /// which forbids the `unsafe` one needs, on exactly these records: the raw length pins that
    /// the record measured is the record built here. `(fields, raw, decoded entry, stored record)`.
    const MEASURED: [(usize, usize, usize, usize); 5] = [
        (0, 1_388, 12_897, 14_618),
        (8, 3_428, 13_393, 17_154),
        (40, 11_588, 35_201, 47_122),
        (120, 31_988, 83_097, 115_418),
        (250, 65_138, 163_781, 229_252),
    ];

    /// The cap bounds what a memory actually holds, not only the raw bytes it copied.
    ///
    /// A decoded record retains between two and nine times its raw bytes — a definition's every
    /// field is its own allocations — and the stored records a history holds retain their decode
    /// and both byte strings again. A memory that charged only raw bytes held about four times
    /// its cap before it forgot anything. Each shape is remembered as a verified blob with its
    /// decode, and as a one-record history, until the memory has cleared several times; after
    /// every step the measured retained size of what it holds must be within the cap.
    #[test]
    fn a_memory_forgets_before_what_it_actually_retains_passes_its_cap() {
        const CAP: usize = 2 * 1024 * 1024;
        for (fields, raw, entry_heap, stored_heap) in MEASURED {
            let entry = created(fields);
            let bytes = record_comparison_bytes_in(RecordForm::DefinitionEmbedded, &entry)
                .expect("record bytes");
            // The same record naming its definition is charged at least as the one that embeds
            // it: its decode holds the same definition.
            let by_digest = record_comparison_bytes(&entry).expect("by-digest bytes");
            assert!(
                by_digest.len() < bytes.len(),
                "{fields} fields: names its definition"
            );
            assert!(
                Decoded::Record(Box::new(entry.clone())).charge(&by_digest)
                    >= Decoded::Record(Box::new(entry.clone())).charge(&bytes),
                "{fields} fields: a record naming its definition is charged below its decode"
            );
            assert_eq!(
                bytes.len(),
                raw,
                "{fields} fields: not the record that was measured"
            );
            let mut memory = VerifiedMemory::with_cap(CAP);
            let mut clears = 0;
            let mut largest = 0;
            for index in 0..(4 * CAP / raw).max(64) {
                let before = memory.blobs.len() + memory.histories.len();
                let digest = format!("sha256:{index:08}");
                memory.admit(&digest, "er.eventlog.record-blob-key/1", &bytes);
                memory.remember(&digest, &bytes, Decoded::Record(Box::new(entry.clone())));
                let subject = Subject::new("alpha", format!("s{index}")).expect("subject");
                let history = SubjectHistory {
                    subject: subject.clone(),
                    origin: HistoryOrigin::Genesis,
                    records: vec![stored(&entry, &subject)],
                };
                memory.remember_history(&history, &entry_instance(&entry), None);
                if memory.blobs.len() + memory.histories.len() <= before {
                    clears += 1;
                }
                let decoded = memory
                    .blobs
                    .values()
                    .filter(|blob| blob.decoded.is_some())
                    .count();
                let retained = memory.blobs.len() * raw
                    + decoded * entry_heap
                    + memory.histories.len() * stored_heap;
                largest = largest.max(retained);
                assert!(
                    retained <= CAP,
                    "{fields} fields, step {index}: holds {retained} measured bytes over a \
                     {CAP}-byte cap, having charged itself {}",
                    memory.held
                );
                if clears == 3 {
                    break;
                }
            }
            assert!(
                clears >= 2,
                "{fields} fields: the memory never had to forget"
            );
            assert!(
                largest >= CAP / 4,
                "{fields} fields: forgets at {largest} measured bytes, far below its cap"
            );
        }
    }

    const MIB: usize = 1024 * 1024;

    /// An imported subject with no record after its anchor, whose state holds one string field of
    /// `field` bytes, and that state.
    fn imported(id: &str, field: usize) -> (SubjectHistory, EntityInstance) {
        use entity_store::asynchronous::{
            LegacyAnchor, LegacyCompleteness, LegacyOrderDeclaration,
        };
        let instance = EntityInstance {
            entity: "alpha".into(),
            version: 1,
            id: id.into(),
            lifecycle_state: "open".into(),
            revision: 3,
            fields: serde_json::from_value(json!({ "title": "x".repeat(field) })).expect("fields"),
        };
        let history = SubjectHistory {
            subject: Subject::new("alpha", id).expect("subject"),
            origin: HistoryOrigin::Imported(LegacyAnchor {
                instance: instance.clone(),
                completeness: LegacyCompleteness::AvailableEvidenceOnly,
                order: LegacyOrderDeclaration::PerKindOnly,
                evidence: Vec::new(),
            }),
            records: Vec::new(),
        };
        (history, instance)
    }

    /// A remembered imported history keeps its own copy of the anchor and of the terminal state,
    /// so a 1 MiB field is held at least twice and is charged at least that. A fixed allowance
    /// for the origin and the size of the largest record for the terminal charged 33,024 bytes
    /// for it: with no record after the anchor, the terminal was charged as an empty record.
    #[test]
    fn an_imported_history_is_charged_at_least_the_anchor_and_state_it_keeps() {
        let (history, terminal) = imported("large", MIB);
        let mut memory = VerifiedMemory::default();
        memory.remember_history(&history, &terminal, None);
        assert_eq!(
            memory.histories.len(),
            1,
            "a history under the cap is remembered"
        );
        eprintln!("an imported 1 MiB history is charged {} bytes", memory.held);
        assert!(
            memory.held >= 2 * MIB,
            "charged {} bytes for an imported history whose anchor and terminal copies each hold a \
             {MIB}-byte string",
            memory.held
        );
    }

    /// What could not fit even in an empty memory is not remembered, and does not make the memory
    /// forget what it holds. Forgetting everything and then inserting it anyway held more than
    /// the cap.
    #[test]
    fn a_history_or_blob_larger_than_the_cap_is_not_remembered() {
        let mut memory = VerifiedMemory::with_cap(MIB);
        let small = b"{\"small\":true}";
        memory.admit("sha256:small", "er.eventlog.record-blob-key/1", small);
        let (history, terminal) = imported("large", MIB / 2);
        memory.remember_history(&history, &terminal, None);
        assert!(
            memory.histories.is_empty(),
            "a history charged over the {MIB}-byte cap was remembered, holding {}",
            memory.held
        );
        let large = vec![b'x'; 2 * MIB];
        memory.admit("sha256:large", "er.eventlog.record-blob-key/1", &large);
        assert!(
            !memory.admitted("sha256:large", "er.eventlog.record-blob-key/1", &large),
            "a blob over the cap was remembered"
        );
        assert!(
            memory.admitted("sha256:small", "er.eventlog.record-blob-key/1", small),
            "remembering nothing made the memory forget what it held"
        );
        assert!(memory.held <= MIB, "holds {} over the cap", memory.held);
    }

    fn entry_instance(entry: &RecordedEntry) -> EntityInstance {
        match entry {
            RecordedEntry::Decision(commit) => commit.instance.clone(),
            RecordedEntry::Observation(_) => panic!("a decision"),
        }
    }
}
