---
format: aep.planning-md/3
id: dependency-blocker:verified-provider-change-authority
kind: dependency-blocker
status: cleared
title: Issue 51 needs an integrity-preserving bounded-read provider contract
relations:
- blocks: story:bounded-batch-and-facade-reads
withholds: test_result
revision: 3
transitions:
- {from: "open", to: "cleared", at: "2026-10-03T16:38:23Z", actor: "human:timo", revision: 3}
---
## Evidence

The read-only scoping of issue #51 found no verified changed-content token or checkpoint authority in pinned Eventlog rev 0a0484634e8c640be29d6b6541d6cc1c1aaef7d3. See crates/eventlog-core/src/capture.rs:273-336 and src/lib.rs:1005-1017,1149-1179 in that revision; ER's adapter/scoped.rs:291-299 and adapter.rs:4159 require batch co-member closure for existing verification.

## Clear condition

An accepted, implemented and tested provider contract or an equally strong ER verification design demonstrates bounded shared-clock reads while detecting altered blobs and preserving complete batch assurance. The 55/601/1203 timing probe must meet the issue's 2x bound. A read-scope routing change alone does not supply this evidence.

## Next owner and action

Coordinator proposes the provider contract and cross-repository scope to the operator. Do not claim issue #51 fixed or weaken existing integrity assertions. Local facade work may proceed as an explicitly partial contribution after wave approval.

## Resolution

The operator accepted the SQLite-visible integrity boundary. Eventlog6983cc25eb92e07844b3a6fa3e0decbdb300f43c implements the optional proof contract and is bot-published with signed common checks, independent regression review and a final545-case PostgreSQL/TLS proof. Runtime1516e748 plus integration receipt/test/docs work preserves full-open verification, complete-new-batch assurance, and external SQL tamper refusal. The exact published-pin integration gate exits0 with421+17 ESS scenarios, all supported and none skipped. Final release probe medians11.532148/13.049370/13.568977ms at55/601/1203 events give1.132/1.177x growth and pass the unchanged2x assertion. Evidence is retained in docs/ess/evidence/provider-tracking and the immutable review records. Clear condition met without changing default constructor behavior or claiming consumer adoption.
