---
format: aep.planning-md/1
id: story:file-store-atomic-groups-from-eventlog
kind: story
status: implemented
title: All-or-nothing batches across subject documents in File Store
summary: Close the File Store single-document atomicity limit, reusing eventlog's crates/eventlog-file atomic groups; gated on the Atlas ADR that decides whether entity-store providers sit on eventlog.
refs:
- provider: atlas
  reference: architecture/adr/0050-ess-evolution-recorded-execution.md
- provider: eventlog
  reference: crates/eventlog-file
relations:
- informed_by: story:replay-from-events
- serves: vision:O2
revision: 10
---
## Problem

`README.md` states the limit plainly: `MemoryStore`, `SqliteStore` and `PostgresStore` support
all-or-nothing ordered batches, and "File Store atomicity is limited to one subject document". A
shell that keeps more than one entity consistent on the filesystem therefore has no all-or-nothing
write to reach for, and either invents one or accepts a torn state it cannot detect.

The artifact that closes this already exists in another repository: `eventlog` 0.2.0 ships
`crates/eventlog-file`, a local JSONL provider with versioned transaction frames, process-safe
writer locking, sequence and previous-digest verification and durable referenced blobs before
acknowledgement, and its `CHANGELOG.md` 0.2.0 § Added (lines 26-30) records the process-safe atomic
groups. Its recovery, privacy and operating boundaries are documented in
`eventlog/docs/design/file-provider.md`. `story:replay-from-events` in this store already names
`eventlog` as "the natural store" for event history.

## Gate — do not start this without the decision

This story is **blocked on a decision that is not ours to take**: whether Entity Runtime's store
providers may sit on `eventlog` at all. `atlas/ROADMAP.md` records that arrow as intended with the
decision open and an Atlas ADR required first, and no manifest in this repository names an
`eventlog` crate today (`grep -rn eventlog --include=Cargo.toml .` returns nothing). `eventlog`'s
own `architecture-decision-record:ess-evolution-05-file-and-atomic-groups` asserts the arrow from
its side ("Provider facade migrations happen in ER") while sitting at status `proposed`, which is
one repository deciding another's dependency direction. The Atlas ADR decides it; until it exists,
this story stays `draft` and nothing here is implemented.

The dependency arrow rule in `AGENTS.md` § Boundaries is what makes this a coordinated migration
rather than a local edit.

## Outcome

File Store offers the same all-or-nothing ordered batch the other three providers offer, across
more than one subject document, with the same refusal semantics — and `README.md` stops naming a
limit that no longer exists.

## Acceptance

A batch spanning two subject documents either lands entirely or leaves both documents byte-identical
to their prior contents, proved against a second process interleaving its own writes and against a
crash between the two documents' writes; the existing `entity-store` conformance suite runs against
File Store with the multi-document batch cases the other providers already pass, and the `README.md`
sentence is rewritten in the same change.

## Options this has to choose between

1. Depend on `eventlog-file` and adopt its transaction frames — the reuse this story is named for,
   and the option the Atlas ADR must admit.
2. Implement the same guarantee inside `entity-store` without the dependency — no cross-repository
   decision, and a second implementation of a problem `eventlog` has already solved and proved.

Do not pick one here. The ADR picks one.

## Origin

Org-state review 2026-09-15, lane `02-er-eventlog`, finding F14 (missed-opportunity, note), at
`eaf4309` (tag `0.18.1`).

## Reconciliation after lineage composition

The earlier gate and options sections record the state at
`ba75b9728a6a41c61f2ea5b54d519adf0419c8c1` and remain as origin evidence. Atlas ADR 0050 is now
accepted under approved plan `ess-evolution-20260915` revision 1, SHA-256
`7579145c3de5a1c6f8088fd7fb804d29dac8903ec505048f3ce595c45023b787`. It selects the Entity Runtime
Eventlog adapter and keeps provider-facade migrations in Entity Runtime, so the dependency-direction
decision is no longer open and the Eventlog-backed option is the accepted direction.

This artifact is the existing Entity Runtime owner for the later FileStore facade and atomic-group
adoption; do not create a duplicate owner. It remains `draft` because the qualified asynchronous ER
Eventlog adapter, explicit compatibility bridge and remaining Eventlog provider proof are not yet
complete. ADR acceptance records direction, not a shipped dependency or readiness evidence.

## Complete candidate implementation assignment

## Complete candidate implementation assignment

The separately scoped full facade/import and existing FileStore author assignment is now dispatched to operation_fulfillment_correction after that worker CLOSED the original SDK M6 author contract. Managed tree ~/.local/state/worktree/trees/b10x/entity-runtime/ess-evolution-provider-facades-20260916 begins at exact7fd93ef43d4a91c460c7305f9e3be90d7b0a4c11; worker lease codex-provider-facades-20260916. Root alone writes canonical AEP and integrates. Full fixed contract: ~/beyond10x/.ess-evolution/waves/0007-er-eventlog-adapter/remaining-provider-facades-contract.md.

Source/design work can proceed independently of provider qualification. Initial design docs/design/eventlog-provider-facades-and-legacy-imports.md resolves dependency direction and retains explicit legacy acquisition; final compatibility must prove usable File/SQLite/PostgreSQL recorded facades and actual CLI/shell routing, complete history/identity/query/receipt/uncertainty behavior, caller-selected PostgreSQL authority, exact imports and File competing-process/crash atomicity. Full original actualPG/minima/strict/docs/site gates and one complete report end the assignment; no narrower facade-only handoff. Original adapter reviews stay closed and denied administration work remains excluded.

Implementation qualification and lifecycle advancement remain subject to the existing adapter/provider acceptance dependency; no completion claim from this assignment. One worker owns both stories to avoid source overlap. Source-only initially while native correction owns heavy2 and M3 bounded1; root allocates compilation after capacity check.

## Complete author acceptance and candidate freeze

Complete author contract closed. The exact 27-path candidate is 5df62103b129ef482f9fdafc3cb3f7c194c7b942 over 7fd93ef43d4a91c460c7305f9e3be90d7b0a4c11, with Eventlog caller-authority candidate 088c27b5df9745c68d8a2240dbb2038998b03efe.

The full task check with actual PostgreSQL and all runtime features passed on unchanged source; independent pure Rust 1.85 checks and feature-enabled Rust 1.91 consumer acceptance passed. The owned disposable database was removed and absence verified. Source manifest SHA256 35ea43b37650bbd6d2d5245504acfdfad9e9401253ba1e01c5331fdc45bd09c6; evidence manifest SHA256 9bc785cd09214cf54892ff9adf71c1a1250397b2b0b12fa94cc19ba10233f81b. Root rehashed every manifest entry, froze exact source, and verified signed common evidence.

The complete report and freeze receipt are retained in the ESS evolution root handoff under waves/0007-er-eventlog-adapter/remaining-provider-facades/. Independent whole source examination and qualified integration remain open. Administration review and four provider-native stages are separate existing blockers; consumer acceptance does not supersede them. No real cutover or publication occurred. The author assignment is closed; root owns original source-review dispatch and integration.

## Complete local integration acceptance

# M2 runtime adapter, facades and explicit imports — accepted locally

2026-09-18T23:11Z. Implemented / verified / integrated = yes / yes / yes. Root committed the final qualified dependency selection on the existing `integrate/ess-evolution-er-20260915` branch as `8b1757365f628338cf697f53deb9ce76acd25a99`, parent `13b8a1faddcbd368ca636606efa9e4927a9221c8`. Provider is qualified Eventlog `f802eb8b01b44ba04a93394b20f0c07391f7757a`. Integration checkout is clean; all five final manifest/lock hashes in `source.sha256` match after commit. No path override, other dependency update or provisional CPU patch was adopted.

## Acceptance evidence

| Approved existing obligation | Acceptance retained |
| --- | --- |
| Complete asynchronous recorded adapter and explicit synchronous bridge; records, receipts, retry, conflict, fault recovery, binding and service composition | `task-check.log` / `task-check.exit`: full `task check` exited zero, including all-feature entity-eventlog tests and strict Clippy on Rust1.91; File/SQLite and real PostgreSQL provider, bridge, fault and service composition tests executed. Original adapter source reviews and accepted corrections remain closed. |
| Compatible SQLite/PostgreSQL facades and explicit legacy acquisition/import, caller-owned transport and complete receipts | `facades.log` / `facades.exit`: Rust1.91 all-feature entity-postgres/entity-sqlite acceptance exited zero. PostgreSQL migration/app URLs and CA were exported from the owned live TLS fixture, including the separately required migration URL; no unavailable-fixture branch substitutes for acceptance. |
| File facade, multi-subject atomicity, imports and existing-only ordinary open | Full entity-eventlog all-feature gate above includes facade and native-provider acceptance. Original facade source examinations and final-open correction are accepted; their independent regressions are retained unchanged. Provider's newly closed native group crash matrices supply the original private-stage gap evidence. |
| Independent pure library minimum | `pure-minimum.log` / `.exit`: Rust1.85 locked/offline workspace build excluding entity-eventlog exited zero. |
| Common source checks and integrated source identity | `common-check.exit` and `common-verify.exit` zero; signed `common-receipt.json`. Commit changed only four manifests and Cargo.lock; post-commit hashes equal the verified source. |
| Changed adopter documentation | No website source changed in this final pin selection. Earlier accepted facade/open correction includes its successful site gate; unchanged evidence retained in `../final-open-correction/coordinator-acceptance.json`. No redundant site prerequisite added. |

All original M2 source reviews are CLOSED; no third examination or reset. Provider acceptance is linked in `../native-qualification-20260918/integration.md`. Final local integration closes the existing adapter, facade/import and FileStore atomic-group product outcome, not merely an author assignment. No release/publication, live-store migration, downstream SDK/Connectors qualification or final ESS accounting acceptance is claimed.

Next ordered outcome is existing M3 public WriterControl and migration command integration. The confirmed local operator-controlled writer model supplies design facts; actual stop/drain/no-restart custody must still be established before any real store activation.
