---
format: aep.planning-md/2
id: review-result:recorded-adapter-design-pass-1
kind: review-result
status: active
title: Complete recorded adapter design technical review, pass one
relations:
- reviews: story:eventlog-recorded-adapter-and-bridge
revision: 1
---
needs-revision
story:eventlog-recorded-adapter-and-bridge — Add a concrete provisioning method, outcome, and typed uncertain-failure identity, because the binding append may commit before its response is lost but the selected contract defines neither a `BatchKey` nor an import-style subject key through which that outcome can be recovered — docs/design/eventlog-recorded-adapter-v0.1.md:55
story:eventlog-recorded-adapter-and-bridge — Define `members[].record_key` as the exact `er_records_v1` row-key derivation or remove it, and require its recomputation on every read, because this persisted field has no selected domain or equality rule — docs/design/eventlog-recorded-indexes-v0.1.md:144
story:eventlog-recorded-adapter-and-bridge — Constrain every persisted row `revision` to `1..=i64::MAX` and require readers and rebuild to refuse values outside that domain, because the selected row schema admits the full `U64` range while accepted ER revision meaning does not — docs/design/eventlog-recorded-indexes-v0.1.md:103
story:eventlog-recorded-adapter-and-bridge — Specify how `shutdown` called after `TimedOut` handles the already-`Closing` lifecycle, including whether a later mode can change and how the retained join is completed, because the design promises another attempt but defines only the initial `Running -> Closing` transition — docs/design/eventlog-recorded-sync-bridge-v0.1.md:352
Read: 1 planning artifact at revision 14, the exact 5 selected design documents and their matching hashes, the 2 accepted recorded-execution contracts, and the concrete ER types, ports, validators, reference store, executor, plus accepted Eventlog core/group/projection and provider paths needed to check binding, bytes, indexes, recovery, errors, import and bridge ownership, using `git rev-parse`, `git status`, `sha256sum`, `rg`, `wc`, `nl`, and `sed`.
Could not establish: native consistent tenant capture, validate-only inline attachment, or asynchronous inline rebuild implementation; all remain explicit provider prerequisites.
Could not establish: File, SQLite, or PostgreSQL adapter qualification, implementation correctness, or executable evidence; this review ran no build, probe, service, SQL, database, or network operation.
Could not establish: SQL-integrity acceptance; the denied SQL review was expressly excluded and was not inspected or retried.
Could not establish: final accepted Eventlog source pins; the reviewed ER proposal references accepted Eventlog source `18322cbe`, while final dependency pins remain required before source dispatch.
```findings
- file: docs/design/eventlog-recorded-adapter-v0.1.md
  line: 55
  category: design
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: Add a concrete provisioning method, outcome, and typed uncertain-failure identity, because the binding append may commit before its response is lost but the selected contract defines neither a `BatchKey` nor an import-style subject key through which that outcome can be recovered
- file: docs/design/eventlog-recorded-indexes-v0.1.md
  line: 144
  category: design
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: Define `members[].record_key` as the exact `er_records_v1` row-key derivation or remove it, and require its recomputation on every read, because this persisted field has no selected domain or equality rule
- file: docs/design/eventlog-recorded-indexes-v0.1.md
  line: 103
  category: design
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: Constrain every persisted row `revision` to `1..=i64::MAX` and require readers and rebuild to refuse values outside that domain, because the selected row schema admits the full `U64` range while accepted ER revision meaning does not
- file: docs/design/eventlog-recorded-sync-bridge-v0.1.md
  line: 352
  category: design
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: Specify how `shutdown` called after `TimedOut` handles the already-`Closing` lifecycle, including whether a later mode can change and how the retained join is completed, because the design promises another attempt but defines only the initial `Running -> Closing` transition
```
