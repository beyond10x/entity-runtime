---
format: aep.planning-md/1
id: dependency-blocker:native-eventlog-capture-and-provider-qualification
kind: dependency-blocker
status: open
title: Native complete capture and remaining Eventlog provider qualification are not yet accepted
refs:
- provider: eventlog
  reference: story:consistent-tenant-capture
relations:
- blocks: story:eventlog-recorded-adapter-and-bridge
withholds: test_result
revision: 3
---
The Eventlog adapter requires an implemented and qualified native ConsistentTenantCapture over
File, SQLite and PostgreSQL, plus accepted SQL blob/redaction integrity. Capture has an accepted
design but no implementation. SQL integrity has production evidence but its independent review
is unresolved after a platform rejection; this blocker does not substitute local tests for that
missing review. Eventlog owns those facts. Clear only after their actual accepted source and
provider gates are available and the adapter can pin that exact dependency. Design/source analysis
may continue; adapter source implementation and provider acceptance remain blocked.

## Inline rebuild dependency

The selected docs/design/eventlog-recorded-indexes-v0.1.md additionally requires Eventlog's explicit
asynchronous inline-rebuild capability on all three providers. Current catch-up rebuild rejects
inline registration; SQL shadow replay resolves blobs through its shadow prefix, and PostgreSQL
uses a deliberately incomplete watermarked prefix. The adapter cannot use that as complete derived
index recovery. Eventlog must retain exact registered projector/spec identity, replay complete
committed authority using active blobs, swap all derived tenant row sets atomically and qualify
concurrent publication, rollback and cancellation/uncertain-commit behavior. This is a concrete
dependency in addition to capture and SQL review, not authority to bypass either prerequisite.
Root inspected the cited accepted source paths; no new provider test or implementation exists.

## Validate-only runtime attachment

The bridge constructs its provider on its owned thread, then must attach the already admitted
projector without changing storage. Current register_inline can write File registration/history,
SQLite DDL/registry, or PostgreSQL schema migration. Eventlog needs an explicit asynchronous
attach_inline_existing operation (or an equivalent read-only constructor contract) on all three
providers. Under the registration lock it validates exact existing projector/spec/physical shape,
refuses missing/drifted/dirty/duplicate declarations and serving freeze, and only installs in-memory
projector code. It performs no DDL, persistent registration, initialization, recovery or rebuild.
Root verified the accepted source paths cited by the bridge companion. This is an additional
provider dependency; actual existing-only construction/attachment and provider qualification must
be proved before adapter startup acceptance. No direct provider-table workaround is authorized.
