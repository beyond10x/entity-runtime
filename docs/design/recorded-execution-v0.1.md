# Asynchronous recorded execution v0.1

Status: accepted for implementation, 2026-09-15, under ESS evolution revision 1 and Atlas ADR 0050.
Owner: story:async-recorded-contract-executor. This adds ports and an executor; it does not replace
the synchronous store-v0.2 contract, introduce a runtime into entity-core, or migrate a store.

## Existing authority and additive boundary

Reuse `DecisionRecord` in entity-core with its complete saved definition, command, subject,
revision, states, result, changed fields and ordered events. Reuse entity-store's `RecordedCommit`,
`RecordedObservation` and `Envelope` whole, including explicit null optional provenance. Reuse
the existing `Value` observation boundary; no new arbitrary property bag is introduced.
Sources: crates/entity-core/src/runtime.rs, crates/entity-store/src/lib.rs and src/envelope.rs.

Add object-safe Send futures in `entity-store::asynchronous`: separate async state reads, global
history/record/batch reads, and a mandatory recorded append port. Use boxed futures with no
generic trait methods or runtime dependency. The recorded writer has no fallback to bare decisions.
Preserve synchronous public signatures, error variants, provider behavior and persisted bytes.

Add `entity-executor` outside the kernel. It exposes create, execute with an explicit predecessor
expectation, observe and an ordered batch of those actions. It accepts a registry, async store and
caller-supplied recording metadata. It performs no implicit clock/ID generation, thread/runtime
selection or external effect. Both these new pure packages retain Rust 1.85; later Eventlog runtime
packages and their complete dependency closure move to 1.91 separately.

## Identity and retry

A `Subject` is the nonblank opaque pair (entity name, instance ID), never a UUID conversion.
Record IDs are global across subjects and decision/observation kinds within one logical store.
`BatchKey` has disjoint `SingleRecord(record_id)` and `Named(batch_id)` variants. Keys are nonblank
for a nonempty request. SingleRecord requires exactly one entry whose record ID equals its key.

Validate request shape/metadata and within-batch duplicate IDs first. Before loading materialized
state or consulting the current registry, look up the original batch and global record identities.
For an existing create, normalize/default its inputs using the saved validated definition and
compare its original complete request/result. For execute, normalize arguments under that saved
definition and compare operation, predecessor expectation, subject and all recording metadata;
do not re-execute against advanced state. Compare a complete observation exactly. Changed input,
kind, subject, expectation or provenance conflicts. Corrupt saved evidence is a failure, not a miss.

A lookup hit alone cannot prove a saved execute result. Before returning a committed retry, load
the immutable history prefix through that record and verify it from genesis or its explicit
anchor. A named retry verifies prefixes for every member subject and compares each lookup's
complete entry/receipt with its history occurrence and original batch membership. Missing,
unreachable, inconsistent or unverifiable prefixes fail; none yields replayed success. These
history reads precede any materialized-current-state or current-registry read. Advanced records
after the requested prefix do not re-decide the old command. Append's concurrent-winner recovery
applies the same integrity rule while observing one consistent authority snapshot.

An exact original named batch retry returns its immutable original receipt before current state
checks. For a new named key, any differing existing entry conflicts. If entries already exist
identically, refuse `PreviouslyRecordedBatchEntries` with their exact indices, including the
all-prior case. Never claim historical records were newly committed atomically with fresh records.
A SingleRecord retry may recover a member of a prior named batch without making a new batch claim.
Different IDs holding equal contents remain different records; duplicate IDs within one request
are refused even when identical, preserving requested ordering and receipt membership.

Empty batches are inert: no IO, key validation/consumption, fingerprint, position or receipt.
For a fresh request, apply actions in order to a local state overlay, then append once. Do not
hold a provider transaction while deciding or automatically re-decide stale caller intent.

## Complete records, comparisons and atomicity

`RecordedEntry` is a closed decision-with-Expect or observation enum; observation expectation is
its exact positive entity revision. A decision advances entity revision once, including a decision
with zero domain events. Each decision or observation occupies one physical history entry; domain
events remain nested and ordered. An observation never advances entity revision.

Create expects Absent at revision 1. Execute expects its exact predecessor. Validate the complete
decision against the saved definition and transaction-local prior state, not only instance/result
equality. A fresh mixed batch can create A, observe A@1, execute A@2, observe A@2, then create B;
an observation of A@1 after the execute refuses the whole batch. Refusal leaves state, history,
global IDs, batch claims and receipts unchanged. Append atomically rechecks all identities and
expectations; preflight lookups are an optimization, never a concurrency guard.

Compare three domain-separated encodings outside core: `er.request/1`, `er.record/1` and
`er.batch/1`. Each is UTF-8 compact JSON with recursively ordered object keys, original array order,
explicit null provenance and the existing exact scalar/number representation. Request material
binds kind, subject, expected predecessor or create version, normalized command and recording
metadata. Record material binds the whole kind-tagged existing typed record. Batch material binds
the namespaced key and ordered complete records/expectations. Compare exact bytes in this unit;
no new digest algorithm is needed. This introduces new comparison domains and never rewrites an
old persisted envelope or its canonical fixtures. Reordering/dropping/changing a batch member
conflicts even when final state might match.

## Receipts and observable uncertainty

`RecordPosition` contains distinct subject and store physical ordinals, separate from entity
revision. Ordinals are monotone and unique in their scope, may start at zero and may have gaps;
checked increment refuses overflow. Fresh members follow request order. Never infer past global
order from imported timestamps. Concrete Eventlog stream/feed coordinates are an adapter concern.

`RecordReceipt` binds record ID, subject, kind, entity revision, both physical positions and the
original batch key/member index. `BatchReceipt` binds a key and its ordered member receipts.
`StoredBatch` retains complete entries and exact comparison material alongside its receipt.

An explicit `CommitReceipt` distinguishes Single(RecordReceipt) from Batch(BatchReceipt).
`AppendOutcome::Empty` is separate; a committed outcome carries CommitReceipt and a replayed flag.
A single retry of a named-batch member returns Single with that member's ORIGINAL batch key/index,
not a fabricated one-entry batch or claim. A named batch returns the original full Batch receipt.
The replayed flag describes the response and never mutates the receipt. A receipt proves storage
acceptance at the provider's declared boundary, not an effect or projection write.

Keep old StoreError variants intact. New typed append refusals and `WriteFailure` distinguish a
proved not-committed result from `Uncertain { key, cause }`. Lost response, dropped future and
unreachable authority are not proof of rollback. Recovery queries/retries the same original
identity; an unavailable lookup is not absence. The reference provider must permit a scripted
commit-then-lost-response conformance case that recovers one original receipt with no duplicate.

## Unified history and explicit imported boundaries

`StoredRecord` contains its complete typed decision/observation, position and receipt.
`SubjectHistory` contains one ordered mixed sequence with `HistoryOrigin::Genesis` or an explicit
`Imported(LegacyAnchor)`. An anchor retains the exact EntityInstance/revision, completeness/order
declaration and available typed legacy evidence. No normal executor action creates a legacy import.

Available enveloped legacy records use a separate `ImportedRecordEvidence`, never StoredRecord or
RecordReceipt. It retains the complete existing typed decision/observation envelope, source/import
identity and original locator, and an explicit known-order value: PerKind(index) or Subject(position).
Store-global order and original batch/receipt coordinates are explicitly unavailable; they are not
filled with zero, source iteration order or newly minted receipt values. Available bare decisions
and events without record IDs remain separate typed anchor evidence and gain no invented envelope.
Only post-anchor committed records occupy the new mixed physical sequence and receive receipts.

The global record index reserves every available imported envelope's original ID across subjects
and kinds, and refuses conflicting duplicate imports. Lookup returns a closed
`RecordLookup::Committed(StoredRecord)` or `Imported(ImportedRecordEvidence)`. Fresh reuse with
different contents conflicts, and any prior imported member makes a new named batch refuse.
An exact single historical retry returns a distinct `AppendOutcome::Historical` with the original
typed evidence and explicit imported assurance, never Committed/replayed or a fabricated receipt.
Executor request matching still uses the original saved definition/metadata; when missing facts
prevent exact matching, return a named HistoricalRetryUnverifiable refusal. A later sync facade
may expose the same preserved legacy payload its old API returned while retaining this internal
assurance distinction. The first unit tests seeded boundaries; it implements no source importer.

Existing File history separates decisions and observations, so original interleaving may be
unknown; old SQL histories may lack global order. Importers later preserve available sequences and
name the missing order. They must not sort timestamps/revisions and label the result observed order.

A subject verifier outside the kernel checks subject-local unique record identities and physical
order, recording metadata, complete decisions, observations and the supplied terminal state.
Its assurance names that Subject explicitly; it does not prove store-global uniqueness.
Genesis uses the established kernel replay semantics. After an explicit anchor, verify only
the recorded suffix: validate each saved definition and re-run Execute against the previous result.
Refuse create-after-anchor, a second boundary, skipped revisions, changed subject, altered commands,
results, changes/events and false terminal state. Return VerifiedFromGenesis or
VerifiedAfterBoundary with the anchor revision; never claim to have proved imported genesis.
Keep the kernel's existing LegacyImport and execute-first replay refusals unchanged.

A separate pure verifier accepts an explicit set of subject histories and a logical-store scope
identifier. It checks cross-subject record IDs (including imported envelopes), new store-position
uniqueness/order and consistent batch membership among supplied records. Its result lists exact
subjects and boundaries covered; it claims only that supplied scope, not absence of hidden
provider records. Whole-store assurance additionally requires a provider-owned consistent complete
snapshot. The reference provider can obtain that snapshot under its own lock; later adapters must
establish completeness/fencing from actual authority. Atomic append always enforces the global
identity index independently of a caller's selected replay scope.

## ESS typed coordinate model

`ess/recorded-execution/system.yaml` and `domains/recording.yaml` are the typed coordinate view of
this new storage contract. They are authored from the decisions above, not an inferred import of
an existing wire format. Subject identity is injective compact JSON of the two-string tuple;
batch identity includes its explicit key variant; member identity pairs that key with its index.
No delimiter concatenation or UUID coercion is permitted.

StoredRecord and ImportedRecordEvidence each reference exactly one Subject. BatchReceipt owns its ordered ReceiptMembers; a
member has no meaning independently of that original receipt and references one StoredRecord.
These are explicit accepted relationships. The single terminal Recorded lifecycle describes the
presence of these storage facts/coordinate projections, not an entity's business lifecycle or a
new generic state setter. Receipt/record facts are immutable; the Subject row is a derived view.

The model deliberately covers coordinates and relations. Complete kernel payloads, nullable
recording metadata and legacy evidence continue to use the existing concrete Rust types; this
coordinate model is not claimed as their full wire schema. ESS Integer preserves integer shape.
Entity and observation revisions remain 1..=i64::MAX, as required by the existing kernel and SQL
contract; creation is 1 and execute refuses a checked increment beyond that maximum. Subject/store
physical ordinals and member indices are u64, allow zero and use checked allocation/conversion.
Preflight every per-subject/store increment for the whole batch before publishing any member.
These distinct bounds and cross-field equality/order are enforced by the Rust contract. Pin
i64::MAX, i64::MAX + 1 as a refused revision, and u64::MAX/overflow for physical coordinates.
No lossy projection or broader revision domain is authorized by model validation.

Exact comparison and coordinate encodings are normative in
[recorded-execution-encoding-v0.1.md](recorded-execution-encoding-v0.1.md). Centralized constructors
and validators reproduce each derived identity from its components, including imported subject
links and receipt member coordinates; model validation alone establishes no such equality.

## First-unit verification and later units

Implement an async in-memory reference store with ordered maps/vectors and clone/validate/swap
transactions, preserving the old MemoryStore. Use a safe standard Wake-based polling harness and
scripted pending futures; no unsafe block_on, time-based race or hidden runtime. Run tests for
object safety/Send, metadata completeness, retry-before-state/registry, mixed batches and repeated
subjects, complete record validation, global conflicts, zero-event revisions, ordered observations,
empty/duplicate/prior-entry refusal, atomic rollback, receipt recovery and anchored verification.
Include numeric boundary, same-content/different-ID and changed-definition/provenance controls.

The live first-unit evidence is registered as R-121, R-122, R-123, R-124, R-125, R-126 and R-127
in `docs/requirements.md`:
`async_ports_are_object_safe_and_return_send_futures` pins the port shape;
`retry_uses_saved_definition_and_verified_prefix_before_current_authority` pins retry ordering;
`mixed_batches_use_ordered_local_state_and_observations_do_not_advance_revision` and
`stale_observation_in_a_mixed_batch_rolls_back_every_record_and_receipt` pin transaction semantics;
`canonical_comparison_bytes_pin_exact_numbers_nulls_and_coordinate_identities` pins the literal
encoding; `exact_imported_retry_is_historical_and_missing_matching_facts_are_not_success` and
`explicit_set_assurance_names_its_scope_subjects_and_import_boundaries` pin bounded history
assurance; and `commit_then_uncertain_and_dropped_response_recover_one_effect_and_receipt` pins
observable uncertainty and same-identity recovery.

Retain a decisive red before implementation and causal mutations for identity lookup ordering,
omitted state/event changes, observation revision advancement, receipt duplication and partial
commit. Preserve existing sync tests, purity, requirement pins and format fixtures. Run affected
suites/fmt/Clippy, the real `task check` with PostgreSQL actually selected, and relevant 1.85 checks.

Later bounded owner units are Eventlog recorded adapter, explicit sync bridge, SQLite facade/import
and PostgreSQL facade/import. They consume this full contract and preserve command sessions,
indexed queries, identity locks and sequence semantics. They are not considered implemented by
the reference store. AEP owns the one planning-store migration story and the six actual cutovers.
