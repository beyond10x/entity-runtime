# Eventlog adapter errors and imported-anchor operation

## Scope and source pins

Status: coordinator-selected proposal under story:eventlog-recorded-adapter-and-bridge. Complete
independent design review precedes implementation; no provider, adapter, migration or bridge
acceptance is recorded by this page. Original scoping evidence is retained unchanged externally.

The conclusions below use:

- Entity Runtime source17da35a7b2d5ad0edfebecb9d770c0ffd558b368 (abbreviated **ER**).
- Eventlog accepted source18322cbe19f0068e9b3fab84d874d6abea181f3e (abbreviated **EL**).
- The selected adapter, encoding, index and sync-bridge companions, which remain proposals.

## Decisions

1. Add one ER-owned integrity error for authority damage that cannot honestly name a `Subject`:

   ```rust
   AsyncStoreError::ProviderIntegrity {
       provider: String,
       detail: String,
   }
   ```

   This is a concrete variant with fixed fields, not a property bag. `CorruptHistory` remains for
   corruption attached to an independently recovered exact `Subject`; it already requires that
   subject (`ER/crates/entity-store/src/asynchronous/types.rs:653-776`). `Backend` remains an
   operational provider failure, and `Unreachable` remains inability to observe authority. An
   unknown event, broken binding, projection-shape contradiction, unresolvable referenced blob,
   compressed-key substitution, or impossible callback/result pairing is `ProviderIntegrity`
   until a subject has been recovered from exact authority. Once the adapter has an independently
   checked subject and the contradiction is in that subject's origin, evidence, suffix, or global
   occurrence, it is `CorruptHistory { subject, detail }`.

2. Carry typed admission refusals across the existing Eventlog `Guard` interface with an
   operation-local, one-shot typed slot plus a finite code enum. Do not parse `Display`, place
   dynamic values in the code, or expose ER policy to Eventlog.

3. Add an adapter-specific administrative imported-anchor operation over the existing
   `SubjectHistory`/`HistoryOrigin::Imported(LegacyAnchor)` input. Do not overload
   `AsyncRecordedWriter::append`: that port requires a `BatchKey` and returns committed receipts,
   neither of which exists for an imported boundary.

4. Expose only the existing pure `validate_entry_against_state` function for transaction-local
   ordinary admission. Import validation can call the already-public `verify_subject_history`; it
   does not need a second importer validator or copied kernel execution.

## Existing boundaries that constrain the mapping

`EventLogError` is the closed variant boundary available to the adapter: `Invalid`,
`GuardRefused`, `Overloaded`, `Closed`, `Deadline`, `UnknownCommit`, `Conflict`,
`IdempotencyMismatch`, `CausationDepthExceeded`, `NotFound`, and `Backend`
(`EL/crates/eventlog-core/src/lib.rs:630-660`). `GuardRefused.code` is explicitly a stable owner
code rather than a display message (`EL/.../eventlog-core/src/lib.rs:635-640`). A guard error
abandons the append and rolls back guard projection writes (`EL/crates/eventlog-core/src/projection.rs:168-180`),
and an inline projector error aborts the append (`EL/.../projection.rs:145-165`). Group admission
runs once before entries; exact group retries bypass both admission and projectors
(`EL/crates/eventlog-core/src/atomic_group.rs:98-113`).

The provider paths make the commit boundary concrete:

- File runs guard, ordered appends, projectors, and group bookkeeping through one transaction;
  deduplication is checked before the guard (`EL/crates/eventlog-file/src/lib.rs:461-521`). Journal
  failures after possible manifest publication become `UnknownCommit`
  (`EL/crates/eventlog-file/src/journal.rs:208-252`).
- SQLite uses one `BEGIN IMMEDIATE`, invokes the guard before appends, and passes the result to the
  one commit/rollback finisher (`EL/crates/eventlog-sqlite/src/atomic_group.rs:18-123` and
  `EL/crates/eventlog-sqlite/src/lib.rs:1776-1793`).
- PostgreSQL commits only an `Ok` transaction result and converts commit-response loss or the outer
  transaction timeout to `UnknownCommit`; an `Err` path attempts rollback and returns the original
  variant only after rollback succeeds (`EL/crates/eventlog-postgres/src/atomic_group.rs:373-408`).

The adapter may rely on that closed contract only after the final three providers qualify it.
In particular, it must never catch an arbitrary provider/transport error and relabel it as the
typed guard refusal merely because a guard slot contains a value.

## Typed guard refusal transport

Use a private finite code enum such as:

```rust
enum ErGuardRefusalCode {
    RevisionConflict,
    RecordConflict,
    BatchConflict,
    PreviouslyRecordedBatchEntries,
    CorruptHistory,
    ProviderIntegrity,
}

struct GuardRefusal {
    code: ErGuardRefusalCode,
    error: AsyncStoreError,
}
```

Each physical append attempt constructs a new `Arc<Mutex<Option<GuardRefusal>>>` shared by its
guard and outer adapter call. On a semantic refusal, the guard atomically stores the exact typed
error and returns the corresponding fixed string form of
`EventLogError::GuardRefused { code }`. Codes contain no subject, record ID, expected revision, or
message. The outer adapter consumes the slot only for an exact `GuardRefused` result, parses only
the finite known code, checks that code against the stored error variant, and returns
`WriteFailure::NotCommitted(stored_error)`. A missing slot, an unknown code, a second write, or a
code/error mismatch is `NotCommitted(ProviderIntegrity { ... })`.

The guard returns a `ProjectionStore` failure directly without populating the slot. If the guard
has stored an ER refusal but PostgreSQL rollback then fails, the provider returns `Backend`, not
the earlier `GuardRefused` (`EL/.../eventlog-postgres/src/atomic_group.rs:399-403`); the adapter must
ignore the slot and map the actual `Backend`. The same rule applies to `UnknownCommit`. This is the
required protection against treating a transport failure as proof of rollback.

The fixed adapter projector does not use the slot and must not emit ER semantic guard codes.
Admission owns binding, subject predecessor, batch, and global-ID conflicts before any event is
written. A projector `Backend` stays a backend failure; an adapter-generated closed-decode or
impossible-row refusal is `Invalid` at the Eventlog boundary and becomes `ProviderIntegrity` at
the ER boundary without examining its string. A `GuardRefused` from the projector has no matching
operation slot and is therefore `ProviderIntegrity`. Projector failure is still transactionally
noncommitting under the group contract; the shared conformance test proves event and projection
rollback (`EL/crates/eventlog-conformance/src/atomic_group.rs:249-315`).

## Exhaustive append error mapping

Public request, canonical encoding, context, group shape, Eventlog grammar, checked coordinates,
and causation depth are validated before blob upload or provider invocation. Consequently, caller
failures return `WriteFailure::NotCommitted(InvalidInput | Encoding | PositionExhausted)` directly;
they do not first become `EventLogError::Invalid`. `AppendGroup::fingerprint` validates metadata,
nonempty/same-tenant entries, events, and `claim == None` before its transaction
(`EL/crates/eventlog-core/src/atomic_group.rs:26-79`).

| `EventLogError` observed from `append_group_guarded` | ER action/result | Commit statement |
| --- | --- | --- |
| `Invalid(_)` | `NotCommitted(ProviderIntegrity)` because the adapter supplied a locally validated closed group; never classify by the text. A caller-caused equivalent was already `InvalidInput`/`Encoding`. | Definite noncommit at this return boundary. |
| `GuardRefused { code }` | Exact known code plus matching one-shot slot becomes `NotCommitted(the typed ER error)`; otherwise `NotCommitted(ProviderIntegrity)`. | Definite noncommit only for the exact returned variant. |
| `Overloaded` | `NotCommitted(Unreachable { provider: "eventlog", detail: "resource budget exhausted" })`. | Provider did not admit a commit. |
| `Closed` | `NotCommitted(Unreachable { provider: "eventlog", detail: "closed" })`. | Provider did not admit a commit. |
| `Deadline { operation }` | `NotCommitted(Unreachable { provider: "eventlog", detail: stable detail built from the typed operation })`. Do not parse a message. PostgreSQL's whole-transaction timeout is separately converted to `UnknownCommit`. | Definite noncommit for this returned variant. |
| `UnknownCommit` | Ordinary append: run semantic recovery; if still unresolved, preserve `WriteFailure::Uncertain { original BatchKey, cause }`. | Never claim rollback. |
| `Conflict { .. }` | Do not map physical stream version to ER revision. Re-read semantic authority, then return an exact existing outcome/conflict, or make another newly frozen physical attempt only if semantic validation still permits it. | This physical attempt did not commit; a concurrent semantic winner may still require recovery. |
| `IdempotencyMismatch { .. }` | Do not expose the Eventlog key as ER identity. Re-read the ER batch/record/import authority. Equal original semantic bytes return the original committed/historical result; different bytes return the existing ER conflict; group bookkeeping without matching ER authority is `ProviderIntegrity`. | Current attempt did not commit; the prior key owner is resolved semantically. |
| `CausationDepthExceeded { .. }` | Caller context is rejected as `InvalidInput` before IO. If the locally validated group nevertheless returns this, use `NotCommitted(ProviderIntegrity)`. | Definite noncommit. |
| `NotFound` | It is not a normal append result. Use `NotCommitted(CorruptHistory)` only when an exact subject is independently known; otherwise `NotCommitted(ProviderIntegrity)`. On reads, an optional absence is `Ok(None)` and a missing required reference follows the same subject-known distinction. | Definite noncommit when returned during append/projector execution. |
| `Backend(_)` | Direct append result becomes `NotCommitted(AsyncStoreError::Backend(...))` without parsing. If it occurs while trying to resolve an earlier `UnknownCommit`, retain `Uncertain`; failure to recover cannot retroactively prove rollback. | Trust this as noncommit only because the provider contract reserves ambiguity for `UnknownCommit`; final qualification must mutate every terminal path. |

For `Conflict`, `IdempotencyMismatch`, and `UnknownCommit`, semantic recovery follows the existing
ER order rather than physical errors. The executor already checks batch identity, then global
record identity, compares exact request/record material, verifies the immutable occurrence, and
returns committed or historical outcomes (`ER/crates/entity-executor/src/lib.rs:371-530`). Exact
imported single-record recovery returns `AppendOutcome::Historical`; named requests containing
prior members return `PreviouslyRecordedBatchEntries` (`ER/.../entity-store/src/asynchronous/memory.rs:166-245`).
Recovery unavailability following ambiguity remains `Uncertain`, even if a direct read failure
would normally be `Unreachable`, `Backend`, `ProviderIntegrity`, or `CorruptHistory`.

## Imported-anchor API

Keep this operation out of `AsyncRecordedWriter` and expose it from the bound adapter's existing
`operation(context)` administrative surface:

```rust
pub trait AsyncImportedAnchorWriter: Send + Sync {
    fn import_anchor<'a>(
        &'a self,
        history: SubjectHistory,
    ) -> BoxFuture<'a, Result<ImportAnchorOutcome, ImportAnchorFailure>>;
}

pub struct ImportAnchorOutcome {
    pub assurance: SubjectAssurance,
    pub replayed: bool,
}

pub enum ImportAnchorFailure {
    NotCommitted(AsyncStoreError),
    Uncertain {
        subject: Subject,
        cause: ImportAnchorUncertainty,
    },
}

pub enum ImportAnchorUncertainty {
    UnknownCommit,
    RecoveryUnavailable,
}
```

The trait is implemented only by the Eventlog operation object holding one caller-supplied
`EventlogOperationContext`; the context remains separate from ER history. This avoids adding a
fabricated `BatchKey`, request, or receipt merely to reuse `WriteFailure`. The outcome is always
`VerifiedAfterBoundary`; `replayed` records whether the call recovered an existing identical
anchor and creates no durable fact.

Validate operational context only after semantic recovery shows a new physical write is needed,
before blob upload. A recovered identical anchor must not depend on fresh context matching the
old metadata or on new context passing admission that is unnecessary for this read-only result.
This is the same context boundary as ordinary semantic retries in the adapter companion.

The input must have `HistoryOrigin::Imported(anchor)` and an empty `records` suffix. This is the
same narrow input shape used by the current in-memory test/reference seam, which explicitly says
it is not a source importer (`ER/crates/entity-store/src/asynchronous/memory.rs:103-160`). Reject
`Genesis` and a nonempty suffix as `InvalidInput`.

### Pure validation before IO

1. Validate `history.subject` and require the anchor instance to name it.
2. Call public `verify_subject_history(&history, &anchor.instance)` and require the exact
   `SubjectAssurance::VerifiedAfterBoundary { subject, anchor_revision }` result. This calls the
   existing imported-boundary validator and checks the supplied terminal state
   (`ER/crates/entity-store/src/asynchronous/verify.rs:397-454`). The validator checks revision
   bounds, subject agreement, evidence revision bounds, envelope validity, source coordinates,
   declared order, and duplicate envelope record IDs
   (`ER/.../entity-store/src/asynchronous/verify.rs:136-215`).
3. Encode the selected closed anchor wrapper and every envelope record using existing canonical
   record bytes. Re-decode/re-encode and compare exact bytes before upload. The selected wrapper,
   finite evidence variants, and prohibition on invented request/batch/receipt/chronology are at
   `ER/docs/design/eventlog-recorded-encoding-v0.1.md:40-81`.

This validates internal consistency of a trusted boundary. It does not recompute the anchor state
from legacy evidence, prove that `CompleteSubject` is true, invent mixed chronology for
`PerKindOnly`, or prove that the source is quiescent. `LegacyAnchor` is deliberately an explicit
trust boundary (`ER/crates/entity-store/src/asynchronous/types.rs:470-577`).

The only additional pure ER exposure needed by the adapter is to make the existing
`validate_entry_against_state` public for ordinary transaction-local overlay checks. It currently
validates entries, `Expect`, decision recomputation from the saved definition/command, and
observation behavior but is re-exported only `pub(crate)`
(`ER/crates/entity-store/src/asynchronous/verify.rs:51-125` and
`ER/crates/entity-store/src/asynchronous.rs:18-21`). Do not expose `validate_imported_boundary`:
the public `verify_subject_history` call above already owns that validation.

### Destination authority and admission

Authority comes from the already bound adapter, not from an untrusted import field. Compare the
exact closed `{logical_scope, stream_identity, tenant}` tuple against native generation and binding
inside the append transaction; all three strings are preserved exactly
(`ER/docs/design/eventlog-recorded-encoding-v0.1.md:7-29`). Import is admitted only while the caller
holds the external migration/maintenance exclusion and the destination binding has already been
provisioned. It never provisions or discovers authority.

Before any physical write, semantic recovery checks the subject origin and every envelope global
record identity against complete authoritative event/blob state and the derived indexes:

- An exact existing anchor event/blob with the same authority, subject, instance, completeness,
  order, and ordered evidence is success with `replayed: true`. It remains success after later
  suffix records have been appended; compare the immutable origin, not current state to the anchor.
- Any different existing genesis/import origin for the subject is
  `RevisionConflict { subject, expected: Expect::Absent, found: Some(current_revision) }`.
- Every `LegacyEvidence::Envelope` reserves its `RecordedEntry.record_id` in the same global
  namespace as committed records. Any occupied key with different complete identity/content is
  `RecordConflict { record_id }`. An exact imported row without its exact anchor/subject authority
  is `CorruptHistory` when the subject is proven, otherwise `ProviderIntegrity`.
- Bare `Decision` and `Event` evidence creates no global record row. The selected row schema gives
  imported entries no batch, request, or receipt and shares the committed/imported key
  (`ER/docs/design/eventlog-recorded-indexes-v0.1.md:93-132`).

The guard locks, in the selected total order, the binding row, every envelope global-record key,
and the one subject key, including absent keys. It compares unhashed identities, refuses a prior
different record or any non-replay subject history, and writes no rows
(`ER/docs/design/eventlog-recorded-indexes-v0.1.md:230-269`).

### One atomic physical append

After validation and semantic recovery:

1. Upload each exact envelope record blob, then the complete anchor wrapper blob. A crash may leave
   content-addressed orphan blobs; they are explicitly nonauthority and create no record or receipt
   (`ER/docs/design/eventlog-recorded-indexes-v0.1.md:189-195`).
2. Freeze one `AppendGroup` containing one `er.import_anchor`, schema `1`, body
   `{"blob": anchor_digest}` event on the derived `er.subject` stream. Use physical
   `Expected::NoStream`, `claim = None`, idempotency key
   `K("er.eventlog.import-command-key/1", C({authority,subject}))`, and request hash equal to the
   anchor blob digest. Freeze the exact NewEvent name/schema_version/data and complete operation metadata too
   (`ER/docs/design/eventlog-recorded-encoding-v0.1.md:83-93` and
   `ER/docs/design/eventlog-recorded-adapter-v0.1.md:158-177`).
3. Call `append_group_guarded`. The inline projector inserts one imported global-record row per
   envelope evidence item and establishes the imported subject row in the same transaction. Bare
   evidence creates no row. A prior different record or any pre-existing subject history refuses;
   exact same-event reapplication is a no-op
   (`ER/docs/design/eventlog-recorded-indexes-v0.1.md:205-219`).
4. Return `ImportAnchorOutcome { assurance: VerifiedAfterBoundary { ... }, replayed: false }` from
   actual committed coordinates. The import event consumes physical stream/global positions but
   becomes no `StoredRecord` and yields no `CommitReceipt`
   (`ER/docs/design/eventlog-recorded-encoding-v0.1.md:119-123`).

Event IDs are provider-minted outputs, not NewEvent inputs. Capture the original RecordedEvent ID
from AppendGroupResult.appends[].events[] and verify it against the committed anchor's PhysicalRef,
stream/version/global position and reference content. Exact retries and semantic recovery retain
that original ID. Never preallocate an ID or extend NewEvent to satisfy a nonexistent input field
(`EL/crates/eventlog-core/src/lib.rs:184-188,557-565`; `atomic_group.rs:82-89`). Tests must compare
the original returned ID with retry/recovered anchor coordinates while preserving unchanged input.

### Retry and unknown-response recovery

An in-process retry first reuses the exact frozen group. Eventlog group deduplication occurs before
the guard/projector and returns original ranges (`EL/crates/eventlog-core/src/atomic_group.rs:82-113`;
provider ordering is visible in `EL/crates/eventlog-file/src/lib.rs:473-494` and
`EL/crates/eventlog-sqlite/src/atomic_group.rs:51-87`).

After restart or `UnknownCommit`, resolve semantic authority first. Exact anchor bytes plus exact
subject origin and all envelope rows return `replayed: true`. A different anchor or global record
owner returns the typed conflicts above. If native authority cannot yet settle the result, return
`ImportAnchorFailure::Uncertain { subject, cause }`; never return `NotCommitted` merely because a
later read failed. A complete snapshot that presently shows absence is not by itself a fence
against an earlier uncertain commit. A new attempt may use the same semantic command/global keys;
the transaction-local locks admit one winner. `IdempotencyMismatch` then triggers semantic recovery
again: exact committed anchor succeeds, different anchor conflicts, command bookkeeping without
the corresponding authoritative anchor is `ProviderIntegrity`, and unavailable recovery remains
uncertain. New operational context is not compared with the original anchor and cannot create an
import-content conflict.

### Subsequent reads and assurance

History reconstruction returns the anchor as `HistoryOrigin::Imported(anchor)` and only later
reference events as `records`. It retains actual physical gaps around the import event. Starting
from the anchor instance, `verify_subject_history` checks every suffix entry and returns
`VerifiedAfterBoundary` (`ER/crates/entity-store/src/asynchronous/verify.rs:397-454`). Each envelope
global lookup returns `RecordLookup::Imported`; `verify_imported_record` requires the exact evidence
to occur at the subject boundary and returns no receipt or replay-from-genesis claim
(`ER/.../entity-store/src/asynchronous/verify.rs:282-321`). Ordinary single-record recovery may
therefore return the existing `AppendOutcome::Historical { evidence, assurance }`, whose API has no
receipt (`ER/crates/entity-store/src/asynchronous/types.rs:273-310`).

## External source-migration and restore prerequisite

The adapter cannot establish source authority from a caller-set completeness enum. Before invoking
`import_anchor`, an external migration owner must:

1. fence/quiesce source writers for the selected scope and hold that exclusion through capture;
2. obtain a source-owned consistent complete capture, or explicitly accept
   `AvailableEvidenceOnly`, with stable source identity and locators;
3. establish which subjects are included, whether each `CompleteSubject` statement is warranted,
   and whether order is only per-kind or genuinely mixed subject order;
4. validate every typed subject boundary plus cross-subject global record-ID uniqueness; and
5. retain an explicit trusted source/restore checkpoint until every destination anchor is settled.

When the source implements ER's complete reader, `verify_complete_store` is the relevant
provider-owned completeness route; an editable snapshot cannot promote itself to complete merely
by setting the marker (`ER/crates/entity-store/src/asynchronous/verify.rs:496-547`). Other legacy
sources need an equivalent source-owned migration procedure. The destination adapter consumes the
result but cannot prove the source fence, capture completeness, or historical chronology.

A restored Eventlog backup with the same generation and binding but missing a coherent later
suffix is indistinguishable from the tuple alone
(`ER/docs/design/eventlog-recorded-adapter-v0.1.md:102-105`). Import must therefore be authorized by
that external checkpoint or target a deliberately reinitialized, freshly bound destination under
maintenance. It must not silently repair a same-generation partial restore based on apparent
subject absence.

## Decisive implementation tests

### Error and transaction tests

- For every finite guard code, force the exact typed ER refusal and assert the matching
  `WriteFailure::NotCommitted` variant, zero events, zero adapter index changes, and no code/message
  leakage. Mutate unknown code, empty slot, mismatched code/error, and double-slot writes to
  `ProviderIntegrity`.
- Have a guard store an ER refusal and then make provider rollback/transport return `Backend`; assert
  the adapter returns `NotCommitted(Backend)`, not the stored guard error. Repeat with
  `UnknownCommit` and assert uncertainty. This is the critical false-rollback control.
- Force projector closed-decode/invariant refusal, projector `Backend`, and projector
  `GuardRefused` without a slot; assert respectively `ProviderIntegrity`, `Backend`, and
  `ProviderIntegrity`, with complete event/index rollback.
- Exercise all eleven `EventLogError` variants through a fake atomic provider and the actual three
  providers where applicable. For `Conflict` and `IdempotencyMismatch`, assert semantic recovery,
  never a physical-version-to-entity-revision cast. For `UnknownCommit`, cover committed,
  rolled-back, conflicting, corrupt, and recovery-unavailable outcomes.
- Provider qualification must fail if any post-publication/commit-response-loss path reports
  `Backend`, `Deadline`, or the original guard error instead of `UnknownCommit`.

### Import tests

- Reject genesis input, nonempty suffix, subject mismatch, zero/out-of-domain revisions, evidence
  later than the anchor, invalid envelopes, blank source coordinates, declared/known order mismatch,
  duplicate envelope IDs, unknown fields/tags, digest mismatch, and noncanonical bytes before IO.
- Import anchors containing each finite evidence variant. Assert envelope rows reserve global IDs,
  bare decisions/events create no record rows, the anchor creates no batch/request/receipt, and the
  evidence array/order bytes remain exact.
- Mutate a global ID against committed and imported owners; both produce `RecordConflict`. Mutate an
  existing genesis or different anchor for the subject; both produce `RevisionConflict(Absent, ...)`.
  Inject an exact row without its anchor and assert the subject-known `CorruptHistory` versus the
  subject-unknown `ProviderIntegrity` branch.
- Retry the same anchor before and after later suffix records; both return the same
  `VerifiedAfterBoundary` with `replayed: true` and create no second event or row. Change each anchor
  field/evidence byte under the same subject key and assert typed conflict.
- Inject failure after each blob upload, guard step, event append, projector row, group bookkeeping,
  and response boundary. Only uploaded blobs may remain orphaned. No failed transaction may expose
  a partial subject or imported global-ID set.
- Race identical imports, different anchors for one subject, import against ordinary genesis, and
  import envelope IDs against ordinary records in both commit orders. Assert one atomic winner and
  exact replay/conflict results on File, SQLite, and PostgreSQL.
- Lose the response on both commit and rollback paths. After restart, assert exact semantic recovery,
  later-suffix recovery, retained uncertainty while capture is unavailable, and no conflict from a
  changed operation context.
- Read the imported boundary plus a multi-record suffix. Assert actual non-dense physical positions,
  `HistoryOrigin::Imported`, exact `RecordLookup::Imported`, `VerifiedAfterBoundary`, and historical
  single-record retry without a fabricated receipt, request, definition, genesis, or chronology.
- Demonstrate separately that a coherently truncated same-generation restore cannot be detected from
  `{logical_scope, stream_identity, tenant}`. The migration/restore harness, not the adapter API,
  must supply and test the external source fence and trusted checkpoint.

No bridge queue/runtime behavior, credentials, fleet procedure, SQL inspection, or final source pin
is selected by this report.
