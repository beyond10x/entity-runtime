---
format: aep.planning-md/3
id: review-result:er-batch-cost-adversary-1
kind: review-result
status: active
title: 'Batch cost unit: adversary pass 1'
relations:
- reviews: story:batch-cost-grows-linearly-with-members
revision: 1
---
# Batch cost unit: adversary pass 1

Adversary report on the uncommitted `wave/batch-cost` tree (base `e7f237ac`), condensed by the
coordinator. Home-directory prefixes are written as `~`.

```
unit: story:batch-cost-grows-linearly-with-members
verdict: NEEDS-CHANGE
cases: executed 234→235, red 1
origin: introduced 0 / pre-existing 0 / undecided 3
```

## Findings

| # | where | finding | verdict | disposition |
|---|---|---|---|---|
| 1 | `crates/entity-eventlog/src/projection.rs` (`verified_batch`, `get_blob`) | On the File provider each member's `get_blob` re-reads and SHA-256-hashes the whole batch blob inside eventlog-file (0.8.1 `eventlog-file/src/lib.rs:696-712`), so 256→512 members cost 2.72–3.17x CPU in 5 runs. `tests/batch_cost_file.rs::a_file_provider_batch_costs_time_linear_in_its_members` is red. | NEEDS-CHANGE | Outside this story: its acceptance names a SQLite store opened `ProviderTracked`, and its out-of-scope line excludes Eventlog. The test is kept, ignored with its reason, as the target for an Eventlog read path that does not re-verify the whole blob. |
| 2 | same | The Postgres provider's projection `get_blob` hashes the full blob on every read (`eventlog-postgres/src/lib.rs:1396`) and moves it over the wire for each member. That is O(M²) bytes. Code-read only, not run. | CONFIRMED | Same cause as 1; same Eventlog follow-up. |
| 3 | `projection.rs` `LAST_BATCH` | The per-thread cache has no size limit and is never cleared, unlike eventlog-sqlite's 32 MiB `VerifiedBlobs`. | CONFIRMED | Fixed in this unit: the cache is bounded at 32 MiB and cleared on a verification error. |

## Attacked, not broken

- A cache hit needs an equal digest and equal bytes.
- A missing blob is still refused, because `get_blob` runs before the cache lookup.
- Error kinds and messages are the same as `required_blob` + `decode_batch`.
- No `RefCell` borrow is held across an `.await`.
- Moving between threads and interleaving stores, tenants or authorities can only cause a miss.
- A rolled-back append leaves only an entry that depends on nothing but its bytes.
- Record bytes and digests are unchanged: the tests that pin them pass.
- The CPU-time test passed 12 of 12 runs at load 13–15. The worst ratios were 2.32 against the 2.5 bound and 1.36 against the 1.75 bound.
- On non-Linux targets the test compiles empty. `entity-eventlog` tests run only in the ubuntu `Eventlog runtime (Rust 1.91)` job.
