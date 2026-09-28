# Integrated verification evidence

The full local `task check` exited zero. It includes specification regeneration, reviewed
scenario-contract fingerprints and every admitted scenario, alongside the repository's existing
Rust correctness, purity, requirements, pin and Eventlog gates. The independent Rust 1.85
checker build also exited zero. CI additionally supplies PostgreSQL; the retained local gate
explicitly reports that optional provider check as skipped because `ENTITY_POSTGRES_URL` was
unset. This does not skip any declared ESS scenario.

`specification-validation.json` records complete manifest validation. The matching ESS CLI and
standalone checker compiled byte-identical canonical IR. Both preserved coordinate entry points
also validate. The checker regenerates model and suite twice and compares exact bytes during the
gate. `production-dependency-isolation.log` records the successful assertion that root workspace
Cargo metadata contains no package whose name starts with `ess-`.

`suite.json` is the exact admitted input associated with `report.json` and `run.json`.
`model.json` is the corresponding canonical IR; the ESS model digest is carried in the suite and
report, and is distinct from a checksum of the serialized IR artifact. `coverage.json` records
reviewed scenario IDs and complete contract fingerprints. `observations.json` contains actual
adapter responses; `implementation.txt` identifies the compiled source closure and executable.
No expected value was obtained from that transcript.

The named mutation reports under `../mutations/` retain their own exact intermediate suites and
source identities. This full passing run observes production after all five mutations were
restored. Bounded independent review and correction probes are retained under `../review/`.

The ESS dependency is pinned by the checker manifest and lockfile. `toolchain.json` records that
exact revision and the matching CLI binary checksum. `task-check.log` and
`msrv-checker.log` replace local machine path prefixes with readable placeholders; structured
suite/model/report/run/observation bytes are retained unchanged.

`aep-admission.log` is the exact refused attempt to attach this report and suite with AEP 0.61.1.
The suite/27 compatibility blocker remains open. This passing executable evidence does not
constitute an AEP `conforming` claim or normative authority adoption while that admission is
unavailable.
