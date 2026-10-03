---
format: aep.planning-md/3
id: dependency-blocker:verified-provider-change-authority
kind: dependency-blocker
status: open
title: Issue 51 needs an integrity-preserving bounded-read provider contract
relations:
- blocks: story:bounded-batch-and-facade-reads
withholds: test_result
revision: 1
---
## Evidence

The read-only scoping of issue #51 found no verified changed-content token or checkpoint authority in pinned Eventlog rev 0a0484634e8c640be29d6b6541d6cc1c1aaef7d3. See crates/eventlog-core/src/capture.rs:273-336 and src/lib.rs:1005-1017,1149-1179 in that revision; ER's adapter/scoped.rs:291-299 and adapter.rs:4159 require batch co-member closure for existing verification.

## Clear condition

An accepted, implemented and tested provider contract or an equally strong ER verification design demonstrates bounded shared-clock reads while detecting altered blobs and preserving complete batch assurance. The 55/601/1203 timing probe must meet the issue's 2x bound. A read-scope routing change alone does not supply this evidence.

## Next owner and action

Coordinator proposes the provider contract and cross-repository scope to the operator. Do not claim issue #51 fixed or weaken existing integrity assertions. Local facade work may proceed as an explicitly partial contribution after wave approval.
