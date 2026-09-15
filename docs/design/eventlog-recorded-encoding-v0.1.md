# Eventlog recorded reference encoding v0.1

Status: selected proposal under story:eventlog-recorded-adapter-and-bridge; independent design
review and native provider dependencies remain outstanding. This page selects the byte contract
alongside [adapter behavior](eventlog-recorded-adapter-v0.1.md), not implementation readiness.

## Existing bytes and exact authority

Retain `C`, `er.record/1`, `er.request/1` and `er.batch/1` exactly as defined by
[recorded execution encoding](recorded-execution-encoding-v0.1.md). Existing entity-store encoders
own their canonical bytes; upload those bytes unchanged. New wrappers use the same recursively
ordered compact UTF-8 JSON, array order, explicit nullable fields and arbitrary-precision numbers.
Never coerce through `f64` or normalize Unicode or opaque strings. Preserve distinct number
spellings whenever the existing parser/serializer retains them; mathematical equality is irrelevant.

`Authority` is a closed object with exactly `logical_scope`, `stream_identity` and `tenant`, all
strings. These are the exact caller-supplied logical scope, expected provider generation and
physical TenantId selected in the adapter design. Compare all three against the native capture.

For ASCII domain `d` and bytes `b`, define:

```text
F(d,b) = UTF8(d) || 0x00 || U64_BE(byte_length(b)) || b
K(d,b) = "sha256:" || lowercase_hex(SHA-256(F(d,b)))
```

The length is exactly eight big-endian bytes, checked before conversion, with no trailing byte.
The digest is a bounded physical key, not an injective identity encoding. Retain and compare
unhashed original coordinates in authority; a matching digest never excuses different originals.

| Blob payload | Exact digest domain |
| --- | --- |
| Binding wrapper | `er.eventlog.binding-blob-key/1` |
| Unchanged record bytes | `er.eventlog.record-blob-key/1` |
| Unchanged request bytes | `er.eventlog.request-blob-key/1` |
| Unchanged batch bytes | `er.eventlog.batch-blob-key/1` |
| Recorded-entry wrapper | `er.eventlog.recorded-entry-blob-key/1` |
| Import-anchor wrapper | `er.eventlog.import-anchor-blob-key/1` |

## Closed reference wrappers

The following notation describes JSON values; capitalized names denote typed values, not literal
JSON tokens. `Digest` is the corresponding `K` result above. All fields are required and closed.

```text
["er.eventlog.binding/1", Authority]

["er.eventlog.recorded-entry/1", {
  "authority": Authority,
  "batch_blob": Digest,
  "batch_key": ["single_record", RecordId] | ["named", BatchId],
  "member_index": U64,
  "record_blob": Digest,
  "request_blob": Digest,
  "subject": [Entity, InstanceId]
}]

["er.eventlog.import-anchor/1", {
  "authority": Authority,
  "completeness": "available_evidence_only" | "complete_subject",
  "evidence": [Evidence, ...],
  "instance": CompleteEntityInstance,
  "order": "per_kind_only" | "subject",
  "subject": [Entity, InstanceId]
}]
```

Import `Evidence` is exactly one closed variant:

```text
{"kind":"envelope", "known_order":["per_kind",U64] | ["subject",U64],
 "record_blob":Digest, "source_id":String, "source_locator":String}
{"kind":"decision", "decision":CompleteDecisionRecord}
{"kind":"event", "event":CompleteDomainEvent}
```

Complete typed payloads retain their existing serialized meaning. Envelope evidence reuses record
bytes; it acquires no original request, batch or receipt. Evidence array order is preserved without
inventing chronology beyond the explicit ordering declaration. Existing LegacyAnchor verification
remains required. Bare legacy decisions/events remain inside this concrete wrapper, not a new
decision engine or a generic envelope registry.

Every reference event has schema version `1` and exact data `{"blob":Digest}`. Binding uses event
name `er.binding` in fixed stream type `er.binding`, ID `singleton`; precisely one binding event
must match its tenant generation. Recorded entries use `er.recorded_entry`, anchors `er.import_anchor`.
Both use stream type `er.subject` and stream ID:

```text
K("er.eventlog.subject-stream-key/1", C({"authority":Authority,"subject":[Entity,InstanceId]}))
```

All original scope/subject strings and payloads remain in erasable blobs, not physical event fields.
Foreign event names/versions, extra body fields, redaction, wrong stream or binding mismatches refuse.

## Resolution, batches and receipts

Recompute every referenced digest, reject duplicate keys before parsing can lose them, and require
exact tag, array length and closed fields. Decode concrete typed values with arbitrary precision,
re-encode through the accepted encoders and compare exact stored bytes. Changed whitespace/key
order and unrecognized nested data cannot silently become accepted canonical content. Source has
encoders but lacks canonical decoders; the adapter must implement these closed decoders explicitly.

For each recorded entry, compare decoded subject, authority, batch key, request reconstructed from
the complete record, and exact batch member record. Convert `member_index` through checked
`usize::try_from`, then bounds-check; never truncate or clamp. Writing checks the inverse conversion.
SingleRecord has one member at index zero and its key equals that member's record ID.

A nonempty append stores one complete batch blob and each member's record/request/wrapper blobs.
Submit one reference event per member in an ordered atomic AppendGroup, including repeated subject
streams. Empty batches write no blob, event, identity or receipt. Every zero-domain-event decision
and every observation still publishes one physical reference event.

Complete native capture must resolve exactly one reference for every batch member, equal batch
digest/key across the batch, indices `0..member_count-1` exactly once, and increasing actual global
positions in member order. Missing/extra/crossed members refuse. Count alone is not completeness.
Orphan uploaded blobs are permitted but do not become committed records. Unknown events are never
filtered out to produce a complete-scope claim.

Reconstruct receipts from actual event stream version/global sequence, preserving gaps and original
SingleRecord/Named key and member index. A later individual retry of a named member returns its
original member receipt in CommitReceipt::Single. Never derive physical positions from entity
revision. Import anchors consume physical positions without becoming fabricated StoredRecords or
historical commit receipts; retain the existing Historical assurance boundary.

## Literal vectors and remaining contract

The record/request/batch positive literals in the accepted encoding companion, with the observation
subject `["E","x"]` and named batch `b`, produce these framed digest vectors:

| Domain suffix after `er.eventlog.` | Payload bytes | Framed bytes | SHA-256 |
| --- | --- | --- | --- |
| `record-blob-key/1` | 225 | 263 | `042c6cac5e47030b8ba26e5401ba28bbaadd29a4c34bd6969d003654c5cb5380` |
| `request-blob-key/1` | 226 | 265 | `f147bb14b0c06bac672d2a3ecb12841d4edd2c1032664ce9fba10d160e9d5ebc` |
| `batch-blob-key/1` | 309 | 346 | `04f3a904e0b2cf4d80f78cc7dc7153953ea08e4fcbdd3f0cc1bab9939a4242a1` |

The complete additional binding/entry/subject/anchor literals and digests are retained at
local-evidence:ess-evolution/waves/0007-er-eventlog-adapter/reference-encoding-vector-clarification.md
and reference-encoding-scope-result.md. The earlier escaped-subject entry combined with the simple
subject's record is a substitution negative, not a valid bundle. Independent Rust framing plus
system SHA-256 reproduced all seven corrected literal byte lengths and digests; this is hash
evidence only, not an executed canonical decoder, resolver or adapter test.

Implementation must commit standalone literal fixtures for all wrappers and unchanged ER domains,
then exercise unknown/duplicate keys, number spelling, null provenance, Unicode/control strings,
wrong digest domain, wrong subject stream, changed scope/tenant/generation, maximum member index,
named/single namespace differences, crossed members and reordered batch arrays against real decode.

Operational CommandMeta, command/index key schemas, admission/rebuild/import protocol, exact pins,
error mapping and bridge runtime remain separate unresolved sections of the adapter contract.
No fixed epoch, implicit clock or narrowed ER provenance is selected to fill operational metadata.
The complete design requires independent review and qualified native capture before implementation.
