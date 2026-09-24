---
format: aep.planning-md/2
id: review-result:er-recorded-contract-pass-1
kind: review-result
status: active
title: Independent review of the asynchronous recorded contract
owner: gpt-5.6-sol
relations:
- reviews: story:async-recorded-contract-executor
revision: 1
---
needs-revision

# Independent ER recorded-contract review

Reviewed proposed `story:async-recorded-contract-executor` revision 4 and its accepted design and
coordinate model against approved ESS evolution revision 1, Atlas ADR 0050, and the existing ER
kernel/store contracts at base `eaf43090636abce025649e565e5af271c4513eae`. The model's declared
relations are directionally correct: `StoredRecord references Subject` uses the source
`subject_id`; `BatchReceipt owns ReceiptMember` uses the target `batch_id`; and `ReceiptMember
references StoredRecord` uses the source `record_id`
(`ess/recorded-execution/domains/recording.yaml:50-70,76-90,96-122`). Those are accepted authored
decisions, not imported-source claims.

## Proposed-contract limitations

### ER-CONTRACT-1 — error — retry success cannot yet establish that saved execute evidence is sound

The design requires corrupted saved evidence to fail, but it also returns an exact single or named
batch retry before current-state/registry lookup (`docs/design/recorded-execution-v0.1.md:33-45,
88-99`). A `DecisionRecord` retains `from_state` and the resulting instance, but not the complete
predecessor instance (`crates/entity-core/src/runtime.rs:122-142`), and
`RecordedCommit::validate` checks envelope/result consistency without recomputing the decision
(`crates/entity-store/src/lib.rs:300-320`). The existing complete verifier can establish an execute
result only by walking from a creation through the prior instance
(`crates/entity-core/src/replay.rs:120-167`). Consequently, a global `lookup_record` hit or
`StoredBatch` hit alone cannot distinguish a valid retry from a stored record whose result, changed
fields, or events were corrupted; matching the caller's operation/arguments/provenance does not
close that gap.

Concrete correction: require retry recovery to load the relevant immutable `SubjectHistory`
prefixes and run the genesis or imported-anchor suffix verifier through every hit before returning
its original receipt. A named retry must do this for every member subject. This history read remains
before any materialized-current-state or current-registry read. Missing, unreachable, internally
inconsistent, or unverifiable history must return a typed corruption/unreachable result, never a
replayed success. If another integrity mechanism is intended, specify its concrete trusted input;
the design explicitly introduces no digest that could substitute for the history proof.

### ER-CONTRACT-2 — error — imported evidence has no non-fabricated record/identity representation

Every `StoredRecord` and `RecordReceipt` is required to carry subject/store physical positions and
original batch key/index (`docs/design/recorded-execution-v0.1.md:79-86,101-106`), while legacy File
history separates decision and observation vectors and legacy snapshots may contain no decision
records at all (`crates/entity-store/src/file.rs:74-93,600-610,929-935`). The accepted contract also
says missing subject or global order must be named rather than reconstructed
(`docs/design/recorded-execution-v0.1.md:108-110`; Atlas
`architecture/adr/0050-ess-evolution-recorded-execution.md:90-94`). If available legacy envelopes
are promoted to `StoredRecord`, assigning store position, interleaving, batch key, member index, and
receipt fabricates facts. If they exist only inside `LegacyAnchor.available evidence`, the contract
does not say how their globally meaningful record IDs participate in `lookup_record`, collision
refusal, or compatible retry recovery.

Concrete correction: define a closed imported-evidence type separate from freshly accepted
`StoredRecord`/`RecordReceipt`. It must retain the exact available envelope and explicitly encode
which subject order, global order, and batch/receipt coordinates are unavailable. Specify that its
record ID is reserved in the store-global identity index and what lookup returns for it; it must not
yield a fresh/replayed commit receipt when no historical receipt exists. The anchor remains the
state/revision trust boundary, and only post-anchor executor records receive new physical positions
and receipts. This is a contract correction for later import consumers, not a request to implement
an importer in this story.

### ER-CONTRACT-3 — error — the stated verifier scope cannot prove cross-subject global facts

The verifier is described over `SubjectHistory`, yet it is also said to check globally unique record
IDs and store-position order "in its supplied scope" (`docs/design/recorded-execution-v0.1.md:101-118`).
A single subject history cannot prove either fact across subjects. A global ID lookup also cannot
prove that a faulty provider has not hidden a duplicate elsewhere; the existing File index scan,
for example, inserts locations into a map without refusing a duplicate discovered in another
subject (`crates/entity-store/src/file.rs:351-415`). `VerifiedFromGenesis` or
`VerifiedAfterBoundary` would therefore overstate store-wide integrity if produced from only one
subject.

Concrete correction: split the assurances. Keep genesis/after-boundary as explicitly subject-local
replay assurance, and add a store-scope verification input containing all included subject
histories (or one authoritative global transcript) that checks cross-subject record-ID uniqueness
and store-position uniqueness/order. Name the supplied store scope in its result. The async
reference provider must still enforce global IDs atomically on append; verification of a subset
must not claim that global property.

### ER-CONTRACT-4 — error — revision and ordinal numeric domains are conflated

The coordinate section says Rust enforces "u64 bounds" for revision while the story only asks for
checked position/revision arithmetic (`docs/design/recorded-execution-v0.1.md:135-139`;
`.engineering/planning/story/async-recorded-contract-executor.md:65-66`). Existing ER semantics are
narrower: execute stops before a revision exceeds signed 64-bit maximum
(`crates/entity-core/src/runtime.rs:411-419`), replay applies the same bound
(`crates/entity-core/src/replay.rs:265-275`), and observations accept only
`1..=i64::MAX` (`crates/entity-store/src/lib.rs:352-374`). This is deliberate compatibility with
the shipped SQL providers (`docs/design/kernel-v0.2.md:13-17`). Physical positions may use the full
unsigned range and start at zero, so treating all of these `Integer` coordinates as one u64 rule
would admit revisions current providers cannot preserve or unnecessarily narrow positions.

Concrete correction: state and validate distinct domains: entity/observation revisions are
`1..=i64::MAX`; creation produces 1 and execute uses checked `+1`; subject/store ordinals and member
indices are explicit `u64` values with zero allowed and checked allocation/conversion from slice
indices. A multi-member append must preflight every per-subject and store-position increment before
publication. Pin exact JSON comparisons at `i64::MAX`, `i64::MAX + 1` as a refused revision, and
`u64::MAX`/overflow for physical coordinates so arbitrary-precision JSON does not blur the typed
boundary.

### ER-CONTRACT-5 — warning — canonical coordinate identities and comparison bytes lack one exact encoding

The design requires injective compact-JSON identities for subject, batch, and member coordinates
and domain-separated compact JSON for request/record/batch comparison
(`docs/design/recorded-execution-v0.1.md:67-75,121-127`). It does not fix the JSON value shapes,
tag spelling/framing, or numeric representation of the member index. Multiple encodings satisfy
that prose but produce different persisted identities and retry comparison bytes. The ESS model
also stores each derived identity beside its component fields without a rule requiring them to
agree (`ess/recorded-execution/domains/recording.yaml:30-44,76-90,96-116`), so model validation alone
does not prevent a `subject_id` or `member_id` that names different components.

Concrete correction: publish one normative byte example/schema for each of `SubjectId`, `BatchId`,
`MemberId`, `er.request/1`, `er.record/1`, and `er.batch/1`, including enum tags, explicit nulls,
array ordering, object-key ordering, and u64 decimal spelling. Centralize construction/validation so
the duplicated component fields must reproduce the stored identity. Add exact fixtures for escaped
strings, `100` versus `100.0`, `-0`, large exponents, `i64::MAX`, and `u64::MAX`; retry comparison
must preserve the recorded spelling rather than convert through `f64`.

## Pre-existing behavior, not findings against this story

- Existing synchronous `HistoryProvider` returns separate decision and observation lists
  (`crates/entity-store/src/lib.rs:207-221`); it cannot supply the new mixed physical history without
  an explicit later bridge/import boundary. The proposed story correctly keeps that compatibility
  work out of this unit.
- Existing `RecordedCommit::validate` is intentionally a structural compatibility check, and old
  synchronous provider retry behavior compares stored documents (`crates/entity-store/src/lib.rs:248-263,
  300-320`). ER-CONTRACT-1 requires the new executor's stronger verification path; it does not ask to
  change the old synchronous API or bytes.
- The current File/SQL origin markers and missing global order are the historical facts the accepted
  anchor design must describe. ER-CONTRACT-2 does not require this first in-memory unit to implement
  the later Eventlog adapter, bridge, or imports.

```findings
- file: docs/design/recorded-execution-v0.1.md
  line: 41
  category: integrity
  severity: blocker
  message: Require history-prefix or equivalent trusted verification of saved execute and named-batch evidence before a retry can return its original receipt.
- file: docs/design/recorded-execution-v0.1.md
  line: 103
  category: compatibility
  severity: blocker
  message: Define imported evidence and global-ID behavior without fabricating physical order, batch coordinates, or historical receipts.
- file: docs/design/recorded-execution-v0.1.md
  line: 112
  category: assurance
  severity: blocker
  message: Separate subject replay assurance from a store-wide verification scope that can actually prove cross-subject IDs and store positions.
- file: docs/design/recorded-execution-v0.1.md
  line: 138
  category: numeric-semantics
  severity: blocker
  message: Preserve the existing 1..=i64::MAX revision domain and define separate checked u64 domains for physical positions and member indices.
- file: docs/design/recorded-execution-v0.1.md
  line: 125
  category: determinism
  severity: warning
  message: Pin exact coordinate-ID and comparison-byte encodings and validate that duplicated component fields reproduce those identities.
```

No code, build, gate, service, Git, AEP, model, or planning mutation was performed. The prior
recorded ESS validation/compile result was accepted as stated; this review does not treat model
validation as proof of runtime behavior.
