---
format: aep.planning-md/3
id: story:recorded-stores-read-the-same-on-a-tree-that-keeps-text-once
kind: story
status: implemented
title: Recorded stores read the same on a tree that keeps each text once
relations:
- serves: vision:O2
revision: 5
transitions:
- {from: "draft", to: "proposed", at: "2026-10-06T09:23:13Z", actor: "human:timo", revision: 3, decided_on: {"recorded":{"test_result":1}}}
- {from: "proposed", to: "active", at: "2026-10-06T09:23:13Z", actor: "human:timo", revision: 4, decided_on: {"recorded":{"test_result":1}}}
- {from: "active", to: "implemented", at: "2026-10-06T09:23:13Z", actor: "human:timo", revision: 5, decided_on: {"recorded":{"test_result":1}}}
---
# Recorded stores read the same on a tree that keeps each text once

## Outcome

Entity Runtime pins Eventlog 0.6.0, whose tree provider reads `eventlog-tree/1` and
`eventlog-tree/2` and ships `eventlog_tree::migrate`. A recorded store on a first-layout tree reads
the same history after the migration and keeps recording.

## Why

AEP planning stores are recorded stores on a tree. Their import anchors and records carry every
artifact's text, some of it several times and some hex-encoded; on a copy of the ESS planning
store the largest blob is 75,332,948 bytes, past the 8 MiB per-file limit of the Gates scanner.
Eventlog 0.6.0 stores each long text once; AEP can adopt it only through an Entity Runtime pinned
at the same Eventlog revision.

## Acceptance

- `a_first_layout_store_reads_the_same_after_its_texts_are_kept_once_and_keeps_recording`
  (`crates/entity-eventlog/tests/tree_text_blobs.rs`).
- `entity-eventlog --all-features` tests pass on the new pin.
