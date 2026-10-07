# Recorded open checkpoint v0.1

Status: option (a) chosen by `story:recorded-open-checkpoint-design` and implemented in
`entity-eventlog` against Eventlog 0.8.0 (beyond10x/entity-runtime#55). It decides how an open of
a recorded Eventlog store under `CapturePolicy::ProviderTracked` stops costing a complete
verification of the whole history, and which provider proof that needs. Where the released
provider differs from § *The Eventlog capability*, the text below follows the provider; the items
this design left open are decided in § *Decided in the implementation*.

Source references: Entity Runtime at `b7362882` (paths relative to this repository); Eventlog at
the pinned rev `6983cc25` (written `eventlog@6983cc25:<path>:<line>`); Eventlog `origin/main` at
`1d089714` (`0.6.0-4-g1d089714`, read 2026-10-06). The implementation builds against the released
Eventlog `0.8.0` (`de30462b`). The measured baseline is
[`open-cost-baseline.txt`](../ess/evidence/provider-tracking/open-cost-baseline.txt); the
measurement with a checkpoint is
[`open-cost-checkpoint.txt`](../ess/evidence/provider-tracking/open-cost-checkpoint.txt).

## Decision

A bounded open makes two choices: what proves the verified prefix unchanged, and what the handle
answers reads from afterwards.

1. **Proof: option (a), a durable capture continuity proof in Eventlog.** Option (b), an
   Entity-Runtime-only checkpoint over the existing port, is rejected: no tracked open, and no
   handle it returns, would see an SQL edit to the verified prefix, which the accepted integrity
   boundary forbids.
2. **Model: the provider's verified index rows.** The checkpoint carries no copy of the verified
   model. A bounded handle answers a subject's state, a record and a batch from the index rows the
   last complete verification held to the events and the continuity proof shows unchanged. A
   persisted whole model is rejected by measurement: parsing its record text alone is about a
   fifth of today's whole verification at every size, so it grows with the store as that does.

| question | answer |
|---|---|
| where the checkpoint is persisted | an Eventlog snapshot of one Entity-Runtime-owned stream in the bound tenant, through the existing snapshot port |
| what it binds | format and verifier version, the authority and its binding reference, projections and limits, the provider's durable continuity bytes for the observation the handle last verified, the tenant position, and its own digest; no model digest and no totals (see *What it binds*) |
| when it is written | on a drain shutdown and on an explicit call, from the handle's last successful verification, when no valid checkpoint was loaded or any bound field differs, and never at a lower position than the persisted one |
| what invalidates it | a failed digest, an unknown format or verifier version, any authority, projection or limit difference, a provider that will not continue from it; a handle that refused on integrity writes none |
| a damaged or foreign checkpoint | is discarded; the open falls back to complete verification |
| a checkpoint ahead of the provider's head | refuses `ProviderTracked` opens with `ProviderIntegrity` until an explicit discard call; `FullVerification` opens are unaffected |
| tenant totals for `admit_growth` | one copy: the usage the provider bound to the checkpoint, advanced by `resulting_usage`; whether a blob is already bound is asked of the provider per digest |
| is an unkeyed digest enough | yes, for the threat the complete verification itself addresses; neither resists an informed writer, who can also forge a record that understates usage (see *What invalidates it*) |
| what a bounded open stops detecting | edits that bypass SQL while no handle is open (raw file writes, media damage), and writers that defeat the provider's change recording on purpose |
| what still costs a complete verification | a suffix holding any event other than a recorded entry, such as a recorded refusal; a provider write outside an atomic group, such as a refusal's blob; an expired journal |
| how a store gets durable continuity | an explicit, named migration per store; it is one-way for every older Entity Runtime and Eventlog until the matching disable call runs |
| the unchanged-head tamper scenarios | each passes unchanged, under either policy; the implementation adds variants that tamper a store whose checkpoint is current, and they refuse too |
| Eventlog work first | yes: the in-process continuity Entity Runtime pins is not on Eventlog `main`, and the durable form is new |

## The problem

`open_with_policy` builds the store handle and then takes `capture_model` once
(`crates/entity-eventlog/src/adapter.rs:959-992`, the call at `:987`). Under `ProviderTracked`
that is `tracked_model` (`adapter.rs:1042-1048`), whose first call has no checkpoint, so the
provider returns `TenantCaptureUpdate::Complete` and the whole model is built
(`crates/entity-eventlog/src/adapter/tracked.rs:72-108`, `build_model` at `:82`). The policy says
so: "Initial open is always verified completely" (`tracked.rs:13`), and so does the bridge:
"opening always verifies the whole authority" (`crates/entity-eventlog/src/sync.rs:1300`).

The provider's continuation proof is a value in memory: `CaptureCheckpoint` is "an immutable
provider-issued observation capability, never a persisted or caller-made cursor"
(`eventlog@6983cc25:crates/eventlog-core/src/capture.rs:47-52`), and the SQLite provider accepts
one only from the same issuer object
(`eventlog@6983cc25:crates/eventlog-sqlite/src/tracked_capture.rs:87-97`, the check at `:351`). A
consumer that runs one short process per command pays a complete verification on every command.

The baseline measures, median of five per size: `start`, the consumer's whole open; `capture`, the
provider's complete capture alone; `verify`, the store open on an already open provider (capture
plus whole-model build); and `record_parse`, parsing every record's canonical text, which is what
any open that loads a persisted copy of the model would at least do.

| events | `start` | `capture` | `verify` | `record_parse` | record text in the model |
|---|---|---|---|---|---|
| 55 | 138.6 (86.3–142.9) | 19.5 (12.6–20.7) | 129.2 (78.6–129.8) | 22.9 (16.3–87.4) | 1.4 MB |
| 601 | 1,079.0 (1,017.6–1,778.6) | 147.0 (143.5–280.0) | 1,004.5 (938.8–1,881.0) | 199.4 (178.1–309.7) | 15.6 MB |
| 1,203 | 2,167.0 (2,084.9–2,312.6) | 300.4 (290.9–338.1) | 1,915.9 (1,863.1–1,933.2) | 381.1 (362.0–551.6) | 31.2 MB |

Milliseconds, the median of three runs' medians with the range of the three, on a host shared with
other builds (load average 22–54 during the runs). The consumer measured 75 / 492 / 963 ms for its
own open at the same event counts (beyond10x/connectors#101). The provider's capture is about 15%
of `verify` at every size; the whole-model build is the rest, as `story:seeded-open-under-one-second`
found on another shape (85–92% in `build_model`).

## What proves the prefix unchanged

**(a) Durable provider continuity.** Eventlog lets a checkpoint outlive its process: the provider
encodes it as bytes, re-admits those bytes in a later process, and answers `capture_tenant_since`
with `Unchanged` or `AppendDelta` only when it can prove that nothing but its own acknowledged
appends changed the tenant's captured material since; otherwise `Complete`. The exact API and
provider behaviour are in *The Eventlog capability*.

**(b) Entity-Runtime-only checkpoint.** Entity Runtime persists its position and reads the suffix
itself with `read_feed(tenant, after_position, limit)`
(`eventlog@6983cc25:crates/eventlog-core/src/lib.rs:875-884`), `get_blob`
(`lib.rs:1135-1141`) and `projection_get` (the per-entity read's row call,
`crates/entity-eventlog/src/adapter/scoped.rs:375-381`). The port can say what was appended after a
position; it cannot say whether anything before it changed.

Today only a complete capture issues a provider checkpoint: the SQLite provider calls
`tracked.complete` after a complete observation and nowhere else
(`eventlog@6983cc25:crates/eventlog-sqlite/src/capture.rs:161-165`). Under (b), a bounded handle
therefore has no in-process checkpoint either, and must take a complete capture on its first
tracked read or continue from the feed for its whole life.

| | (a) durable provider proof | (b) Entity Runtime only |
|---|---|---|
| SQL edit to the verified prefix made while no handle is open | `Complete`, verified, refused | not seen by the open |
| SQL edit made while a bounded-opened handle is live | seen by the in-process proof, as today | not seen: the handle has no provider checkpoint |
| an index row edited while no handle is open | seen | not seen, and served by every row-backed read (see below) |
| raw file edit made while no handle is open | not seen (new gap, both options) | not seen |
| Eventlog work | land the pinned continuity on `main`; add the durable form (SQLite; other providers keep `Complete`) | none |
| accepted boundary: SQL writes through other connections invalidate reuse (`docs/design/eventlog-recorded-adapter-v0.1.md:397-400`) | kept for opens and live handles | broken for opens and for the handles they return |

The deciding row is the last. The operator accepted the SQL-visible boundary for live handles with
the explicit fallback that "a newly opened handle still verifies content"
(`eventlog-recorded-adapter-v0.1.md:400`). Option (b) removes that fallback and the live-handle
detection with it. Option (a) removes only the fallback for writes SQL cannot see.

Eventlog work is due under either option in one respect: the rev Entity Runtime pins, `6983cc25`
("feat: provide verified SQLite capture continuity"), is on the unmerged branch
`origin/fix/er-51-capture-checkpoints` and is not an ancestor of Eventlog `main` (merge base
`06c1e99c`); `main` has no `capture_tenant_since` and no `tracked_capture.rs`. A coordinated
re-pin to a released Eventlog is needed regardless, and the durable form belongs in that release.

## What a bounded handle reads from

Under `ProviderTracked` every scoped read is answered from the whole tracked model
(`scoped.rs:150-157`), and the suffix verifier holds every bound blob's bytes and every
projection row besides it (`Held`, `tracked.rs:29-35`). With today's read path, an open that
installs no model only moves the complete verification to the first read.

**A persisted whole model** would give the handle the same `Held` it has after a complete open.
`CapturedModel` has no canonical encoding today (`adapter.rs:3657-3684`; the only model digest is
test-only and renders `Debug`, `adapter.rs:5294-5328`), and one could be written. The baseline
rules it out: its load cannot cost less than parsing the record text it carries, and that floor
(`record_parse`) is 22.9 ms at 55 events and 381.1 ms at 1,203, about a fifth of today's `verify`
at every size. The implementation story's bound (1,203 events within twice the cost of 55) is out
of reach: the floor alone at 1,203 exceeds twice today's whole open at 55 (2 × 138.6 ms), and a
bounded open at 55 costs less than today's. The store would still cost more to open every day.

**The verified index rows** are a model the provider already keeps. A complete verification holds
every row of the four fixed indexes equal to its rendering of the verified events
(`validate_projection_sets`, `adapter.rs:4463`), and every tracked advance holds each changed row
(`tracked.rs:400-450`). The rows name what a command reads:

- the subject row: revision, origin, the blob of the record or anchor its state comes from
  (`state_source`) and the physical head (`crates/entity-eventlog/src/projection.rs:209-212`);
- the record row: subject, revision, batch key, member index, record, request and batch blobs, and
  physical position (`projection.rs:194-200`);
- the batch row: its batch blob and every member's record, blobs and position
  (`projection.rs:242-251`).

Under (a), nothing but appends the open verifies has written them since the checkpoint. A state
read is then one row and one blob, whatever the store's size. This is chosen.

## Chosen design

### Where the checkpoint is persisted

In the provider, as the Eventlog snapshot of one stream the bound authority owns: stream type
`er.open-checkpoint`, stream id the framed key of the authority (built as the subject stream key
is), in the bound tenant. It is written with `snapshot_generation` and `save_snapshot_checked` and
read with `load_snapshot` (`eventlog@6983cc25:crates/eventlog-core/src/lib.rs:901-944`), which all
four providers implement.

- Snapshot tables are not captured material, so a checkpoint never enters a capture, a model, a
  read bound or a projection. Run, not only read:
  `a_checkpoint_snapshot_is_not_captured_and_writing_it_keeps_in_process_continuity`
  (`crates/entity-eventlog/tests/review_open_checkpoint.rs`) captures the tenant before and after
  such a snapshot write and gets equal captures, and a store holding the snapshot still opens.
  Against Eventlog 0.8.0 the write also keeps the handle's in-process continuity; against the
  pinned `6983cc25` it ended it, which the test asserted under its earlier name.
- One authority, one backup: the checkpoint is restored, copied and forgotten with the store;
  `forget_tenant` removes it (`eventlog@6983cc25:crates/eventlog-sqlite/src/lib.rs:2132-2148`).
- No new IO edge: no path beside the provider, which `AGENTS.md:245-254` would have to name and
  which a PostgreSQL owner does not have.
- A blob was rejected: blobs are captured, orphans included, and would count against the tenant's
  read bounds on every complete capture.

### What it binds

One canonical record under its own framed domain:

| field | source | why |
|---|---|---|
| `format` | constant `er.eventlog.open-checkpoint/1` | an unknown format is never interpreted |
| `verifier` | `entity-eventlog`'s crate version | a newer verifier re-verifies completely what an older one accepted |
| `authority` | `Authority` (logical scope, tenant, stream identity) and the binding `PhysicalRef` | a checkpoint for another generation is never applied |
| `projections`, `limits` | `projection_specs()`, the handle's `CaptureLimits` | the provider binds both too (`tracked_capture.rs:94-95`) |
| `provider` | the durable form of the provider checkpoint the handle holds, which names the observation it last verified | the proof about the prefix |
| `position` | the last verified tenant position (`Held.last_position`, `tracked.rs:97`) | the suffix starts after it; a head behind it is a truncation |
| `digest` | framed SHA-256 of all of the above | corruption of the record itself |

The `provider` bytes always come from the checkpoint the provider issued with the handle's last
verified observation. Entity Runtime never asks for a durable checkpoint of "now": the epoch they
carry is the one the provider read inside that observation's transaction (see *The Eventlog
capability*), so a foreign write after the handle's last read and before its drain stays visible
to the next open.

It binds **no model digest**. The verified model is the provider's rows, and the continuity proof
is what says they are the ones verified. A digest over them could be checked only by reading every
row, which is the work a bounded open removes; a complete verification re-derives the model from
the events anyway. The story's "canonical digest of the verified model" is replaced by the
provider proof plus the position.

It binds **no read-bound totals** either. The provider already binds the usage of the observation
to its checkpoint (`eventlog@6983cc25:crates/eventlog-sqlite/src/tracked_capture.rs:96`); a second
copy in this record would be compared with nothing on `Unchanged`, because `CaptureCheckpoint` is
opaque (`eventlog@6983cc25:crates/eventlog-core/src/capture.rs:47-52`), and a copy taken from the
handle's `CaptureHeld`, which its own writes advance optimistically (`adapter.rs:1160-1170`), could
differ from the provider's without anything noticing. One copy is kept, the provider's (see
*Tenant totals*).

### When it is written

- **On `ShutdownMode::Drain` and on an explicit facade call** (for a long-lived owner), when all of
  these hold:
  - the provider returned a durable form of the handle's held checkpoint;
  - the handle has not refused with `ProviderIntegrity` since its last verification;
  - the open loaded no valid checkpoint (none, a tombstone, or one discarded for a reason in *What
    invalidates it*), or any field the record binds differs from the persisted record's;
  - the record persisted now, read again just before writing, is not a valid checkpoint at a
    higher position than the held one. Only a record whose `digest` and `authority` hold, and
    that is not a tombstone, counts as one; a damaged record never blocks a write;
  - if the record persisted now is a tombstone, it is the one this handle loaded at its open. A
    handle that opened before a discard never writes over it.
- **Never mid-life by itself.** Against the pinned `6983cc25` a snapshot write changed the
  connection's `total_changes`, part of the in-process stamp (`tracked_capture.rs:20-41`), so the
  next observation discarded the in-process journal (`tracked_capture.rs:108-113`) and a mid-life
  write bought the next read a complete capture. Eventlog 0.8.0 re-synchronizes the stamp inside the
  snapshot write's own transaction, as the Eventlog capability asked, and the review test above now
  shows the next read takes no capture. The implementation still writes only at a drain and on the
  explicit call: a record per read would cost a snapshot write per read and name nothing a drain
  does not.

The third condition replaces a discarded checkpoint even when the head has not moved: after an
upgrade (new `verifier`), a change of limits or a failed digest, the first process to drain writes
a valid one, so read-only processes do not pay a complete verification on every open until some
write moves the head. The fourth keeps a long-lived handle at an older position from replacing a
newer checkpoint; because it compares only with a valid record, a damaged record whose position
still decodes, even one above the head, is replaced by the next drain rather than blocking every
drain until a discard. The fifth keeps a discard from being undone by a handle that was live when it
ran (see *What invalidates it*). The fourth is not atomic: the snapshot port has no compare-and-set (`save_snapshot_checked`
accepts any write that names the stream's generation,
`eventlog@6983cc25:crates/eventlog-sqlite/src/lib.rs:2062-2066`), so two drains racing can still
leave the lower one last. That costs the next open a longer delta, never correctness, because the
`provider` bytes of either record name an observation that was verified.

The implementation's tests for this rule: a read-only drain after a `verifier` change writes a
valid checkpoint at the unchanged head, and the next open is bounded; a handle whose held position
is below the persisted valid one writes nothing; a drain over a damaged record above the head
writes; a handle opened before a discard leaves the tombstone in place at its drain.

The record is small, so writing it at every drain is cheap. A process that exits without draining
leaves the previous checkpoint; the next open verifies a longer suffix, or completely once the
provider's journal no longer reaches it. Both are correct; only the cost differs.

### What invalidates it

| condition | result |
|---|---|
| snapshot absent, unreadable, unknown `format` or `verifier`, or a tombstone | complete verification |
| `digest` does not match | complete verification |
| `authority`, `projections` or `limits` differ from the handle's | complete verification |
| the provider cannot restore the durable bytes, or answers the restored checkpoint with `Complete` | complete verification |
| a checkpoint whose `digest` and `authority` hold names a `position` beyond the complete capture's last position | refuse the `ProviderTracked` open: `ProviderIntegrity`, naming the checkpoint's position and the head |
| the handle refused with `ProviderIntegrity` after its last successful verification | no checkpoint is written at drain |

A damaged checkpoint is a damaged cache: discarded, never trusted and never fatal, as an Eventlog
snapshot that fails to decode is discarded (`eventlog@6983cc25:crates/eventlog-core/src/lib.rs:713-716`).
For a damaged checkpoint the worst outcome is the cost of today's open. For a forged one there are
two exceptions, both open only to a writer with SQL access to the file who knows the encodings.

The first is a checkpoint ahead of the head. A store truncated at its tail is self-consistent,
so a complete verification accepts it, and only the checkpoint knows events are missing; that is
why this condition refuses rather than falls back. The price is that anyone who can write SQL to
the file can write such a checkpoint with a recomputed digest (the construction is public,
`crates/entity-eventlog/src/encoding.rs:333-343`) and so refuse every `ProviderTracked` open. The
recovery is explicit, and there are two:

- **An explicit discard call**, `discard_open_checkpoint` (an administrative capability beside
  provisioning and `rebuild_indexes`), after an operator has decided the store is to be used as it
  is. The snapshot port has no delete: eventlog-core offers `save_snapshot`, `snapshot_generation`,
  `save_snapshot_checked` and `load_snapshot` (`eventlog@6983cc25:crates/eventlog-core/src/lib.rs:901-942`),
  and only `forget_tenant` removes a snapshot. So discard overwrites the record with a tombstone:
  the same `format`, a `discarded` marker, the authority, and `replaces`, the framed digest of
  the state it overwrote, so no two tombstones are equal unless they replaced byte-identical
  records, and the `digest`. The design asked for a random nonce; no library here reads a random
  source on the caller's behalf, and naming the replaced state gives the same answer for every
  sequence of discards a store can reach. An open treats a tombstone as no checkpoint, verifies
  completely and, at its drain, writes a new checkpoint over it.
- **`FullVerification` opens never read the checkpoint**, so the store stays readable and writable
  under that policy meanwhile.

Discard is run with no `ProviderTracked` handle live on the store. A handle that opened before
the discard holds the record it loaded and would otherwise write it again at its drain, bringing
the refusal back. The library cannot see other processes, so the caller knows this the way it
knows any administrative precondition: within one process, the discard call is offered on the
owner (`EventlogRecordedStoreOwner`) before a facade is started from it, so no facade of that
process exists; across processes, the operator shuts down every owner of the store first, as the
refusal's message says. The fifth drain condition makes a violation harmless rather than relying on
it: a handle whose loaded record is not the tombstone now persisted writes nothing.

Falling back instead of refusing was rejected: it would turn the only detection a bounded open
adds into silence, while the denial it prevents is one the same writer can already cause by
damaging any verified byte, which also refuses every open until repaired.

The second exception is a record whose `provider` bytes understate the usage. The durable form
binds values anyone who can read the file can also read (epoch, token, store instance, position,
schema version), so a writer who knows the provider's encoding can build bytes that restore, with a
recomputed record `digest`. `checkpoint_usage` then hands `admit_growth` totals that are too low, and
the handle can commit past its own read bounds, after which complete captures of the store are
refused at the bound. R-151 says a command "is refused with `BatchExceedsReadBounds` before any
upload when it would leave the tenant holding more events, blobs or index rows than the handle's
`CaptureLimits`" (`docs/requirements.md:224`). The same writer can exceed the bounds directly, by
inserting rows or blobs through SQL, so this adds no capability the attacker lacked. The
implementation changes R-151 to say what is counted: totals "counted from the usage the provider
bound to the handle's last verified observation, or from its last complete capture, plus its own
writes since; a writer with SQL access to the store can understate that usage, as it can exceed
the bounds directly".

### How a bounded open runs

1. Load the snapshot. Check `format`, `verifier`, `digest`, `authority`, `projections` and
   `limits`, and that the tenant's binding row names this authority and the record's binding
   event. Any failure: step 4.
2. Restore the provider's checkpoint from its durable bytes and call
   `capture_tenant_since(tenant, projection_specs(), limits, Some(&restored))`.
3. `Unchanged`: the handle starts at the checkpoint's position, with the totals the provider bound
   to its checkpoint (`checkpoint_usage`); a provider that returns none sends the open to step 4.
   `AppendDelta`: verify the delta (below) and start after it, with its `resulting_usage`. Either
   way the checkpoint the provider returned becomes the handle's in-process checkpoint.
4. `Complete`, or any fallback above: `build_model` over the complete capture, as today; then
   compare the capture's last position with the checkpoint's. The handle holds the whole model, as
   every `ProviderTracked` handle does today.

The usage step 3 starts from is kept with the bounded observation, not taken from the handle's
read-bound totals: a handle's own write advances those before the provider reports it, and a
suffix measured against them would never add up. Every tracked handle also keeps the highest
position it has verified, and refuses a later complete capture whose head is behind it: history is
append-only, so that is a store that lost its tail under a live handle.

**Verifying a delta without a whole model** is `advance` (`tracked.rs:210-470`) with its prior
state taken from the delta's before-values instead of a held model: for each subject the delta
touches, the state the before-value of its subject row names; for each new record id and batch
key, an absent before-value of its row. Each new record is then checked as `advance` checks it:
wrapper and blobs against their digests, positions after the subject's physical head, the record
against the prior state, receipts, every changed row's after-value equal to the rendering of the
new records with none omitted, and the usage totals. A blob a wrapper names that the delta does not
carry is read with `get_blob` and held to its digest. A subject whose origin is not genesis, or
whose history branches, sends the handle to complete verification, as `advance` does today
(`tracked.rs:280-292`).

`CapturePolicy::FullVerification` never reads or writes the checkpoint. Its open, every complete
read and `rebuild_indexes` stay complete verifications: that is the full verification on demand.

### How a bounded handle answers

Before each read the handle asks the provider to continue from its in-process checkpoint:
`Unchanged` costs nothing more, `AppendDelta` is verified as above, `Complete` makes the handle a
whole-model handle through a complete verification. Then:

| read | answered from | cost | verified by |
|---|---|---|---|
| a subject's state and revision | its subject row and the `state_source` blob | one row, one blob | the last complete verification and every delta since, attested by the continuity proof; the blob held to its digest |
| a record lookup | its record row and the blobs it names | one row, at most three blobs | the same |
| a batch lookup | its batch row and member blobs | one row, the members' blobs | the same |
| a subject's history | its stream, read per entity as `FullVerification` reads it (`scoped.rs`) | that subject's history | the per-entity verifier, as today |
| a complete snapshot, the recorded refusals | a complete capture | the whole store | `build_model`, as today |
| a write's preflight and read-back | the streams, blobs and index rows its scope names, read per entity as a `FullVerification` handle's write reads them; its own append as a delta | the histories of the subjects it writes | the per-entity verifier, and the suffix verifier for the append |

The implementation answers a write's preflight per entity rather than from rows alone. The append
path, the executor's reads and recovery take a model of the scope, histories included; the
per-entity reader builds one, verified, from the subjects' streams, which no row holds. Its cost is
the written subjects' histories, as on a `FullVerification` handle, never the whole store.

No read is answered from content nobody verified: each row-backed answer names the complete
verification and the deltas that held its row, and the implementation shows that in a test per
read (the implementation story's "no silent unverified read").

### No silent unverified read

Every verification a bounded open no longer does, where it happens instead, and the test that
shows it (tests in `crates/entity-eventlog/tests/bounded_open.rs` unless named otherwise; the
unit tests are in `crates/entity-eventlog/src/adapter/bounded/tests.rs` and
`crates/entity-eventlog/src/adapter/checkpoint.rs`):

| verification a complete open does | where it happens after a bounded open | test |
|---|---|---|
| every event, blob and row before the checkpoint, held to the events | in the complete verification that wrote the checkpoint, then the provider's continuity proof: any SQL write since gives `Complete`, and the open verifies completely | the five `checkpoint-current-*-tamper` ESS scenarios; `a_foreign_write_under_a_live_bounded_handle_makes_its_next_read_complete` |
| the same, on demand | a `FullVerification` open, which never reads the checkpoint, and every complete read | `full_verification_verifies_the_whole_history_beside_a_valid_checkpoint`; `a_complete_read_of_a_bounded_handle_verifies_the_whole_store_once` |
| the events after the checkpoint | the suffix verifier, from the rows they change and the blobs they bind | `an_open_after_appends_verifies_only_the_suffix`; `a_suffix_verified_from_rows_is_accepted_whole_and_refused_for_every_forged_coordinate`; `a_suffix_continuing_another_state_than_its_subject_row_names_is_refused` |
| the binding and the stream identity | the binding row read on every bounded open; the checkpoint's own `authority`; the provider's check of a restored checkpoint against the stored identity | `a_checkpoint_naming_another_binding_event_is_not_applied`; `checkpoint-current-identity-tamper` |
| the checkpoint itself | its digest, format, verifier, projections and limits on every open; a damaged one costs a complete open | `a_record_changed_without_its_digest_is_damaged_and_another_verifier_is_foreign`; `tampered-checkpoint-falls-back-to-complete` |
| a tail the store lost | the position the checkpoint names, after the complete verification it falls back to | `a_checkpoint_beyond_the_head_refuses_tracked_opens_until_discarded`; `checkpoint-ahead-of-head-refuses-until-discarded` |
| each state, record and batch read | the row the verifications above held, and every blob it reads held to its digest; a raw edit the provider cannot see is refused by the read that reads it | `every_answer_of_a_bounded_handle_equals_a_complete_handles`; `a_raw_file_edit_is_refused_by_the_read_of_the_edited_blob_and_by_full_verification` |
| each history and a write's preflight | the per-entity verifier, as on a `FullVerification` handle; the write's own append as a suffix | `every_answer_of_a_bounded_handle_equals_a_complete_handles`; `a_bounded_handle_verifies_its_own_write_as_a_suffix` |
| the read-bound totals | the usage the provider bound to the checkpoint, advanced by each verified suffix; a digest the handle has not seen bound is asked of the provider | `a_bounded_handle_refuses_a_write_past_its_read_bound_as_a_whole_handle_does` |

### Tenant totals for `admit_growth`

`admit_growth` holds a write against the handle's event, blob and row totals and the set of bound
digests (`adapter.rs:1116-1158`, the totals' type at `:3636-3642`). The totals have one source,
the provider: the usage it bound to the checkpoint's observation (`tracked_capture.rs:96`), which
the Eventlog capability exposes as `checkpoint_usage`, and on a delta its `resulting_usage`, which
`advance` already holds to the additions it admitted (`tracked.rs:451-462`). The Entity Runtime
record carries no copy, so there is nothing to disagree with on `Unchanged` (see *What it binds*).
Within one handle, its own writes advance the totals in memory as today (`adapter.rs:1160-1170`),
and the next delta or complete verification replaces them with the provider's. The digest set is not carried: it
is as large as the store's blob count. A bounded handle asks the provider whether each blob a write binds is already bound
(`get_blob`, one point read per blob the write binds), so it counts exactly what a whole-model
handle counts.

### Is an unkeyed digest enough

Yes, for what it is for. The checkpoint's digest is unkeyed framed SHA-256
(`crates/entity-eventlog/src/encoding.rs:333-343`). It catches corruption of the checkpoint and
uninformed edits to it. It does not stop a writer who rewrites the checkpoint and recomputes the
digest. Neither does today's complete verification stop a writer who rewrites content and every
digest that names it: every digest it checks is the same unkeyed construction, stored in the same
database (an event names its wrapper by digest, the wrapper names its record, request and batch
blobs). A keyed checkpoint would need a key, and no library here selects credentials on the
caller's behalf (`AGENTS.md:253-254`). A caller-supplied key is a possible later option, not part of
v0.1. This is inferred from the digest construction, not from an attack that was run.

### What a bounded open does not detect

Against a complete open, a bounded open does not detect:

- **Changes that bypass SQL while no handle is open**: raw writes to the database file, a copied-in
  page, media damage. Today they are outside the live-handle guarantee but caught by the next open
  (`eventlog-recorded-adapter-v0.1.md:399-400`); after this change only a `FullVerification` open
  or a complete read catches them. The implementation states this narrowing in R-151,
  `AGENTS.md:245` and `CHANGELOG.md`.
- **A writer that defeats the provider's change recording on purpose**: a connection that disables
  triggers (`SQLITE_DBCONFIG_ENABLE_TRIGGER`) or rewrites the provider's continuity tables
  consistently. It is in the same class as the informed writer above.
- **Damage that only a newer verifier would refuse**: bounded by the `verifier` field, which sends
  the first open after an upgrade through complete verification.

It does detect, as today, every SQL write through any connection to events, blobs, the identity row
and index rows, and a substituted stream identity; and, unlike a complete open, a store whose head
is behind the checkpoint.

### The unchanged-head tamper scenarios

Each scenario provisions, creates one record, mutates through a second SQLite connection without
moving the event head, then expects a warm `Snapshot` and a `Reopen` to refuse with
`ProviderIntegrity` (the mutation, warm read and reopen steps at `:23-37` of each file; the
fixtures are `checks/ess-conformance/src/provider.rs:224-231`). `Provision` provisions through a
facade that opens with `FullVerification` (`crates/entity-eventlog/src/sync.rs:613`), then reopens
under the scenario's policy (`provider.rs:132-159`). `Reopen` drains the old facade first
(`provider.rs:81-85`, `:86-106`). So no checkpoint exists at the final `Reopen`: the provisioning
facade never writes one, and the tracked handle refused before its drain.

| scenario | mutation | warm `Snapshot` | `Reopen` |
|---|---|---|---|
| `unchanged-head-event-tamper` | head event's `data` replaced | refused by the in-process proof, unchanged | no checkpoint exists; the complete open refuses. Unchanged |
| `unchanged-head-blob-tamper` | first blob zeroed | same | same. Unchanged |
| `unchanged-head-delete-blob-tamper` | first blob deleted | same | same. Unchanged |
| `unchanged-head-identity-tamper` | stream identity substituted | same | same. Unchanged |
| `unchanged-head-projection-tamper` | every subject row's body replaced | same | same. Unchanged |

Each still passes under `FullVerification`, which never reads a checkpoint. None needs
`--coverage-review`.

They do not exercise the checkpoint: their `Reopen` follows a refusal. Nor can a variant built
only from the commands the specification declares today
(`ess/provider-tracking/domains/operations.yaml:5-86`: `Register`, `Provision`, `Reopen`,
`Create`, `Batch`, `Load`, `Histories`, `LookupBatch`, `Snapshot`, `SqlMutate`): no checkpoint is
written until durable continuity is enabled (*Installing durable continuity*), so such a variant
would reopen completely and pass without touching a checkpoint. The implementation story writes
these commands into `operations.yaml` first, before any code:

| command | what it does | why the scenarios need it |
|---|---|---|
| `entity-provider.tracking.EnableDurableCheckpoints` | runs the administrative enable call on the open facade's store | without it no checkpoint is ever written |
| `entity-provider.tracking.DiscardCheckpoint` | closes the facade, runs `discard_open_checkpoint` on the owner, leaves the store closed | the recovery from a checkpoint ahead of the head, and the tombstone rule |
| `entity-provider.tracking.KeepCopy` | fixture: drains and closes the facade, copies the SQLite file aside, leaves the store closed | the starting point of a tail truncation |
| `entity-provider.tracking.TruncateTail` | fixture: drains and closes the facade, puts the kept copy back but carries the current checkpoint record into it, leaves the store closed | a self-consistent store whose head is behind its checkpoint, which no SQL edit of one row produces |

A tampered checkpoint record needs no new command: it is a new `kind` of `SqlMutate`, whose request
is a JSON document the harness interprets (`checks/ess-conformance/src/provider.rs:224-231`).

Each tamper variant then runs: `Provision`, `EnableDurableCheckpoints`, `Create`, `Reopen` (the
drain writes a checkpoint at the head), the same `SqlMutate` with no read after it, and `Reopen`
under `ProviderTracked`, expecting `ProviderIntegrity`. The second handle opened from the
checkpoint and never read after the mutation, so its record would equal the persisted one and its
drain writes nothing; were it written, its `provider` bytes would still name the observation before
the mutation. The second `Reopen` starts from the checkpoint. Under (a) the provider answers it with
`Complete` and the complete verification refuses. Under (b) the event, blob, delete-blob and
projection variants would open, and a row replaced with well-formed content would be served by
row-backed reads.

The truncation scenario runs, in order: `Provision`, `EnableDurableCheckpoints`, `Create`,
`KeepCopy`, `Reopen`, `Create`, `Reopen` (the drain writes a checkpoint at the second record's
head), `TruncateTail`; then `Reopen` under `ProviderTracked` expecting `ProviderIntegrity`, `Reopen`
under `FullVerification` expecting success, `DiscardCheckpoint`, and `Reopen` under
`ProviderTracked` expecting success.

### Cost

A bounded open whose provider answers `Unchanged` reads one snapshot row and asks the provider one
continuity question; with a delta it also verifies the delta. A state read is one row and one blob.
Neither touches the verified prefix, so both are flat in the store's size. The start-up work that
remains (the worker thread, the runtime, opening SQLite, attaching the projector) is expected to be
flat too, and is not measured separately here. `start` exceeds `verify` by 8–13 ms at 55 events
but by 169–379 ms at 1,203, run for run. Opening SQLite and attaching the projector read the schema
and the projection registry, not rows (`eventlog@6983cc25:crates/eventlog-sqlite/src/lib.rs:305-320`,
`inline_admin.rs:81-125`), so the excess is more likely the worker thread allocating the capture
and model afresh. That is a hypothesis; the implementation's probe settles it. The implementation
story measures the whole with the probe in `crates/entity-eventlog/tests/shared_clock_cost.rs`
against the baseline above, at 1,203 events within twice the cost at 55.

Reads of a whole history and complete reads stay proportional to what they return.

Two kinds of history send an open, and a live handle's next read, back to complete verification,
at today's cost:

- **A suffix holding any event other than a recorded entry.** The delta verifier inherits
  `advance`'s refusal of every event not named `er.recorded_entry` or carrying a digest
  (`tracked.rs:216-225`). Recorded refusals (`er.refused_request`), imported anchors
  (`er.import_anchor`) and the binding (`er.binding`) are such events
  (`crates/entity-eventlog/src/projection.rs:118-122`).
- **A provider write outside an atomic group.** Recording a refusal binds its blob with a
  standalone `put_blob` and appends its event with a single `append`
  (`crates/entity-eventlog/src/adapter.rs:2794-2806`). Neither is an acknowledged atomic group, so
  it ends in-process continuity today (`tracked.rs:764-783` shows it for a standalone `put_blob`)
  and, under the Eventlog capability below, durable continuity too.

For the consumer this means: a command executed through `Executor::recording_refusals`
(`crates/entity-executor/src/lib.rs:290`) that records a refusal makes the next open a complete
verification, and the bound holds only while the suffix since the last checkpoint holds recorded
entries alone. One complete open writes a new checkpoint at its drain, so the cost is paid once per
refusal-bearing suffix, not on every later open. Verifying refusal events in the delta, and
recording a refusal as one atomic group, are the follow-up that removes it; this design does not
include them.

### Installing durable continuity on a store

The durable proof needs every write to the captured tables to leave a durable mark, and on SQLite
the natural mark is a provider-owned trigger. Today's provider refuses a store that carries any
trigger on two of those tables, and withholds continuity for a trigger anywhere:

| check | where (pinned `6983cc25`) | what a trigger causes |
|---|---|---|
| blob table, at every open | `crates/eventlog-sqlite/src/lib.rs:953-963`, reached from `open_existing` (`:318`) through `require_existing_schema` (`:395`); the same check on Eventlog `main` at `lib.rs:950-961` | the open refuses: `Invalid("unsupported SQLite blob trigger or table behavior")` |
| projection tables, at attach | `inline_admin.rs:106` into `admit_projection`, the trigger count at `capture.rs:639-647` | the attach refuses; the facade reports "projection admission does not match durable structure" |
| the inspector | `inspection.rs:351-360` | `UnsupportedSource` |
| continuity, at every capture | `capture.rs:149-159` | nothing is refused; the provider issues no checkpoint, so every tracked read recaptures |

`crates/entity-eventlog/tests/review_open_checkpoint.rs` runs the first, second and fourth rows
against the pinned provider (the three `#[ignore]`d trigger probes, which need the `sqlite3`
shell): a blob-table trigger refuses the open and the facade start, a projection-table trigger
refuses the facade start, and an event-table trigger leaves a tracked open taking two captures and
two model builds where one of each would do.

Installing the provider's triggers is therefore **one-way** for every older reader. Once a store
carries them, Entity Runtime 0.26.0 and older, and Eventlog `6983cc25` and Eventlog `main`, cannot
open it or attach to it again. Every process that opens the store, including any second tool, must
run the new versions before the store is migrated, and the migration must be something an operator
chooses. The consumer known to open Eventlog SQLite files through `entity-eventlog` is
beyond10x/connectors, which pins Entity Runtime `0.26.0` with `entity-eventlog`'s `sqlite` and
`sync-bridge` features (`Cargo.toml:21` at connectors `origin/main` `da4f9d6d`, read 2026-10-06).
Its operators' stores are the ones the one-way step reaches: every connectors process on a store,
including older installed binaries, must carry the new Entity Runtime before that store is
enabled. The other consumers listed at `AGENTS.md:222-236` name neither `entity-eventlog` nor an
`eventlog-facade` or `eventlog-providers` feature in any manifest at their `origin/main` (aep
`5a2a0e5f`, aep-service `237a7fd`, atlas `5a2c04e4`, bench `e3c44fd`, read 2026-10-06 with
`git grep` over `*Cargo.toml`).

So durable continuity is never installed implicitly, neither by an open nor by provisioning:

- **Eventlog:** `SqliteEventStore::enable_durable_continuity` installs the triggers and the
  continuity tables on an existing store in one transaction, is idempotent, and refuses a store
  carrying any trigger it does not own. `SqliteEventStore::disable_durable_continuity` removes them
  again, after which older versions open the store as before. `open_existing` keeps never creating
  tables (`lib.rs:112-113`), and a store without the triggers keeps today's behaviour:
  `durable_checkpoint` returns `None`.
- **Entity Runtime:** an administrative call on the facade, beside provisioning and
  `rebuild_indexes` (named in the implementation; `enable_durable_open_checkpoints` is the working
  name), runs the Eventlog call for the store its owner names, together with its reverse. Until
  it runs, a `ProviderTracked` open writes no checkpoint and verifies completely, as today.
- The new provider's three refusing checks and its continuity check admit exactly its own triggers,
  by name and SQL text, and give their present answer for any other trigger.
- A table created after the enable call gets no trigger from it: a projection table that
  `create_projections` makes later (Entity Runtime calls it when provisioning,
  `crates/entity-eventlog/src/sync.rs:569-572`), or a future index table. A foreign edit to such a
  table would move neither epoch nor token. So the issue requires that `create_projections` on an
  enabled store installs the trigger in the same transaction or refuses, and that a restored
  checkpoint continues only if every projection table it names carries the provider's trigger;
  the trigger count today's projection check reads (`capture.rs:639-647`) cannot tell whose
  trigger it is.

### The Eventlog capability

The text below is written to be filed as an Eventlog issue unchanged; its paths are relative to the
Eventlog repository at `6983cc25`. The implementation depends on it through a dependency blocker on
the Eventlog capability.

**Prerequisite.** Land `6983cc25` (in-process SQLite capture continuity, branch
`fix/er-51-capture-checkpoints`) on `main` and release it; Entity Runtime 0.26.0 pins that commit.
(Done: Eventlog 0.8.0 carries both, beyond10x/eventlog#39. The API below is the released one; the
behaviour bullets are corrected where the release answered them.)

**API (additive: default trait methods keep every provider compiling unchanged; the migration calls are SQLite-only).**

```rust
/// Provider-encoded bytes naming one observation a later process may continue from.
///
/// Opaque to consumers. The provider's encoding names its own kind and format version.
#[derive(Clone, PartialEq, Eq)]
pub struct DurableCaptureCheckpoint(Vec<u8>);

impl DurableCaptureCheckpoint {
    pub fn from_bytes(bytes: Vec<u8>) -> Self;
    pub fn as_bytes(&self) -> &[u8];
}

pub trait ConsistentTenantCapture {
    // existing methods unchanged

    /// The persistable form of `checkpoint`, if this provider can later prove continuity from
    /// its observation after the issuing process and connection are gone.
    fn durable_checkpoint(&self, checkpoint: &CaptureCheckpoint)
        -> Option<DurableCaptureCheckpoint> { None }

    /// A checkpoint `capture_tenant_since` accepts in place of the one `durable` was made
    /// from. Unknown, foreign, malformed or newer-format bytes give `None`, never an error.
    fn restore_checkpoint(&self, durable: &DurableCaptureCheckpoint)
        -> Option<CaptureCheckpoint> { None }

    /// The usage this provider bound to `checkpoint`'s observation, the same counts a complete
    /// capture of that observation would report. `None` for a checkpoint it did not issue.
    fn checkpoint_usage(&self, checkpoint: &CaptureCheckpoint)
        -> Option<CaptureUsage> { None }
}

impl SqliteEventStore {
    /// Installs the provider-owned change recording durable continuity needs, in one
    /// transaction. Idempotent. Refuses a store that carries any trigger the provider does not
    /// own. Older Eventlog versions refuse a store carrying it (see below).
    pub async fn enable_durable_continuity(&self) -> Result<(), EventLogError>;

    /// Removes it again, after which older Eventlog versions open the store as before.
    pub async fn disable_durable_continuity(&self) -> Result<(), EventLogError>;
}
```

**Contract.** For a restored checkpoint, `capture_tenant_since` returns `Unchanged` only if no write
of any kind reached the tenant's captured material (events, blobs, stream identity, the requested
projections) since its observation; `AppendDelta` only if every such write was an atomic group this
provider's append paths acknowledged, by any process, with exactly the delta the in-process path
returns (events, blobs bound, projection row changes with before and after values,
`resulting_usage`); and `Complete` otherwise. "Its observation" is the capture the checkpoint was
issued with: `durable_checkpoint` encodes the durable state read inside that capture's transaction,
never the state at the time it is called, so a foreign write between the observation and the call
is reported, not absorbed. A false `Complete` is always allowed; a false `Unchanged` or
`AppendDelta` never is. The checkpoint returned with any variant is again eligible for
`durable_checkpoint`.

**SQLite behaviour.**

- Durable continuity exists only on a store where `enable_durable_continuity` has run;
  `open_existing` keeps never creating tables (`crates/eventlog-sqlite/src/lib.rs:112-113`), and
  provisioning does not install it. Without it, `durable_checkpoint` returns `None`.
- Every write to captured tables through any connection advances a durable mutation epoch and
  replaces a durable random token (`randomblob(16)`) in the same statement; provider-owned triggers
  are one way to do it.
- The in-process checkpoint reads the epoch and token inside the same transaction as its stamp
  (`capture.rs:135-137`), and the durable form binds those: the store instance identity minted at
  `enable_durable_continuity`, the prefix, tenant, stream identity, projections, limits, usage,
  position, `PRAGMA schema_version`, epoch and token.
- Each acknowledged atomic group records, in the group's transaction, a durable journal entry:
  positions from and to, epoch and token before and after, the group's events, the blob digests it
  bound and its row changes. The journal is bounded and pruned from the oldest entry, as the
  in-memory journal is (`tracked_capture.rs:16-17`).
- A restored checkpoint continues as `Unchanged` when epoch, token, schema version, store instance
  and position all equal the current ones, and as `AppendDelta` when the journal entries after its
  position form an unbroken chain of epoch and token ending at the current ones. A foreign write
  breaks the chain; a dropped or altered trigger changes the schema version; each gives `Complete`.
- A restored checkpoint continues only if every projection table it names carries the provider's
  trigger, checked by name and SQL text in the same transaction; otherwise `Complete`. A table
  created after `enable_durable_continuity` ran would otherwise take foreign edits that move
  neither epoch nor token. For the same reason `create_projections`
  (`crates/eventlog-core/src/lib.rs:974`, SQLite `crates/eventlog-sqlite/src/lib.rs:2508`) on an
  enabled store installs the trigger on each table it creates, in the same transaction, or refuses.
- Installing the triggers is one-way for every older Eventlog version. Today's provider refuses a
  trigger on the blob table at every open (`lib.rs:953-963`, reached from `open_existing` at `:318`
  through `require_existing_schema` at `:395`; the same check on `main`, `lib.rs:950-961`) and on a
  projection table at attach (`inline_admin.rs:106`, `capture.rs:639-647`), and its inspector
  refuses one too (`inspection.rs:351-360`). Its continuity check (`capture.rs:149-159`) refuses
  nothing; it only withholds the checkpoint. The new provider's three refusing checks and its
  continuity check admit exactly its own triggers, by name and SQL text, and give their present
  answer for any other trigger.
- The provider's own writes to tables outside the captured material (snapshots, snapshot
  generations, the continuity tables) end neither durable nor in-process continuity. Eventlog
  0.8.0 does this: its snapshot write re-synchronizes the in-process stamp inside its own
  transaction.
- A standalone `put_blob`, `delete_blob` or `append` outside an atomic group is not journaled in
  Eventlog 0.8.0: it moves the mark without an entry, so the next restored checkpoint gives
  `Complete`. So does projection administration.
- A tenant with redacted history gives `Complete` from a restored checkpoint, never a delta, and a
  cap that the restored checkpoint's usage plus the delta would cross gives `Complete`, never a
  limit refusal: the usage came from caller-held bytes, and the complete path applies the limits to
  the real observation (Eventlog 0.8.0, `docs/design/durable-capture-continuity.md` there).

**Known limit of counters, and why the token.** With epoch and position alone, a copy of the file
restored from an older backup and then written to as many times as the groups it lost carries the
same epoch and position as a newer durable checkpoint, and the provider would answer `Unchanged` or
`AppendDelta` for content it never journaled. The random token replaced on every write makes that
coincidence a 128-bit collision. A consumer that keeps its checkpoint in the same file, as Entity
Runtime does, restores the older checkpoint with the file and does not reach this case; one that
keeps it elsewhere does.

**Out of scope, stated in the provider's documentation.** Raw file edits that bypass SQLite; a
connection that disables triggers or rewrites the continuity tables consistently. PostgreSQL, file
and tree providers keep the default (`None`, hence `Complete`).

**Acceptance (conformance, real SQLite file, a new process or `SqliteEventStore` for each step).**

- `Unchanged` from a restored checkpoint on an untouched store; an exact `AppendDelta` after
  appends made by another store instance; `checkpoint_usage` equal to a complete capture's usage.
- `Complete` after each of: an event's data updated, a blob's bytes updated, a blob deleted, the
  stream identity updated, a projection row updated, a projection row inserted, an event inserted
  by SQL, a trigger dropped and recreated, the journal pruned past the checkpoint, an older copy of
  the database file restored, and an older copy restored and then written as many times as it lost.
- `Complete` after a foreign write made between the observation and the `durable_checkpoint` call:
  capture, write through a second connection, then take the durable form and restore it.
- `None` from `restore_checkpoint` for truncated, foreign-provider and newer-format bytes, and from
  `durable_checkpoint` on a store where `enable_durable_continuity` has not run.
- `enable_durable_continuity` twice changes nothing the second time, refuses a store carrying a
  foreign trigger, and after `disable_durable_continuity` the store opens and attaches under the
  previous release.
- On an enabled store, `create_projections` for a new projection either installs the trigger, so a
  foreign edit to the new table gives `Complete` from a later restored checkpoint, or refuses; and a
  checkpoint taken while a requested projection table carries no provider trigger (a table created
  after the enable call without one), restored after a foreign edit to that table, gives
  `Complete`.
- Continuity kept across the provider's own snapshot writes.

## Consequences for the implementation and other documents

- The implementation story's acceptance asks that "the model digest after a bounded open equals
  the digest after a complete one". With no model copy there is no such digest; the equivalent
  scenario is that every state, record, batch and history a bounded handle answers equals what a
  complete handle answers on the same store.
- The implementation writes the four new commands of *The unchanged-head tamper scenarios* into
  `ess/provider-tracking/domains/operations.yaml`, with their harness in
  `checks/ess-conformance/src/provider.rs`, before any code, and changes R-151's read-bound sentence
  as *What invalidates it* states.
- The implementation reaches `crates/entity-eventlog/src/adapter/scoped.rs` (a bounded handle's
  per-entity reads), which its scope does not list; the row-backed reads and the suffix verifier
  are `adapter/bounded.rs` and the record `adapter/checkpoint.rs`. `projection.rs` is unchanged:
  rows are decoded with its `tagged_body`.
- To change in the same work: R-151's "fully verifies on open" (`docs/requirements.md:224`), the
  policy doc comment (`tracked.rs:13-14`), the bridge and facade doc comments (`sync.rs:1300`,
  `crates/entity-eventlog/src/facade.rs:170-173`), the adapter design
  (`docs/design/eventlog-recorded-adapter-v0.1.md:389`, `:399-400`), the bridge design
  (`docs/design/eventlog-recorded-sync-bridge-v0.1.md:499`), the IO-boundary sentence at
  `AGENTS.md:245`, `CHANGELOG.md`, and the Eventlog re-pin, which `entity-sqlite`,
  `entity-postgres` and `entity-cli` name at the same rev (`AGENTS.md:340-344`).

## Decided in the implementation

This design left three items open. The implementation decided them as follows.

### The wire form

The checkpoint is the snapshot of stream type `er.open-checkpoint`, stream id
`key_for_value("er.eventlog.open-checkpoint-stream-key/1", {"authority": authority})`, in the bound
tenant, written with `version` 0 (the stream has no events), `state_schema_version` 1 and
`recorded_at` the Unix epoch: no library here reads a clock, and a checkpoint's meaning is its
position. Its state is one JSON object read by exact key; a state with an unknown or missing key is
damaged:

| key | value |
|---|---|
| `format` | `er.eventlog.open-checkpoint/1` |
| `kind` | `checkpoint`, or `discarded` for a tombstone |
| `verifier` | the `entity-eventlog` crate version that wrote it |
| `authority` | the `Authority` object |
| `binding` | the binding event's `PhysicalRef` |
| `projections` | the four projection names, in request order |
| `limits` | `max_events`, `max_blobs`, `max_projection_rows`, `max_payload_bytes` |
| `provider` | the provider's durable checkpoint bytes, lower-case hexadecimal |
| `position` | the last verified tenant position |
| `digest` | `framed_key("er.eventlog.open-checkpoint-digest/1", canonical bytes of every other key)` |

A tombstone carries `format`, `kind`, `authority`, `replaces` (`framed_key` in
`er.eventlog.open-checkpoint-replaced/1` of the canonical bytes of the state it overwrote) and
`digest` over the other four. The digest is recomputed from the typed record, never from the
stored text, so a reordered or reformatted state still verifies and an edited one does not.

The reference vector, held by
`crates/entity-eventlog/src/adapter/checkpoint.rs::a_checkpoint_record_has_one_wire_form_and_digest`
and recomputed independently with `sha256sum` when it was pinned: authority
`{"logical_scope":"scope","stream_identity":"identity","tenant":"tenant"}`, binding
`{"event_id":"binding-event","global_seq":1,"stream_id":"singleton","stream_version":1}`, limits
10/20/30/40, provider bytes `007fff10`, position 42 and verifier `reference` give the digest
`sha256:1a1fd81af5a7e6932efad0d0a69d5fdeccd0b4def6c739ea0d28c2140cfe90c0`.

### The calls

| call | what it does |
|---|---|
| `RecordedProviderFacade::enable_durable_open_checkpoints(wait)` (and the bridge's) | runs `SqliteEventStore::enable_durable_continuity` on the worker's store, then verifies the authority again so the handle holds an observation with a durable form; `InvalidInput` for any other provider |
| `RecordedProviderFacade::disable_durable_open_checkpoints(wait)` (and the bridge's) | runs `disable_durable_continuity`; every persisted checkpoint then restores to nothing |
| `RecordedProviderFacade::write_open_checkpoint(wait)`, `EventlogRecordedStore::write_open_checkpoint()` | the explicit checkpoint write, by the rules of *When it is written*; a drain shutdown calls it |
| `EventlogRecordedStoreOwner::discard_open_checkpoint()`, `EventlogRecordedStore::discard_open_checkpoint(backend, authority)` | the discard; on the owner it opens the SQLite store itself and must run before a facade is started from it; owners of other providers return `false` without opening their store |
| `RecordedProviderFacade::open_verification()`, `EventlogRecordedStore::open_verification()` | how the open verified: `Complete`, `Checkpoint`, or `Suffix { events }` |

### A complete-opened handle keeps its model

A handle that opened completely keeps its whole model for its life, as every `ProviderTracked`
handle does today; only a handle opened from a checkpoint answers from rows. Dropping the model
once a durable checkpoint exists would also bound the owner process's memory
(beyond10x/connectors#103); it is not part of this work.
