---
format: aep.planning-md/2
id: story:eventlog-recorded-adapter-and-bridge
kind: story
status: implemented
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
  path: crates/entity-sqlite/Cargo.toml
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
- confidence: cited
  path: eventlog:crates/eventlog-file/
- confidence: cited
  path: eventlog:crates/eventlog-postgres/
- confidence: cited
  path: eventlog:crates/eventlog-sqlite/
revision: 39
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
record/envelope types remain authoritative.

The five Eventlog adapter/encoding/indexes/sync-bridge/errors-import design companions are selected
and have completed both design examinations. The final review's nonexistent NewEvent ID input
finding was corrected against actual provider-minted IDs; its separate fixed disposition is in
the journal. The accepted inline-admin seam was subsequently adopted in index/bridge documents.
No third design review or unresolved wire/API selection is required. Source implementation and
provider-dependent acceptance still wait for qualified native inline administration.

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

SQL integrity is accepted/integrated at8746a693c6084ab516170278a72c6c8cf5c8e46c. Native consistent
tenant capture is accepted/integrated atd016adb0c1f8177657c64d00e9e7bdff80bd1d5b, exact tree
f1298d1a3b9f9c0947eff4eadcf7f0c4a8f871f8. Both required provider gates and signed common receipt
verification pass; actual PostgreSQL17.6/TLS production proof224passed/0failed/0skipped/no missing
required cases. All capture final review findings corrected; review budgets are closed. Existing
SQL review recovered and its finding corrected; no new or equivalent denied SQL review required.
Receipt local-evidence:ess-evolution/waves/0011-provider-capture/source-correction-2/integration.md.

The combined provider dependency remains OPEN only for complete inline projection administration:
validate-only attachment and actual registered-projector rebuild, their acceptance and exact accepted
dependency pin. Existing design completed both passes and is already reflected in the ER24d31cf1
index/bridge documents. Administration source is active on the qualified capture base. Its current
tests do not yet constitute accepted source/full provider qualification.

This supersedes earlier present-tense assertions that capture is unimplemented or SQL review missing.
When administration is accepted, clear this existing blocker and dispatch the whole adapter/bridge
implementation against that exact source; no new design review. Pure service boundary design can
proceed independently. No facade, migration command or real cutover is claimed complete.

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

## Current provider readiness and next dispatch

SQL integrity is accepted/integrated at8746a693c6084ab516170278a72c6c8cf5c8e46c. Native consistent
tenant capture is accepted/integrated atd016adb0c1f8177657c64d00e9e7bdff80bd1d5b, exact tree
f1298d1a3b9f9c0947eff4eadcf7f0c4a8f871f8. Both required provider gates and signed common receipt
verification pass; actual PostgreSQL17.6/TLS production proof224passed/0failed/0skipped/no missing
required cases. All capture final review findings corrected; review budgets are closed. Existing
SQL review recovered and its finding corrected; no new or equivalent denied SQL review required.
Receipt local-evidence:ess-evolution/waves/0011-provider-capture/source-correction-2/integration.md.

The combined provider dependency remains OPEN only for complete inline projection administration:
validate-only attachment and actual registered-projector rebuild, their acceptance and exact accepted
dependency pin. Existing design completed both passes and is already reflected in the ER24d31cf1
index/bridge documents. Administration source is active on the qualified capture base. Its current
tests do not yet constitute accepted source/full provider qualification.

This supersedes earlier present-tense assertions that capture is unimplemented or SQL review missing.
When administration is accepted, clear this existing blocker and dispatch the whole adapter/bridge
implementation against that exact source; no new design review. Pure service boundary design can
proceed independently. No facade, migration command or real cutover is claimed complete.

# Start adapter source work while provider acceptance remains gated

Observed 2026-09-16T09:25Z. Approved M2/§3, existing
story:eventlog-recorded-adapter-and-bridge; no new product requirement or review unit.

The provider milestone remained open because administration source examination was
interrupted by platform review, despite implemented native capability and complete
original provider checks at43ceaa09. Waiting for that examination before writing any
adapter code unnecessarily serialized independent implementation. Its result is required
for provider/adapter acceptance, not to type-check source against the already concrete
interface. SQL and capture are accepted; the adapter design's two passes are closed.

Scheduling amendment: permit the original whole adapter/bridge implementor to work
against frozen administration candidate43ceaa09ceec610e25891815e33e03e8df92ee28 and
accepted ERda5d368756f5a63e4b2efd5589f7bc3441cd7aff. Native checks may execute as candidate
integration evidence. No unaccepted provider pin qualifies M1/M2, and no adapter source
is integrated or declared accepted while B-ADMIN-REVIEW remains unresolved. If the
provider changes, update exact pin and rerun affected/composed gates before acceptance.
The existing dependency blocker remains OPEN; this does not clear it or waive any gate.

This assignment implements the adapter's accepted interface. It does not examine the
denied administration source diff, use the interrupted reviewer's tests, replace that
review with consumer tests, or retry/switch models for denied work. The provider report,
review interruption and external resolution remain independent retained facts.

Whole source/check deliverable and stopping condition remain implementation-contract.md.
The exact author tree/pins/build capacity and source boundaries are supplied separately.
Shared async framing/helpers stay serialized at integration; no backend substitute,
publication, real migration, facade completion or cutover is implied.

This supersedes only that contract's prior source-dispatch wait for an accepted admin
pin. It preserves accepted-provider and exact final-vector requirements at qualification
and local integration. Root alone owns AEP and milestone closure.

## Complete adapter dependency and register composition

Original M2 all-feature resolution found incompatible native SQLite link identities: entity-sqlite rusqlite0.40 versus frozen Eventlog candidate0.37. Root aligned the consumer to0.37 bundled, retaining the exact provider pin. Full existing entity-sqlite/workspace/MSRV gates must verify behavior. No provider modification/review retry or acceptance waiver. Manifest is recorded in scope; stop at coherent original graph and original gates.

Root applied exact R-151 row and minimal accepted-design traceability anchor without changed contract semantics. Evidence: local-evidence:ess-evolution/waves/0007-er-eventlog-adapter/implementation/coordinator-dependency-composition.md. Whole source was ready; compilation/checks now own the bounded one-job lane. PostgreSQL/task gate awaits heavy capacity; provider independent acceptance remains blocked.

## Adapter author closed, original acceptance remains red

Authorreporteb3e873d94504126b71eeccaf42fc6dd6813d5f649c6e5a7e03c23aac6aea52e: finaltree-localtaskcheck0, actualPG12 andadapter6+5, runtime1.91Clippy0, pure1.85build0, allprocessesterminal/leaseended. The512figure is requirements-scanner functioncount, not actualexecuted count; coordinator-runner-counts.json records printedrunnerblocks.

Original providerfault/race/restart andbridgebarrier acceptance incomplete, so no source/claim acceptance orintegration. Fullsource frozen in closed-author-source.sha256. Fresh separatelybounded assignment completesall originalmissing classes in onecontract via public-seam delegatingrealprovider harness wherepossible; noEventlogsource widening oradminreviewretry. Contract local-evidence:ess-evolution/waves/0007-er-eventlog-adapter/remaining-acceptance-brief.md. M5ownsreleasedheavy lane; this assignment sourceonlyinitially.

## Complete consumer delivery submitted; independent source review queued

The complete consumer source remains frozen at a973af1cfcbaf7c06420ca5c7cf503aa98144d04 for its first independent source examination, now CLOSED with two introduced binding-recovery blockers in review-result:recorded-eventlog-adapter-source-pass-1. Exact report766a060fd4254be0c2c3615e49c9be6d11ae3ed8fd5431993d2a622c413ed709 and test43e8501a are retained. The regressions demonstrate UnknownCommit hiding a conclusive foreign Conflict, and a foreign conflict returned before validating the required derived row. Both isolated cases and the complete adapter package reached intended failures101. Forty-eight existing functions remained green, but two PostgreSQL functions returned at environment guards; that review run is not PostgreSQL provider execution. Prior actual provider gate evidence remains separate.

One complete first-review correction is active under source-correction-1-brief.md in the original author tree; root copied the exact reviewer test and pinned R-151. It covers both classification defects as the full binding recovery class, preserves uncertain outcomes where recovery is inconclusive, and retains the original checks/real provider/full gate obligations. Source-only initially. One final whole source examination remains after source and original acceptance are complete.

A separate original bridge acceptance gap is now explicit: sync-bridge design lines446–487 require the actual worker-loop barrier matrix, whereas tests at sync.rs1428–1545 directly seed/clear shared state and call finish. Those tests prove components but not actual queued/dispatched futures, retirement counts or no stranded waiters. The prior acceptance statement overstated that boundary. Exact bounded deliverable and stopping condition local-evidence:ess-evolution/waves/0007-er-eventlog-adapter/bridge-worker-acceptance-gap.md. That assignment is queued after the current binding correction closes, not added to its worker scope; it remains this M2 owner and uses the same one final source-review budget. No new public hook/provider primitive or product requirement.

Four native provider-private stage gaps and blocked administration examination remain unchanged. No denied review retry. Provider qualification and composed M5 record/request4 gate still precede adapter acceptance/integration.

## Production bridge acceptance closed; accepted-target composition

The complete original bridge-worker acceptance author assignment closed at candidate424a788a2b1e0f7cb5d9cff91ba90ce3d0c80b7c (tree bfe77e30ba04a66e45479e9d63acd96ccc36fb5c). Full task check with assigned actual PostgreSQL exited0; the original fifteen-row matrix, strict Rust1.91, three compiled red/restored controls and all twenty source hashes are retained. Exact report local-evidence:ess-evolution/waves/0007-er-eventlog-adapter/bridge-worker-acceptance-completion/report.md SHA670d1180b77341a6ce269fe8e9c485d2b4e9d924d8762dc6d05882d4579b530c. Root verified source manifest c850871a and gate log daaafefa, froze through bot tooling and verified signed common evidence. Both author/committer are the organization bot. The original author assignment is closed, not extended.

Original M2/M5 composition remains: adapter candidate424a788 descends from da5d3687, while canonical ER accepted250f699 includes service/3 operation fulfillments and record/request4. Root merged the accepted target into the retained candidate without committing; only docs/requirements conflicted, resolved by retaining R151/R152/R153. The shared verifier combines the public adapter seam with unchanged accepted fulfillment replay. Three legacy adapter ExecuteRequest fixtures now explicitly supply empty fulfillments. Neither independent gate alone proves the composition.

Fixed remaining deliverable under this same owner: actual adapter/provider service3 action/retry/replay/reopen acceptance, original minima and full actual-PG gate on the complete composition, then the one final whole adapter source pass2of2. Contract local-evidence:ess-evolution/waves/0007-er-eventlog-adapter/service3-composition-contract.md. No new review unit or reopened accepted-target review. Root owns merge/shared docs and candidate freeze. The four native provider-stage gaps and B-ADMIN-REVIEW remain explicit; no qualified integration, cutover or publication is claimed.

## Complete accepted-target composition candidate

Service3 composition author assignment closed at3190afe02abd2685613c31512c89ba47dba706e0, treed8aa5e9e6774efcb36b6380ff18eda9b2025294f, parents424a788a2b1e0f7cb5d9cff91ba90ce3d0c80b7c and accepted250f6993181822ab1e36c17d38dbc909d084d423. Exact report local-evidence:ess-evolution/waves/0007-er-eventlog-adapter/service3-composition-acceptance/report.md SHA811768b60e6f0b022fdd68b8640fc400abb0d52fd7f7f675b120f88809f594d9. Source21manifest4ee16253 verified; full actual-PG task check log10df31441a exit0, pure1.85 workspace excluding runtime adapter exit0. Five actual File/SQLite/PostgreSQL/bridge cases prove complete actions, original-key retry/conflict, record/request4 bytes, atomic batches, verified history and reopen. Action-loss mutation compiled/red101 and exact restored selector0. No production composition change beyond accepted-target merge was needed.

Root committed the complete merge through bot tooling, checked both bot identities, verified signed common receipt and clean candidate tree, and released temporary lease. This candidate is locally frozen but not canonical qualified adapter integration: B-ADMIN-REVIEW and four provider-private stage gaps remain. Canonical source remains accepted250f699 and its planning lineage stays here.

The existing second/final whole adapter source examination is dispatched independently against this entire composed candidate, with baseaccepted250f699. First source report and exact reviewer test remain retained; no third source pass, new review unit, accepted-target review reset or denied administration work. Source review2 candidate details and stopping contract: local-evidence:ess-evolution/waves/0007-er-eventlog-adapter/source-review-2-brief.md.

## Final whole source review and active F3 correction

Immutable review-result:recorded-eventlog-adapter-source-pass-2 records report4e8b6ba45776f4b11dd80d3d06f987dca6d2099cbfd568e35dfe339ba976f631. F1/F2 are resolved; F3 identifies admission after terminal drain during autonomous worker panic. The original review gives source reasoning and a compiled shared-caller probe, not an executed narrow failure; safe shutdown/Drop races are excluded. Both whole source reviews are closed; no third pass.

Original adapter author owns the bounded final correction: coordinate admission/terminal drain, demonstrate a deterministic driver-panic regression and causal control, retain complete existing bridge/binding/service3 behavior, and run the final actual-PostgreSQL gate. Contract: local-evidence:ess-evolution/waves/0007-er-eventlog-adapter/final-source-correction-brief.md. Qualified integration remains blocked by the separate administration review restriction and outstanding native provider stages; consumer tests do not clear them. No provider source or equivalent denied review retry is authorized by this correction.

## Final source correction verified and frozen

F3 author assignment CLOSED at local9769cc59e0185f8b3320d5c4e7ceda19958c11ed, treefe0920bf3f6e5235d2f88ef53f2c846e0a468a8b. Reportc8ee43a4 and exact sync sourcea7f29620 reverified; botcommit/common receipt verified, candidate clean. Deterministic driver-panic regression/control/restored0/101/0; full actualPostgreSQL taskcheck0 with required bridge/binding/service3 suites. Exact receipt local-evidence:ess-evolution/waves/0007-er-eventlog-adapter/final-source-correction/coordinator-freeze/receipt.md.

Both whole source passes are closed and final F3 correction has its required evidence; no third review. Canonical ER remains250f699. B-ADMIN and four native provider-private fault stages still prevent qualified provider/adapter integration; facade/import acceptance remains part of full M2. SDK source implementation may consume the frozen candidate explicitly, without claiming qualified integration.

## Final provider qualification and integration

# Remaining native qualification for the existing adapter outcome

Owner: existing active story:eventlog-recorded-adapter-and-bridge, canonical planning tree entity-runtime/ess-evolution-er-eventlog-adapter-20260915. Root owns planning and final provider/ER integration; one Sol/high implementor under the recorded quota fallback, no children. This is one finite assignment after the administration correction assignment CLOSED, not an extension of that assignment or a new review project.

## Requirement and demonstrated evidence gap

Approved ESS-EVOLUTION.md sections2/3 and storage acceptance require atomic rollback, idempotency, crash/reopen and original receipts. Accepted ER docs/design/eventlog-recorded-errors-import-v0.1.md:384–392 explicitly requires failure after event append, projector row, group bookkeeping and response boundaries with no partial imported/global identity set, and recovery after both commit and rollback. Original remaining-acceptance/adapter-acceptance-completion.md and source-review-2/report.md identify four unexecuted provider-private stages: individual event/group persistence, private idempotency/group bookkeeping, immediately before native commit, and immediately after native commit before reply construction. Public delegating wrappers exercise whole-call uncertainty; they cannot establish those private crash points. Existing File journal process-death tests exercise generic journal values, not a complete provider group with its receipt/bookkeeping. Retain all their evidence for what it proves; do not replay or replace unrelated suites.

## Bounded deliverable

Close only those original native stage gaps across actual File, SQLite and PostgreSQL, using the smallest native tests and existing process/fixture conventions. Demonstrate full group/event/projection/idempotency consistency on reopen and exact original retry coordinates after a committed lost reply; demonstrate absence of partial state and fresh retry after noncommit. Include repeated-stream/group members where necessary to distinguish a partial prefix. Assertions must observe provider behavior, not just confirm that a test hook fired. Different providers need not pretend to have identical physical persistence steps; identify the actual atomic boundary and account for every listed stage there.

Reuse existing tests and evidence when they directly prove a stage. Test-only private checkpoint seams are permitted when actual source shows no existing controllable seam, using the existing local child-process pattern rather than a generic framework. Keep them cfg(test), no public feature/API, no production behavior, format, schema or durable byte changes. No broad audit, invented domain, new provider, timing/deadline changes, benchmark, refactor or new review round. If a real production defect appears, preserve the failing case and return the concrete required correction to root; do not silently widen this assignment.

## Source custody and execution

Reuse clean managed eventlog/ess-evolution-inline-admin-implementation-20260916 at accepted daf6b814b94ec34fd0db57cd35c64b3be86677ae. Independent administration review ACCEPT is now closed; immutable accepted source remains in its separate clean reviewer and integration trees. Acquire/heartbeat/release own lease ess-evolution-native-qualification-sol-20260918. No worktree creation, other-tree mutation, planning writes, commits, publication or real store access.

Write scope: additive native tests in crates/eventlog-{file,sqlite,postgres}/ and the smallest cfg(test) stage call sites in the relevant native group/transaction/journal code. Add required proof-roster entries only if those new decisive cases otherwise would not run in the existing full gate. No dependency/manifest/public interface changes. Root will review the complete bounded patch and owns the final source freeze.

One build lane is granted, maximum2 Cargo jobs, one test thread, debug/incremental0, locked/offline, tree-local target, no shared cache override. Inspect capacity before builds; global8GiB hard stop, no second heavy lane below20GiB. Use source-only work and File/SQLite execution first; root supplies a fresh disposable PostgreSQL17.6/TLS fixture when needed. No claim of complete acceptance without actual PostgreSQL. Do not provision or touch another owner's fixture.

Preserve raw command outputs and own exits under waves/0007-er-eventlog-adapter/native-qualification-20260918/. Required checks: meaningful failing fault-sensitivity control then restored green for the added behavioral assertions, focused native cases, affected provider suites, fmt/strict Clippy, final full Eventlog gate and production proof with no required missing/skipped lanes. Root must first compose any necessary final shared changes so full gates run once on the final vector. Notify root when ready for that final freeze; this is a checkpoint within this same assignment, not a new task.

Stopping condition: all listed original native stage gaps have direct source/case evidence and required checks pass, or one concrete implementation/execution blocker is demonstrated and returned. Return one complete source/requirements/check/custody report, release own lease, then CLOSE. No automatic follow-on assignment. Root then selects the qualified provider pin and completes existing ER integration/gates; author completion alone is not a milestone.

## Canonical source composition before final provider selection

# ER final integration preparation

2026-09-18T22:42Z. With administration accepted atdaf6b814, root advanced the existing clean `integrate/ess-evolution-er-20260915` branch from17da35a7 to the already accepted ER13b8a1faddcbd368ca636606efa9e4927a9221c8, fast-forward only. Verified ancestry includes original adapter9769cc59 and coherent SQLite compatibility6a960962. Retained13b8 four-source hash manifest matches in the integration checkout; its signed common receipt verified independently. Earlier full actual-PG task check and source acceptance remain the evidence for that unchanged source. No new build, source patch, planning-journal copy or worktree.

This drains accepted source composition into the canonical integration branch but does not close M2. Its manifest still pins Eventlog6d5e249. Root will select the qualified final provider revision once after existing native stage acceptance completes, update the Git pin and lock coherently, then run required final ER task check/actual PostgreSQL/runtime1.91/pure minima/common checks. No provisional CPU patch from the separate dirty ER author tree was copied. No new review, optional optimization or accounting work started.

Root holds ess-evolution-er-integration-root-20260918 on the integration tree and the separate canonical ER planning lease; native worker owns only its Eventlog author source and sole build lane. Completed administration build/fixture remains retired; no new PostgreSQL fixture until native worker is ready. Whole source/gate acceptance, publication and real cutovers are not inferred from a fast-forward.

## Provider acceptance and final runtime integration

# Provider acceptance and local integration

2026-09-18T23:08Z. M1 implemented/verified/integrated = yes/yes/yes for the approved local provider acceptance. Root fast-forwarded the existing Eventlog integration branch to `f802eb8b01b44ba04a93394b20f0c07391f7757a`, parent accepted administration `daf6b814b94ec34fd0db57cd35c64b3be86677ae`. The integration tree is clean and all six entries in `root-freeze.sha256` match there. Signed common check and verify exited zero; `common-receipt.json` is retained beside this receipt.

The administration source review remains ACCEPT with both original passes closed. The bounded native assignment is CLOSED GREEN: [report](report.md), full Eventlog gate and production proof both zero, PostgreSQL 17.6 with actual TLS, conformance_valid=true, no missing required cases and no failed/skipped cases. The retained full logs, source manifest and causal fault controls distinguish actual native recovery from merely reaching a hook. The proof predates the commit and records source_dirty=true; the frozen six-file source equals the committed and integrated source. This is local acceptance, not publication or deployment capacity admission.

Root verified no active compiler/test process and accepted the worker's released custody. Scoped `cargo clean --target-dir` retired 1.3GiB of task-owned compiler output; source and all acceptance evidence remain. An earlier forced-removal command was rejected before execution; it was not retried. No managed tree was removed and no new tree was created. Provider fixture remains root-owned for immediate ER acceptance.

M2 now selects this provider revision coherently in its four manifests and lock file, with no other dependency upgrades. Its final task check is running in the existing ER integration tree, using the real retained PostgreSQL fixture and exported migration URL. M2 is not accepted until its full gate, optional-provider acceptance, supported pure minimum and common checks pass on that final vector. No third source review or new prerequisite is introduced.

## Complete local integration acceptance

# M2 runtime adapter, facades and explicit imports — accepted locally

2026-09-18T23:11Z. Implemented / verified / integrated = yes / yes / yes. Root committed the final qualified dependency selection on the existing `integrate/ess-evolution-er-20260915` branch as `8b1757365f628338cf697f53deb9ce76acd25a99`, parent `13b8a1faddcbd368ca636606efa9e4927a9221c8`. Provider is qualified Eventlog `f802eb8b01b44ba04a93394b20f0c07391f7757a`. Integration checkout is clean; all five final manifest/lock hashes in `source.sha256` match after commit. No path override, other dependency update or provisional CPU patch was adopted.

## Acceptance evidence

| Approved existing obligation | Acceptance retained |
| --- | --- |
| Complete asynchronous recorded adapter and explicit synchronous bridge; records, receipts, retry, conflict, fault recovery, binding and service composition | `task-check.log` / `task-check.exit`: full `task check` exited zero, including all-feature entity-eventlog tests and strict Clippy on Rust1.91; File/SQLite and real PostgreSQL provider, bridge, fault and service composition tests executed. Original adapter source reviews and accepted corrections remain closed. |
| Compatible SQLite/PostgreSQL facades and explicit legacy acquisition/import, caller-owned transport and complete receipts | `facades.log` / `facades.exit`: Rust1.91 all-feature entity-postgres/entity-sqlite acceptance exited zero. PostgreSQL migration/app URLs and CA were exported from the owned live TLS fixture, including the separately required migration URL; no unavailable-fixture branch substitutes for acceptance. |
| File facade, multi-subject atomicity, imports and existing-only ordinary open | Full entity-eventlog all-feature gate above includes facade and native-provider acceptance. Original facade source examinations and final-open correction are accepted; their independent regressions are retained unchanged. Provider's newly closed native group crash matrices supply the original private-stage gap evidence. |
| Independent pure library minimum | `pure-minimum.log` / `.exit`: Rust1.85 locked/offline workspace build excluding entity-eventlog exited zero. |
| Common source checks and integrated source identity | `common-check.exit` and `common-verify.exit` zero; signed `common-receipt.json`. Commit changed only four manifests and Cargo.lock; post-commit hashes equal the verified source. |
| Changed adopter documentation | No website source changed in this final pin selection. Earlier accepted facade/open correction includes its successful site gate; unchanged evidence retained in `../final-open-correction/coordinator-acceptance.json`. No redundant site prerequisite added. |

All original M2 source reviews are CLOSED; no third examination or reset. Provider acceptance is linked in `../native-qualification-20260918/integration.md`. Final local integration closes the existing adapter, facade/import and FileStore atomic-group product outcome, not merely an author assignment. No release/publication, live-store migration, downstream SDK/Connectors qualification or final ESS accounting acceptance is claimed.

Next ordered outcome is existing M3 public WriterControl and migration command integration. The confirmed local operator-controlled writer model supplies design facts; actual stop/drain/no-restart custody must still be established before any real store activation.
