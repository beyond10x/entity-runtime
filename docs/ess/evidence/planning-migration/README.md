# Verified Git-native planning migration

The documented migration build at AEP commit `9c0f1da44429ff935fa0b2d743457945d51e1c51`
converted ER's `aep.project/3` Eventlog store into `aep.project/5`. `verified.log` records its
artifact-by-artifact comparison of status, revision, title, relations, body, transitions and
evidence. Current AEP then validated the Git-native result. No artifact status was edited by hand.
The former Eventlog authority remains in preceding Git commits; the migration removed its files
from the current layout after writing and verifying the new authority.

The old CLI refused the newly created release story's draft-to-proposed move
with `semantic_mismatch` at the authority boundary. Its status remained draft during migration.
The current CLI reported the concrete missing requirement: agreed work must serve an objective.
Adding the repository's declared `serves vision:O2` relation through AEP allowed both the proposed
and active moves. No lifecycle rule was weakened and no status was forced.
