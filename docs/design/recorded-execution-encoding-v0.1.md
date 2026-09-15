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

RecordValue is a two-element array ["er.record/1", TaggedRecord]. TaggedRecord is one of:

- {"kind":"decision","commit": COMPLETE_RecordedCommit}
- {"kind":"observation","observation": COMPLETE_RecordedObservation}

COMPLETE values use the existing Rust type's serialized fields in full, including the complete
definition snapshot, commands/results/changes/events and null provenance. Record comparison bytes
are C(RecordValue). Expectation is not added to those existing values.

Recording is the object with exactly record_id, recorded_at, actor, correlation and causation.
RequestValue is ["er.request/1", Request], with Request exactly one of these shapes:

- {"kind":"create","subject":[entity,id],"definition_version":version,"fields":normalized_fields,"recording":Recording}
- {"kind":"execute","subject":[entity,id],"expected_revision":predecessor,"operation":operation,"arguments":normalized_arguments,"recording":Recording}
- {"kind":"observation","observation":COMPLETE_RecordedObservation}

Normalize fields/arguments only through the established kernel validation/default behavior under
the original saved definition. Do not erase a numeric representation distinction just to make a
retry compare equal. Request comparison bytes are C(RequestValue).

## Batch value

ExpectValue is exactly {"kind":"absent"} or {"kind":"revision","revision":n}.
An observation uses its exact observed revision. BatchValue is:

["er.batch/1", KeyValue, [{"expect":ExpectValue,"record":RecordValue}, ...]]

The member array preserves submitted order. RecordValue is nested as a value, not JSON text or
a digest. Batch comparison bytes are C(BatchValue). SingleRecord requires exactly one member
whose record ID matches the key. The explicit empty-batch rule produces no stored comparison.

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
