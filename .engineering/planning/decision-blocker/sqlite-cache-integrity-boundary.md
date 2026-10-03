---
format: aep.planning-md/3
id: decision-blocker:sqlite-cache-integrity-boundary
kind: decision-blocker
status: open
title: Choose the integrity boundary for SQLite warm cache verification
relations:
- blocks: story:bounded-batch-and-facade-reads
revision: 1
---
## Question

The operator was asked whether a full verification on open followed by SQLite change tracking may establish unchanged state, detecting writes from any SQLite connection including SQL tampering, or whether every read must detect arbitrary raw database-file edits bypassing SQLite. No answer has arrived yet.

## Consequence

The narrowed SQLite boundary admits investigation of a provider-owned checkpoint and atomic append delta. The arbitrary-file boundary does not admit returning a cached answer merely because SQLite reports no change. Keep current complete verification until the operator answers; do not infer agreement from silence.

## Clear condition

Record the operator's chosen integrity boundary and reflect it in the provider contract and executable verification cases. This clears a decision only, not dependency-blocker:verified-provider-change-authority or any performance evidence requirement.
