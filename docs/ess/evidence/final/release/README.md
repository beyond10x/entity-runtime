# ER 0.25.1 final source verification

All 414 selected scenarios passed: 303 authored and 111 generated, with zero failures, runner
errors, unsupported observations, skips, synthesis refusals or cases outside the declared scope.
`suite.json` is the exact admitted inventory suite/29 associated with `report.json` and `run.json`.
`observations.json` holds actual adapter results; `implementation.txt` identifies the selected
source closure and executable. Expected values come from the scenarios, never this transcript.

The complete `task check` and independent Rust 1.85 checker build exited zero. The local gate
explicitly reports PostgreSQL as unavailable when `ENTITY_POSTGRES_URL` is unset; CI exercises
that provider against its service container. No declared ESS scenario is skipped. Retained log
paths are normalized to placeholders; structured evidence bytes remain unchanged.

The matching pinned ESS CLI validates the complete manifest and both preserved coordinate entry
points. Its canonical IR is byte-identical to the checker's `model.json`. The gate regenerates
the model and suite twice and compares exact bytes; `coverage.json` fingerprints every scenario
contract and requires explicit review for changes. Production dependency isolation is verified
by root Cargo metadata. The model digest is distinct from a checksum of serialized IR bytes.

`toolchain.json` records exact producer and AEP reader provenance. `aep-admission.log` retains
successful typed coverage admission of this exact pair. Each domain's failing production mutation
and restored success remain under `../../mutations/`; bounded independent reviews are retained
under `../../review/` and `../../planning-migration/`. The final run observes restored sources.

The later `aep-lifecycle-transition.log` records the actual `validated` to `conforming` move.
AEP retained and re-admitted the original report and suite from its evidence store, checked the
current model and complete passing inventory, and recorded one derived eligibility separately
from the unchanged `ess_conformance_coverage_v1` records. No legacy success record was asserted.
The original admission log retains its original reader identity; lifecycle admission used the
reviewed AEP bridge identified separately in `toolchain.json`.

AEP subsequently added committed status, evidence and review-history safeguards. The final pin
includes those guards. `planning-history-validation.log` retains successful validation of ER's
migrated store by that build; the earlier `planning-validation.log` retains its original identity.

These are source verification records. Publication, mainline integration and release asset
verification are tracked separately by `story:adopt-current-plan-and-release-contracts`.
