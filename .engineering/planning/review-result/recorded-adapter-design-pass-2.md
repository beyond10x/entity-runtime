---
format: aep.planning-md/2
id: review-result:recorded-adapter-design-pass-2
kind: review-result
status: active
title: Complete recorded adapter design technical review, final pass
relations:
- reviews: story:eventlog-recorded-adapter-and-bridge
revision: 1
---
needs-revision
story:eventlog-recorded-adapter-and-bridge — The binding and import protocols must replace their requirement to freeze an event ID with capture and validation of the provider-minted ID returned in `AppendGroupResult`, because accepted Eventlog `NewEvent` carries only name, schema version, and data while the provider creates `RecordedEvent.event_id` during append — docs/design/eventlog-recorded-adapter-v0.1.md:170; docs/design/eventlog-recorded-errors-import-v0.1.md:271; EL/crates/eventlog-core/src/lib.rs:184
Read: 1 planning artifact at revision 15, the immutable pass-1 report, the exact 5 current design documents and their matching hashes, the 2 accepted recorded-execution contracts, and the concrete ER and accepted Eventlog types, ports, validators, executor, atomic-group, projection, result, provider and ownership paths needed to recheck the complete contract, using `git rev-parse`, `sha256sum`, `rg`, `wc`, `nl`, and `sed`; all 4 pass-1 findings are resolved by their cited amendments.
Could not establish: native consistent tenant capture, validate-only inline attachment, or asynchronous inline rebuild implementation; all remain explicit provider prerequisites.
Could not establish: File, SQLite, or PostgreSQL adapter qualification, implementation correctness, or executable evidence; this review ran no build, probe, service, SQL, database, or network operation.
Could not establish: SQL-integrity acceptance; the denied SQL review was expressly excluded and was not inspected or retried.
Could not establish: final accepted Eventlog source pins; the reviewed ER proposal references accepted Eventlog source `18322cbe`, while final dependency pins remain required before source dispatch.
```findings
- file: docs/design/eventlog-recorded-adapter-v0.1.md
  line: 170
  category: design
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: The binding and import protocols must replace their requirement to freeze an event ID with capture and validation of the provider-minted ID returned in `AppendGroupResult`, because accepted Eventlog `NewEvent` carries only name, schema version, and data while the provider creates `RecordedEvent.event_id` during append
```
