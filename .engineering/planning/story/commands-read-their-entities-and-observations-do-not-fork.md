---
format: aep.planning-md/1
id: story:commands-read-their-entities-and-observations-do-not-fork
kind: story
status: implemented
title: A command reads only its entities' streams, and an observation does not make a head
relations:
- serves: vision:O2
revision: 4
---
## Outcome

Two Entity Runtime units of the planning-on-entity-runtime design (aep
`docs/design/planning-on-entity-runtime-v0.1.md` § 5, § 13):

- **R2 (E-R2), per-entity reads.** A command reads the binding stream, its subjects' streams,
  the blobs they bind and the index rows for its record ids, batch key and subjects, through
  `read_many`, instead of a complete tenant capture per read (5 per command before, 0 after).
  Three unverified attempts fall back to one complete capture; a command that would cross the
  handle's `CaptureLimits` is refused before upload; the identity check reads the binding row
  first. R-151 and the recorded-adapter design state what is still a complete read and the
  narrower tamper detection.
- **An observation does not make a head.** Only decisions make heads (R-156 to R-158); an
  observation hangs off the decision it observed, so evidence on one branch and a move on another
  do not fork. Merges and ordinary writes to a subject with several tips append with
  `Expected::Merge` over every tip; a provider fork refusal is the typed `Forked`.

## Review

Each unit had an adversary pass: R2 (4 findings) and the observation rule (3 findings) are fixed
and pinned by the adversaries' own tests.
