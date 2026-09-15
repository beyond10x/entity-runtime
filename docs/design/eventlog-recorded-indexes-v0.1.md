# ER Eventlog inline indexes and explicit rebuild v0.1

Status: coordinator-selected proposal for `story:eventlog-recorded-adapter-and-bridge`, requiring
complete independent design review before implementation. It establishes no provider qualification.
Source references are ER17da35a7 with the adapter/encoding companion proposals, and accepted
Eventlog18322cbe. The original independent scoping report remains unchanged in local evidence.
The coordinator verified the cited shadow blob-prefix paths, incomplete watermarked replay and
crate-private ER validator. The proposed public administrative capability below is asynchronous
and object-safe so all three providers can implement it without a caller-runtime bridge.

## Result

The four required read models fit one fixed inline projector and one guard. Existing Eventlog
append transactions can lock absent projection keys and publish all four tables atomically with an
ordered `AppendGroup`. Existing Eventlog admission is usable only as an explicit administrative or
provider-bootstrap operation. A read-only adapter open can verify already admitted and already
registered projections through the accepted native-capture dependency; the current public
`EventStore` projection reads and `is_inline` alone cannot prove that condition.

There is no cross-provider API that can truthfully rebuild this blob-resolving inline projector.
SQLite and PostgreSQL reject rebuild when the projector is registered inline, and their existing
shadow rebuild context also sends `ProjectionStore::get_blob` to the shadow prefix rather than the
authoritative blob table. PostgreSQL's existing rebuild deliberately replays a watermarked eligible
prefix, which is weaker than the complete committed tenant history required for an authoritative
inline index. The smallest Eventlog-owned addition is one explicit inline-rebuild capability across
File, SQLite and PostgreSQL, with separate active-blob and shadow-row namespaces and a complete
provider-owned snapshot under exclusive publication. It must retain inline registration and swap
all four tenant row sets atomically.

The transaction-local ER state validator is also presently crate-private. The adapter guard should
reuse that pure validator rather than copy the decision engine. Making that existing function an
intentional public entity-store seam is the smallest ER-owned exposure; it does not belong in the
projector.

## Fixed projector and physical keys

One projector named `er_recorded_v1` owns these four `ProjectionSpec`s, all with
`indexed: &[]`:

| Projection | Row key | Purpose |
| --- | --- | --- |
| `er_binding_v1` | literal `singleton` | one authority binding for the tenant generation |
| `er_records_v1` | `K("er.eventlog.record-index-key/1", C({"authority":Authority,"record_id":RecordId}))` | one global namespace shared by committed and imported envelope record IDs |
| `er_batches_v1` | `K("er.eventlog.batch-index-key/1", C({"authority":Authority,"batch_key":BatchKey}))` | one complete original single/named request and receipt |
| `er_subjects_v1` | `K("er.eventlog.subject-index-key/1", C({"authority":Authority,"subject":Subject}))` | current typed-state source, origin and latest physical head |

`K`, `C`, `Authority`, `Subject` and `BatchKey` have exactly the meanings selected in
`docs/design/eventlog-recorded-encoding-v0.1.md:7-29,45-65,83-90`. Including `Authority` in every
hashed coordinate keeps generations disjoint. Every row repeats the full unhashed authority and
the unhashed identity used by its key; decoding recomputes the key and rejects a digest collision or
substitution. `BatchKey` remains the closed `single_record`/`named` sum, so equal opaque strings in
the two namespaces remain distinct. The existing ER coordinate encoding establishes those
namespaces at `crates/entity-store/src/asynchronous/types.rs:55-100`.

No secondary indexed field is needed. Reads use exact keys, while complete-open integrity comes
from native capture rather than `find` or pagination. This also avoids making a provider's generic
JSON field rendering part of ER identity. Eventlog permits zero indexed fields and validates fixed
projection names (`crates/eventlog-core/src/projection.rs:14-49`).

## Closed row types

Each body is a two-element JSON array `[Tag, Body]`. Tags below are exact. `Body` and every nested
variant are closed: all fields are required, unknown fields, unknown variants, wrong array length
and wrong tag/version refuse. Rust implementations should use concrete types with
`deny_unknown_fields`; coordinates and counters are checked `u64`s. Every field named `revision`
has the narrower ER domain `1..=i64::MAX`, including both record variants, every batch member,
subject state and any resolved anchor/record state. Decode and validate this bound before using
a row in lookup, semantic retry, guard admission, complete capture or rebuild; a plain U64 schema
or successful integer deserialization is insufficient. This preserves the existing validator at
`crates/entity-store/src/asynchronous/verify.rs:22-30`. Do not cast physical stream versions or
global positions to ER revisions, and do not apply the ER bound to those physical coordinates.
Out-of-domain authored inputs refuse before publication; persisted violations are integrity
failures. Inline projection and rebuild must refuse rather than publish an invalid derived row.
A projection row is derived
data, so its JSON serialization is not a substitute for the canonical blob bytes.

In the row notation below, `ErRevision` means a JSON integer in `1..=9223372036854775807`,
validated on every construction and decode; it is not unrestricted U64.

The common closed coordinate is:

```text
PhysicalRef = {
  "event_id": String,
  "global_seq": U64,
  "stream_id": String,
  "stream_version": U64
}
```

The binding row is:

```text
["er.eventlog.binding-index/1", {
  "authority": Authority,
  "binding_blob": Digest,
  "physical": PhysicalRef
}]
```

Its event must be the unique `er.binding` schema-1 event in stream type `er.binding`, stream ID
`singleton`, and its blob must decode to the same authority. The physical reference records what
was actually committed; it is not predicted.

The global record row is one closed sum:

```text
["er.eventlog.record-index/1", {
  "entry": {
    "kind": "committed",
    "authority": Authority,
    "record_id": RecordId,
    "subject": Subject,
    "record_kind": "decision" | "observation",
    "revision": ErRevision,
    "batch_key": BatchKey,
    "member_index": U64,
    "record_blob": Digest,
    "request_blob": Digest,
    "batch_blob": Digest,
    "physical": PhysicalRef
  }
}]

["er.eventlog.record-index/1", {
  "entry": {
    "kind": "imported",
    "authority": Authority,
    "record_id": RecordId,
    "subject": Subject,
    "record_kind": "decision" | "observation",
    "revision": ErRevision,
    "anchor_blob": Digest,
    "evidence_index": U64,
    "record_blob": Digest,
    "anchor_physical": PhysicalRef
  }
}]
```

`evidence_index` is the exact index in the anchor evidence array and is checked through
`usize::try_from` before indexing. The imported variant deliberately has no batch, request or
receipt coordinate. Both variants occupy the same projection and key derivation; this is the
global committed/imported record-ID exclusion. The record blob remains the complete content.

The batch/request row is:

```text
["er.eventlog.batch-index/1", {
  "authority": Authority,
  "batch_key": BatchKey,
  "batch_blob": Digest,
  "members": [{
    "member_index": U64,
    "record_id": RecordId,
    "record_key": Digest,
    "subject": Subject,
    "record_kind": "decision" | "observation",
    "revision": ErRevision,
    "record_blob": Digest,
    "request_blob": Digest,
    "physical": PhysicalRef
  }, ...]
}]
```

Members are nonempty and ordered exactly by `member_index`, which must be `0..len-1` exactly once;
each nested value must equal the corresponding committed record row and resolved recorded-entry
wrapper. `batch_blob` resolves to the exact unchanged `er.batch/1` bytes. Thus the row reconstructs
the original single/named key, ordered member identities, exact requests, records and actual receipt
positions. A `SingleRecord` row has one member at index zero and its key value equals that member's
record ID, as required by the selected encoding (`docs/design/eventlog-recorded-encoding-v0.1.md:95-123`).

Each members[].record_key is exactly
`K("er.eventlog.record-index-key/1", C({"authority":Authority,"record_id":RecordId}))`, using the
containing batch row's authority and that member's original record ID. It is the actual
er_records_v1 row key selected above, not a blob digest, subject key, batch key or new namespace.
Recompute it on every row read before dereferencing it and require equality with both the stored
member value and the referenced record row's recomputed key. Validate original authority/record ID
as well as the digest. Guard/projector and rebuild derive it from those originals; none trusts a
persisted key as an independent authority. A substituted key refuses even if it reaches another
well-formed record. Tests mutate the member key alone and with a crossed valid row, and exercise
revision0,1,i64::MAX,i64::MAX+1,u64::MAX at every persisted revision site in readers and rebuild.

The subject row is:

```text
["er.eventlog.subject-index/1", {
  "authority": Authority,
  "subject": Subject,
  "origin":
    {"kind":"genesis"} |
    {"kind":"imported","anchor_blob":Digest,"anchor_physical":PhysicalRef},
  "revision": ErRevision,
  "state_source":
    {"kind":"decision","record_blob":Digest} |
    {"kind":"anchor","anchor_blob":Digest},
  "physical_head": PhysicalRef
}]
```

The adapter resolves `state_source` transaction-locally to recover the complete current
`EntityInstance`. It does not serialize arbitrary-precision entity state into generic projection
JSON. A decision changes `revision` and points `state_source` to its record blob. An observation
changes only `physical_head`; it retains the prior revision and state source. An imported anchor
establishes the imported origin, revision and anchor state source. Later decisions retain the
imported origin while advancing state source. This corresponds to ER's explicit `HistoryOrigin`
and subject-history boundary (`crates/entity-store/src/asynchronous/types.rs:545-577`) while keeping
actual stream version and global sequence distinct from entity revision
(`crates/entity-store/src/asynchronous/types.rs:206-233`).

## Authority and deterministic projection

The binding event, recorded-entry/import-anchor events and every referenced live blob are the
authority. All four row sets are derived and rebuildable. Eventlog command/group bookkeeping is
retry machinery, not ER record authority. An orphan uploaded blob creates no row, record, batch or
receipt. A missing/redacted/corrupt referenced blob or event cannot be repaired by a projection
rebuild; it makes replay and open refuse, matching
`docs/design/eventlog-recorded-adapter-v0.1.md:194-203`.

`Projector::apply` performs only closed decoding, digest/domain recomputation, reference equality,
coordinate folding and row consistency. It does not execute a saved ER command or decide a new
state. Transaction-local blob reads are an explicit `ProjectionStore` capability
(`crates/eventlog-core/src/projection.rs:51-70`). For every event it validates tenant, event
name/schema, stream type/ID, exact `{"blob":Digest}` data, authority and the resolved wrapper/blob
graph. Any unknown event in the tenant refuses instead of being skipped.

Projection operations are:

1. `er.binding`: require no prior binding row; insert it. Reapplication of the identical event ID
   and identical body is a no-op; any other existing row refuses.
2. `er.import_anchor`: decode and validate the anchor; insert one imported global-record row for
   each envelope evidence item, then establish the subject row. Bare decision/event evidence gets
   no fabricated record row. A prior different event at any global record key or any pre-existing
   subject history refuses. Exact same-event reapplication is a no-op.
3. `er.recorded_entry`: resolve wrapper, record, request and batch; insert its committed global
   record row; fold the subject physical head and decision/observation state source. On a non-final
   member, do not create or modify the batch row. On the final member, read every earlier member's
   transaction-local record row, require exact batch blob/key, original index, record/request
   digests and strictly increasing actual global positions, then insert the one complete ordered
   batch row. A prior row is accepted only when it names the same physical event and has exactly
   the same body; a different event using that record or batch identity refuses.

Eventlog invokes inline projectors after recording each event and aborts the append when an inline
projector errors (`crates/eventlog-core/src/projection.rs:145-165`). File shows the concrete ordered
behavior: the transaction records an event, applies every inline projector to the written events,
then proceeds to the next group entry (`crates/eventlog-file/src/lib.rs:230-284,461-521`). Therefore
earlier members are visible to the projector inside the same transaction, the final member can
prove the complete batch, and no observer can see the earlier record/subject rows or final batch
row unless the entire group commits. This behavior must be qualified on SQLite and PostgreSQL too;
the design cannot infer it from File alone.

## Guard, lock set and repeated subjects

For every new physical operation, form the complete lock set before validating any row:

* the binding tuple `("er_binding_v1", "singleton")`;
* the one batch tuple for an ordinary nonempty request;
* one global record tuple for every ordinary member or every imported envelope evidence item;
* one subject tuple for every distinct subject touched.

Deduplicate exact tuples, then sort by the fixed projection rank `binding=0`, `batch=1`,
`record=2`, `subject=3`, followed by unsigned UTF-8 row-key bytes. Call `get_for_update` once per
tuple in that order, including keys whose rows are absent. Digest equality does not deduplicate
different originals: recompute and compare the row's full identity, and refuse a collision.
Repeated subjects acquire one physical row lock; the guard then evaluates members in original
caller order against a local typed overlay. Each decision advances that overlay, while an
observation requires and retains the current entity revision. Physical `Expected` values remain a
separate caller-order vector and account for each earlier member on the same subject stream.

`get_for_update` is specifically limited to inline projections
(`crates/eventlog-core/src/projection.rs:117-131`). PostgreSQL locks the projection identity before
`SELECT ... FOR UPDATE`, so an absent key is fenced (`crates/eventlog-postgres/src/lib.rs:1439-1480`).
SQLite's enclosing `BEGIN IMMEDIATE` transaction holds the single writer even for an absent result
(`crates/eventlog-sqlite/src/lib.rs:2087-2109`); File executes the guard and group under its provider
transaction (`crates/eventlog-file/src/lib.rs:461-521`). Eventlog group deduplication precedes the
guard in File (`crates/eventlog-file/src/lib.rs:468-494`), consistent with the selected exact
physical-retry path; the same ordering remains a required provider qualification.

After locking, the guard strictly decodes all rows, resolves their state/batch/record references,
compares the exact binding/generation and rejects any occupied conflicting global or batch key. It
checks ER `Expect` and each recorded entry against the overlay. The established implementation for
that last operation is `validate_entry_against_state`, including saved-definition command replay
and observation rules, but it is currently `pub(crate)`
(`crates/entity-store/src/asynchronous/verify.rs:32-125` and
`crates/entity-store/src/asynchronous.rs:18`). The smallest ER change is to expose that existing
pure operation with its current typed result; copying its kernel logic into this adapter or the
projector would create a second decision engine.

The guard itself writes no index rows. Successful reference-event projection owns all row writes,
so an event can never commit without its indexes and a rejected guard has no partial effect. This
matches Eventlog's guard rollback contract (`crates/eventlog-core/src/projection.rs:168-180`).

## Admission and nonmutating open

Admission is explicit and occurs under the already selected maintenance boundary, before binding
provisioning and before any provider handle begins serving:

1. An administrative schema-capable handle calls `create_projections` for exactly the one projector
   and four specs. It refuses incompatible existing declarations/physical shape.
2. Every runtime writer handle attaches exactly `er_recorded_v1` inline before its first append
   using a new validate-only `attach_inline_existing` capability. It checks exact already admitted
   registry/physical shape and only installs in-memory code, under the handle's registration lock
   before serving freezes it. Missing/drifted/dirty/duplicate declarations refuse without DDL,
   journal writes or recovery. Provider bootstrap owns attachment; adapter `open` does not call it.
   The runtime handle passed to the adapter is already attached.
3. Provisioning captures the empty/binding state under maintenance, then writes the authoritative
   binding and derived singleton row atomically through the registered projector.

This separation is necessary because `register_inline` is not a read-only probe. File registers
the persistent projection shape and can append its registry operation
(`crates/eventlog-file/src/lib.rs:906-965`). PostgreSQL `register_inline` calls
`create_projections`; `create_projections` may migrate when schema writes are enabled and otherwise
validates physical tables (`crates/eventlog-postgres/src/lib.rs:957-1028`). SQLite likewise admits
tables as part of registration. All providers freeze inline registration once serving begins.

A nonmutating open requires both:

* `is_inline("er_recorded_v1")` on the already constructed handle; and
* the accepted future native `ConsistentTenantCapture`, requesting the exact four specs and proving
  their admitted registry/physical shape, all authoritative events/live blobs and complete row sets
  in one provider snapshot.

It derives the expected rows from captured authority and compares exact key/body sets, binding and
generation. Missing/incompatible admission, a missing/extra/different row, dirty projection,
unknown event or unresolved reference refuses and directs an operator to explicit administration.
Open performs no registration, DDL, recovery or rebuild.

Current `is_inline` only reports in-memory registration by projector name
(`crates/eventlog-core/src/lib.rs:846-847`; PostgreSQL implementation at
`crates/eventlog-postgres/src/lib.rs:1031-1034`). `projection_get` can inspect one row, and
`projection_list` is paginated (`crates/eventlog-core/src/lib.rs:870-907`); neither proves exact
physical admission or complete row-set equality. Pagination ending cannot replace native capture.

## Explicit rebuild and the missing Eventlog capability

Rebuild is an administrative operation under the selected maintenance boundary, with ordinary
writers, erasure and readers excluded for the whole operation. Its protocol is:

1. Capture and validate the generation, binding and authoritative event/blob graph without trusting
   the damaged derived rows. Require the exact admitted four projection specs and registered inline
   projector.
2. Invoke a new Eventlog inline-rebuild operation. It holds the provider's exclusive publication
   coordination, reads all committed events and live blobs for the tenant in one provider-owned
   snapshot, folds them in increasing `global_seq` into shadow copies of all four tables, and keeps
   the active inline projector registered throughout.
3. Unknown events, redaction, missing/corrupt blobs, closed-decode failures, global-ID conflicts,
   invalid subject folding or incomplete/crossed batch members abort without swapping. On success,
   replace all four tenant row sets in one transaction and return the exact applied count and final
   authoritative position.
4. Before maintenance ends, take a fresh native capture and compare the complete rebuilt key/body
   sets to a separate deterministic derivation from authority. Only that check reports success.

The smallest public owner/API seam is Eventlog core plus all three providers, preferably a distinct
administrative capability rather than weakening catch-up semantics:

```text
trait InlineProjectionAdmin: Send + Sync {
  fn rebuild_inline_projection<'a>(
    &'a self,
    projector: Arc<dyn Projector>,
    tenant: &'a TenantId
  ) -> BoxFuture<'a, Result<InlineRebuildResult, EventLogError>>;
}

InlineRebuildResult { applied: U64, position: U64 }
```

The operation requires the same projector name and exact specs already registered inline; it does
not admit/create a table, unregister/re-register, or accept a differently named projector that
happens to target the same tables. It must separate the authoritative event/blob namespace from the
shadow projection-row namespace in the provider's `ProjectionStore` context.

Use the existing Eventlog boxed-future convention. Cancellation and uncertain commit retain the
provider's existing bounded/quarantine/retirement contract; a dropped future is not proof that a
transaction rolled back. This interface is a required Eventlog dependency proposal, not a new
implemented ER trait or authority to issue direct provider SQL from the adapter.

This addition is required by concrete source behavior:

* File's existing rebuild replays all tenant events transactionally and can resolve blobs through
  its normal transaction, but it also calls `register` and does not enforce the proposed
  already-admitted-inline administrative contract (`crates/eventlog-file/src/lib.rs:988-1033`). Its
  mechanics can back the new capability after adding that precondition.
* SQLite explicitly rejects an inline projector, creates `eventlog_rebuild` shadow tables and
  supplies `prefix: "eventlog_rebuild"` to the projector
  (`crates/eventlog-sqlite/src/lib.rs:1637-1688`). Its `get_blob` constructs
  `<prefix>_blobs` (`crates/eventlog-sqlite/src/lib.rs:1845-1872`), so this adapter would query a
  shadow blob table rather than the authoritative owner blob table. The new context needs separate
  row and blob prefixes. SQLite already has the desired single transaction and atomic tenant-row
  replacement (`crates/eventlog-sqlite/src/lib.rs:1647-1705`).
* PostgreSQL also rejects an inline projector, holds an exclusive publication gate and atomically
  replaces tenant rows, but replays only its watermarked eligible prefix
  (`crates/eventlog-postgres/src/lib.rs:1156-1192`). An unrelated old transaction can therefore
  withhold already committed rows, as its own comment states at lines 1174-1176. Authoritative
  inline rebuild must instead take the complete committed tenant snapshot after publication is
  excluded. Its shadow context sets `prefix: "eventlog_rebuild"` while `get_blob` reads
  `<prefix>_blobs` (`crates/eventlog-postgres/src/lib.rs:1181-1182,1254-1283`), requiring the same
  active-blob/shadow-row split as SQLite.

Using a second projector name to evade `is_inline(projector.name())`, directly rewriting provider
tables, unregistering a live projector, or accepting the PostgreSQL watermark are not equivalent
capabilities. They fail the registered-projector identity, portable API, blob-resolution or
complete-history requirements.

## Decisive qualification cases

The implementation/review gate for this unit should establish at least these bounded cases on File,
SQLite and PostgreSQL:

* two concurrent ordinary requests, and ordinary versus import, contend on the same absent global
  record key; exactly one commits and the loser reports the existing identity conflict;
* named and single keys containing the same opaque value remain separate, while an exact semantic
  retry recovers the original row and actual positions;
* repeated-subject batch members validate in caller order, observations retain revision, and
  physical stream versions advance for every member;
* a failed member/projector rolls back all events and all rows; before commit no prefix row is
  visible, and after commit the batch row contains every member exactly once in order;
* a final-member replay is idempotent, while a different physical event at the same record or batch
  key refuses;
* digest-key collision/substitution is detected by the repeated originals and authority;
* open with missing/incompatible/unregistered/dirty projections refuses without mutation; orphan
  blobs remain non-authority;
* rebuild preserves actual sparse physical coordinates and original batch membership, and rebuild
  failure on unknown/redacted/missing/corrupt authority leaves active rows unswapped;
* PostgreSQL rebuild includes a committed event hidden from the ordinary feed watermark by an
  unrelated old transaction; SQLite/PostgreSQL blob resolution during shadow replay reads the live
  authoritative blob table;
* concurrent append and erasure cannot cross the provider's rebuild publication boundary; the
  post-rebuild native capture exactly matches all four derived row sets.

These cases establish this index/admission/rebuild unit only. They do not close native-capture or
SQL-integrity review, runtime bridge/error mapping, dependency pins, legacy source capture, or the
overall adapter story.
