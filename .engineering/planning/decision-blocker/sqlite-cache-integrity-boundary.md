---
format: aep.planning-md/3
id: decision-blocker:sqlite-cache-integrity-boundary
kind: decision-blocker
status: cleared
title: Choose the integrity boundary for SQLite warm cache verification
relations:
- blocks: story:bounded-batch-and-facade-reads
revision: 3
transitions:
- {from: "open", to: "cleared", at: "2026-10-03T15:26:44Z", actor: "human:timo", revision: 3}
---
## Question

The operator was asked whether a full verification on open followed by SQLite change tracking may establish unchanged state, detecting writes from any SQLite connection including SQL tampering, or whether every read must detect arbitrary raw database-file edits bypassing SQLite. No answer has arrived yet.

## Consequence

The narrowed SQLite boundary admits investigation of a provider-owned checkpoint and atomic append delta. The arbitrary-file boundary does not admit returning a cached answer merely because SQLite reports no change. Keep current complete verification until the operator answers; do not infer agreement from silence.

## Clear condition

Record the operator's chosen integrity boundary and reflect it in the provider contract and executable verification cases. This clears a decision only, not dependency-blocker:verified-provider-change-authority or any performance evidence requirement.

## Operator decision

The operator explicitly answered "Accept SQLite change tracking" on 2026-10-03. Full verification on open remains required. Warm reads may rely on SQLite-mediated change tracking and need not detect raw database-file modifications that bypass SQLite. Writes from other connections, including SQL tampering, must invalidate the cache. This resolves the integrity-boundary choice; provider change authority and implementation/performance evidence remain outstanding.
