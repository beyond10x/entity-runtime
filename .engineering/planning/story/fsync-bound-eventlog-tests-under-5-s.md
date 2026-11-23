---
format: aep.planning-md/3
id: story:fsync-bound-eventlog-tests-under-5-s
kind: story
status: draft
title: fsync-bound entity-eventlog tests stay under 5 s under disk contention
summary: tree_branches up to 10.3 s, service_3_composition 8.0 s, guarded_write cleanup 5.96 s under disk load
revision: 1
---
## Outcome

The fsync-bound entity-eventlog tests stay under 5 s each on the shared host under disk
contention, by syncing less in the test setup (not by weakening what they assert) or by running
those binaries with fewer nextest threads.

## Evidence (r2-budget hand-back, 2026-11-23)

- Under disk contention (load about 42): `tree_branches` tests up to 10.3 s,
  `service_3_composition` 8.0 s, `providers` tree 6.8 s,
  `guarded_write_blob_binding::a_commit_whose_cleanup_failed_is_answered_as_committed_by_each_write`
  5.96 s.
- One `tree_branches` test alone: 1.8-2.9 s with 72 fsync calls averaging 41 ms (strace).
- r2-budget D2, option A on default.

## Acceptance

- Each named test under 5 s in two runs of `scripts/test-step.sh` at a stated load, its
  assertions unchanged; the fsync count per test before and after.
