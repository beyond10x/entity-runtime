---
format: aep.planning-md/1
id: review-result:er-recorded-contract-pass-2
kind: review-result
status: active
title: Independent second review of the corrected recorded execution contract
owner: gpt-5.6-sol
relations:
- reviews: story:async-recorded-contract-executor
revision: 1
---
approve

# Independent ER recorded-contract review, pass 2

Reviewed the revised accepted design, companion encoding contract, ESS coordinate model and
`story:async-recorded-contract-executor` revision 6 against ESS evolution revision 1, Atlas ADR
0050 and the existing kernel/store types at base `eaf43090636abce025649e565e5af271c4513eae`.
This is proposed-contract review only, not code acceptance or runtime evidence.

All five original findings close:

- **ER-CONTRACT-1:** committed single and named-batch retries must load and verify every relevant
  immutable subject-history prefix before current state or registry reads, reject missing or
  inconsistent evidence, and apply the same rule after an uncertain append
  (`docs/design/recorded-execution-v0.1.md:41-48`). Genesis replay and anchored suffix replay have
  concrete recomputation rules (`docs/design/recorded-execution-v0.1.md:140-148`) compatible with
  the existing complete-record replay boundary (`crates/entity-core/src/replay.rs:120-167`).
- **ER-CONTRACT-2:** imported envelopes now inhabit `ImportedRecordEvidence`, retain only observed
  source/order facts, reserve their original global IDs, and cannot acquire a new physical
  position, batch coordinate or receipt (`docs/design/recorded-execution-v0.1.md:117-130`). Exact
  historical retry returns `AppendOutcome::Historical`; unavailable matching facts yield
  `HistoricalRetryUnverifiable` (`docs/design/recorded-execution-v0.1.md:129-134`). The ESS model
  reflects the separate entity and unavailable-receipt fact without making it a `StoredRecord` or
  receipt member (`ess/recorded-execution/domains/recording.yaml:102-133`). No importer is required
  in this first reference unit.
- **ER-CONTRACT-3:** subject replay assurance is explicitly subject-local
  (`docs/design/recorded-execution-v0.1.md:140-147`). Cross-subject checks accept a named logical
  store scope and explicit subject set, enumerate the covered subjects/boundaries, and disclaim
  hidden provider records; whole-store assurance additionally requires a provider-owned consistent
  complete snapshot (`docs/design/recorded-execution-v0.1.md:150-157`). A selected retry prefix
  therefore does not overstate store-wide integrity.
- **ER-CONTRACT-4:** entity and observation revisions are fixed at `1..=i64::MAX`, while subject and
  store positions and member indices are checked `u64` domains permitting zero, with whole-batch
  allocation preflight (`docs/design/recorded-execution-v0.1.md:173-182`). This agrees with the
  existing execute cap (`crates/entity-core/src/runtime.rs:411-419`) and observation validation
  (`crates/entity-store/src/lib.rs:352-374`); the encoding contract requires both maxima and both
  overflow refusals as independent fixtures (`docs/design/recorded-execution-encoding-v0.1.md:98-101`).
- **ER-CONTRACT-5:** the companion contract fixes canonicalization, tags, shapes, field presence,
  array order, unsigned coordinate spelling and literal identity vectors
  (`docs/design/recorded-execution-encoding-v0.1.md:7-37`), then fixes complete request, record and
  batch values and literal observation/batch bytes
  (`docs/design/recorded-execution-encoding-v0.1.md:39-86`). Its negative controls cover escaped
  strings, `100`, `100.0`, `-0`, large exponents, explicit nulls and both numeric maxima, while
  explicitly preserving the actual stored `Value` spelling after any decode-time canonicalization
  (`docs/design/recorded-execution-encoding-v0.1.md:88-101`). Component-to-derived-identity and
  receipt-to-record equality are mandatory (`docs/design/recorded-execution-encoding-v0.1.md:33-37`).

The Eventlog adapter, synchronous bridge and SQLite/PostgreSQL facades/importers remain later units
as required by `docs/design/recorded-execution-v0.1.md:204-207`; their absence is not a finding
against this first contract/executor unit.

```findings
[]
```

No implementation, build, gate, service, Git/AEP mutation, model mutation or repository-file edit
was performed.
