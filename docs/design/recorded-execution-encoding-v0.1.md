# Recorded execution comparison and coordinate encodings v0.1

Status: accepted, 2026-09-15; part of recorded-execution-v0.1.md. These new encodings do not alter
an existing stored envelope. A future change to their shapes, framing or scalar spelling needs
a new version domain and explicit migration.

## Canonical function

Let C(value) be serde_json compact UTF-8 serialization after recursively ordering every object
by its exact string keys. Arrays retain order. Strings use the workspace's existing serde_json
escaping, without Unicode normalization. Values use the existing arbitrary_precision feature:
retain the Number's exact stored spelling, without conversion through f64 or numeric coercion.
No trailing newline, BOM, separator or whitespace is added. Rust u64 coordinates serialize as
their base-10 unsigned integer spelling. Optional recording fields are explicit nulls.

## Derived coordinate identity strings

SubjectId is the UTF-8 string C([entity_name, instance_id]). KeyValue is exactly either
["single_record", record_id] or ["named", batch_id]. BatchId is the string C(KeyValue).
MemberId is the string C([KeyValue, member_index]); the index is a JSON u64 number, not text.

Normative examples (each code line is the complete resulting identity string):

```json
["E","x"]
["named","b"]
["single_record","r"]
[["named","b"],0]
[["named","b"],18446744073709551615]
["E\"\\","x\n"]
```

Constructors validate nonblank opaque input strings without trimming/replacing their stored
values. Stored component fields must reproduce the derived identity exactly. Receipt subject,
kind, revision, positions, original batch key and member index must match its complete stored
record and membership. Imported evidence's subject/record ID must match its preserved envelope.
The global index treats imported and newly committed IDs as the same ID namespace.

## Complete record and request values

RecordValue is a two-element array [RecordDomain, TaggedRecord]. RecordDomain is the framing the
record is spelled in: "er.record/1" for a kernel/1 decision and for every observation, and
"er.record/2" for a service/1 decision — a decision whose saved definition snapshot declares
semantics: service/1. TaggedRecord is one of:

- {"kind":"decision","commit": COMPLETE_RecordedCommit}
- {"kind":"observation","observation": COMPLETE_RecordedObservation}

COMPLETE values use the existing Rust type's serialized fields in full, including the complete
definition snapshot, commands/results/changes/events and null provenance. Record comparison bytes
are C(RecordValue). Expectation is not added to those existing values.

Recording is the object with exactly record_id, recorded_at, actor, correlation and causation.
RequestValue is [RequestDomain, Request]. RequestDomain moves with RecordDomain and by the same
rule: "er.request/2" where the record is er.record/2, and "er.request/1" otherwise. Request is
exactly one of these shapes:

- {"kind":"create","subject":[entity,id],"definition_version":version,"fields":normalized_fields,"recording":Recording}
- {"kind":"create","subject":[entity,id],"definition_version":version,"arguments":normalized_arguments,"recording":Recording}
- {"kind":"execute","subject":[entity,id],"expected_revision":predecessor,"operation":operation,"arguments":normalized_arguments,"recording":Recording}
- {"kind":"observation","observation":COMPLETE_RecordedObservation}

The first create shape is er.request/1's and is unchanged. The second is er.request/2's: a
service/1 creation's original request is the caller's **arguments**, not the fields its selected
branch produced. Reconstructing it as the fields would hand a retry a request the caller never
sent, which is what these bytes exist to prevent. A service/1 execute is the same three keys the
er.request/1 execute shape has, in the er.request/2 framing.

Normalize fields/arguments only through the established kernel validation/default behavior under
the original saved definition. Do not erase a numeric representation distinction just to make a
retry compare equal. Request comparison bytes are C(RequestValue).

## Why a new domain, and what a reader that does not know it does

A service/1 decision's record carries keys er.record/1 has never carried — outcome, effect,
response, and a create command with an arguments key — and RecordValue is defined as the existing
Rust type's serialized fields in full. That is a shape change, so it takes a new version domain,
which is this document's own governing rule.

The framing tag is the first element of the array, so a reader that does not know a framing refuses
the document **there**, by the name of the framing it found, without parsing the second element at
all. `entity_store::asynchronous::record_framing` reads the tag on its own and
`read_record_in_domain` is that refusal.

A store holding service/1 records must not be opened by a build predating them. Nothing rewrites an
existing stored envelope: a kernel/1 record, a kernel/1 request and every observation keep the exact
bytes they had, and a kernel/1 definition cannot declare an identity at all, so no stored id moves.

## Batch value

ExpectValue is exactly {"kind":"absent"} or {"kind":"revision","revision":n}.
An observation uses its exact observed revision. BatchValue is:

["er.batch/1", KeyValue, [{"expect":ExpectValue,"record":RecordValue}, ...]]

The member array preserves submitted order. RecordValue is nested as a value, not JSON text or
a digest. Batch comparison bytes are C(BatchValue). SingleRecord requires exactly one member
whose record ID matches the key. The explicit empty-batch rule produces no stored comparison.

**The batch tag does not move.** Each member carries its own RecordDomain, so a reader that walks a
batch meets er.record/2 at the member and refuses there, by the name of the framing it does not
know. Moving the batch tag as well would put two tags on one refusal and would rewrite the
comparison bytes of a batch whose members are all /1.

## One complete observation vector

This is the exact observation record comparison byte string. It uses the existing complete
RecordedObservation/Envelope field names and makes key ordering/null preservation inspectable.

```json
["er.record/1",{"kind":"observation","observation":{"entity":"E","envelope":{"actor":null,"causation":null,"correlation":null,"record":{"n":100.0},"record_id":"r","recorded_at":"2026-09-15T00:00:00Z"},"id":"x","revision":1}}]
```

The exact request vector is the same value with the first array element er.request/1.
The exact named batch vector is:

```json
["er.batch/1",["named","b"],[{"expect":{"kind":"revision","revision":1},"record":["er.record/1",{"kind":"observation","observation":{"entity":"E","envelope":{"actor":null,"causation":null,"correlation":null,"record":{"n":100.0},"record_id":"r","recorded_at":"2026-09-15T00:00:00Z"},"id":"x","revision":1}}]}]]
```

## Required byte fixtures and negative controls

Pin the complete examples as literal bytes independently of the encoder. Repeat the observation
vector with n set from parsed JSON tokens 100, 100.0, -0 and 1e9999: retain the exact token and
distinct comparison bytes wherever the existing arbitrary-precision Number does. Assert input
object key order makes no difference while array order does. Include escaped strings and explicit
null versus changed provenance. Never use a Rust floating-point literal to stand for an exact
original JSON token. Existing scalar semantics remain authoritative if decoding has already
canonicalized a spelling; report that fact rather than inventing original input bytes.

Also pin i64::MAX and u64::MAX JSON spellings while independently checking their different typed
domains: a revision above i64::MAX refuses, and physical allocation above u64::MAX refuses without
publishing a prefix. Changing the framing/tag, derived ID/components, one array member, numeric
spelling or recording field must trigger the corresponding conflict or integrity refusal.

Pin the two framings separately, as four claims and not one: a kernel/1 decision still frames as
er.record/1 and er.request/1; a service/1 decision frames as er.record/2 and its request carries
arguments and no fields key; a reader that knows only er.record/1 refuses an er.record/2 document
by naming both framings, including one whose payload is not readable JSON; and a batch of kernel/1
members keeps its er.batch/1 bytes while a service/1 member carries er.record/2 inside the same
er.batch/1 tag. `crates/entity-store/tests/service_framing.rs` is those four.
