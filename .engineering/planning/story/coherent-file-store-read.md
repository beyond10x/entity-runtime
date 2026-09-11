---
format: aep.planning-md/1
id: story:coherent-file-store-read
kind: story
status: draft
title: Read validated File Store state and history without repeated decoding
relations:
- serves: vision:O2
- informed_by: story:provider-integrity-hardening
revision: 1
---
## Consumer need

Org-brain captures current subjects plus retained decision causes for bounded extraction. Its advisory cache reloads changed subjects through StateProvider::load and HistoryProvider::records; cold enumeration additionally calls StateProvider::ids. In the consumed File Store implementation, each path independently calls read_subject, so one logical subject read repeatedly decodes its complete retained history. Source evidence: crates/entity-store/src/file.rs, StateProvider::{load,ids}, HistoryProvider::records and FileStore::read_subject at tag0.17.7. The public consumer path is org-brain crates/brain-rules/src/extract/capture.rs at d1dde0e0e7c9af01bdff8884774c3a9f9df889c6. The consumer keeps canonical kernel and ledger evidence authoritative; it must not implement a parallel File Store decoder to improve preparation.

## Requested outcome

Provide an owner-supported way to obtain a subject's validated current state and decision history coherently without repeated whole-subject decoding for one logical read. Evaluate whether an immutable read handle or equivalent snapshot boundary can let a caller capture a consistent version under its coordination lock and finish expensive decoding outside that lock. This is a consumer contract request, not a prescribed storage rewrite or authority to weaken history validation. Entity Runtime owns API design, implementation and release; the requesting brain session owns later compatibility qualification and consumption only.

## Acceptance

- State, revision and decision history returned by the supported read describe the same committed subject version, including when another handle replaces that subject concurrently.
- One logical combined read does not independently decode that complete subject once for state and again for history. Show a synthetic retained-history benchmark and observable read/decode work, including cold enumeration and changed-subject refresh beside unchanged subjects.
- Any cache or snapshot cannot authorize writes against stale revisions, hide changed canonical bytes or lose current record-identity checks. Missing, malformed, corrupt or unsupported subjects remain refusals through the canonical reader; immutable read evidence never becomes write authority.
- Preserve File Store locking, temporary-file evidence, recorded envelopes, observations and legacy verification. If the owner chooses a format boundary, model and qualify the explicit migration before consumer adoption.
- Validate at least1000subjects with long histories, a small changing subset, separate concurrent reader/writer handles and process restart. Compare complete state/history output with the existing supported readers; retain altered-history rejection and byte-exact legacy evidence checks.
- Document what consistency the API provides and what still belongs to caller coordination. Report measured costs instead of claiming that the consumer's full preparation or delivery target is achieved by an isolated reader benchmark.

## Scope and ownership

Candidate owner surfaces: crates/entity-store/src/file.rs and its public provider interfaces/conformance tests; owner design and requirements register if a contract changes. Consumer adoption changes belong in org-brain and its private instance, after the owner publishes a supported dependency revision. No runtime implementation, release, dependency pin or live service is changed by this request.
