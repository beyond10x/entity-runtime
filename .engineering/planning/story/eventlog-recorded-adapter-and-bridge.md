---
format: aep.planning-md/1
id: story:eventlog-recorded-adapter-and-bridge
kind: story
status: draft
title: Persist complete recorded execution through Eventlog with an explicit synchronous bridge
owner: Entity Runtime maintainers
refs:
- provider: atlas
  reference: architecture/adr/0050-ess-evolution-recorded-execution.md
- provider: eventlog
  reference: story:consistent-tenant-capture
relations:
- serves: vision:O2
- depends_on: story:async-recorded-contract-executor
scope:
- confidence: inferred
  path: .github/workflows/
- confidence: inferred
  path: CHANGELOG.md
- confidence: inferred
  path: Cargo.lock
- confidence: inferred
  path: Cargo.toml
- confidence: inferred
  path: Taskfile.yml
- confidence: inferred
  path: crates/entity-eventlog/
- confidence: cited
  path: crates/entity-executor/src/lib.rs
- confidence: cited
  path: crates/entity-store/src/asynchronous/
- confidence: cited
  path: docs/design/eventlog-recorded-adapter-v0.1.md
- confidence: cited
  path: docs/design/eventlog-recorded-encoding-v0.1.md
- confidence: cited
  path: docs/design/eventlog-recorded-errors-import-v0.1.md
- confidence: cited
  path: docs/design/eventlog-recorded-indexes-v0.1.md
- confidence: cited
  path: docs/design/eventlog-recorded-sync-bridge-v0.1.md
- confidence: inferred
  path: docs/requirements.md
- confidence: cited
  path: ess/recorded-execution/
revision: 17
---
## Outcome

Provide the durable Eventlog implementation of the accepted asynchronous recorded-storage ports
and an explicit synchronous bridge to the existing Executor. Preserve full records, original
receipts and verifiable history across File, SQLite and PostgreSQL. This is the shared adapter,
not a duplicate FileStore facade story or an AEP-specific lifecycle engine.

## Authority and typed home

Approved ESS evolution revision1, Atlas ADR0050 and accepted recorded-execution-v0.1.md plus its
encoding companion. Existing Subject/BatchKey/RecordedEntry/StoredRecord/StoredBatch/receipt/import
coordinates live in entity-store/src/asynchronous/ and ess/recorded-execution/. Complete kernel
record/envelope types remain authoritative. The new proposed binding design is
 docs/design/eventlog-recorded-adapter-v0.1.md; its remaining UNMAPPED encoding/runtime decisions
must be settled and independently reviewed before implementation.

## Acceptance

- Implement AsyncRecordedStore through qualified Eventlog native providers, preserving exact
  complete canonical record/request/batch bytes in blobs and strict versioned reference events.
  Zero-event decisions and observations occupy real physical events; entity revision semantics
  remain independent. No bare-event fallback, extra executor or fabricated legacy record.
- Preserve arbitrary nonblank ER identity strings, explicit scope/tenant authority and original
  global ID namespaces. Any bounded physical mapping has exact original-identity collision checks
  and independent literal byte vectors. Reads do not provision or normalize authority.
- Use actual event versions/global positions in original record/batch receipts, preserving physical
  gaps, member order and original membership on retry. Explicit imported evidence retains its
  exact limited HistoryOrigin/Historical assurance and never acquires an invented old receipt.
- Enforce global record/batch identity and ER predecessor checks inside the append/group transaction
  through typed inline indexes, including absent keys and repeated-subject overlays. Index/state/
  event/batch completion either commit together or not at all. Same-ID changed content and new named
  batches containing previously committed members retain the existing exact typed refusals.
- Implement complete_snapshot only through admitted native ConsistentTenantCapture. Compare the
  full event/record/batch/state vector and every referenced blob; no pagination completeness claim.
  Orphan preuploads remain distinct from committed records; lost/corrupt/redacted authority refuses.
- Add the explicit synchronous bridge outside the kernel with an owned dedicated runtime/provider
  thread and bounded typed queue. It invokes the existing Executor, supports caller runtime contexts
  without nested caller block_on, refuses owner-thread re-entry and preserves pre-admission versus
  uncertain-write outcomes and original retry keys. Shutdown reports actual drained/joined state.
- Qualify real File, SQLite memory/file and PostgreSQL lanes for complete zero-event/observation
  persistence, concurrency, repeated/multi-subject groups, identity conflicts, lost replies, restart,
  crash cuts, tamper/corrupt indexes/blobs, capture consistency and bridge admission/shutdown. Retain
  actual mutation-sensitive failures and exact native byte vectors rather than encoder-derived
  expectations. Required PostgreSQL lanes actually execute; a skipped lane does not qualify.
- Preserve old readers/format bytes and pure-package minima. Raise only the Eventlog-backed closure
  to Rust1.91 with exact coordinated pins and justified runtime features. Run actual task check,
  independent code examination, and relevant compatibility/MSRV checks before local integration.

## Scope

- crates/entity-eventlog/ — inferred new IO-edge adapter/encoding/projection/bridge and provider tests.
- crates/entity-store/src/asynchronous/ — cited existing ports/types/encoding/verifier; consume their
  contract, with any required API addition explicit in the final reviewed design.
- crates/entity-executor/src/lib.rs — cited sole runtime-neutral executor, not a second implementation.
- Cargo.toml and Cargo.lock — inferred exact Eventlog/provider/runtime pins and Rust-minimum boundary.
- docs/design/eventlog-recorded-adapter-v0.1.md and ess/recorded-execution/ — cited design/typed homes.
- docs/requirements.md, CHANGELOG.md, Taskfile.yml and .github/workflows/ — inferred acceptance pins,
  honest delivery and required provider/MSRV gate wiring, without publication authority.
- Confidence: high on cited seams; exact new format/module/API choices remain design work.
- Would collide with async contract, workspace dependency, provider-facade or competing adapter edits.

## Readiness and completion boundary

The first async ports/reference-provider/executor unit is implemented and locally integrated at
17da35a7b2d5ad0edfebecb9d770c0ffd558b368 with its actual gate. Native Eventlog capture is design-
accepted but unimplemented; remaining SQL-integrity independent review is unresolved. Require
accepted capture/provider implementations and exact pins before this adapter's implementation.
Separate dependency blocking is recorded rather than assuming the design approval supplies code.

Existing story:file-store-atomic-groups-from-eventlog owns later FileStore facade adoption.
Retained StoredRuntime/CLI/MCP and SQLite/PostgreSQL facades, legacy import/cutover operations and
AEP migration are later convergence owners. This adapter must support their recorded contract,
not silently claim those downstream changes completed. Local acceptance only; no release,
publication, deployment or live planning-store cutover is authorized by this artifact.

## Selected scope binding

The proposed adapter design now selects the exact existing-coordinate tuple
`(logical_scope, tenant, expected_stream_identity)`. Preserve nonblank logical scope byte for byte,
keep the separately supplied physical tenant, and compare opaque nonempty generation exactly.
Explicit provisioning establishes one immutable durable binding; later opens receive the caller's
expected tuple and never create identity, admit projections, recover storage or rebuild indexes.
Each complete native capture rechecks authority, and each write needs the generation/binding check
inside its append/group transaction. Identity-only state is incomplete provision; foreign events
refuse complete-scope claims. Binding events preserve their real physical positions.

This selects design semantics, not source implementation or final wire/API admission. The exact
claim/provision/import/recovery protocol remains unresolved. In particular, the recommendation for
a new native empty-tenant capability is not adopted merely because auxiliary operational state may
exist. The protocol must prove one-scope authority without filtering unknown events or accepting an
erase/re-provision race. Same-generation backup rollback cannot be detected from the tuple alone;
the adapter must state that limit and must not imply an external monotone authority exists.

Source basis: entity-store/src/asynchronous/verify.rs and types.rs at the accepted async integration;
Eventlog's existing write-on-miss stream_identity and the separately accepted native capture design.
The exact proposed binding, guard, byte formats and decisive tests still require completion and
independent design review. Native capture/provider dependency remains open; no lifecycle advance,
adapter build, provider operation or cutover is claimed by this section.

## Provisioning ownership decision

Select explicit offline administrative provisioning under caller-owned exclusion of ordinary
tenant writes and erasure, with concurrent exact/competing provisioners governed by one singleton
binding stream and guarded inline row. Native capture must account for all authoritative events;
unknown events refuse. Unrelated provider operational rows do not become ER facts and do not need
a new all-auxiliary-state empty-tenant scan/append capability. Reserved-coordinate conflicts remain
refusals. Integrations must establish and test their maintenance exclusion rather than accepting a
flag or observation as proof; runtime open never provisions implicitly.

The accepted append/group transaction and binding-row guard are the selected mechanism for
ordinary bound writes versus erasure. Capture/preflight alone is not a write fence. Online first
provision concurrent with erasure is outside this administrative contract and would require a
separately reviewed transaction-local expected-generation assertion. Same-generation backup
rollback detection is not claimed. Exact binding format, idempotency/recovery, import protocol and
provider race qualification remain required before implementation and acceptance.

This resolves the first-provision ownership policy in the proposed adapter design without adding
a provider primitive or weakening event completeness. The native capture and SQL review dependency
remains open; the story remains draft and no source/cutover completion is asserted.

## Selected reference encoding proposal

The companion docs/design/eventlog-recorded-encoding-v0.1.md selects six concrete blob kinds and
three reference-event kinds, retaining unchanged canonical er.record/1/request/1/batch/1 bytes.
Closed binding/recorded-entry/import-anchor wrappers carry exact original authority and subject/
batch membership. Domain-separated SHA-256 keys frame ASCII domain, NUL, checked U64 byte length
and literal bytes; matching digests never replace exact unhashed-coordinate checks. Every committed
record, including an observation or zero-domain-event decision, occupies a real reference event.
Complete capture verifies exact batch membership and actual physical receipts; imports retain their
declared evidence limits without fabricated historical receipts or chronology.

Source basis: existing asynchronous encoding.rs/types.rs/verify.rs and the accepted encoding
companion; Eventlog core bounded identities, NewEvent, opaque blob binding and atomic groups at the
accepted foundation integration. These concrete wrapper formats are coordinator-selected proposals,
not facts already implemented. Original source-analysis reports remain in local evidence, with the
escaped-subject mismatch explicitly corrected to a substitution negative. Independent Rust framing
plus system SHA-256 reproduced seven literal byte-length/digest vectors; this was not an adapter,
canonical-decoder or provider execution. Canonical decoder fixtures and complete independent design
review remain required. Existing persisted formats and semantic provenance remain unchanged.

Operational CommandMeta and stable retries, final index schemas/admission/rebuild/import operations,
bridge details, typed errors and qualified dependency pins remain unresolved. A bounded read-only
operational-context report has returned for root selection; it grants no implementation readiness.
The owner stays draft and its native capture/provider dependency remains open.

## Operational context and retry selection

Select the adapter-owned per-operation context facade in the proposed adapter design, retaining
existing ER ports, request bytes and Executor signatures. Caller-supplied Eventlog operational
subject/actor/request/trace/causation/time facts are validated only for a possible new nonempty
physical write; no library clock/default or narrowing of ER provenance is introduced. Use guarded
atomic groups with stable derived keys and claim=None for provision, single/named append and import.

Exact physical Eventlog retries freeze the full group, metadata and physical expectations. ER
semantic recovery instead returns verified original committed receipts without appending and
without requiring current operational context to equal historical context. Ordinary reads may
span many original contexts. Snapshot absence is observation, not an old-transaction fence;
group/global-ID serialization and post-conflict recovery determine the sole winner or preserve
uncertainty. Semantic byte differences retain existing typed conflicts.

Source basis: Eventlog CommandMeta and atomic_group fingerprint/provider ordering; existing ER
identity-first recovery, asynchronous ports and exact equality contract. The first local analysis
proposed an additional context-equality restriction; its preserved clarification explicitly
withdraws that unsupported restriction. Root selects the corrected rule, not the original one.
Operational-context tests remain future provider qualification. Final projection/rebuild/import
protocol, bridge details, error mapping, exact pins and independent full design review remain owed.

## Selected inline index and rebuild proposal

docs/design/eventlog-recorded-indexes-v0.1.md selects one projector and four closed key-only
projections: binding, global committed/imported record identity, complete original batch/request,
and subject state/physical head. Original authority and identities are repeated and checked against
hashed keys. Guards lock absent keys in a total order and validate repeated subjects in caller order
through the existing pure ER validator, which must become an intentional public seam. Projectors
resolve canonical references and atomically publish the final complete batch without replaying a
new kernel decision. Nonmutating open requires exact complete native capture and derived-row equality.

Admission and rebuild remain explicit administrative operations under the already selected actual
writer/eraser/reader maintenance boundary. Eventlog needs an asynchronous object-safe inline rebuild
capability that retains registration, separates active blob and shadow row namespaces, replays all
committed tenant authority under exclusive publication and atomically swaps all four row sets.
Root verified current SQLite/PostgreSQL rebuild rejects inline projectors, resolves blobs through
the shadow prefix, and PostgreSQL deliberately replays an incomplete watermarked prefix. These
source facts establish a dependency gap; no provider qualification or implementation is claimed.
Cancellation/uncertain commit must preserve existing provider quarantine/retirement semantics.

Original bounded source report: local-evidence:ess-evolution/waves/0007-er-eventlog-adapter/index-rebuild-scope-result.md.
The coordinator selected its finite proposal and made the public admin API explicitly asynchronous.
The complete adapter contract still requires bridge/error/import details and independent review;
native capture and the unresolved Eventlog SQL review remain prerequisites. No cutover or source
implementation follows from recording this proposal.

## Selected synchronous bridge proposal

docs/design/eventlog-recorded-sync-bridge-v0.1.md selects a non-clone owner in entity-eventlog::sync,
owned current-thread Tokio runtime/provider/Registry on a dedicated OS thread, and closed typed
requests through an explicit finite fail-fast queue. Per-call waits use caller-side condition
variables; worker re-entry refuses. Queued-to-cancelled proves no provider invocation, while a
dispatched write with a lost result returns existing original-key Uncertain and keeps running.
All returned async errors retain their exact types. Every direct AppendRequest validates before
empty return; Executor::batch with no actions separately retains its key-ignoring inert contract.

Shutdown closes admission before drain/cancel-queued, retains the join handle after timeout and
distinguishes Joined from still-running work. Drop requests closure without claiming completion.
Panic containment covers polling and a worker-exit guard notifies waiters. Startup waits for a
conclusive handshake and makes no universal bounded-filesystem claim. Explicit sync-bridge and
provider features use Rust1.91 for this IO edge while pure packages retain their existing minima.
Older synchronous Store/StoredRuntime and HTTP/CLI/MCP refusal surfaces require separate reviewed
convergence; this bridge cannot erase write uncertainty to fit their existing contracts.

The final scoping report remains at local-evidence:ess-evolution/waves/0007-er-eventlog-adapter/bridge-scope-result.md,
SHA256 b30de0590f9115a55324cb349d2357bf8c19367ee0f492a5877bbf1dc2b64db3.
Root independently checked the malformed-empty request boundary and current registration APIs.
Eventlog must supply validate-only attachment of already-admitted inline code on each runtime
handle, without DDL, journal recovery or persistent writes. Index bootstrap now requires this
explicit seam; current register_inline cannot substitute for it on all three providers.

The whole adapter design still requires exact typed provider/guard error and imported-anchor
operation contracts and independent review. Capture, inline attachment/rebuild and remaining SQL
qualification are unimplemented/unaccepted dependencies; this proposal claims no durable adapter
test, source implementation, migrated store or service cutover.

## Selected typed errors and imported-anchor operation

docs/design/eventlog-recorded-errors-import-v0.1.md selects the remaining finite error and import
contract. Add AsyncStoreError::ProviderIntegrity for authority damage without a defensible Subject;
retain CorruptHistory for exact recovered subject evidence. A private one-shot typed guard slot
crosses Eventlog's stable-code interface only when the returned GuardRefused and stored code/error
match exactly. Never replace a returned Backend or UnknownCommit with the stored semantic refusal.
The exhaustive eleven-variant mapping relies on qualified provider terminal semantics; post-commit
ambiguity must be UnknownCommit, and failed recovery never retroactively proves noncommit.

The adapter-specific AsyncImportedAnchorWriter consumes an Imported SubjectHistory with an empty
suffix and returns VerifiedAfterBoundary or its own typed subject-keyed uncertainty. It invents no
BatchKey or receipt. Public verify_subject_history already validates the boundary; only ordinary
validate_entry_against_state needs new public exposure. Exact immutable origin/evidence bytes,
global envelope record-ID locks, one physical anchor event and all derived rows commit atomically.
Identical retries succeed even after later suffix records; changed anchors/conflicting records
retain typed conflicts. Fresh operational context validates only if semantic recovery leaves a
new physical write necessary. No caller-set completeness enum proves the source migration fence.

External source exclusion, source-owned capture/completeness/order, global-ID uniqueness and a
trusted restore checkpoint remain migration-owner prerequisites. The same-generation backup limit
is explicit. Original source analysis is local-evidence:ess-evolution/waves/0007-er-eventlog-adapter/adapter-errors-import-result.md,
SHA256 88ba93c3b2b8c745e591c8a28507712a1a444a9acdf1025b0095af5d401ec73b.
Root selected the proposal, made the administrative trait Send+Sync and corrected the context
wording to preserve the already selected identity-first semantic recovery. Complete independent
contract review precedes implementation; provider dependencies and actual full gates remain open.

## Complete design review pass one corrections

The immutable review-result:recorded-adapter-design-pass-1 records four introduced design
blockers. Root amended the actual selected contracts in response; no implementation is claimed.

- docs/design/eventlog-recorded-adapter-v0.1.md now defines AsyncBindingProvisioner with concrete
  provision/recovery methods, original Authority recovery identity, actual PhysicalRef outcome,
  typed conflict/uncertainty and complete retry rules. Explicit identity preparation and projection
  admission remain outside this non-initializing capability under caller-established maintenance
  exclusion. The existing er.binding/1 command identity is preserved. Lost replies, presently
  absent snapshots and failed recovery never prove rollback of an earlier uncertain invocation.
- docs/design/eventlog-recorded-indexes-v0.1.md defines every members[].record_key as exactly the
  existing er_records_v1 key derivation, recomputed from original authority/record_id on every
  read, write and rebuild. No additional digest domain or persisted format is introduced.
- The same index contract explicitly types all four persisted revision sites as ErRevision in
  1..=i64::MAX, matching existing asynchronous/verify.rs:22-30. Readers, guards, projector and
  rebuild refuse out-of-domain values; physical positions remain their separate u64 domain.
- docs/design/eventlog-recorded-sync-bridge-v0.1.md defines repeated shutdown through Closing
  and Joined, monotonic Drain-to-CancelQueued escalation, retained handle/mode, one provider
  retirement and join, cached terminal result, and Drop/worker-failure behavior. Deterministic
  barrier tests cover repeated timeout, mode changes, provider failure and exact once retirement.

All four findings changed the named design artifacts. The original report remains unchanged.
The same independent reviewer must run the second/final complete technical review over the
revised five documents. Source implementation remains blocked on accepted provider prerequisites;
this revision does not clear native capture, inline attachment/rebuild or SQL qualification.

## Final design finding and coordinator correction

The second/final independent review-result:recorded-adapter-design-pass-2 resolved all four prior
findings and introduced one: the proposed binding/import freeze included a nonexistent NewEvent
event ID input. Actual findings ledger is carried0/new1/resolved4; both original reports remain
unchanged. The two-round design review budget is complete; no third review is dispatched.

Root verified accepted Eventlog18322cbe: eventlog-core/src/lib.rs:184-188 declares only
name/schema_version/data on NewEvent; AppendGroupResult.appends carries AppendResult.events,
and RecordedEvent holds the provider-minted event ID. File lib.rs:242 and PostgreSQL
atomic_group.rs:150 demonstrate provider allocation. These are read-only source facts, not new
provider execution or SQL review acceptance.

Root corrected both binding and import contracts to freeze only real inputs, then capture and
cross-check returned provider event IDs with authoritative records, physical coordinates and
derived rows. Retries retain the original ID. The ordinary append path uses the same existing
group input/output types; no new Eventlog field, wire format or persistence change is needed.
The complete selected-design search found exactly the two erroneous freeze sites, both amended.

This is a coordinator-verified correction after the final independent review, not an invented
approve/findings[] verdict. No independent third review occurred. The final original report remains
needs-revision with that finding; its separate fixed outcome records this concrete correction.
Native capture, SQL qualification, inline attachment/rebuild and final accepted dependency pins
still block source dispatch. Code/provider verification and final full gates remain required.

## Reviewed Eventlog administration seam adoption

Eventlog's inline-projection-administration design completed both technical design rounds; final
review-result:inline-projection-admin-design-pass-2 approves with no findings and resolves the
first three. Adopt its concrete seam in eventlog-recorded-indexes-v0.1.md and the sync bridge.

Validate-only attachment is global structural admission and in-memory installation. It preserves
provider dirty markers and existing projection-use/capture refusal, rather than pretending SQL
has a portable dirty registry bit. This corrects the original attachment wording without allowing
the adapter to serve stale or dirty state: startup/open still requires complete native capture
and exact independent row comparison. Redacted authority still refuses before ER rebuild.

Explicit rebuild selects the registered projector by name and invokes that actual instance,
retaining exact spec validation, native snapshot, active blob/shadow row separation and atomic
all-table publication. No caller-supplied alternate Arc substitutes code under the registered name.
Provider registration coordination remains held throughout the operation. This is the concrete
provider choice refining the earlier unimplemented Arc-taking seam, not a new ER trait or format.

The reviewed provider design does not qualify its implementation. Native capture, SQL-integrity
acceptance, inline-admin code/provider proof and exact accepted source pins remain blocking.
No source adapter, full gate, migration or third ER design review is claimed by this amendment.
