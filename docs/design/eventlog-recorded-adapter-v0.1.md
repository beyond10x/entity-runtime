# Recorded Eventlog adapter and explicit synchronous bridge v0.1

Status: proposed under ESS evolution revision1 and Atlas ADR0050. This selects an implementation
direction and records remaining exact format decisions; it is not ready for source implementation.
Native tenant capture and the remaining Eventlog provider qualification are required dependencies.

## Existing authority

Consume AsyncRecordedStore and Executor directly. Subject, BatchKey, RecordedEntry, StoredRecord,
StoredBatch, receipts and explicit HistoryOrigin remain owned by entity-store; complete definitions,
decisions and observations retain their existing kernel/envelope types. The accepted coordinate
model is ess/recorded-execution/, with canonical bytes in recorded-execution-encoding-v0.1.md.
Do not build another decision engine, bare-event fallback or planning-specific adapter contract.

The new IO edge is provisionally crates/entity-eventlog/. Its runtime closure uses Rust1.91;
entity-core, entity-store and entity-executor retain their established pure boundaries/minima.
One adapter contract must hold for Eventlog File, SQLite and PostgreSQL, with actual provider tests.
An implemented async reference provider is not evidence that this durable adapter already exists.

## Complete durable content and opaque identities

Store complete canonical record/request comparison content through Eventlog blobs and append a
strict versioned reference event for every committed ER record. Zero-domain-event decisions and
observations each occupy a real physical reference event. Preserve exact arbitrary-precision
number spelling, explicit nullable provenance and accepted er.record/1/request/1/batch/1 bytes.
Generic projection JSON cannot replace the canonical record content or reconstruct missing facts.

Retain accepted injective compact-JSON Subject and BatchKey coordinates without trimming or Unicode
normalization. Eventlog's physical identity limits do not narrow ER's opaque identity domain.
Any physical digest key requires a distinct explicit domain/version, original identity in durable
content and exact collision/substitution checks on read and write. Hash collision resistance is
not an injective lossless encoding. The selected proposed byte contract is
[Eventlog recorded reference encoding](eventlog-recorded-encoding-v0.1.md): six concrete blob
kinds and three reference-event kinds retain the existing ER bytes and original coordinates.
Closed decoders, independent literal fixtures and the complete contract review precede code.

Bind construction to the explicit authority tuple described below. Provision tenant identity
deliberately, never while reading. Missing identity or requested scope mismatch refuses.

## Logical scope and provider generation

The authority coordinate is the exact tuple `(logical_scope, tenant, expected_stream_identity)`.
Logical scope retains the existing entity-store rule: reject when `trim().is_empty()`, otherwise
preserve the complete original UTF-8 string. Eventlog's bounded physical TenantId is separately
provided by the caller; do not derive it by narrowing, trimming or normalizing logical scope.
Expected stream identity is nonempty opaque text compared exactly, without UUID assumptions.
This selects semantics for existing coordinates; the public adapter API and wire encoding still
need the complete format contract below.

Explicit provisioning establishes one immutable durable binding of that tuple for the tenant
generation. The caller retains the expected tuple in its authority configuration and supplies it
on later opens. Do not discover and silently adopt a different generation during open. A binding
reference and its complete canonical blob are authority; an inline binding row is derived and
rebuildable. The binding occupies an actual physical event position without inventing an ER record.
An uncertain provision must recover the exact original binding or refuse a conflicting tuple;
it must never overwrite a winner, generate another scope identity or erase conflicting history.

Opening is nonmutating: obtain existing identity, complete events and live blobs through native
capture, then compare the tenant, generation and authoritative binding. Do not call the existing
write-on-miss stream_identity method, admit projections, run DDL, initialize, recover or rebuild.
An identity-only tenant is incomplete provision, not an empty openable ER store. Missing or corrupt
binding content, mismatch, redacted history and required recovery remain explicit refusals.
Capture limits are explicit; exceeding one refuses rather than truncating the logical scope.

A successful open grants no lasting authority lease. Each complete read rechecks the binding and
generation within its own native snapshot. A different requested logical scope refuses before IO.
Each write must compare the exact generation/binding inside its append or group transaction before
publishing any ER record or index. Erasure followed by re-provision must not let an old adapter
write into, or accept, the new generation. Freeze the native mechanism proving that atomic check
before implementation; an earlier capture alone is not a write fence.

The selected tenant represents one ER scope. Complete capture must account for every authoritative
event, including explicit binding/import records; unknown foreign events refuse rather than being
filtered. Pre-uploaded orphan blobs remain permitted without becoming records. Unrelated caches,
counters, command bookkeeping and unrequested projections are not ER authority. Conflicts in the
reserved adapter coordinates refuse; absence of every unrelated auxiliary row is not required.

Provisioning is an explicit offline administrative operation. Its caller owns the maintenance
boundary excluding ordinary tenant writers and erasure across identity preparation, native capture
and binding commit. Concurrent provisioners remain supported by the singleton protocol. A runtime
open never enters maintenance or provisions implicitly. Operational integrations must establish
and test the exclusion they rely on; an input flag or a momentary empty capture does not prove it.

Admit the fixed binding projection before provisioning. Capture with that projection and explicit
limits; refuse pre-existing authoritative events except an exact matching committed provision
retry. Upload binding content, then use a fixed singleton stream, Expected::NoStream and a guard
locking the binding row. Commit the binding event and row together. Exact retries use the same
command identity and content; competing scopes conflict. Identity-only and orphan-blob crash
residue remain non-authority and need deliberate retry. Freeze exact command/binding coordinates,
recovery and import admission in the complete format design.

Read-only open requests the already-admitted binding/index projections and compares their rows to
the captured authoritative events; missing or incompatible projections refuse without admission.
For ordinary writes after binding, lock and compare the exact binding row inside the transaction.
Provider writer/publication ordering against erasure makes an old write either commit before erasure
and be removed, or observe a missing/new binding and refuse. Qualify that race on every provider.
The separate identity-preparation/binding-append gap is safe only inside the stated provisioning
maintenance boundary. Online provisioning concurrent with erasure is outside this contract; it
would need a separately reviewed transaction-local expected-generation assertion. Do not add an
all-auxiliary-state empty-tenant primitive to satisfy ER completeness.

Missing or different generation after restore is detectable by exact comparison. A backup restored
with the same generation and binding but fewer later records is not detectable from that tuple
alone. Do not claim otherwise or introduce an external monotone service implicitly. The explicit
legacy import/reinitialization protocol must state its restore authority and evidence limits.

Required cases include nonblank Unicode/control/long scope round trips without normalization;
read-only missing/identity-only/corrupt binding refusal; same-tuple provision retry and conflicting
scope provision; changed-generation open/write refusal; erase/re-provision races; unknown-event
complete-capture refusal; and exact preservation of physical positions around binding records.

### Concrete administrative provisioning and recovery

The unbound administrative adapter implements this separate object-safe capability. `Authority`
is the existing closed tuple from the encoding companion; `PhysicalRef` is the concrete coordinate
from the index companion. Neither becomes a Subject, BatchKey, StoredRecord or CommitReceipt.

```rust
pub trait AsyncBindingProvisioner: Send + Sync {
    fn provision_binding<'a>(
        &'a self,
        authority: Authority,
        context: EventlogOperationContext,
    ) -> BoxFuture<'a, Result<ProvisionBindingOutcome, ProvisionBindingFailure>>;

    fn recover_binding<'a>(
        &'a self,
        authority: Authority,
    ) -> BoxFuture<'a, Result<Option<ProvisionBindingOutcome>, ProvisionBindingFailure>>;
}

pub struct ProvisionBindingOutcome {
    pub authority: Authority,
    pub physical: PhysicalRef,
    pub replayed: bool,
}

pub enum ProvisionBindingFailure {
    NotCommitted(AsyncStoreError),
    Conflict { requested: Authority, found: Authority },
    Uncertain { authority: Authority, cause: ProvisionBindingUncertainty },
}

pub enum ProvisionBindingUncertainty {
    UnknownCommit,
    RecoveryUnavailable,
}
```

The caller explicitly prepares the native tenant identity and admits/attaches the four fixed
projections before calling this capability, under the maintenance exclusion already required
above. It supplies that exact observed identity in Authority; these methods do not initialize
identity, select credentials, admit projections, erase or enter maintenance. This separates native
administrative preparation from binding commit without pretending the preparation is atomic.
The provisioner's constructor holds the selected provider/capture capabilities and explicit capture
limits. The API's authority input is not evidence of writer/eraser exclusion.

Validate the tuple before IO, then resolve complete native authority before validating fresh
operational context or uploading anything. An existing matching canonical binding plus its exact
derived row returns its original physical coordinates with replayed=true, including after later
valid same-authority records. A valid different binding returns Conflict with both original tuples.
Missing identity, a different native generation, malformed binding/row, unknown events, multiple
bindings or inconsistent bookkeeping refuse as ProviderIntegrity; never invent a found tuple.
An identity-only tenant is eligible for an explicit new provision, but is not an openable runtime.

For new provision, validate context, encode the existing binding wrapper, and upload it under its
existing digest domain. Freeze one AppendGroup containing one er.binding schema-1 event with
data {blob:binding_digest}, stream type er.binding, stream ID singleton, Expected::NoStream,
CommandMeta.claim=None, literal idempotency_key er.binding/1, request_hash=binding_digest,
complete NewEvent name/schema_version/data and original operational metadata. This uses the previously selected command
identity, not a new digest domain. The guard locks the singleton row and checks the same authority;
the inline projector and event commit atomically. Recover the actual committed event/row before
returning replayed=false and its PhysicalRef. No predicted position is returned.

Eventlog owns event-ID allocation: NewEvent has no event_id input
(`eventlog-core/src/lib.rs:184-188`). The adapter captures the actual provider-minted
RecordedEvent.event_id from AppendGroupResult.appends[].events[] and cross-checks it with the
committed binding row/native capture, along with stream/version/global position and reference
content. Exact group retry returns those original IDs and positions. Do not invent, preallocate
or add an event-ID field to the frozen input; no Eventlog API or persisted format change is needed.

Exact physical retries reuse the frozen group. After UnknownCommit, lost response, physical
Conflict or IdempotencyMismatch, recover the canonical binding and derived row: equal originals
return replayed=true, a valid different binding returns Conflict, and unresolved authority returns
Uncertain carrying the complete original Authority. An unavailable or presently absent capture
after a possibly committed attempt does not prove rollback. Preserve uncertainty through failed
recovery or later unsuccessful physical retries; never downgrade it to NotCommitted. A safe new
physical attempt uses the same singleton stream/command and original binding bytes, and must
recover semantic authority again on fingerprint conflict. Fresh metadata does not change binding
identity and is never compared with the original metadata during semantic recovery.

recover_binding is read-only and requires no context. It returns Some with replayed=true only
after complete validation. None means an existing matching native generation currently contains
no authoritative binding; it is not a fence or proof that an earlier uncertain call cannot commit.
An unavailable recovery returns Uncertain { authority, cause: RecoveryUnavailable }; corruption
returns the explicit integrity error without claiming any earlier uncertain append rolled back.
NotCommitted applies only when this invocation is proved not to have published a binding, never
as a conclusion about an earlier uncertain invocation. Callers retain that earlier uncertainty
until an actual binding outcome or the externally controlled retirement of that generation settles it.
No automatic destructive recovery or overwrite is allowed.

Tests must cover committed-before-lost-response recovery, restart with fresh context, absent and
unavailable recovery while an earlier append is held in flight, both winner orders of competing
provisioners, exact tuple conflict, generation mismatch, malformed or missing rows, and original
physical-position preservation. Each provider must prove one binding event/row and no fabricated
ER record, batch or receipt; preparation/exclusion remains a separately tested operational boundary.

## Positions and original receipts

Use actual reference-event stream version for RecordPosition.subject and global_seq for
RecordPosition.store. Preserve physical gaps; neither coordinate is the entity revision. Import
anchors consume physical coordinates without becoming fabricated StoredRecords, so the first
post-anchor record can have a later physical position. The current verifier requires strictly
increasing coordinates, not dense renumbering (asynchronous/verify.rs:332-343).

Create receipts from actual committed events and exact original key/member identity, never from
predicted allocations. Preserve SingleRecord/Named namespaces and original receipt variants.
A later individual retry of a batch member returns its original membership. Imported evidence
without an original durable receipt returns the existing Historical assurance boundary; no new
receipt, genesis, definition or global chronology is invented for legacy material.

## Atomic inline indexes

Use concrete typed inline projections for global record identity, request/batch identity and
subject state/physical binding. Each compressed key must reproduce the row's original identity.
The append/group guard locks every relevant key, including absent keys, in stable total order;
actual member execution preserves caller order. Validate ER Expect against transaction-local
entity revision separately from Eventlog physical stream expectations. Repeated subjects observe
prior members; observations never advance entity revision.

Eventlog PostgreSQL get_for_update acquires its projection-row identity lock before the row read,
including absent keys (eventlog-postgres/src/lib.rs:1439-1484 at integration18322cbe). SQLite uses
the enclosing IMMEDIATE transaction and File its complete provider transaction lock. A plain get
is insufficient for concurrent global-ID admission. Qualify the exact race across all providers.

Inline index updates and final complete batch-index publication are atomic with all reference
events. The stored batch must reproduce exact ordered members/comparison/receipts; a matching count
does not establish completion. Restart and lookup cannot expose a partial batch as committed.
docs/design/eventlog-recorded-indexes-v0.1.md selects four closed key-only row schemas, exact
guard lock ordering, complete final-batch publication, explicit admission, nonmutating open and
administrative rebuild. Binding/events/live blobs remain authoritative; indexes remain derived.
Eventlog needs the specified asynchronous inline-rebuild capability; current catch-up rebuild
cannot supply it. ER exposes its existing pure transaction-local validator instead of copying
kernel logic. These are proposal dependencies, not implemented capabilities or qualified providers.

## Operational metadata and two retry paths

The adapter IO edge receives a closed per-operation EventlogOperationContext: subject, actor,
request_id and trace_id strings, optional causation_id, u32 causation_depth and typed occurred_at.
The shell supplies these operational facts. They are distinct from ER entity Subject and opaque,
nullable Recording provenance; never parse ER recorded_at or invent a clock, epoch, principal or
trace to fill them. Apply Eventlog's existing bounded physical/principal grammar and depth limit.

An additive operation(context) facade over the bound adapter implements existing AsyncRecordedStore
ports without adding fields to ER requests or changing Executor signatures. Reads delegate to the
bound reader. Validate/freeze context when a new nonempty physical write remains necessary, before
blob upload or append. Empty batches stay inert; already committed semantic recovery requires no
matching current context and must not be rejected merely because that attempt's context differs.
Provision/import administrative calls supply context for their possible new physical command too.

Use append_group_guarded with claim=None for provision, single/named append and import, including
one-entry groups. The binding command key is er.binding/1. Ordinary batch and import keys use the
encoding companion's K framing with respective domains er.eventlog.batch-command-key/1 over
C({authority,batch_key}) and er.eventlog.import-command-key/1 over C({authority,subject}). Their
request hashes are respectively the complete binding, batch or anchor blob digest. These are
derived retry coordinates, not substitutes for caller-supplied operational identities.

ER Expect in the batch bytes remains distinct from physical Eventlog Expected. A new physical
group captures exact subject heads: NoStream for absence, otherwise Exact(version); repeated
subjects advance according to earlier entries. The transaction-local guard still validates ER
predecessors and exact binding/global identities. Freeze this physical expectation vector and
complete CommandMeta together. An exact physical retry reuses that entire group unchanged.
Provider deduplication precedes guards/projectors and returns original positions on equal fingerprint.

ER semantic recovery follows its existing identity-first ordering and exact request/record/batch
equality. If the committed request matches, return its original receipt without another append.
A new recovery attempt's trace/time/actor is neither substituted into the original event nor
required to equal it. This includes individual recovery of a named member. Historical reads may
span many operational contexts; validate original stored facts and within-group agreement, never
compare all history with the current context. Differing semantic bytes retain existing ER conflicts.

A snapshot showing absence is not a fence against an earlier uncertain attempt committing later.
Reuse the same semantic/group/global-ID keys. Provider group serialization and transaction-local
identity guards admit at most one winner. After group, expectation or identity conflict, recover
semantic authority again: equal original bytes return the winner's original receipt, different
bytes return the existing typed ER conflict, and an unresolved winner retains uncertainty. Changed
operational context alone is not ER content conflict. No lost reply proves rollback.

Qualify exact physical retries, semantic recovery under fresh context, histories with different
contexts, named-member recovery and old-uncertain/new-attempt races in both commit orders. Assert
one physical commit, unchanged original metadata/positions, no duplicate projector effects, exact
semantic conflicts for changed content and retained uncertainty when recovery remains unavailable.
No durable metadata sidecar or separate recovery-attempt audit event is introduced.

## Complete capture and integrity

Require Eventlog's separately designed native ConsistentTenantCapture capability. Pagination ending
with has_more=false is not provider-owned completeness. Capture all events/live blobs/requested rows
under one snapshot, then cross-check committed record/batch/state indexes and resolve every referenced
blob. Pre-uploaded orphan blobs are not committed records; retain that distinction without omitting
them from generic capture or fabricating receipts. Deleted/corrupt referenced content refuses.
Redacted recorded authority cannot be repaired by merely rebuilding a projection.

A command reads per entity (amended for planning-on-ER unit R2, E-R2; pinned by R-151 and the
`per_entity_reads` and `adversary_r2_per_entity_reads` tests). The `EventlogOperationStore`
readers, the executor's batch reads and the append's preflight and post-commit check read, through
`EventStore::read_many`, the binding stream, the streams of the subjects the command names, the
blobs those events bind, and the index rows of its record ids, batch key and subjects. A subject an
index row names for one of those ids or keys is read too, and so is every subject that shares a
batch with a record read, because a batch is verified whole. The result is verified by the same
code a complete capture is. Three things are narrower than a complete capture:

- Only the index rows for the keys the command names are held to the events. A tampered row of
  another entity is not refused by a command; the next complete read refuses it.
- The provider's stream identity is asked only behind a present binding row. The Eventlog 0.5.0 port has no
  non-minting identity call and `stream_identity` mints on a miss, so a tenant forgotten between
  the two calls can still be given a new identity.
- A per-entity read is several provider calls, not one snapshot. A read that does not verify is
  taken again, and after three attempts one complete capture decides, so an honest concurrent
  writer ends as a revision conflict or success and never as a tamper verdict.

The read bound of step 4b of the errors/import companion applies to every command. A handle keeps
the counts of its last complete capture plus what its own writes added since, and refuses a
command with `BatchExceedsReadBounds` before any upload when the tenant would then hold more
events, blobs or index rows than its `CaptureLimits`. Another writer's additions are counted only
from this handle's next complete read.

The store handle's own readers, `complete_snapshot`, recorded refusals, imports, binding recovery
and the recovery after an uncertain, conflicting or reused-identity append reply still take a
fresh complete capture per call; a handle does not reuse a capture across calls. What it reuses is the verification of one. The handle keeps
the last capture it verified, whole, beside the model built from it. A later capture equal to it
reuses that model. A later capture that is it plus only events this handle's own committed appends
returned, over byte-identical blobs, advances the model by those events: each is admitted by the
whole build's code, each subject they touch is verified from the state its verified history
reached, and every projection row is held against the advanced model. Any other capture — another
writer's head, a changed or missing blob, a changed event, a changed row — is verified whole, and
an advance that refuses is answered by the whole build, so a refusal is worded as it always was.
The append's preflight per-entity read also supplies the group's expected heads: uploading blobs moves no
head, and a head another writer moves before the group commits is refused by the provider's
expected-version check and the guard.

Capture implementation and SQL integrity acceptance are dependencies; this design supplies neither.
All actual provider gates, concurrent writes, crash/restart/uncertain-response cases and mutation
controls must qualify the final adapter transaction vector, not just the generic Eventlog API.

## Explicit synchronous bridge

docs/design/eventlog-recorded-sync-bridge-v0.1.md selects the explicit entity-eventlog::sync API,
an owned dedicated OS thread/current-thread Tokio runtime, bounded fail-fast typed queue and
shared Queued/Cancelled/Dispatched/Completed phases. The owner constructs Registry/provider and
borrows the existing Executor per request. No caller-runtime block_on or arbitrary callback queue.
Validate every public AppendRequest before its unique inert empty return; Executor's empty action
batch separately ignores its key. Queued cancellation proves no invocation; lost write results
after dispatch retain original-key uncertainty and never trigger an automatic retry.

The companion fixes typed read/write/executor errors, finite queue configuration, per-call waits,
explicit drain/cancel shutdown with retained join ownership on timeout, panic-poll containment and
waiter notification. Startup has a conclusive ownership handshake and no promised filesystem
deadline. The owner needs Eventlog's validate-only attachment of already-admitted inline code.
Existing Store/StoredRuntime, CLI/MCP and provider facades need separate convergence; their older
refusal envelopes cannot silently represent uncertain writes. The new IO package/bridge remains
Rust1.91 with explicit sync-bridge/provider features, while pure dependencies retain their minima.

## Contract completion before implementation

This accepted adapter, encoding, index, error/import and synchronous-bridge contract establishes
R-151; its companion documents supply the detailed acceptance cases referenced below.

The encoding, index and sync-bridge companions select exact wire shapes, digest framing,
admission/rebuild and runtime ownership. docs/design/eventlog-recorded-errors-import-v0.1.md
selects exhaustive provider error mapping, one-shot typed guard refusal transport, the concrete
ProviderIntegrity variant, and the separate administrative imported-anchor API/protocol. Imported
boundaries reuse public verify_subject_history; ordinary transaction-local validation exposes
the existing pure validator. No fabricated Subject, BatchKey, old receipt or legacy chronology.
Context validation remains necessary only for a new physical write after semantic recovery.

The import owner must establish its external source fence, completeness/order authority and trusted
restore checkpoint. The adapter cannot prove those facts from caller data. Native capture,
validate-only attachment, inline rebuild and SQL qualification remain Eventlog dependencies.
Select exact accepted Eventlog pins after those dependencies qualify; runtime features are in the
bridge companion. Add old/new-reader fixtures before parser extensions and independently review
the complete contract before implementation. These proposals close no provider, adapter, facade
or migration outcome, and no generic in-memory test replaces actual three-provider qualification.
